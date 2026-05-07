use crate::{
    math::{PixelUnit, Position, PositionUnit, Velocity},
    render::{
        game_sprites::{GameSpriteKind, GameSprites},
        layer::Layer,
        render_state::RenderState,
        sprite_renderer::SpriteRenderable,
    },
};

const FRAME_COUNT: u32 = 19;
const TICKS_PER_FRAME: u32 = 2;
const MOVE_DURATION: u32 = TICKS_PER_FRAME * 5;
const DURATION: u32 = FRAME_COUNT * TICKS_PER_FRAME;

pub struct ExhaustAnimation {
    pub remaining_ticks: u32,
    pub position: Position,
    pub velocity: Velocity,
    pub heading: glam::Vec2,
    pub thrust: glam::Vec2,
    pub rocket: bool,
}

impl ExhaustAnimation {
    fn get_positions(&self) -> (Position, Position, Position) {
        let elapsed_ticks = (DURATION - self.remaining_ticks).min(MOVE_DURATION) as i32;
        let move_percent = elapsed_ticks as f32 / MOVE_DURATION as f32;

        let side = self.heading.perp();
        let spread_amount = if self.rocket { 9.0f32 } else { 4.0f32 };
        let spread = 3.0f32 + move_percent * spread_amount;

        let center = self.position;

        let left_position = Position::new(
            PositionUnit(center.x.0 - ((side.x * spread) * 1000.0f32) as i32),
            PositionUnit(center.y.0 - ((side.y * spread) * 1000.0f32) as i32),
        );

        let right_position = Position::new(
            PositionUnit(center.x.0 + ((side.x * spread) * 1000.0f32) as i32),
            PositionUnit(center.y.0 + ((side.y * spread) * 1000.0f32) as i32),
        );

        let center_position = Position::new(
            PositionUnit(center.x.0 + (self.thrust.x * 0.75f32) as i32),
            PositionUnit(center.y.0 + (self.thrust.y * 0.75f32) as i32),
        );

        (left_position, right_position, center_position)
    }
}

pub struct ExhaustController {
    pub exhaust_animations: Vec<ExhaustAnimation>,
    pub exhaust_cooldown_ticks: u32,
}

impl ExhaustController {
    pub fn new() -> Self {
        Self {
            exhaust_animations: vec![],
            exhaust_cooldown_ticks: 0,
        }
    }

    pub fn tick(&mut self) {
        if self.exhaust_cooldown_ticks > 0 {
            self.exhaust_cooldown_ticks -= 1;
        }

        self.tick_timers();

        for animation in &mut self.exhaust_animations {
            let elapsed_ticks = DURATION - animation.remaining_ticks;

            if elapsed_ticks < MOVE_DURATION {
                if animation.remaining_ticks % TICKS_PER_FRAME == 0 {
                    animation.position.x.0 += animation.velocity.x.0 * TICKS_PER_FRAME as i32;
                    animation.position.y.0 += animation.velocity.y.0 * TICKS_PER_FRAME as i32;

                    animation.velocity.x.0 /= 2;
                    animation.velocity.y.0 /= 2;
                }
            }
        }
    }

    fn tick_timers(&mut self) {
        let mut index = 0;
        loop {
            if index >= self.exhaust_animations.len() {
                break;
            }

            let animation = &mut self.exhaust_animations[index];
            if animation.remaining_ticks > 0 {
                animation.remaining_ticks -= 1;
            }

            if animation.remaining_ticks == 0 {
                self.exhaust_animations.swap_remove(index);
                continue;
            }

            index += 1;
        }
    }

    pub fn drop_exhaust(
        &mut self,
        heading: glam::Vec2,
        position: Position,
        velocity: Velocity,
        radius: i32,
        rocket: bool,
        reverse: bool,
    ) {
        if self.exhaust_cooldown_ticks > 0 {
            return;
        }

        self.exhaust_cooldown_ticks = 8;

        let x = position.x.0 / 1000;
        let y = position.y.0 / 1000;

        let center_back = glam::Vec2::new(
            x as f32 - (heading.x * (radius + 1) as f32),
            y as f32 - (heading.y * (radius + 1) as f32),
        );

        let start_position = Position::from_pixels(
            PixelUnit(center_back.x as i32),
            PixelUnit(center_back.y as i32),
        );

        let mut thrust = heading * -1600.0f32 * (TICKS_PER_FRAME as f32);

        if reverse {
            thrust = -thrust;
        }

        let velocity = Velocity::new(
            PositionUnit(thrust.x as i32 + velocity.x.0),
            PositionUnit(thrust.y as i32 + velocity.y.0),
        );

        self.exhaust_animations.push(ExhaustAnimation {
            remaining_ticks: DURATION,
            position: start_position,
            velocity,
            heading,
            thrust,
            rocket,
        });
    }

    pub fn render(&self, render_state: &mut RenderState, sprites: &GameSprites) {
        let Some(exhaust_sprites) = sprites.get_set(GameSpriteKind::Exhaust) else {
            return;
        };

        let Some(rocket_sprites) = sprites.get_set(GameSpriteKind::Rocket) else {
            return;
        };

        for animation in &self.exhaust_animations {
            let (left_position, right_position, center_position) = animation.get_positions();
            let elapsed = DURATION - animation.remaining_ticks;

            let renderable = if animation.rocket {
                let frame = (elapsed / TICKS_PER_FRAME).min(25);

                &rocket_sprites.renderables[frame as usize]
            } else {
                let frame = (elapsed / TICKS_PER_FRAME).min(18);

                &exhaust_sprites.renderables[frame as usize]
            };

            Self::draw(
                render_state,
                renderable,
                left_position,
                animation.remaining_ticks,
            );
            Self::draw(
                render_state,
                renderable,
                right_position,
                animation.remaining_ticks,
            );

            if animation.rocket {
                Self::draw(
                    render_state,
                    renderable,
                    center_position,
                    animation.remaining_ticks,
                );
            }
        }
    }

    fn draw(
        render_state: &mut RenderState,
        renderable: &SpriteRenderable,
        position: Position,
        remaining_ticks: u32,
    ) {
        let (x, y) = position.to_pixels();
        let z_offset = remaining_ticks as f32 / DURATION as f32;

        render_state.sprite_renderer.draw_centered_with_z(
            &render_state.camera,
            renderable,
            x,
            y,
            Layer::AfterTiles.z() + z_offset * 0.9f32,
        )
    }
}
