use std::sync::Arc;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

use winit::{
    application::ApplicationHandler,
    event::{KeyEvent, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode, PhysicalKey, SmolStr},
    window::Window,
};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
#[cfg(target_arch = "wasm32")]
use winit::platform::web::EventLoopExtWebSys;

use crate::{
    client::Client,
    input::{InputMapping, InputState},
    menu::MenuAction,
    net::packet::c2s::{RegistrationFormMessage, RegistrationSex},
};
use crate::{
    menu::Menu,
    render::{
        game_sprites::{GameSpriteLoader, GameSprites},
        layer::Layer,
        render_state::{RenderError, RenderState, RenderStateCreateError},
    },
};

pub mod arena_settings;
pub mod chat;
pub mod checksum;
pub mod client;
pub mod clock;
pub mod input;
pub mod map;
pub mod math;
pub mod menu;
pub mod net;
pub mod player;
pub mod powerball;
pub mod prize;
pub mod radar;
pub mod render;
pub mod rng;
pub mod select_box;
pub mod ship;
pub mod ship_controller;
pub mod simulation;
pub mod spawn;
pub mod spectate_controller;
pub mod statbox;
pub mod weapon;

#[cfg(target_arch = "wasm32")]
pub mod web_util;

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

#[derive(Clone, Debug)]
pub struct ApplicationConfig {
    pub proxy_url: Option<String>,
    pub proxy_hash: Option<Vec<u8>>,

    pub remote_ip: Option<String>,
    pub remote_port: Option<u16>,

    pub username: String,
    pub password: String,
}

struct ApplicationLoadingState {
    sprite_loader: GameSpriteLoader,
}

impl ApplicationLoadingState {
    pub fn new() -> Self {
        let sprite_loader = GameSpriteLoader::new();

        Self { sprite_loader }
    }

    pub fn render(&mut self, render_state: &mut RenderState) {
        let x_pixels = render_state.config.width / 2;
        let y_pixels = render_state.config.height / 2;

        render_state.draw_ui_text(
            "Loading",
            x_pixels as i32,
            y_pixels as i32,
            Layer::TopMost,
            render::text_renderer::TextColor::Pink,
            render::text_renderer::TextAlignment::Center,
        );
    }
}

struct ApplicationPlayingState {
    timer: Timer, // Used for delta time calculations for client update.
    client: Client,
    sprites: GameSprites,
    menu: Menu,
    input_mapping: InputMapping,
    input_state: InputState,
}

impl ApplicationPlayingState {
    pub fn new(config: &ApplicationConfig, sprites: GameSprites) -> Self {
        let socket;

        #[cfg(not(target_arch = "wasm32"))]
        {
            let ip = config.remote_ip.clone().unwrap_or("127.0.0.1".to_string());
            let port = config.remote_port.unwrap_or(5000);

            log::info!("Connecting to {}:{}", ip, port);

            socket = crate::net::udp_socket::UdpSocket::new(&ip.trim(), port).unwrap();
        }
        #[cfg(target_arch = "wasm32")]
        {
            let proxy_url = config
                .proxy_url
                .clone()
                .unwrap_or("https://127.0.0.1:4433".to_string());

            let mut hash = None;

            if let Some(proxy_hash) = &config.proxy_hash {
                hash = Some(proxy_hash);
            }

            socket =
                crate::net::webtransport_socket::WebTransportSocket::new(&proxy_url, hash).unwrap();
        }

        let registration = RegistrationFormMessage::new(
            "nullspace",
            "nullspace@nullspace.com",
            "nullspace city",
            "nullspace state",
            RegistrationSex::Female,
            20,
        );

        let client = Client::new(
            &config.username,
            &config.password,
            "zone_name",
            #[cfg(not(target_arch = "wasm32"))]
            net::connection::SocketKind::Udp(socket),
            #[cfg(target_arch = "wasm32")]
            net::connection::SocketKind::WebTransport(socket),
            registration,
        )
        .unwrap();

        let mut input_mapping = InputMapping::new();

        input_mapping.register_defaults();

        Self {
            timer: Timer::new(),
            client,
            sprites,
            menu: Menu::new(),
            input_mapping,
            input_state: InputState::new(),
        }
    }

    pub fn update(&mut self, render_state: &mut RenderState) {
        let dt = self.timer.elapsed();

        render_state
            .animation_renderer
            .update(self.client.connection.get_game_tick());

        if let Err(e) = self
            .client
            .update(Some(render_state), &mut self.input_state, dt)
        {
            log::error!("{e}");
        };

        self.input_state.clear_triggered();

        self.sprites
            .colors
            .tick(self.client.connection.get_game_tick());

        self.menu.handled = false;
        self.client.chat_controller.full_history = self.menu.is_open();
    }

    pub fn render(&mut self, render_state: &mut RenderState) {
        self.client.render(render_state, &self.sprites);

        if self.menu.is_open() {
            self.menu.render(render_state, &self.sprites);
        }
    }

    pub fn handle_key(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        code: KeyCode,
        is_pressed: bool,
    ) {
        match code {
            KeyCode::ShiftLeft | KeyCode::ShiftRight => {
                if is_pressed {
                    self.input_state
                        .set_modifier_triggered(input::InputModifier::Shift);
                }

                self.input_state
                    .set_modifier_down(input::InputModifier::Shift, is_pressed);
            }
            KeyCode::AltLeft | KeyCode::AltRight => {
                if is_pressed {
                    self.input_state
                        .set_modifier_triggered(input::InputModifier::Alt);
                }

                self.input_state
                    .set_modifier_down(input::InputModifier::Alt, is_pressed);
            }
            KeyCode::ControlLeft | KeyCode::ControlRight => {
                if is_pressed {
                    self.input_state
                        .set_modifier_triggered(input::InputModifier::Control);
                }

                self.input_state
                    .set_modifier_down(input::InputModifier::Control, is_pressed);
            }
            _ => {}
        }

        if let Some(action) = self.input_mapping.get_action(code, &self.input_state) {
            if is_pressed {
                self.input_state.set_triggered(action);
            }

            self.input_state.set_down(action, is_pressed);
        }

        if is_pressed {
            if let Some(action) = self.menu.handle_key(code) {
                match action {
                    MenuAction::MenuToggle => {
                        if !self.client.statbox.cancel_select_box() {
                            self.menu.toggle();
                        }
                    }
                    MenuAction::Quit => {
                        event_loop.exit();
                    }
                    MenuAction::ArenaList => {
                        self.client
                            .chat_controller
                            .send_public(&mut self.client.connection, "?arena");
                    }
                    MenuAction::ShipRequest(ship_kind) => {
                        let ship_request =
                            crate::net::packet::c2s::RequestShipMessage { ship_kind };
                        if let Err(e) = self.client.connection.send_reliable(&ship_request) {
                            log::error!("{e}");
                        }
                    }
                    _ => {}
                }

                return;
            }
        }
    }

    pub fn handle_text(&mut self, c: &SmolStr) {
        if c.is_empty() {
            return;
        }

        let code = c.as_bytes()[0];

        // If we receive some text that isn't Escape, close the menu and proceed to handle it.
        if code != 0x1B && self.menu.is_open() {
            self.menu.toggle();
        }

        if self.menu.handled {
            return;
        }

        // If we press enter with the chat controller empty, handle select box.
        if code == 0x0d {
            if self.client.chat_controller.input.is_empty() {
                if let Some(input_text) = self.client.statbox.activate_select_box() {
                    for c in input_text.as_bytes() {
                        self.client.chat_controller.input.push(*c);
                    }

                    self.client
                        .chat_controller
                        .send_input(&mut self.client.connection);
                }
                return;
            }
        }

        if self.client.chat_controller.handle_key(
            code,
            self.input_state
                .is_modifier_down(input::InputModifier::Control),
        ) {
            self.client
                .chat_controller
                .send_input(&mut self.client.connection);
        }
    }
}

enum ApplicationState {
    Loading(ApplicationLoadingState),
    Playing(ApplicationPlayingState),
}

impl ApplicationConfig {
    pub fn new_web(
        proxy_url: String,
        proxy_hash: Vec<u8>,
        username: String,
        password: String,
    ) -> Self {
        Self {
            proxy_url: Some(proxy_url),
            proxy_hash: Some(proxy_hash),
            remote_ip: None,
            remote_port: None,
            username,
            password,
        }
    }

    pub fn new_exe(
        remote_ip: String,
        remote_port: u16,
        username: String,
        password: String,
    ) -> Self {
        Self {
            proxy_url: None,
            proxy_hash: None,
            remote_ip: Some(remote_ip),
            remote_port: Some(remote_port),
            username,
            password,
        }
    }
}

pub struct Application {
    config: ApplicationConfig,
    state: ApplicationState,

    render_state: RenderState,
    window: Arc<Window>,
}

impl Application {
    pub async fn new(
        config: ApplicationConfig,
        window: Arc<Window>,
    ) -> Result<Self, RenderStateCreateError> {
        let render_state = RenderState::new(window.clone()).await?;

        let state = ApplicationState::Loading(ApplicationLoadingState::new());

        Ok(Self {
            config,
            state,
            window,
            render_state,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.render_state.resize(width, height);
    }

    pub fn handle_key(&mut self, event_loop: &ActiveEventLoop, code: KeyCode, is_pressed: bool) {
        let _ = event_loop;

        match &mut self.state {
            ApplicationState::Playing(playing) => playing.handle_key(event_loop, code, is_pressed),
            ApplicationState::Loading(_) => {}
        }
    }

    pub fn handle_text(&mut self, c: &SmolStr) {
        match &mut self.state {
            ApplicationState::Playing(playing) => playing.handle_text(c),
            ApplicationState::Loading(_) => {}
        }
    }

    pub fn update(&mut self) {
        match &mut self.state {
            ApplicationState::Playing(playing) => playing.update(&mut self.render_state),
            ApplicationState::Loading(loading) => {
                let sprites = loading.sprite_loader.try_create(&mut self.render_state);

                if let Some(sprites) = sprites {
                    self.state = ApplicationState::Playing(ApplicationPlayingState::new(
                        &self.config,
                        sprites,
                    ));
                }
            }
        }
    }

    pub fn render(&mut self, window: Arc<Window>) -> Result<bool, RenderError> {
        match &mut self.state {
            ApplicationState::Playing(playing) => playing.render(&mut self.render_state),
            ApplicationState::Loading(loading) => loading.render(&mut self.render_state),
        };

        let game_sprites = match &self.state {
            ApplicationState::Playing(playing) => Some(&playing.sprites),
            ApplicationState::Loading(_) => None,
        };

        self.render_state.render(window.clone(), game_sprites)
    }

    pub fn exiting(&mut self) {
        match &mut self.state {
            ApplicationState::Playing(playing) => {
                use crate::net::packet::Serialize;

                let packet = crate::net::packet::bi::DisconnectMessage {};

                if let Err(e) = playing.client.connection.send_packet(&packet.serialize()) {
                    log::error!("{e}");
                }
            }
            _ => {}
        }
    }
}

pub enum ApplicationEvent {
    Application(Application),
    Update,
}

pub struct EventProcessor {
    application: Option<Application>,
    config: ApplicationConfig,

    #[cfg(target_arch = "wasm32")]
    proxy: Option<winit::event_loop::EventLoopProxy<ApplicationEvent>>,

    #[cfg(target_arch = "wasm32")]
    _update_interval: crate::web_util::Interval,
}

impl EventProcessor {
    pub fn new(config: ApplicationConfig, event_loop: &EventLoop<ApplicationEvent>) -> Self {
        #[cfg(target_arch = "wasm32")]
        let proxy = Some(event_loop.create_proxy());

        let interval_proxy: winit::event_loop::EventLoopProxy<ApplicationEvent> =
            event_loop.create_proxy();

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
            config,
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
            app.exiting();
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

            // Block events from cascading to browser window.
            window_attributes = window_attributes.with_prevent_default(true);
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
                pollster::block_on(Application::new(self.config.clone(), window))
                    .expect("unable to create surface"),
            );
        }

        #[cfg(target_arch = "wasm32")]
        {
            if let Some(proxy) = self.proxy.take() {
                let config = self.config.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    assert!(
                        proxy
                            .send_event(ApplicationEvent::Application(
                                Application::new(config, window)
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
            WindowEvent::RedrawRequested => {
                match app.render(app.window.clone()) {
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
                }
            }
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
                        logical_key,
                        state: key_state,
                        text,
                        ..
                    },
                ..
            } => {
                app.handle_key(event_loop, code, key_state.is_pressed());

                if key_state.is_pressed() {
                    if let Some(text) = &text {
                        app.handle_text(text);
                    } else {
                        // Web doesn't seem to handle Backspace into text correctly, so catch it here.
                        match logical_key {
                            winit::keyboard::Key::Named(named) => {
                                if let winit::keyboard::NamedKey::Backspace = named {
                                    app.handle_text(&SmolStr::new_inline("\u{08}"));
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
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

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub struct WebUpdateProxy {
    proxy: winit::event_loop::EventLoopProxy<ApplicationEvent>,
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
impl WebUpdateProxy {
    #[wasm_bindgen]
    pub fn request_update(&self) {
        if let Err(e) = self.proxy.send_event(ApplicationEvent::Update) {
            log::error!("{e}");
        }
    }
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn execute_app(
    proxy_url: &str,
    proxy_hash: Vec<u8>,
    username: &str,
    password: &str,
) -> WebUpdateProxy {
    let config = ApplicationConfig::new_web(
        proxy_url.to_string(),
        proxy_hash,
        username.to_string(),
        password.to_string(),
    );
    let event_loop: EventLoop<ApplicationEvent> = EventLoop::with_user_event()
        .build()
        .expect("event loop must be supported on this platform");

    let event_processor = EventProcessor::new(config, &event_loop);
    let update_proxy = WebUpdateProxy {
        proxy: event_loop.create_proxy(),
    };

    event_loop.spawn_app(event_processor);

    update_proxy
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn run_web() -> Result<(), wasm_bindgen::JsValue> {
    console_error_panic_hook::set_once();
    console_log::init_with_level(log::Level::Debug).unwrap_throw();

    Ok(())
}
