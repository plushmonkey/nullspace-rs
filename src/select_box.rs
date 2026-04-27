use smol_str::format_smolstr;

use crate::{
    net::packet::s2c::ArenaDirectoryEntry,
    render::{
        colors::ColorRenderableKind,
        game_sprites::GameSprites,
        layer::Layer,
        render_state::RenderState,
        text_renderer::{TextAlignment, TextColor},
    },
};

pub struct SelectBox {
    pub title: String,
    pub contents: Vec<(String, i32, TextColor)>,
    pub max_length: i32,

    pub is_arena_directory: bool,

    pub top_index: usize,
    pub selected_index: usize,
}

impl SelectBox {
    const DISPLAY_COUNT: usize = 15;

    pub fn new(title: String, contents: Vec<(String, i32, TextColor)>) -> Self {
        let mut max_length = title.len();

        for (line, _, _) in &contents {
            max_length = max_length.max(line.len());
        }

        Self {
            title,
            contents,
            max_length: max_length as i32,
            top_index: 0,
            selected_index: 0,
            is_arena_directory: false,
        }
    }

    pub fn new_directory(arenas: &Vec<ArenaDirectoryEntry>) -> Self {
        let title = " Arena-name    Count".to_string();
        let mut contents = vec![];

        for entry in arenas {
            // Ticker (1) + Arena (16) + Count (4)
            const LINE_LENGTH: usize = 1 + 16 + 3;

            let color = if entry.current {
                TextColor::Yellow
            } else {
                TextColor::White
            };

            let (name, select_index): (&str, i32) =
                if let Ok(public_number) = entry.name.parse::<u16>() {
                    (
                        &format_smolstr!("(Public {public_number})"),
                        public_number as i32,
                    )
                } else {
                    (&entry.name, -1)
                };

            let count_str = format_smolstr!("{}", entry.count);
            let name_len = name.len().min(16);

            let mut line = String::with_capacity(LINE_LENGTH);

            line.push_str(&name[..name_len]);

            for _ in 0..(16 - name_len) {
                line.push(' ');
            }

            for _ in 0..(3_usize.saturating_sub(count_str.len())) {
                line.push(' ');
            }
            line.push_str(&count_str);

            if entry.current {
                contents.insert(0, (line, select_index, color))
            } else {
                contents.push((line, select_index, color));
            }
        }

        contents.sort_by(|left, right| left.0.cmp(&right.0));

        let mut result = Self::new(title, contents);

        result.is_arena_directory = true;

        result
    }

    pub fn move_selected(&mut self, direction: i32, shift: bool) {
        let move_amount = if shift {
            direction * Self::DISPLAY_COUNT as i32
        } else {
            direction
        };

        self.selected_index = self
            .selected_index
            .saturating_add_signed(move_amount as isize);

        if self.selected_index >= self.contents.len() {
            self.selected_index = self.contents.len() - 1;
        }

        if self.selected_index < self.top_index {
            self.top_index = self.selected_index;
        }

        if self.selected_index >= self.top_index + Self::DISPLAY_COUNT {
            self.top_index = self.selected_index.saturating_sub(Self::DISPLAY_COUNT - 1);
        }
    }

    pub fn select(&mut self) -> String {
        let selected = &self.contents[self.selected_index];

        if self.is_arena_directory {
            if selected.1 >= 0 {
                return format!("?go {}", selected.1).to_string();
            } else {
                if let Some(name) = selected.0.split(' ').next() {
                    return format!("?go {}", name).to_string();
                } else {
                    return format!("?go");
                }
            }
        } else {
            return format!("?select {} {}", selected.1, selected.0).to_string();
        }
    }

    pub fn render(&self, render_state: &mut RenderState, game_sprites: &GameSprites) {
        let x_center = render_state.config.width as i32 / 2;
        let inner_width = self.max_length * render_state.text_renderer.character_width;

        let start_x = x_center - (inner_width / 2);
        let start_y = 2;
        let end_x = x_center + (inner_width / 2) + 3;

        let mut current_y = 3;

        render_state.text_renderer.draw(
            &mut render_state.sprite_renderer,
            &render_state.ui_camera,
            &self.title,
            start_x + 2,
            current_y,
            Layer::TopMost,
            TextColor::Green,
            TextAlignment::Left,
        );

        current_y += render_state.text_renderer.character_height;

        game_sprites.colors.draw(
            &mut render_state.sprite_renderer,
            &render_state.ui_camera,
            Layer::TopMost,
            ColorRenderableKind::BorderInner,
            start_x,
            current_y,
            end_x - start_x,
            1,
        );

        current_y += 3;

        let bottom_index = (self.top_index + Self::DISPLAY_COUNT).min(self.contents.len());

        for i in self.top_index..bottom_index {
            let (line, _, color) = &self.contents[i];

            if i == self.selected_index {
                render_state.text_renderer.draw(
                    &mut render_state.sprite_renderer,
                    &render_state.ui_camera,
                    ">",
                    start_x + 2,
                    current_y,
                    Layer::TopMost,
                    *color,
                    TextAlignment::Left,
                );
            }

            render_state.text_renderer.draw(
                &mut render_state.sprite_renderer,
                &render_state.ui_camera,
                &line,
                start_x + 2 + render_state.text_renderer.character_width,
                current_y,
                Layer::TopMost,
                *color,
                TextAlignment::Left,
            );
            current_y += render_state.text_renderer.character_height;
        }

        game_sprites.colors.draw_border(
            &mut render_state.sprite_renderer,
            &render_state.ui_camera,
            Layer::AfterGauges,
            start_x,
            start_y,
            end_x,
            current_y,
            true,
        );
    }
}
