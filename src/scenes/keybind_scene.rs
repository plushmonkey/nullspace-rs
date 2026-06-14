use std::collections::HashMap;

use smol_str::format_smolstr;
use winit::keyboard::KeyCode;

use crate::{
    game_settings::GameSettings,
    input::{InputAction, InputMapping, InputModifier, InputModifierSet, InputState},
    platform::Platform,
    render::{
        game_sprites::GameSprites,
        layer::Layer,
        render_state::RenderState,
        text_renderer::{FontKind, TextAlignment, TextColor},
    },
    scenes::{Scene, SceneKeyAction},
};

pub struct KeybindTab {
    pub title: String,
    pub actions: Vec<InputAction>,
}

struct RecordingState {
    index: usize,
    modifiers: InputModifierSet,
}

pub struct KeybindScene {
    mapping: HashMap<InputAction, (KeyCode, InputModifierSet)>,
    tabs: Vec<KeybindTab>,
    active_tab: usize,
    active_index: usize,

    recording: Option<RecordingState>,
}

impl KeybindScene {
    pub fn new(mapping: InputMapping) -> Self {
        let mut tabs = vec![];

        tabs.push(KeybindTab {
            title: "Movement".to_string(),
            actions: vec![
                InputAction::MoveLeft,
                InputAction::MoveRight,
                InputAction::MoveForward,
                InputAction::MoveBackward,
                InputAction::Afterburner,
            ],
        });

        tabs.push(KeybindTab {
            title: "Weapons".to_string(),
            actions: vec![
                InputAction::Bomb,
                InputAction::Bullet,
                InputAction::Mine,
                InputAction::Thor,
                InputAction::Burst,
            ],
        });

        tabs.push(KeybindTab {
            title: "Toggles".to_string(),
            actions: vec![
                InputAction::Multifire,
                InputAction::Antiwarp,
                InputAction::Stealth,
                InputAction::Cloak,
                InputAction::XRadar,
            ],
        });

        tabs.push(KeybindTab {
            title: "Actions".to_string(),
            actions: vec![
                InputAction::Repel,
                InputAction::Warp,
                InputAction::Portal,
                InputAction::Decoy,
                InputAction::Rocket,
                InputAction::Brick,
                InputAction::Attach,
            ],
        });

        tabs.push(KeybindTab {
            title: "Misc".to_string(),
            actions: vec![
                InputAction::StatboxCycle,
                InputAction::StatboxUp,
                InputAction::StatboxDown,
                InputAction::FullRadar,
                InputAction::ChatBox,
            ],
        });

        Self {
            mapping: mapping.create_reverse_mapping(),
            tabs,
            active_tab: 0,
            active_index: 0,
            recording: None,
        }
    }

    fn get_input_string(code: Option<KeyCode>, modifiers: InputModifierSet) -> String {
        let alt_code = if modifiers.is_set(InputModifier::Alt) {
            "Alt + "
        } else {
            ""
        };

        let control_code = if modifiers.is_set(InputModifier::Control) {
            "Control + "
        } else {
            ""
        };

        let shift_code = if modifiers.is_set(InputModifier::Shift) {
            "Shift + "
        } else {
            ""
        };

        if let Some(code) = code {
            match code {
                KeyCode::ShiftLeft | KeyCode::ShiftRight => {
                    format!("{}{}Shift", alt_code, control_code)
                }
                KeyCode::ControlLeft | KeyCode::ControlRight => {
                    if !shift_code.is_empty() {
                        format!("{}Control + Shift", alt_code)
                    } else {
                        format!("{}Control", alt_code)
                    }
                }
                KeyCode::AltLeft | KeyCode::AltRight => {
                    if !control_code.is_empty() && !shift_code.is_empty() {
                        format!("Alt + Control + Shift")
                    } else if !control_code.is_empty() {
                        format!("Alt + Control")
                    } else if !shift_code.is_empty() {
                        format!("Alt + Shift")
                    } else {
                        format!("Alt{}{}", control_code, shift_code)
                    }
                }
                _ => format!("{}{}{}{:?}", alt_code, control_code, shift_code, code),
            }
        } else {
            format!("{}{}{}", alt_code, control_code, shift_code)
        }
    }
}

impl Scene for KeybindScene {
    fn render(
        &mut self,
        game_settings: &GameSettings,
        render_state: &mut RenderState,
        sprites: &mut GameSprites,
    ) {
        let _ = game_settings;
        let _ = render_state;
        let _ = sprites;

        let font_width = render_state.text_renderer.character_width(FontKind::Normal);
        let font_height = render_state
            .text_renderer
            .character_height(FontKind::Normal);

        let window_width = 350;

        let x = render_state.width() as i32 / 2;
        let y = 100;

        render_state.text_renderer.draw_with_font(
            &mut render_state.sprite_renderer,
            &render_state.ui_camera,
            FontKind::Large,
            "KEYBINDS",
            x,
            y,
            Layer::TopMost,
            TextColor::White,
            TextAlignment::Center,
        );

        let start_y = y - font_height;

        let y = y + 40;

        let tab = &self.tabs[self.active_tab];

        let width = render_state.text_renderer.draw(
            &mut render_state.sprite_renderer,
            &render_state.ui_camera,
            &tab.title,
            x,
            y,
            Layer::TopMost,
            TextColor::Yellow,
            TextAlignment::Center,
        );

        if self.active_tab > 0 {
            render_state.text_renderer.draw(
                &mut render_state.sprite_renderer,
                &render_state.ui_camera,
                "<",
                x - width / 2 - font_width,
                y,
                Layer::TopMost,
                TextColor::White,
                TextAlignment::Center,
            );
        }

        if self.active_tab < self.tabs.len() - 1 {
            render_state.text_renderer.draw(
                &mut render_state.sprite_renderer,
                &render_state.ui_camera,
                ">",
                x + width / 2 + font_width,
                y,
                Layer::TopMost,
                TextColor::White,
                TextAlignment::Center,
            );
        }

        let mut y = y + font_height * 2;

        for i in 0..tab.actions.len() {
            let action = &tab.actions[i];

            let mut color = if i == self.active_index {
                TextColor::Red
            } else {
                TextColor::White
            };

            let is_recording = if let Some(recording) = &self.recording {
                recording.index == i
            } else {
                false
            };

            if is_recording {
                color = TextColor::Fuchsia;
            }

            render_state.text_renderer.draw(
                &mut render_state.sprite_renderer,
                &render_state.ui_camera,
                &format_smolstr!("{:?}", action),
                x - window_width / 2 + font_width,
                y,
                Layer::TopMost,
                color,
                TextAlignment::Left,
            );

            if is_recording {
                let recording = self.recording.as_ref().unwrap();

                render_state.text_renderer.draw(
                    &mut render_state.sprite_renderer,
                    &render_state.ui_camera,
                    &Self::get_input_string(None, recording.modifiers),
                    x + window_width / 2 - font_width + 2,
                    y,
                    Layer::TopMost,
                    TextColor::Fuchsia,
                    TextAlignment::Right,
                );
            } else {
                if let Some((code, modifiers)) = self.mapping.get(action) {
                    render_state.text_renderer.draw(
                        &mut render_state.sprite_renderer,
                        &render_state.ui_camera,
                        &Self::get_input_string(Some(*code), *modifiers),
                        x + window_width / 2 - font_width + 2,
                        y,
                        Layer::TopMost,
                        color,
                        TextAlignment::Right,
                    );
                }
            }

            y += font_height;
        }

        sprites.colors.draw_border(
            &mut render_state.sprite_renderer,
            &render_state.ui_camera,
            Layer::AfterChat,
            x - window_width / 2,
            start_y,
            x + window_width / 2,
            y + font_height,
            true,
        );
    }

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
        let _ = event_loop;
        let _ = platform;
        let _ = input_state;
        let _ = input_mapping;
        let _ = game_settings;

        if let Some(recording) = &mut self.recording {
            if !InputMapping::is_modifier_code(code) {
                if is_pressed {
                    let tab = &self.tabs[self.active_tab];
                    let action = tab.actions[self.active_index];

                    if code == KeyCode::Escape {
                        self.mapping.remove(&action);
                    } else {
                        self.mapping.insert(action, (code, recording.modifiers));
                    }

                    self.recording = None;
                }
            } else {
                if is_pressed {
                    match code {
                        KeyCode::ShiftLeft | KeyCode::ShiftRight => {
                            recording.modifiers.set(InputModifier::Shift)
                        }
                        KeyCode::ControlLeft | KeyCode::ControlRight => {
                            recording.modifiers.set(InputModifier::Control)
                        }
                        KeyCode::AltLeft | KeyCode::AltRight => {
                            recording.modifiers.set(InputModifier::Alt)
                        }
                        _ => {}
                    }
                } else {
                    let tab = &self.tabs[self.active_tab];
                    let action = tab.actions[self.active_index];

                    self.mapping.insert(action, (code, recording.modifiers));
                    self.recording = None;
                }
            }

            return Some(SceneKeyAction::Ignore);
        } else {
            if is_pressed {
                match code {
                    KeyCode::Escape | KeyCode::F10 => {
                        input_mapping.load_reverse_mapping(&self.mapping);
                        input_mapping.save(platform);

                        return Some(SceneKeyAction::PopScene);
                    }
                    KeyCode::ArrowLeft => {
                        if self.active_tab > 0 {
                            self.active_tab -= 1;
                            self.active_index = 0;
                        }
                    }
                    KeyCode::ArrowRight => {
                        if self.active_tab < self.tabs.len() - 1 {
                            self.active_tab = self.active_tab + 1;
                            self.active_index = 0;
                        }
                    }
                    KeyCode::ArrowUp => {
                        if self.active_index > 0 {
                            self.active_index -= 1;
                        }
                    }
                    KeyCode::ArrowDown => {
                        let tab = &self.tabs[self.active_tab];

                        if self.active_index < tab.actions.len() - 1 {
                            self.active_index += 1;
                        }
                    }
                    KeyCode::PageUp => {
                        self.active_index = 0;
                    }
                    KeyCode::PageDown => {
                        let tab = &self.tabs[self.active_tab];

                        self.active_index = tab.actions.len() - 1;
                    }
                    KeyCode::Enter => {
                        self.recording = Some(RecordingState {
                            index: self.active_index,
                            modifiers: input_state.get_modifier_down_set(),
                        })
                    }
                    _ => {}
                }
            }
        }

        return Some(SceneKeyAction::Ignore);
    }

    fn handle_text(
        &mut self,
        input_state: &mut crate::input::InputState,
        c: &winit::keyboard::SmolStr,
    ) -> bool {
        let _ = input_state;
        let _ = c;

        false
    }

    fn is_active(&self) -> bool {
        true
    }
}
