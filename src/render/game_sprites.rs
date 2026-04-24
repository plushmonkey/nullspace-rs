use std::sync::mpsc::{Receiver, Sender, channel};

use crate::render::{
    colors::Colors,
    render_state::RenderState,
    sprite_renderer::{SheetIndex, SpriteRenderable},
    texture::Texture,
};
use thiserror::Error;

pub struct SpriteSet {
    pub renderables: Vec<SpriteRenderable>,
    pub texture: Option<Texture>,
    pub sheet_index: SheetIndex,
}

impl SpriteSet {
    pub fn empty() -> Self {
        Self {
            renderables: vec![],
            texture: None,
            sheet_index: SheetIndex(0xFFFFFFFF),
        }
    }

    pub fn new(
        render_state: &mut RenderState,
        img: &image::RgbaImage,
        cols: u32,
        rows: u32,
    ) -> Self {
        let width = img.width();
        let height = img.height();

        Self::new_from_slice(render_state, img, 0, 0, width, height, cols, rows)
    }

    pub fn new_from_slice(
        render_state: &mut RenderState,
        img: &image::RgbaImage,
        x_start: u32,
        y_start: u32,
        x_end: u32,
        y_end: u32,
        cols: u32,
        rows: u32,
    ) -> Self {
        use image::EncodableLayout;

        let width = x_end - x_start;
        let height = y_end - y_start;

        let renderable_width = width / cols;
        let renderable_height = height / rows;

        let texture = Texture::new_2d(
            &render_state.device,
            img.width(),
            img.height(),
            render_state.get_texture_format(),
        );

        RenderState::buffer_texture(&render_state.queue, &texture, &img.as_bytes());

        let nearest_sampler = renderable_width == 1 || renderable_height == 1;

        let sheet_index = render_state.sprite_renderer.create_sprite_sheet(
            &render_state.device,
            &texture,
            nearest_sampler,
        );
        let sheet = render_state.sprite_renderer.get_sheet(sheet_index).unwrap();

        let mut renderables = vec![];

        for row in 0..rows {
            let y = y_start + row * renderable_height;
            for col in 0..cols {
                let x = x_start + col * renderable_width;

                let renderable = sheet.create_renderable(x, y, renderable_width, renderable_height);

                renderables.push(renderable);
            }
        }

        Self {
            renderables,
            texture: Some(texture),
            sheet_index: sheet_index,
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub enum GameSpriteKind {
    Ships,
    Bullets,
    Bombs,
    Mines,
    Shrapnel,
    Flag,
    Goal,
    AsteroidSmall,
    AsteroidLarge,
    AsteroidSmall2,
    SpaceStation,
    Wormhole,
    BulletExplosion,
    BombExplosion,
    PlayerExplosion,
    EmpExplosion,
    Flash,
    Colors,
    Powerball,
    Gradient,
    Trail,
    Spectate,
    Brick,
    Repel,
}
// This must match the last kind in the enum. std::mem::variant_count still unstable.
const GAME_SPRITE_KIND_SIZE: usize = GameSpriteKind::Repel as usize + 1;

// cols, rows definition for each sprite kind
pub const GAME_SPRITE_SHEET_DEFINITIONS: [(u32, u32); GAME_SPRITE_KIND_SIZE] = [
    (10, 4 * 8), // Ships 4 rows for each ship kind (8)
    (4, 10),     // Bullets
    (10, 13),    // Bombs
    (10, 8),     // Mines
    (10, 6),     // Shrapnel
    (10, 2),     // Flag
    (9, 2),      // Goal
    (15, 2),     // AsteroidSmall
    (10, 3),     // AsteroidLarge
    (15, 2),     // AsteroidSmal2
    (5, 2),      // SpaceStation
    (4, 6),      // Wormhole
    (7, 1),      // BulletExplosion
    (4, 11),     // BombExplosion
    (6, 6),      // PlayerExplosion
    (5, 2),      // EmpExplosion
    (6, 3),      // Flash
    (1, 1),      // Colors
    (10, 3),     // Powerball
    (14, 8),     // Gradient
    (10, 5),     // Trail
    (11, 1),     // Spectate
    (10, 2),     // Brick
    (5, 2),      // Repel
];

pub struct GameSprites {
    pub sprites: [SpriteSet; GAME_SPRITE_KIND_SIZE],
    pub colors: Colors,
}

impl GameSprites {
    pub fn new() -> Self {
        let sprites = [(); GAME_SPRITE_KIND_SIZE].map(|_| SpriteSet::empty());
        Self {
            sprites,
            colors: Colors::new(0, 0),
        }
    }

    pub fn get_set(&self, kind: GameSpriteKind) -> Option<&SpriteSet> {
        let index = kind as usize;

        if self.sprites[index].renderables.is_empty() {
            return None;
        }

        Some(&self.sprites[index])
    }
}

#[derive(Error, Debug)]
pub enum GameSpriteLoadError {
    #[error("failed to fetch image {0}")]
    ImageFetchError(String),

    #[error("failed to decode image {0}")]
    ImageDecodeError(String),
}

type LoadSet = Vec<(GameSpriteKind, image::RgbaImage)>;

pub struct GameSpriteLoader {
    sender: Sender<LoadSet>,
    receiver: Receiver<LoadSet>,
}

impl GameSpriteLoader {
    pub fn new() -> Self {
        let (sender, receiver) = channel();

        let mut result = Self { sender, receiver };

        result.load();

        result
    }

    pub fn try_create(&mut self, render_state: &mut RenderState) -> Option<GameSprites> {
        let Ok(loadset) = self.receiver.try_recv() else {
            return None;
        };

        let mut sprites = GameSprites::new();

        for (kind, img) in loadset {
            let index = kind as usize;
            let (cols, rows) = GAME_SPRITE_SHEET_DEFINITIONS[index];

            sprites.sprites[index] = SpriteSet::new(render_state, &img, cols, rows);
        }

        let (colors_width, colors_height, colors_sheet_index) =
            if let Some(colors_spriteset) = sprites.get_set(GameSpriteKind::Colors) {
                if let Some(texture) = &colors_spriteset.texture {
                    (
                        texture.texture.width(),
                        texture.texture.height(),
                        colors_spriteset.sheet_index,
                    )
                } else {
                    (0, 0, SheetIndex(0xFFFFFFFF))
                }
            } else {
                (0, 0, SheetIndex(0xFFFFFFFF))
            };

        sprites.colors.width = colors_width;
        sprites.colors.height = colors_height;
        sprites.colors.sheet_index = colors_sheet_index;

        Some(sprites)
    }

    pub fn load(&mut self) {
        let sender = self.sender.clone();

        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(async move {
            Self::load_impl(sender).await;
        });

        #[cfg(not(target_arch = "wasm32"))]
        pollster::block_on(async move {
            Self::load_impl(sender).await;
        });
    }

    async fn load_impl(sender: Sender<LoadSet>) {
        let fetches: [(GameSpriteKind, &str); GAME_SPRITE_KIND_SIZE] = [
            (GameSpriteKind::Ships, "graphics/ships.png"),
            (GameSpriteKind::Bullets, "graphics/bullets.bm2"),
            (GameSpriteKind::Bombs, "graphics/bombs.bm2"),
            (GameSpriteKind::Mines, "graphics/mines.bm2"),
            (GameSpriteKind::Shrapnel, "graphics/shrapnel.bm2"),
            (GameSpriteKind::Flag, "graphics/flag.bm2"),
            (GameSpriteKind::Goal, "graphics/goal.bm2"),
            (GameSpriteKind::AsteroidSmall, "graphics/over1.bm2"),
            (GameSpriteKind::AsteroidLarge, "graphics/over2.bm2"),
            (GameSpriteKind::AsteroidSmall2, "graphics/over3.bm2"),
            (GameSpriteKind::SpaceStation, "graphics/over4.bm2"),
            (GameSpriteKind::Wormhole, "graphics/over5.bm2"),
            (GameSpriteKind::BulletExplosion, "graphics/explode0.bm2"),
            (GameSpriteKind::BombExplosion, "graphics/explode2.bm2"),
            (GameSpriteKind::PlayerExplosion, "graphics/explode1.bm2"),
            (GameSpriteKind::EmpExplosion, "graphics/empburst.bm2"),
            (GameSpriteKind::Flash, "graphics/warp.bm2"),
            (GameSpriteKind::Colors, "graphics/colors.bm2"),
            (GameSpriteKind::Powerball, "graphics/powerb.bm2"),
            (GameSpriteKind::Gradient, "graphics/gradient.bm2"),
            (GameSpriteKind::Trail, "graphics/trail.bm2"),
            (GameSpriteKind::Spectate, "graphics/spectate.bm2"),
            (GameSpriteKind::Brick, "graphics/wall.bm2"),
            (GameSpriteKind::Repel, "graphics/repel.bm2"),
        ];

        assert!(fetches.len() == GAME_SPRITE_KIND_SIZE);

        // Kick off each load here, then collect them below
        let results = fetches.map(|(kind, path)| (kind, Self::load_image(path)));

        let mut loadset = LoadSet::new();

        for (kind, result) in results {
            let img = match result.await {
                Ok(img) => img,
                Err(e) => {
                    log::error!("{:?}: {e}", kind);
                    continue;
                }
            };

            loadset.push((kind, img));
        }

        if let Err(e) = sender.send(loadset) {
            log::error!("{e}");
        }
    }

    async fn load_image(path: &str) -> Result<image::RgbaImage, GameSpriteLoadError> {
        #[cfg(target_arch = "wasm32")]
        match crate::web_util::load_image(path).await {
            Ok(image_data) => {
                let img = match image::RgbaImage::from_raw(
                    image_data.width(),
                    image_data.height(),
                    image_data.data().to_vec(),
                ) {
                    Some(img) => img,
                    None => {
                        return Err(GameSpriteLoadError::ImageDecodeError(
                            "container not large enough".into(),
                        ));
                    }
                };
                return Ok(img);
            }
            Err(_) => {
                return Err(GameSpriteLoadError::ImageFetchError(path.into()));
            }
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let data = match std::fs::read(path) {
                Ok(data) => data,
                Err(_) => {
                    return Err(GameSpriteLoadError::ImageFetchError(path.into()));
                }
            };

            let img = match image::load_from_memory(&data) {
                Ok(img) => img.to_rgba8(),
                Err(_) => {
                    return Err(GameSpriteLoadError::ImageDecodeError(path.into()));
                }
            };

            return Ok(img);
        }
    }
}
