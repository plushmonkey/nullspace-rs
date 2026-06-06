#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

use crate::render::text_renderer::FontKind;

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
#[derive(Copy, Clone, Debug)]
pub enum NotificationArea {
    Off,
    Center,
    Chat,
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum RenderNameMode {
    Off,
    Others,
    All,
}

impl RenderNameMode {
    pub fn next(&self) -> Self {
        match self {
            RenderNameMode::Off => RenderNameMode::Others,
            RenderNameMode::Others => RenderNameMode::All,
            RenderNameMode::All => RenderNameMode::Off,
        }
    }

    pub fn to_str(&self) -> &'static str {
        match self {
            RenderNameMode::Off => "Off",
            RenderNameMode::Others => "Others",
            RenderNameMode::All => "All",
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
#[derive(Copy, Clone, Debug)]
pub struct GameSettings {
    pub kill_notify_area: NotificationArea,
    // If this is true then all kills will go to the notifcation area, if false then only kills involving us.
    pub kill_notify_others: bool,

    pub enter_notify_area: NotificationArea,
    pub leave_notify_area: NotificationArea,

    pub transparent_statbox: bool,

    pub chat_font_kind: FontKind,

    pub name_length: u8,
    pub chat_lines: u8,

    pub radar_target_bounty: u16,
    pub radar_transparent: bool,
    pub map_transparent: bool,
    pub radar_grid: bool,

    pub render_stars: bool,
    pub render_exhaust: bool,
    pub render_bomb_trails: bool,
    pub render_gun_trails: bool,
    pub render_ball_trails: bool,

    pub render_name_mode: RenderNameMode,
    pub render_player_ping: bool,

    pub multifire_spawn: bool,

    pub statbox_size: u16,
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
impl GameSettings {
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(constructor))]
    pub fn new() -> GameSettings {
        GameSettings {
            kill_notify_area: NotificationArea::Center,
            kill_notify_others: false,
            enter_notify_area: NotificationArea::Center,
            leave_notify_area: NotificationArea::Center,
            transparent_statbox: false,
            chat_font_kind: FontKind::Normal,
            name_length: 10,
            chat_lines: 8,
            radar_target_bounty: 100,
            radar_transparent: false,
            map_transparent: false,
            radar_grid: false,
            render_stars: true,
            render_exhaust: true,
            render_bomb_trails: true,
            render_gun_trails: true,
            render_ball_trails: true,
            render_name_mode: RenderNameMode::All,
            render_player_ping: true,
            multifire_spawn: false,
            statbox_size: 10,
        }
    }
}
