//#[cfg(not(target_arch = "wasm32"))]
//use ctrlc;

use std::sync::Arc;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

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

use crate::render::map_renderer::MapRenderer;
use crate::{
    client::Client,
    net::packet::c2s::{RegistrationFormMessage, RegistrationSex},
    render::camera::Camera,
};

pub mod arena_settings;
pub mod checksum;
pub mod client;
pub mod clock;
pub mod map;
pub mod math;
pub mod net;
pub mod player;
pub mod prize;
pub mod render;
pub mod rng;
pub mod ship;
pub mod simulation;
pub mod spawn;
pub mod weapon;

#[derive(Default, Copy, Clone)]
struct Input {
    pub left: bool,
    pub right: bool,
    pub down: bool,
    pub up: bool,
    pub shift: bool,
}

#[derive(Error, Debug)]
pub enum ApplicationCreateError {
    #[error("{0}")]
    CreateSurfaceError(#[from] wgpu::CreateSurfaceError),

    #[error("{0}")]
    RequestAdapterError(#[from] wgpu::RequestAdapterError),

    #[error("{0}")]
    RequestDeviceError(#[from] wgpu::RequestDeviceError),
}

#[derive(Error, Debug)]
pub enum ApplicationRenderError {
    #[error("lost device")]
    LostDevice,
}

struct Timer {
    #[cfg(not(target_arch = "wasm32"))]
    last_update_time: Instant,

    #[cfg(target_arch = "wasm32")]
    last_update_time: f64,
}

impl Timer {
    pub fn new() -> Self {
        Self {
            #[cfg(not(target_arch = "wasm32"))]
            last_update_time: Instant::now(),
            #[cfg(target_arch = "wasm32")]
            last_update_time: web_sys::window().unwrap().performance().unwrap().now(),
        }
    }

    // Returns time since last elapsed call and updates timer value to 'now'.
    pub fn elapsed(&mut self) -> f32 {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let elapsed = self.last_update_time.elapsed().as_secs_f32();

            self.last_update_time = Instant::now();

            elapsed
        }

        #[cfg(target_arch = "wasm32")]
        {
            let now = web_sys::window().unwrap().performance().unwrap().now();
            let elapsed = now - self.last_update_time;

            self.last_update_time = now;

            (elapsed / 1000.0) as f32
        }
    }
}

pub struct Application {
    instance: wgpu::Instance,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    surface: wgpu::Surface<'static>,
    is_surface_configured: bool,

    camera: Camera,
    map_renderer: MapRenderer,
    input: Input,

    timer: Timer,

    window: Arc<Window>,
    client: Client,
}

impl Application {
    pub async fn new(window: Arc<Window>) -> Result<Self, ApplicationCreateError> {
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

        let mut map_renderer = MapRenderer::new(&device, &config.format);

        let map_bytes = include_bytes!("../map.lvl");
        let map = map::Map::new("map.lvl", map_bytes).expect("included map should load");
        let map_tileset = render::map_renderer::MapTileset::new(map_bytes);

        map_renderer.set_map(&map, &map_tileset, &queue);

        let camera = Camera::new(
            size.width as f32,
            size.height as f32,
            glam::Vec2::new(512.0f32, 512.0f32),
            1.0f32 / 16.0f32,
        );

        #[cfg(not(target_arch = "wasm32"))]
        let socket = crate::net::udp_socket::UdpSocket::new("127.0.0.1", 5000).unwrap();
        #[cfg(target_arch = "wasm32")]
        let socket =
            crate::net::webtransport_socket::WebTransportSocket::new("https://127.0.0.1:4433")
                .unwrap();

        let registration = RegistrationFormMessage::new(
            "puppet",
            "puppet@puppet.com",
            "puppet city",
            "puppet state",
            RegistrationSex::Female,
            20,
        );

        let client = Client::new(
            "nullspace",
            "password",
            "zone_name",
            #[cfg(not(target_arch = "wasm32"))]
            net::connection::SocketKind::Udp(socket),
            #[cfg(target_arch = "wasm32")]
            net::connection::SocketKind::WebTransport(socket),
            registration,
        )
        .unwrap();

        Ok(Self {
            instance,
            device,
            queue,
            config,
            surface,
            is_surface_configured: false,
            map_renderer,
            camera,
            input: Input::default(),
            timer: Timer::new(),
            window,
            client,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
            self.is_surface_configured = true;

            self.camera
                .set_surface_dimensions(width as f32, height as f32);
        }
    }

    pub fn handle_key(&mut self, event_loop: &ActiveEventLoop, code: KeyCode, is_pressed: bool) {
        match (code, is_pressed) {
            (KeyCode::Escape, true) => event_loop.exit(),
            _ => {}
        }

        match code {
            KeyCode::ArrowLeft => {
                self.input.left = is_pressed;
            }
            KeyCode::ArrowRight => {
                self.input.right = is_pressed;
            }
            KeyCode::ArrowDown => {
                self.input.down = is_pressed;
            }
            KeyCode::ArrowUp => {
                self.input.up = is_pressed;
            }
            KeyCode::ShiftLeft | KeyCode::ShiftRight => {
                self.input.shift = is_pressed;
            }
            _ => {}
        }
    }

    pub fn update(&mut self) {
        const CAMERA_SPEED: f32 = 100.0f32;

        let dt = self.timer.elapsed();

        let mut offset = glam::Vec2::ZERO;

        if self.input.down {
            offset.y += 1.0f32;
        }
        if self.input.up {
            offset.y -= 1.0f32;
        }
        if self.input.right {
            offset.x += 1.0f32;
        }
        if self.input.left {
            offset.x -= 1.0f32;
        }

        let speed = if self.input.shift {
            CAMERA_SPEED * 3.0f32
        } else {
            CAMERA_SPEED
        };

        self.camera.position += offset * speed * dt;

        self.camera.position.x = self.camera.position.x.clamp(0.0f32, 1024.0f32);
        self.camera.position.y = self.camera.position.y.clamp(0.0f32, 1024.0f32);

        self.map_renderer.update(&self.camera, &self.queue);
        if let Err(e) = self.client.update() {
            log::error!("{e}");
        }
    }

    pub fn render(&mut self) -> Result<bool, ApplicationRenderError> {
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
                let size = self.window.inner_size();

                self.resize(size.width, size.height);

                let redraw = size.width > 0 && size.height > 0;

                return Ok(redraw);
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                self.surface = match self.instance.create_surface(self.window.clone()) {
                    Ok(surface) => surface,
                    Err(e) => {
                        log::error!("{e}");
                        return Err(ApplicationRenderError::LostDevice);
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
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });

            self.map_renderer.render(&mut render_pass);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        self.window.pre_present_notify();
        output_texture.present();

        Ok(true)
    }
}

pub enum ApplicationEvent {
    Application(Application),
    Update,
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
extern "C" {
    fn setInterval(closure: &Closure<dyn FnMut()>, millis: u32) -> f64;
    fn clearInterval(token: f64);
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub struct Interval {
    _closure: Closure<dyn FnMut()>,
    token: f64,
}

#[cfg(target_arch = "wasm32")]
impl Interval {
    pub fn new<F: 'static>(millis: u32, f: F) -> Interval
    where
        F: FnMut(),
    {
        let closure = Closure::new(f);
        let token = setInterval(&closure, millis);

        Interval {
            _closure: closure,
            token,
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl Drop for Interval {
    fn drop(&mut self) {
        clearInterval(self.token);
    }
}

pub struct EventProcessor {
    application: Option<Application>,
    #[cfg(target_arch = "wasm32")]
    proxy: Option<winit::event_loop::EventLoopProxy<ApplicationEvent>>,

    #[cfg(target_arch = "wasm32")]
    _update_interval: Interval,
}

impl EventProcessor {
    pub fn new(event_loop: &EventLoop<ApplicationEvent>) -> Self {
        #[cfg(target_arch = "wasm32")]
        let proxy = Some(event_loop.create_proxy());

        let interval_proxy = event_loop.create_proxy();

        #[cfg(target_arch = "wasm32")]
        let update_interval = Interval::new(1, move || {
            let _ = interval_proxy.send_event(ApplicationEvent::Update);
        });

        #[cfg(not(target_arch = "wasm32"))]
        std::thread::spawn(move || {
            loop {
                let _ = interval_proxy.send_event(ApplicationEvent::Update);
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        });

        Self {
            application: None,
            #[cfg(target_arch = "wasm32")]
            proxy,

            #[cfg(target_arch = "wasm32")]
            _update_interval: update_interval,
        }
    }
}

impl ApplicationHandler<ApplicationEvent> for EventProcessor {
    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(app) = &mut self.application {
            use crate::net::packet::Serialize;

            let packet = crate::net::packet::bi::DisconnectMessage {};

            if let Err(e) = app.client.connection.send_packet(&packet.serialize()) {
                log::error!("{e}");
            }
        }
    }
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
                            .send_event(ApplicationEvent::Application(
                                Application::new(window)
                                    .await
                                    .expect("unable to create surface")
                            ))
                            .is_ok()
                    );
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
            WindowEvent::RedrawRequested => match app.render() {
                Ok(redraw) => {
                    if redraw {
                        app.window.request_redraw();
                    }
                }
                Err(e) => {
                    log::error!("{e}");
                    event_loop.exit();
                }
            },
            WindowEvent::Resized(size) => {
                app.resize(size.width, size.height);

                #[cfg(not(target_arch = "wasm32"))]
                {
                    // Switch to waiting for events when minimized so we don't spin cpu.
                    if size.width > 0 && size.height > 0 {
                        event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);
                    } else {
                        event_loop.set_control_flow(winit::event_loop::ControlFlow::Wait);
                    }
                }
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
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, mut event: ApplicationEvent) {
        match event {
            ApplicationEvent::Application(mut application) => {
                #[cfg(target_arch = "wasm32")]
                {
                    application.window.request_redraw();
                    application.resize(
                        application.window.inner_size().width,
                        application.window.inner_size().height,
                    );
                }
                self.application = Some(application);
            }
            ApplicationEvent::Update => {
                if let Some(app) = &mut self.application {
                    app.update();
                }
            }
        }
    }
}

pub fn run() {
    #[cfg(not(target_arch = "wasm32"))]
    {
        env_logger::Builder::new()
            .filter(None, log::LevelFilter::Warn)
            .filter(Some("nullspace"), log::LevelFilter::Debug)
            .init()
    }

    #[cfg(target_arch = "wasm32")]
    {
        console_log::init_with_level(log::Level::Debug).unwrap_throw();
    }

    let event_loop: EventLoop<ApplicationEvent> = EventLoop::with_user_event()
        .build()
        .expect("event loop must be supported on this platform");
    #[cfg(not(target_arch = "wasm32"))]
    {
        event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);
        let mut event_processor = EventProcessor::new(&event_loop);

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
