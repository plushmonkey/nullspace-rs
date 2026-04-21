use crate::{
    arena_settings::ArenaSettings,
    clock::GameTick,
    map::Map,
    math::Position,
    player::PlayerManager,
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
    pub kind: WeaponKind,
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
}

impl Simulation {
    pub fn new(tick: GameTick) -> Self {
        Self {
            player_manager: PlayerManager::new(),
            weapon_manager: WeaponManager::new(),
            powerball_manager: PowerballManager::new(),
            tick,
            events: vec![],
        }
    }

    pub fn tick(&mut self, map: &Map, settings: &ArenaSettings) {
        self.events.clear();

        for player in &mut self.player_manager.players {
            player_simulation::integrate_player(map, settings, player);
        }

        self.weapon_manager.simulate(
            map,
            settings,
            &mut self.player_manager,
            self.tick,
            &mut self.events,
        );

        for powerball in &mut self.powerball_manager.balls {
            if powerball.state == PowerballState::World {
                integrate_powerball(
                    map,
                    settings.powerball_mode,
                    settings.powerball_bounce,
                    powerball,
                );

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

                                if map.cast(player_position, direction, dist).hit {
                                    continue;
                                }
                            }

                            powerball.remaining_pickup_ticks = 100;
                        }
                    }
                }
            }
        }

        self.tick = GameTick::new(self.tick.value().wrapping_add(1), 0);
    }
}
