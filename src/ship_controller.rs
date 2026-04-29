use crate::{
    arena_settings::ArenaSettings,
    clock::GameTick,
    input::{InputAction, InputState},
    player::{PlayerId, PlayerManager},
    ship::{Ship, ShipKind},
};

pub struct ShipController {
    pub ship: Ship,
}

impl ShipController {
    pub fn new() -> Self {
        Self { ship: Ship::new() }
    }

    pub fn tick(
        &mut self,
        input_state: &InputState,
        player_manager: &mut PlayerManager,
        settings: &ArenaSettings,
        current_tick: GameTick,
    ) {
        if self.ship.repel_effect_remaining_ticks > 0 {
            self.ship.repel_effect_remaining_ticks -= 1;
        }

        self.perform_rotation(input_state, player_manager);
        self.perform_acceleration(input_state, player_manager, settings, current_tick);
    }

    fn perform_acceleration(
        &mut self,
        input_state: &InputState,
        player_manager: &mut PlayerManager,
        settings: &ArenaSettings,
        current_tick: GameTick,
    ) {
        let me = player_manager
            .get_self_mut()
            .expect("Ship controller player must exist");
        let ship_settings = settings.get_ship_settings(me.ship_kind);

        let rocket_enabled = self.ship.is_using_rocket(current_tick);
        let engine_shutdown = self.ship.is_engine_shutdown(current_tick);

        let afterburners_cost = ship_settings.afterburner_energy as u32 * 1000;

        let afterburners_enabled = input_state.is_down(InputAction::Afterburner)
            && self.ship.current_energy > afterburners_cost;

        if me.attach_parent == PlayerId::invalid() {
            let mut thrust = if afterburners_enabled {
                ship_settings.maximum_thrust as u32
            } else {
                self.ship.thrust
            };

            // TODO: Turret penalty

            if engine_shutdown {
                thrust = 0;
            }

            if rocket_enabled {
                thrust = settings.rocket_thrust as u32;
            }

            if me.flag_count > 0 || (me.carrying_ball && settings.powerball_flag_upgrades) {
                let new_thrust = thrust as i64 + settings.flagger_thrust_adjustment as i64;

                if new_thrust < 0 {
                    thrust = 0;
                } else {
                    thrust = new_thrust as u32;
                }
            }

            if rocket_enabled || input_state.is_down(InputAction::MoveForward) {
                let heading = me.get_heading();

                me.velocity.x.0 = me.velocity.x.0 + (heading.x * thrust as f32) as i32;
                me.velocity.y.0 = me.velocity.y.0 + (heading.y * thrust as f32) as i32;
            }

            if input_state.is_down(InputAction::MoveBackward) {
                let heading = me.get_heading();

                me.velocity.x.0 = me.velocity.x.0 - (heading.x * thrust as f32) as i32;
                me.velocity.y.0 = me.velocity.y.0 - (heading.y * thrust as f32) as i32;
            }
        } else {
            // TODO: Sync with parent
        }

        let mut speed = if afterburners_enabled {
            ship_settings.maximum_speed as u32
        } else {
            self.ship.speed
        };

        if rocket_enabled {
            speed = settings.rocket_speed as u32;

            // If rocket speed is less than our actual ship speed, we should use our ship speed.
            if speed < self.ship.speed {
                speed = self.ship.speed;
            }
        }

        // TODO: Turret penalty

        if self.ship.repel_effect_remaining_ticks > 0 {
            speed = settings.repel_speed as u32;
        }

        if me.flag_count > 0 || (me.carrying_ball && settings.powerball_flag_upgrades) {
            let new_speed = speed as i64 + settings.flagger_speed_adjustment as i64;

            if new_speed < 0 {
                speed = 0;
            } else {
                speed = new_speed as u32;
            }
        }

        me.velocity.truncate(speed as i32);

        if afterburners_enabled {
            self.ship.current_energy -= afterburners_cost;
        }
    }

    fn perform_rotation(&mut self, input_state: &InputState, player_manager: &mut PlayerManager) {
        // Orientation is stored as direction * 1000 so partial directions can be stored.
        const MAX_ORIENTATION: i32 = 40 * 1000;

        if input_state.is_down(InputAction::MoveLeft) {
            self.ship.current_orientation -= self.ship.rotation as i32;
        }

        if input_state.is_down(InputAction::MoveRight) {
            self.ship.current_orientation += self.ship.rotation as i32;
        }

        if self.ship.current_orientation < 0 {
            self.ship.current_orientation += MAX_ORIENTATION;
        }

        if self.ship.current_orientation >= MAX_ORIENTATION {
            self.ship.current_orientation -= MAX_ORIENTATION;
        }

        if let Some(me) = player_manager.get_self_mut() {
            me.direction = (self.ship.current_orientation / 1000) as u8 % 40;
        }
    }

    pub fn reset_ship(
        &mut self,
        settings: &ArenaSettings,
        current_tick: GameTick,
        ship_kind: ShipKind,
    ) {
        self.ship.reset(settings, current_tick, ship_kind);
    }
}
