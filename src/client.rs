use crate::arena_settings::ArenaSettings;
use crate::chat::ChatController;
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
use crate::powerball::PowerballState;
use crate::powerball::is_team_goal;
use crate::radar::IndicatorFlag;
use crate::radar::Radar;
use crate::render::colors::ColorRenderableKind;
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
use crate::simulation::player_simulation::PLAYER_EXPLOSION_DURATION;
use crate::simulation::player_simulation::PLAYER_FLASH_DURATION;
use crate::simulation::player_simulation::update_player_lerp_target;
use crate::statbox::Statbox;
use crate::weapon::WeaponKind;

use miniz_oxide::inflate::decompress_to_vec_zlib;
use smol_str::format_smolstr;

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

    spectate_player_id: Option<PlayerId>,
    last_spectate_freq: u16,

    radar: Radar,
    // TODO: Remove. This is just for testing until input is handled.
    pub fullscreen_radar: bool,

    pub chat_controller: ChatController,
    pub statbox: Statbox,
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
            spectate_player_id: None,
            last_spectate_freq: 0xFFFF,
            radar: Radar::new(),
            fullscreen_radar: false,
            chat_controller: ChatController::new(),
            statbox: Statbox::new(),
        })
    }

    pub fn get_view_self(&self) -> Option<&Player> {
        let id = if let Some(spectate_player_id) = self.spectate_player_id {
            spectate_player_id
        } else {
            self.connection.player_id
        };

        self.simulation.player_manager.get_by_id(id)
    }

    pub fn get_freq(&self) -> u16 {
        let Some(spectate_player_id) = self.spectate_player_id else {
            return self.last_spectate_freq;
        };

        if let Some(spectate_player) = self.simulation.player_manager.get_by_id(spectate_player_id)
        {
            spectate_player.frequency
        } else {
            self.last_spectate_freq
        }
    }

    pub fn render(&mut self, render_state: &mut RenderState, sprites: &GameSprites) {
        self.chat_controller.render(render_state);
        self.statbox
            .render(&self.simulation.player_manager, render_state, sprites);

        if let Some(spectate_player_id) = self.spectate_player_id {
            if let Some(player) = self.simulation.player_manager.get_by_id(spectate_player_id) {
                if let Some(player_position) = player.position {
                    render_state.camera.position = player_position.into();
                }
            }
        }

        self.render_players(render_state, sprites);
        self.render_weapons(render_state, sprites);
        self.render_powerballs(render_state, sprites);

        self.render_map_animations(render_state, sprites);

        self.radar.render(
            render_state,
            sprites,
            &self.map,
            self.settings.map_zoom_factor as u16,
            self.get_freq(),
            self.settings.powerball_mode,
            self.fullscreen_radar,
        );
    }

    pub fn spectate_player(&mut self, player_id: Option<PlayerId>) {
        let Some(player_id) = player_id else {
            if let Some(existing_spectate_player_id) = self.spectate_player_id {
                if let Some(player) = self
                    .simulation
                    .player_manager
                    .get_by_id(existing_spectate_player_id)
                {
                    self.last_spectate_freq = player.frequency;
                }
            }

            self.spectate_player_id = None;
            return;
        };

        if let Some(existing_id) = self.spectate_player_id {
            if existing_id == player_id {
                return;
            }
        }

        if let Some(_) = self.simulation.player_manager.get_by_id(player_id) {
            self.spectate_player_id = Some(player_id);

            let spectate_request = SpectateMessage { player_id };

            if let Err(e) = self.connection.send_reliable(&spectate_request) {
                log::error!("{e}");
            }
        }
    }

    fn render_players(&mut self, render_state: &mut RenderState, sprites: &GameSprites) {
        let self_position = Position::new(
            PositionUnit(render_state.camera.position.x as i32 * 16000),
            PositionUnit(render_state.camera.position.y as i32 * 16000),
        );

        let (self_view_id, self_indicator_flags) =
            if let Some(spectate_id) = self.spectate_player_id {
                (
                    spectate_id,
                    IndicatorFlag::SmallMap | IndicatorFlag::FullMap,
                )
            } else {
                (
                    self.simulation.player_manager.self_id,
                    IndicatorFlag::FullMap,
                )
            };

        if let Some(player) = self.simulation.player_manager.get_by_id(self_view_id) {
            let color_kind = if player.flag_count > 0 {
                ColorRenderableKind::RadarSelfFlagCarry
            } else {
                ColorRenderableKind::RadarSelf
            };

            self.radar.add_indicator(
                color_kind,
                self_position,
                self.connection.get_game_tick(),
                self_indicator_flags,
            );
        }

        for player in &self.simulation.player_manager.players {
            if player.ship_kind == ShipKind::Spectator {
                continue;
            }

            let Some(player_position) = player.position else {
                continue;
            };

            let x_pixels = player_position.x.0 / 1000;
            let y_pixels = player_position.y.0 / 1000;

            if player.explosion_remaining_ticks > 0 {
                if let Some(explosion_renderables) =
                    sprites.get_set(GameSpriteKind::PlayerExplosion)
                {
                    let tick_count = PLAYER_EXPLOSION_DURATION - player.explosion_remaining_ticks;

                    let frame_count = explosion_renderables.renderables.len();
                    let ticks_per_frame = PLAYER_EXPLOSION_DURATION as usize / frame_count;
                    let index = (tick_count as usize / ticks_per_frame).min(frame_count - 1);

                    let renderable = &explosion_renderables.renderables[index];

                    render_state.sprite_renderer.draw_centered(
                        &render_state.camera,
                        renderable,
                        x_pixels,
                        y_pixels,
                        Layer::AfterShips,
                    );
                }
            } else if player.flash_remaining_ticks > 0 {
                if let Some(flash_renderables) = sprites.get_set(GameSpriteKind::Flash) {
                    let tick_count = PLAYER_FLASH_DURATION - player.flash_remaining_ticks;

                    let frame_count = flash_renderables.renderables.len();
                    let ticks_per_frame = PLAYER_FLASH_DURATION as usize / frame_count;
                    let index = (tick_count as usize / ticks_per_frame).min(frame_count - 1);

                    let renderable = &flash_renderables.renderables[index];

                    render_state.sprite_renderer.draw_centered(
                        &render_state.camera,
                        renderable,
                        x_pixels,
                        y_pixels,
                        Layer::AfterShips,
                    );
                }
            }

            // Player indicator continues to be on radar even while they are exploding, so add it before the enter delay check.
            // Don't render our own indicator because we did it already.
            if Some(player.id) != self.spectate_player_id {
                let color_kind = if player.frequency == self.get_freq() {
                    ColorRenderableKind::RadarTeammate
                } else {
                    ColorRenderableKind::RadarEnemyTarget
                };

                self.radar.add_indicator(
                    color_kind,
                    player_position,
                    self.connection.get_game_tick(),
                    IndicatorFlag::SmallMap,
                );
            }

            if player.enter_delay > 0 {
                continue;
            }

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

                let name_color = if player.frequency == self.get_freq() {
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

                if let Some(energy) = player.energy {
                    let energy_x = x_pixels - (renderable.size[0] as i32) / 2;
                    let energy_y = y_pixels + (renderable.size[1] as i32) / 2;

                    let initial_energy = (self
                        .settings
                        .get_ship_settings(player.ship_kind)
                        .initial_energy) as u32;

                    let energy_color = if energy <= initial_energy / 4 {
                        TextColor::DarkRed
                    } else if energy <= initial_energy / 2 {
                        TextColor::Yellow
                    } else {
                        TextColor::White
                    };

                    render_state.draw_world_text(
                        &format_smolstr!("{}", energy),
                        energy_x,
                        energy_y,
                        Layer::Ships,
                        energy_color,
                        TextAlignment::Right,
                    );
                }
            }
        }
    }

    fn render_weapons(&mut self, render_state: &mut RenderState, sprites: &GameSprites) {
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
                    let renderable_index = (bouncing.level * 4) as usize + 5 * 4 + animation_index;
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
                    let ticks = (weapon.last_update_tick - weapon.spawn_timestamp).value() as usize;
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
                    if let Some(player) = self.simulation.player_manager.get_by_id(weapon.player_id)
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

                            let name_color = if player.frequency == self.get_freq() {
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

            match &weapon.kind {
                WeaponKind::Bomb(bomb)
                | WeaponKind::ProximityBomb(bomb)
                | WeaponKind::Thor(bomb) => {
                    if let Some(player) = self.get_view_self() {
                        if player.ship_kind != ShipKind::Spectator {
                            let mut visbility_level = self
                                .settings
                                .get_ship_settings(player.ship_kind)
                                .see_bomb_level;

                            if bomb.mine
                                && !self.settings.get_ship_settings(player.ship_kind).see_mines
                            {
                                visbility_level = 0;
                            }

                            if visbility_level > 0 && visbility_level <= 1 + bomb.level as u16 {
                                self.radar.add_indicator(
                                    ColorRenderableKind::RadarBomb,
                                    weapon.position,
                                    self.connection.get_game_tick(),
                                    IndicatorFlag::SmallMap,
                                );
                            }
                        }
                    }
                }
                WeaponKind::Decoy(_) => {
                    let color_kind = if self.get_freq() == weapon.frequency {
                        ColorRenderableKind::RadarDecoy
                    } else {
                        ColorRenderableKind::RadarEnemyTarget
                    };

                    self.radar.add_indicator(
                        color_kind,
                        weapon.position,
                        self.connection.get_game_tick(),
                        IndicatorFlag::SmallMap,
                    );
                }
                _ => {}
            }
        }
    }

    pub fn render_powerballs(&self, render_state: &mut RenderState, sprites: &GameSprites) {
        let Some(ball_sprites) = sprites.get_set(GameSpriteKind::Powerball) else {
            return;
        };

        let render_duration = 100;

        for ball in &self.simulation.powerball_manager.balls {
            match &ball.state {
                PowerballState::World => {
                    if ball.remaining_pickup_ticks > 80 {
                        continue;
                    }

                    let phasing = ball.is_phasing(
                        self.connection.get_game_tick(),
                        self.settings.powerball_pass_delay as i32,
                    );

                    let x_pixels = ball.position.x.0 / 1000;
                    let y_pixels = ball.position.y.0 / 1000;
                    let index =
                        self.get_animation_index(10, render_duration) + phasing as usize * 10;

                    let renderable = &ball_sprites.renderables[index];

                    render_state.sprite_renderer.draw_centered(
                        &render_state.camera,
                        renderable,
                        x_pixels,
                        y_pixels,
                        Layer::AfterWeapons,
                    );
                }
                PowerballState::Carried => {
                    if let Some(carrier) = self.simulation.player_manager.get_by_id(ball.carrier_id)
                    {
                        if carrier.ship_kind == ShipKind::Spectator {
                            continue;
                        }

                        if let Some(position) = carrier.position {
                            let index = self.get_animation_index(10, render_duration);
                            let heading = carrier.get_heading();
                            let offset = heading
                                * self
                                    .settings
                                    .get_ship_settings(carrier.ship_kind)
                                    .get_radius() as f32;

                            let renderable = &ball_sprites.renderables[index];

                            let x_pixels = position.x.0 / 1000 + offset.x as i32;
                            let y_pixels = position.y.0 / 1000 + offset.y as i32;

                            render_state.sprite_renderer.draw_centered(
                                &render_state.camera,
                                renderable,
                                x_pixels,
                                y_pixels,
                                Layer::AfterWeapons,
                            );
                        }
                    }
                }
                _ => {}
            }
        }
    }

    pub fn render_trails(&mut self, render_state: &mut RenderState) {
        const BULLET_TRAIL_DURATION: u32 = 14;
        const BOMB_TRAIL_DURATION: u32 = 30;

        let current_tick = self.connection.current_tick;

        for weapon in &mut self.simulation.weapon_manager.weapons {
            match &weapon.kind {
                WeaponKind::Bullet(bullet) | WeaponKind::BouncingBullet(bullet) => {
                    let trail_diff = current_tick.diff(&weapon.last_trail_tick);

                    if trail_diff < 2 {
                        continue;
                    }

                    let start_index = (bullet.level as usize * 14) + 3 * 14;
                    let (x_pixels, y_pixels) = weapon.position.to_pixels();

                    render_state.animation_renderer.add(
                        GameSpriteKind::Gradient,
                        current_tick,
                        start_index,
                        start_index + 14,
                        BULLET_TRAIL_DURATION,
                        x_pixels,
                        y_pixels,
                        Layer::Weapons,
                    );

                    weapon.last_trail_tick = current_tick;
                }
                WeaponKind::Shrapnel(shrapnel) => {
                    let trail_diff = current_tick.diff(&weapon.last_trail_tick);

                    if trail_diff < 2 {
                        continue;
                    }

                    let start_index = (shrapnel.level as usize * 14) + 3 * 14;
                    let (x_pixels, y_pixels) = weapon.position.to_pixels();

                    render_state.animation_renderer.add(
                        GameSpriteKind::Gradient,
                        current_tick,
                        start_index,
                        start_index + 14,
                        BULLET_TRAIL_DURATION,
                        x_pixels,
                        y_pixels,
                        Layer::Weapons,
                    );

                    weapon.last_trail_tick = current_tick;
                }
                WeaponKind::Burst(_) => {
                    let trail_diff = current_tick.diff(&weapon.last_trail_tick);

                    if trail_diff < 2 {
                        continue;
                    }

                    let start_index = 5 * 14;
                    let (x_pixels, y_pixels) = weapon.position.to_pixels();

                    render_state.animation_renderer.add(
                        GameSpriteKind::Gradient,
                        current_tick,
                        start_index,
                        start_index + 14,
                        BULLET_TRAIL_DURATION,
                        x_pixels,
                        y_pixels,
                        Layer::Weapons,
                    );

                    weapon.last_trail_tick = current_tick;
                }
                WeaponKind::Bomb(bomb) | WeaponKind::ProximityBomb(bomb) => {
                    let trail_diff = current_tick.diff(&weapon.last_trail_tick);

                    if trail_diff < 5 {
                        continue;
                    }

                    let start_index = bomb.level as usize * 10;
                    let (x_pixels, y_pixels) = weapon.position.to_pixels();

                    render_state.animation_renderer.add(
                        GameSpriteKind::Trail,
                        current_tick,
                        start_index,
                        start_index + 10,
                        BOMB_TRAIL_DURATION,
                        x_pixels,
                        y_pixels,
                        Layer::Weapons,
                    );

                    weapon.last_trail_tick = current_tick;
                }
                WeaponKind::Thor(_) => {
                    let trail_diff = current_tick.diff(&weapon.last_trail_tick);

                    if trail_diff < 5 {
                        continue;
                    }

                    let start_index = 4 * 10;
                    let (x_pixels, y_pixels) = weapon.position.to_pixels();

                    render_state.animation_renderer.add(
                        GameSpriteKind::Trail,
                        current_tick,
                        start_index,
                        start_index + 10,
                        BOMB_TRAIL_DURATION,
                        x_pixels,
                        y_pixels,
                        Layer::Weapons,
                    );

                    weapon.last_trail_tick = current_tick;
                }
                _ => {}
            }
        }

        for ball in &mut self.simulation.powerball_manager.balls {
            let trail_diff = current_tick.diff(&ball.last_trail_tick);

            if trail_diff < 3 {
                continue;
            }

            if ball.velocity.x.0 != 0 || ball.velocity.y.0 != 0 {
                let start_index = 20;
                let (x_pixels, y_pixels) = ball.position.to_pixels();

                render_state.animation_renderer.add(
                    GameSpriteKind::Powerball,
                    current_tick,
                    start_index,
                    start_index + 10,
                    BOMB_TRAIL_DURATION,
                    x_pixels,
                    y_pixels,
                    Layer::Weapons,
                );

                ball.last_trail_tick = current_tick;
            }
        }
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

        let view_min = Position::from_pixels(
            PixelUnit(center_x - half_width),
            PixelUnit(center_y - half_height),
        );
        let view_max = Position::from_pixels(
            PixelUnit(center_x + half_width),
            PixelUnit(center_y + half_height),
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
                let position = Position::from_pixels(PixelUnit(x_pixels), PixelUnit(y_pixels));

                if !view_rect.contains(position) {
                    continue;
                }

                let renderable = match &game_sprite_kind {
                    GameSpriteKind::Goal => {
                        const GOAL_FRAMES: usize = 9;

                        let enemy_goal =
                            !is_team_goal(self.settings.powerball_mode, position, self.get_freq());

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

        let self_freq = self.get_freq();

        if let Some(brick_sprites) = sprites.get_set(GameSpriteKind::Brick) {
            for brick in &self.map.bricks {
                let index =
                    self.get_animation_index(10, 50) + (self_freq == brick.frequency) as usize * 10;

                let renderable = &brick_sprites.renderables[index];
                let x_pixels = brick.tile.x() as i32 * 16;
                let y_pixels = brick.tile.y() as i32 * 16;

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
            let position = Position::from_pixels(PixelUnit(x_pixels), PixelUnit(y_pixels));

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

        self.receive_messages(&mut render_state)?;

        let local_now = GameTick::now(0);
        let tick_count = local_now.diff(&self.local_tick);

        for _ in 0..tick_count {
            self.connection.tick();

            self.map
                .tick(&self.settings, self.connection.get_game_tick());

            self.simulation.tick(&self.map, &self.settings);

            if let Some(render_state) = &mut render_state {
                self.render_trails(render_state);

                let self_position = Position::new(
                    PositionUnit(render_state.camera.position.x as i32 * 16000),
                    PositionUnit(render_state.camera.position.y as i32 * 16000),
                );

                self.radar.update(
                    render_state.config.width,
                    self.settings.map_zoom_factor as u16,
                    self_position,
                    self.connection.get_game_tick(),
                );

                for event in &self.simulation.events {
                    match &event.kind {
                        SimulationEventKind::WeaponExplosion(explosion) => {
                            let x_pixels = explosion.position.x.0 / 1000;
                            let y_pixels = explosion.position.y.0 / 1000;

                            match &explosion.kind {
                                WeaponKind::Bullet(_)
                                | WeaponKind::BouncingBullet(_)
                                | WeaponKind::Shrapnel(_) => {
                                    render_state.animation_renderer.add(
                                        GameSpriteKind::BulletExplosion,
                                        event.tick,
                                        0,
                                        7,
                                        7 * 6,
                                        x_pixels,
                                        y_pixels,
                                        Layer::AfterShips,
                                    );
                                }
                                WeaponKind::Bomb(bomb)
                                | WeaponKind::ProximityBomb(bomb)
                                | WeaponKind::Thor(bomb) => {
                                    let (kind, frames, duration) = if bomb.emp {
                                        (GameSpriteKind::EmpExplosion, 10, 40)
                                    } else {
                                        (GameSpriteKind::BombExplosion, 44, 44 * 3)
                                    };

                                    render_state.animation_renderer.add(
                                        kind,
                                        event.tick,
                                        0,
                                        frames,
                                        duration,
                                        x_pixels,
                                        y_pixels,
                                        Layer::Explosions,
                                    );

                                    const RADAR_EXPLOSION_DURATION: i32 = 132;

                                    if explosion.frequency == self.get_freq() {
                                        self.radar.add_indicator(
                                            ColorRenderableKind::RadarExplosion,
                                            explosion.position,
                                            self.connection.get_game_tick()
                                                + RADAR_EXPLOSION_DURATION,
                                            IndicatorFlag::SmallMap,
                                        );
                                    } else {
                                        // We render the RadarBomb color if have visibility of bombs because we terminate weapons differently than Continuum.
                                        // Continuum keeps the weapon around with its RadarBomb still animating during the explosion, but we remove the weapon
                                        // and have to do it manually here.
                                        if let Some(player) = self.get_view_self() {
                                            if player.ship_kind != ShipKind::Spectator {
                                                let visbility_level = self
                                                    .settings
                                                    .get_ship_settings(player.ship_kind)
                                                    .see_bomb_level;

                                                if visbility_level > 0
                                                    && visbility_level <= 1 + bomb.level as u16
                                                {
                                                    self.radar.add_indicator(
                                                        ColorRenderableKind::RadarBomb,
                                                        explosion.position,
                                                        self.connection.get_game_tick()
                                                            + RADAR_EXPLOSION_DURATION,
                                                        IndicatorFlag::SmallMap,
                                                    );
                                                }
                                            }
                                        }
                                    }
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
            GameServerMessage::Chat(chat) => {
                let mut sender_name = String::new();

                match chat.kind {
                    ChatKind::Public | ChatKind::PublicMacro => {
                        if let Some(sender) = self.simulation.player_manager.get_by_id(chat.sender)
                        {
                            log::debug!("{}> {}", sender.name, chat.message);
                            sender_name = sender.name.clone();
                        }
                    }
                    ChatKind::Team => {
                        if let Some(sender) = self.simulation.player_manager.get_by_id(chat.sender)
                        {
                            log::debug!("T {}> {}", sender.name, chat.message);
                            sender_name = sender.name.clone();
                        }
                    }
                    ChatKind::Frequency => {
                        if let Some(sender) = self.simulation.player_manager.get_by_id(chat.sender)
                        {
                            log::debug!("F {}> {}", sender.name, chat.message);

                            sender_name = sender.name.clone();
                        }
                    }
                    ChatKind::Arena | ChatKind::Error | ChatKind::Warning => {
                        if !chat.message.is_empty() {
                            log::debug!("A {}", chat.message);
                        }
                    }
                    ChatKind::Private => {
                        if let Some(sender) = self.simulation.player_manager.get_by_id(chat.sender)
                        {
                            log::debug!("P {}> {}", sender.name, chat.message);

                            sender_name = sender.name.clone();
                        }
                    }
                    ChatKind::RemotePrivate => {
                        log::debug!("RP {}", chat.message);
                    }
                    ChatKind::Channel => {
                        log::debug!("C {}", chat.message);
                    }
                    ChatKind::Fuchsia => {
                        log::debug!("F {}", chat.message);
                    }
                }

                self.chat_controller.handle_chat_message(
                    chat.kind,
                    sender_name,
                    chat.message.clone(),
                );
            }
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
            GameServerMessage::PlayerId(message) => {
                // We need to initialize the simulation here before we receive player enter events.
                self.simulation = Simulation::new(self.connection.get_game_tick());
                self.last_position_tick = self.connection.get_game_tick();
                self.simulation.player_manager.self_id = message.id;

                self.map.clear_bricks();

                if let Some(render_state) = render_state {
                    render_state.camera.position = glam::Vec2::new(0.0f32, 0.0f32);

                    render_state.animation_renderer.clear();
                    self.chat_controller.clear();
                }

                // TODO: Test code for switching through views until input is handled.
                self.statbox.set_view(
                    &self.simulation.player_manager,
                    crate::statbox::StatboxView::Names,
                );
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
            GameServerMessage::BrickDrop(message) => {
                let start = glam::Vec2::new(message.x1 as f32, message.y1 as f32);
                let end = glam::Vec2::new(message.x2 as f32, message.y2 as f32);
                let direction = (end - start).normalize();
                let distance = start.distance(end).ceil() as i32 + 1;

                let end_tick = message.timestamp + self.settings.brick_time as i32;

                let mut position = start;

                // TODO: Self brick warp.

                for _ in 0..distance {
                    self.map.insert_brick(
                        position.x as u16,
                        position.y as u16,
                        message.frequency,
                        end_tick,
                    );

                    position += direction;
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
                        entry.flag_points,
                        entry.kill_points,
                    );

                    player.wins = entry.kills;
                    player.losses = entry.deaths;
                    player.flag_count = entry.flag_count;
                    player.attach_parent = entry.attach_parent;
                    player.last_position_timestamp = self.connection.get_game_tick();

                    log::debug!("{} entered arena {:?}", entry.name, entry.ship_kind);

                    if !sent_spectate_request && entry.ship_kind != ShipKind::Spectator {
                        let spectate_request = SpectateMessage {
                            player_id: entry.player_id,
                        };

                        self.connection.send_reliable(&spectate_request)?;
                        self.spectate_player_id = Some(player.id);

                        log::debug!("Spectating target {}", entry.name);
                        sent_spectate_request = true;
                    }

                    // If there was someone already in this place, say that they left.
                    // This can happen when joining at the same exact time as other players.
                    if let Some(old_player) = self.simulation.player_manager.add_player(player) {
                        log::debug!("{} left arena", old_player.name);
                    }
                }

                self.statbox.rebuild(&self.simulation.player_manager);
            }
            GameServerMessage::PlayerLeaving(leaving) => {
                if let Some(player) = self
                    .simulation
                    .player_manager
                    .remove_player(leaving.player_id)
                {
                    log::debug!("{} left arena", player.name);
                }

                self.statbox.rebuild(&self.simulation.player_manager);
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

                    if message.status & StatusFlags::Flash != 0 {
                        // Always override new flashes if we get one, even if the message timestamp is older.
                        if player.flash_remaining_ticks == 0 {
                            player.status |= StatusFlags::Flash;
                        }

                        if let Some(current_position) = player.position {
                            let (x_pixels, y_pixels) = current_position.to_pixels();

                            if let Some(render_state) = render_state {
                                let (cols, rows) =
                                    GAME_SPRITE_SHEET_DEFINITIONS[GameSpriteKind::Flash as usize];
                                let frame_count = cols * rows;

                                render_state.animation_renderer.add(
                                    GameSpriteKind::Flash,
                                    message_timestamp,
                                    0,
                                    frame_count as usize,
                                    PLAYER_FLASH_DURATION,
                                    x_pixels,
                                    y_pixels,
                                    Layer::Explosions,
                                );
                            }
                        }
                    }

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

                        if let Some(extra) = &message.extra {
                            player.energy = Some(extra.energy as u32);
                        }
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

                    if message.status & StatusFlags::Flash != 0 {
                        // Always override new flashes if we get one, even if the message timestamp is older.
                        if player.flash_remaining_ticks == 0 {
                            player.status |= StatusFlags::Flash;
                        }

                        if let Some(current_position) = player.position {
                            let (x_pixels, y_pixels) = current_position.to_pixels();

                            if let Some(render_state) = render_state {
                                let (cols, rows) =
                                    GAME_SPRITE_SHEET_DEFINITIONS[GameSpriteKind::Flash as usize];
                                let frame_count = cols * rows;

                                render_state.animation_renderer.add(
                                    GameSpriteKind::Flash,
                                    message_timestamp,
                                    0,
                                    frame_count as usize,
                                    PLAYER_FLASH_DURATION,
                                    x_pixels,
                                    y_pixels,
                                    Layer::Explosions,
                                );
                            }
                        }
                    }

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

                        if let Some(extra) = &message.extra {
                            player.energy = Some(extra.energy as u32);
                        }
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
                    killer.wins = killer.wins.wrapping_add(1);
                    killer.kill_points = killer.kill_points.wrapping_add(message.bounty as i32);
                    killer.bounty = killer
                        .bounty
                        .wrapping_add_signed(self.settings.bounty_increase_for_kill);
                }

                if let Some(killed) = self
                    .simulation
                    .player_manager
                    .get_by_id_mut(message.killed_id)
                {
                    killed.enter_delay = self.settings.enter_delay as u16;
                    killed.explosion_remaining_ticks = PLAYER_EXPLOSION_DURATION;
                    killed.losses = killed.losses.wrapping_add(1);
                }

                self.statbox.rebuild(&self.simulation.player_manager);
            }
            GameServerMessage::ScoreUpdate(message) => {
                if let Some(player) = self
                    .simulation
                    .player_manager
                    .get_by_id_mut(message.player_id)
                {
                    player.kill_points = message.kill_points;
                    player.flag_points = message.flag_points;
                    player.wins = message.kills;
                    player.losses = message.deaths;
                    self.statbox.rebuild(&self.simulation.player_manager);
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

                self.statbox.rebuild(&self.simulation.player_manager);
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

                self.statbox.rebuild(&self.simulation.player_manager);
            }
            GameServerMessage::PowerballPosition(message) => {
                self.simulation.powerball_manager.on_ball_position_message(
                    &mut self.simulation.player_manager,
                    &self.settings,
                    message,
                );
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
        self.simulation.powerball_paused = false;
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
