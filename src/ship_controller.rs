use crate::{
    arena_settings::ArenaSettings,
    clock::GameTick,
    input::{InputAction, InputState},
    map::{Map, TILE_ID_SAFE},
    net::connection::Connection,
    player::{PlayerId, PlayerManager, StatusFlags},
    rng::VieRng,
    ship::{Ship, ShipCapabilityFlag, ShipKind},
    spawn::generate_spawn_position,
    weapon::{BombWeapon, BulletWeapon, BurstWeapon, DecoyWeapon, WeaponKind},
};

pub struct ShipController {
    pub ship: Ship,
}

impl ShipController {
    const REPEL_DELAY_TICKS: i32 = 50;

    pub fn new() -> Self {
        Self { ship: Ship::new() }
    }

    pub fn tick(
        &mut self,
        input_state: &InputState,
        connection: &mut Connection,
        player_manager: &mut PlayerManager,
        map: &Map,
        settings: &ArenaSettings,
        current_tick: GameTick,
    ) {
        let me = player_manager
            .get_self_mut()
            .expect("Ship controller player must exist");
        let ship_settings = settings.get_ship_settings(me.ship_kind);

        self.tick_effects(current_tick);
        self.perform_rotation(input_state, player_manager);
        let afterburners_enabled =
            self.perform_acceleration(input_state, player_manager, settings, current_tick);

        self.ship.current_energy =
            (self.ship.current_energy + self.ship.recharge).min(self.ship.max_energy);

        self.tick_status(StatusFlags::XRadar, ship_settings.xradar_energy as u32);
        self.tick_status(StatusFlags::Stealth, ship_settings.stealth_energy as u32);
        self.tick_status(StatusFlags::Cloak, ship_settings.cloak_energy as u32);
        self.tick_status(StatusFlags::Antiwarp, ship_settings.antiwarp_energy as u32);

        if input_state.is_triggered(InputAction::Multifire)
            && self.ship.capability & ShipCapabilityFlag::Multifire != 0
        {
            self.ship.multifire = !self.ship.multifire;
        }

        self.fire_weapons(
            input_state,
            connection,
            player_manager,
            map,
            settings,
            current_tick,
            afterburners_enabled,
        );

        let me = player_manager
            .get_self_mut()
            .expect("Ship controller player must exist");
        me.status = self.ship.status;
        me.bounty = self.ship.bounty;
    }

    fn tick_effects(&mut self, current_tick: GameTick) {
        if self.ship.repel_effect_remaining_ticks > 0 {
            self.ship.repel_effect_remaining_ticks -= 1;
        }

        if self.ship.emped_remaining_ticks > 0 {
            self.ship.emped_remaining_ticks -= 1;
        }

        if self.ship.super_remaining_ticks > 0 {
            self.ship.super_remaining_ticks -= 1;
        }

        if self.ship.shield_remaining_ticks > 0 {
            self.ship.shield_remaining_ticks -= 1;
        }

        if self.ship.portal_remaining_ticks > 0 {
            self.ship.portal_remaining_ticks -= 1;
        }

        if self.ship.flag_remaining_ticks > 0 {
            self.ship.flag_remaining_ticks -= 1;
        }

        if let Some(fake_antiwarp_end_tick) = self.ship.fake_antiwarp_end_tick {
            if current_tick >= fake_antiwarp_end_tick {
                self.ship.fake_antiwarp_end_tick = None;
            }
        }

        if let Some(rocket_end_tick) = self.ship.rocket_end_tick {
            if current_tick >= rocket_end_tick {
                self.ship.rocket_end_tick = None;
            }
        }

        if let Some(shutdown_end_tick) = self.ship.shutdown_end_tick {
            if current_tick >= shutdown_end_tick {
                self.ship.shutdown_end_tick = None;
            }
        }
    }

    fn tick_status(&mut self, status_flag: u8, cost: u32) {
        if self.ship.status & status_flag != 0 {
            if self.ship.current_energy > cost {
                self.ship.current_energy -= cost;
            } else {
                self.ship.status &= !status_flag;
            }
        }
    }

    fn perform_acceleration(
        &mut self,
        input_state: &InputState,
        player_manager: &mut PlayerManager,
        settings: &ArenaSettings,
        current_tick: GameTick,
    ) -> bool {
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

        afterburners_enabled
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

    pub fn fire_weapons(
        &mut self,
        input_state: &InputState,
        connection: &mut Connection,
        player_manager: &mut PlayerManager,
        map: &Map,
        settings: &ArenaSettings,
        current_tick: GameTick,
        afterburners_enabled: bool,
    ) {
        self.ship.weapon = None;

        let me = player_manager
            .get_self_mut()
            .expect("Ship controller player must exist");
        let ship_settings = settings.get_ship_settings(me.ship_kind);

        let Some(me_position) = me.position else {
            return;
        };

        let in_safe = map.get_tile_from_position(&me_position) == TILE_ID_SAFE;
        let can_fast_shoot = !afterburners_enabled || !ship_settings.disable_fast_shooting;

        let flagger_settings =
            me.flag_count > 0 || (me.carrying_ball && settings.powerball_flag_upgrades);

        let bomb_fire_delay = if flagger_settings {
            ship_settings.bomb_fire_delay as i32 + settings.flagger_bomb_fire_delay as i32
        } else {
            ship_settings.bomb_fire_delay as i32
        }
        .max(0);

        let mut weapon_kind = WeaponKind::None;

        if input_state.is_triggered(InputAction::Repel)
            && current_tick > self.ship.next_repel_tick
            && self.ship.repels > 0
        {
            weapon_kind = WeaponKind::Repel;

            if !in_safe {
                self.ship.repels -= 1;
            }

            self.ship.next_bomb_tick = current_tick + bomb_fire_delay;
            self.ship.next_bullet_tick = current_tick + bomb_fire_delay;
            self.ship.next_repel_tick = current_tick + Self::REPEL_DELAY_TICKS;
        }

        if input_state.is_triggered(InputAction::Burst)
            && current_tick > self.ship.next_bomb_tick
            && self.ship.bursts > 0
        {
            weapon_kind = WeaponKind::Burst(BurstWeapon { active: false });

            if !in_safe {
                self.ship.bursts -= 1;
            }

            self.ship.next_bomb_tick = current_tick + bomb_fire_delay;
            self.ship.next_bullet_tick = current_tick + bomb_fire_delay;
            self.ship.next_repel_tick = current_tick + Self::REPEL_DELAY_TICKS;
        }

        if input_state.is_triggered(InputAction::Thor)
            && current_tick > self.ship.next_bomb_tick
            && self.ship.thors > 0
            && can_fast_shoot
        {
            weapon_kind = WeaponKind::Thor(BombWeapon {
                level: 0,
                shrapnel_count: 0,
                shrapnel_level: 0,
                shrapnel_bouncing: false,
                mine: false,
                emp: false,
                remaining_bounces: 0,
                rng_seed: 0,
                active_prox: None,
            });

            if !in_safe {
                self.ship.thors -= 1;
            }

            self.ship.next_bomb_tick = current_tick + bomb_fire_delay;
            self.ship.next_bullet_tick = current_tick + bomb_fire_delay;
            self.ship.next_repel_tick = current_tick + Self::REPEL_DELAY_TICKS;
        }

        if input_state.is_triggered(InputAction::Decoy)
            && current_tick > self.ship.next_bomb_tick
            && self.ship.decoys > 0
        {
            weapon_kind = WeaponKind::Decoy(DecoyWeapon {
                initial_rotation: 0,
            });

            if !in_safe {
                self.ship.decoys -= 1;
            }

            self.ship.next_bomb_tick = current_tick + bomb_fire_delay;
            self.ship.next_bullet_tick = current_tick + bomb_fire_delay;
            self.ship.next_repel_tick = current_tick + Self::REPEL_DELAY_TICKS;
        }

        if input_state.is_triggered(InputAction::Brick)
            && current_tick > self.ship.next_bomb_tick
            && self.ship.bricks > 0
            && !in_safe
        {
            self.ship.bricks -= 1;

            let brick = crate::net::packet::c2s::DropBrickMessage {
                x: (me_position.x.0 / 16000) as u16,
                y: (me_position.y.0 / 16000) as u16,
            };

            if let Err(e) = connection.send_reliable(&brick) {
                log::error!("{e}");
            }

            self.ship.next_bomb_tick = current_tick + bomb_fire_delay;
            self.ship.next_bullet_tick = current_tick + bomb_fire_delay;
        }

        if input_state.is_triggered(InputAction::Rocket)
            && current_tick > self.ship.next_bomb_tick
            && self.ship.rocket_end_tick.is_none()
            && self.ship.rockets > 0
        {
            self.ship.rockets -= 1;

            self.ship.rocket_end_tick = Some(current_tick + ship_settings.rocket_time as i32);
        }

        if input_state.is_triggered(InputAction::Portal) && self.ship.portals > 0 {
            // TODO: Check antiwarp

            self.ship.portal_position = Some(me_position);
            self.ship.portals -= 1;
            self.ship.portal_remaining_ticks = settings.warp_point_delay as u32;
        }

        if input_state.is_triggered(InputAction::Warp) {
            // TODO: Check antiwarp

            let (new_position, use_energy) =
                if let Some(portal_position) = self.ship.portal_position {
                    (portal_position, false)
                } else {
                    let rng = VieRng::new(current_tick.value() as i32);

                    let new_position = generate_spawn_position(
                        settings,
                        map,
                        me.ship_kind,
                        me.frequency,
                        rng,
                        player_manager.players.len(),
                    );

                    (new_position, true)
                };

            if let Some(me) = player_manager.get_self_mut() {
                if !use_energy || self.ship.is_max_energy() {
                    me.position = Some(new_position);

                    self.ship.status |= StatusFlags::Flash;
                    self.ship.next_bomb_tick = current_tick + Self::REPEL_DELAY_TICKS;
                }

                if use_energy {
                    self.ship.current_energy = 1000;
                    me.velocity.clear();
                    self.ship.fake_antiwarp_end_tick =
                        Some(current_tick + settings.antiwarp_settle_delay as i32);
                }
            }
        }

        let mut energy_cost = 0;

        if input_state.is_down(InputAction::Bullet)
            && current_tick > self.ship.next_bullet_tick
            && self.ship.guns > 0
            && can_fast_shoot
        {
            let multifire =
                self.ship.multifire && self.ship.capability & ShipCapabilityFlag::Multifire != 0;
            let mut level = self.ship.guns - 1;

            if flagger_settings && settings.flagger_gun_upgrade {
                level += 1;
            }

            if self.ship.capability & ShipCapabilityFlag::BouncingBullets != 0 {
                weapon_kind = WeaponKind::BouncingBullet(BulletWeapon {
                    level,
                    multi: multifire,
                    link_id: None,
                });
            } else {
                weapon_kind = WeaponKind::Bullet(BulletWeapon {
                    level,
                    multi: multifire,
                    link_id: None,
                });
            }

            let delay = if multifire {
                energy_cost = ship_settings.multi_fire_energy as i32 * self.ship.guns as i32;
                ship_settings.multi_fire_delay
            } else {
                energy_cost = ship_settings.bullet_fire_energy as i32 * self.ship.guns as i32;
                ship_settings.bullet_fire_delay
            } as i32;

            if flagger_settings {
                energy_cost = energy_cost * (settings.flagger_fire_cost_percent / 1000) as i32
            }

            if self.ship.current_energy >= energy_cost as u32 {
                self.ship.next_bullet_tick = current_tick + delay;
                self.ship.next_bomb_tick = self.ship.next_bullet_tick;
                self.ship.next_repel_tick = current_tick + Self::REPEL_DELAY_TICKS;
            } else {
                weapon_kind = WeaponKind::None;
            }
        }

        if input_state.is_down(InputAction::Mine)
            && current_tick > self.ship.next_bomb_tick
            && self.ship.bombs > 0
        {
            let mut level = self.ship.bombs - 1;

            if flagger_settings && settings.flagger_gun_upgrade {
                level += 1;
            }

            let mut bomb_weapon = BombWeapon {
                level,
                shrapnel_count: 0,
                shrapnel_level: 0,
                shrapnel_bouncing: false,
                mine: true,
                emp: false,
                remaining_bounces: 0,
                rng_seed: 0,
                active_prox: None,
            };

            if self.ship.guns > 0 {
                bomb_weapon.shrapnel_count = self.ship.shrapnel;
                bomb_weapon.shrapnel_level = self.ship.guns - 1;
                bomb_weapon.shrapnel_bouncing =
                    self.ship.capability & ShipCapabilityFlag::BouncingBullets != 0;
            }

            if self.ship.capability & ShipCapabilityFlag::Proximity != 0 {
                weapon_kind = WeaponKind::ProximityBomb(bomb_weapon);
            } else {
                weapon_kind = WeaponKind::Bomb(bomb_weapon);
            }

            energy_cost = ship_settings.mine_fire_energy as i32
                + ship_settings.mine_fire_energy_upgrade as i32 * level as i32;

            if flagger_settings {
                energy_cost = energy_cost * (settings.flagger_fire_cost_percent / 1000) as i32
            }

            if self.ship.current_energy >= energy_cost as u32 {
                self.ship.next_bullet_tick = current_tick + ship_settings.mine_fire_delay as i32;
                self.ship.next_bomb_tick = current_tick + ship_settings.mine_fire_delay as i32;
                self.ship.next_repel_tick = current_tick + Self::REPEL_DELAY_TICKS;
            } else {
                weapon_kind = WeaponKind::None;
            }
        }

        if input_state.is_down(InputAction::Bomb)
            && current_tick > self.ship.next_bomb_tick
            && self.ship.bombs > 0
        {
            let mut level = self.ship.bombs - 1;

            if flagger_settings && settings.flagger_gun_upgrade {
                level += 1;
            }

            let mut bomb_weapon = BombWeapon {
                level,
                shrapnel_count: 0,
                shrapnel_level: 0,
                shrapnel_bouncing: false,
                mine: false,
                emp: false,
                remaining_bounces: 0,
                rng_seed: 0,
                active_prox: None,
            };

            if self.ship.guns > 0 {
                bomb_weapon.shrapnel_count = self.ship.shrapnel;
                bomb_weapon.shrapnel_level = self.ship.guns - 1;
                bomb_weapon.shrapnel_bouncing =
                    self.ship.capability & ShipCapabilityFlag::BouncingBullets != 0;
            }

            if self.ship.capability & ShipCapabilityFlag::Proximity != 0 {
                weapon_kind = WeaponKind::ProximityBomb(bomb_weapon);
            } else {
                weapon_kind = WeaponKind::Bomb(bomb_weapon);
            }

            energy_cost = ship_settings.bomb_fire_energy as i32
                + ship_settings.bomb_fire_energy_upgrade as i32 * level as i32;

            if flagger_settings {
                energy_cost = energy_cost * (settings.flagger_fire_cost_percent / 1000) as i32
            }

            if self.ship.current_energy >= energy_cost as u32 {
                self.ship.next_bullet_tick = current_tick + bomb_fire_delay;
                self.ship.next_bomb_tick = current_tick + bomb_fire_delay;
                self.ship.next_repel_tick = current_tick + Self::REPEL_DELAY_TICKS;
            } else {
                weapon_kind = WeaponKind::None;
            }
        }

        if let WeaponKind::None = weapon_kind {
            return;
        }

        if self.ship.current_energy < energy_cost as u32 {
            return;
        }

        self.ship.weapon = Some(weapon_kind);
    }
}
