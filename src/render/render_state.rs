use std::sync::Arc;

use image::EncodableLayout;
use thiserror::Error;
use winit::{dpi::PhysicalSize, window::Window};

use crate::{
    map::Map,
    render::{
        animation_renderer::AnimationRenderer,
        background_renderer::BackgroundRenderer,
        camera::Camera,
        game_sprites::{GameSprites, SpriteSet},
        layer::Layer,
        map_renderer::{MapRenderer, MapTileset},
        sprite_renderer::SpriteRenderer,
        text_renderer::{TextAlignment, TextColor, TextRenderer},
        texture::Texture,
    },
};

#[derive(Error, Debug)]
pub enum RenderStateCreateError {
    #[error("{0}")]
    CreateSurfaceError(#[from] wgpu::CreateSurfaceError),

    #[error("{0}")]
    RequestAdapterError(#[from] wgpu::RequestAdapterError),

    #[error("{0}")]
    RequestDeviceError(#[from] wgpu::RequestDeviceError),
}

#[derive(Error, Debug)]
pub enum RenderError {
    #[error("lost device")]
    LostDevice,
}

// Holds the window render state and the game renderers that have custom shaders.
pub struct RenderState {
    pub instance: wgpu::Instance,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    pub is_surface_configured: bool,

    surface: wgpu::Surface<'static>,
    depth_texture: Texture,

    pub camera: Camera,
    pub ui_camera: Camera,

    pub map_renderer: MapRenderer,
    pub sprite_renderer: SpriteRenderer,
    pub text_renderer: TextRenderer,
    pub background_renderer: BackgroundRenderer,
    pub animation_renderer: AnimationRenderer,

    pub render_map: bool,
}

impl RenderState {
    pub async fn new(window: Arc<Window>) -> Result<Self, RenderStateCreateError> {
        let size = window.inner_size();

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            #[cfg(not(target_arch = "wasm32"))]
            backends: wgpu::Backends::PRIMARY,
            #[cfg(target_arch = "wasm32")]
            backends: wgpu::Backends::BROWSER_WEBGPU,
            flags: Default::default(),
            memory_budget_thresholds: Default::default(),
            backend_options: Default::default(),
            display: None,
        });

        let surface = instance.create_surface(window.clone())?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance, // Chrome has a bug where HighPerformance is ignored, but specify it here anyway.
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await?;

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::defaults(),
                memory_hints: Default::default(),
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                trace: wgpu::Trace::Off,
            })
            .await?;

        let surface_caps = surface.get_capabilities(&adapter);

        let mut req_present_mode = wgpu::PresentMode::AutoVsync;

        // wgpu has a bug with vsync and resizing windows on Windows, so select a better mode if it's available.
        // AutoVsync is available everywhere and will be the default if the better modes aren't available.
        for present_mode in surface_caps.present_modes {
            if present_mode == wgpu::PresentMode::Mailbox {
                req_present_mode = present_mode;
                break;
            } else if present_mode == wgpu::PresentMode::Immediate {
                req_present_mode = present_mode;
            }
        }

        log::info!("Using present mode {:?}", req_present_mode);

        let surface_format = surface_caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);
        log::info!("Surface format {:?}", surface_format);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            //present_mode: surface_caps.present_modes[0], // TODO: Probably just want to use Fifo (works on all platforms) for vsync or make it configurable
            present_mode: req_present_mode,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        let depth_texture = Texture::new_depth(&device, &config);

        let map_renderer = MapRenderer::new(&device, &config.format, &depth_texture);

        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(async move {
            match crate::web_util::load_image("graphics/tiles.png").await {
                Ok(img_data) => {
                    log::info!(
                        "tiles image data loaded {}, {}",
                        img_data.width(),
                        img_data.height()
                    );
                }
                Err(e) => {
                    log::error!("{e}");
                }
            }
        });

        const SPRITE_RENDERER_PUSH_SIZE: usize = 4096 * 8;

        let mut sprite_renderer = SpriteRenderer::new(
            &device,
            &config.format,
            &depth_texture,
            SPRITE_RENDERER_PUSH_SIZE,
        );

        let view_format = if surface_format.is_srgb() {
            wgpu::TextureFormat::Rgba8UnormSrgb
        } else {
            wgpu::TextureFormat::Rgba8Unorm
        };

        const TEXT_IMG_DATA: &[u8] = include_bytes!("../../www/graphics/tallfont.bm2");

        let text_img = image::load_from_memory(TEXT_IMG_DATA).unwrap();

        let text_texture =
            Texture::new_2d(&device, text_img.width(), text_img.height(), view_format);

        Self::buffer_texture(&queue, &text_texture, &text_img.to_rgba8().as_bytes());

        let text_renderer = TextRenderer::new(&device, &text_texture, &mut sprite_renderer);
        let background_renderer = BackgroundRenderer::new(&device, &config.format, &depth_texture);
        let animation_renderer = AnimationRenderer::new();

        // Start camera at y 1024 so we slowly scroll up during join screen.
        let camera = Camera::new(
            size.width as f32,
            size.height as f32,
            glam::Vec2::new(0.0f32, 1024.0f32),
            1.0f32 / 16.0f32,
        );

        let ui_camera = Camera::new(
            size.width as f32,
            size.height as f32,
            glam::Vec2::new((size.width as f32) / 2.0f32, (size.height as f32) / 2.0f32),
            1.0f32,
        );

        Ok(Self {
            instance,
            device,
            queue,
            config,
            is_surface_configured: false,
            surface,
            depth_texture,
            camera,
            ui_camera,
            map_renderer,
            sprite_renderer,
            text_renderer,
            background_renderer,
            animation_renderer,
            render_map: false,
        })
    }

    pub fn render(
        &mut self,
        window: Arc<Window>,
        game_sprites: Option<&GameSprites>,
    ) -> Result<bool, RenderError> {
        if !self.is_surface_configured {
            return Ok(true);
        }

        // Grab the current texture in the swap chain so it can be rendered to and queued to be presented.
        let output_texture = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(surface_texture) => surface_texture,
            wgpu::CurrentSurfaceTexture::Suboptimal(surface_texture) => {
                drop(surface_texture);
                self.surface.configure(&self.device, &self.config);
                return Ok(true);
            }
            wgpu::CurrentSurfaceTexture::Timeout
            | wgpu::CurrentSurfaceTexture::Occluded
            | wgpu::CurrentSurfaceTexture::Validation => {
                return Ok(true);
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                let size = window.inner_size();

                self.resize(size.width, size.height);

                let redraw = size.width > 0 && size.height > 0;

                return Ok(redraw);
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                self.surface = match self.instance.create_surface(window.clone()) {
                    Ok(surface) => surface,
                    Err(e) => {
                        log::error!("{e}");
                        return Err(RenderError::LostDevice);
                    }
                };
                return Ok(true);
            }
        };

        let view = output_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_texture.view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });

            if let Some(game_sprites) = &game_sprites {
                self.animation_renderer.render(
                    &self.camera,
                    &mut self.sprite_renderer,
                    game_sprites,
                );
            }

            self.background_renderer
                .render(&mut render_pass, &self.camera, &self.queue);

            if self.render_map {
                self.map_renderer
                    .render(&mut render_pass, &self.camera, &self.queue);
            }

            self.sprite_renderer.render(&mut render_pass, &self.queue);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        window.pre_present_notify();
        output_texture.present();

        Ok(true)
    }

    pub fn size(&self) -> PhysicalSize<u32> {
        PhysicalSize {
            width: self.config.width,
            height: self.config.height,
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
            self.is_surface_configured = true;
        }

        self.depth_texture = Texture::new_depth(&self.device, &self.config);

        self.camera
            .set_surface_dimensions(width as f32, height as f32);

        self.ui_camera = Camera::new(
            width as f32,
            height as f32,
            glam::Vec2::new((width as f32) / 2.0f32, (height as f32) / 2.0f32),
            1.0f32,
        );
    }

    pub fn on_map_change(&mut self, map: &Map, bytes: &[u8]) {
        let tileset = MapTileset::new(bytes);

        self.map_renderer.set_map(map, &tileset, &self.queue);

        let x_start = 9 * 16;
        let x_end = x_start + 8 * 16;
        let y_start = 8 * 16;
        let y_end = y_start + 16;

        self.map_renderer.door_spriteset =
            SpriteSet::new_from_slice(self, &tileset.image, x_start, y_start, x_end, y_end, 8, 1);

        self.camera.position = glam::Vec2::new(0.0f32, 0.0f32);
    }

    pub fn draw_world_text(
        &mut self,
        text: &str,
        x_pixels: i32,
        y_pixels: i32,
        layer: Layer,
        color: TextColor,
        align: TextAlignment,
    ) {
        self.text_renderer.draw(
            &mut self.sprite_renderer,
            &self.camera,
            text,
            x_pixels,
            y_pixels,
            layer,
            color,
            align,
        );
    }

    pub fn draw_ui_text(
        &mut self,
        text: &str,
        x_pixels: i32,
        y_pixels: i32,
        layer: Layer,
        color: TextColor,
        align: TextAlignment,
    ) {
        self.text_renderer.draw(
            &mut self.sprite_renderer,
            &self.ui_camera,
            text,
            x_pixels,
            y_pixels,
            layer,
            color,
            align,
        );
    }

    pub fn get_texture_format(&self) -> wgpu::TextureFormat {
        if self.config.format.is_srgb() {
            wgpu::TextureFormat::Rgba8UnormSrgb
        } else {
            wgpu::TextureFormat::Rgba8Unorm
        }
    }

    pub fn buffer_texture(queue: &wgpu::Queue, texture: &Texture, data: &[u8]) {
        let width = texture.texture.width();
        let height = texture.texture.height();

        let texture_info = texture.texture.as_image_copy();
        queue.write_texture(
            texture_info,
            data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(texture.texture.width() * 4),
                rows_per_image: Some(texture.texture.height()),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
    }
}
