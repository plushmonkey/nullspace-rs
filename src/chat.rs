use smol_str::{SmolStr, StrExt, ToSmolStr};

use crate::{
    clock::GameTick,
    game_settings::GameSettings,
    net::{connection::Connection, packet::s2c::ChatKind},
    player::PlayerManager,
    radar::Radar,
    render::{
        game_sprites::GameSprites,
        layer::Layer,
        render_state::{ReferencePoint, RenderState},
        text_renderer::{TextAlignment, TextColor},
    },
    statbox::Statbox,
};

pub enum ChatCommand {
    ChangeFrequency(u32),
    Go(SmolStr),
}

pub enum ChatSendKind {
    Public,
    Private,
    Team,
    Frequency,
    Channel,
}

pub enum ChatRenderMode {
    Normal,
    Full,
    None,
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
    pub render_mode: ChatRenderMode,
    pub recent_private: Vec<String>,

    message_spans: Vec<(u16, u16)>,
}

impl ChatController {
    const MAX_MESSAGE_HISTORY: usize = 64;
    const RECENT_NAME_COUNT: usize = 5;

    pub fn new() -> Self {
        Self {
            input: vec![],
            messages: vec![],
            insert_index: 0,
            render_mode: ChatRenderMode::Normal,
            recent_private: vec![],
            message_spans: vec![],
        }
    }

    pub fn clear(&mut self) {
        self.messages.clear();
        self.insert_index = 0;
    }

    pub fn update_render_mode(&mut self, menu_open: bool, radar_full: bool) {
        if radar_full {
            self.render_mode = ChatRenderMode::None;
            return;
        }

        if menu_open {
            self.render_mode = ChatRenderMode::Full;
        } else {
            self.render_mode = ChatRenderMode::Normal;
        }
    }

    pub fn render(
        &mut self,
        render_state: &mut RenderState,
        sprites: &GameSprites,
        game_settings: &GameSettings,
    ) {
        let name_len = game_settings.name_length as usize;

        const LEFT_SPACING: i32 = 2;

        let font_width = render_state
            .text_renderer
            .character_width(game_settings.chat_font_kind);
        let font_height = render_state
            .text_renderer
            .character_height(game_settings.chat_font_kind);

        let height = render_state.height();

        let mut current_y = height.saturating_sub_signed(font_height + 2) as i32;

        let chat_region_width = Self::get_chat_region_width(render_state);

        if chat_region_width <= 0 {
            return;
        }

        if !self.input.is_empty() {
            let color = match self.get_chat_send_kind() {
                ChatSendKind::Team => TextColor::Yellow,
                ChatSendKind::Private => TextColor::Green,
                ChatSendKind::Channel => TextColor::Red,
                _ => TextColor::White,
            };

            if let Ok(input) = core::str::from_utf8(&self.input) {
                let max_chat_region_characters = ((chat_region_width as i32) / font_width) as usize;

                Self::wrap_chat(&mut self.message_spans, input, max_chat_region_characters);

                let mut render_cursor = true;

                for (start_index, end_index) in self.message_spans.iter().rev() {
                    let current = &input[*start_index as usize..*end_index as usize];

                    let render_width = render_state.text_renderer.draw_with_font(
                        &mut render_state.sprite_renderer,
                        &render_state.ui_camera,
                        game_settings.chat_font_kind,
                        current,
                        LEFT_SPACING,
                        current_y,
                        Layer::Chat,
                        color,
                        TextAlignment::Left,
                    );

                    if render_cursor {
                        let cursor_x = LEFT_SPACING + render_width;
                        let cursor_y = current_y;

                        render_cursor = false;

                        let tick = GameTick::now(0).value();
                        if (tick / 30) % 2 == 0 {
                            sprites.colors.draw_cursor(
                                &mut render_state.sprite_renderer,
                                &render_state.ui_camera,
                                Layer::Chat,
                                cursor_x,
                                cursor_y,
                                font_height,
                            );
                        }
                    }

                    current_y -= font_height;
                }
            }
        }

        let max_output_count = self.get_max_output_count(
            render_state.height() as i32,
            render_state
                .text_renderer
                .character_height(game_settings.chat_font_kind),
            game_settings,
        );

        if max_output_count == 0 {
            return;
        }

        if !self.messages.is_empty() {
            let mut current_index = Self::wrap_index(self.insert_index, -1, self.messages.len());
            let first_index = current_index;

            let mut output_count = 0;

            'render_loop: loop {
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

                        let trimmed_name_len = entry.sender.len().min(name_len);
                        let inset_pixels =
                            LEFT_SPACING + (name_len - trimmed_name_len) as i32 * font_width;

                        let name_width = trimmed_name_len * font_width as usize;
                        let message_inset = inset_pixels + name_width as i32 + 2 * font_width;

                        let max_chat_region_characters =
                            ((chat_region_width as i32 - message_inset) / font_width) as usize;

                        Self::wrap_chat(
                            &mut self.message_spans,
                            &entry.message,
                            max_chat_region_characters,
                        );

                        for (start_index, end_index) in self.message_spans.iter().rev() {
                            let current =
                                &entry.message[*start_index as usize..*end_index as usize].trim();

                            render_state.text_renderer.draw_with_font(
                                &mut render_state.sprite_renderer,
                                &render_state.ui_camera,
                                game_settings.chat_font_kind,
                                &entry.sender[..trimmed_name_len],
                                inset_pixels,
                                current_y,
                                Layer::Chat,
                                name_color,
                                TextAlignment::Left,
                            );

                            render_state.text_renderer.draw_with_font(
                                &mut render_state.sprite_renderer,
                                &render_state.ui_camera,
                                game_settings.chat_font_kind,
                                "> ",
                                inset_pixels + name_width as i32,
                                current_y,
                                Layer::Chat,
                                name_color,
                                TextAlignment::Left,
                            );

                            render_state.text_renderer.draw_with_font(
                                &mut render_state.sprite_renderer,
                                &render_state.ui_camera,
                                game_settings.chat_font_kind,
                                current,
                                message_inset,
                                current_y,
                                Layer::Chat,
                                message_color,
                                TextAlignment::Left,
                            );

                            output_count += 1;

                            if output_count >= max_output_count {
                                break 'render_loop;
                            }

                            current_y -= font_height;
                        }
                    }
                    _ => {
                        let max_chat_region_characters =
                            ((chat_region_width as i32) / font_width) as usize;

                        Self::wrap_chat(
                            &mut self.message_spans,
                            &entry.message,
                            max_chat_region_characters,
                        );

                        for (start_index, end_index) in self.message_spans.iter().rev() {
                            let current =
                                &entry.message[*start_index as usize..*end_index as usize].trim();

                            render_state.text_renderer.draw_with_font(
                                &mut render_state.sprite_renderer,
                                &render_state.ui_camera,
                                game_settings.chat_font_kind,
                                current,
                                LEFT_SPACING,
                                current_y,
                                Layer::Chat,
                                message_color,
                                TextAlignment::Left,
                            );

                            output_count += 1;

                            if output_count >= max_output_count {
                                break 'render_loop;
                            }

                            current_y -= font_height;
                        }
                    }
                }

                current_index = Self::wrap_index(current_index, -1, self.messages.len());

                if current_index == first_index {
                    break;
                }
            }
        }

        render_state.set_reference_point(ReferencePoint::ChatTopLeft, (0, current_y));
    }

    // Wraps chat into spans stored in self.message_spans so the memory can be reused.
    fn wrap_chat(message_spans: &mut Vec<(u16, u16)>, message: &str, max_size: usize) {
        message_spans.clear();

        if max_size == 0 || message.len() == 0 {
            message_spans.push((0, message.len() as u16));
            return;
        }

        let mut index = 0;

        while index < message.len() {
            let mut end_index = (index + max_size).min(message.len());

            if end_index < message.len() {
                // Search backwards to find a space separator

                while end_index > index {
                    if message.as_bytes()[end_index] == b' ' {
                        break;
                    }
                    end_index -= 1;
                }

                if end_index == index {
                    end_index = (index + max_size).min(message.len());
                }
            }

            message_spans.push((index as u16, end_index as u16));

            index = end_index;
        }
    }

    fn get_max_output_count(
        &self,
        surface_height: i32,
        font_height: i32,
        game_settings: &GameSettings,
    ) -> usize {
        match self.render_mode {
            ChatRenderMode::Normal => game_settings.chat_lines as usize,
            ChatRenderMode::Full => {
                let max_height = ((surface_height - font_height) * 3) / 4;

                (max_height / font_height) as usize
            }
            ChatRenderMode::None => 0,
        }
    }

    fn get_chat_region_width(render_state: &RenderState) -> i32 {
        let surface_width = render_state.width() as i32;
        let radar_dim = Radar::get_dim_from_surface_width(render_state.width()) as i32;
        let radar_border_size = 3 * 2;

        surface_width - radar_dim - radar_border_size - Radar::CORNER_INSET as i32
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
                    if code == b':' && !self.input.is_empty() {
                        let mut is_recent_request = false;
                        let mut current_recent = None;

                        if self.input[0] == b':' {
                            if self.input.len() > 1 {
                                // Search for the next ':'. If it is the last character of the input, then it is a request for recent.
                                if let Some(position) =
                                    self.input[1..].iter().position(|c| *c == b':')
                                {
                                    if position + 2 == self.input.len() {
                                        is_recent_request = true;
                                        current_recent = Some(&self.input[1..]);
                                    }
                                }
                            } else {
                                // New chat input would be '::', so it is a request for recent.
                                is_recent_request = true;
                            }
                        }

                        if is_recent_request {
                            if self.recent_private.is_empty() {
                                return false;
                            }

                            let new_name = if let Some(current) = current_recent {
                                if let Ok(current_name) = std::str::from_utf8(current) {
                                    self.get_next_recent_name(current_name).to_smolstr()
                                } else {
                                    self.get_next_recent_name("").to_smolstr()
                                }
                            } else {
                                self.get_next_recent_name("").to_smolstr()
                            };

                            self.input.clear();
                            self.input.push(b':');
                            for c in new_name.as_bytes() {
                                self.input.push(*c);
                            }
                            self.input.push(b':');

                            return false;
                        }
                    }

                    self.input.push(code);
                }
            }
        }

        false
    }

    fn get_next_recent_name(&self, current: &str) -> &str {
        if let Some(position) = self
            .recent_private
            .iter()
            .position(|name| name.to_lowercase_smolstr() == current.to_lowercase_smolstr())
        {
            let prev_index = ((position as isize - 1) + self.recent_private.len() as isize)
                % self.recent_private.len() as isize;

            return &self.recent_private[prev_index as usize];
        } else {
            return &self.recent_private[self.recent_private.len() - 1];
        }
    }

    pub fn send_public(&mut self, connection: &mut Connection, message: &str) {
        use crate::net::packet::Serialize;

        let chat = crate::net::packet::c2s::SendChatMessage::public(message);

        if let Err(e) = connection.send_packet(&chat.serialize()) {
            log::error!("{e}");
        }
    }

    pub fn send_input(
        &mut self,
        connection: &mut Connection,
        statbox: &Statbox,
        player_manager: &PlayerManager,
    ) -> Option<ChatCommand> {
        if let Some(command) = self.handle_input_commands() {
            self.input.clear();
            return Some(command);
        }

        match std::str::from_utf8(&self.input) {
            Ok(msg) => {
                let send_kind = self.get_chat_send_kind();

                let chat = match &send_kind {
                    ChatSendKind::Public => crate::net::packet::c2s::SendChatMessage::public(msg),
                    ChatSendKind::Team => {
                        let skip = if self.input[0] == b'\'' { 1 } else { 2 };

                        crate::net::packet::c2s::SendChatMessage::team(&msg[skip..])
                    }
                    ChatSendKind::Private => {
                        if self.input[0] == b':' {
                            if let Some(remote_end) =
                                self.input[1..].iter().position(|c| *c == b':')
                            {
                                let player_name = &msg[1..remote_end + 1];

                                if let Some(local_player) = player_manager.get_by_name(player_name)
                                {
                                    crate::net::packet::c2s::SendChatMessage::private(
                                        local_player.id,
                                        &msg[remote_end + 2..],
                                    )
                                } else {
                                    crate::net::packet::c2s::SendChatMessage::remote_private(msg)
                                }
                            } else {
                                crate::net::packet::c2s::SendChatMessage::public(msg)
                            }
                        } else {
                            let selected_player = statbox.get_selected_player_id();

                            crate::net::packet::c2s::SendChatMessage::private(
                                selected_player,
                                &msg[1..],
                            )
                        }
                    }
                    ChatSendKind::Frequency => {
                        let selected_player = statbox.get_selected_player_id();

                        crate::net::packet::c2s::SendChatMessage::frequency(
                            selected_player,
                            &msg[1..],
                        )
                    }
                    ChatSendKind::Channel => crate::net::packet::c2s::SendChatMessage::channel(msg),
                };

                if let Err(e) = connection.send_reliable(&chat) {
                    log::error!("{e}");
                }

                let sender = connection.player_name.clone();

                match &send_kind {
                    ChatSendKind::Public => {
                        self.handle_chat_message(ChatKind::Public, sender, chat.text.to_string())
                    }
                    ChatSendKind::Private => {
                        if chat.kind == ChatKind::RemotePrivate {
                            self.handle_chat_message(
                                ChatKind::RemotePrivate,
                                sender,
                                chat.text.to_string(),
                            )
                        } else {
                            self.handle_chat_message(
                                ChatKind::Private,
                                sender,
                                chat.text.to_string(),
                            )
                        }
                    }
                    ChatSendKind::Team => {
                        self.handle_chat_message(ChatKind::Team, sender, chat.text.to_string())
                    }
                    ChatSendKind::Frequency => {
                        self.handle_chat_message(ChatKind::Frequency, sender, chat.text.to_string())
                    }
                    ChatSendKind::Channel => self.handle_chat_message(
                        ChatKind::Channel,
                        "".to_string(),
                        chat.text.to_string(),
                    ),
                }
            }
            Err(e) => {
                log::error!("{e}");
            }
        }

        self.input.clear();

        None
    }

    fn handle_input_commands(&self) -> Option<ChatCommand> {
        if self.input.is_empty() {
            return None;
        }

        let Ok(input) = std::str::from_utf8(&self.input) else {
            return None;
        };

        let input = input.trim();

        if input.len() < 2 {
            return None;
        }

        let invoker = input.as_bytes()[0];

        if invoker == b'=' {
            if let Ok(freq) = input[1..].parse::<u16>() {
                return Some(ChatCommand::ChangeFrequency(freq as u32));
            }

            return Some(ChatCommand::ChangeFrequency(0xFFFFFFFF));
        }

        if invoker == b'?' {
            let command = &input[1..];

            if command.starts_with("go") {
                let target = &command[2..].trim();

                return Some(ChatCommand::Go(target.to_smolstr()));
            }
        }

        None
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
                if self.input.len() >= 2 && self.input[2..].iter().find(|c| **c == b':').is_some() {
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
        match &kind {
            ChatKind::RemotePrivate | ChatKind::Private => {
                let sender_lower = sender.to_lowercase_smolstr();

                if let Some(existing) = self
                    .recent_private
                    .iter()
                    .position(|name| name.to_lowercase_smolstr() == sender_lower)
                {
                    self.recent_private.remove(existing);
                }

                if self.recent_private.len() >= Self::RECENT_NAME_COUNT {
                    self.recent_private.remove(0);
                }

                self.recent_private.push(sender.clone());
            }
            _ => {}
        }

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

    pub fn add_system_message(&mut self, message: String) {
        self.handle_chat_message(ChatKind::Arena, "".to_string(), message);
    }
}
