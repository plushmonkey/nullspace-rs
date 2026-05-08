use smol_str::format_smolstr;

use crate::{
    arena_settings::ArenaSettings,
    clock::GameTick,
    input::{InputAction, InputState},
    math::MAX_POSITION,
    net::{
        connection::Connection,
        packet::{
            c2s::SpectateMessage,
            s2c::{PlayerLeavingMessage, PlayerTeamAndShipChangeMessage},
        },
    },
    player::{Player, PlayerId, PlayerManager, StatusFlags},
    render::{
        game_sprites::{GameSpriteKind, GameSprites},
        layer::Layer,
        render_state::RenderState,
        text_renderer::{TextAlignment, TextColor},
    },
    ship::ShipKind,
    statbox::Statbox,
};

pub struct SpectateController {
    pub spectate_player_id: Option<PlayerId>,
    pub last_spectate_freq: u16,
    freecam: bool,
    pub xradar: bool,
}

impl SpectateController {
    pub fn new() -> Self {
        Self {
            spectate_player_id: None,
            last_spectate_freq: 0xFFFF,
            freecam: false,
            xradar: false,
        }
    }

    pub fn tick(
        &mut self,
        input_state: &InputState,
        player_manager: &mut PlayerManager,
        connection: &mut Connection,
        statbox: &Statbox,
        settings: &ArenaSettings,
    ) {
        if let Some(spectate_id) = self.spectate_player_id {
            if let Some(player) = player_manager.get_by_id(spectate_id) {
                if let Some(player_position) = player.position {
                    player_manager.get_self_mut().unwrap().position = Some(player_position);
                }
            }
        }

        if let Some(me) = player_manager.get_self_mut() {
            if let Some(me_position) = &mut me.position {
                let mut offset_x = 0;
                let mut offset_y = 0;

                if input_state.is_down(InputAction::MoveLeft) {
                    offset_x -= 1;
                }

                if input_state.is_down(InputAction::MoveRight) {
                    offset_x += 1;
                }

                if input_state.is_down(InputAction::MoveForward) {
                    offset_y -= 1;
                }

                if input_state.is_down(InputAction::MoveBackward) {
                    offset_y += 1;
                }

                if input_state.is_modifier_down(crate::input::InputModifier::Shift) {
                    offset_x *= 8000 * 2;
                    offset_y *= 8000 * 2;
                } else {
                    offset_x *= 8000;
                    offset_y *= 8000;
                }

                if offset_x != 0 || offset_y != 0 {
                    me_position.x.0 += offset_x;
                    me_position.y.0 += offset_y;

                    me_position.x.0 = me_position.x.0.clamp(0, MAX_POSITION);
                    me_position.y.0 = me_position.y.0.clamp(0, MAX_POSITION);

                    self.freecam = true;
                    self.spectate_player(None, player_manager, connection);
                }
            }
        }

        if input_state.is_modifier_down(crate::input::InputModifier::Control) {
            let selected_player_id = statbox.get_selected_player_id();

            if let Some(player) = player_manager.get_by_id(selected_player_id) {
                if player.ship_kind != ShipKind::Spectator {
                    self.spectate_player(Some(selected_player_id), player_manager, connection);
                }
            }
        }

        if input_state.is_triggered(InputAction::XRadar) {
            self.xradar = !self.xradar;
        }

        if settings.no_spec_xradar {
            self.xradar = false;
        }
    }

    pub fn render(
        &self,
        render_state: &mut RenderState,
        sprites: &GameSprites,
        player_manager: &PlayerManager,
        settings: &ArenaSettings,
        current_tick: GameTick,
    ) {
        if let Some(spectate_player_id) = self.spectate_player_id {
            if let Some(spectate_player) = player_manager.get_by_id(spectate_player_id) {
                Self::render_extra_data(render_state, spectate_player, current_tick);
            }
        }

        if !settings.no_spec_xradar {
            if let Some(icon_sprites) = sprites.get_set(GameSpriteKind::Icons) {
                let icon_width = icon_sprites.renderables[0].size[0] as i32;
                let icon_height = icon_sprites.renderables[0].size[1] as i32;

                let x = render_state.config.width as i32 - icon_width;
                let y = (render_state.config.height / 2) as i32 - 26 * 2 + icon_height * 4;

                let index = if self.xradar { 36 } else { 37 };

                let renderable = &icon_sprites.renderables[index];

                render_state.sprite_renderer.draw(
                    &render_state.ui_camera,
                    renderable,
                    x,
                    y,
                    Layer::Gauges,
                );
            }
        }
    }

    fn render_extra_data(render_state: &mut RenderState, player: &Player, current_tick: GameTick) {
        const EXTRA_DATA_TIMEOUT: i32 = 300;

        let Some(last_extra_tick) = player.last_extra_data_timestamp else {
            return;
        };

        let Some(extra) = player.extra_position_data else {
            return;
        };

        if current_tick.diff(&last_extra_tick) > EXTRA_DATA_TIMEOUT {
            return;
        }

        let super_string = if extra.items.super_active {
            "Super!"
        } else {
            "      "
        };

        let shields_string = if extra.items.shield_active {
            "Shields"
        } else {
            ""
        };

        let stealth_string = if player.status & StatusFlags::Stealth != 0 {
            "Stealth"
        } else {
            "       "
        };

        let cloak_string = if player.status & StatusFlags::Cloak != 0 {
            "Cloak"
        } else {
            ""
        };

        let antiwarp_string = if player.status & StatusFlags::Antiwarp != 0 {
            "Antiwarp"
        } else {
            ""
        };

        let rows = [
            format_smolstr!("Engy:{}  S2CLatency:{}ms", extra.energy, extra.s2c_latency),
            format_smolstr!(
                "Brst:{}  Repl:{}  Prtl:{}",
                extra.items.bursts,
                extra.items.repels,
                extra.items.portals
            ),
            format_smolstr!(
                "Decy:{}  Thor:{}  {} {}",
                extra.items.decoys,
                extra.items.thors,
                stealth_string,
                cloak_string
            ),
            format_smolstr!(
                "Wall:{}  Rckt:{}  {}",
                extra.items.bricks,
                extra.items.rockets,
                antiwarp_string
            ),
            format_smolstr!("{}  {}", super_string, shields_string),
            format_smolstr!("Timer:{}", extra.flag_timer),
        ];

        let x = ((render_state.config.width / 2) & !1) as i32;
        let mut y = 0;

        let row_count = 5 + (extra.flag_timer > 0) as usize;

        for i in 0..row_count {
            let row = &rows[i];

            render_state.text_renderer.draw(
                &mut render_state.sprite_renderer,
                &render_state.ui_camera,
                row,
                x,
                y,
                Layer::Gauges,
                TextColor::White,
                TextAlignment::Left,
            );

            y += render_state.text_renderer.character_height;
        }
    }

    pub fn spectate_player(
        &mut self,
        player_id: Option<PlayerId>,
        player_manager: &PlayerManager,
        connection: &mut Connection,
    ) {
        let Some(player_id) = player_id else {
            if let Some(existing_spectate_player_id) = self.spectate_player_id {
                if let Some(player) = player_manager.get_by_id(existing_spectate_player_id) {
                    self.last_spectate_freq = player.frequency;
                }
            }

            self.freecam = true;

            let spectate_request = SpectateMessage {
                player_id: PlayerId::invalid(),
            };

            if let Err(e) = connection.send_reliable(&spectate_request) {
                log::error!("{e}");
            }

            self.spectate_player_id = None;
            return;
        };

        if let Some(existing_id) = self.spectate_player_id {
            if existing_id == player_id {
                return;
            }
        }

        if let Some(_) = player_manager.get_by_id(player_id) {
            self.spectate_player_id = Some(player_id);

            let spectate_request = SpectateMessage { player_id };

            self.freecam = false;

            if let Err(e) = connection.send_reliable(&spectate_request) {
                log::error!("{e}");
            }
        }
    }

    pub fn handle_ship_change(
        &mut self,
        message: &PlayerTeamAndShipChangeMessage,
        player_manager: &mut PlayerManager,
        connection: &mut Connection,
        statbox: &Statbox,
    ) {
        if self.freecam {
            return;
        }

        if let Some(spectate_id) = self.spectate_player_id {
            if message.player_id == spectate_id {
                self.spectate_new_target(player_manager, connection, statbox);
            }
        } else {
            self.spectate_new_target(player_manager, connection, statbox);
        }
    }

    pub fn handle_player_entering(
        &mut self,
        player_manager: &mut PlayerManager,
        connection: &mut Connection,
        statbox: &Statbox,
    ) {
        if self.freecam {
            return;
        }

        if self.spectate_player_id.is_none() {
            self.spectate_new_target(player_manager, connection, statbox);
        }
    }

    pub fn handle_player_leave(
        &mut self,
        message: &PlayerLeavingMessage,
        player_manager: &mut PlayerManager,
        connection: &mut Connection,
        statbox: &Statbox,
    ) {
        if self.freecam {
            return;
        }

        if let Some(spectate_id) = self.spectate_player_id {
            if message.player_id == spectate_id {
                self.spectate_new_target(player_manager, connection, statbox);
            }
        }
    }

    // Goes through statbox to find the first player in a ship and spectates them.
    fn spectate_new_target(
        &mut self,
        player_manager: &mut PlayerManager,
        connection: &mut Connection,
        statbox: &Statbox,
    ) {
        let player_id = statbox.get_first_playing_id(player_manager);

        self.spectate_player(player_id, player_manager, connection);
    }
}
