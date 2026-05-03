use crate::{
    input::{InputAction, InputState},
    math::MAX_POSITION,
    net::{
        connection::Connection,
        packet::{
            c2s::SpectateMessage,
            s2c::{PlayerLeavingMessage, PlayerTeamAndShipChangeMessage},
        },
    },
    player::{PlayerId, PlayerManager},
    ship::ShipKind,
    statbox::Statbox,
};

pub struct SpectateController {
    pub spectate_player_id: Option<PlayerId>,
    pub last_spectate_freq: u16,
    freecam: bool,
}

impl SpectateController {
    pub fn new() -> Self {
        Self {
            spectate_player_id: None,
            last_spectate_freq: 0xFFFF,
            freecam: false,
        }
    }

    pub fn tick(
        &mut self,
        input_state: &InputState,
        player_manager: &mut PlayerManager,
        connection: &mut Connection,
        statbox: &Statbox,
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
