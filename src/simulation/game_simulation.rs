use crate::{
    arena_settings::ArenaSettings,
    clock::GameTick,
    map::Map,
    math::Position,
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

    pub fn tick(&mut self, map: &Map, settings: &ArenaSettings) {
        self.events.clear();

        self.tick = GameTick::new(self.tick.value().wrapping_add(1), 0);

        for player in &mut self.player_manager.players {
            player_simulation::integrate_player(map, settings, player);
            // If we have a parent, store us and the parent so we can sync after everyone has been simulated.
            if player.attach_parent.valid() {
                self.child_players.push((player.id, player.attach_parent));
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

        for powerball in &mut self.powerball_manager.balls {
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

                            powerball.remaining_pickup_ticks = 100;
                        }
                    }
                }
            }
        }
    }
}
