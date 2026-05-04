use smol_str::format_smolstr;

use crate::{
    arena_settings::ArenaSettings,
    clock::GameTick,
    input::{InputAction, InputState},
    map::{Map, TILE_ID_SAFE},
    net::connection::Connection,
    player::{PlayerManager, StatusFlags},
    powerball::PowerballManager,
    radar::Radar,
    render::{
        animation_renderer::get_animation_index,
        game_sprites::{GameSpriteKind, GameSprites, SpriteSet},
        layer::Layer,
        render_state::RenderState,
        sprite_renderer::SpriteRenderable,
        text_renderer::{TextAlignment, TextColor},
    },
    rng::VieRng,
    ship::{Ship, ShipCapabilityFlag, ShipKind},
    simulation::{
        game_simulation::WeaponExplosionEvent, player_simulation::PLAYER_EXPLOSION_DURATION,
    },
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
        powerball_manager: &mut PowerballManager,
        map: &Map,
        radar: &Radar,
        settings: &ArenaSettings,
        current_tick: GameTick,
        render_state: &mut Option<&mut RenderState>,
    ) {
        let player_count = player_manager.players.len();

        let me = player_manager
            .get_self_mut()
            .expect("Ship controller player must exist");

        if me.enter_delay > 0 {
            // Clear velocity after explosion.
            if me.enter_delay < settings.enter_delay as u16 {
                me.velocity.clear();
            }
            return;
        }

        if me.position.is_none() {
            let rng = VieRng::new(current_tick.value() as i32);

            me.position = Some(generate_spawn_position(
                settings,
                map,
                me.ship_kind,
                me.frequency,
                rng,
                player_count,
            ));

            me.status |= StatusFlags::Flash;
            me.velocity.clear();

            self.reset_ship(settings, current_tick, me.ship_kind);
            return;
        }

        let ship_settings = settings.get_ship_settings(me.ship_kind);

        self.tick_effects();
        self.perform_rotation(input_state, player_manager);
        let afterburners_enabled = self.perform_acceleration(input_state, player_manager, settings);

        if self.ship.emped_remaining_ticks == 0 {
            self.ship.current_energy =
                (self.ship.current_energy + self.ship.recharge).min(self.ship.max_energy);
        }

        if input_state.is_triggered(InputAction::Multifire)
            && self.ship.capability & ShipCapabilityFlag::Multifire != 0
        {
            self.ship.multifire = !self.ship.multifire;
        }

        if input_state.is_triggered(InputAction::Stealth)
            && self.ship.capability & ShipCapabilityFlag::Stealth != 0
        {
            self.ship.status ^= StatusFlags::Stealth;
        }

        if input_state.is_triggered(InputAction::Cloak)
            && self.ship.capability & ShipCapabilityFlag::Cloak != 0
        {
            self.ship.status ^= StatusFlags::Cloak;
        }

        if input_state.is_triggered(InputAction::XRadar)
            && self.ship.capability & ShipCapabilityFlag::XRadar != 0
        {
            self.ship.status ^= StatusFlags::XRadar;
        }

        if input_state.is_triggered(InputAction::Antiwarp)
            && self.ship.capability & ShipCapabilityFlag::Antiwarp != 0
        {
            self.ship.status ^= StatusFlags::Antiwarp;
        }

        self.tick_status(StatusFlags::XRadar, ship_settings.xradar_energy as u32);
        self.tick_status(StatusFlags::Stealth, ship_settings.stealth_energy as u32);
        self.tick_status(StatusFlags::Cloak, ship_settings.cloak_energy as u32);
        self.tick_status(StatusFlags::Antiwarp, ship_settings.antiwarp_energy as u32);

        self.fire_weapons(
            input_state,
            connection,
            player_manager,
            powerball_manager,
            map,
            radar,
            settings,
            current_tick,
            afterburners_enabled,
        );

        let me = player_manager
            .get_self_mut()
            .expect("Ship controller player must exist");
        me.status = self.ship.status;
        me.bounty = self.ship.bounty;

        if self.ship.emped_remaining_ticks > 0 {
            Self::render_emp_trail(player_manager, render_state, current_tick);
        }
    }

    fn tick_effects(&mut self) {
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

            if self.ship.portal_remaining_ticks == 0 {
                self.ship.portal_position = None;
            }
        }

        if self.ship.flag_remaining_ticks > 0 {
            self.ship.flag_remaining_ticks -= 1;
        }

        if self.ship.fake_antiwarp_remaining_ticks > 0 {
            self.ship.fake_antiwarp_remaining_ticks -= 1;
        }

        if self.ship.rocket_remaining_ticks > 0 {
            self.ship.rocket_remaining_ticks -= 1;
        }

        if self.ship.shutdown_remaining_ticks > 0 {
            self.ship.shutdown_remaining_ticks -= 1;
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
    ) -> bool {
        let me = player_manager
            .get_self_mut()
            .expect("Ship controller player must exist");
        let ship_settings = settings.get_ship_settings(me.ship_kind);

        let rocket_enabled = self.ship.rocket_remaining_ticks > 0;
        let engine_shutdown = self.ship.shutdown_remaining_ticks > 0;

        let afterburners_cost = ship_settings.afterburner_energy as u32;

        let afterburners_enabled = input_state.is_down(InputAction::Afterburner)
            && self.ship.current_energy > afterburners_cost;

        if !me.attach_parent.valid() {
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
            me.direction = self.ship.get_direction();
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

    pub fn is_antiwarped(
        &self,
        player_manager: &PlayerManager,
        radar: &Radar,
        antiwarp_pixels: u32,
    ) -> bool {
        let antiwarp_pixels = antiwarp_pixels as i32;

        if self.ship.fake_antiwarp_remaining_ticks > 0 {
            return true;
        }

        let Some(me) = player_manager.get_self() else {
            return false;
        };

        let Some(me_position) = me.position else {
            return false;
        };

        for player in &player_manager.players {
            if player.ship_kind == ShipKind::Spectator {
                continue;
            }

            if player.frequency == me.frequency {
                continue;
            }

            if player.enter_delay > 0 {
                continue;
            }

            if player.status & StatusFlags::Antiwarp == 0 {
                continue;
            }

            let Some(player_position) = player.position else {
                continue;
            };

            if !radar.in_view(player_position) {
                continue;
            }

            let (dx, dy) = me_position.delta_pixels(&player_position);

            if dx.abs() < antiwarp_pixels || dy.abs() < antiwarp_pixels {
                return true;
            }
        }

        false
    }

    pub fn fire_weapons(
        &mut self,
        input_state: &InputState,
        connection: &mut Connection,
        player_manager: &mut PlayerManager,
        powerball_manager: &mut PowerballManager,
        map: &Map,
        radar: &Radar,
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

        let me_ship_kind = me.ship_kind;
        let me_frequency = me.frequency;

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
                // NOTE: Self decoys and other Continuum clients do not sync decoy rotations.
                // No need to try to fix this since two Continuum clients do not have the same rotation.
                initial_rotation: me.direction,
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
            && self.ship.rocket_remaining_ticks == 0
            && self.ship.rockets > 0
        {
            self.ship.rockets -= 1;

            self.ship.rocket_remaining_ticks = ship_settings.rocket_time as u32;
        }

        if input_state.is_triggered(InputAction::Portal) && self.ship.portals > 0 {
            if self.is_antiwarped(player_manager, radar, settings.antiwarp_pixels as u32) {
                // TODO: Notification
            } else {
                self.ship.portal_position = Some(me_position);
                self.ship.portals -= 1;
                self.ship.portal_remaining_ticks = settings.warp_point_delay as u32;
            }
        }

        if input_state.is_triggered(InputAction::Warp) {
            if let Some(carry) = &powerball_manager.carry_state {
                if self.shoot_powerball(
                    player_manager,
                    connection,
                    settings,
                    carry.ball_id as u8,
                    current_tick,
                ) {
                    powerball_manager.clear_carry_state();

                    self.ship.next_bomb_tick = current_tick + 60;
                    self.ship.next_bullet_tick = current_tick + 60;
                    return;
                }
            }

            if self.is_antiwarped(player_manager, radar, settings.antiwarp_pixels as u32) {
                // TODO: Notification
            } else {
                let (new_position, use_energy) =
                    if let Some(portal_position) = self.ship.portal_position {
                        self.ship.portal_position = None;
                        self.ship.portal_remaining_ticks = 0;

                        (portal_position, false)
                    } else {
                        let rng = VieRng::new(current_tick.value() as i32);

                        let new_position = generate_spawn_position(
                            settings,
                            map,
                            me_ship_kind,
                            me_frequency,
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
                        self.ship.fake_antiwarp_remaining_ticks =
                            settings.antiwarp_settle_delay as u32;
                    }
                }
            }
        }

        let mut energy_cost = 0;
        let has_super = self.ship.super_remaining_ticks > 0;

        if input_state.is_down(InputAction::Bullet)
            && current_tick > self.ship.next_bullet_tick
            && self.ship.guns > 0
            && can_fast_shoot
        {
            if !settings.powerball_gun_allowed {
                if let Some(carry) = &powerball_manager.carry_state {
                    if self.shoot_powerball(
                        player_manager,
                        connection,
                        settings,
                        carry.ball_id as u8,
                        current_tick,
                    ) {
                        powerball_manager.clear_carry_state();

                        self.ship.next_bomb_tick = current_tick + 60;
                        self.ship.next_bullet_tick = current_tick + 60;
                        return;
                    }
                }
            }

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

            if has_super || self.ship.current_energy >= energy_cost as u32 {
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
            if !settings.powerball_bomb_allowed {
                if let Some(carry) = &powerball_manager.carry_state {
                    if self.shoot_powerball(
                        player_manager,
                        connection,
                        settings,
                        carry.ball_id as u8,
                        current_tick,
                    ) {
                        powerball_manager.clear_carry_state();

                        self.ship.next_bomb_tick = current_tick + 60;
                        self.ship.next_bullet_tick = current_tick + 60;
                        return;
                    }
                }
            }

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
                emp: ship_settings.emp_bomb,
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

            if let Some(me) = player_manager.get_self() {
                bomb_weapon.initialize_rng_seed(
                    me_position,
                    me.velocity,
                    me.get_heading(),
                    0,
                    me.frequency,
                );
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

            if has_super || self.ship.current_energy >= energy_cost as u32 {
                let delay = (ship_settings.mine_fire_delay as i32).max(bomb_fire_delay);

                self.ship.next_bullet_tick = current_tick + delay as i32;
                self.ship.next_bomb_tick = current_tick + delay as i32;
                self.ship.next_repel_tick = current_tick + Self::REPEL_DELAY_TICKS;
            } else {
                weapon_kind = WeaponKind::None;
            }
        }

        if input_state.is_down(InputAction::Bomb)
            && current_tick > self.ship.next_bomb_tick
            && self.ship.bombs > 0
        {
            if !settings.powerball_bomb_allowed {
                if let Some(carry) = &powerball_manager.carry_state {
                    if self.shoot_powerball(
                        player_manager,
                        connection,
                        settings,
                        carry.ball_id as u8,
                        current_tick,
                    ) {
                        powerball_manager.clear_carry_state();

                        self.ship.next_bomb_tick = current_tick + 60;
                        self.ship.next_bullet_tick = current_tick + 60;
                        return;
                    }
                }
            }

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
                emp: ship_settings.emp_bomb,
                remaining_bounces: ship_settings.bomb_bounce_count as u32,
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

            if has_super || self.ship.current_energy >= energy_cost as u32 {
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

        if in_safe {
            let me = player_manager
                .get_self_mut()
                .expect("Ship controller player must exist");
            me.velocity.clear();
            return;
        }

        energy_cost *= 1000;

        if has_super {
            energy_cost = 0;
        }

        if self.ship.current_energy < energy_cost as u32 {
            return;
        }

        match &mut weapon_kind {
            WeaponKind::Bomb(bomb) | WeaponKind::ProximityBomb(bomb) | WeaponKind::Thor(bomb) => {
                if !bomb.mine {
                    if let Some(me) = player_manager.get_self() {
                        bomb.initialize_rng_seed(
                            me_position,
                            me.velocity,
                            me.get_heading(),
                            ship_settings.bomb_speed as u32,
                            me.frequency,
                        );
                    }
                }
            }
            _ => {}
        }

        self.ship.current_energy -= energy_cost as u32;
        self.ship.weapon = Some(weapon_kind);
    }

    fn shoot_powerball(
        &mut self,
        player_manager: &PlayerManager,
        connection: &mut Connection,
        settings: &ArenaSettings,
        ball_id: u8,
        current_tick: GameTick,
    ) -> bool {
        let Some(me) = player_manager.get_self() else {
            return false;
        };

        let Some(me_position) = me.position else {
            return false;
        };

        let speed = if me.ship_kind != ShipKind::Spectator {
            settings.get_ship_settings(me.ship_kind).powerball_speed
        } else {
            return false;
        };

        let mut velocity = me.velocity;
        let forward = me.get_heading() * speed as f32;

        velocity.x.0 += forward.x as i32;
        velocity.y.0 += forward.y as i32;

        let message = crate::net::packet::c2s::PowerballFireMessage {
            ball_id,
            x: (me_position.x.0 / 1000) as u16,
            y: (me_position.y.0 / 1000) as u16,
            x_velocity: velocity.x.0 as i16,
            y_velocity: velocity.y.0 as i16,
            player_id: me.id,
            timestamp: current_tick,
        };

        if let Err(e) = connection.send_reliable(&message) {
            log::error!("{e}");
        }

        true
    }

    pub fn apply_damage(
        &mut self,
        player_manager: &mut PlayerManager,
        connection: &mut Connection,
        settings: &ArenaSettings,
        event: &WeaponExplosionEvent,
    ) {
        if self.ship.current_energy == 0 {
            return;
        }

        let hit_me = if let Some(hit_player_id) = event.hit_player {
            hit_player_id == player_manager.self_id
        } else {
            false
        };

        let Some(me) = player_manager.get_self() else {
            return;
        };

        let Some(me_position) = me.position else {
            return;
        };

        if me.status & StatusFlags::Safety != 0 {
            return;
        }

        let mut damage = match &event.kind {
            WeaponKind::Bullet(bullet) | WeaponKind::BouncingBullet(bullet) => {
                if hit_me {
                    settings.bullet_damage_level
                        + settings.bullet_damage_upgrade * bullet.level as i32
                } else {
                    0
                }
            }
            WeaponKind::Shrapnel(shrapnel) => {
                if hit_me {
                    let alive_time = settings.bullet_alive_time - event.remaining_ticks as i32;
                    if alive_time <= 25 {
                        settings.inactive_shrap_damage * shrapnel.level as i32
                    } else {
                        let damage = settings.bullet_damage_level
                            + settings.bullet_damage_upgrade * shrapnel.level as i32;

                        ((damage as i64 * settings.shrapnel_damage_percent as i64) / 1000) as i32
                    }
                } else {
                    0
                }
            }
            WeaponKind::Bomb(bomb) | WeaponKind::ProximityBomb(bomb) | WeaponKind::Thor(bomb) => {
                let mut full_damage = settings.bomb_damage_level;
                let level = if let WeaponKind::Thor(_) = event.kind {
                    3
                } else {
                    bomb.level
                };

                if bomb.emp {
                    full_damage =
                        ((full_damage as i64 * settings.ebomb_damage_percent as i64) / 1000) as i32;
                }

                if bomb.remaining_bounces > 0 {
                    full_damage = ((full_damage as i64
                        * settings.bouncing_bomb_damage_percent as i64)
                        / 1000) as i32;
                }

                let explode_pixels = settings.bomb_explode_pixels as i32
                    + settings.bomb_explode_pixels as i32 * level as i32;

                let distance = me_position.max_axis_distance(&event.position);

                if distance < explode_pixels {
                    let mut damage = (explode_pixels - distance) * (full_damage / explode_pixels);

                    if event.shooter != me.id {
                        if let Some(shooter) = player_manager.get_by_id(event.shooter) {
                            if let Some(shooter_position) = shooter.position {
                                let shooter_distance =
                                    shooter_position.max_axis_distance(&event.position);

                                if shooter_distance < explode_pixels {
                                    let damage_reduction = (explode_pixels - shooter_distance)
                                        * (full_damage / explode_pixels)
                                        / 2;

                                    damage -= damage_reduction;
                                    if damage < 0 {
                                        damage = 0;
                                    }
                                }
                            }
                        }
                    }

                    if damage > 0 && bomb.emp && event.shooter != me.id {
                        let emp_ticks = (settings.ebomb_shutdown_time as i64 * damage as i64)
                            / full_damage as i64;

                        if emp_ticks > 0 {
                            self.ship.emped_remaining_ticks = emp_ticks as u32;
                        }
                    }

                    damage as i32
                } else {
                    0
                }
            }
            WeaponKind::Burst(_) => settings.burst_damage_level,
            _ => 0,
        };

        if self.ship.shield_remaining_ticks > 0 {
            let reduction = (damage as i64 * self.ship.shield_remaining_ticks as i64)
                / settings.get_ship_settings(me.ship_kind).shield_time as i64;
            damage -= reduction as i32;
        }

        if !settings.exact_damage && damage > 0 {
            match &event.kind {
                WeaponKind::Bullet(_) | WeaponKind::BouncingBullet(_) | WeaponKind::Burst(_) => {
                    let mut rng = VieRng::new(GameTick::now(0).value() as i32);

                    let r = rng.next() % (damage as u32 / 1000 * damage as u32 / 1000 + 1);

                    damage = f32::sqrt(r as f32) as i32 * 1000;
                }
                _ => {}
            }
        }

        if me.flag_count > 0 || (me.carrying_ball && settings.powerball_flag_upgrades) {
            damage = ((damage as i64 * settings.flagger_damage_percent as i64) / 1000) as i32;
        }

        if damage > 0 {
            // TODO: Watchdamage sending

            let apply_damage = if damage as u32 > self.ship.current_energy {
                match &event.kind {
                    WeaponKind::Bomb(_) | WeaponKind::ProximityBomb(_) | WeaponKind::Thor(_) => {
                        if event.shooter == me.id {
                            self.ship.current_energy = 1000;
                            false
                        } else {
                            true
                        }
                    }
                    _ => true,
                }
            } else {
                true
            };

            if apply_damage {
                self.ship.current_energy = self.ship.current_energy.saturating_sub(damage as u32);

                if self.ship.current_energy == 0 {
                    if let Some(me) = player_manager.get_self_mut() {
                        me.enter_delay =
                            settings.enter_delay as u16 + PLAYER_EXPLOSION_DURATION as u16;

                        let death = crate::net::packet::c2s::DeathMessage {
                            killer_id: event.shooter,
                            bounty: me.bounty,
                        };

                        if let Err(e) = connection.send_reliable(&death) {
                            log::error!("{e}");
                        }
                    }
                }
            }
        }
    }

    pub fn render(
        &self,
        player_manager: &PlayerManager,
        render_state: &mut RenderState,
        sprites: &GameSprites,
        settings: &ArenaSettings,
        current_tick: GameTick,
    ) {
        self.render_energy(render_state, sprites);
        self.render_energybar(render_state, sprites, settings, current_tick);

        self.render_icons(render_state, sprites);

        self.render_attach_turret(player_manager, render_state, sprites);

        if let Some(portal_position) = self.ship.portal_position {
            let (x_pixels, y_pixels) = portal_position.to_pixels();

            if let Some(warp_point_sprites) = sprites.get_set(GameSpriteKind::WarpPoint) {
                let animation_index = get_animation_index(current_tick.value(), 10, 10 * 10);
                let renderable = &warp_point_sprites.renderables[animation_index];

                render_state.sprite_renderer.draw_centered(
                    &render_state.camera,
                    renderable,
                    x_pixels,
                    y_pixels,
                    Layer::AfterBackground,
                );

                let (ui_x, ui_y) = render_state
                    .get_hud_timer_position(crate::render::render_state::HudTimerKind::Portal);

                render_state.sprite_renderer.draw(
                    &render_state.ui_camera,
                    renderable,
                    ui_x as i32,
                    ui_y,
                    Layer::Gauges,
                );

                let remaining_time = self.ship.portal_remaining_ticks as f32 / 100.0f32;
                let text_y =
                    ui_y + renderable.size[1] as i32 - render_state.text_renderer.character_height;

                render_state.text_renderer.draw(
                    &mut render_state.sprite_renderer,
                    &render_state.ui_camera,
                    &format_smolstr!("{:.1}", remaining_time),
                    ui_x as i32,
                    text_y,
                    Layer::Gauges,
                    TextColor::Yellow,
                    TextAlignment::Right,
                );
            }
        }

        if self.ship.shield_remaining_ticks > 0 {
            if let Some(shield_sprites) = sprites.get_set(GameSpriteKind::Shield) {
                let animation_index = get_animation_index(current_tick.value(), 10, 10 * 10);
                let renderable = &shield_sprites.renderables[animation_index];

                let (ui_x, ui_y) = render_state
                    .get_hud_timer_position(crate::render::render_state::HudTimerKind::Shield);

                render_state.sprite_renderer.draw(
                    &render_state.ui_camera,
                    renderable,
                    ui_x as i32,
                    ui_y,
                    Layer::Gauges,
                );

                let shield_percent = (self.ship.shield_remaining_ticks as f32
                    / settings.get_ship_settings(self.ship.kind).shield_time as f32)
                    * 100.0f32;
                let text_y =
                    ui_y + renderable.size[1] as i32 - render_state.text_renderer.character_height;

                render_state.text_renderer.draw(
                    &mut render_state.sprite_renderer,
                    &render_state.ui_camera,
                    &format_smolstr!("{}%", shield_percent as i32),
                    ui_x as i32,
                    text_y,
                    Layer::Gauges,
                    TextColor::Yellow,
                    TextAlignment::Right,
                );
            }
        }

        if self.ship.super_remaining_ticks > 0 {
            if let Some(super_sprites) = sprites.get_set(GameSpriteKind::Super) {
                let animation_index = get_animation_index(current_tick.value(), 10, 10 * 10);
                let renderable = &super_sprites.renderables[animation_index];

                let (ui_x, ui_y) = render_state
                    .get_hud_timer_position(crate::render::render_state::HudTimerKind::Super);

                render_state.sprite_renderer.draw(
                    &render_state.ui_camera,
                    renderable,
                    ui_x as i32,
                    ui_y,
                    Layer::Gauges,
                );

                let remaining_time = self.ship.super_remaining_ticks as f32 / 100.0f32;
                let text_y =
                    ui_y + renderable.size[1] as i32 - render_state.text_renderer.character_height;

                render_state.text_renderer.draw(
                    &mut render_state.sprite_renderer,
                    &render_state.ui_camera,
                    &format_smolstr!("{:.1}", remaining_time),
                    ui_x as i32,
                    text_y,
                    Layer::Gauges,
                    TextColor::Yellow,
                    TextAlignment::Right,
                );
            }
        }
    }

    fn render_icons(&self, render_state: &mut RenderState, sprites: &GameSprites) {
        let y = (render_state.config.height / 2) as i32 - 26 * 2;

        if let Some(icon_sprites) = sprites.get_set(GameSpriteKind::Icons) {
            let icon_width = icon_sprites.renderables[0].size[0];
            let icon_height = icon_sprites.renderables[0].size[1] as i32;

            let right_side_x = (render_state.config.width - icon_width) as i32;

            if let Some(gun_index) = self.get_gun_renderable_index() {
                let renderable = &icon_sprites.renderables[gun_index];

                render_state.sprite_renderer.draw(
                    &render_state.ui_camera,
                    renderable,
                    right_side_x,
                    y,
                    Layer::Gauges,
                );
            }

            if let Some(bomb_index) = self.get_bomb_renderable_index() {
                let renderable = &icon_sprites.renderables[bomb_index];

                render_state.sprite_renderer.draw(
                    &render_state.ui_camera,
                    renderable,
                    right_side_x,
                    y + icon_height,
                    Layer::Gauges,
                );
            }

            self.render_status_indicator(
                render_state,
                icon_sprites,
                StatusFlags::Stealth,
                32,
                right_side_x,
                y + icon_height * 2,
            );
            self.render_status_indicator(
                render_state,
                icon_sprites,
                StatusFlags::Cloak,
                34,
                right_side_x,
                y + icon_height * 3,
            );
            self.render_status_indicator(
                render_state,
                icon_sprites,
                StatusFlags::XRadar,
                36,
                right_side_x,
                y + icon_height * 4,
            );
            self.render_status_indicator(
                render_state,
                icon_sprites,
                StatusFlags::Antiwarp,
                38,
                right_side_x,
                y + icon_height * 5,
            );

            self.render_item_indicator(
                render_state,
                sprites,
                &icon_sprites.renderables[30],
                self.ship.bursts,
                0,
                y,
            );

            self.render_item_indicator(
                render_state,
                sprites,
                &icon_sprites.renderables[31],
                self.ship.repels,
                0,
                y + icon_height,
            );

            self.render_item_indicator(
                render_state,
                sprites,
                &icon_sprites.renderables[40],
                self.ship.decoys,
                0,
                y + icon_height * 2,
            );

            self.render_item_indicator(
                render_state,
                sprites,
                &icon_sprites.renderables[41],
                self.ship.thors,
                0,
                y + icon_height * 3,
            );

            self.render_item_indicator(
                render_state,
                sprites,
                &icon_sprites.renderables[42],
                self.ship.bricks,
                0,
                y + icon_height * 4,
            );

            self.render_item_indicator(
                render_state,
                sprites,
                &icon_sprites.renderables[43],
                self.ship.rockets,
                0,
                y + icon_height * 5,
            );

            self.render_item_indicator(
                render_state,
                sprites,
                &icon_sprites.renderables[46],
                self.ship.portals,
                0,
                y + icon_height * 6,
            );
        }
    }

    fn render_item_indicator(
        &self,
        render_state: &mut RenderState,
        sprites: &GameSprites,
        renderable: &SpriteRenderable,
        count: u8,
        x: i32,
        y: i32,
    ) {
        let width = renderable.size[0] as i32;

        if count == 0 {
            render_state.sprite_renderer.draw(
                &render_state.ui_camera,
                renderable,
                x - (width - 4),
                y,
                Layer::Gauges,
            );
        } else {
            render_state.sprite_renderer.draw(
                &render_state.ui_camera,
                renderable,
                x,
                y,
                Layer::Gauges,
            );

            if count > 1 {
                if let Some(font_sprites) = sprites.get_set(GameSpriteKind::IconFont) {
                    let index = (count as usize).min(10);

                    let digit_renderable = &font_sprites.renderables[index];
                    render_state.sprite_renderer.draw(
                        &render_state.ui_camera,
                        digit_renderable,
                        x + width - 3,
                        y + 6,
                        Layer::Gauges,
                    );
                }
            }
        }
    }

    fn render_status_indicator(
        &self,
        render_state: &mut RenderState,
        icon_sprites: &SpriteSet,
        status: u8,
        on_index: usize,
        x: i32,
        y: i32,
    ) {
        let width = icon_sprites.renderables[0].size[0] as i32;

        let (index, x_offset) = if self.ship.status & status != 0 {
            (on_index, 0)
        } else if self.ship.capability & status != 0 {
            (on_index + 1, 0)
        } else {
            (on_index + 1, width - 4)
        };

        let renderable = &icon_sprites.renderables[index];

        render_state.sprite_renderer.draw(
            &render_state.ui_camera,
            renderable,
            x + x_offset,
            y,
            Layer::Gauges,
        );
    }

    fn get_gun_renderable_index(&self) -> Option<usize> {
        if self.ship.guns == 0 {
            return None;
        }

        let mut index = 0;

        if self.ship.capability & ShipCapabilityFlag::Multifire != 0 {
            if self.ship.multifire {
                index = 3;
            } else {
                index = 6;
            }
        }

        if self.ship.capability & ShipCapabilityFlag::BouncingBullets != 0 {
            index += 9;
        }

        Some(index + self.ship.guns as usize - 1)
    }

    fn get_bomb_renderable_index(&self) -> Option<usize> {
        if self.ship.bombs == 0 {
            return None;
        }

        let mut index = 18;

        if self.ship.capability & ShipCapabilityFlag::Proximity != 0 && self.ship.shrapnel == 0 {
            index += 3;
        } else if self.ship.capability & ShipCapabilityFlag::Proximity != 0
            && self.ship.shrapnel > 0
        {
            index += 9;
        } else if self.ship.capability & ShipCapabilityFlag::Proximity == 0
            && self.ship.shrapnel > 0
        {
            index += 6;
        }

        Some(index + self.ship.bombs as usize - 1)
    }

    fn render_energy(&self, render_state: &mut RenderState, sprites: &GameSprites) {
        if let Some(font_sprites) = sprites.get_set(GameSpriteKind::EnergyFont) {
            let mut energy = self.ship.current_energy / 1000;
            let mut x_pixels = render_state.config.width as i32;
            let y_pixels = 0;

            loop {
                let digit = energy % 10;
                let renderable = &font_sprites.renderables[digit as usize];

                x_pixels -= renderable.size[0] as i32;

                render_state.sprite_renderer.draw(
                    &render_state.ui_camera,
                    renderable,
                    x_pixels,
                    y_pixels,
                    Layer::Gauges,
                );

                energy /= 10;

                if energy == 0 {
                    break;
                }
            }
        }
    }

    fn render_energybar(
        &self,
        render_state: &mut RenderState,
        sprites: &GameSprites,
        settings: &ArenaSettings,
        current_tick: GameTick,
    ) {
        let energybar_z = Layer::Gauges.z() + 0.9f32;
        let config_max_z = Layer::Gauges.z();
        let ship_max_z = Layer::Gauges.z() + 0.1f32;

        if let Some(energybar_sprites) = sprites.get_set(GameSpriteKind::HealthBar) {
            let renderable = &energybar_sprites.renderables[0];

            let x_pixels = (render_state.config.width / 2) as i32;
            let y_pixels = (renderable.size[1] / 2) as i32;

            render_state.sprite_renderer.draw_centered_with_z(
                &render_state.ui_camera,
                renderable,
                x_pixels,
                y_pixels,
                energybar_z,
            );
        }

        if let Some(gradient_sprites) = sprites.get_set(GameSpriteKind::Gradient) {
            // Render the bar that indicates the max energy set in ship settings.
            {
                let mut config_max_energy_renderable = gradient_sprites.renderables[5];

                config_max_energy_renderable.size[0] =
                    (render_state.config.width as f32 * 0.35f32) as u32;
                config_max_energy_renderable.size[1] = 2;

                let x_pixels = (render_state.config.width / 2) as i32;
                let y_pixels = 10;

                render_state.sprite_renderer.draw_centered_with_z(
                    &render_state.ui_camera,
                    &config_max_energy_renderable,
                    x_pixels,
                    y_pixels,
                    config_max_z,
                );
            }

            let config_max_energy =
                settings.get_ship_settings(self.ship.kind).maximum_energy as u32;
            let upgrade_percent = (self.ship.max_energy / 1000) as f32 / config_max_energy as f32;

            // Render the bar that indicates our ship's max energy compared to the max in settings.
            {
                let mut ship_max_energy_renderable = gradient_sprites.renderables[0];

                ship_max_energy_renderable.size[0] =
                    (render_state.config.width as f32 * 0.35f32 * upgrade_percent) as u32;
                ship_max_energy_renderable.size[1] = 2;

                let x_pixels = (render_state.config.width / 2) as i32;
                let y_pixels = 10;

                render_state.sprite_renderer.draw_centered_with_z(
                    &render_state.ui_camera,
                    &ship_max_energy_renderable,
                    x_pixels,
                    y_pixels,
                    ship_max_z,
                );
            }

            // Render the current energy bar.
            {
                let energy_percent =
                    self.ship.current_energy as f32 / self.ship.max_energy.max(1) as f32;

                let animation_index = get_animation_index(current_tick.value(), 14, 14 * 10);

                let start_index = if energy_percent < 0.25f32 {
                    14 * 2
                } else if energy_percent < 0.5f32 {
                    14
                } else {
                    0
                };

                let full_energy_width =
                    (render_state.config.width as f32 * 0.35f32 * upgrade_percent) as u32;

                let mut renderable = gradient_sprites.renderables[start_index + animation_index];
                renderable.size[0] = (energy_percent * full_energy_width as f32) as u32;
                renderable.size[1] = 6;

                // Set uv so it doesn't interpolate and doesn't bleed along edges.
                if let Some(sheet) = render_state
                    .sprite_renderer
                    .get_sheet(gradient_sprites.sheet_index)
                {
                    renderable.uv_size[0] = 0.0f32;
                    renderable.uv_size[1] = 0.0f32;
                    renderable.uv_start[0] += 1.0f32 / (sheet.width as f32 * 2.0f32);
                    renderable.uv_start[1] += 1.0f32 / (sheet.height as f32 * 2.0f32);
                }

                let x_pixels = (render_state.config.width / 2) as i32;
                let y_pixels = 16;

                render_state.sprite_renderer.draw_centered(
                    &render_state.ui_camera,
                    &renderable,
                    x_pixels,
                    y_pixels,
                    Layer::Gauges,
                );
            }
        }
    }

    fn render_attach_turret(
        &self,
        player_manager: &PlayerManager,
        render_state: &mut RenderState,
        sprites: &GameSprites,
    ) {
        if let Some(me) = player_manager.get_self() {
            if me.attach_parent.valid() {
                if let Some(parent) = player_manager.get_by_id(me.attach_parent) {
                    if let Some(parent_position) = parent.position {
                        let (x_pixels, y_pixels) = parent_position.to_pixels();

                        if let Some(turret_sprites) = sprites.get_set(GameSpriteKind::Turret) {
                            let renderable =
                                &turret_sprites.renderables[self.ship.get_direction() as usize];

                            render_state.sprite_renderer.draw_centered(
                                &render_state.camera,
                                renderable,
                                x_pixels,
                                y_pixels,
                                Layer::AfterShips,
                            );
                        }
                    }
                }
            }
        }
    }

    fn render_emp_trail(
        player_manager: &PlayerManager,
        render_state: &mut Option<&mut RenderState>,
        current_tick: GameTick,
    ) {
        let Some(me) = player_manager.get_self() else {
            return;
        };
        let Some(me_position) = me.position else {
            return;
        };

        let Some(render_state) = render_state else {
            return;
        };

        let (x_pixels, y_pixels) = me_position.to_pixels();

        if current_tick.value() % 15 == 0 {
            render_state.animation_renderer.add(
                GameSpriteKind::Spark,
                current_tick,
                0,
                10,
                50,
                x_pixels,
                y_pixels,
                Layer::AfterBackground,
            );
        }
    }
}
