use crate::{
    clock::GameTick,
    map::Map,
    math::{PixelUnit, Position, Rectangle},
    render::{
        colors::ColorRenderableKind,
        game_sprites::GameSprites,
        layer::Layer,
        render_state::RenderState,
        sprite_renderer::{SheetIndex, SpriteRenderable},
        texture::Texture,
    },
};

#[allow(nonstandard_style)]
pub mod IndicatorFlag {
    pub const SmallMap: u8 = 1 << 0;
    pub const FullMap: u8 = 1 << 1;
}
pub type IndicatorFlags = u8;

struct RadarIndicator {
    color_kind: ColorRenderableKind,
    world_position: Position,
    end_tick: GameTick,
    flags: IndicatorFlags,
}

struct RadarSprite {
    renderable: SpriteRenderable,
    sheet: SheetIndex,
}

impl RadarSprite {
    pub fn new() -> Self {
        let invalid_sheet = SheetIndex(0xFFFFFFFF);

        Self {
            renderable: SpriteRenderable {
                uv_start: [0.0f32, 0.0f32],
                uv_size: [0.0f32, 0.0f32],
                size: [0, 0],
                sheet_index: invalid_sheet,
            },
            sheet: invalid_sheet,
        }
    }
}

struct RadarBuildParameters {
    mapzoom: u16,
    width: u32,
    height: u32,
    frequency: u16,
    powerball_mode: u8,
}

impl RadarBuildParameters {
    pub fn empty() -> Self {
        Self {
            mapzoom: 0,
            width: 0,
            height: 0,
            frequency: 0xFFFF,
            powerball_mode: 0,
        }
    }
}

struct RadarView {
    position: Position,

    dim: [u32; 2],
    world_min: Position,
    world_max: Position,

    min_uv: [f32; 2],
    max_uv: [f32; 2],
}

impl RadarView {
    pub fn empty() -> Self {
        Self {
            position: Position::empty(),
            dim: [0, 0],
            world_min: Position::empty(),
            world_max: Position::empty(),
            min_uv: [0.0f32, 0.0f32],
            max_uv: [0.0f32, 0.0f32],
        }
    }
}

pub struct Radar {
    // This is the sprite for the default radar. We only render a portion of this depending on our world position.
    sprite_radar: RadarSprite,
    // This is the sprite for the entire map that's displayed when holding alt.
    sprite_entire: RadarSprite,

    dirty: bool,

    build_parameters: RadarBuildParameters,

    view: RadarView,

    indicators: Vec<RadarIndicator>,
}

impl Radar {
    pub fn new() -> Self {
        Self {
            sprite_radar: RadarSprite::new(),
            sprite_entire: RadarSprite::new(),

            dirty: true,

            build_parameters: RadarBuildParameters::empty(),
            view: RadarView::empty(),

            indicators: vec![],
        }
    }

    pub fn invalidate(&mut self) {
        self.dirty = true;
    }

    pub fn update(
        &mut self,
        surface_width: u32,
        mapzoom: u16,
        position: Position,
        current_tick: GameTick,
    ) {
        self.indicators.retain(|i| current_tick < i.end_tick);

        if self.dirty {
            return;
        }

        let surface_width = surface_width;

        let mapzoom = mapzoom.max(1);

        let dim = (((surface_width / 6) / 4) * 8) / 2;
        let full_dim = (surface_width * 8) / mapzoom as u32;

        let ivar8 = (surface_width / 6) + (surface_width >> 0x1F);
        let ivar5 = full_dim;
        let ivar6 = (position.y.0 / 1000) as u32 * ivar5;
        let ivar4 = ((ivar8 >> 2) - (ivar8 >> 0x1F)) * 8 * 4;

        let ivar8 = (ivar4 + (ivar4 >> 0x1F & 7)) >> 3;
        let ivar4 = (position.x.0 / 1000) as u32 * ivar5;

        let texture_min_x = ((ivar4 + (ivar4 >> 0x1F & 0x3FFF)) >> 0xE) as i32 - (ivar8 / 2) as i32;
        let texture_min_y = ((ivar6 + (ivar6 >> 0x1F & 0x3FFF)) >> 0xE) as i32 - (ivar8 / 2) as i32;

        let ivar5 = ivar5.wrapping_sub(ivar8) as i32;

        let texture_min_x = texture_min_x.clamp(0, ivar5);
        let texture_min_y = texture_min_y.clamp(0, ivar5);

        let texture_max_x = texture_min_x + ivar8 as i32;
        let texture_max_y = texture_min_y + ivar8 as i32;

        self.view.position = position;
        self.view.dim = [dim, dim];

        self.view.min_uv = [
            texture_min_x as f32 / full_dim as f32,
            texture_min_y as f32 / full_dim as f32,
        ];
        self.view.max_uv = [
            texture_max_x as f32 / full_dim as f32,
            texture_max_y as f32 / full_dim as f32,
        ];

        self.view.world_min = Position::from_pixels(
            PixelUnit((self.view.min_uv[0] * 1024.0f32 * 16.0f32) as i32),
            PixelUnit((self.view.min_uv[1] * 1024.0f32 * 16.0f32) as i32),
        );
        self.view.world_max = Position::from_pixels(
            PixelUnit((self.view.max_uv[0] * 1024.0f32 * 16.0f32) as i32),
            PixelUnit((self.view.max_uv[1] * 1024.0f32 * 16.0f32) as i32),
        );
    }

    pub fn add_indicator(
        &mut self,
        kind: ColorRenderableKind,
        position: Position,
        end_tick: GameTick,
        flags: IndicatorFlags,
    ) {
        let indicator = RadarIndicator {
            color_kind: kind,
            world_position: position,
            end_tick,
            flags,
        };

        self.indicators.push(indicator);
    }

    pub fn render(
        &mut self,
        render_state: &mut RenderState,
        game_sprites: &GameSprites,
        map: &Map,
        mapzoom: u16,
        frequency: u16,
        powerball_mode: u8,
        fullsize: bool,
    ) {
        if self.should_recreate(
            render_state.config.width,
            render_state.config.height,
            mapzoom,
            frequency,
            powerball_mode,
        ) {
            self.recreate(render_state, map, mapzoom, frequency, powerball_mode);
        }

        if self.dirty {
            return;
        }

        const CORNER_INSET: u32 = 6;

        let bottom_x = render_state.config.width.saturating_sub(CORNER_INSET);
        let bottom_y = render_state.config.height.saturating_sub(CORNER_INSET);

        if fullsize {
            let size = &self.sprite_entire.renderable.size;
            let start_x = bottom_x.saturating_sub(size[0]);
            let start_y = bottom_y.saturating_sub(size[1]);

            render_state.sprite_renderer.draw(
                &render_state.ui_camera,
                &self.sprite_entire.renderable,
                start_x as i32,
                start_y as i32,
                Layer::AfterChat,
            );

            for indicator in &self.indicators {
                if indicator.flags & IndicatorFlag::FullMap == 0 {
                    continue;
                }

                let u = (indicator.world_position.x.0 / 1000) as f32 / 16.0f32 / 1024.0f32;
                let v = (indicator.world_position.y.0 / 1000) as f32 / 16.0f32 / 1024.0f32;

                let radar_position = [
                    (self.sprite_entire.renderable.size[0] as f32 * u) as i32,
                    (self.sprite_entire.renderable.size[1] as f32 * v) as i32,
                ];

                self.render_indicator(
                    render_state,
                    game_sprites,
                    indicator.color_kind,
                    start_x as i32 + radar_position[0],
                    start_y as i32 + radar_position[1],
                );
            }
        } else {
            let uv_size = [
                self.view.max_uv[0] - self.view.min_uv[0],
                self.view.max_uv[1] - self.view.min_uv[1],
            ];
            let visible_renderable = SpriteRenderable {
                uv_start: self.view.min_uv,
                uv_size: uv_size,
                size: self.view.dim,
                sheet_index: self.sprite_radar.renderable.sheet_index,
            };

            let start_x = bottom_x.saturating_sub(self.view.dim[0]);
            let start_y = bottom_y.saturating_sub(self.view.dim[1]);

            render_state.sprite_renderer.draw(
                &render_state.ui_camera,
                &visible_renderable,
                start_x as i32,
                start_y as i32,
                Layer::AfterChat,
            );

            game_sprites.colors.draw_border(
                &mut render_state.sprite_renderer,
                &render_state.ui_camera,
                Layer::Chat,
                start_x as i32 - 1,
                start_y as i32 - 1,
                bottom_x as i32,
                bottom_y as i32,
                false,
            );

            let world_rect = Rectangle::new(self.view.world_min, self.view.world_max);
            let world_extents = world_rect.extents();

            for indicator in &self.indicators {
                if indicator.flags & IndicatorFlag::SmallMap == 0 {
                    continue;
                }

                if !world_rect.contains(indicator.world_position) {
                    continue;
                }

                let u = (indicator.world_position.x.0 - self.view.world_min.x.0) as f32
                    / world_extents.0 as f32;
                let v = (indicator.world_position.y.0 - self.view.world_min.y.0) as f32
                    / world_extents.1 as f32;

                let radar_position = [
                    (self.view.dim[0] as f32 * u) as i32,
                    (self.view.dim[1] as f32 * v) as i32,
                ];

                self.render_indicator(
                    render_state,
                    game_sprites,
                    indicator.color_kind,
                    start_x as i32 + radar_position[0],
                    start_y as i32 + radar_position[1],
                );
            }
        }
    }

    fn render_indicator(
        &self,
        render_state: &mut RenderState,
        game_sprites: &GameSprites,
        color_kind: ColorRenderableKind,
        x_pixels: i32,
        y_pixels: i32,
    ) {
        let (width, height) = match &color_kind {
            ColorRenderableKind::RadarSelfFlagCarry
            | ColorRenderableKind::RadarTeammateFlagCarry
            | ColorRenderableKind::RadarEnemyFlagCarry => (3, 3),
            _ => (2, 2),
        };

        game_sprites.colors.draw_centered(
            &mut render_state.sprite_renderer,
            &render_state.ui_camera,
            Layer::AfterChat,
            color_kind,
            x_pixels,
            y_pixels,
            width,
            height,
        );
    }

    fn should_recreate(
        &self,
        surface_width: u32,
        surface_height: u32,
        mapzoom: u16,
        frequency: u16,
        powerball_mode: u8,
    ) -> bool {
        let mut mapzoom = mapzoom;

        if mapzoom < 1 {
            mapzoom = 1;
        }

        let (surface_width, surface_height) =
            Self::get_surface_dimensions(surface_width, surface_height);

        let params = &self.build_parameters;

        self.dirty
            || surface_width != params.width
            || surface_height != params.height
            || mapzoom != params.mapzoom
            || params.frequency != frequency
            || params.powerball_mode != powerball_mode
    }

    // This modifies the surface dimensions to get a good surface size for the radar.
    // Since we allow stretching, we want to shrink the radar if our height gets too low compared to width.
    fn get_surface_dimensions(width: u32, height: u32) -> (u32, u32) {
        let max_width = (height as f32 * 1.7777f32) as u32;
        let width = width.min(max_width);

        (width, height)
    }

    fn recreate(
        &mut self,
        render_state: &mut RenderState,
        map: &Map,
        mapzoom: u16,
        frequency: u16,
        powerball_mode: u8,
    ) {
        let mut mapzoom = mapzoom as u32;
        let surface_width = render_state.config.width;
        let surface_height = render_state.config.height;

        let (surface_width, surface_height) =
            Self::get_surface_dimensions(surface_width, surface_height);

        // If our surface is too small, disable radar rendering.
        if surface_width < 128 || surface_height < 128 {
            self.dirty = true;
            return;
        }

        let entire_dim = (surface_width / 2) - 64;

        if mapzoom < 1 {
            mapzoom = 1;
        }

        self.build_parameters.mapzoom = mapzoom as u16;
        self.build_parameters.width = surface_width;
        self.build_parameters.height = surface_height;
        self.build_parameters.frequency = frequency;
        self.build_parameters.powerball_mode = powerball_mode;

        self.dirty = false;

        // Use original width here so we don't get render artifacts.
        let radar_dim = (render_state.config.width * 8) / mapzoom;

        let entire_texture = Self::render_radar(
            &render_state.device,
            &render_state.queue,
            entire_dim,
            render_state.get_texture_format(),
            map,
            frequency,
            powerball_mode,
        );

        let radar_texture = Self::render_radar(
            &render_state.device,
            &render_state.queue,
            radar_dim,
            render_state.get_texture_format(),
            map,
            frequency,
            powerball_mode,
        );

        let invalid_sheet = SheetIndex(0xFFFFFFFF);

        // Create new sheets and renderables if we have never created one.
        // Adjust existing sheets and renderables if we already have one.

        if self.sprite_radar.sheet.0 == invalid_sheet.0 {
            let sheet_index = render_state.sprite_renderer.create_sprite_sheet(
                &render_state.device,
                &radar_texture,
                false,
            );

            self.sprite_radar.sheet = sheet_index;
            self.sprite_radar.renderable = SpriteRenderable {
                uv_start: [0.0f32, 0.0f32],
                uv_size: [1.0f32, 1.0f32],
                size: [radar_dim, radar_dim],
                sheet_index,
            };
        } else {
            let index = self.sprite_radar.sheet;

            render_state.sprite_renderer.change_sheet_texture(
                index,
                &render_state.device,
                &radar_texture,
            );
            self.sprite_radar.renderable.size = [radar_dim, radar_dim];
        }

        if self.sprite_entire.sheet.0 == invalid_sheet.0 {
            let sheet_index = render_state.sprite_renderer.create_sprite_sheet(
                &render_state.device,
                &entire_texture,
                false,
            );

            self.sprite_entire.sheet = sheet_index;
            self.sprite_entire.renderable = SpriteRenderable {
                uv_start: [0.0f32, 0.0f32],
                uv_size: [1.0f32, 1.0f32],
                size: [entire_dim, entire_dim],
                sheet_index,
            };
        } else {
            let index = self.sprite_entire.sheet;

            render_state.sprite_renderer.change_sheet_texture(
                index,
                &render_state.device,
                &entire_texture,
            );
            self.sprite_entire.renderable.size = [entire_dim, entire_dim];
        }
    }

    fn render_radar(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        dim: u32,
        format: wgpu::TextureFormat,
        map: &Map,
        frequency: u16,
        powerball_mode: u8,
    ) -> Texture {
        let texture = Texture::new_2d(device, dim, dim, format);
        let mut data = Vec::<u32>::new();

        data.resize((dim * dim) as usize, 0);

        if dim < 1024 {
            for y in 0..dim {
                for x in 0..dim {
                    let index = (y * dim + x) as usize;

                    data[index] = 0xFF0A190A;
                }
            }

            for y in 0..1024 {
                let dest_y = ((y as f32 / 1024.0f32) * dim as f32) as u16;

                for x in 0..1024 {
                    let dest_x = ((x as f32 / 1024.0f32) * dim as f32) as u16;

                    let id = map.get_tile(x, y);
                    let index = (dest_y as u32 * dim + dest_x as u32) as usize;

                    if id == 0 || id > 241 {
                        // Empty tile, do not render
                    } else {
                        data[index] =
                            Self::get_radar_tile_color(id, x, y, frequency, powerball_mode);
                    }
                }
            }
        } else {
            let mut y_tile_index = 0;
            for gen_y in 0..dim {
                let y = y_tile_index / dim;

                let mut x_tile_index = 0;
                for gen_x in 0..dim {
                    let x = x_tile_index / dim;
                    let id = map.get_tile(x as u16, y as u16);

                    let index = gen_y * dim + gen_x;

                    data[index as usize] = Self::get_radar_tile_color(
                        id,
                        x as u16,
                        y as u16,
                        frequency,
                        powerball_mode,
                    );

                    x_tile_index += 1024;
                }

                y_tile_index += 1024;
            }
        }

        RenderState::buffer_texture(queue, &texture, &bytemuck::cast_slice(&data));

        texture
    }

    fn get_radar_tile_color(id: u8, x: u16, y: u16, frequency: u16, powerball_mode: u8) -> u32 {
        if id == 0 || id > 241 {
            return 0xFF0A190A;
        } else if id == 171 {
            return 0xFF185218;
        } else if id == 172 {
            let position =
                Position::from_pixels(PixelUnit(x as i32 * 16), PixelUnit(y as i32 * 16));

            if crate::powerball::is_team_goal(powerball_mode, position, frequency) {
                return 0xFF219CAD;
            }

            return 0xFF0839FF;
        } else if id >= 162 && id <= 169 {
            return 0xFFADADAD;
        }

        return 0xFF5a5a5a;
    }
}
