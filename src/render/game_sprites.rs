use std::sync::mpsc::{Receiver, Sender, channel};

use crate::render::{
    render_state::RenderState, sprite_renderer::SpriteRenderable, texture::Texture,
};
use thiserror::Error;

pub struct SpriteSet {
    pub renderables: Vec<SpriteRenderable>,
    pub texture: Option<Texture>,
}

impl SpriteSet {
    pub fn empty() -> Self {
        Self {
            renderables: vec![],
            texture: None,
        }
    }

    pub fn new(
        render_state: &mut RenderState,
        img: &image::RgbaImage,
        cols: u32,
        rows: u32,
    ) -> Self {
        use image::EncodableLayout;

        let width = img.width();
        let height = img.height();
        let renderable_width = width / cols;
        let renderable_height = height / rows;

        let texture = Texture::new_2d(
            &render_state.device,
            width,
            height,
            render_state.get_texture_format(),
        );

        RenderState::buffer_texture(&render_state.queue, &texture, &img.as_bytes());
        let sheet_index = render_state
            .sprite_renderer
            .create_sprite_sheet(&render_state.device, &&texture);
        let sheet = render_state.sprite_renderer.get_sheet(sheet_index).unwrap();

        let mut renderables = vec![];

        for row in 0..rows {
            let y = row * renderable_height;
            for col in 0..cols {
                let x = col * renderable_width;

                let renderable = sheet.create_renderable(x, y, renderable_width, renderable_height);
                renderables.push(renderable);
            }
        }

        Self {
            renderables,
            texture: Some(texture),
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
    Repel,
}
// This must match the last kind in the enum. std::mem::variant_count still unstable.
const GAME_SPRITE_KIND_SIZE: usize = GameSpriteKind::Repel as usize + 1;

// cols, rows definition for each sprite kind
const SHEET_DEFINITIONS: [(u32, u32); GAME_SPRITE_KIND_SIZE] = [
    (10, 4 * 8), // Ships 4 rows for each ship kind (8)
    (4, 10),     // Bullets
    (10, 13),    // Bombs
    (10, 8),     // Mines
    (10, 6),     // Shrapnel
    (5, 2),      // Repel
];

pub struct GameSprites {
    pub sprites: [SpriteSet; GAME_SPRITE_KIND_SIZE],
}

impl GameSprites {
    pub fn new() -> Self {
        let sprites = [(); GAME_SPRITE_KIND_SIZE].map(|_| SpriteSet::empty());
        Self { sprites }
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
            let (cols, rows) = SHEET_DEFINITIONS[index];

            sprites.sprites[index] = SpriteSet::new(render_state, &img, cols, rows);
        }

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
