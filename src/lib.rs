//#[cfg(not(target_arch = "wasm32"))]
//use ctrlc;

use std::sync::Arc;
use wgpu::{RequestAdapterOptions, wgt::DeviceDescriptor};
use winit::{
    application::ApplicationHandler,
    event::{KeyEvent, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::Window,
};

use thiserror::Error;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
#[cfg(target_arch = "wasm32")]
use winit::platform::web::EventLoopExtWebSys;

pub mod arena_settings;
pub mod checksum;
pub mod client;
pub mod clock;
pub mod map;
pub mod math;
pub mod net;
pub mod player;
pub mod prize;
pub mod rng;
pub mod ship;
pub mod simulation;
pub mod spawn;
pub mod weapon;

#[derive(Error, Debug)]
enum ApplicationCreateError {
    #[error("{0}")]
    CreateSurfaceError(#[from] wgpu::CreateSurfaceError),

    #[error("{0}")]
    RequestAdapterError(#[from] wgpu::RequestAdapterError),

    #[error("{0}")]
    RequestDeviceError(#[from] wgpu::RequestDeviceError),
}

#[derive(Error, Debug)]
enum ApplicationRenderError {
    #[error("{0}")]
    SurfaceError(#[from] wgpu::SurfaceError),
}

struct Application {
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    surface: wgpu::Surface<'static>,
    is_surface_configured: bool,
    window: Arc<Window>,
}

impl Application {
    pub async fn new(window: Arc<Window>) -> Result<Self, ApplicationCreateError> {
        let size = window.inner_size();

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            #[cfg(not(target_arch = "wasm32"))]
            backends: wgpu::Backends::PRIMARY,
            #[cfg(target_arch = "wasm32")]
            backends: wgpu::Backends::BROWSER_WEBGPU,
            flags: Default::default(),
            memory_budget_thresholds: Default::default(),
            backend_options: Default::default(),
        });

        let surface = instance.create_surface(window.clone())?;

        let adapter = instance
            .request_adapter(&RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance, // Chrome has a bug where HighPerformance is ignored, but specify it here anyway.
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await?;

        let (device, queue) = adapter
            .request_device(&DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::defaults(),
                memory_hints: Default::default(),
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                trace: wgpu::Trace::Off,
            })
            .await?;

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: surface_caps.present_modes[0], // TODO: Probably just want to use Fifo (works on all platforms) for vsync or make it configurable
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        Ok(Self {
            device,
            queue,
            config,
            surface,
            is_surface_configured: false,
            window,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
            self.is_surface_configured = true;
        }
    }

    pub fn handle_key(&self, event_loop: &ActiveEventLoop, code: KeyCode, is_pressed: bool) {
        match (code, is_pressed) {
            (KeyCode::Escape, true) => event_loop.exit(),
            _ => {}
        }
    }

    pub fn update(&mut self) {
        //
    }

    pub fn render(&mut self) -> Result<(), ApplicationRenderError> {
        self.window.request_redraw();

        if !self.is_surface_configured {
            return Ok(());
        }

        // Grab the current texture in the swap chain so it can be rendered to and queued to be presented.
        let output_texture = match self.surface.get_current_texture() {
            Ok(surface_texture) => surface_texture,
            Err(e) => {
                match e {
                    wgpu::SurfaceError::Outdated => {
                        self.surface.configure(&self.device, &self.config);
                        return Ok(());
                    }
                    wgpu::SurfaceError::Timeout => {
                        return Ok(());
                    }
                    _ => {}
                }

                return Err(e.into());
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
            let _render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.1,
                            g: 0.2,
                            b: 0.3,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output_texture.present();

        Ok(())
    }
}

struct EventProcessor {
    application: Option<Application>,
    #[cfg(target_arch = "wasm32")]
    proxy: Option<winit::event_loop::EventLoopProxy<Application>>,
}

impl EventProcessor {
    pub fn new(#[cfg(target_arch = "wasm32")] event_loop: &EventLoop<Application>) -> Self {
        #[cfg(target_arch = "wasm32")]
        let proxy = Some(event_loop.create_proxy());

        Self {
            application: None,
            #[cfg(target_arch = "wasm32")]
            proxy,
        }
    }
}

impl ApplicationHandler<Application> for EventProcessor {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        #[allow(unused_mut)]
        let mut window_attributes = Window::default_attributes().with_title("nullspace");

        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::JsCast;
            use winit::platform::web::WindowAttributesExtWebSys;

            const CANVAS_ID: &str = "canvas";

            let window = wgpu::web_sys::window().unwrap_throw();
            let document = window.document().unwrap_throw();
            let canvas = document.get_element_by_id(CANVAS_ID).unwrap_throw();
            let html_canvas_element = canvas.unchecked_into();

            window_attributes = window_attributes.with_canvas(Some(html_canvas_element));

            // Allow keyboard events and right clicking on canvas. TODO: Maybe disable later
            window_attributes = window_attributes.with_prevent_default(false);
        }

        let window = event_loop.create_window(window_attributes);

        let window = Arc::new(match window {
            Ok(window) => window,
            Err(err) => {
                panic!("Failed to create window: {}", err);
            }
        });

        #[cfg(not(target_arch = "wasm32"))]
        {
            self.application = Some(
                pollster::block_on(Application::new(window)).expect("unable to create surface"),
            );
        }

        #[cfg(target_arch = "wasm32")]
        {
            if let Some(proxy) = self.proxy.take() {
                wasm_bindgen_futures::spawn_local(async move {
                    assert!(
                        proxy
                            .send_event(
                                Application::new(window)
                                    .await
                                    .expect("unable to create surface")
                            )
                            .is_ok()
                    )
                });
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        let app = match &mut self.application {
            Some(app) => app,
            None => return,
        };

        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                app.update();

                match app.render() {
                    Ok(_) => {}
                    Err(e) => {
                        log::error!("{e}");
                        event_loop.exit();
                    }
                }
            }
            WindowEvent::Resized(size) => {
                app.resize(size.width, size.height);
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(code),
                        state: key_state,
                        ..
                    },
                ..
            } => app.handle_key(event_loop, code, key_state.is_pressed()),
            _ => {}
        }
    }

    #[allow(unused_mut)]
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, mut event: Application) {
        #[cfg(target_arch = "wasm32")]
        {
            event.window.request_redraw();
            event.resize(
                event.window.inner_size().width,
                event.window.inner_size().height,
            );
        }
        self.application = Some(event);
    }
}

pub fn run() {
    #[cfg(not(target_arch = "wasm32"))]
    {
        env_logger::init();
    }
    #[cfg(target_arch = "wasm32")]
    {
        console_log::init_with_level(log::Level::Info).unwrap_throw();
    }

    let event_loop: EventLoop<Application> = EventLoop::with_user_event()
        .build()
        .expect("event loop must be supported on this platform");
    #[cfg(not(target_arch = "wasm32"))]
    {
        event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);
        let mut event_processor = EventProcessor::new();
        event_loop
            .run_app(&mut event_processor)
            .expect("event loop should run");
    }

    #[cfg(target_arch = "wasm32")]
    {
        let event_processor = EventProcessor::new(&event_loop);
        event_loop.spawn_app(event_processor);
    }
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn run_web() -> Result<(), wasm_bindgen::JsValue> {
    console_error_panic_hook::set_once();

    run();

    Ok(())
}
