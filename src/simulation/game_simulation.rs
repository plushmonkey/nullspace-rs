use crate::{
    arena_settings::ArenaSettings,
    clock::GameTick,
    map::Map,
    player::PlayerManager,
    simulation::{player_simulation, weapon_simulation::WeaponManager},
    weapon::Weapon,
};

pub struct Simulation {
    pub player_manager: PlayerManager,
    pub weapon_manager: WeaponManager,
    pub tick: GameTick,
}

impl Simulation {
    pub fn new(tick: GameTick) -> Self {
        Self {
            player_manager: PlayerManager::new(),
            weapon_manager: WeaponManager::new(),
            tick,
        }
    }

    pub fn add_weapon(&mut self, weapon: Weapon) {
        self.weapon_manager.add_weapon(weapon);
    }

    pub fn tick(&mut self, map: &Map, settings: &ArenaSettings) {
        for player in &mut self.player_manager.players {
            player_simulation::integrate_player(map, settings, player);
        }

        self.weapon_manager
            .simulate(map, settings, &mut self.player_manager);

        self.tick = GameTick::new(self.tick.value().wrapping_add(1), 0);
    }
}
