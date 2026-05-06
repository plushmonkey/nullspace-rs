use thiserror::Error;

use crate::{
    arena_settings::ArenaSettings,
    clock::GameTick,
    map::Map,
    math::{Position, Rectangle},
    net::connection::Connection,
    notification::NotificationManager,
    player::PlayerManager,
    radar::{IndicatorFlag, Radar},
    render::{
        animation_renderer::get_animation_index,
        colors::ColorRenderableKind,
        game_sprites::{GameSpriteKind, GameSprites},
        layer::Layer,
        render_state::RenderState,
        text_renderer::TextColor,
    },
    rng::VieRng,
    ship::{Ship, ShipCapabilityFlag, ShipKind},
    ship_controller::ShipController,
};

#[derive(Copy, Clone, PartialEq, Debug)]
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

pub struct PrizeGreen {
    pub remaining_ticks: u32,
    pub x_tile: u16,
    pub y_tile: u16,
    pub prize_id: i32,
}

pub struct PrizeManager {
    pub greens: Vec<PrizeGreen>,
    pub seed: Option<i32>,
    pub spawn_tick_counter: u32,
}

impl PrizeManager {
    pub fn new() -> Self {
        Self {
            greens: vec![],
            seed: None,
            spawn_tick_counter: 0,
        }
    }

    pub fn set_seed(&mut self, new_seed: i32) {
        self.seed = Some(new_seed);
        self.spawn_tick_counter = 0;
    }

    pub fn tick(
        &mut self,
        player_manager: &mut PlayerManager,
        settings: &ArenaSettings,
        map: &Map,
        connection: &mut Connection,
        notifications: &mut NotificationManager,
        ship_controller: &mut Option<&mut ShipController>,
    ) {
        self.perform_collisions(
            player_manager,
            settings,
            connection,
            map,
            notifications,
            ship_controller,
        );
        self.expire_greens();
        self.spawn_prizes(settings, map, player_manager.players.len());
    }

    fn perform_collisions(
        &mut self,
        player_manager: &mut PlayerManager,
        settings: &ArenaSettings,
        connection: &mut Connection,
        map: &Map,
        notifications: &mut NotificationManager,
        ship_controller: &mut Option<&mut ShipController>,
    ) {
        let current_tick = connection.get_game_tick();
        let player_count = player_manager.players.len();

        for player in &mut player_manager.players {
            if player.ship_kind == ShipKind::Spectator || player.is_dead() {
                continue;
            }

            let player_collider =
                player.get_collider(settings.get_ship_settings(player.ship_kind).get_radius());

            for green in &mut self.greens {
                if green.remaining_ticks == 0 {
                    continue;
                }

                let green_collider = Rectangle::new(
                    Position::from_tile(green.x_tile as i32, green.y_tile as i32),
                    Position::from_tile(green.x_tile as i32 + 1, green.y_tile as i32 + 1),
                );

                if green_collider.intersects(&player_collider) {
                    if player.id == player_manager.self_id {
                        if let Some(ship_controller) = ship_controller {
                            if let Err(e) = apply_prize_id(
                                settings,
                                &mut ship_controller.ship,
                                current_tick,
                                green.prize_id,
                                Some(notifications),
                            ) {
                                log::error!("{e}");
                            }

                            ship_controller.ship.bounty =
                                ship_controller.ship.bounty.wrapping_add(1);

                            if green.prize_id == Prize::Warp as i32 {
                                ship_controller.warp_with_energy_loss(
                                    player,
                                    settings,
                                    map,
                                    current_tick,
                                    player_count,
                                );
                            }

                            let message = crate::net::packet::c2s::TakePrizeMessage {
                                timestamp: current_tick,
                                x: green.x_tile,
                                y: green.y_tile,
                                prize: green.prize_id as i16,
                            };
                            if let Err(e) = connection.send_reliable(&message) {
                                log::error!("{e}");
                            }
                        }
                    }

                    green.remaining_ticks = 0;
                }
            }
        }
    }

    fn spawn_prizes(&mut self, settings: &ArenaSettings, map: &Map, player_count: usize) {
        let Some(seed) = self.seed else {
            return;
        };

        self.spawn_tick_counter = self.spawn_tick_counter.wrapping_add(1);
        let max_greens = (settings.prize_factor as usize * player_count) / 1000;

        if settings.prize_delay > 0 && self.spawn_tick_counter >= settings.prize_delay as u32 {
            let spawn_extent = ((settings.minimum_virtual as usize
                + settings.upgrade_virtual as usize * player_count)
                as u32)
                .clamp(3, 1024);

            let mut rng = VieRng::new(seed);

            for _ in 0..settings.prize_hide_count {
                let x_rng = rng.next();
                let y_rng = rng.next();

                let x = ((x_rng % (spawn_extent - 2)) + 1 + ((1024 - spawn_extent) / 2)) as u16;
                let y = ((y_rng % (spawn_extent - 2)) + 1 + ((1024 - spawn_extent) / 2)) as u16;

                let prize_id = generate_prize_id(settings, &mut rng, true);

                let duration_rng = rng.next();

                let duration_range =
                    (settings.prize_max_exist - settings.prize_min_exist).max(0) as u32;
                let duration =
                    (duration_rng % (duration_range + 1)) + settings.prize_min_exist as u32;

                if self.greens.len() < max_greens && map.get_tile(x, y) == 0 {
                    self.spawn_green(x, y, prize_id, duration);
                }
            }

            self.set_seed(rng.seed);
        }
    }

    pub fn spawn_green(&mut self, x_tile: u16, y_tile: u16, prize_id: i32, duration: u32) {
        self.greens.push(PrizeGreen {
            remaining_ticks: duration,
            x_tile,
            y_tile,
            prize_id,
        });
    }

    pub fn on_prize_collected(&mut self, x: u16, y: u16) {
        for i in 0..self.greens.len() {
            if self.greens[i].x_tile == x && self.greens[i].y_tile == y {
                self.greens.remove(i);
                return;
            }
        }
    }

    fn expire_greens(&mut self) {
        let mut green_index = 0;

        loop {
            if green_index >= self.greens.len() {
                break;
            }

            if self.greens[green_index].remaining_ticks > 0 {
                self.greens[green_index].remaining_ticks -= 1;
            }

            if self.greens[green_index].remaining_ticks == 0 {
                self.greens.swap_remove(green_index);
                continue;
            }

            green_index += 1;
        }
    }

    pub fn render(
        &self,
        render_state: &mut RenderState,
        sprites: &GameSprites,
        radar: &mut Radar,
        current_tick: GameTick,
    ) {
        let Some(prize_sprites) = sprites.get_set(GameSpriteKind::Prize) else {
            return;
        };

        let animation_index = get_animation_index(current_tick.value(), 10, 10 * 10);
        let renderable = &prize_sprites.renderables[animation_index];

        for green in &self.greens {
            let x_pixels = green.x_tile as i32 * 16;
            let y_pixels = green.y_tile as i32 * 16;

            render_state.sprite_renderer.draw(
                &render_state.camera,
                renderable,
                x_pixels,
                y_pixels,
                Layer::AfterTiles,
            );

            let position = Position::from_tile(green.x_tile as i32, green.y_tile as i32);

            radar.add_indicator(
                ColorRenderableKind::RadarPrize,
                position,
                current_tick,
                IndicatorFlag::SmallMap,
            );
        }
    }
}

pub fn generate_prize_id(
    settings: &ArenaSettings,
    rng: &mut VieRng,
    negative_allowed: bool,
) -> i32 {
    let total_weight = settings.prize_weights.calculate_total_weight();

    if total_weight == 0 {
        return 0;
    }

    let weights = settings.prize_weights.get_weights();

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
        let random_prize = generate_prize_id(settings, &mut rng, false);

        if is_valid_multiprize_id(random_prize) {
            if let Ok(_) = apply_prize_id(settings, ship, tick, random_prize, None) {
                applied += 1;
            }
        }

        if applied >= count {
            break;
        }
    }
}

const POSITIVE_NOTIFICATIONS: [&'static str; 29] = [
    "",
    "Charge rate increased.",
    "Maximum energy level increased.",
    "Rotation speed increased.",
    "Stealth available.",
    "Cloak available.",
    "X-Radar available.",
    "Warp!",
    "Guns upgraded.",
    "Bombs upgraded.",
    "Bouncing bullets.",
    "Thrusters upgraded.",
    "Top speed increased.",
    "Full charge.",
    "Engines shut-down.",
    "MultiFire bullets.",
    "Proximity bombs.",
    "Temporary SuperPower!",
    "Temporary Shields.",
    "Shrapnel increased.",
    "AntiWarp available.",
    "Repeller increased.",
    "Burst increased.",
    "Decoy increased.",
    "Thor's hammer increased.",
    "MultiPrize!",
    "Brick increased.",
    "Rocket Increased.",
    "Portal increased.",
];

const NEGATIVE_NOTIFICATIONS: [&'static str; 29] = [
    "",
    "Charge rate decreased.",
    "Maximum energy level decreased.",
    "Rotation speed decreased.",
    "Stealth lost.",
    "Cloak lost.",
    "X-Radar lost.",
    "Warp!",
    "Guns downgraded.",
    "Bombs downgraded.",
    "Bouncing bullets lost.",
    "Thrusters downgraded.",
    "Top speed reduced.",
    "Energy depleted.",
    "Engines shut-down (severe).",
    "MultiFire lost.",
    "Proximity bombs lost.",
    "",
    "",
    "Shrapnel reduced.",
    "AntiWarp lost.",
    "Repeller lost.",
    "Burst lost.",
    "Decoy lost.",
    "Thor's hammer lost.",
    "",
    "Brick lost.",
    "Rocket lost.",
    "Portal lost.",
];

pub fn apply_prize_id(
    settings: &ArenaSettings,
    ship: &mut Ship,
    tick: GameTick,
    prize_id: i32,
    notifications: Option<&mut NotificationManager>,
) -> Result<(Prize, bool), PrizeError> {
    let mut prize = Prize::try_from(prize_id)?;
    let negative = prize_id < 0;

    let ship_settings = settings.get_ship_settings(ship.kind);

    let mut max_notification = false;

    match &prize {
        Prize::None => {
            let mut rng = VieRng::new(tick.value() as i32);

            for _ in 0..9999 {
                let random_prize = generate_prize_id(settings, &mut rng, false);

                if is_valid_multiprize_id(random_prize) {
                    apply_prize_id(settings, ship, tick, random_prize, None)?;
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
                    max_notification = true;
                }
            } else {
                ship.recharge = ship.recharge + (ship_settings.upgrade_recharge as u32);

                if ship.recharge > ship_settings.maximum_recharge as u32 {
                    ship.recharge = ship_settings.maximum_recharge as u32;
                    max_notification = true;
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
                    max_notification = true;
                }
            } else {
                ship.max_energy = ship.max_energy + (ship_settings.upgrade_energy as u32 * 1000);

                if ship.max_energy > ship_settings.maximum_energy as u32 * 1000 {
                    ship.max_energy = ship_settings.maximum_energy as u32 * 1000;
                    max_notification = true;
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
                    max_notification = true;
                }
            } else {
                ship.rotation = ship.rotation + (ship_settings.upgrade_rotation as u32);

                if ship.rotation > ship_settings.maximum_rotation as u32 {
                    ship.rotation = ship_settings.maximum_rotation as u32;
                    max_notification = true;
                }
            }
        }
        Prize::Stealth => {
            if ship_settings.stealth_status == 0 {
                prize = Prize::FullCharge;
            } else {
                if negative {
                    if ship.capability & ShipCapabilityFlag::Stealth != 0 {
                        max_notification = true;
                    }

                    ship.capability &= !ShipCapabilityFlag::Stealth;
                } else {
                    if ship.capability & ShipCapabilityFlag::Stealth == 0 {
                        max_notification = true;
                    }

                    ship.capability |= ShipCapabilityFlag::Stealth;
                }
            }
        }
        Prize::Cloak => {
            if ship_settings.cloak_status == 0 {
                prize = Prize::FullCharge;
            } else {
                if negative {
                    if ship.capability & ShipCapabilityFlag::Cloak != 0 {
                        max_notification = true;
                    }
                    ship.capability &= !ShipCapabilityFlag::Cloak;
                } else {
                    if ship.capability & ShipCapabilityFlag::Cloak == 0 {
                        max_notification = true;
                    }
                    ship.capability |= ShipCapabilityFlag::Cloak;
                }
            }
        }
        Prize::XRadar => {
            if ship_settings.xradar_status == 0 {
                prize = Prize::FullCharge;
            } else {
                if negative {
                    if ship.capability & ShipCapabilityFlag::XRadar != 0 {
                        max_notification = true;
                    }
                    ship.capability &= !ShipCapabilityFlag::XRadar;
                } else {
                    if ship.capability & ShipCapabilityFlag::XRadar == 0 {
                        max_notification = true;
                    }
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
                    max_notification = true;
                    ship.guns = ship_settings.initial_guns;
                }
            } else {
                ship.guns = ship.guns.saturating_add(1);

                if ship.guns > ship_settings.max_guns {
                    max_notification = true;
                    ship.guns = ship_settings.max_guns;
                }
            }
        }
        Prize::Bombs => {
            if negative {
                ship.bombs = ship.bombs.saturating_sub(1);

                if ship.bombs < ship_settings.initial_bombs {
                    max_notification = true;
                    ship.bombs = ship_settings.initial_bombs;
                }
            } else {
                ship.bombs = ship.bombs.saturating_add(1);

                if ship.bombs > ship_settings.max_bombs {
                    max_notification = true;
                    ship.bombs = ship_settings.max_bombs;
                }
            }
        }
        Prize::BouncingBullets => {
            if negative {
                if ship.capability & ShipCapabilityFlag::BouncingBullets != 0 {
                    max_notification = true;
                }

                ship.capability &= !ShipCapabilityFlag::BouncingBullets;
            } else {
                if ship.capability & ShipCapabilityFlag::BouncingBullets == 0 {
                    max_notification = true;
                }

                ship.capability |= ShipCapabilityFlag::BouncingBullets;
            }
        }
        Prize::Thruster => {
            if negative {
                ship.thrust = ship
                    .thrust
                    .saturating_sub(ship_settings.upgrade_thrust as u32);
                if ship.thrust < ship_settings.initial_thrust as u32 {
                    max_notification = true;
                    ship.thrust = ship_settings.initial_thrust as u32;
                }
            } else {
                ship.thrust = ship.thrust + (ship_settings.upgrade_thrust as u32);

                if ship.thrust > ship_settings.maximum_thrust as u32 {
                    max_notification = true;
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
                    max_notification = true;
                    ship.speed = ship_settings.initial_speed as u32;
                }
            } else {
                ship.speed = ship.speed + (ship_settings.upgrade_speed as u32);

                if ship.speed > ship_settings.maximum_speed as u32 {
                    max_notification = true;
                    ship.speed = ship_settings.maximum_speed as u32;
                }
            }
        }
        Prize::EngineShutdown => {
            let mut shutdown_ticks = settings.engine_shutdown_time as i32;

            if negative {
                shutdown_ticks *= 3;
            }

            if shutdown_ticks as u32 > ship.shutdown_remaining_ticks {
                ship.shutdown_remaining_ticks = shutdown_ticks as u32;
            }
        }
        Prize::Multifire => {
            if negative {
                if ship.capability & ShipCapabilityFlag::Multifire != 0 {
                    max_notification = true;
                }
                ship.capability &= !ShipCapabilityFlag::Multifire;
            } else {
                if ship.capability & ShipCapabilityFlag::Multifire == 0 {
                    max_notification = true;
                }
                ship.capability |= ShipCapabilityFlag::Multifire;
            }
        }
        Prize::Proximity => {
            if negative {
                if ship.capability & ShipCapabilityFlag::Proximity != 0 {
                    max_notification = true;
                }
                ship.capability &= !ShipCapabilityFlag::Proximity;
            } else {
                if ship.capability & ShipCapabilityFlag::Proximity == 0 {
                    max_notification = true;
                }
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
                if ship.shrapnel <= ship_settings.shrapnel_rate {
                    max_notification = true;
                }
                ship.shrapnel = ship.shrapnel.saturating_sub(ship_settings.shrapnel_rate);
            } else {
                ship.shrapnel = ship.shrapnel.saturating_add(ship_settings.shrapnel_rate);

                if ship.shrapnel > ship_settings.max_shrapnel {
                    ship.shrapnel = ship_settings.max_shrapnel;
                    max_notification = true;
                }
            }
        }
        Prize::Antiwarp => {
            if ship_settings.antiwarp_status == 0 {
                prize = Prize::FullCharge;
            } else {
                if negative {
                    if ship.capability & ShipCapabilityFlag::Antiwarp != 0 {
                        max_notification = true;
                    }
                    ship.capability &= !ShipCapabilityFlag::Antiwarp;
                } else {
                    if ship.capability & ShipCapabilityFlag::Antiwarp == 0 {
                        max_notification = true;
                    }
                    ship.capability |= ShipCapabilityFlag::Antiwarp;
                }
            }
        }
        Prize::Repel => {
            if negative {
                if ship.repels == 1 {
                    max_notification = true;
                }
                ship.repels = ship.repels.saturating_sub(1);
            } else {
                ship.repels = ship.repels.saturating_add(1);

                if ship.repels > ship_settings.max_repel {
                    max_notification = true;
                    ship.repels = ship_settings.max_repel;
                }
            }
        }
        Prize::Burst => {
            if negative {
                if ship.bursts == 1 {
                    max_notification = true;
                }
                ship.bursts = ship.bursts.saturating_sub(1);
            } else {
                ship.bursts = ship.bursts.saturating_add(1);

                if ship.bursts > ship_settings.max_burst {
                    max_notification = true;
                    ship.bursts = ship_settings.max_burst;
                }
            }
        }
        Prize::Decoy => {
            if negative {
                if ship.decoys == 1 {
                    max_notification = true;
                }
                ship.decoys = ship.decoys.saturating_sub(1);
            } else {
                ship.decoys = ship.decoys.saturating_add(1);

                if ship.decoys > ship_settings.max_decoy {
                    max_notification = true;
                    ship.decoys = ship_settings.max_decoy;
                }
            }
        }
        Prize::Thor => {
            if negative {
                if ship.thors == 1 {
                    max_notification = true;
                }
                ship.thors = ship.thors.saturating_sub(1);
            } else {
                ship.thors = ship.thors.saturating_add(1);

                if ship.thors > ship_settings.max_thor {
                    max_notification = true;
                    ship.thors = ship_settings.max_thor;
                }
            }
        }
        Prize::Multiprize => {
            let count = settings.multi_prize_count as usize;
            let mut rng = VieRng::new(tick.value() as i32);

            for _ in 0..count {
                for _ in 0..9999 {
                    let random_prize = generate_prize_id(settings, &mut rng, false);

                    if is_valid_multiprize_id(random_prize) {
                        apply_prize_id(settings, ship, tick, random_prize, None)?;
                        break;
                    }
                }
            }
        }
        Prize::Brick => {
            if negative {
                if ship.bricks == 1 {
                    max_notification = true;
                }
                ship.bricks = ship.bricks.saturating_sub(1);
            } else {
                ship.bricks = ship.bricks.saturating_add(1);

                if ship.bricks > ship_settings.max_brick {
                    ship.bricks = ship_settings.max_brick;
                    max_notification = true;
                }
            }
        }
        Prize::Rocket => {
            if negative {
                if ship.rockets == 1 {
                    max_notification = true;
                }
                ship.rockets = ship.rockets.saturating_sub(1);
            } else {
                ship.rockets = ship.rockets.saturating_add(1);

                if ship.rockets > ship_settings.max_rocket {
                    max_notification = true;
                    ship.rockets = ship_settings.max_rocket;
                }
            }
        }
        Prize::Portal => {
            if negative {
                if ship.portals == 1 {
                    max_notification = true;
                }
                ship.portals = ship.portals.saturating_sub(1);
            } else {
                ship.portals = ship.portals.saturating_add(1);

                if ship.portals > ship_settings.max_portal {
                    max_notification = true;
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

    if prize != Prize::None {
        if let Some(notifications) = notifications {
            let max_str = if max_notification {
                if negative { " MIN" } else { " MAX" }
            } else {
                ""
            };
            let mesg = if negative {
                NEGATIVE_NOTIFICATIONS[prize as usize]
            } else {
                POSITIVE_NOTIFICATIONS[prize as usize]
            };

            // TODO: Damage

            notifications.push(format!("{}{}", mesg, max_str), TextColor::Green);
        }
    }

    Ok((prize, negative))
}
