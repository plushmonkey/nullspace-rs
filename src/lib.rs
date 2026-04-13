use std::sync::Arc;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

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

use crate::render::render_state::{RenderState, RenderStateCreateError};
use crate::{
    client::Client,
    net::packet::c2s::{RegistrationFormMessage, RegistrationSex},
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

#[cfg(target_arch = "wasm32")]
pub mod web_util;

#[derive(Default, Copy, Clone)]
struct Input {
    pub left: bool,
    pub right: bool,
    pub down: bool,
    pub up: bool,
    pub shift: bool,
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
    input: Input,

    timer: Timer, // Used for delta time calculations for client update.
    client: Client,

    render_state: RenderState,
    window: Arc<Window>,
}

impl Application {
    pub async fn new(window: Arc<Window>) -> Result<Self, RenderStateCreateError> {
        let render_state = RenderState::new(window.clone()).await?;

        #[cfg(not(target_arch = "wasm32"))]
        let socket = crate::net::udp_socket::UdpSocket::new("127.0.0.1", 5000).unwrap();
        #[cfg(target_arch = "wasm32")]
        let socket =
            crate::net::webtransport_socket::WebTransportSocket::new("https://127.0.0.1:4433")
                .unwrap();

        let registration = RegistrationFormMessage::new(
            "nullspace",
            "nullspace@nullspace.com",
            "nullspace city",
            "nullspace state",
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
            input: Input::default(),
            timer: Timer::new(),
            window,
            client,
            render_state,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.render_state.resize(width, height);
    }

    pub fn handle_key(&mut self, event_loop: &ActiveEventLoop, code: KeyCode, is_pressed: bool) {
        let _ = event_loop;

        match (code, is_pressed) {
            //(KeyCode::Escape, true) => event_loop.exit(),
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

        self.render_state.camera.position += offset * speed * dt;

        self.render_state.camera.position.x =
            self.render_state.camera.position.x.clamp(0.0f32, 1024.0f32);
        self.render_state.camera.position.y =
            self.render_state.camera.position.y.clamp(0.0f32, 1024.0f32);

        if let Err(e) = self.client.update() {
            log::error!("{e}");
        }
    }
}

pub enum ApplicationEvent {
    Application(Application),
    Update,
}

pub struct EventProcessor {
    application: Option<Application>,
    #[cfg(target_arch = "wasm32")]
    proxy: Option<winit::event_loop::EventLoopProxy<ApplicationEvent>>,

    #[cfg(target_arch = "wasm32")]
    _update_interval: crate::web_util::Interval,
}

impl EventProcessor {
    pub fn new(event_loop: &EventLoop<ApplicationEvent>) -> Self {
        #[cfg(target_arch = "wasm32")]
        let proxy = Some(event_loop.create_proxy());

        let interval_proxy = event_loop.create_proxy();

        #[cfg(target_arch = "wasm32")]
        let update_interval = crate::web_util::Interval::new(1, move || {
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

            // Allow keyboard events and right clicking on canvas.
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
            WindowEvent::RedrawRequested => match app.render_state.render(app.window.clone()) {
                Ok(redraw) => {
                    if redraw {
                        app.window.request_redraw();
                    }

                    // TODO: Remove this once vsync works properly on Windows.
                    // Only here now to reduce cpu/gpu spin, but it makes the game choppy.
                    // Could manually time vsync, but this works well enough for now.
                    #[cfg(not(target_arch = "wasm32"))]
                    std::thread::sleep(std::time::Duration::from_millis(5));
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
