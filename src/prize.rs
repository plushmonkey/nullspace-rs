use thiserror::Error;

use crate::{
    arena_settings::ArenaSettings,
    clock::GameTick,
    rng::VieRng,
    ship::{Ship, ShipCapabilityFlag},
};

#[derive(Copy, Clone, Debug)]
pub enum Prize {
    None,
    Recharge,
    Energy,
    Rotation,
    Stealth,
    Cloak,
    XRadar,
    Warp,
    Guns,
    Bombs,
    BouncingBullets,
    Thruster,
    TopSpeed,
    FullCharge,
    EngineShutdown,
    Multifire,
    Proximity,
    Super,
    Shields,
    Shrapnel,
    Antiwarp,
    Repel,
    Burst,
    Decoy,
    Thor,
    Multiprize,
    Brick,
    Rocket,
    Portal,
}

#[derive(Error, Debug)]
pub enum PrizeError {
    #[error("invalid prize id")]
    InvalidPrizeId,

    #[error("invalid ship for prizing")]
    InvalidShip,
}

impl std::convert::TryFrom<i32> for Prize {
    type Error = PrizeError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        const PRIZE_ID_MAP: [Prize; 29] = [
            Prize::None,
            Prize::Recharge,
            Prize::Energy,
            Prize::Rotation,
            Prize::Stealth,
            Prize::Cloak,
            Prize::XRadar,
            Prize::Warp,
            Prize::Guns,
            Prize::Bombs,
            Prize::BouncingBullets,
            Prize::Thruster,
            Prize::TopSpeed,
            Prize::FullCharge,
            Prize::EngineShutdown,
            Prize::Multifire,
            Prize::Proximity,
            Prize::Super,
            Prize::Shields,
            Prize::Shrapnel,
            Prize::Antiwarp,
            Prize::Repel,
            Prize::Burst,
            Prize::Decoy,
            Prize::Thor,
            Prize::Multiprize,
            Prize::Brick,
            Prize::Rocket,
            Prize::Portal,
        ];
        let value = value.abs() as usize;

        if value >= PRIZE_ID_MAP.len() {
            Err(PrizeError::InvalidPrizeId)
        } else {
            Ok(PRIZE_ID_MAP[value])
        }
    }
}

pub fn generate_prize_id(settings: &ArenaSettings, prize_seed: i32, negative_allowed: bool) -> i32 {
    let total_weight = settings.prize_weights.calculate_total_weight();

    if total_weight == 0 {
        return 0;
    }

    let weights = settings.prize_weights.get_weights();

    let mut rng = VieRng::new(prize_seed);
    let mut random = rng.next() as u32;
    let mut result = 0;
    let mut weight = 0;

    for prize_id in 0..weights.len() {
        weight += weights[prize_id] as u32;

        if random % total_weight < weight {
            random = rng.next();

            if !negative_allowed
                || settings.prize_negative_factor == 0
                || random % settings.prize_negative_factor as u32 != 0
            {
                result = (prize_id + 1) as i32;
                break;
            }

            result = (prize_id + 1) as i32 * -1;
            break;
        }
    }

    result
}

fn is_valid_multiprize_id(random_prize: i32) -> bool {
    const ID_ENGINE_SHUTDOWN: i32 = 14;
    const ID_SHIELDS: i32 = 18;
    const ID_SUPER: i32 = 17;
    const ID_MULTIPRIZE: i32 = 25;
    const ID_WARP: i32 = 7;

    let invalid_prize = random_prize == 0
        || random_prize == ID_ENGINE_SHUTDOWN
        || random_prize == ID_SHIELDS
        || random_prize == ID_SUPER
        || random_prize == ID_MULTIPRIZE
        || random_prize == ID_WARP;

    !invalid_prize
}

pub fn apply_random_prizes(settings: &ArenaSettings, ship: &mut Ship, tick: GameTick, count: i32) {
    let mut rng = VieRng::new(tick.value() as i32);

    let mut applied = 0;

    for _ in 0..9999 {
        let random_prize = generate_prize_id(settings, rng.next() as i32, false);

        if is_valid_multiprize_id(random_prize) {
            if let Ok(_) = apply_prize_id(settings, ship, tick, random_prize) {
                applied += 1;
            }
        }

        if applied >= count {
            break;
        }
    }
}

pub fn apply_prize_id(
    settings: &ArenaSettings,
    ship: &mut Ship,
    tick: GameTick,
    prize_id: i32,
) -> Result<(Prize, bool), PrizeError> {
    let mut prize = Prize::try_from(prize_id)?;
    let negative = prize_id < 0;

    let ship_settings = settings.get_ship_settings(ship.kind);

    match &prize {
        Prize::None => {
            let mut rng = VieRng::new(tick.value() as i32);

            for _ in 0..9999 {
                let random_prize = generate_prize_id(settings, rng.next() as i32, false);

                if is_valid_multiprize_id(random_prize) {
                    apply_prize_id(settings, ship, tick, random_prize)?;
                    break;
                }
            }
        }
        Prize::Recharge => {
            if negative {
                ship.recharge = ship
                    .recharge
                    .saturating_sub(ship_settings.upgrade_recharge as u32);
                if ship.recharge < ship_settings.initial_recharge as u32 {
                    ship.recharge = ship_settings.initial_recharge as u32;
                }
            } else {
                ship.recharge = ship.recharge + (ship_settings.upgrade_recharge as u32);

                if ship.recharge > ship_settings.maximum_recharge as u32 {
                    ship.recharge = ship_settings.maximum_recharge as u32;
                }
            }
        }
        Prize::Energy => {
            if negative {
                ship.max_energy = ship
                    .max_energy
                    .saturating_sub(ship_settings.upgrade_energy as u32 * 1000);
                if ship.max_energy < ship_settings.initial_energy as u32 * 1000 {
                    ship.max_energy = ship_settings.initial_energy as u32 * 1000;
                }
            } else {
                ship.max_energy = ship.max_energy + (ship_settings.upgrade_energy as u32 * 1000);

                if ship.max_energy > ship_settings.maximum_energy as u32 * 1000 {
                    ship.max_energy = ship_settings.maximum_energy as u32 * 1000;
                }
            }
        }
        Prize::Rotation => {
            if negative {
                ship.rotation = ship
                    .rotation
                    .saturating_sub(ship_settings.upgrade_rotation as u32);
                if ship.rotation < ship_settings.initial_rotation as u32 {
                    ship.rotation = ship_settings.initial_rotation as u32;
                }
            } else {
                ship.rotation = ship.rotation + (ship_settings.upgrade_rotation as u32);

                if ship.rotation > ship_settings.maximum_rotation as u32 {
                    ship.rotation = ship_settings.maximum_rotation as u32;
                }
            }
        }
        Prize::Stealth => {
            if ship_settings.stealth_status == 0 {
                prize = Prize::FullCharge;
            } else {
                if negative {
                    ship.capability &= !ShipCapabilityFlag::Stealth;
                } else {
                    ship.capability |= ShipCapabilityFlag::Stealth;
                }
            }
        }
        Prize::Cloak => {
            if ship_settings.cloak_status == 0 {
                prize = Prize::FullCharge;
            } else {
                if negative {
                    ship.capability &= !ShipCapabilityFlag::Cloak;
                } else {
                    ship.capability |= ShipCapabilityFlag::Cloak;
                }
            }
        }
        Prize::XRadar => {
            if ship_settings.xradar_status == 0 {
                prize = Prize::FullCharge;
            } else {
                if negative {
                    ship.capability &= !ShipCapabilityFlag::XRadar;
                } else {
                    ship.capability |= ShipCapabilityFlag::XRadar;
                }
            }
        }
        Prize::Warp => {
            if negative {
                prize = Prize::FullCharge;
            } else {
                // This should be handled outside of this function so it can easily access the player for warping.
            }
        }
        Prize::Guns => {
            if negative {
                ship.guns = ship.guns.saturating_sub(1);

                if ship.guns < ship_settings.initial_guns {
                    ship.guns = ship_settings.initial_guns;
                }
            } else {
                ship.guns = ship.guns.saturating_add(1);

                if ship.guns > ship_settings.max_guns {
                    ship.guns = ship_settings.max_guns;
                }
            }
        }
        Prize::Bombs => {
            if negative {
                ship.bombs = ship.bombs.saturating_sub(1);

                if ship.bombs < ship_settings.initial_bombs {
                    ship.bombs = ship_settings.initial_bombs;
                }
            } else {
                ship.bombs = ship.bombs.saturating_add(1);

                if ship.bombs > ship_settings.max_bombs {
                    ship.bombs = ship_settings.max_bombs;
                }
            }
        }
        Prize::BouncingBullets => {
            if negative {
                ship.capability &= !ShipCapabilityFlag::BouncingBullets;
            } else {
                ship.capability |= ShipCapabilityFlag::BouncingBullets;
            }
        }
        Prize::Thruster => {
            if negative {
                ship.thrust = ship
                    .thrust
                    .saturating_sub(ship_settings.upgrade_thrust as u32);
                if ship.thrust < ship_settings.initial_thrust as u32 {
                    ship.thrust = ship_settings.initial_thrust as u32;
                }
            } else {
                ship.thrust = ship.thrust + (ship_settings.upgrade_thrust as u32);

                if ship.thrust > ship_settings.maximum_thrust as u32 {
                    ship.thrust = ship_settings.maximum_thrust as u32;
                }
            }
        }
        Prize::TopSpeed => {
            if negative {
                ship.speed = ship
                    .speed
                    .saturating_sub(ship_settings.upgrade_speed as u32);
                if ship.speed < ship_settings.initial_speed as u32 {
                    ship.speed = ship_settings.initial_speed as u32;
                }
            } else {
                ship.speed = ship.speed + (ship_settings.upgrade_speed as u32);

                if ship.speed > ship_settings.maximum_speed as u32 {
                    ship.speed = ship_settings.maximum_speed as u32;
                }
            }
        }
        Prize::EngineShutdown => {
            let mut shutdown_ticks = settings.engine_shutdown_time as i32;

            if negative {
                shutdown_ticks *= 3;
            }

            ship.shutdown_end_tick = Some(tick + shutdown_ticks);
        }
        Prize::Multifire => {
            if negative {
                ship.capability &= !ShipCapabilityFlag::Multifire;
            } else {
                ship.capability |= ShipCapabilityFlag::Multifire;
            }
        }
        Prize::Proximity => {
            if negative {
                ship.capability &= !ShipCapabilityFlag::Proximity;
            } else {
                ship.capability |= ShipCapabilityFlag::Proximity;
            }
        }
        Prize::Super => {
            let mut rng = VieRng::new(tick.value() as i32);

            if ship_settings.super_time > 0 {
                let super_ticks = rng.next() % ship_settings.super_time;

                if super_ticks > ship.super_remaining_ticks {
                    ship.super_remaining_ticks = super_ticks;
                }
            }
        }
        Prize::Shields => {
            ship.shield_remaining_ticks = ship_settings.shield_time;
        }
        Prize::Shrapnel => {
            if negative {
                ship.shrapnel = ship.shrapnel.saturating_sub(ship_settings.shrapnel_rate);
            } else {
                ship.shrapnel = ship.shrapnel.saturating_add(ship_settings.shrapnel_rate);

                if ship.shrapnel > ship_settings.max_shrapnel {
                    ship.shrapnel = ship_settings.max_shrapnel;
                }
            }
        }
        Prize::Antiwarp => {
            if ship_settings.antiwarp_status == 0 {
                prize = Prize::FullCharge;
            } else {
                if negative {
                    ship.capability &= !ShipCapabilityFlag::Antiwarp;
                } else {
                    ship.capability |= ShipCapabilityFlag::Antiwarp;
                }
            }
        }
        Prize::Repel => {
            if negative {
                ship.repels = ship.repels.saturating_sub(1);
            } else {
                ship.repels = ship.repels.saturating_add(1);

                if ship.repels > ship_settings.max_repel {
                    ship.repels = ship_settings.max_repel;
                }
            }
        }
        Prize::Burst => {
            if negative {
                ship.bursts = ship.bursts.saturating_sub(1);
            } else {
                ship.bursts = ship.bursts.saturating_add(1);

                if ship.bursts > ship_settings.max_burst {
                    ship.bursts = ship_settings.max_burst;
                }
            }
        }
        Prize::Decoy => {
            if negative {
                ship.decoys = ship.decoys.saturating_sub(1);
            } else {
                ship.decoys = ship.decoys.saturating_add(1);

                if ship.decoys > ship_settings.max_decoy {
                    ship.decoys = ship_settings.max_decoy;
                }
            }
        }
        Prize::Thor => {
            if negative {
                ship.thors = ship.thors.saturating_sub(1);
            } else {
                ship.thors = ship.thors.saturating_add(1);

                if ship.thors > ship_settings.max_thor {
                    ship.thors = ship_settings.max_thor;
                }
            }
        }
        Prize::Multiprize => {
            let count = settings.multi_prize_count as usize;
            let mut rng = VieRng::new(tick.value() as i32);

            for _ in 0..count {
                for _ in 0..9999 {
                    let random_prize = generate_prize_id(settings, rng.next() as i32, false);

                    if is_valid_multiprize_id(random_prize) {
                        apply_prize_id(settings, ship, tick, random_prize)?;
                        break;
                    }
                }
            }
        }
        Prize::Brick => {
            if negative {
                ship.bricks = ship.bricks.saturating_sub(1);
            } else {
                ship.bricks = ship.bricks.saturating_add(1);

                if ship.bricks > ship_settings.max_brick {
                    ship.bricks = ship_settings.max_brick;
                }
            }
        }
        Prize::Rocket => {
            if negative {
                ship.rockets = ship.rockets.saturating_sub(1);
            } else {
                ship.rockets = ship.rockets.saturating_add(1);

                if ship.rockets > ship_settings.max_rocket {
                    ship.rockets = ship_settings.max_rocket;
                }
            }
        }
        Prize::Portal => {
            if negative {
                ship.portals = ship.portals.saturating_sub(1);
            } else {
                ship.portals = ship.portals.saturating_add(1);

                if ship.portals > ship_settings.max_portal {
                    ship.portals = ship_settings.max_portal;
                }
            }
        }
        _ => {}
    }

    if let Prize::FullCharge = prize {
        if negative {
            ship.current_energy = 1;
        } else {
            ship.current_energy = ship.max_energy;
        }
    }

    Ok((prize, negative))
}
