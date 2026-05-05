use crate::{
    arena_settings::ArenaSettings,
    clock::GameTick,
    map::{Map, TILE_ID_SAFE, TILE_ID_THOR_KILLER, TILE_ID_WEAPON_KILLER, TILE_ID_WORMHOLE},
    math::{
        PixelUnit, Position, PositionUnit, Rectangle, Velocity, get_heading_from_direction,
        radians, rotate_vec2,
    },
    player::{Player, PlayerId, PlayerManager},
    rng::VieRng,
    ship::ShipKind,
    simulation::game_simulation::{SimulationEvent, SimulationEventKind, WeaponExplosionEvent},
    weapon::{ProximityBombData, ShrapnelWeapon, Weapon, WeaponKind},
};

#[derive(PartialEq, Copy, Clone)]
enum WeaponSimulateResult {
    Continue,
    WallExplosion,
    PlayerExplosion(PlayerId),
    TimedOut,
}

pub struct WeaponManager {
    pub weapons: Vec<Weapon>,
    pub next_link_id: u32,
    link_removal: Vec<(u32, WeaponSimulateResult)>,
}

impl WeaponManager {
    pub fn new() -> Self {
        Self {
            weapons: vec![],
            next_link_id: 0,
            link_removal: vec![],
        }
    }

    pub fn spawn_weapons(
        &mut self,
        player: &Player,
        position: Position,
        velocity: Velocity,
        direction: u8,
        kind: WeaponKind,
        settings: &ArenaSettings,
        timestamp: GameTick,
    ) -> usize {
        let mut kind = kind;
        if player.ship_kind == ShipKind::Spectator {
            return 0;
        }

        let ship_settings = settings.get_ship_settings(player.ship_kind);
        let mut spawn_count = 0;

        let heading = get_heading_from_direction(direction);

        match &mut kind {
            WeaponKind::Bullet(bullet) | WeaponKind::BouncingBullet(bullet) => {
                let multi = bullet.multi;

                if multi || ship_settings.double_barrel {
                    bullet.link_id = Some(self.next_link_id);
                    self.next_link_id = self.next_link_id.wrapping_add(1);
                }

                if ship_settings.double_barrel {
                    let perp = heading.perp();
                    let offset = perp * ship_settings.get_radius() as f32 * 0.75f32;
                    let offset_x = offset.x as i32;
                    let offset_y = offset.y as i32;

                    self.spawn_weapon(
                        player,
                        position - Position::from_pixels(PixelUnit(offset_x), PixelUnit(offset_y)),
                        velocity,
                        heading,
                        kind,
                        settings,
                        timestamp,
                    );
                    spawn_count += 1;

                    self.spawn_weapon(
                        player,
                        position + Position::from_pixels(PixelUnit(offset_x), PixelUnit(offset_y)),
                        velocity,
                        heading,
                        kind,
                        settings,
                        timestamp,
                    );
                    spawn_count += 1;
                } else {
                    self.spawn_weapon(
                        player, position, velocity, heading, kind, settings, timestamp,
                    );
                    spawn_count += 1;
                }

                if multi {
                    let rads = radians(ship_settings.multi_fire_angle as f32 / 111.0f32);
                    let player_heading = heading;
                    let first_heading = rotate_vec2(player_heading, rads);
                    let second_heading = rotate_vec2(player_heading, -rads);

                    self.spawn_weapon(
                        player,
                        position,
                        velocity,
                        first_heading,
                        kind,
                        settings,
                        timestamp,
                    );
                    spawn_count += 1;

                    self.spawn_weapon(
                        player,
                        position,
                        velocity,
                        second_heading,
                        kind,
                        settings,
                        timestamp,
                    );
                    spawn_count += 1;
                }
            }
            WeaponKind::Burst(_) => {
                let count = ship_settings.burst_shrapnel as usize;
                for i in 0..count {
                    let degrees = (i * 40000) / count * 9;
                    let rads = radians(degrees as f32 / 1000.0f32);
                    let direction = glam::Vec2::new(f32::sin(rads), -f32::cos(rads));

                    self.spawn_weapon(
                        player, position, velocity, direction, kind, settings, timestamp,
                    );
                    spawn_count += 1;
                }
            }
            _ => {
                self.spawn_weapon(
                    player, position, velocity, heading, kind, settings, timestamp,
                );
                spawn_count += 1;
            }
        }

        spawn_count
    }

    fn spawn_weapon(
        &mut self,
        player: &Player,
        position: Position,
        velocity: Velocity,
        heading: glam::Vec2,
        kind: WeaponKind,
        settings: &ArenaSettings,
        timestamp: GameTick,
    ) {
        let ship_settings = settings.get_ship_settings(player.ship_kind);

        let (speed, remaining_ticks) = match &kind {
            WeaponKind::Bullet(_) | WeaponKind::BouncingBullet(_) => (
                ship_settings.bullet_speed as i32,
                settings.bullet_alive_time as u32,
            ),
            WeaponKind::Bomb(bomb) | WeaponKind::ProximityBomb(bomb) => {
                let speed = if bomb.mine {
                    0
                } else {
                    ship_settings.bomb_speed
                } as i32;
                let remaining_ticks = if bomb.mine {
                    settings.mine_alive_time
                } else {
                    settings.bomb_alive_time
                } as u32;

                (speed, remaining_ticks)
            }
            WeaponKind::Repel => {
                let remaining_ticks = 60;

                (0, remaining_ticks)
            }
            WeaponKind::Decoy(_) => (0, settings.decoy_alive_time as u32),
            WeaponKind::Burst(_) => (
                ship_settings.burst_speed as i32,
                settings.bullet_alive_time as u32,
            ),
            WeaponKind::Thor(_) => (
                ship_settings.bomb_speed as i32,
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

        // Player sends a weapon packet with their game time at the time of spawn, server receives that and changes the timestamp to the time the server received it.
        // The server calculates the difference and stores that in ping.
        // The timestamp is already reduced by the ping amount in the calling functions.
        //
        // This is used during the simulation step to tick enough times to match our current game tick.
        let spawn_timestamp = timestamp;
        let weapon = Weapon::new(
            kind,
            position,
            velocity,
            player.id,
            player.frequency,
            remaining_ticks,
            spawn_timestamp,
        );

        self.weapons.push(weapon);
    }

    pub fn simulate(
        &mut self,
        map: &Map,
        settings: &ArenaSettings,
        player_manager: &mut PlayerManager,
        current_tick: GameTick,
        events: &mut Vec<SimulationEvent>,
    ) {
        let mut weapon_index: usize = 0;

        self.link_removal.clear();

        // Custom loop for weapon ticking instead of using iterators, just to make sure it never reconstructs vector and never shuffle-removes.
        'main_loop: loop {
            if weapon_index >= self.weapons.len() {
                break;
            }

            // Weapons are spawned without being simulated, so we must simulate them forward here to match current gamestate.
            let weapon_update_count =
                current_tick.diff(&self.weapons[weapon_index].last_update_tick);

            for _ in 0..weapon_update_count {
                // We must skip weapons that were part of a set of linked bullets.
                if let WeaponKind::Bullet(bullet) | WeaponKind::BouncingBullet(bullet) =
                    &self.weapons[weapon_index].kind
                {
                    if let Some(link_id) = &bullet.link_id {
                        if self
                            .link_removal
                            .iter()
                            .find(|link| link.0 == *link_id)
                            .is_some()
                        {
                            weapon_index += 1;
                            continue 'main_loop;
                        }
                    }
                }

                let sim_result = Self::tick_weapon(
                    map,
                    settings,
                    player_manager,
                    self,
                    weapon_index,
                    current_tick,
                );

                self.weapons[weapon_index].last_update_tick =
                    self.weapons[weapon_index].last_update_tick + 1;

                let hit_player: Option<PlayerId> = match &sim_result {
                    WeaponSimulateResult::PlayerExplosion(player_id) => Some(*player_id),
                    _ => None,
                };

                // Handle explosions by spawning shrapnel and generating explosion events so they can be handled outside of the sim.
                match &sim_result {
                    WeaponSimulateResult::PlayerExplosion(_)
                    | WeaponSimulateResult::WallExplosion => {
                        self.handle_weapon_explosion(settings, weapon_index);

                        let weapon = &self.weapons[weapon_index];

                        let event = SimulationEvent {
                            kind: SimulationEventKind::WeaponExplosion(WeaponExplosionEvent {
                                position: weapon.position,
                                frequency: weapon.frequency,
                                kind: weapon.kind.clone(),
                                remaining_ticks: weapon.remaining_ticks,
                                shooter: weapon.player_id,
                                hit_player,
                            }),
                            tick: weapon.last_update_tick,
                        };

                        events.push(event);
                    }
                    _ => {}
                }

                // Only player explosions cause the linked bullets to all explode.
                if let WeaponSimulateResult::PlayerExplosion(_) = sim_result {
                    if let WeaponKind::Bullet(bullet) | WeaponKind::BouncingBullet(bullet) =
                        &self.weapons[weapon_index].kind
                    {
                        if let Some(link_id) = bullet.link_id {
                            self.link_removal.push((link_id, sim_result));
                        }
                    }
                }

                if sim_result != WeaponSimulateResult::Continue {
                    self.weapons.swap_remove(weapon_index);
                    continue 'main_loop;
                }
            }

            weapon_index += 1;
        }

        // Go through each link removed and destroy the weapons associated.
        // Also spawn explosion events on linked bullets.
        if !self.link_removal.is_empty() {
            weapon_index = 0;
            loop {
                if weapon_index >= self.weapons.len() {
                    break;
                }

                if let WeaponKind::Bullet(bullet) | WeaponKind::BouncingBullet(bullet) =
                    &self.weapons[weapon_index].kind
                {
                    if let Some(link_id) = &bullet.link_id {
                        match &self.link_removal.iter().find(|link| link.0 == *link_id) {
                            Some((_, sim_result)) => {
                                match sim_result {
                                    WeaponSimulateResult::PlayerExplosion(_)
                                    | WeaponSimulateResult::WallExplosion => {
                                        self.handle_weapon_explosion(settings, weapon_index);

                                        let weapon = &self.weapons[weapon_index];

                                        let event = SimulationEvent {
                                            kind: SimulationEventKind::WeaponExplosion(
                                                WeaponExplosionEvent {
                                                    frequency: weapon.frequency,
                                                    position: weapon.position,
                                                    kind: weapon.kind.clone(),
                                                    remaining_ticks: weapon.remaining_ticks,
                                                    shooter: weapon.player_id,
                                                    hit_player: None, // We ignore the hit player because we don't want both events to apply damage.
                                                },
                                            ),
                                            tick: weapon.last_update_tick,
                                        };

                                        events.push(event);
                                    }
                                    _ => {}
                                }

                                self.weapons.swap_remove(weapon_index);
                                continue;
                            }
                            None => {}
                        }
                    }
                }

                weapon_index += 1;
            }
        }
    }

    fn tick_weapon(
        map: &Map,
        settings: &ArenaSettings,
        player_manager: &mut PlayerManager,
        weapon_manager: &mut WeaponManager,
        weapon_index: usize,
        current_tick: GameTick,
    ) -> WeaponSimulateResult {
        let weapon = &mut weapon_manager.weapons[weapon_index];

        if weapon.remaining_ticks > 1 {
            weapon.remaining_ticks = weapon.remaining_ticks.saturating_sub(1);
        } else {
            return WeaponSimulateResult::TimedOut;
        }

        let player = player_manager.get_by_id(weapon.player_id);
        if player.is_none() {
            return WeaponSimulateResult::TimedOut;
        }

        let player = player.expect("weapon player should exist during tick");

        if player.ship_kind == ShipKind::Spectator {
            return WeaponSimulateResult::TimedOut;
        }

        if let Some(player_position) = player.position {
            if map.get_tile_from_position(&player_position) == TILE_ID_SAFE {
                return WeaponSimulateResult::TimedOut;
            }
        }

        let sim_result = Self::integrate_weapon_position(map, weapon);

        if sim_result != WeaponSimulateResult::Continue {
            return sim_result;
        }

        match &mut weapon.kind {
            // Handle proximity sensor
            WeaponKind::ProximityBomb(bomb) | WeaponKind::Thor(bomb) => {
                if let Some(active_prox) = &mut bomb.active_prox {
                    let Some(hit_player) = player_manager.get_by_id(active_prox.hit_player_id)
                    else {
                        // The player that activated the prox sensor left the arena.
                        return WeaponSimulateResult::WallExplosion;
                    };

                    if hit_player.ship_kind == ShipKind::Spectator {
                        return WeaponSimulateResult::WallExplosion;
                    }

                    let Some(hit_player_position) = hit_player.position else {
                        return WeaponSimulateResult::WallExplosion;
                    };

                    let highest = ProximityBombData::calculate_highest_delta(
                        weapon.position,
                        hit_player_position,
                    );

                    if highest > active_prox.highest_offset
                        || current_tick >= active_prox.sensor_end_tick
                    {
                        return WeaponSimulateResult::PlayerExplosion(hit_player.id);
                    } else {
                        active_prox.highest_offset = highest;
                    }

                    return WeaponSimulateResult::Continue;
                }
            }
            // Don't attempt player collision if the burst isn't active yet.
            WeaponKind::Burst(burst) => {
                if !burst.active {
                    return WeaponSimulateResult::Continue;
                }
            }
            // There's nothing to process with a decoy.
            WeaponKind::Decoy(_) => {
                return WeaponSimulateResult::Continue;
            }
            WeaponKind::Repel => {
                Self::simulate_repel(map, settings, player_manager, weapon_manager, weapon_index);

                return WeaponSimulateResult::Continue;
            }
            _ => {}
        }

        const BASE_WEAPON_RADIUS: i32 = 3500;
        let weapon_radius = match &weapon.kind {
            WeaponKind::Bomb(_) => PositionUnit(BASE_WEAPON_RADIUS),
            WeaponKind::ProximityBomb(bomb) | WeaponKind::Thor(bomb) => {
                // It's +2 to add the base 1 tile prox that always exists, then another +1 because level 0 is actually level 1 bomb.
                let radius = (bomb.level + 2) as i32 * 16000 + BASE_WEAPON_RADIUS;

                PositionUnit(radius)
            }
            _ => PositionUnit(BASE_WEAPON_RADIUS),
        };

        // Perform player collision tests
        for player in &player_manager.players {
            if player.ship_kind == ShipKind::Spectator {
                continue;
            }

            if player.frequency == weapon.frequency {
                continue;
            }

            if player.is_dead() {
                continue;
            }

            if !player.is_synchronized(current_tick) {
                continue;
            }

            let Some(player_position) = player.position else {
                continue;
            };

            let ship_settings = settings.get_ship_settings(player.ship_kind);
            let player_radius = PositionUnit(ship_settings.get_radius() as i32 * 1000);

            let collider = Rectangle::from_radius(weapon.position, weapon_radius + player_radius);

            if collider.contains(player_position) {
                match &mut weapon.kind {
                    WeaponKind::ProximityBomb(bomb) | WeaponKind::Thor(bomb) => {
                        let highest_offset = ProximityBombData::calculate_highest_delta(
                            weapon.position,
                            player_position,
                        );

                        // If we had a collision during the first update tick, then we should activate the sensor immediately because it's a close bomb.
                        let sensor_end_tick = if current_tick == weapon.spawn_timestamp {
                            current_tick
                        } else {
                            current_tick + settings.bomb_explode_delay as i32
                        };

                        bomb.active_prox = Some(ProximityBombData {
                            hit_player_id: player.id,
                            highest_offset,
                            sensor_end_tick,
                        });
                    }
                    _ => {
                        return WeaponSimulateResult::PlayerExplosion(player.id);
                    }
                }
            }
        }

        WeaponSimulateResult::Continue
    }

    fn integrate_weapon_position(map: &Map, weapon: &mut Weapon) -> WeaponSimulateResult {
        // todo: gravity bombs

        let prev_x = weapon.position.x;
        weapon.position.x = weapon.position.x + weapon.velocity.x;

        let x_collide = match &weapon.kind {
            WeaponKind::Thor(_) => {
                let tile_id = map.get_tile_from_position(&weapon.position);

                if tile_id == TILE_ID_THOR_KILLER {
                    return WeaponSimulateResult::TimedOut;
                }

                false
            }
            _ => {
                let tile_id = map.get_tile_from_position(&weapon.position);

                if tile_id == TILE_ID_WEAPON_KILLER {
                    return WeaponSimulateResult::TimedOut;
                }

                if tile_id == TILE_ID_THOR_KILLER
                    || map.is_solid_position(weapon.position, weapon.frequency)
                {
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
            WeaponKind::Thor(_) => {
                let tile_id = map.get_tile_from_position(&weapon.position);

                if tile_id == TILE_ID_THOR_KILLER {
                    return WeaponSimulateResult::TimedOut;
                }

                false
            }
            _ => {
                let tile_id = map.get_tile_from_position(&weapon.position);

                if tile_id == TILE_ID_WEAPON_KILLER {
                    return WeaponSimulateResult::TimedOut;
                }

                if tile_id == TILE_ID_THOR_KILLER
                    || map.is_solid_position(weapon.position, weapon.frequency)
                {
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
                WeaponKind::Shrapnel(shrapnel) => {
                    // Shrapnel that collides near spawn times out.
                    let alive_ticks = weapon.last_update_tick.diff(&weapon.spawn_timestamp);

                    if alive_ticks <= 25 {
                        return WeaponSimulateResult::TimedOut;
                    }

                    if !shrapnel.bouncing {
                        return WeaponSimulateResult::WallExplosion;
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
                WeaponKind::Bullet(_) => {
                    return WeaponSimulateResult::WallExplosion;
                }
                _ => {}
            }
        }

        if map.get_tile_from_position(&weapon.position) == TILE_ID_WORMHOLE {
            return WeaponSimulateResult::TimedOut;
        }

        return WeaponSimulateResult::Continue;
    }

    fn simulate_repel(
        map: &Map,
        settings: &ArenaSettings,
        player_manager: &mut PlayerManager,
        weapon_manager: &mut WeaponManager,
        weapon_index: usize,
    ) {
        let effect_radius = settings.repel_distance as i32;
        let effect_speed = settings.repel_speed;
        let repel_weapon = &weapon_manager.weapons[weapon_index];
        let repel_position = repel_weapon.position;
        let repel_freq = repel_weapon.frequency;

        let collider = Rectangle::from_radius(repel_position, PixelUnit(effect_radius).into());

        for weapon in &mut weapon_manager.weapons {
            if weapon.frequency == repel_freq {
                continue;
            }

            if let WeaponKind::Repel = weapon.kind {
                continue;
            }

            if collider.contains(weapon.position) {
                let dx = weapon.position.x.0 - repel_position.x.0;
                let dy = weapon.position.y.0 - repel_position.y.0;

                let direction = glam::Vec2::new(dx as f32, dy as f32).normalize();

                weapon.velocity.x = PositionUnit((direction.x * effect_speed as f32) as i32);
                weapon.velocity.y = PositionUnit((direction.y * effect_speed as f32) as i32);

                // Convert mines into bombs with new bomb alive time.
                if let WeaponKind::Bomb(bomb) | WeaponKind::ProximityBomb(bomb) = &mut weapon.kind {
                    if bomb.mine {
                        bomb.mine = false;
                        weapon.remaining_ticks = settings.bomb_alive_time as u32;
                    }
                }
            }
        }

        for player in &mut player_manager.players {
            if player.frequency == repel_freq || player.is_dead() {
                continue;
            }

            if player.ship_kind == ShipKind::Spectator {
                continue;
            }

            let Some(player_position) = player.position else {
                continue;
            };

            if collider.contains(player_position) {
                if map.get_tile_from_position(&player_position) == TILE_ID_SAFE {
                    continue;
                }

                let dx = player_position.x.0 - repel_position.x.0;
                let dy = player_position.y.0 - repel_position.y.0;

                let direction = glam::Vec2::new(dx as f32, dy as f32).normalize();

                player.velocity.x = PositionUnit((direction.x * effect_speed as f32) as i32);
                player.velocity.y = PositionUnit((direction.y * effect_speed as f32) as i32);
            }
        }
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
                let spawn_timestamp = weapon.last_update_tick;

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

                    self.weapons.push(Weapon::new(
                        weapon_kind,
                        position,
                        velocity,
                        player_id,
                        frequency,
                        settings.bullet_alive_time as u32,
                        spawn_timestamp,
                    ));
                }
            }
            _ => {}
        }
    }
}
