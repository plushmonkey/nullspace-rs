use crate::arena_settings::ArenaSettings;
use crate::checksum;
use crate::clock::*;
use crate::map::Map;
use crate::math::PixelUnit;
use crate::math::PositionUnit;
use crate::math::{Position, Velocity};
use crate::net::connection::ConnectionError;
use crate::net::connection::{Connection, ConnectionState};
use crate::net::packet::bi::*;
use crate::net::packet::c2s::*;
use crate::net::packet::s2c::*;
use crate::player::*;
use crate::ship::ShipKind;
use crate::simulation::game_simulation::Simulation;
use crate::simulation::player_simulation::update_player_lerp_target;

use miniz_oxide::inflate::decompress_to_vec_zlib;
use std::fs::{self, DirBuilder};

fn build_zone_directory(zone: &str) -> Result<(), std::io::Error> {
    DirBuilder::new()
        .recursive(true)
        .create(format!("zones/{}", zone))?;
    Ok(())
}

fn get_zone_path(zone: &str, filename: &str) -> String {
    format!("zones/{}/{}", zone, filename)
}

pub struct Client {
    pub connection: Connection,
    pub map: Map,
    pub settings: Box<ArenaSettings>,
    pub last_position_tick: GameTick,

    pub username: String,
    pub password: String,
    pub zone: String,

    pub registration: RegistrationFormMessage,

    pub simulation: Simulation,

    // This is the local tick for the last processed tick.
    local_tick: GameTick,
}

impl Client {
    pub fn new(
        username: &str,
        password: &str,
        zone: &str,
        remote_ip: &str,
        remote_port: u16,
        registration: RegistrationFormMessage,
    ) -> Result<Client, ConnectionError> {
        let connection = Connection::new(remote_ip, remote_port)?;

        Ok(Client {
            connection,
            map: Map::empty(""),
            settings: Box::new(ArenaSettings::default()),
            last_position_tick: GameTick::now(0),
            username: username.to_owned(),
            password: password.to_owned(),
            zone: zone.to_owned(),
            registration,
            simulation: Simulation::new(GameTick::now(0)),
            local_tick: GameTick::now(0),
        })
    }

    pub fn run(&mut self, rx: std::sync::mpsc::Receiver<()>) -> Result<(), ConnectionError> {
        'main_loop: loop {
            // Exit loop if we receive a control-c signal.
            if let Ok(_) = rx.try_recv() {
                break 'main_loop;
            }

            loop {
                let message = self.connection.receive_message();
                if let Err(e) = message {
                    println!("Error: {}", e);

                    match e {
                        ConnectionError::IoError(_) => {
                            break;
                        }
                        _ => {}
                    }

                    continue;
                }

                let message = message.unwrap();

                if let Some(message) = message {
                    self.process_message(message)?;
                } else {
                    // We are done processing everything now.
                    break;
                }
            }

            let local_now = GameTick::now(0);
            let tick_count = local_now.diff(&self.local_tick);

            for _ in 0..tick_count {
                self.connection.current_tick = self.connection.current_tick + 1;

                self.simulation.tick(&self.map, &self.settings);

                if self.connection.current_tick.value() % 100 == 0 {
                    if let Some(player) = self.simulation.player_manager.get_by_name("monkey") {
                        println!(
                            "L {} at {:?} {:?}",
                            player.name, player.position, player.velocity
                        );
                    }
                }

                match self.connection.state {
                    ConnectionState::Playing => {
                        if self
                            .connection
                            .get_game_tick()
                            .diff(&self.last_position_tick)
                            > 300
                        {
                            let position = PositionMessage {
                                direction: 0,
                                timestamp: self.connection.get_game_tick(),
                                x_position: 0,
                                y_position: 0,
                                x_velocity: 0,
                                y_velocity: 0,
                                togglables: 0,
                                bounty: 0,
                                energy: 0,
                                weapon_info: 0,
                            };

                            self.connection.send(&position)?;

                            self.last_position_tick = self.connection.get_game_tick();
                        }
                    }
                    ConnectionState::Disconnected => {
                        break 'main_loop;
                    }
                    _ => {}
                }
            }

            self.local_tick = self.local_tick + tick_count;

            std::thread::sleep(std::time::Duration::from_millis(1));
        }

        // Always send disconnect when we are exiting so we don't linger on the server.
        let disconnect = DisconnectMessage {};
        self.connection.send(&disconnect)?;

        Ok(())
    }

    fn process_core_message(&mut self, message: &CoreServerMessage) -> Result<(), ConnectionError> {
        match message {
            CoreServerMessage::EncryptionResponse(_) => {
                let password = PasswordMessage::new(
                    &self.username,
                    &self.password,
                    false,
                    0x1231241,
                    240,
                    0x86,
                    123412,
                );

                self.connection.send_reliable(&password)?;

                let sync_request = SyncRequestMessage::new(GameTick::now(0), 2, 2);
                self.connection.send(&sync_request)?;
            }
            CoreServerMessage::SyncResponse(_) => {
                println!("Got sync response");
            }
            _ => {}
        }

        Ok(())
    }

    fn process_game_message(&mut self, message: &GameServerMessage) -> Result<(), ConnectionError> {
        match message {
            GameServerMessage::Chat(chat) => match chat.kind {
                ChatKind::Public | ChatKind::PublicMacro => {
                    if let Some(sender) = self.simulation.player_manager.get_by_id(chat.sender) {
                        println!("{}> {}", sender.name, chat.message);
                    }
                }
                ChatKind::Team => {
                    if let Some(sender) = self.simulation.player_manager.get_by_id(chat.sender) {
                        println!("T {}> {}", sender.name, chat.message);
                    }
                }
                ChatKind::Frequency => {
                    if let Some(sender) = self.simulation.player_manager.get_by_id(chat.sender) {
                        println!("F {}> {}", sender.name, chat.message);
                    }
                }
                ChatKind::Arena | ChatKind::Error | ChatKind::Warning => {
                    if !chat.message.is_empty() {
                        println!("A {}", chat.message);
                    }
                }
                ChatKind::Private => {
                    if let Some(sender) = self.simulation.player_manager.get_by_id(chat.sender) {
                        println!("P {}> {}", sender.name, chat.message);
                    }
                }
                ChatKind::RemotePrivate => {
                    println!("RP {}", chat.message);
                }
                ChatKind::Channel => {
                    println!("C {}", chat.message);
                }
            },
            GameServerMessage::PasswordResponse(password_response) => {
                println!("Got password response: {}", password_response.response);

                match &password_response.response {
                    LoginResponse::Ok => {
                        let arena_request = ArenaJoinMessage::new(
                            ShipKind::Spectator,
                            1920,
                            1080,
                            ArenaRequest::AnyPublic,
                        );
                        self.connection.send_reliable(&arena_request)?;
                    }
                    LoginResponse::Unregistered => {
                        if password_response.registration_request {
                            let mut registration_packet = vec![0; 766].into_boxed_slice();

                            println!("Sending registration");

                            self.registration.serialize(&mut registration_packet);
                            self.connection.send_reliable_data(&registration_packet)?;
                        } else {
                            let password = PasswordMessage::new(
                                &self.username,
                                &self.password,
                                true,
                                0x1231241,
                                240,
                                0x86,
                                123412,
                            );

                            self.connection.send_reliable(&password)?;
                        }
                    }
                    _ => {
                        println!("Failed to login: {:?}", password_response.response);
                        self.connection.state = ConnectionState::Disconnected;
                    }
                }
            }
            GameServerMessage::ArenaSettings(settings_message) => {
                println!("Received arena settings");
                // println!("{:?}", settings);
                self.settings = settings_message.clone();
            }
            GameServerMessage::SynchronizationRequest(sync) => {
                if sync.checksum_key != 0 && self.map.checksum != 0 {
                    // Send security packet
                    println!("Sync requested");

                    let settings_checksum =
                        checksum::settings_checksum(sync.checksum_key, &self.settings);
                    let exe_checksum = checksum::vie_checksum(sync.checksum_key);
                    let level_checksum = checksum::checksum_map(&self.map, sync.checksum_key);

                    let response =
                        SecurityMessage::new(0, settings_checksum, exe_checksum, level_checksum);
                    println!("Sending security packet");
                    self.connection.send_reliable(&response)?;
                }
            }
            GameServerMessage::PlayerEntering(entering) => {
                // TODO: Remove. Just here for testing so we get position packets from anywhere.
                let mut sent_spectate_request = false;

                for entry in &entering.players {
                    let mut player = Player::new(
                        entry.player_id,
                        &entry.name,
                        &entry.squad,
                        entry.ship_kind,
                        entry.frequency,
                    );

                    player.flag_count = entry.flag_count;
                    player.attach_parent = entry.attach_parent;
                    player.last_position_timestamp = self.connection.get_game_tick();

                    // If there was someone already in this place, say that they left.
                    // This can happen when joining at the same exact time as other players.
                    if let Some(old_player) = self.simulation.player_manager.add_player(player) {
                        println!("{} left arena", old_player.name);
                    }

                    println!("{} entered arena {:?}", entry.name, entry.ship_kind);

                    if !sent_spectate_request && entry.ship_kind != ShipKind::Spectator {
                        let spectate_request = SpectateMessage {
                            player_id: entry.player_id,
                        };

                        self.connection.send_reliable(&spectate_request)?;
                        println!("Spectating target {}", entry.name);
                        sent_spectate_request = true;
                    }
                }
            }
            GameServerMessage::PlayerLeaving(leaving) => {
                if let Some(player) = self
                    .simulation
                    .player_manager
                    .remove_player(leaving.player_id)
                {
                    println!("{} left arena", player.name);
                }
            }

            GameServerMessage::SmallPosition(message) => {
                if let Some(player) = self
                    .simulation
                    .player_manager
                    .get_by_id_mut(message.player_id)
                {
                    let message_timestamp =
                        GameTick::from_mini(self.connection.get_game_tick(), message.timestamp)
                            - message.ping as i32;

                    if player.last_position_timestamp < message_timestamp {
                        let position = Position::new(
                            PixelUnit(message.x as i32).into(),
                            PixelUnit(message.y as i32).into(),
                        );

                        player.velocity = Velocity::new(
                            PositionUnit(message.x_velocity as i32),
                            PositionUnit(message.y_velocity as i32),
                        );

                        let sim_ticks = self.connection.current_tick.diff(&message_timestamp);
                        update_player_lerp_target(
                            player,
                            position,
                            &self.map,
                            &self.settings,
                            sim_ticks,
                        );

                        player.direction = message.direction;
                        player.bounty = message.bounty as u16;
                        player.status = message.status;
                        player.ping = message.ping;
                        player.last_position_timestamp = message_timestamp;

                        println!(
                            "[SmallPosition] {} at {:?} {:?}",
                            player.name, player.position, player.velocity
                        );
                    }
                }
            }
            GameServerMessage::LargePosition(message) => {
                if let Some(player) = self
                    .simulation
                    .player_manager
                    .get_by_id_mut(message.player_id)
                {
                    let message_timestamp =
                        GameTick::from_mini(self.connection.get_game_tick(), message.timestamp)
                            - message.ping as i32;

                    if player.last_position_timestamp < message_timestamp {
                        let position = Position::new(
                            PixelUnit(message.x as i32).into(),
                            PixelUnit(message.y as i32).into(),
                        );

                        player.velocity = Velocity::new(
                            PositionUnit(message.x_velocity as i32),
                            PositionUnit(message.y_velocity as i32),
                        );

                        let sim_ticks = self.connection.current_tick.diff(&message_timestamp);
                        update_player_lerp_target(
                            player,
                            position,
                            &self.map,
                            &self.settings,
                            sim_ticks,
                        );

                        player.direction = message.direction;
                        player.bounty = message.bounty;
                        player.status = message.status;
                        player.ping = message.ping;
                        player.last_position_timestamp = message_timestamp;

                        println!(
                            "[LargePosition] {} at {:?} {}",
                            player.name, player.position, message.weapon
                        );
                    }
                }
            }
            GameServerMessage::BatchedSmallPosition(message) => {
                for message in &message.positions {
                    if let Some(player) = self
                        .simulation
                        .player_manager
                        .get_by_id_mut(message.player_id)
                    {
                        let message_timestamp = GameTick::from_batched(
                            self.connection.get_game_tick(),
                            message.timestamp,
                        );

                        if player.last_position_timestamp < message_timestamp {
                            let position = Position::new(
                                PixelUnit(message.x as i32).into(),
                                PixelUnit(message.y as i32).into(),
                            );

                            player.velocity = Velocity::new(
                                PositionUnit(message.x_velocity as i32),
                                PositionUnit(message.y_velocity as i32),
                            );

                            let sim_ticks = self.connection.current_tick.diff(&message_timestamp);
                            update_player_lerp_target(
                                player,
                                position,
                                &self.map,
                                &self.settings,
                                sim_ticks,
                            );

                            player.direction = message.direction;
                            player.last_position_timestamp = message_timestamp;

                            println!(
                                "[BatchedSmall] {} at {:?} {:?}",
                                player.name, player.position, player.velocity
                            );
                        }
                    }
                }
            }
            GameServerMessage::BatchedLargePosition(message) => {
                for message in &message.positions {
                    if let Some(player) = self
                        .simulation
                        .player_manager
                        .get_by_id_mut(message.player_id)
                    {
                        let message_timestamp = GameTick::from_batched(
                            self.connection.get_game_tick(),
                            message.timestamp,
                        );

                        if player.last_position_timestamp < message_timestamp {
                            let position = Position::new(
                                PixelUnit(message.x as i32).into(),
                                PixelUnit(message.y as i32).into(),
                            );

                            player.velocity = Velocity::new(
                                PositionUnit(message.x_velocity as i32),
                                PositionUnit(message.y_velocity as i32),
                            );

                            let sim_ticks = self.connection.current_tick.diff(&message_timestamp);
                            update_player_lerp_target(
                                player,
                                position,
                                &self.map,
                                &self.settings,
                                sim_ticks,
                            );

                            player.direction = message.direction;
                            player.last_position_timestamp = message_timestamp;
                            if let Some(status) = message.status {
                                player.status = status;
                            }

                            println!(
                                "[BatchedLarge] {} at {:?} {:?}",
                                player.name, player.position, player.velocity
                            );
                        }
                    }
                }
            }
            GameServerMessage::PlayerDeath(message) => {
                if let Some(killer) = self.simulation.player_manager.get_by_id(message.killer_id) {
                    if let Some(killed) =
                        self.simulation.player_manager.get_by_id(message.killed_id)
                    {
                        println!("{} killed by {}", killed.name, killer.name);
                    }
                }

                if let Some(killer) = self
                    .simulation
                    .player_manager
                    .get_by_id_mut(message.killer_id)
                {
                    killer.flag_count += message.flag_transfer;
                }
            }
            GameServerMessage::PlayerFrequencyChange(change) => {
                if let Some(player) = self
                    .simulation
                    .player_manager
                    .get_by_id_mut(change.player_id)
                {
                    player.frequency = change.frequency;
                }
            }
            GameServerMessage::PlayerTeamAndShipChange(change) => {
                if let Some(player) = self
                    .simulation
                    .player_manager
                    .get_by_id_mut(change.player_id)
                {
                    player.ship_kind = change.ship_kind;
                    player.frequency = change.frequency;
                }
            }
            GameServerMessage::MapInformation(info) => {
                println!("Map name: {}", info.filename);

                self.connection.state = ConnectionState::MapDownload;

                let chat = SendChatMessage::public("?arena");
                self.connection.send_reliable(&chat)?;

                let map_path = get_zone_path(&self.zone, &info.filename);
                let map_data = fs::read(map_path);

                if let Ok(map_data) = map_data {
                    let checksum = checksum::crc32(&map_data);

                    if checksum == info.checksum {
                        if let Ok(new_map) = Map::new(&info.filename, &map_data) {
                            self.handle_map_load(new_map, info.checksum);
                        } else {
                            println!("Map read error: failed to load tiles");
                            self.connection.state = ConnectionState::Disconnected;
                        }
                    }
                }

                if matches!(self.connection.state, ConnectionState::MapDownload) {
                    // Request
                    let map_request = MapRequestMessage {};
                    self.connection.send_reliable(&map_request)?;

                    self.connection.state = ConnectionState::MapDownload;

                    self.map = Map::empty(&info.filename);
                    self.map.checksum = info.checksum;
                }
            }
            GameServerMessage::CompressedMap(compressed) => {
                if compressed.filename == self.map.filename {
                    let inflated = decompress_to_vec_zlib(compressed.data.as_slice());

                    match inflated {
                        Ok(inflated) => {
                            let map_path = get_zone_path(&self.zone, &compressed.filename);

                            if let Err(e) = build_zone_directory(&self.zone) {
                                println!("Error creating zone directory: {}", e);
                            }

                            if let Err(e) = fs::write(map_path, inflated.as_slice()) {
                                println!("Error writing map: {}", e);
                            }

                            if let Ok(new_map) = Map::new(&self.map.filename, &inflated) {
                                self.handle_map_load(new_map, checksum::crc32(&inflated));
                            } else {
                                println!("Map read error: failed to load tiles");
                            }
                        }
                        Err(e) => {
                            println!("Error: {}", e);
                        }
                    }
                }
            }
            GameServerMessage::ArenaDirectory(directory) => {
                println!("directory: {:?}", directory);
            }
            _ => {}
        }

        Ok(())
    }

    fn handle_map_load(&mut self, map: Map, checksum: u32) {
        self.map = map;
        self.map.checksum = checksum;
        self.connection.state = ConnectionState::Playing;
    }

    fn process_message(&mut self, message: ServerMessage) -> Result<(), ConnectionError> {
        match message {
            ServerMessage::Core(core_message) => self.process_core_message(&core_message),
            ServerMessage::Game(game_message) => self.process_game_message(&game_message),
        }
    }
}
