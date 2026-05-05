use crate::{
    arena_settings::ArenaSettings, clock::GameTick, math::Position, player::StatusFlags,
    prize::apply_random_prizes, weapon::WeaponKind,
};

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum ShipKind {
    Warbird = 1,
    Javelin,
    Spider,
    Leviathan,
    Terrier,
    Weasel,
    Lancaster,
    Shark,
    Spectator,
}

impl ShipKind {
    pub fn network_value(&self) -> u8 {
        *self as u8 - 1
    }

    pub fn from_network_value(v: u8) -> ShipKind {
        match v {
            0 => ShipKind::Warbird,
            1 => ShipKind::Javelin,
            2 => ShipKind::Spider,
            3 => ShipKind::Leviathan,
            4 => ShipKind::Terrier,
            5 => ShipKind::Weasel,
            6 => ShipKind::Lancaster,
            7 => ShipKind::Shark,
            8 => ShipKind::Spectator,
            _ => ShipKind::Spectator,
        }
    }
}

#[allow(nonstandard_style)]
pub mod ShipCapabilityFlag {
    pub const Stealth: u8 = 1 << 0;
    pub const Cloak: u8 = 1 << 1;
    pub const XRadar: u8 = 1 << 2;
    pub const Antiwarp: u8 = 1 << 3;
    pub const Multifire: u8 = 1 << 4;
    pub const Proximity: u8 = 1 << 5;
    pub const BouncingBullets: u8 = 1 << 6;
}

pub type ShipCapabilityFlags = u8;

pub struct Ship {
    pub kind: ShipKind,

    pub current_energy: u32,
    pub current_orientation: i32,

    pub max_energy: u32,
    pub recharge: u32,
    pub rotation: u32,
    pub thrust: u32,
    pub speed: u32,

    pub guns: u8,
    pub bombs: u8,
    pub shrapnel: u8,
    pub repels: u8,

    pub bursts: u8,
    pub decoys: u8,
    pub thors: u8,
    pub bricks: u8,

    pub rockets: u8,
    pub portals: u8,

    pub next_bullet_tick: GameTick,
    pub next_bomb_tick: GameTick,
    pub next_repel_tick: GameTick,

    pub rocket_remaining_ticks: u32,
    pub shutdown_remaining_ticks: u32,
    pub fake_antiwarp_remaining_ticks: u32,
    pub emped_remaining_ticks: u32,
    pub super_remaining_ticks: u32,
    pub shield_remaining_ticks: u32,
    pub portal_remaining_ticks: u32,
    pub flag_remaining_ticks: u32,
    pub repel_effect_remaining_ticks: u32,

    pub portal_position: Option<Position>,

    pub multifire: bool,
    pub status: u8,
    pub capability: ShipCapabilityFlags,
    pub bounty: u16,

    pub weapon: Option<WeaponKind>,
}

impl Ship {
    pub fn new() -> Self {
        Self {
            kind: ShipKind::Spectator,
            current_energy: 0,
            current_orientation: 0,
            max_energy: 0,
            recharge: 0,
            rotation: 0,
            thrust: 0,
            speed: 0,
            guns: 0,
            bombs: 0,
            shrapnel: 0,
            repels: 0,
            bursts: 0,
            decoys: 0,
            thors: 0,
            bricks: 0,
            rockets: 0,
            portals: 0,
            next_bullet_tick: GameTick::empty(),
            next_bomb_tick: GameTick::empty(),
            next_repel_tick: GameTick::empty(),
            rocket_remaining_ticks: 0,
            shutdown_remaining_ticks: 0,
            fake_antiwarp_remaining_ticks: 0,
            emped_remaining_ticks: 0,
            super_remaining_ticks: 0,
            shield_remaining_ticks: 0,
            portal_remaining_ticks: 0,
            flag_remaining_ticks: 0,
            repel_effect_remaining_ticks: 0,
            portal_position: None,
            multifire: false,
            status: 0,
            capability: 0,
            bounty: 0,
            weapon: None,
        }
    }

    pub fn get_direction(&self) -> u8 {
        (self.current_orientation / 1000) as u8 % 40
    }

    pub fn is_max_energy(&self) -> bool {
        self.current_energy >= self.max_energy
    }

    pub fn reset(&mut self, settings: &ArenaSettings, current_tick: GameTick, ship_kind: ShipKind) {
        self.kind = ship_kind;
        self.shrapnel = 0;

        self.rocket_remaining_ticks = 0;
        self.shutdown_remaining_ticks = 0;
        self.fake_antiwarp_remaining_ticks = 0;
        self.emped_remaining_ticks = 0;
        self.super_remaining_ticks = 0;
        self.shield_remaining_ticks = 0;
        self.portal_remaining_ticks = 0;
        self.flag_remaining_ticks = 0;
        self.repel_effect_remaining_ticks = 0;

        self.portal_position = None;

        self.multifire = false;
        self.status = 0;
        self.capability = 0;
        self.bounty = 0;

        if let ShipKind::Spectator = ship_kind {
            return;
        }

        self.status = StatusFlags::Flash;

        let ship_settings = settings.get_ship_settings(ship_kind);

        self.next_bomb_tick = current_tick;
        self.next_bullet_tick = current_tick;
        self.next_repel_tick = current_tick;

        self.max_energy = ship_settings.initial_energy as u32 * 1000;
        self.recharge = ship_settings.initial_recharge as u32;
        self.rotation = ship_settings.initial_rotation as u32;
        self.thrust = ship_settings.initial_thrust as u32;
        self.speed = ship_settings.initial_speed as u32;

        self.guns = ship_settings.initial_guns;
        self.bombs = ship_settings.initial_bombs;

        self.repels = ship_settings.initial_repel;
        self.bursts = ship_settings.initial_burst;
        self.decoys = ship_settings.initial_decoy;
        self.thors = ship_settings.initial_thor;
        self.bricks = ship_settings.initial_brick;
        self.rockets = ship_settings.initial_rocket;
        self.portals = ship_settings.initial_portal;

        if self.max_energy > ship_settings.maximum_energy as u32 * 1000 {
            self.max_energy = ship_settings.maximum_energy as u32 * 1000;
        }

        if self.recharge > ship_settings.maximum_recharge as u32 {
            self.recharge = ship_settings.maximum_recharge as u32;
        }

        if self.rotation > ship_settings.maximum_rotation as u32 {
            self.rotation = ship_settings.maximum_rotation as u32;
        }

        if self.thrust > ship_settings.maximum_thrust as u32 {
            self.thrust = ship_settings.maximum_thrust as u32;
        }

        if self.speed > ship_settings.maximum_speed as u32 {
            self.speed = ship_settings.maximum_speed as u32;
        }

        if self.guns > ship_settings.max_guns {
            self.guns = ship_settings.max_guns;
        }

        if self.bombs > ship_settings.max_bombs {
            self.bombs = ship_settings.max_bombs;
        }

        if self.repels > ship_settings.max_repel {
            self.repels = ship_settings.max_repel;
        }

        if self.bursts > ship_settings.max_burst {
            self.bursts = ship_settings.max_burst;
        }

        if self.decoys > ship_settings.max_decoy {
            self.decoys = ship_settings.max_decoy;
        }

        if self.thors > ship_settings.max_thor {
            self.thors = ship_settings.max_thor;
        }

        if self.bricks > ship_settings.max_brick {
            self.bricks = ship_settings.max_brick;
        }

        if self.rockets > ship_settings.max_rocket {
            self.rockets = ship_settings.max_rocket;
        }

        if self.portals > ship_settings.max_portal {
            self.portals = ship_settings.max_portal;
        }

        if ship_settings.stealth_status == 2 {
            self.capability |= ShipCapabilityFlag::Stealth;
        }

        if ship_settings.cloak_status == 2 {
            self.capability |= ShipCapabilityFlag::Cloak;
        }

        if ship_settings.xradar_status == 2 {
            self.capability |= ShipCapabilityFlag::XRadar;
        }

        if ship_settings.antiwarp_status == 2 {
            self.capability |= ShipCapabilityFlag::Antiwarp;
        }

        self.current_energy = self.max_energy;

        if settings.prize_weights.calculate_total_weight() > 0 {
            apply_random_prizes(
                settings,
                self,
                current_tick,
                ship_settings.initial_bounty as i32,
            );
        }

        self.current_energy = self.max_energy;
        self.bounty = ship_settings.initial_bounty;
    }
}
