use crate::arena_settings::ArenaSettings;
use crate::checksum;
use crate::clock::*;
use crate::map::Map;
use crate::math::PixelUnit;
use crate::math::PositionUnit;
use crate::math::{Position, Velocity};
use crate::net::connection::ConnectionError;
use crate::net::connection::SocketKind;
use crate::net::connection::{Connection, ConnectionState};
use crate::net::packet::bi::*;
use crate::net::packet::c2s::*;
use crate::net::packet::s2c::*;
use crate::player::*;
use crate::render::game_sprites::GameSpriteKind;
use crate::render::game_sprites::GameSprites;
use crate::render::layer::Layer;
use crate::render::render_state::RenderState;
use crate::render::text_renderer::TextAlignment;
use crate::render::text_renderer::TextColor;
use crate::ship::ShipKind;
use crate::simulation::game_simulation::Simulation;
use crate::simulation::player_simulation::update_player_lerp_target;
use crate::weapon::WeaponKind;

use miniz_oxide::inflate::decompress_to_vec_zlib;

#[cfg(not(target_arch = "wasm32"))]
fn build_zone_directory(zone: &str) -> Result<(), std::io::Error> {
    std::fs::DirBuilder::new()
        .recursive(true)
        .create(format!("zones/{}", zone))?;
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
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
        socket: SocketKind,
        registration: RegistrationFormMessage,
    ) -> Result<Client, ConnectionError> {
        let connection = Connection::new(socket)?;

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

    pub fn render(&mut self, render_state: &mut RenderState, sprites: &GameSprites) {
        // TODO: This is all test code.
        // TODO: It should be spawning animations that are updated every tick, not tied to render calls.

        for player in &self.simulation.player_manager.players {
            if player.ship_kind != ShipKind::Spectator {
                render_state.camera.position = player.position.into();
                break;
            }
        }

        for player in &self.simulation.player_manager.players {
            if player.ship_kind == ShipKind::Spectator {
                continue;
            }

            let x_pixels = player.position.x.0 / 1000;
            let y_pixels = player.position.y.0 / 1000;

            if let Some(ship_renderables) = sprites.get_set(GameSpriteKind::Ships) {
                let ship_kind_index = player.ship_kind.network_value() as usize * 40;
                let ship_index = ship_kind_index + player.direction as usize;

                let renderable = &ship_renderables.renderables[ship_index];

                render_state.sprite_renderer.draw_centered(
                    &render_state.camera,
                    renderable,
                    x_pixels,
                    y_pixels,
                    Layer::Ships,
                );

                let name_x = x_pixels + (renderable.size[0] as i32) / 2;
                let name_y = y_pixels + (renderable.size[1] as i32) / 2;

                render_state.draw_world_text(
                    &format!("{}({})", player.name, player.bounty),
                    name_x,
                    name_y,
                    Layer::Ships,
                    TextColor::Yellow,
                    TextAlignment::Left,
                );
            }

            for weapon in &self.simulation.weapon_manager.weapons {
                let x_pixels = weapon.position.x.0 / 1000;
                let y_pixels = weapon.position.y.0 / 1000;

                match weapon.kind {
                    WeaponKind::Bullet(bullet) => {
                        if let Some(renderables) = sprites.get_set(GameSpriteKind::Bullets) {
                            let renderable_index = (bullet.level * 4) as usize;
                            let renderable = &renderables.renderables[renderable_index];
                            render_state.sprite_renderer.draw_centered(
                                &render_state.camera,
                                renderable,
                                x_pixels,
                                y_pixels,
                                Layer::Weapons,
                            );
                        }
                    }
                    WeaponKind::BouncingBullet(bouncing) => {
                        let renderable_index = (bouncing.level * 4) as usize + 5 * 4;
                        if let Some(renderables) = sprites.get_set(GameSpriteKind::Bullets) {
                            let renderable = &renderables.renderables[renderable_index];
                            render_state.sprite_renderer.draw_centered(
                                &render_state.camera,
                                renderable,
                                x_pixels,
                                y_pixels,
                                Layer::Weapons,
                            );
                        }
                    }
                    WeaponKind::Bomb(bomb) | WeaponKind::ProximityBomb(bomb) => {
                        if bomb.mine {
                            let renderable_index = {
                                if bomb.emp {
                                    (bomb.level * 10) as usize + 40
                                } else {
                                    (bomb.level * 10) as usize
                                }
                            };

                            if let Some(renderables) = sprites.get_set(GameSpriteKind::Mines) {
                                let renderable = &renderables.renderables[renderable_index];
                                render_state.sprite_renderer.draw_centered(
                                    &render_state.camera,
                                    renderable,
                                    x_pixels,
                                    y_pixels,
                                    Layer::Weapons,
                                );
                            }
                        } else {
                            let renderable_index = {
                                if bomb.emp {
                                    (bomb.level * 10) as usize + 40
                                } else {
                                    if bomb.remaining_bounces > 0 {
                                        (bomb.level * 10) as usize + 80
                                    } else {
                                        (bomb.level * 10) as usize
                                    }
                                }
                            };

                            if let Some(renderables) = sprites.get_set(GameSpriteKind::Bombs) {
                                let renderable = &renderables.renderables[renderable_index];
                                render_state.sprite_renderer.draw_centered(
                                    &render_state.camera,
                                    renderable,
                                    x_pixels,
                                    y_pixels,
                                    Layer::Weapons,
                                );
                            }
                        }
                    }
                    WeaponKind::Thor => {
                        let renderable_index = 120;
                        if let Some(renderables) = sprites.get_set(GameSpriteKind::Bombs) {
                            let renderable = &renderables.renderables[renderable_index];
                            render_state.sprite_renderer.draw_centered(
                                &render_state.camera,
                                renderable,
                                x_pixels,
                                y_pixels,
                                Layer::Weapons,
                            );
                        }
                    }
                    WeaponKind::Shrapnel(shrapnel) => {
                        let renderable_index =
                            (shrapnel.level as usize * 10) + (shrapnel.bouncing as usize) * 30;
                        if let Some(renderables) = sprites.get_set(GameSpriteKind::Shrapnel) {
                            let renderable = &renderables.renderables[renderable_index];
                            render_state.sprite_renderer.draw_centered(
                                &render_state.camera,
                                renderable,
                                x_pixels,
                                y_pixels,
                                Layer::Weapons,
                            );
                        }
                    }
                    WeaponKind::Repel => {
                        let renderable_index = 7;
                        if let Some(renderables) = sprites.get_set(GameSpriteKind::Repel) {
                            let renderable = &renderables.renderables[renderable_index];
                            render_state.sprite_renderer.draw_centered(
                                &render_state.camera,
                                renderable,
                                x_pixels,
                                y_pixels,
                                Layer::Weapons,
                            );
                        }
                    }
                    WeaponKind::Burst(burst) => {
                        let renderable_index = (4 * 4) + 2 + (burst.active as usize) * (4 * 4);
                        if let Some(renderables) = sprites.get_set(GameSpriteKind::Bullets) {
                            let renderable = &renderables.renderables[renderable_index];
                            render_state.sprite_renderer.draw_centered(
                                &render_state.camera,
                                renderable,
                                x_pixels,
                                y_pixels,
                                Layer::Weapons,
                            );
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    pub fn update(
        &mut self,
        render_state: Option<&mut RenderState>,
    ) -> Result<(), ConnectionError> {
        let mut render_state = render_state;

        loop {
            let message = self.connection.receive_message();
            if let Err(e) = message {
                log::error!("Error: {}", e);

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
                self.process_message(&mut render_state, message)?;
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

            match self.connection.state {
                ConnectionState::Playing => {
                    if self
                        .connection
                        .get_game_tick()
                        .diff(&self.last_position_tick)
                        > 300
                    {
                        let (x_position, y_position) = match &render_state {
                            Some(render_state) => {
                                let x_position = render_state.camera.position.x as i32 * 16;
                                let y_position = render_state.camera.position.x as i32 * 16;

                                (x_position, y_position)
                            }
                            None => (0, 0),
                        };

                        let position = PositionMessage {
                            direction: 0,
                            timestamp: self.connection.get_game_tick(),
                            x_position: x_position as u16,
                            y_position: y_position as u16,
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
                    break;
                }
                _ => {}
            }
        }

        self.local_tick = self.local_tick + tick_count;

        Ok(())
    }

    fn process_core_message(
        &mut self,
        _render_state: &mut Option<&mut RenderState>,
        message: &CoreServerMessage,
    ) -> Result<(), ConnectionError> {
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
                log::debug!("Got sync response");
            }
            _ => {}
        }

        Ok(())
    }

    fn process_game_message(
        &mut self,
        render_state: &mut Option<&mut RenderState>,
        message: &GameServerMessage,
    ) -> Result<(), ConnectionError> {
        match message {
            GameServerMessage::Chat(chat) => match chat.kind {
                ChatKind::Public | ChatKind::PublicMacro => {
                    if let Some(sender) = self.simulation.player_manager.get_by_id(chat.sender) {
                        log::debug!("{}> {}", sender.name, chat.message);
                    }
                }
                ChatKind::Team => {
                    if let Some(sender) = self.simulation.player_manager.get_by_id(chat.sender) {
                        log::debug!("T {}> {}", sender.name, chat.message);
                    }
                }
                ChatKind::Frequency => {
                    if let Some(sender) = self.simulation.player_manager.get_by_id(chat.sender) {
                        log::debug!("F {}> {}", sender.name, chat.message);
                    }
                }
                ChatKind::Arena | ChatKind::Error | ChatKind::Warning => {
                    if !chat.message.is_empty() {
                        log::debug!("A {}", chat.message);
                    }
                }
                ChatKind::Private => {
                    if let Some(sender) = self.simulation.player_manager.get_by_id(chat.sender) {
                        log::debug!("P {}> {}", sender.name, chat.message);
                    }
                }
                ChatKind::RemotePrivate => {
                    log::debug!("RP {}", chat.message);
                }
                ChatKind::Channel => {
                    log::debug!("C {}", chat.message);
                }
            },
            GameServerMessage::PasswordResponse(password_response) => {
                log::debug!("Got password response: {}", password_response.response);

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

                            log::debug!("Sending registration");

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
                        log::debug!("Failed to login: {:?}", password_response.response);
                        self.connection.state = ConnectionState::Disconnected;
                    }
                }
            }
            GameServerMessage::ArenaSettings(settings_message) => {
                log::debug!("Received arena settings");
                // println!("{:?}", settings);
                self.settings = settings_message.clone();
            }
            GameServerMessage::SynchronizationRequest(sync) => {
                if sync.checksum_key != 0 && self.map.checksum != 0 {
                    // Send security packet
                    log::debug!("Sync requested");

                    let settings_checksum =
                        checksum::settings_checksum(sync.checksum_key, &self.settings);
                    let exe_checksum = checksum::vie_checksum(sync.checksum_key);
                    let level_checksum = checksum::checksum_map(&self.map, sync.checksum_key);

                    let response =
                        SecurityMessage::new(0, settings_checksum, exe_checksum, level_checksum);
                    log::debug!("Sending security packet");
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
                        log::debug!("{} left arena", old_player.name);
                    }

                    log::debug!("{} entered arena {:?}", entry.name, entry.ship_kind);

                    if !sent_spectate_request && entry.ship_kind != ShipKind::Spectator {
                        let spectate_request = SpectateMessage {
                            player_id: entry.player_id,
                        };

                        self.connection.send_reliable(&spectate_request)?;
                        log::debug!("Spectating target {}", entry.name);
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
                    log::debug!("{} left arena", player.name);
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

                        log::debug!(
                            "[SmallPosition] {} at {:?} {:?}",
                            player.name,
                            player.position,
                            player.velocity
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

                    let position = Position::new(
                        PixelUnit(message.x as i32).into(),
                        PixelUnit(message.y as i32).into(),
                    );

                    let velocity = Velocity::new(
                        PositionUnit(message.x_velocity as i32),
                        PositionUnit(message.y_velocity as i32),
                    );

                    if player.last_position_timestamp < message_timestamp {
                        player.velocity = velocity;

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

                        log::debug!(
                            "[LargePosition] {} at {:?} {}",
                            player.name,
                            player.position,
                            message.weapon
                        );
                    }

                    let weapon_kind =
                        WeaponKind::new(message.weapon, position, velocity, player, &self.settings);

                    if let Some(weapon_kind) = weapon_kind {
                        self.simulation.weapon_manager.spawn_weapons(
                            player,
                            velocity,
                            weapon_kind,
                            &self.settings,
                            message_timestamp,
                            self.connection.get_game_tick(),
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

                            log::debug!(
                                "[BatchedSmall] {} at {:?} {:?}",
                                player.name,
                                player.position,
                                player.velocity
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

                            log::debug!(
                                "[BatchedLarge] {} at {:?} {:?}",
                                player.name,
                                player.position,
                                player.velocity
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
                        log::debug!("{} killed by {}", killed.name, killer.name);
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
                log::debug!("Map name: {}", info.filename);

                self.connection.state = ConnectionState::MapDownload;

                let chat = SendChatMessage::public("?arena");
                self.connection.send_reliable(&chat)?;

                #[cfg(not(target_arch = "wasm32"))]
                {
                    let map_path = get_zone_path(&self.zone, &info.filename);
                    let map_data = std::fs::read(map_path);

                    if let Ok(map_data) = map_data {
                        let checksum = checksum::crc32(&map_data);

                        if checksum == info.checksum {
                            if let Ok(new_map) = Map::new(&info.filename, &map_data) {
                                if let Some(render_state) = render_state {
                                    render_state.on_map_change(&new_map, &map_data);
                                }

                                self.handle_map_load(new_map, info.checksum);
                            } else {
                                log::debug!("Map read error: failed to load tiles");
                                self.connection.state = ConnectionState::Disconnected;
                            }
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
                            #[cfg(not(target_arch = "wasm32"))]
                            {
                                let map_path = get_zone_path(&self.zone, &compressed.filename);

                                if let Err(e) = build_zone_directory(&self.zone) {
                                    log::debug!("Error creating zone directory: {}", e);
                                }

                                if let Err(e) = std::fs::write(map_path, inflated.as_slice()) {
                                    log::debug!("Error writing map: {}", e);
                                }
                            }

                            if let Ok(new_map) = Map::new(&self.map.filename, &inflated) {
                                if let Some(render_state) = render_state {
                                    render_state.on_map_change(&new_map, &inflated);
                                }

                                self.handle_map_load(new_map, checksum::crc32(&inflated));
                            } else {
                                log::debug!("Map read error: failed to load tiles");
                            }
                        }
                        Err(e) => {
                            log::debug!("Error: {}", e);
                        }
                    }
                }
            }
            GameServerMessage::ArenaDirectory(directory) => {
                log::debug!("directory: {:?}", directory);
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

    fn process_message(
        &mut self,
        render_state: &mut Option<&mut RenderState>,
        message: ServerMessage,
    ) -> Result<(), ConnectionError> {
        match message {
            ServerMessage::Core(core_message) => {
                self.process_core_message(render_state, &core_message)
            }
            ServerMessage::Game(game_message) => {
                self.process_game_message(render_state, &game_message)
            }
        }
    }
}
