use crate::render::{
    camera::Camera,
    layer::Layer,
    sprite_renderer::{SheetIndex, SpriteRenderable, SpriteRenderer},
    texture::Texture,
};

#[derive(Copy, Clone, Debug)]
pub enum TextColor {
    White,
    Green,
    Blue,
    DarkRed,
    Yellow,
    Fuchsia,
    Red,
    Pink,
}

impl TextColor {
    pub fn index(&self) -> usize {
        *self as usize
    }
}

pub enum TextAlignment {
    Left,
    Center,
    Right,
}

pub struct TextRenderer {
    _sprite_sheet_index: SheetIndex,
    renderables: Vec<SpriteRenderable>,
    character_width: i32,
    character_height: i32,
}

// TODO: Support foreign font
impl TextRenderer {
    const CHARACTERS_PER_ROW: u32 = 48;
    const CHARACTERS_PER_COL: u32 = 16;

    pub fn new(
        device: &wgpu::Device,
        texture: &Texture,
        sprite_renderer: &mut SpriteRenderer,
    ) -> Self {
        let sprite_sheet_index = sprite_renderer.create_sprite_sheet(device, texture);

        let sheet = sprite_renderer.get_sheet(sprite_sheet_index).unwrap();
        let mut renderables = vec![];

        let character_width = (texture.texture.width() / Self::CHARACTERS_PER_ROW) as i32;
        let character_height = (texture.texture.height() / Self::CHARACTERS_PER_COL) as i32;

        for i in 0..(Self::CHARACTERS_PER_ROW * Self::CHARACTERS_PER_COL) {
            let x = (i % Self::CHARACTERS_PER_ROW) * character_width as u32;
            let y = (i / Self::CHARACTERS_PER_ROW) * character_height as u32;

            let renderable =
                sheet.create_renderable(x, y, character_width as u32, character_height as u32);

            renderables.push(renderable);
        }

        Self {
            _sprite_sheet_index: sprite_sheet_index,
            renderables,
            character_width,
            character_height,
        }
    }

    // This will push renderables to the sprite renderer.
    // The sprite renderer will need to be rendered to actually see the result of this draw call.
    pub fn draw(
        &self,
        sprite_renderer: &mut SpriteRenderer,
        camera: &Camera,
        text: &str,
        x: i32,
        y: i32,
        layer: Layer,
        color: TextColor,
        align: TextAlignment,
    ) {
        let mut current_x;
        let mut current_y = y;

        // Precompute the transform so we don't have to matrix multiply every character.
        let mvp = camera.projection() * camera.view();

        for line in text.split('\n') {
            current_x = x;

            match align {
                TextAlignment::Center => {
                    current_x -= (line.len() as i32 * self.character_width) / 2;
                }
                TextAlignment::Right => {
                    current_x -= line.len() as i32 * self.character_width;
                }
                _ => {}
            }

            // This game only supports ascii text, so we can just take the raw bytes and grab the character from it.
            for c in line.as_bytes() {
                if *c < 0x20 || *c > 0x7F {
                    continue;
                }

                let character_index = (*c - 0x20) as usize;
                let color_index = color.index() * (Self::CHARACTERS_PER_ROW * 2) as usize;
                let renderable_index = color_index + character_index;

                let renderable = &self.renderables[renderable_index];

                sprite_renderer.draw_with_transform(
                    mvp,
                    camera.scale,
                    renderable,
                    current_x,
                    current_y,
                    layer,
                );

                current_x += self.character_width;
            }

            current_y += self.character_height;
        }
    }
}
