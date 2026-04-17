use crate::{
    arena_settings::ArenaSettings,
    clock::GameTick,
    map::Map,
    math::Position,
    player::PlayerManager,
    simulation::{player_simulation, weapon_simulation::WeaponManager},
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
    pub tick: GameTick,
    pub events: Vec<SimulationEvent>,
}

impl Simulation {
    pub fn new(tick: GameTick) -> Self {
        Self {
            player_manager: PlayerManager::new(),
            weapon_manager: WeaponManager::new(),
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

        self.tick = GameTick::new(self.tick.value().wrapping_add(1), 0);
    }
}
