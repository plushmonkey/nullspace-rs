use crate::{
    arena_settings::ArenaSettings,
    map::{Map, TILE_ID_SAFE, TILE_ID_WORMHOLE},
    math::{PositionUnit, Velocity, radians},
    player::PlayerManager,
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

pub struct WeaponManager {
    pub weapons: Vec<Weapon>,
}

impl WeaponManager {
    pub fn new() -> Self {
        Self { weapons: vec![] }
    }

    pub fn add_weapon(&mut self, weapon: Weapon) {
        self.weapons.push(weapon);
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
                self.handle_weapon_explosion(settings, weapon_index);
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

    fn handle_weapon_explosion(&mut self, settings: &ArenaSettings, weapon_index: usize) {
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

                    let weapon_kind = WeaponKind::Shrapnel(ShrapnelWeapon {
                        level: shrapnel_level,
                        bouncing: shrapnel_bouncing,
                    });

                    self.add_weapon(Weapon::new(
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
