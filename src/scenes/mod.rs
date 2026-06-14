use std::sync::{Arc, Mutex};

use winit::keyboard::{KeyCode, SmolStr};

use crate::{
    game_settings::GameSettings,
    input::{InputMapping, InputState},
    platform::Platform,
    render::{game_sprites::GameSprites, render_state::RenderState},
};

pub mod game_scene;
pub mod keybind_scene;
pub mod menu_scene;

pub enum SceneKeyAction {
    Ignore,
    AddScene(Arc<Mutex<dyn Scene + Send + 'static>>),
    PopScene,
}

pub trait Scene {
    fn render(
        &mut self,
        game_settings: &GameSettings,
        render_state: &mut RenderState,
        sprites: &mut GameSprites,
    );

    // Returns true if this scene handled the key and any scenes below this shouldn't receive the input.
    fn handle_key(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        platform: &mut Platform,
        input_state: &mut InputState,
        input_mapping: &mut InputMapping,
        game_settings: &mut GameSettings,
        code: KeyCode,
        is_pressed: bool,
    ) -> Option<SceneKeyAction>;

    // Returns true if this scene handled the text and any scenes below this shouldn't receive the text.
    fn handle_text(&mut self, input_state: &mut InputState, c: &SmolStr) -> bool;

    fn is_active(&self) -> bool;
}

pub struct SceneStack {
    scenes: Vec<Arc<Mutex<dyn Scene + Send + 'static>>>,
}

impl SceneStack {
    pub fn new() -> Self {
        Self { scenes: vec![] }
    }

    pub fn add_scene(&mut self, scene: Arc<Mutex<dyn Scene + Send + 'static>>) {
        self.scenes.push(scene);
    }

    pub fn pop_scene(&mut self) {
        let _ = self.scenes.pop();
    }

    pub fn get_active_scene_count(&self) -> usize {
        let mut count = 0;

        for scene in &self.scenes {
            let scene = scene.lock().unwrap();

            if scene.is_active() {
                count += 1;
            }
        }

        count
    }

    pub fn render(
        &mut self,
        game_settings: &GameSettings,
        render_state: &mut RenderState,
        sprites: &mut GameSprites,
    ) {
        for scene in &mut self.scenes {
            let mut scene = scene.lock().unwrap();

            scene.render(game_settings, render_state, sprites);
        }
    }

    // Returns true if this scene handled the key and any scenes below this shouldn't receive the input.
    pub fn handle_key(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        platform: &mut Platform,
        input_state: &mut InputState,
        input_mapping: &mut InputMapping,
        game_settings: &mut GameSettings,
        code: KeyCode,
        is_pressed: bool,
    ) -> bool {
        for scene in &mut self.scenes.iter_mut().rev() {
            let mut scene = scene.lock().unwrap();

            if let Some(action) = scene.handle_key(
                event_loop,
                platform,
                input_state,
                input_mapping,
                game_settings,
                code,
                is_pressed,
            ) {
                drop(scene);

                match action {
                    SceneKeyAction::Ignore => {
                        return true;
                    }
                    SceneKeyAction::AddScene(new_scene) => {
                        self.add_scene(new_scene);

                        return true;
                    }
                    SceneKeyAction::PopScene => {
                        self.pop_scene();

                        return true;
                    }
                }
            }
        }

        false
    }

    // Returns true if this scene handled the text and any scenes below this shouldn't receive the text.
    pub fn handle_text(&mut self, input_state: &mut InputState, c: &SmolStr) {
        for scene in &mut self.scenes.iter_mut().rev() {
            let mut scene = scene.lock().unwrap();

            if scene.handle_text(input_state, c) {
                break;
            }
        }
    }
}
