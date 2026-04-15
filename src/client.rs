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

    spec_freq: u16,
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
            spec_freq: 0,
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

        let weapon_count = self.simulation.weapon_manager.weapons.len();
        render_state.text_renderer.draw(
            &mut render_state.sprite_renderer,
            &render_state.ui_camera,
            &format!("weapons: {}", weapon_count),
            0,
            0,
            Layer::TopMost,
            TextColor::Yellow,
            TextAlignment::Left,
        );

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

                let name_color = if player.frequency == self.spec_freq {
                    TextColor::Yellow
                } else {
                    TextColor::Blue
                };

                render_state.draw_world_text(
                    &format!("{}({})", player.name, player.bounty),
                    name_x,
                    name_y,
                    Layer::Ships,
                    name_color,
                    TextAlignment::Left,
                );
            }

            for weapon in &self.simulation.weapon_manager.weapons {
                let x_pixels = weapon.position.x.0 / 1000;
                let y_pixels = weapon.position.y.0 / 1000;

                match weapon.kind {
                    WeaponKind::Bullet(bullet) => {
                        if let Some(renderables) = sprites.get_set(GameSpriteKind::Bullets) {
                            let animation_index = self.get_animation_index(4, 20);
                            let renderable_index = (bullet.level * 4) as usize + animation_index;
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
                        let animation_index = self.get_animation_index(4, 20);
                        let renderable_index =
                            (bouncing.level * 4) as usize + 5 * 4 + animation_index;
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
                        let animation_index = self.get_animation_index(10, 100);

                        if bomb.mine {
                            let renderable_index = {
                                if bomb.emp {
                                    (bomb.level * 10) as usize + 40
                                } else {
                                    (bomb.level * 10) as usize
                                }
                            } + animation_index;

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
                            } + animation_index;

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
                    WeaponKind::Thor(_) => {
                        let animation_index = self.get_animation_index(10, 100);
                        let renderable_index = 120 + animation_index;
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
                        let animation_index = self.get_animation_index(10, 60);
                        let renderable_index = (shrapnel.level as usize * 10)
                            + (shrapnel.bouncing as usize) * 30
                            + animation_index;
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
                        let ticks_per_frame = 60 / 10;
                        let ticks = (self.connection.get_game_tick() - weapon.spawn_timestamp)
                            .value() as usize;
                        let animation_index = (ticks / ticks_per_frame) % 10;

                        let renderable_index = animation_index;
                        if let Some(renderables) = sprites.get_set(GameSpriteKind::Repel) {
                            let renderable = &renderables.renderables[renderable_index];
                            render_state.sprite_renderer.draw_centered(
                                &render_state.camera,
                                renderable,
                                x_pixels,
                                y_pixels,
                                Layer::Explosions,
                            );
                        }
                    }
                    WeaponKind::Burst(burst) => {
                        let animation_index = self.get_animation_index(4, 20);
                        let renderable_index =
                            (4 * 4) + (burst.active as usize) * (5 * 4) + animation_index;
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
                    WeaponKind::Decoy(decoy) => {
                        if let Some(player) =
                            self.simulation.player_manager.get_by_id(weapon.player_id)
                        {
                            let orientation = ((decoy.initial_rotation + 40)
                                - (((player.direction + 40) - decoy.initial_rotation) % 40))
                                % 40;

                            let ship_kind_index = player.ship_kind.network_value() as usize * 40;
                            let ship_index = ship_kind_index + orientation as usize;

                            if let Some(renderables) = sprites.get_set(GameSpriteKind::Ships) {
                                let renderable = &renderables.renderables[ship_index];

                                render_state.sprite_renderer.draw_centered(
                                    &render_state.camera,
                                    renderable,
                                    x_pixels,
                                    y_pixels,
                                    Layer::Ships,
                                );

                                let name_x = x_pixels + (renderable.size[0] as i32) / 2;
                                let name_y = y_pixels + (renderable.size[1] as i32) / 2;

                                let name_color = if player.frequency == self.spec_freq {
                                    TextColor::Yellow
                                } else {
                                    TextColor::Blue
                                };

                                render_state.draw_world_text(
                                    &format!("{}({})", player.name, player.bounty),
                                    name_x,
                                    name_y,
                                    Layer::Ships,
                                    name_color,
                                    TextAlignment::Left,
                                );
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn get_animation_index(&self, frames: usize, duration: usize) -> usize {
        let ticks_per_frame = duration / frames;
        let ticks = self.connection.get_game_tick().value() as usize;

        (ticks / ticks_per_frame) % frames
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
            self.connection.tick();
            self.simulation.tick(&self.map, &self.settings);

            match self.connection.state {
                ConnectionState::Playing => {
                    if self
                        .connection
                        .get_game_tick()
                        .diff(&self.last_position_tick)
                        > 100
                    {
                        let (x_position, y_position) = match &render_state {
                            Some(render_state) => {
                                let x_position = render_state.camera.position.x as i32 * 16;
                                let y_position = render_state.camera.position.y as i32 * 16;

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

                        // Make sure our player data will be considered synchronized by the simulation.
                        if let Some(player) = self
                            .simulation
                            .player_manager
                            .get_by_id_mut(self.connection.player_id)
                        {
                            player.last_position_timestamp = self.last_position_tick;
                        }
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

                let sync_request = ClockSyncRequestMessage::new(GameTick::now(0), 2, 2);
                self.connection.send(&sync_request)?;
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
            GameServerMessage::PlayerId(_) => {
                // We need to initialize the simulation here before we receive player enter events.
                self.simulation = Simulation::new(self.connection.get_game_tick());
                self.last_position_tick = self.connection.get_game_tick();
            }
            GameServerMessage::ArenaSettings(settings_message) => {
                log::debug!("Received arena settings");
                self.settings = settings_message.clone();
            }
            GameServerMessage::SynchronizationRequest(sync) => {
                if sync.checksum_key != 0 && self.map.checksum != 0 {
                    // Send security packet
                    log::debug!("Game sync requested");

                    let settings_checksum =
                        checksum::settings_checksum(sync.checksum_key, &self.settings);
                    let exe_checksum = checksum::vie_checksum(sync.checksum_key);
                    let level_checksum = checksum::checksum_map(&self.map, sync.checksum_key);

                    let ping_average = self.connection.sync_history.get_average_ping();
                    let ping_low = self.connection.sync_history.get_low_ping();
                    let ping = self.connection.ping;
                    let ping_high = self.connection.sync_history.get_high_ping();

                    let response = SecurityMessage::new(
                        self.connection.weapons_recv,
                        settings_checksum,
                        exe_checksum,
                        level_checksum,
                        ping as u16 / 10,
                        ping_average as u16 / 10,
                        ping_low as u16 / 10,
                        ping_high as u16 / 10,
                    );
                    log::debug!("Sending game sync packet");
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

                    log::debug!("{} entered arena {:?}", entry.name, entry.ship_kind);

                    if !sent_spectate_request && entry.ship_kind != ShipKind::Spectator {
                        let spectate_request = SpectateMessage {
                            player_id: entry.player_id,
                        };

                        self.connection.send_reliable(&spectate_request)?;
                        self.spec_freq = player.frequency;

                        log::debug!("Spectating target {}", entry.name);
                        sent_spectate_request = true;
                    }

                    // If there was someone already in this place, say that they left.
                    // This can happen when joining at the same exact time as other players.
                    if let Some(old_player) = self.simulation.player_manager.add_player(player) {
                        log::debug!("{} left arena", old_player.name);
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

                        log::trace!(
                            "[SmallPosition] {} at {:?} {:?}",
                            player.name,
                            player.position,
                            player.velocity
                        );
                    } else {
                        Self::validate_packet_timestamp(
                            self.connection.get_game_tick(),
                            message_timestamp,
                            "small",
                        );
                    }
                } else {
                    log::warn!(
                        "got small position packet from bad player id {}",
                        message.player_id.value
                    );
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

                        log::trace!(
                            "[LargePosition] {} at {:?} {}",
                            player.name,
                            player.position,
                            message.weapon
                        );
                    } else {
                        if Self::validate_packet_timestamp(
                            self.connection.get_game_tick(),
                            message_timestamp,
                            "large",
                        ) {
                            return Ok(());
                        }
                    }

                    let weapon_kind =
                        WeaponKind::new(message.weapon, position, velocity, player, &self.settings);

                    if let Some(weapon_kind) = weapon_kind {
                        let spawn_count = self.simulation.weapon_manager.spawn_weapons(
                            player,
                            velocity,
                            weapon_kind,
                            &self.settings,
                            message_timestamp,
                            self.connection.get_game_tick(),
                        );

                        log::trace!("Spawn count for {}: {}", player.name, spawn_count);
                    } else {
                        log::warn!("Failed to create WeaponKind from {}", message.weapon);
                    }
                } else {
                    log::warn!(
                        "got large position packet from bad player id {}",
                        message.player_id.value
                    );
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

                            log::trace!(
                                "[BatchedSmall] {} at {:?} {:?}",
                                player.name,
                                player.position,
                                player.velocity
                            );
                        } else {
                            Self::validate_packet_timestamp(
                                self.connection.get_game_tick(),
                                message_timestamp,
                                "small batched",
                            );
                        }
                    } else {
                        log::warn!(
                            "got small batched position packet from bad player id {}",
                            message.player_id.value
                        );
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

                            log::trace!(
                                "[BatchedLarge] {} at {:?} {:?}",
                                player.name,
                                player.position,
                                player.velocity
                            );
                        } else {
                            Self::validate_packet_timestamp(
                                self.connection.get_game_tick(),
                                message_timestamp,
                                "large batched",
                            );
                        }
                    } else {
                        log::warn!(
                            "got large batched position packet from bad player id {}",
                            message.player_id.value
                        );
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

                if let Some(killed) = self
                    .simulation
                    .player_manager
                    .get_by_id_mut(message.killed_id)
                {
                    killed.enter_delay = self.settings.enter_delay as u16;
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

    fn validate_packet_timestamp(current_tick: GameTick, timestamp: GameTick, ctx: &str) -> bool {
        if current_tick.diff(&timestamp) > 100 {
            log::warn!(
                "Received {} packet timestamp that was far out of range of normal Recv: {} Now: {}",
                ctx,
                timestamp.value(),
                current_tick.value()
            );

            true
        } else {
            false
        }
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
