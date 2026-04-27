use winit::keyboard::KeyCode;

use crate::{
    render::{
        colors::ColorRenderableKind,
        game_sprites::GameSprites,
        layer::Layer,
        render_state::RenderState,
        text_renderer::{TextAlignment, TextColor},
    },
    ship::ShipKind,
};

pub enum MenuAction {
    Quit,
    Help,
    StatBox,
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
}

pub struct Menu {
    pub open: bool,
    // This is set to true on the update tick where the menu handled some key.
    // This is to prevent the key from being handled in handle_text.
    pub handled: bool,
}

impl Menu {
    pub fn new() -> Self {
        Self {
            open: false,
            handled: false,
        }
    }

    pub fn toggle(&mut self) {
        self.open = !self.open;
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn handle_key(&mut self, code: KeyCode) -> Option<MenuAction> {
        if !self.open {
            return None;
        }

        let result = match code {
            KeyCode::KeyQ => Some(MenuAction::Quit),
            KeyCode::F1 => Some(MenuAction::Help),
            KeyCode::F2 => Some(MenuAction::StatBox),
            KeyCode::F3 => Some(MenuAction::NameTags),
            KeyCode::F4 => Some(MenuAction::Radar),
            KeyCode::F5 => Some(MenuAction::Messages),
            KeyCode::F6 => Some(MenuAction::HelpTicker),
            KeyCode::F8 => Some(MenuAction::EngineSounds),
            KeyCode::KeyA => Some(MenuAction::ArenaList),
            KeyCode::KeyB => Some(MenuAction::SetBanner),
            KeyCode::KeyI => Some(MenuAction::IgnoreMacros),
            KeyCode::Digit1 => Some(MenuAction::ShipRequest(ShipKind::Warbird)),
            KeyCode::Digit2 => Some(MenuAction::ShipRequest(ShipKind::Javelin)),
            KeyCode::Digit3 => Some(MenuAction::ShipRequest(ShipKind::Spider)),
            KeyCode::Digit4 => Some(MenuAction::ShipRequest(ShipKind::Leviathan)),
            KeyCode::Digit5 => Some(MenuAction::ShipRequest(ShipKind::Terrier)),
            KeyCode::Digit6 => Some(MenuAction::ShipRequest(ShipKind::Weasel)),
            KeyCode::Digit7 => Some(MenuAction::ShipRequest(ShipKind::Lancaster)),
            KeyCode::Digit8 => Some(MenuAction::ShipRequest(ShipKind::Shark)),
            KeyCode::KeyS => Some(MenuAction::ShipRequest(ShipKind::Spectator)),
            _ => None,
        };

        if result.is_some() {
            self.open = false;
            self.handled = true;
        }

        result
    }

    pub fn render(&self, render_state: &mut RenderState, sprites: &GameSprites) {
        let x_center = render_state.config.width as i32 / 2;
        let menu_width = 284;
        let menu_height = 170;

        let start_x = x_center - menu_width / 2;
        let start_y = 2;
        let end_x = x_center + menu_width / 2;
        let end_y = start_y + menu_height;

        let font_height = render_state.text_renderer.character_height;

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
            " I = Ignore macros",
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

        current_y += 12;

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

        let right_side_x = end_x - 13 * render_state.text_renderer.character_width;
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
}
