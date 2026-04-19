use crate::{
    net::connection::Connection,
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

pub struct ChatController {
    pub input: Vec<u8>,
}

impl ChatController {
    pub fn new() -> Self {
        Self { input: vec![] }
    }

    pub fn render(&self, render_state: &mut RenderState) {
        if self.input.is_empty() {
            return;
        }

        let color = match self.get_chat_send_kind() {
            ChatSendKind::Team => TextColor::Yellow,
            ChatSendKind::Private => TextColor::Green,
            ChatSendKind::Channel => TextColor::Red,
            _ => TextColor::White,
        };

        let height = render_state.config.height;

        render_state.text_renderer.draw_slice(
            &mut render_state.sprite_renderer,
            &render_state.ui_camera,
            &self.input,
            0,
            height.saturating_sub(12) as i32,
            Layer::Chat,
            color,
            TextAlignment::Left,
        );
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

                let chat = match self.get_chat_send_kind() {
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
                            crate::net::packet::c2s::ArenaRequest::Name(target.to_string())
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
}
