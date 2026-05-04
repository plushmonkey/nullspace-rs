use crate::{
    clock::GameTick,
    render::{
        camera::Camera,
        game_sprites::{GameSpriteKind, GameSprites},
        layer::Layer,
        sprite_renderer::SpriteRenderer,
    },
};

struct Animation {
    kind: GameSpriteKind,
    duration: u32,

    last_update_tick: GameTick,
    remaining_ticks: u32,

    current_frame: u16,

    frame_start: u16,
    frame_end: u16,

    position: [i32; 2],
    layer: Layer,
}

pub struct AnimationRenderer {
    animations: Vec<Animation>,
    last_update_tick: GameTick,
}

impl AnimationRenderer {
    pub fn new() -> Self {
        Self {
            animations: vec![],
            last_update_tick: GameTick::empty(),
        }
    }

    pub fn clear(&mut self) {
        self.animations.clear();
    }

    pub fn render(
        &mut self,
        camera: &Camera,
        sprite_renderer: &mut SpriteRenderer,
        game_sprites: &GameSprites,
    ) {
        for animation in &self.animations {
            let Some(sprites) = game_sprites.get_set(animation.kind) else {
                continue;
            };

            let renderable = &sprites.renderables[animation.current_frame as usize];

            sprite_renderer.draw_centered(
                &camera,
                renderable,
                animation.position[0],
                animation.position[1],
                animation.layer,
            );
        }
    }

    pub fn update(&mut self, current_tick: GameTick) {
        let mut index = 0;

        // Don't bother looping if we haven't ticked at all.
        if self.last_update_tick == current_tick {
            return;
        }

        loop {
            if index >= self.animations.len() {
                break;
            }

            let animation = &mut self.animations[index];

            let tick_diff = current_tick.diff(&animation.last_update_tick);

            animation.remaining_ticks = animation
                .remaining_ticks
                .saturating_sub_signed(tick_diff as i32);

            if animation.remaining_ticks <= 0 {
                self.animations.swap_remove(index);
                continue;
            }

            let frame_count = (animation.frame_end - animation.frame_start) as usize;
            let ticks_per_frame = animation.duration as usize / frame_count;
            let tick_count = animation.duration.saturating_sub(animation.remaining_ticks);

            animation.current_frame = animation.frame_start
                + u16::min(
                    (tick_count / ticks_per_frame as u32) as u16,
                    (frame_count - 1) as u16,
                );

            if tick_diff > 0 {
                animation.last_update_tick = current_tick;
            }

            index += 1;
        }

        self.last_update_tick = current_tick;
    }

    pub fn add(
        &mut self,
        kind: GameSpriteKind,
        start_tick: GameTick,
        frame_start: usize,
        frame_end: usize,
        duration: u32,
        x_pixels: i32,
        y_pixels: i32,
        layer: Layer,
    ) {
        self.animations.push(Animation {
            kind,
            duration: duration,
            last_update_tick: start_tick,
            remaining_ticks: duration,
            current_frame: frame_start as u16,
            frame_start: frame_start as u16,
            frame_end: frame_end as u16,
            position: [x_pixels, y_pixels],
            layer,
        });
    }
}

pub fn get_animation_index(tick_value: u32, frames: usize, duration: usize) -> usize {
    let ticks_per_frame = duration / frames;
    let ticks = tick_value as usize;

    (ticks / ticks_per_frame) % frames
}
