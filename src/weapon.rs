use crate::{
    clock::GameTick,
    math::{Position, Velocity},
    player::PlayerId,
};

#[derive(Copy, Clone)]
pub struct BulletWeapon {
    pub level: u8,
    pub link_id: Option<u32>,
}

#[derive(Copy, Clone)]
pub struct ProximityBombData {
    pub hit_player_id: Option<PlayerId>,
    pub highest_offset: u32,
    pub sensor_end_tick: GameTick,
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
    pub prox: Option<ProximityBombData>,
}

impl BombWeapon {
    pub fn initialize_rng_seed(&mut self, position: Position, velocity: Velocity, frequency: u16) {
        self.rng_seed = (self.shrapnel_count as u32)
            .wrapping_add(self.level as u32)
            .wrapping_add(position.x.0 as u32)
            .wrapping_add(position.y.0 as u32)
            .wrapping_add_signed(velocity.x.0 as i32)
            .wrapping_add_signed(velocity.y.0 as i32)
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
pub enum WeaponKind {
    None,
    Bullet(BulletWeapon),
    BouncingBullet(BulletWeapon),
    Bomb(BombWeapon),
    ProximityBomb(BombWeapon),
    Repel,
    Decoy(DecoyWeapon),
    Burst(BurstWeapon),
    Thor,
    Wormhole,
    Shrapnel(ShrapnelWeapon),
}

pub struct Weapon {
    pub kind: WeaponKind,

    pub position: Position,
    pub velocity: Velocity,

    pub player_id: PlayerId,
    pub frequency: u16,

    pub remaining_ticks: u32,
}

impl Weapon {
    pub fn new(
        kind: WeaponKind,
        position: Position,
        velocity: Velocity,
        player_id: PlayerId,
        frequency: u16,
        remaining_ticks: u32,
    ) -> Self {
        let mut kind = kind;

        match &mut kind {
            WeaponKind::Bomb(bomb_weapon) | WeaponKind::ProximityBomb(bomb_weapon) => {
                bomb_weapon.initialize_rng_seed(position, velocity, frequency);
            }
            _ => {}
        }

        Self {
            kind,
            position,
            velocity,
            player_id,
            frequency,
            remaining_ticks,
        }
    }
}
