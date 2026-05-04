use crate::{
    arena_settings::ArenaSettings,
    clock::GameTick,
    math::{PixelUnit, Position, PositionUnit, Velocity},
    net::packet::s2c::PowerballPositionMessage,
    player::{PlayerId, PlayerManager},
    radar::{IndicatorFlag, Radar},
    ship::ShipKind,
};

const MAX_BALL_COUNT: usize = 8;
const BALL_START_FRICTION: u32 = 1000000;

#[derive(Copy, Clone, PartialEq)]
pub enum PowerballState {
    Invalid,
    World,
    Carried,
}

pub struct Powerball {
    pub carrier_id: PlayerId,
    pub frequency: u16,

    pub friction_delta: i16,
    pub friction: u32,

    pub position: Position,
    pub velocity: Velocity,

    pub timestamp: GameTick,

    pub state: PowerballState,

    // When a player is near the ball and could possibly pick the ball up,
    // this gets set to a number of ticks to make the ball invisible/phased.
    pub remaining_pickup_ticks: u32,

    pub current_sim_tick: GameTick,
    pub last_trail_tick: GameTick,
}

impl Powerball {
    pub fn empty() -> Self {
        Self {
            carrier_id: PlayerId::invalid(),
            frequency: 0xFFFF,
            friction_delta: 0,
            friction: 0,
            position: Position::empty(),
            velocity: Velocity::empty(),
            timestamp: GameTick::empty(),
            state: PowerballState::Invalid,
            remaining_pickup_ticks: 0,
            current_sim_tick: GameTick::empty(),
            last_trail_tick: GameTick::empty(),
        }
    }

    pub fn is_phasing(&self, current_tick: GameTick, pass_delay: i32) -> bool {
        self.remaining_pickup_ticks > 0 || current_tick.diff(&self.timestamp) < pass_delay
    }
}

#[derive(Copy, Clone)]
pub struct CarryState {
    pub ball_id: usize,

    pub remaining_ticks: u32,
}

pub struct PowerballManager {
    pub balls: [Powerball; MAX_BALL_COUNT],
    pub carry_state: Option<CarryState>,
}

impl PowerballManager {
    pub const PICKUP_PHASE_TICKS: u32 = 100;

    pub fn new() -> Self {
        Self {
            balls: [(); MAX_BALL_COUNT].map(|_| Powerball::empty()),
            carry_state: None,
        }
    }

    pub fn clear_carry_state(&mut self) {
        self.carry_state = None;
    }

    pub fn get_carry_remaining_ticks(&self) -> Option<u32> {
        if let Some(carry_state) = &self.carry_state {
            return Some(carry_state.remaining_ticks);
        }

        None
    }

    pub fn tick_carry_state(&mut self) -> Option<u8> {
        if let Some(carry_state) = &mut self.carry_state {
            if carry_state.remaining_ticks > 0 {
                carry_state.remaining_ticks -= 1;
            }

            if carry_state.remaining_ticks == 0 {
                let ball_id = carry_state.ball_id as u8;

                self.carry_state = None;

                return Some(ball_id);
            }
        }

        None
    }

    pub fn get_ball_by_id(&self, ball_id: u8) -> Option<&Powerball> {
        if ball_id >= 8 {
            None
        } else {
            Some(&self.balls[ball_id as usize])
        }
    }

    pub fn get_ball_by_id_mut(&mut self, ball_id: u8) -> Option<&mut Powerball> {
        if ball_id >= 8 {
            None
        } else {
            Some(&mut self.balls[ball_id as usize])
        }
    }

    pub fn is_carrying_ball(&self, player_id: PlayerId) -> bool {
        for ball in &self.balls {
            if ball.carrier_id == player_id {
                return true;
            }
        }
        false
    }

    // Returns true if the ball was just picked up by us.
    pub fn on_ball_position_message(
        &mut self,
        player_manager: &mut PlayerManager,
        settings: &ArenaSettings,
        message: &PowerballPositionMessage,
    ) -> bool {
        if message.ball_id >= 8 {
            log::warn!("Got ball position for invalid ball {}", message.ball_id);
            return false;
        }

        let mut carry_state = self.carry_state.take();

        let Some(ball) = self.get_ball_by_id_mut(message.ball_id) else {
            return false;
        };

        let new_ball_world_position = ball.state == PowerballState::Invalid
            || message.timestamp > ball.timestamp
            || (ball.state == PowerballState::Carried && message.timestamp.value() != 0);

        let mut new_pickup = false;

        if new_ball_world_position {
            ball.carrier_id = message.owner_id;
            ball.timestamp = message.timestamp;
            ball.position =
                Position::from_pixels(PixelUnit(message.x as i32), PixelUnit(message.y as i32));
            ball.velocity = Velocity::new(
                PositionUnit(message.x_velocity as i32),
                PositionUnit(message.y_velocity as i32),
            );
            ball.frequency = 0xFFFF;
            ball.state = PowerballState::World;
            ball.current_sim_tick = message.timestamp;

            if let Some(carry) = &carry_state {
                if carry.ball_id == message.ball_id as usize {
                    carry_state = None;
                }
            }

            let mut carrier_ship_kind = crate::ship::ShipKind::Warbird;

            if let Some(carrier) = player_manager.get_by_id_mut(message.owner_id) {
                carrier_ship_kind = carrier.ship_kind;

                ball.frequency = carrier.frequency;
                ball.remaining_pickup_ticks = 0;

                carrier.carrying_ball = false;
            }

            ball.friction_delta = settings
                .get_ship_settings(carrier_ship_kind)
                .powerball_friction as i16;

            ball.friction = BALL_START_FRICTION;
        } else if message.timestamp.value() == 0 {
            if message.owner_id != PlayerId::invalid() {
                // Ball is carried if the timestamp is zero with a valid carrier id.
                ball.timestamp = message.timestamp;
                ball.carrier_id = message.owner_id;
                ball.position =
                    Position::from_pixels(PixelUnit(message.x as i32), PixelUnit(message.y as i32));
                ball.velocity = Velocity::empty();

                if ball.state != PowerballState::Carried
                    && ball.carrier_id == player_manager.self_id
                {
                    if let Some(me) = player_manager.get_self() {
                        if me.ship_kind != ShipKind::Spectator {
                            let remaining_ticks = settings
                                .get_ship_settings(me.ship_kind)
                                .powerball_throw_timer
                                as u32;

                            carry_state = Some(CarryState {
                                ball_id: message.ball_id as usize,
                                remaining_ticks,
                            });

                            new_pickup = true;
                        }
                    }
                }

                if let Some(carrier) = player_manager.get_by_id_mut(message.owner_id) {
                    ball.state = PowerballState::Carried;
                    ball.frequency = carrier.frequency;

                    carrier.carrying_ball = true;
                } else {
                    ball.state = PowerballState::Invalid;
                }
            } else {
                // Invalid player id and timestamp 0 means the ball no longer exists.
                ball.state = PowerballState::Invalid;

                if let Some(player) = player_manager.get_by_id_mut(ball.carrier_id) {
                    player.carrying_ball = false;
                }

                if let Some(carry) = &carry_state {
                    if carry.ball_id == message.ball_id as usize {
                        carry_state = None;
                    }
                }
            }
        }

        self.carry_state = carry_state;

        new_pickup
    }

    pub fn render_radar(
        &self,
        radar: &mut Radar,
        player_manager: &PlayerManager,
        view_freq: u16,
        current_tick: GameTick,
        global_display: bool,
    ) {
        let full_map = if global_display {
            IndicatorFlag::FullMap
        } else {
            0
        };

        for ball in &self.balls {
            match &ball.state {
                PowerballState::World => {
                    radar.add_indicator(
                        crate::render::colors::ColorRenderableKind::RadarBall,
                        ball.position,
                        current_tick,
                        full_map | IndicatorFlag::SmallMap,
                    );
                }
                PowerballState::Carried => {
                    let position = if let Some(carrier) = player_manager.get_by_id(ball.carrier_id)
                    {
                        if let Some(carrier_position) = carrier.position {
                            carrier_position
                        } else {
                            ball.position
                        }
                    } else {
                        ball.position
                    };
                    let kind = if ball.frequency == view_freq {
                        crate::render::colors::ColorRenderableKind::RadarTeammateFlagCarry
                    } else {
                        crate::render::colors::ColorRenderableKind::RadarEnemyFlagCarry
                    };

                    radar.add_indicator(
                        kind,
                        position,
                        current_tick,
                        full_map | IndicatorFlag::SmallMap,
                    );
                }
                _ => {}
            }
        }
    }
}

pub fn is_team_goal(powerball_mode: u8, position: Position, frequency: u16) -> bool {
    let x = position.x.0 / 16000;
    let y = position.y.0 / 16000;

    match powerball_mode {
        0 => false,
        1 => {
            if frequency & 1 != 0 {
                x >= 512
            } else {
                x < 512
            }
        }
        2 => {
            if frequency & 1 != 0 {
                y >= 512
            } else {
                y < 512
            }
        }
        3 => is_team_goal_mode3(position, frequency),
        4 => !is_team_goal_mode3(position, frequency),
        5 => is_team_goal_mode5(position, frequency),
        6 => !is_team_goal_mode5(position, frequency),
        _ => false,
    }
}

fn is_team_goal_mode3(position: Position, frequency: u16) -> bool {
    let corner = frequency % 4;

    let x = position.x.0 / 16000;
    let y = position.y.0 / 16000;

    match corner {
        0 => x < 512 && y < 512,
        1 => y >= 512 && y < 512,
        2 => x < 512 && y >= 512,
        3 => x >= 512 && y >= 512,
        _ => false,
    }
}

fn is_team_goal_mode5(position: Position, frequency: u16) -> bool {
    let direction = frequency % 4;

    let x = position.x.0 / 16000;
    let y = position.y.0 / 16000;

    match direction {
        0 => {
            if y < 512 {
                x < y
            } else {
                x + y < 1024
            }
        }
        1 => {
            if x < 512 {
                x + y >= 1024
            } else {
                x < y
            }
        }
        2 => {
            if x < 512 {
                x >= y
            } else {
                x + y < 1024
            }
        }
        3 => {
            if y <= 512 {
                x + y >= 1024
            } else {
                x >= y
            }
        }
        _ => false,
    }
}
