use smol_str::format_smolstr;

use crate::{
    arena_settings::ArenaSettings,
    clock::GameTick,
    map::{AnimatedTileKind, Map},
    math::{Position, Rectangle},
    net::{
        connection::Connection,
        packet::s2c::{
            FlagClaimMessage, FlagDropMessage, FlagPositionMessage, TurfFlagUpdateMessage,
        },
    },
    player::{PlayerId, PlayerManager},
    radar::{IndicatorFlag, Radar},
    render::{
        animation_renderer::get_animation_index,
        colors::ColorRenderableKind,
        game_sprites::{GameSpriteKind, GameSprites},
        layer::Layer,
        render_state::RenderState,
        text_renderer::{TextAlignment, TextColor},
    },
    ship::ShipKind,
};

#[derive(Copy, Clone)]
pub struct FlagWorldState {
    pub x_tile: u16,
    pub y_tile: u16,
    pub frequency: u16,

    pub hidden_ticks_remaining: u32,
    pub pickup_delay_ticks_remaining: u32,
}

#[derive(Copy, Clone)]
pub enum GameFlag {
    Turf(FlagWorldState),
    Carried(PlayerId),
    World(FlagWorldState),
    Unknown,
}

pub struct FlagController {
    pub flags: Vec<GameFlag>,
}

impl FlagController {
    const PICKUP_HIDE_TICKS: u32 = 300;
    const PICKUP_DELAY_TICKS: u32 = 20;

    pub fn new() -> Self {
        Self { flags: vec![] }
    }

    pub fn clear(&mut self) {
        self.flags.clear();
    }

    pub fn tick(
        &mut self,
        player_manager: &mut PlayerManager,
        connection: &mut Connection,
        settings: &ArenaSettings,
    ) {
        let current_tick = connection.get_game_tick();

        let carry_allowed = if settings.carry_flags == 1 {
            256
        } else if settings.carry_flags > 1 {
            settings.carry_flags as u16 - 1
        } else {
            0
        };

        for flag_id in 0..self.flags.len() {
            let flag = &mut self.flags[flag_id];

            let wz_flag = match flag {
                GameFlag::World(_) => true,
                _ => false,
            };

            match flag {
                GameFlag::World(state) | GameFlag::Turf(state) => {
                    if state.hidden_ticks_remaining > 0 {
                        state.hidden_ticks_remaining -= 1;
                    }

                    if state.pickup_delay_ticks_remaining > 0 {
                        state.pickup_delay_ticks_remaining -= 1;
                    }

                    if wz_flag && state.hidden_ticks_remaining > 0 {
                        continue;
                    }

                    let flag_collider = Rectangle::new(
                        Position::from_tile(state.x_tile as i32, state.y_tile as i32),
                        Position::from_tile(state.x_tile as i32 + 1, state.y_tile as i32 + 1),
                    );

                    for player in &player_manager.players {
                        if player.ship_kind == ShipKind::Spectator {
                            continue;
                        }

                        if player.enter_delay > 0 {
                            continue;
                        }

                        if player.frequency == state.frequency {
                            continue;
                        }

                        if carry_allowed > 0 && player.flag_count >= carry_allowed {
                            continue;
                        }

                        if !player.is_synchronized(current_tick) {
                            continue;
                        }

                        let player_collider = player.get_collider(
                            settings.get_ship_settings(player.ship_kind).get_radius(),
                        );

                        if player_collider.intersects(&flag_collider) {
                            if wz_flag {
                                state.hidden_ticks_remaining = Self::PICKUP_HIDE_TICKS;
                            }

                            if player.id == player_manager.self_id
                                && state.pickup_delay_ticks_remaining == 0
                            {
                                state.pickup_delay_ticks_remaining = Self::PICKUP_DELAY_TICKS;

                                let request = crate::net::packet::c2s::FlagRequestMessage {
                                    flag_id: flag_id as u16,
                                };
                                if let Err(e) = connection.send_reliable(&request) {
                                    log::error!("{e}");
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        if let Some(me) = player_manager.get_self_mut() {
            if me.flag_count > 0 && me.flag_remaining_ticks > 0 {
                me.flag_remaining_ticks -= 1;

                if me.flag_remaining_ticks == 0 {
                    let drop = crate::net::packet::c2s::DropFlagsMessage {};

                    if let Err(e) = connection.send_reliable(&drop) {
                        log::error!("{e}");
                    }
                }
            }
        }
    }

    pub fn handle_flag_position_message(&mut self, message: &FlagPositionMessage) {
        self.resize_to_fit_id(message.flag_id);

        let flag = &mut self.flags[message.flag_id as usize];

        *flag = GameFlag::World(FlagWorldState {
            x_tile: message.x,
            y_tile: message.y,
            frequency: message.owner_freq,
            hidden_ticks_remaining: 0,
            pickup_delay_ticks_remaining: 0,
        });
    }

    pub fn handle_flag_claim_message(
        &mut self,
        message: &FlagClaimMessage,
        player_manager: &mut PlayerManager,
        map: &Map,
        settings: &ArenaSettings,
    ) {
        self.resize_to_fit_id(message.flag_id);

        if settings.carry_flags > 0 {
            self.flags[message.flag_id as usize] = GameFlag::Carried(message.player_id);

            if let Some(player) = player_manager.get_by_id_mut(message.player_id) {
                player.flag_count += 1;

                player.flag_remaining_ticks = settings.flag_drop_delay as u32;
            }
        } else {
            let flag_tiles = &map.animated_tiles[AnimatedTileKind::Flag as usize];
            let flag_id = message.flag_id as usize;

            if flag_id < flag_tiles.len() {
                if let Some(player) = player_manager.get_by_id(message.player_id) {
                    self.flags[flag_id] = GameFlag::Turf(FlagWorldState {
                        x_tile: flag_tiles[flag_id].x(),
                        y_tile: flag_tiles[flag_id].y(),
                        frequency: player.frequency,
                        hidden_ticks_remaining: 0,
                        pickup_delay_ticks_remaining: 0,
                    });
                }
            }
        }
    }

    pub fn handle_flag_drop_message(
        &mut self,
        message: &FlagDropMessage,
        player_manager: &mut PlayerManager,
    ) {
        if let Some(player) = player_manager.get_by_id_mut(message.player_id) {
            player.flag_count = 0;
        }
    }

    pub fn handle_turf_update_message(&mut self, message: &TurfFlagUpdateMessage, map: &Map) {
        self.resize_to_fit_id(message.flag_teams.len() as u16);

        let flag_tiles = &map.animated_tiles[AnimatedTileKind::Flag as usize];

        for flag_id in 0..message.flag_teams.len() {
            let frequency = message.flag_teams[flag_id];

            if flag_id < flag_tiles.len() {
                self.flags[flag_id] = GameFlag::Turf(FlagWorldState {
                    x_tile: flag_tiles[flag_id].x(),
                    y_tile: flag_tiles[flag_id].y(),
                    frequency,
                    hidden_ticks_remaining: 0,
                    pickup_delay_ticks_remaining: 0,
                });
            }
        }
    }

    pub fn count(&self) -> usize {
        self.flags.len()
    }

    fn resize_to_fit_id(&mut self, id: u16) {
        if id as usize >= self.flags.len() {
            self.flags.resize(id as usize + 1, GameFlag::Unknown);
        }
    }

    pub fn render(
        &self,
        render_state: &mut RenderState,
        sprites: &GameSprites,
        radar: &mut Radar,
        current_tick: GameTick,
        frequency: u16,
        self_flag_ticks: u32,
    ) {
        let Some(flag_sprites) = sprites.get_set(GameSpriteKind::Flag) else {
            return;
        };

        let animation_index = get_animation_index(current_tick.value(), 10, 10 * 10);

        if self_flag_ticks > 0 {
            if let Some(drop_flag_sprites) = sprites.get_set(GameSpriteKind::DropFlag) {
                let renderable = &drop_flag_sprites.renderables[animation_index];
                let (ui_x, ui_y) = render_state
                    .get_hud_timer_position(crate::render::render_state::HudTimerKind::Flag);

                render_state.sprite_renderer.draw(
                    &render_state.ui_camera,
                    renderable,
                    ui_x,
                    ui_y,
                    Layer::Gauges,
                );

                let seconds = self_flag_ticks as f32 / 100.0f32;
                let text_y =
                    ui_y + renderable.size[1] as i32 - render_state.text_renderer.character_height;

                render_state.text_renderer.draw(
                    &mut render_state.sprite_renderer,
                    &render_state.ui_camera,
                    &format_smolstr!("{:.1}", seconds),
                    ui_x,
                    text_y,
                    Layer::Gauges,
                    TextColor::DarkRed,
                    TextAlignment::Right,
                );
            }
        }

        for flag in &self.flags {
            match flag {
                GameFlag::World(state) | GameFlag::Turf(state) => {
                    if state.hidden_ticks_remaining > 0 {
                        continue;
                    }

                    let owned = state.frequency == frequency;

                    let renderable =
                        &flag_sprites.renderables[animation_index + owned as usize * 10];
                    let x_pixels = state.x_tile as i32 * 16;
                    let y_pixels = state.y_tile as i32 * 16;

                    render_state.sprite_renderer.draw(
                        &render_state.camera,
                        renderable,
                        x_pixels,
                        y_pixels,
                        Layer::AfterTiles,
                    );

                    if owned {
                        let position =
                            Position::from_tile(state.x_tile as i32, state.y_tile as i32);
                        radar.add_indicator(
                            ColorRenderableKind::RadarTeamFlag,
                            position,
                            current_tick,
                            IndicatorFlag::SmallMap,
                        );
                    }
                }
                _ => {}
            }
        }
    }
}
