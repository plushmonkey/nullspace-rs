use crate::render::{
    layer::Layer,
    render_state::RenderState,
    text_renderer::{TextAlignment, TextColor},
};

pub struct Notification {
    pub message: String,
    pub color: TextColor,
    pub remaining_ticks: u32,
}

impl Notification {
    pub fn empty() -> Self {
        Self {
            message: String::new(),
            color: TextColor::Yellow,
            remaining_ticks: 0,
        }
    }
}

const MAX_NOTIFICATIONS: usize = 7;
const NOTIFICATION_DURATION: u32 = 500;

pub struct NotificationManager {
    pub notifications: [Notification; MAX_NOTIFICATIONS],
}

impl NotificationManager {
    pub fn new() -> Self {
        Self {
            notifications: [(); MAX_NOTIFICATIONS].map(|_| Notification::empty()),
        }
    }

    pub fn tick(&mut self) {
        for notification in &mut self.notifications {
            if notification.remaining_ticks > 0 {
                notification.remaining_ticks -= 1;
            }
        }
    }

    pub fn clear(&mut self) {
        for notification in &mut self.notifications {
            notification.remaining_ticks = 0;
        }
    }

    pub fn render(&self, render_state: &mut RenderState) {
        let mut y = (render_state.config.height as f32 * 0.6f32) as i32;
        let x = (render_state.config.width as f32 * 0.2f32) as i32;

        for notification in &self.notifications {
            if notification.remaining_ticks > 0 {
                render_state.text_renderer.draw(
                    &mut render_state.sprite_renderer,
                    &render_state.ui_camera,
                    &notification.message,
                    x,
                    y,
                    Layer::AfterChat,
                    notification.color,
                    TextAlignment::Left,
                );
            }

            y += render_state.text_renderer.character_height;
        }
    }

    pub fn push(&mut self, message: String, color: TextColor) {
        let notification = self.get_oldest_notification();

        notification.message = message;
        notification.color = color;
        notification.remaining_ticks = NOTIFICATION_DURATION;
    }

    pub fn push_str(&mut self, message: &str, color: TextColor) {
        self.push(message.to_string(), color);
    }

    fn get_oldest_notification(&mut self) -> &mut Notification {
        let mut best_index = 0;
        let mut lowest_remaining_ticks = 0xFFFFFFFF;

        for i in 0..MAX_NOTIFICATIONS {
            if self.notifications[i].remaining_ticks == 0 {
                return &mut self.notifications[i];
            }

            if self.notifications[i].remaining_ticks < lowest_remaining_ticks {
                lowest_remaining_ticks = self.notifications[i].remaining_ticks;
                best_index = i;
            }
        }

        &mut self.notifications[best_index]
    }
}
