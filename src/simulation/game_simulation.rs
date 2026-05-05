use crate::{
    arena_settings::ArenaSettings,
    clock::GameTick,
    map::Map,
    math::{Position, Rectangle},
    player::{PlayerId, PlayerManager},
    powerball::{PowerballManager, PowerballState},
    ship::ShipKind,
    simulation::{
        player_simulation, powerball_simulation::integrate_powerball,
        weapon_simulation::WeaponManager,
    },
    weapon::WeaponKind,
};

pub struct WeaponExplosionEvent {
    pub position: Position,
    pub frequency: u16,
    pub kind: WeaponKind,
    // How many ticks were remaining in the weapon that exploded.
    // This is needed for inactive shrapnel detection.
    pub remaining_ticks: u32,
    pub shooter: PlayerId,
    pub hit_player: Option<PlayerId>,
}

pub enum SimulationEventKind {
    WeaponExplosion(WeaponExplosionEvent),
    PowerballPickupRequest(u8, GameTick),
    PowerballTimeout(u8),
    DoorWarp,
}

pub struct SimulationEvent {
    pub kind: SimulationEventKind,
    pub tick: GameTick,
}

pub struct Simulation {
    pub player_manager: PlayerManager,
    pub weapon_manager: WeaponManager,
    pub powerball_manager: PowerballManager,
    pub tick: GameTick,
    pub events: Vec<SimulationEvent>,
    pub powerball_paused: bool,

    child_players: Vec<(PlayerId, PlayerId)>,
}

impl Simulation {
    pub fn new(tick: GameTick) -> Self {
        Self {
            player_manager: PlayerManager::new(),
            weapon_manager: WeaponManager::new(),
            powerball_manager: PowerballManager::new(),
            tick,
            events: vec![],
            powerball_paused: true,
            child_players: vec![],
        }
    }

    fn perform_doorwarp(&mut self, map: &Map, settings: &ArenaSettings) {
        let Some(me) = self.player_manager.get_self_mut() else {
            return;
        };

        let Some(_) = me.position else {
            return;
        };

        if me.ship_kind == ShipKind::Spectator {
            return;
        }

        let player_collider =
            me.get_collider(settings.get_ship_settings(me.ship_kind).get_radius());

        for door_tile in &map.doors {
            if !map.is_solid(door_tile.x(), door_tile.y(), me.frequency) {
                continue;
            }

            let door_collider = Rectangle::new(
                Position::from_tile(door_tile.x() as i32, door_tile.y() as i32),
                Position::from_tile(door_tile.x() as i32 + 1, door_tile.y() as i32 + 1),
            );

            if door_collider.intersects(&player_collider) {
                self.events.push(SimulationEvent {
                    kind: SimulationEventKind::DoorWarp,
                    tick: self.tick,
                });
                return;
            }
        }
    }

    pub fn tick(&mut self, map: &mut Map, settings: &ArenaSettings) {
        self.events.clear();

        self.tick = GameTick::new(self.tick.value().wrapping_add(1), 0);

        if map.doors_mutated {
            self.perform_doorwarp(map, settings);
            map.doors_mutated = false;
        }

        for player in &mut self.player_manager.players {
            player_simulation::integrate_player(map, settings, player);

            // If we have a parent, store us and the parent so we can sync after everyone has been simulated.
            if player.attach_parent.valid() {
                self.child_players.push((player.id, player.attach_parent));
            }

            // Time out extra data so we don't continue displaying energy while it's not being sent.
            if let Some(last_extra_tick) = player.last_extra_data_timestamp {
                const EXTRA_DATA_TIMEOUT: i32 = 350;

                if self.tick.diff(&last_extra_tick) > EXTRA_DATA_TIMEOUT {
                    player.last_extra_data_timestamp = None;
                    player.extra_position_data = None;
                }
            }
        }

        // Synchronize players to their attach parent.
        for (player_id, parent_id) in &self.child_players {
            if let Some(parent) = self.player_manager.get_by_id(*parent_id) {
                if !parent.is_synchronized(self.tick) {
                    continue;
                }

                if let Some(parent_position) = parent.position {
                    let parent_velocity = parent.velocity;
                    let parent_lerp_velocity = parent.lerp_velocity;
                    let parent_lerp_ticks = parent.lerp_remaining_ticks;

                    if let Some(player) = self.player_manager.get_by_id_mut(*player_id) {
                        player.position = Some(parent_position);
                        player.velocity = parent_velocity;
                        player.lerp_velocity = parent_lerp_velocity;
                        player.lerp_remaining_ticks = parent_lerp_ticks;
                    }
                }
            }
        }
        self.child_players.clear();

        self.weapon_manager.simulate(
            map,
            settings,
            &mut self.player_manager,
            self.tick,
            &mut self.events,
        );

        self.update_balls(map, settings);
    }

    fn update_balls(&mut self, map: &Map, settings: &ArenaSettings) {
        if self.powerball_paused {
            return;
        }

        if let Some(ball_id) = self.powerball_manager.tick_carry_state() {
            // We dropped the ball, so we need to fire it.
            self.events.push(SimulationEvent {
                kind: SimulationEventKind::PowerballTimeout(ball_id),
                tick: self.tick,
            });
        }

        for ball_id in 0..self.powerball_manager.balls.len() {
            let powerball = &mut self.powerball_manager.balls[ball_id];
            let sim_ticks = self.tick.diff(&powerball.current_sim_tick).min(6000);

            if powerball.state == PowerballState::World {
                for _ in 0..sim_ticks {
                    integrate_powerball(
                        map,
                        settings.powerball_mode,
                        settings.powerball_bounce,
                        powerball,
                    );

                    if powerball.velocity.x.0 == 0 && powerball.velocity.y.0 == 0 {
                        break;
                    }

                    if powerball.friction == 0 {
                        break;
                    }
                }

                powerball.current_sim_tick = self.tick;

                let phasing = powerball.is_phasing(self.tick, settings.powerball_pass_delay as i32);

                if !phasing {
                    for player in &self.player_manager.players {
                        if player.is_dead() || !player.is_synchronized(self.tick) {
                            continue;
                        }

                        if player.ship_kind == ShipKind::Spectator {
                            continue;
                        }

                        // Check if this player can pick up the ball. The can't if they shot it and it's still moving.
                        if player.id == powerball.carrier_id
                            && (powerball.velocity.x.0 != 0 || powerball.velocity.y.0 != 0)
                        {
                            continue;
                        }

                        let Some(player_position) = player.position else {
                            continue;
                        };

                        let ball_proximity = settings
                            .get_ship_settings(player.ship_kind)
                            .powerball_proximity
                            as i32;

                        let delta = powerball.position.delta_pixels(&player_position);

                        if delta.0.abs() <= ball_proximity && delta.1.abs() <= ball_proximity {
                            if settings.disable_ball_through_walls {
                                let direction = glam::Vec2::new(delta.0 as f32, delta.1 as f32)
                                    .normalize_or_zero();
                                // Calculate distance in tile space between player and ball
                                let dx = delta.0 as f32 / 16.0f32;
                                let dy = delta.1 as f32 / 16.0f32;
                                let dist = (dx * dx + dy * dy).sqrt();

                                if map
                                    .cast(player_position, direction, dist, player.frequency)
                                    .hit
                                {
                                    continue;
                                }
                            }

                            if player.id == self.player_manager.self_id {
                                self.events.push(SimulationEvent {
                                    kind: SimulationEventKind::PowerballPickupRequest(
                                        ball_id as u8,
                                        powerball.timestamp,
                                    ),
                                    tick: self.tick,
                                });
                            }

                            powerball.remaining_pickup_ticks = PowerballManager::PICKUP_PHASE_TICKS;
                        }
                    }
                }
            }
        }
    }
}
