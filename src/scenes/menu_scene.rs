use std::sync::{Arc, Mutex};

use winit::keyboard::{KeyCode, SmolStr};

use crate::{
    client::Client,
    game_settings::GameSettings,
    input::{InputMapping, InputState, is_input_keycode},
    platform::Platform,
    render::{
        colors::ColorRenderableKind,
        game_sprites::GameSprites,
        layer::Layer,
        render_state::RenderState,
        text_renderer::{FontKind, TextAlignment, TextColor},
    },
    scenes::{Scene, SceneKeyAction, keybind_scene::KeybindScene},
    ship::ShipKind,
};

pub enum MenuAction {
    MenuToggle,
    Quit,
    Help,
    Statbox,
    NameTags,
    Radar,
    Messages,
    HelpTicker,
    EngineSounds,
    ArenaList,
    SetBanner,
    IgnoreMacros,
    AdjustStatBoxUp,
    AdjustStatBoxDown,
    ShipRequest(ShipKind),
    Keybind,
}

pub struct MenuScene {
    client: Arc<Mutex<Client>>,
    active: bool,
}

impl MenuScene {
    pub fn new(client: Arc<Mutex<Client>>) -> Self {
        Self {
            client,
            active: false,
        }
    }

    fn get_action(&mut self, code: KeyCode) -> Option<MenuAction> {
        if code == KeyCode::Escape {
            return Some(MenuAction::MenuToggle);
        }

        if !self.active {
            return None;
        }

        let (result, close) = match code {
            KeyCode::KeyQ => (Some(MenuAction::Quit), true),
            KeyCode::F1 => (Some(MenuAction::Help), true),
            KeyCode::F2 => (Some(MenuAction::Statbox), false),
            KeyCode::F3 => (Some(MenuAction::NameTags), false),
            KeyCode::F4 => (Some(MenuAction::Radar), false),
            KeyCode::F5 => (Some(MenuAction::Messages), false),
            KeyCode::F6 => (Some(MenuAction::HelpTicker), false),
            KeyCode::F8 => (Some(MenuAction::EngineSounds), false),
            KeyCode::KeyA => (Some(MenuAction::ArenaList), true),
            KeyCode::KeyB => (Some(MenuAction::SetBanner), true),
            KeyCode::KeyI => (Some(MenuAction::IgnoreMacros), false),
            KeyCode::Digit1 => (Some(MenuAction::ShipRequest(ShipKind::Warbird)), true),
            KeyCode::Digit2 => (Some(MenuAction::ShipRequest(ShipKind::Javelin)), true),
            KeyCode::Digit3 => (Some(MenuAction::ShipRequest(ShipKind::Spider)), true),
            KeyCode::Digit4 => (Some(MenuAction::ShipRequest(ShipKind::Leviathan)), true),
            KeyCode::Digit5 => (Some(MenuAction::ShipRequest(ShipKind::Terrier)), true),
            KeyCode::Digit6 => (Some(MenuAction::ShipRequest(ShipKind::Weasel)), true),
            KeyCode::Digit7 => (Some(MenuAction::ShipRequest(ShipKind::Lancaster)), true),
            KeyCode::Digit8 => (Some(MenuAction::ShipRequest(ShipKind::Shark)), true),
            KeyCode::KeyS => (Some(MenuAction::ShipRequest(ShipKind::Spectator)), true),
            KeyCode::KeyK => (Some(MenuAction::Keybind), true),
            _ => (None, is_input_keycode(code)),
        };

        if close {
            self.active = false;
        }

        result
    }
}

impl Scene for MenuScene {
    fn is_active(&self) -> bool {
        self.active
    }

    fn render(
        &mut self,
        game_settings: &GameSettings,
        render_state: &mut RenderState,
        sprites: &mut GameSprites,
    ) {
        if !self.active {
            return;
        }

        let _ = game_settings;

        let x_center = render_state.width() as i32 / 2;
        let menu_width = 284;
        let menu_height = 170;

        let start_x = x_center - menu_width / 2;
        let start_y = 2;
        let end_x = x_center + menu_width / 2;
        let end_y = start_y + menu_height;

        let font_height = render_state
            .text_renderer
            .character_height(FontKind::Normal);

        sprites.colors.draw_border(
            &mut render_state.sprite_renderer,
            &render_state.ui_camera,
            Layer::AfterGauges,
            start_x,
            start_y,
            end_x,
            end_y,
            true,
        );

        let mut current_y = 2;

        render_state.text_renderer.draw(
            &mut render_state.sprite_renderer,
            &render_state.ui_camera,
            "-= Menu =-",
            x_center,
            start_y + 1,
            Layer::TopMost,
            TextColor::Green,
            TextAlignment::Center,
        );

        current_y += font_height;

        sprites.colors.draw(
            &mut render_state.sprite_renderer,
            &render_state.ui_camera,
            Layer::TopMost,
            ColorRenderableKind::BorderInner,
            start_x,
            current_y,
            menu_width,
            1,
        );

        current_y += 2;

        let right_side_y = current_y;

        let left_messages = [
            "Q  = Quit",
            "F1 = Help",
            "F2 = Stat box",
            "F3 = Name tags",
            "F4 = Radar",
            "F5 = Messages",
            "F6 = Help ticker",
            "F8 = Engine sounds",
            " A = Arena List",
            " B = Set Banner",
            " K = Set keybinds",
        ];

        for message in left_messages {
            render_state.text_renderer.draw(
                &mut render_state.sprite_renderer,
                &render_state.ui_camera,
                message,
                start_x + 3,
                current_y,
                Layer::TopMost,
                TextColor::White,
                TextAlignment::Left,
            );

            current_y += font_height;
        }

        render_state.text_renderer.draw(
            &mut render_state.sprite_renderer,
            &render_state.ui_camera,
            "PgUp/PgDn = Adjust stat box",
            start_x + 3,
            current_y,
            Layer::TopMost,
            TextColor::White,
            TextAlignment::Left,
        );

        current_y += font_height;

        render_state.text_renderer.draw(
            &mut render_state.sprite_renderer,
            &render_state.ui_camera,
            "Any other key to resume game",
            x_center,
            current_y,
            Layer::TopMost,
            TextColor::Yellow,
            TextAlignment::Center,
        );

        let right_side_x =
            end_x - 13 * render_state.text_renderer.character_width(FontKind::Normal) - 1;
        current_y = right_side_y;

        render_state.text_renderer.draw(
            &mut render_state.sprite_renderer,
            &render_state.ui_camera,
            "  Ships",
            right_side_x,
            current_y,
            Layer::TopMost,
            TextColor::DarkRed,
            TextAlignment::Left,
        );

        current_y += font_height;

        let right_messages = [
            "1 = Warbird",
            "2 = Javelin",
            "3 = Spider",
            "4 = Leviathan",
            "5 = Terrier",
            "6 = Weasel",
            "7 = Lancaster",
            "8 = Shark",
            "S = Spectator",
        ];

        for message in right_messages {
            render_state.text_renderer.draw(
                &mut render_state.sprite_renderer,
                &render_state.ui_camera,
                message,
                right_side_x,
                current_y,
                Layer::TopMost,
                TextColor::White,
                TextAlignment::Left,
            );

            current_y += font_height;
        }
    }

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
    ) -> Option<SceneKeyAction> {
        let _ = platform;
        let _ = input_state;
        let _ = input_mapping;

        if is_pressed {
            if let Some(action) = self.get_action(code) {
                let client = &mut *self.client.lock().unwrap();

                match action {
                    MenuAction::MenuToggle => {
                        if !client.statbox.cancel_select_box() {
                            self.active = !self.active;
                        }
                    }
                    MenuAction::Quit => {
                        event_loop.exit();
                    }
                    MenuAction::ArenaList => {
                        client
                            .chat_controller
                            .send_public(&mut client.connection, "?arena");
                    }
                    MenuAction::ShipRequest(ship_kind) => {
                        client.handle_ship_request(ship_kind);
                    }
                    MenuAction::NameTags => {
                        game_settings.render_name_mode = game_settings.render_name_mode.next();

                        client.chat_controller.add_system_message(format!(
                            "Name view mode: {}",
                            game_settings.render_name_mode.to_str()
                        ));
                    }
                    MenuAction::Statbox => {
                        client.statbox.next_view(&client.simulation.player_manager);
                    }
                    MenuAction::Keybind => {
                        let keybind_scene = KeybindScene::new(input_mapping.clone());

                        return Some(SceneKeyAction::AddScene(Arc::new(Mutex::new(
                            keybind_scene,
                        ))));
                    }
                    _ => {}
                }

                return Some(SceneKeyAction::Ignore);
            }
        }

        None
    }

    // Returns true if this scene handled the text and any scenes below this shouldn't receive the text.
    fn handle_text(&mut self, input_state: &mut InputState, c: &SmolStr) -> bool {
        let _ = input_state;
        let _ = c;

        false
    }
}
