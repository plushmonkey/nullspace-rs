use crate::arena_settings::ArenaSettings;
use crate::checksum;
use crate::clock::*;
use crate::map::ANIMATED_TILE_KIND_COUNT;
use crate::map::DoorRng;
use crate::map::Map;
use crate::map::TILE_ID_FIRST_DOOR;
use crate::map::TILE_ID_FLAG;
use crate::math::PixelUnit;
use crate::math::PositionUnit;
use crate::math::Rectangle;
use crate::math::{Position, Velocity};
use crate::net::connection::ConnectionError;
use crate::net::connection::SocketKind;
use crate::net::connection::{Connection, ConnectionState};
use crate::net::packet::bi::*;
use crate::net::packet::c2s::*;
use crate::net::packet::s2c::*;
use crate::player::*;
use crate::powerball::is_team_goal;
use crate::radar::Radar;
use crate::render::game_sprites::GAME_SPRITE_SHEET_DEFINITIONS;
use crate::render::game_sprites::GameSpriteKind;
use crate::render::game_sprites::GameSprites;
use crate::render::layer::Layer;
use crate::render::render_state::RenderState;
use crate::render::text_renderer::TextAlignment;
use crate::render::text_renderer::TextColor;
use crate::ship::ShipKind;
use crate::simulation::game_simulation::Simulation;
use crate::simulation::game_simulation::SimulationEventKind;
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

    radar: Radar,
    // TODO: Remove. This is just for testing until input is handled.
    pub fullscreen_radar: bool,
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
            radar: Radar::new(),
            fullscreen_radar: false,
        })
    }

    pub fn render(&mut self, render_state: &mut RenderState, sprites: &GameSprites) {
        // TODO: This is all test code.
        // TODO: It should be spawning animations that are updated every tick, not tied to render calls.

        self.radar.render(
            render_state,
            &self.map,
            self.settings.map_zoom_factor as u16,
            self.spec_freq,
            self.settings.powerball_mode,
            self.fullscreen_radar,
        );

        for player in &self.simulation.player_manager.players {
            if player.ship_kind != ShipKind::Spectator {
                if let Some(player_position) = player.position {
                    render_state.camera.position = player_position.into();
                    self.spec_freq = player.frequency;
                }
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

            let Some(player_position) = player.position else {
                continue;
            };

            let x_pixels = player_position.x.0 / 1000;
            let y_pixels = player_position.y.0 / 1000;

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
                        let ticks =
                            (weapon.last_update_tick - weapon.spawn_timestamp).value() as usize;
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

        self.render_map_animations(render_state, sprites);
    }

    pub fn render_map_animations(&self, render_state: &mut RenderState, sprites: &GameSprites) {
        const OFFSCREEN_PIXELS: i32 = 8 * 16;
        let (screen_width, screen_height) = (
            render_state.size().width as i32,
            render_state.size().height as i32,
        );
        let half_width = (screen_width / 2) + OFFSCREEN_PIXELS;
        let half_height = (screen_height / 2) + OFFSCREEN_PIXELS;

        let center_x = (render_state.camera.position.x * 16.0f32) as i32;
        let center_y = (render_state.camera.position.y * 16.0f32) as i32;

        let view_min = Position::new(
            PositionUnit(center_x - half_width),
            PositionUnit(center_y - half_height),
        );
        let view_max = Position::new(
            PositionUnit(center_x + half_width),
            PositionUnit(center_y + half_height),
        );

        let view_rect = Rectangle::new(view_min, view_max);
        const ANIMATED_TILE_MAPPING: [(GameSpriteKind, usize); ANIMATED_TILE_KIND_COUNT] = [
            (GameSpriteKind::Goal, 50),
            (GameSpriteKind::AsteroidSmall, 150),
            (GameSpriteKind::AsteroidSmall2, 150),
            (GameSpriteKind::AsteroidLarge, 150),
            (GameSpriteKind::SpaceStation, 100),
            (GameSpriteKind::Wormhole, 250),
            (GameSpriteKind::Flag, 100),
        ];

        // Loop over the animated tiles except for flags. Flags require extra game state to determine how they should be rendered.
        for i in 0..ANIMATED_TILE_KIND_COUNT - 1 {
            let tiles = &self.map.animated_tiles[i];

            if tiles.is_empty() {
                continue;
            }

            let (game_sprite_kind, duration) = ANIMATED_TILE_MAPPING[i];

            let Some(sprite_set) = sprites.get_set(game_sprite_kind) else {
                continue;
            };

            let frames = GAME_SPRITE_SHEET_DEFINITIONS[game_sprite_kind as usize];
            let frames = frames.0 * frames.1;

            for tile in tiles {
                let x_pixels = tile.x() as i32 * 16;
                let y_pixels = tile.y() as i32 * 16;
                let position = Position::new(PositionUnit(x_pixels), PositionUnit(y_pixels));

                if !view_rect.contains(position) {
                    continue;
                }

                let renderable = match &game_sprite_kind {
                    GameSpriteKind::Goal => {
                        const GOAL_FRAMES: usize = 9;

                        let enemy_goal =
                            !is_team_goal(self.settings.powerball_mode, position, self.spec_freq);

                        // First half of goal frames are team goals, second half are enemy.
                        // This increments the animation index to point into the appropriate set.
                        let animation_index = self.get_animation_index(GOAL_FRAMES, duration)
                            + enemy_goal as usize * GOAL_FRAMES;

                        &sprite_set.renderables[animation_index]
                    }
                    _ => {
                        let animation_index = self.get_animation_index(frames as usize, duration);
                        &sprite_set.renderables[animation_index]
                    }
                };

                render_state.sprite_renderer.draw(
                    &render_state.camera,
                    renderable,
                    x_pixels,
                    y_pixels,
                    Layer::Tiles,
                );
            }
        }

        if render_state
            .map_renderer
            .door_spriteset
            .renderables
            .is_empty()
        {
            return;
        }

        for door_tile in &self.map.doors {
            let current_id = self.map.get_tile(door_tile.x(), door_tile.y());

            // The map mutates its door tiles into a flag tile if it's considered open, so skip rendering it.
            if current_id == TILE_ID_FLAG {
                continue;
            }

            let x_pixels = door_tile.x() as i32 * 16;
            let y_pixels = door_tile.y() as i32 * 16;
            let position = Position::new(PositionUnit(x_pixels), PositionUnit(y_pixels));

            if !view_rect.contains(position) {
                continue;
            }

            // There are two door sets and each one is 4 frames. Dividing by 4 will give us the first or second half depending on tile id.
            let set = (door_tile.id() - TILE_ID_FIRST_DOOR) as usize / 4;
            let frame = self.get_animation_index(4, 40);

            let index = (set * 4) + frame;

            let renderable = &render_state.map_renderer.door_spriteset.renderables[index];

            render_state.sprite_renderer.draw(
                &render_state.camera,
                renderable,
                x_pixels,
                y_pixels,
                Layer::Tiles,
            );
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

        let local_now = GameTick::now(0);
        let tick_count = local_now.diff(&self.local_tick);

        self.receive_messages(&mut render_state)?;

        for _ in 0..tick_count {
            self.connection.tick();

            self.map
                .tick(&self.settings, self.connection.get_game_tick());

            self.simulation.tick(&self.map, &self.settings);

            if let Some(render_state) = &mut render_state {
                let self_position = Position::new(
                    PositionUnit(render_state.camera.position.x as i32 * 16000),
                    PositionUnit(render_state.camera.position.y as i32 * 16000),
                );

                self.radar.update(
                    render_state.config.width,
                    self.settings.map_zoom_factor as u16,
                    self_position,
                );

                for event in &self.simulation.events {
                    match &event.kind {
                        SimulationEventKind::WeaponExplosion(explosion) => {
                            let x_pixels = explosion.position.x.0 / 1000;
                            let y_pixels = explosion.position.y.0 / 1000;

                            match &explosion.kind {
                                WeaponKind::Bullet(_) | WeaponKind::Shrapnel(_) => {
                                    render_state.animation_renderer.add(
                                        GameSpriteKind::BulletExplosion,
                                        event.tick,
                                        0,
                                        6,
                                        7 * 6,
                                        x_pixels,
                                        y_pixels,
                                        Layer::Explosions,
                                    );
                                }
                                WeaponKind::Bomb(_)
                                | WeaponKind::ProximityBomb(_)
                                | WeaponKind::Thor(_) => {
                                    render_state.animation_renderer.add(
                                        GameSpriteKind::BombExplosion,
                                        event.tick,
                                        0,
                                        43,
                                        44 * 3,
                                        x_pixels,
                                        y_pixels,
                                        Layer::Explosions,
                                    );
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }

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

    fn receive_messages(
        &mut self,
        render_state: &mut Option<&mut RenderState>,
    ) -> Result<(), ConnectionError> {
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
                self.process_message(render_state, message)?;
            } else {
                // We are done processing everything now.
                break;
            }
        }

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

                if let Some(render_state) = render_state {
                    render_state.camera.position = glam::Vec2::new(0.0f32, 0.0f32);
                }
            }
            GameServerMessage::ArenaSettings(settings_message) => {
                log::debug!("Received arena settings");
                self.settings = settings_message.clone();

                if self.settings.door_mode >= 0 {
                    self.map.door_rng = Some(DoorRng::new(
                        self.settings.door_mode as u32,
                        self.connection.get_game_tick(),
                        self.settings.door_mode as u8,
                        self.settings.door_mode as u8,
                    ));

                    self.map.set_door_mode(self.settings.door_mode as u8);
                }
            }
            GameServerMessage::SynchronizationRequest(sync) => {
                self.map.set_door_seed(sync.door_seed, sync.timestamp);
                self.map.tick(&self.settings, sync.timestamp);

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

                        let sim_ticks = self.connection.get_game_tick().diff(&message_timestamp);

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

                    let direction = message.direction;

                    if player.last_position_timestamp < message_timestamp {
                        player.velocity = velocity;

                        let sim_ticks = self.connection.get_game_tick().diff(&message_timestamp);

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
                            position,
                            velocity,
                            direction,
                            weapon_kind,
                            &self.settings,
                            message_timestamp,
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
                    killed.position = None;
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
                    player.position = None;
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
                            if let Ok(new_map) =
                                Map::new(&info.filename, &map_data, self.map.door_rng)
                            {
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

                            if let Ok(new_map) =
                                Map::new(&self.map.filename, &inflated, self.map.door_rng)
                            {
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

        self.radar.invalidate();
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
