use std::sync::{Arc, Mutex};
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

use crate::render::{
    game_sprites::{GameSpriteLoader, GameSprites},
    layer::Layer,
    render_state::{RenderError, RenderState, RenderStateCreateError},
};
use crate::{
    client::Client,
    game_settings::GameSettings,
    input::{InputMapping, InputState},
    net::{
        connection::{ConnectionError, ConnectionState},
        packet::c2s::{RegistrationFormMessage, RegistrationSex},
    },
    platform::Platform,
    scenes::{SceneStack, menu_scene::MenuScene},
};

pub mod arena_settings;
pub mod attach;
pub mod chat;
pub mod checksum;
pub mod client;
pub mod clock;
pub mod exhaust;
pub mod flag;
pub mod game_settings;
pub mod input;
pub mod lvz;
pub mod map;
pub mod math;
pub mod net;
pub mod notification;
pub mod platform;
pub mod player;
pub mod powerball;
pub mod prize;
pub mod radar;
pub mod render;
pub mod rng;
pub mod scenes;
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

    pub game_settings: GameSettings,
}

struct ApplicationLoadingState {
    sprite_loader: GameSpriteLoader,
    input_mapping: Option<InputMapping>,
    platform: Option<Platform>,
}

impl ApplicationLoadingState {
    pub fn new() -> Self {
        let sprite_loader = GameSpriteLoader::new();
        let mut platform = Platform::new();

        InputMapping::request_load(&mut platform);

        Self {
            sprite_loader,
            platform: Some(platform),
            input_mapping: None,
        }
    }

    pub fn render(&mut self, render_state: &mut RenderState) {
        let x_pixels = render_state.width() / 2;
        let y_pixels = render_state.height() / 2;

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

struct ApplicationConnectErrorState {
    message: String,
}

impl ApplicationConnectErrorState {
    pub fn new(message: String) -> Self {
        Self { message }
    }

    pub fn render(&self, render_state: &mut RenderState) {
        let x_pixels = render_state.width() / 2;
        let y_pixels = render_state.height() / 2;

        render_state.draw_ui_text(
            &self.message,
            x_pixels as i32,
            y_pixels as i32,
            Layer::TopMost,
            render::text_renderer::TextColor::DarkRed,
            render::text_renderer::TextAlignment::Center,
        );
    }
}

struct ApplicationPlayingState {
    client: Arc<Mutex<Client>>,
    platform: Platform,
    timer: Timer, // Used for delta time calculations for client update.
    sprites: GameSprites,
    input_mapping: InputMapping,
    input_state: InputState,
    action_input: bool,
    scene_stack: SceneStack,
}

impl ApplicationPlayingState {
    pub fn new(
        config: &ApplicationConfig,
        sprites: GameSprites,
        platform: Platform,
        input_mapping: InputMapping,
    ) -> Self {
        let socket;

        #[cfg(not(target_arch = "wasm32"))]
        {
            let ip = config.remote_ip.clone().unwrap_or("127.0.0.1".to_string());
            let port = config.remote_port.unwrap_or(5000);

            log::debug!("Connecting to {}:{}", ip, port);

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

        let client = Arc::new(Mutex::new(client));
        let menu = Arc::new(Mutex::new(MenuScene::new(client.clone())));

        let mut scene_stack = SceneStack::new();

        scene_stack.add_scene(client.clone());
        scene_stack.add_scene(menu.clone());

        Self {
            client,
            platform,
            timer: Timer::new(),
            sprites,
            input_mapping,
            input_state: InputState::new(),
            action_input: false,
            scene_stack,
        }
    }

    pub fn update(
        &mut self,
        render_state: &mut RenderState,
        game_settings: &mut GameSettings,
    ) -> Result<(), ConnectionError> {
        let dt = self.timer.elapsed();

        let windows_open = self.scene_stack.get_active_scene_count() > 1;

        let mut client = self.client.lock().unwrap();

        render_state
            .animation_renderer
            .update(client.connection.get_game_tick());

        client.update(
            Some(render_state),
            &mut self.platform,
            game_settings,
            &mut self.input_state,
            dt,
        )?;

        self.sprites.colors.tick(client.connection.get_game_tick());

        self.action_input = false;

        let full_radar = client.radar.render_full;

        client
            .chat_controller
            .update_render_mode(windows_open, full_radar);

        Ok(())
    }

    pub fn render(&mut self, render_state: &mut RenderState, game_settings: &GameSettings) {
        self.scene_stack
            .render(game_settings, render_state, &mut self.sprites);
    }

    pub fn handle_key(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        game_settings: &mut GameSettings,
        code: KeyCode,
        is_pressed: bool,
    ) {
        self.action_input = self.scene_stack.handle_key(
            event_loop,
            &mut self.platform,
            &mut self.input_state,
            &mut self.input_mapping,
            game_settings,
            code,
            is_pressed,
        );
    }

    pub fn handle_text(&mut self, c: &SmolStr) {
        if !self.action_input {
            self.scene_stack.handle_text(&mut self.input_state, c);
        }
    }
}

enum ApplicationState {
    Loading(ApplicationLoadingState),
    Playing(ApplicationPlayingState),
    ConnectError(ApplicationConnectErrorState),
}

impl ApplicationConfig {
    pub fn new_web(
        proxy_url: String,
        proxy_hash: Vec<u8>,
        username: String,
        password: String,
        game_settings: GameSettings,
    ) -> Self {
        Self {
            proxy_url: Some(proxy_url),
            proxy_hash: Some(proxy_hash),
            remote_ip: None,
            remote_port: None,
            username,
            password,
            game_settings,
        }
    }

    pub fn new_exe(
        remote_ip: String,
        remote_port: u16,
        username: String,
        password: String,
        game_settings: GameSettings,
    ) -> Self {
        Self {
            proxy_url: None,
            proxy_hash: None,
            remote_ip: Some(remote_ip),
            remote_port: Some(remote_port),
            username,
            password,
            game_settings,
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
            ApplicationState::Playing(playing) => {
                playing.handle_key(event_loop, &mut self.config.game_settings, code, is_pressed)
            }
            ApplicationState::Loading(_) => {}
            ApplicationState::ConnectError(_) => {
                if is_pressed && code == KeyCode::Escape {
                    event_loop.exit();
                }
            }
        }
    }

    pub fn handle_text(&mut self, c: &SmolStr) {
        match &mut self.state {
            ApplicationState::Playing(playing) => playing.handle_text(c),
            ApplicationState::Loading(_) => {}
            ApplicationState::ConnectError(_) => {}
        }
    }

    pub fn update(&mut self) {
        match &mut self.state {
            ApplicationState::Playing(playing) => {
                if let Err(e) =
                    playing.update(&mut self.render_state, &mut self.config.game_settings)
                {
                    let client = playing.client.lock().unwrap();
                    let connection_state = client.connection.state;
                    drop(client);

                    match &connection_state {
                        ConnectionState::Playing | ConnectionState::Disconnected => {
                            //
                        }
                        _ => {
                            self.state = ApplicationState::ConnectError(
                                ApplicationConnectErrorState::new(e.to_string()),
                            );
                        }
                    }
                }
            }
            ApplicationState::Loading(loading) => {
                if loading.input_mapping.is_some() {
                    let sprites = loading.sprite_loader.try_create(&mut self.render_state);

                    if let Some(sprites) = sprites {
                        self.state = ApplicationState::Playing(ApplicationPlayingState::new(
                            &self.config,
                            sprites,
                            loading.platform.take().unwrap(),
                            loading.input_mapping.take().unwrap(),
                        ));
                    }
                } else {
                    if let Some(platform) = &mut loading.platform {
                        if platform.is_load_complete() {
                            let keybind_req = platform
                                .load_requests
                                .first()
                                .expect("keybinds should be loaded");

                            if let Some(data) = &keybind_req.result {
                                loading.input_mapping = Some(InputMapping::load(data));
                            } else {
                                let mut input_mapping = InputMapping::new();

                                input_mapping.register_defaults();

                                loading.input_mapping = Some(input_mapping);
                            }

                            platform.load_requests.clear();
                        }
                    }
                }
            }
            ApplicationState::ConnectError(_) => {}
        }
    }

    pub fn render(&mut self, window: Arc<Window>) -> Result<bool, RenderError> {
        match &mut self.state {
            ApplicationState::Playing(playing) => {
                playing.render(&mut self.render_state, &self.config.game_settings)
            }
            ApplicationState::Loading(loading) => loading.render(&mut self.render_state),
            ApplicationState::ConnectError(error) => error.render(&mut self.render_state),
        };

        let game_sprites = match &self.state {
            ApplicationState::Playing(playing) => Some(&playing.sprites),
            ApplicationState::Loading(_) => None,
            ApplicationState::ConnectError(_) => None,
        };

        self.render_state
            .render(window.clone(), game_sprites, &self.config.game_settings)
    }

    pub fn exiting(&mut self) {
        match &mut self.state {
            ApplicationState::Playing(playing) => {
                use crate::net::packet::Serialize;

                let packet = crate::net::packet::bi::DisconnectMessage {};

                let mut client = playing.client.lock().unwrap();

                if let Err(e) = client.connection.send_packet(&packet.serialize()) {
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

unsafe impl Send for ApplicationEvent {}

pub struct EventProcessor {
    application: Option<Application>,
    config: ApplicationConfig,

    #[cfg(target_arch = "wasm32")]
    proxy: Option<winit::event_loop::EventLoopProxy<ApplicationEvent>>,

    #[cfg(target_arch = "wasm32")]
    update_interval: crate::web_util::Interval,
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
            update_interval,
        }
    }
}

impl ApplicationHandler<ApplicationEvent> for EventProcessor {
    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(app) = &mut self.application {
            #[cfg(target_arch = "wasm32")]
            self.update_interval.clear();

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
pub struct WebExecuteContext {
    proxy: Option<winit::event_loop::EventLoopProxy<ApplicationEvent>>,
    quit_func: Option<js_sys::Function>,
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
impl WebExecuteContext {
    #[wasm_bindgen]
    pub fn request_update(&mut self) {
        let Some(proxy) = &self.proxy else {
            return;
        };

        if let Err(_) = proxy.send_event(ApplicationEvent::Update) {
            if let Some(quit_func) = &self.quit_func {
                if let Err(_) = quit_func.call0(&JsValue::NULL) {
                    log::error!("Failed to call javascript quit function.");
                }

                self.proxy = None;
            }
        }
    }

    #[wasm_bindgen(setter)]
    pub fn set_on_quit(&mut self, quit_func: js_sys::Function) {
        self.quit_func = Some(quit_func);
    }
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn execute_app(
    proxy_url: &str,
    proxy_hash: Vec<u8>,
    username: &str,
    password: &str,
    game_settings: GameSettings,
) -> WebExecuteContext {
    let config = ApplicationConfig::new_web(
        proxy_url.to_string(),
        proxy_hash,
        username.to_string(),
        password.to_string(),
        game_settings,
    );
    let event_loop: EventLoop<ApplicationEvent> = EventLoop::with_user_event()
        .build()
        .expect("event loop must be supported on this platform");

    let event_processor = EventProcessor::new(config, &event_loop);
    let update_proxy = WebExecuteContext {
        proxy: Some(event_loop.create_proxy()),
        quit_func: None,
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
