use crate::{
    clock::GameTick,
    render::{
        camera::Camera,
        layer::Layer,
        sprite_renderer::{SheetIndex, SpriteRenderable, SpriteRenderer},
    },
};

#[derive(Copy, Clone)]
pub enum ColorRenderableKind {
    Blank = 0,

    BorderInner = 1,
    BorderCenter = 2,
    BorderOuter = 3,

    Background = 16,

    RadarPrize = 23,

    RadarPortal = 25,
    RadarTeamFlag = 26,
    RadarSelfFlagCarry = 27,
    RadarSelf = 28,
    RadarTeammate = 29,
    RadarTeammateFlagCarry = 30,

    RadarEnemyFlagCarry = 31,
    RadarEnemyTarget = 33,
    RadarEnemy = 34,
    RadarBomb = 35,
    RadarDecoy = 36,
    RadarExplosion = 37,
}

pub struct Colors {
    pub width: u32,
    pub height: u32,
    pub sheet_index: SheetIndex,
    pub current_tick: GameTick,
    pub current_u: f32,
}

impl Colors {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            sheet_index: SheetIndex(0xFFFFFF),
            current_tick: GameTick::empty(),
            current_u: 0.0f32,
        }
    }

    pub fn tick(&mut self, current_tick: GameTick) {
        self.current_tick = current_tick;
        self.current_u = (self.current_tick.value() % self.width) as f32 / self.width as f32;
    }

    pub fn draw(
        &self,
        sprite_renderer: &mut SpriteRenderer,
        camera: &Camera,
        layer: Layer,
        kind: ColorRenderableKind,
        x_pixels: i32,
        y_pixels: i32,
        width: i32,
        height: i32,
    ) {
        let v = (kind as i32 as f32 / self.height as f32) + 1.0f32 / (self.height as f32 * 2.0f32);

        let renderable = SpriteRenderable {
            uv_start: [self.current_u, v],
            uv_size: [0.0f32, 0.0f32],
            size: [width as u32, height as u32],
            sheet_index: self.sheet_index,
        };

        sprite_renderer.draw(camera, &renderable, x_pixels, y_pixels, layer);
    }

    pub fn draw_centered(
        &self,
        sprite_renderer: &mut SpriteRenderer,
        camera: &Camera,
        layer: Layer,
        kind: ColorRenderableKind,
        x_pixels: i32,
        y_pixels: i32,
        width: i32,
        height: i32,
    ) {
        let v = (kind as i32 as f32 / self.height as f32) + 1.0f32 / (self.height as f32 * 2.0f32);

        let renderable = SpriteRenderable {
            uv_start: [self.current_u, v],
            uv_size: [0.0f32, 0.0f32],
            size: [width as u32, height as u32],
            sheet_index: self.sheet_index,
        };

        let mvp = camera.projection() * camera.view();

        let (width, height) = (renderable.size[0] as f32, renderable.size[1] as f32);

        let x_pixels = x_pixels as f32 - width / 2.0f32;
        let y_pixels = y_pixels as f32 - height / 2.0f32;

        sprite_renderer.draw_with_transform(
            mvp,
            camera.scale,
            &renderable,
            x_pixels,
            y_pixels,
            layer.z(),
        );
    }

    // This draws a border starting from the inner border pixel position.
    pub fn draw_border(
        &self,
        sprite_renderer: &mut SpriteRenderer,
        camera: &Camera,
        layer: Layer,
        start_x_pixels: i32,
        start_y_pixels: i32,
        end_x_pixels: i32,
        end_y_pixels: i32,
        inner_background: bool,
    ) {
        let width = end_x_pixels - start_x_pixels;
        let height = end_y_pixels - start_y_pixels;

        // Draw top
        self.draw(
            sprite_renderer,
            camera,
            layer,
            ColorRenderableKind::BorderInner,
            start_x_pixels,
            start_y_pixels,
            width + 1,
            1,
        );
        self.draw(
            sprite_renderer,
            camera,
            layer,
            ColorRenderableKind::BorderCenter,
            start_x_pixels - 1,
            start_y_pixels - 1,
            width + 2,
            1,
        );
        self.draw(
            sprite_renderer,
            camera,
            layer,
            ColorRenderableKind::BorderOuter,
            start_x_pixels,
            start_y_pixels - 2,
            width,
            1,
        );

        // Draw bottom
        self.draw(
            sprite_renderer,
            camera,
            layer,
            ColorRenderableKind::BorderInner,
            start_x_pixels,
            start_y_pixels + height,
            width + 1,
            1,
        );
        self.draw(
            sprite_renderer,
            camera,
            layer,
            ColorRenderableKind::BorderCenter,
            start_x_pixels - 1,
            start_y_pixels + height + 1,
            width + 2,
            1,
        );
        self.draw(
            sprite_renderer,
            camera,
            layer,
            ColorRenderableKind::BorderOuter,
            start_x_pixels,
            start_y_pixels + height + 2,
            width,
            1,
        );

        // Draw left
        self.draw(
            sprite_renderer,
            camera,
            layer,
            ColorRenderableKind::BorderInner,
            start_x_pixels,
            start_y_pixels,
            1,
            height + 1,
        );
        self.draw(
            sprite_renderer,
            camera,
            layer,
            ColorRenderableKind::BorderCenter,
            start_x_pixels - 1,
            start_y_pixels - 1,
            1,
            height + 2,
        );
        self.draw(
            sprite_renderer,
            camera,
            layer,
            ColorRenderableKind::BorderOuter,
            start_x_pixels - 2,
            start_y_pixels,
            1,
            height,
        );

        // Draw right
        self.draw(
            sprite_renderer,
            camera,
            layer,
            ColorRenderableKind::BorderInner,
            start_x_pixels + width,
            start_y_pixels,
            1,
            height + 1,
        );
        self.draw(
            sprite_renderer,
            camera,
            layer,
            ColorRenderableKind::BorderCenter,
            start_x_pixels + width + 1,
            start_y_pixels - 1,
            1,
            height + 2,
        );
        self.draw(
            sprite_renderer,
            camera,
            layer,
            ColorRenderableKind::BorderOuter,
            start_x_pixels + width + 2,
            start_y_pixels,
            1,
            height,
        );

        if inner_background {
            self.draw(
                sprite_renderer,
                camera,
                layer,
                ColorRenderableKind::Background,
                start_x_pixels + 1,
                start_y_pixels + 1,
                width - 1,
                height - 1,
            );
        }
    }
}
