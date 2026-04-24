use crate::{
    net::{connection::Connection, packet::s2c::ChatKind},
    render::{
        layer::Layer,
        render_state::RenderState,
        text_renderer::{TextAlignment, TextColor},
    },
};

pub enum ChatSendKind {
    Public,
    Private,
    Team,
    Frequency,
    Channel,
}

pub struct ChatEntry {
    pub kind: ChatKind,
    pub sender: String,
    pub message: String,
}

pub struct ChatController {
    pub input: Vec<u8>,
    pub messages: Vec<ChatEntry>,
    pub insert_index: usize,
}

impl ChatController {
    const MAX_MESSAGE_HISTORY: usize = 10;

    pub fn new() -> Self {
        Self {
            input: vec![],
            messages: vec![],
            insert_index: 0,
        }
    }

    pub fn clear(&mut self) {
        self.messages.clear();
        self.insert_index = 0;
    }

    pub fn render(&self, render_state: &mut RenderState) {
        const NAMELEN: usize = 10;

        let font_width = render_state.text_renderer.character_width;
        let font_height = render_state.text_renderer.character_height;

        let height = render_state.config.height;

        let mut current_y = height.saturating_sub_signed(font_height + 2) as i32;

        if !self.input.is_empty() {
            let color = match self.get_chat_send_kind() {
                ChatSendKind::Team => TextColor::Yellow,
                ChatSendKind::Private => TextColor::Green,
                ChatSendKind::Channel => TextColor::Red,
                _ => TextColor::White,
            };

            render_state.text_renderer.draw_slice(
                &mut render_state.sprite_renderer,
                &render_state.ui_camera,
                &self.input,
                0,
                current_y,
                Layer::Chat,
                color,
                TextAlignment::Left,
            );

            current_y -= font_height;
        }

        // TODO: Wrapping chat lines
        if !self.messages.is_empty() {
            let mut current_index = Self::wrap_index(self.insert_index, -1, self.messages.len());
            let first_index = current_index;

            loop {
                let entry = &self.messages[current_index];
                let message_color = Self::get_chat_message_color(entry.kind);

                match entry.kind {
                    ChatKind::Public
                    | ChatKind::PublicMacro
                    | ChatKind::Team
                    | ChatKind::Frequency
                    | ChatKind::Private => {
                        let name_color = match entry.kind {
                            ChatKind::Team => TextColor::Yellow,
                            ChatKind::Frequency => TextColor::Green,
                            ChatKind::Private => TextColor::Green,
                            _ => TextColor::Blue,
                        };

                        let trimmed_name_len = entry.sender.len().min(NAMELEN);
                        let inset_pixels = (NAMELEN - trimmed_name_len) as i32 * font_width;

                        render_state.text_renderer.draw(
                            &mut render_state.sprite_renderer,
                            &render_state.ui_camera,
                            &entry.sender[..trimmed_name_len],
                            inset_pixels,
                            current_y,
                            Layer::Chat,
                            name_color,
                            TextAlignment::Left,
                        );

                        let name_width = trimmed_name_len * font_width as usize;

                        render_state.text_renderer.draw(
                            &mut render_state.sprite_renderer,
                            &render_state.ui_camera,
                            "> ",
                            inset_pixels + name_width as i32,
                            current_y,
                            Layer::Chat,
                            name_color,
                            TextAlignment::Left,
                        );

                        let message_inset = inset_pixels + name_width as i32 + 2 * font_width;

                        render_state.text_renderer.draw(
                            &mut render_state.sprite_renderer,
                            &render_state.ui_camera,
                            &entry.message,
                            message_inset,
                            current_y,
                            Layer::Chat,
                            message_color,
                            TextAlignment::Left,
                        );
                    }
                    _ => {
                        render_state.text_renderer.draw(
                            &mut render_state.sprite_renderer,
                            &render_state.ui_camera,
                            &entry.message,
                            0,
                            current_y,
                            Layer::Chat,
                            message_color,
                            TextAlignment::Left,
                        );
                    }
                }

                current_y -= font_height;
                current_index = Self::wrap_index(current_index, -1, self.messages.len());

                if current_index == first_index {
                    break;
                }
            }
        }
    }

    fn get_chat_message_color(kind: ChatKind) -> TextColor {
        match kind {
            ChatKind::Arena => TextColor::Green,
            ChatKind::Team => TextColor::Yellow,
            ChatKind::Private => TextColor::Green,
            ChatKind::Warning => TextColor::DarkRed,
            ChatKind::RemotePrivate => TextColor::Green,
            ChatKind::Error => TextColor::DarkRed,
            ChatKind::Channel => TextColor::Red,
            ChatKind::Fuchsia => TextColor::Fuchsia,
            _ => TextColor::Blue,
        }
    }

    fn wrap_index(index: usize, delta: isize, max: usize) -> usize {
        index.wrapping_add_signed(delta).wrapping_add(max) % max
    }

    // Returns true if input should be sent.
    pub fn handle_key(&mut self, code: u8, control: bool) -> bool {
        const MAX_INPUT_LENGTH: usize = 250;

        match code {
            0x08 => {
                // Backspace
                if control {
                    self.input.clear();
                } else {
                    self.input.pop();
                }
            }
            0x0D => {
                // Enter
                if !self.input.is_empty() {
                    return true;
                }
            }
            _ => {
                if code >= 0x20 && self.input.len() < MAX_INPUT_LENGTH {
                    self.input.push(code);
                }
            }
        }

        false
    }

    pub fn send_input(&mut self, connection: &mut Connection) {
        if self.handle_input_commands(connection) {
            self.input.clear();
            return;
        }

        match std::str::from_utf8(&self.input) {
            Ok(msg) => {
                use crate::net::packet::Serialize;

                let send_kind = self.get_chat_send_kind();

                let chat = match &send_kind {
                    ChatSendKind::Public => crate::net::packet::c2s::SendChatMessage::public(msg),
                    ChatSendKind::Team => {
                        let skip = if self.input[0] == b'\'' { 1 } else { 2 };

                        crate::net::packet::c2s::SendChatMessage::frequency(
                            connection.player_id,
                            &msg[skip..],
                        )
                        //crate::net::packet::c2s::SendChatMessage::team(&msg[skip..])
                    }
                    ChatSendKind::Private => {
                        if self.input[0] == b':' {
                            crate::net::packet::c2s::SendChatMessage::remote_private(msg)
                        } else {
                            // TODO: Implement once statbox is implemented.
                            crate::net::packet::c2s::SendChatMessage::public(msg)
                        }
                    }
                    ChatSendKind::Frequency => {
                        // TODO: Implement once statbox is implemented.
                        crate::net::packet::c2s::SendChatMessage::frequency(
                            connection.player_id,
                            &msg[1..],
                        )
                    }
                    ChatSendKind::Channel => crate::net::packet::c2s::SendChatMessage::channel(msg),
                };

                if let Err(e) = connection.send_packet(&chat.serialize()) {
                    log::error!("{e}");
                }

                let sender = connection.player_name.clone();

                match &send_kind {
                    ChatSendKind::Public => {
                        self.handle_chat_message(ChatKind::Public, sender, msg.to_string())
                    }
                    ChatSendKind::Private => {
                        self.handle_chat_message(ChatKind::Private, sender, msg.to_string())
                    }
                    ChatSendKind::Team => {
                        let skip = if self.input[0] == b'\'' { 1 } else { 2 };
                        self.handle_chat_message(ChatKind::Team, sender, msg[skip..].to_string())
                    }
                    ChatSendKind::Frequency => {
                        self.handle_chat_message(ChatKind::Frequency, sender, msg[1..].to_string())
                    }
                    ChatSendKind::Channel => {
                        self.handle_chat_message(ChatKind::Channel, "".to_string(), msg.to_string())
                    }
                }
            }
            Err(e) => {
                log::error!("{e}");
            }
        }

        self.input.clear();
    }

    fn handle_input_commands(&self, connection: &mut Connection) -> bool {
        use crate::net::packet::Serialize;

        if self.input.is_empty() {
            return false;
        }

        if self.input[0] == b'=' {
            // TODO: This should be handled elsewhere so we can check energy.
            // TODO: Only one command can be handled per input, so we could just return out of the handle key function.

            let Ok(msg) = std::str::from_utf8(&self.input) else {
                return true;
            };

            if let Ok(freq) = msg.parse::<u16>() {
                let request = crate::net::packet::c2s::FrequencyChangeMessage { frequency: freq };

                if let Err(e) = connection.send_packet(&request.serialize()) {
                    log::error!("{e}");
                }
            }

            return true;
        }

        if self.input[0] == b'?' {
            let Ok(command) = std::str::from_utf8(&self.input[1..]) else {
                return false;
            };

            if command.starts_with("go") {
                let target = &command[2..].trim();

                if target.is_empty() {
                    let request = crate::net::packet::c2s::ArenaJoinMessage::new(
                        crate::ship::ShipKind::Spectator,
                        1920,
                        1080,
                        crate::net::packet::c2s::ArenaRequest::AnyPublic,
                    );

                    if let Err(e) = connection.send_packet(&request.serialize()) {
                        log::error!("{e}");
                    }
                } else {
                    let request = if let Ok(number) = target.parse::<u16>() {
                        crate::net::packet::c2s::ArenaJoinMessage::new(
                            crate::ship::ShipKind::Spectator,
                            1920,
                            1080,
                            crate::net::packet::c2s::ArenaRequest::SpecificPublic(number),
                        )
                    } else {
                        crate::net::packet::c2s::ArenaJoinMessage::new(
                            crate::ship::ShipKind::Spectator,
                            1920,
                            1080,
                            crate::net::packet::c2s::ArenaRequest::Name(target.to_string()),
                        )
                    };

                    if let Err(e) = connection.send_packet(&request.serialize()) {
                        log::error!("{e}");
                    }
                }

                return true;
            }
        }

        false
    }

    pub fn get_chat_send_kind(&self) -> ChatSendKind {
        if self.input.is_empty() {
            return ChatSendKind::Public;
        }

        match self.input[0] {
            b'\'' => ChatSendKind::Team,
            b'/' => {
                if self.input.len() > 1 && self.input[1] == b'/' {
                    ChatSendKind::Team
                } else {
                    ChatSendKind::Private
                }
            }
            b':' => {
                if self.input.iter().find(|c| **c == b':').is_some() {
                    ChatSendKind::Private
                } else {
                    ChatSendKind::Public
                }
            }
            b';' => ChatSendKind::Channel,
            b'"' => ChatSendKind::Frequency,
            _ => ChatSendKind::Public,
        }
    }

    pub fn handle_chat_message(&mut self, kind: ChatKind, sender: String, message: String) {
        let entry = ChatEntry {
            kind: kind,
            sender,
            message: message,
        };

        if self.messages.len() < Self::MAX_MESSAGE_HISTORY {
            self.messages.push(entry);
            self.insert_index = self.messages.len() % Self::MAX_MESSAGE_HISTORY;
        } else {
            self.messages[self.insert_index] = entry;
            self.insert_index = (self.insert_index + 1) % Self::MAX_MESSAGE_HISTORY;
        }
    }
}
