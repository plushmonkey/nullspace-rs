use crate::{
    arena_settings::ArenaSettings,
    clock::GameTick,
    math::{Position, Velocity},
    player::{Player, PlayerId},
    ship::ShipKind,
};

#[derive(Copy, Clone)]
pub struct BulletWeapon {
    pub level: u8,
    pub multi: bool,
    pub link_id: Option<u32>,
}

#[derive(Copy, Clone)]
pub struct ProximityBombData {
    pub hit_player_id: PlayerId,
    pub highest_offset: u16,
    pub sensor_end_tick: GameTick,
}

impl ProximityBombData {
    pub fn calculate_highest_delta(weapon_position: Position, player_position: Position) -> u16 {
        let dx = i32::abs((weapon_position.x.0 / 1000) - (player_position.x.0 / 1000)) as u16;
        let dy = i32::abs((weapon_position.y.0 / 1000) - (player_position.y.0 / 1000)) as u16;

        u16::max(dx, dy)
    }
}

#[derive(Copy, Clone)]
pub struct BombWeapon {
    pub level: u8,
    pub shrapnel_count: u8,
    pub shrapnel_level: u8,

    pub shrapnel_bouncing: bool,
    pub mine: bool,
    pub emp: bool,

    pub remaining_bounces: u32,
    pub rng_seed: i32,
    pub active_prox: Option<ProximityBombData>,
}

impl BombWeapon {
    pub fn initialize_rng_seed(
        &mut self,
        position: Position,
        velocity: Velocity,
        heading: glam::Vec2,
        speed: u32,
        frequency: u16,
    ) {
        let mut velocity = velocity;

        velocity.x.0 += (heading.x * speed as f32) as i32;
        velocity.y.0 += (heading.y * speed as f32) as i32;

        self.rng_seed = (self.shrapnel_count as u32)
            .wrapping_add(self.level as u32)
            .wrapping_add(position.x.0 as u32)
            .wrapping_add(position.y.0 as u32)
            .wrapping_add(velocity.x.0 as u32)
            .wrapping_add(velocity.y.0 as u32)
            .wrapping_add(frequency as u32) as i32;
    }
}

#[derive(Copy, Clone)]
pub struct ShrapnelWeapon {
    pub level: u8,
    pub bouncing: bool,
}

#[derive(Copy, Clone)]
pub struct DecoyWeapon {
    pub initial_rotation: u8,
}

#[derive(Copy, Clone)]
pub struct BurstWeapon {
    pub active: bool,
}

#[derive(Copy, Clone)]
struct WeaponPacketParameters {
    value: u16,
}

impl WeaponPacketParameters {
    pub fn new(value: u16) -> Self {
        Self { value }
    }

    pub fn kind(&self) -> u16 {
        self.value & 0x1F
    }

    pub fn level(&self) -> u8 {
        ((self.value >> 5) & 0x03) as u8
    }

    pub fn shrapnel_bouncing(&self) -> bool {
        (self.value >> 7) & 0x01 != 0
    }

    pub fn shrapnel_level(&self) -> u8 {
        ((self.value >> 8) & 0x03) as u8
    }

    pub fn shrapnel_count(&self) -> u8 {
        ((self.value >> 10) & 0x1F) as u8
    }

    pub fn alternate(&self) -> bool {
        (self.value >> 15) & 0x01 != 0
    }
}

#[derive(Copy, Clone)]
pub enum WeaponKind {
    None,
    Bullet(BulletWeapon),
    BouncingBullet(BulletWeapon),
    Bomb(BombWeapon),
    ProximityBomb(BombWeapon),
    Repel,
    Decoy(DecoyWeapon),
    Burst(BurstWeapon),
    Thor(BombWeapon),
    Wormhole,
    Shrapnel(ShrapnelWeapon),
}

impl WeaponKind {
    pub fn new(
        packed: u16,
        position: Position,
        velocity: Velocity,
        player: &Player,
        settings: &ArenaSettings,
    ) -> Option<Self> {
        let parameters = WeaponPacketParameters::new(packed);
        let frequency = player.frequency;

        if player.ship_kind == ShipKind::Spectator {
            return None;
        }

        let ship_settings = settings.get_ship_settings(player.ship_kind);

        let kind = match parameters.kind() {
            1 => {
                let kind = WeaponKind::Bullet(BulletWeapon {
                    level: parameters.level(),
                    multi: parameters.alternate(),
                    link_id: None,
                });

                kind
            }
            2 => {
                let kind = WeaponKind::BouncingBullet(BulletWeapon {
                    level: parameters.level(),
                    multi: parameters.alternate(),
                    link_id: None,
                });

                kind
            }
            3 => {
                let emp = ship_settings.emp_bomb;
                let remaining_bounces = ship_settings.bomb_bounce_count as u32;

                let mut bomb_weapon = BombWeapon {
                    level: parameters.level(),
                    shrapnel_count: parameters.shrapnel_count(),
                    shrapnel_level: parameters.shrapnel_level(),
                    shrapnel_bouncing: parameters.shrapnel_bouncing(),
                    mine: parameters.alternate(),
                    emp,
                    remaining_bounces,
                    rng_seed: 0,
                    active_prox: None,
                };

                bomb_weapon.initialize_rng_seed(
                    position,
                    velocity,
                    player.get_heading(),
                    ship_settings.bomb_speed as u32,
                    frequency,
                );

                WeaponKind::Bomb(bomb_weapon)
            }
            4 => {
                let emp = ship_settings.emp_bomb;
                let remaining_bounces = ship_settings.bomb_bounce_count as u32;

                let mut bomb_weapon = BombWeapon {
                    level: parameters.level(),
                    shrapnel_count: parameters.shrapnel_count(),
                    shrapnel_level: parameters.shrapnel_level(),
                    shrapnel_bouncing: parameters.shrapnel_bouncing(),
                    mine: parameters.alternate(),
                    emp,
                    remaining_bounces,
                    rng_seed: 0,
                    active_prox: None,
                };

                bomb_weapon.initialize_rng_seed(
                    position,
                    velocity,
                    player.get_heading(),
                    ship_settings.bomb_speed as u32,
                    frequency,
                );

                WeaponKind::ProximityBomb(bomb_weapon)
            }
            5 => WeaponKind::Repel,
            6 => WeaponKind::Decoy(DecoyWeapon {
                initial_rotation: player.direction,
            }),
            7 => WeaponKind::Burst(BurstWeapon { active: false }),
            8 => WeaponKind::Thor(BombWeapon {
                level: 4,
                shrapnel_count: 0,
                shrapnel_level: 0,
                shrapnel_bouncing: false,
                mine: false,
                emp: false,
                remaining_bounces: 0,
                rng_seed: 0,
                active_prox: None,
            }),
            _ => {
                return None;
            }
        };

        Some(kind)
    }
}

pub struct Weapon {
    pub kind: WeaponKind,

    pub position: Position,
    pub velocity: Velocity,

    pub player_id: PlayerId,
    pub frequency: u16,

    pub remaining_ticks: u32,
    pub spawn_timestamp: GameTick,
}

impl Weapon {
    pub fn new(
        kind: WeaponKind,
        position: Position,
        velocity: Velocity,
        player_id: PlayerId,
        frequency: u16,
        remaining_ticks: u32,
        spawn_timestamp: GameTick,
    ) -> Self {
        Self {
            kind,
            position,
            velocity,
            player_id,
            frequency,
            remaining_ticks,
            spawn_timestamp,
        }
    }
}
