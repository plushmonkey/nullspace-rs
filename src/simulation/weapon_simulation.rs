use crate::{
    arena_settings::ArenaSettings,
    clock::GameTick,
    map::{Map, TILE_ID_SAFE, TILE_ID_WORMHOLE},
    math::{PixelUnit, Position, PositionUnit, Velocity, radians, rotate_vec2},
    player::{Player, PlayerManager},
    rng::VieRng,
    ship::ShipKind,
    weapon::{ShrapnelWeapon, Weapon, WeaponKind},
};

#[derive(PartialEq)]
enum WeaponSimulateResult {
    Continue,
    WallExplosion,
    PlayerExplosion,
    TimedOut,
}

// TODO: There needs to be a way to output events, maybe just store inside a vector so the client can render the changes next tick.

pub struct WeaponManager {
    pub weapons: Vec<Weapon>,
    pub next_link_id: u32,
}

impl WeaponManager {
    pub fn new() -> Self {
        Self {
            weapons: vec![],
            next_link_id: 0,
        }
    }

    pub fn spawn_weapons(
        &mut self,
        player: &Player,
        velocity: Velocity,
        kind: WeaponKind,
        settings: &ArenaSettings,
        timestamp: GameTick,
        current_tick: GameTick,
    ) {
        if player.ship_kind == ShipKind::Spectator {
            return;
        }

        let ship_settings = settings.get_ship_settings(player.ship_kind);

        match &kind {
            WeaponKind::Bullet(bullet) | WeaponKind::BouncingBullet(bullet) => {
                let mut bullet = *bullet;
                let multi = bullet.multi || ship_settings.double_barrel;

                if multi {
                    bullet.link_id = Some(self.next_link_id.wrapping_add(1));
                }

                if ship_settings.double_barrel {
                    let perp = player.get_heading().perp();
                    let offset = perp * ship_settings.radius as f32 * 0.75f32;
                    let offset_x = offset.x as i32;
                    let offset_y = offset.y as i32;

                    self.spawn_weapon(
                        player,
                        player.position
                            - Position::from_pixels(PixelUnit(offset_x), PixelUnit(offset_y)),
                        velocity,
                        player.get_heading(),
                        kind,
                        settings,
                        timestamp,
                        current_tick,
                    );

                    self.spawn_weapon(
                        player,
                        player.position
                            + Position::from_pixels(PixelUnit(offset_x), PixelUnit(offset_y)),
                        velocity,
                        player.get_heading(),
                        kind,
                        settings,
                        timestamp,
                        current_tick,
                    );
                } else {
                    self.spawn_weapon(
                        player,
                        player.position,
                        velocity,
                        player.get_heading(),
                        kind,
                        settings,
                        timestamp,
                        current_tick,
                    );
                }

                if bullet.multi {
                    let rads = radians(ship_settings.multi_fire_angle as f32 / 111.0f32);
                    let player_heading = player.get_heading();
                    let first_heading = rotate_vec2(player_heading, rads);
                    let second_heading = rotate_vec2(player_heading, -rads);

                    self.spawn_weapon(
                        player,
                        player.position,
                        velocity,
                        first_heading,
                        kind,
                        settings,
                        timestamp,
                        current_tick,
                    );

                    self.spawn_weapon(
                        player,
                        player.position,
                        velocity,
                        second_heading,
                        kind,
                        settings,
                        timestamp,
                        current_tick,
                    );
                }
            }
            WeaponKind::Burst(_) => {
                let count = ship_settings.burst_shrapnel as usize;
                for i in 0..count {
                    let degrees = (i * 40000) / count * 9;
                    let rads = radians(degrees as f32 / 1000.0f32);
                    let direction = glam::Vec2::new(f32::sin(rads), -f32::cos(rads));

                    self.spawn_weapon(
                        player,
                        player.position,
                        velocity,
                        direction,
                        kind,
                        settings,
                        timestamp,
                        current_tick,
                    );
                }
            }
            _ => {
                self.spawn_weapon(
                    player,
                    player.position,
                    velocity,
                    player.get_heading(),
                    kind,
                    settings,
                    timestamp,
                    current_tick,
                );
            }
        }
    }

    fn spawn_weapon(
        &mut self,
        player: &Player,
        position: Position,
        velocity: Velocity,
        heading: glam::Vec2,
        kind: WeaponKind,
        settings: &ArenaSettings,
        _timestamp: GameTick, // TODO: Sim
        _current_tick: GameTick,
    ) -> WeaponSimulateResult {
        let ship_settings = settings.get_ship_settings(player.ship_kind);

        let (speed, remaining_ticks) = match &kind {
            WeaponKind::Bullet(_) | WeaponKind::BouncingBullet(_) => (
                ship_settings.bullet_speed as u32,
                settings.bullet_alive_time as u32,
            ),
            WeaponKind::Bomb(bomb) | WeaponKind::ProximityBomb(bomb) => {
                let speed = if bomb.mine {
                    0
                } else {
                    ship_settings.bomb_speed
                } as u32;
                let remaining_ticks = if bomb.mine {
                    settings.mine_alive_time
                } else {
                    settings.bomb_alive_time
                } as u32;

                (speed, remaining_ticks)
            }
            WeaponKind::Repel => {
                let remaining_ticks = 50; // TODO: How long does repels last?

                (0, remaining_ticks)
            }
            WeaponKind::Decoy(_) => (0, settings.decoy_alive_time as u32),
            WeaponKind::Burst(_) => (
                ship_settings.burst_speed as u32,
                settings.bullet_alive_time as u32,
            ),
            WeaponKind::Thor => (
                ship_settings.bomb_speed as u32,
                settings.bomb_alive_time as u32,
            ),
            _ => (0, 0),
        };

        let velocity = match &kind {
            WeaponKind::Repel => Velocity::new(PositionUnit(0), PositionUnit(0)),
            _ => {
                let mut weapon_velocity = velocity;

                if let WeaponKind::Burst(_) = &kind {
                    weapon_velocity = Velocity::new(PositionUnit(0), PositionUnit(0));
                }

                if let WeaponKind::Bomb(bomb) | WeaponKind::ProximityBomb(bomb) = &kind {
                    if bomb.mine {
                        weapon_velocity = Velocity::new(PositionUnit(0), PositionUnit(0));
                    }
                }

                let heading_velocity_x = (heading.x * speed as f32) as i32;
                let heading_velocity_y = (heading.y * speed as f32) as i32;

                weapon_velocity.x = weapon_velocity.x + PositionUnit(heading_velocity_x);
                weapon_velocity.y = weapon_velocity.y + PositionUnit(heading_velocity_y);

                weapon_velocity
            }
        };

        let weapon = Weapon::new(
            kind,
            position,
            velocity,
            player.id,
            player.frequency,
            remaining_ticks,
        );

        // TODO: Simulate

        self.weapons.push(weapon);

        WeaponSimulateResult::Continue
    }

    pub fn simulate(
        &mut self,
        map: &Map,
        settings: &ArenaSettings,
        player_manager: &mut PlayerManager,
    ) {
        let mut weapon_index: usize = 0;

        // Custom loop for weapon ticking instead of using iterators, just to make sure it never reconstructs vector and never shuffle-removes.
        loop {
            if weapon_index >= self.weapons.len() {
                break;
            }

            let sim_result = Self::tick_weapon(
                map,
                settings,
                player_manager,
                &mut self.weapons[weapon_index],
            );

            if sim_result == WeaponSimulateResult::PlayerExplosion
                || sim_result == WeaponSimulateResult::WallExplosion
            {
                self.handle_weapon_explosion(map, settings, weapon_index);
            }

            if sim_result != WeaponSimulateResult::Continue {
                self.weapons.swap_remove(weapon_index);
                // TODO: Remove link
                continue;
            }

            weapon_index += 1;
        }
    }

    fn tick_weapon(
        map: &Map,
        _settings: &ArenaSettings,
        player_manager: &mut PlayerManager,
        weapon: &mut Weapon,
    ) -> WeaponSimulateResult {
        if weapon.remaining_ticks > 0 {
            weapon.remaining_ticks = weapon.remaining_ticks.saturating_sub(1);
        } else {
            return WeaponSimulateResult::TimedOut;
        }

        let player = player_manager.get_by_id(weapon.player_id);
        if player.is_none() {
            return WeaponSimulateResult::TimedOut;
        }

        let player = player.expect("weapon player should exist during tick");
        if player.ship_kind == ShipKind::Spectator
            || map.get_tile_from_position(&player.position) == TILE_ID_SAFE
        {
            return WeaponSimulateResult::TimedOut;
        }

        let sim_result = Self::integrate_weapon_position(map, weapon);

        if sim_result != WeaponSimulateResult::Continue {
            return sim_result;
        }

        sim_result
    }

    fn integrate_weapon_position(map: &Map, weapon: &mut Weapon) -> WeaponSimulateResult {
        // todo: gravity bombs

        let prev_x = weapon.position.x;
        weapon.position.x = weapon.position.x + weapon.velocity.x;

        let x_collide = match &weapon.kind {
            WeaponKind::Thor => false,
            _ => {
                // TODO: Handle special tiles here
                if map.is_solid_position(weapon.position) {
                    weapon.position.x = prev_x;
                    weapon.velocity.x.0 *= -1;
                    true
                } else {
                    false
                }
            }
        };

        let prev_y = weapon.position.y;
        weapon.position.y = weapon.position.y + weapon.velocity.y;

        let y_collide = match &weapon.kind {
            WeaponKind::Thor => false,
            _ => {
                // TODO: Handle special tiles here
                if map.is_solid_position(weapon.position) {
                    weapon.position.y = prev_y;
                    weapon.velocity.y.0 *= -1;
                    true
                } else {
                    false
                }
            }
        };

        if x_collide || y_collide {
            match &mut weapon.kind {
                WeaponKind::Shrapnel(_) => {
                    // Shrapnel that collides near death times out
                    if weapon.remaining_ticks < 25 {
                        return WeaponSimulateResult::TimedOut;
                    }
                }
                WeaponKind::Bomb(bomb_weapon) | WeaponKind::ProximityBomb(bomb_weapon) => {
                    if bomb_weapon.remaining_bounces == 0 {
                        return WeaponSimulateResult::WallExplosion;
                    }

                    bomb_weapon.remaining_bounces -= 1;
                }
                WeaponKind::Burst(burst_weapon) => {
                    burst_weapon.active = true;
                }
                _ => {}
            }
        }

        if map.get_tile_from_position(&weapon.position) == TILE_ID_WORMHOLE {
            return WeaponSimulateResult::TimedOut;
        }

        return WeaponSimulateResult::Continue;
    }

    fn handle_weapon_explosion(
        &mut self,
        map: &Map,
        settings: &ArenaSettings,
        weapon_index: usize,
    ) {
        let weapon = &self.weapons[weapon_index];

        match &weapon.kind {
            WeaponKind::Bomb(bomb_weapon) | WeaponKind::ProximityBomb(bomb_weapon) => {
                let mut rng = VieRng::new(bomb_weapon.rng_seed);
                let shrapnel_count = bomb_weapon.shrapnel_count;
                let shrapnel_level = bomb_weapon.shrapnel_level;
                let shrapnel_bouncing = bomb_weapon.shrapnel_bouncing;
                let position = weapon.position;
                let player_id = weapon.player_id;
                let frequency = weapon.frequency;

                for i in 0..shrapnel_count as u32 {
                    let orientation = if !settings.shrapnel_random {
                        (i * 40000) / (shrapnel_count as u32) * 9
                    } else {
                        (rng.next() as u32 % 40000) * 9
                    };

                    let direction_x: f32 = f32::sin(radians(orientation as f32 / 1000.0f32));
                    let direction_y: f32 = -f32::cos(radians(orientation as f32 / 1000.0f32));

                    let velocity = Velocity::new(
                        PositionUnit((direction_x * settings.shrapnel_speed as f32) as i32),
                        PositionUnit((direction_y * settings.shrapnel_speed as f32) as i32),
                    );

                    let step_x = (position.x.0 + velocity.x.0) / 16000;
                    let step_y = (position.y.0 + velocity.y.0) / 16000;

                    if map.is_solid(step_x as u16, step_y as u16) {
                        continue;
                    }

                    let weapon_kind = WeaponKind::Shrapnel(ShrapnelWeapon {
                        level: shrapnel_level,
                        bouncing: shrapnel_bouncing,
                    });

                    self.weapons.push(Weapon::new(
                        weapon_kind,
                        position,
                        velocity,
                        player_id,
                        frequency,
                        settings.bullet_alive_time as u32,
                    ));
                }
            }
            _ => {}
        }
    }
}
