use smol_str::{StrExt, format_smolstr};

use crate::{
    player::{Player, PlayerId, PlayerManager},
    render::{
        game_sprites::GameSprites,
        layer::Layer,
        render_state::RenderState,
        text_renderer::{TextAlignment, TextColor},
    },
};

struct SlidingView {
    pub top: usize,
    pub size: usize,
}

impl SlidingView {
    pub fn new(size: usize) -> Self {
        Self { top: 0, size }
    }
}

#[derive(Copy, Clone, PartialEq)]
pub enum StatboxView {
    Names,
    Points,
    PointSort,
    TeamSort,
    Full,
    Frequency,
    None,
}

const MAX_NAME_VIEW_LENGTH: usize = 12;
const MAX_SQUAD_VIEW_LENGTH: usize = 10;

const BORDER_LEFT_WIDTH: i32 = 3;
const TICKER_WIDTH: i32 = 10;
const BANNER_WIDTH: i32 = 12;
const SPACING_WIDTH: i32 = 8;

// TODO: This allocates a lot more than it needs to. It could end up being really slow with many players.
// Could use smolstr to allocate on the stack when rendering instead of building strings for everything.
pub struct Statbox {
    view: StatboxView,

    sliding_view: SlidingView,
    _selected_index: usize,

    sorted_players: Vec<PlayerId>,
}

impl Statbox {
    pub fn new() -> Self {
        Self {
            view: StatboxView::None,
            sliding_view: SlidingView::new(15),
            _selected_index: 0,
            sorted_players: vec![],
        }
    }

    fn render_name_row(
        &mut self,
        player_manager: &PlayerManager,
        render_state: &mut RenderState,
        _game_sprites: &GameSprites,
        me: &Player,
        i: usize,
        current_x: i32,
        current_y: i32,
    ) -> i32 {
        let Some(player) = player_manager.get_by_id(self.sorted_players[i]) else {
            return 0;
        };

        let color = if me.frequency == player.frequency {
            TextColor::Yellow
        } else {
            TextColor::White
        };

        let name_len = player.name.len().min(MAX_NAME_VIEW_LENGTH);

        render_state.text_renderer.draw(
            &mut render_state.sprite_renderer,
            &render_state.ui_camera,
            &player.name[..name_len],
            current_x,
            current_y,
            Layer::AfterGauges,
            color,
            TextAlignment::Left,
        );

        12 * 8 + 10 + 3
    }

    fn render_points_row(
        &mut self,
        player_manager: &PlayerManager,
        render_state: &mut RenderState,
        _game_sprites: &GameSprites,
        me: &Player,
        i: usize,
        current_x: i32,
        current_y: i32,
        points_width_pixels: i32,
    ) -> i32 {
        let Some(player) = player_manager.get_by_id(self.sorted_players[i]) else {
            return 0;
        };

        let color = if me.frequency == player.frequency {
            TextColor::Yellow
        } else {
            TextColor::White
        };

        let name_len = player.name.len().min(MAX_NAME_VIEW_LENGTH);

        render_state.text_renderer.draw(
            &mut render_state.sprite_renderer,
            &render_state.ui_camera,
            &player.name[..name_len],
            current_x,
            current_y,
            Layer::AfterGauges,
            color,
            TextAlignment::Left,
        );

        let name_width = render_state.text_renderer.character_width * MAX_NAME_VIEW_LENGTH as i32;

        let points_x = BORDER_LEFT_WIDTH + TICKER_WIDTH + name_width + BANNER_WIDTH + SPACING_WIDTH;

        render_state.text_renderer.draw(
            &mut render_state.sprite_renderer,
            &render_state.ui_camera,
            &format_smolstr!("{}", player.get_points()),
            points_x + points_width_pixels,
            current_y,
            Layer::AfterGauges,
            color,
            TextAlignment::Right,
        );

        points_x + points_width_pixels
    }

    fn render_teamsort_row(
        &mut self,
        player_manager: &PlayerManager,
        render_state: &mut RenderState,
        _game_sprites: &GameSprites,
        me: &Player,
        i: usize,
        current_x: i32,
        current_y: i32,
        points_width_pixels: i32,
    ) -> i32 {
        let Some(player) = player_manager.get_by_id(self.sorted_players[i]) else {
            return 0;
        };

        let color = if me.frequency == player.frequency {
            TextColor::Yellow
        } else {
            TextColor::White
        };

        let name_len = player.name.len().min(MAX_NAME_VIEW_LENGTH);

        render_state.text_renderer.draw(
            &mut render_state.sprite_renderer,
            &render_state.ui_camera,
            &player.name[..name_len],
            current_x,
            current_y,
            Layer::AfterGauges,
            color,
            TextAlignment::Left,
        );

        let name_width = render_state.text_renderer.character_width * MAX_NAME_VIEW_LENGTH as i32;

        let points_x = BORDER_LEFT_WIDTH + TICKER_WIDTH + name_width + BANNER_WIDTH + SPACING_WIDTH;

        render_state.text_renderer.draw(
            &mut render_state.sprite_renderer,
            &render_state.ui_camera,
            &format_smolstr!("{}", player.get_points()),
            points_x + points_width_pixels,
            current_y,
            Layer::AfterGauges,
            color,
            TextAlignment::Right,
        );

        points_x + points_width_pixels
    }

    fn render_full_row(
        &mut self,
        player_manager: &PlayerManager,
        render_state: &mut RenderState,
        _game_sprites: &GameSprites,
        me: &Player,
        i: usize,
        current_x: i32,
        current_y: i32,
        wins_width_pixels: i32,
        losses_width_pixels: i32,
        rating_width_pixels: i32,
        average_width_pixels: i32,
    ) -> i32 {
        let font_width = render_state.text_renderer.character_width;

        let Some(player) = player_manager.get_by_id(self.sorted_players[i]) else {
            return 0;
        };

        let color = if me.frequency == player.frequency {
            TextColor::Yellow
        } else {
            TextColor::White
        };

        render_state.text_renderer.draw(
            &mut render_state.sprite_renderer,
            &render_state.ui_camera,
            &player.name,
            current_x,
            current_y,
            Layer::AfterGauges,
            color,
            TextAlignment::Left,
        );

        let squad_x = MAX_NAME_VIEW_LENGTH as i32 * font_width
            + BORDER_LEFT_WIDTH
            + TICKER_WIDTH
            + BANNER_WIDTH
            + SPACING_WIDTH;

        let squad_len = player.squad.len().min(MAX_SQUAD_VIEW_LENGTH);

        render_state.text_renderer.draw(
            &mut render_state.sprite_renderer,
            &render_state.ui_camera,
            &player.squad[..squad_len],
            squad_x,
            current_y,
            Layer::AfterGauges,
            color,
            TextAlignment::Left,
        );

        let wins_x = squad_x + (MAX_SQUAD_VIEW_LENGTH as i32 + 1) * font_width;
        render_state.text_renderer.draw(
            &mut render_state.sprite_renderer,
            &render_state.ui_camera,
            &format_smolstr!("{}", player.wins),
            wins_x + wins_width_pixels,
            current_y,
            Layer::AfterGauges,
            color,
            TextAlignment::Right,
        );

        let losses_x = wins_x + wins_width_pixels + font_width;
        render_state.text_renderer.draw(
            &mut render_state.sprite_renderer,
            &render_state.ui_camera,
            &format_smolstr!("{}", player.losses),
            losses_x + losses_width_pixels,
            current_y,
            Layer::AfterGauges,
            color,
            TextAlignment::Right,
        );

        let rating_x = losses_x + losses_width_pixels + font_width;
        render_state.text_renderer.draw(
            &mut render_state.sprite_renderer,
            &render_state.ui_camera,
            &format_smolstr!("{}", player.get_rating()),
            rating_x + rating_width_pixels,
            current_y,
            Layer::AfterGauges,
            color,
            TextAlignment::Right,
        );

        let average_x = rating_x + rating_width_pixels + font_width + average_width_pixels;
        render_state.text_renderer.draw(
            &mut render_state.sprite_renderer,
            &render_state.ui_camera,
            &format_smolstr!("{:.1}", player.get_average()),
            average_x,
            current_y,
            Layer::AfterGauges,
            color,
            TextAlignment::Right,
        );

        average_x
    }

    fn render_window(
        &mut self,
        player_manager: &PlayerManager,
        render_state: &mut RenderState,
        game_sprites: &GameSprites,
    ) {
        let Some(me) = player_manager.get_by_id(player_manager.self_id) else {
            return;
        };

        let mut current_y = 4;
        let heading_y = current_y;

        match &self.view {
            StatboxView::Frequency => {
                //
            }
            _ => {
                render_state.text_renderer.draw(
                    &mut render_state.sprite_renderer,
                    &render_state.ui_camera,
                    &format_smolstr!("{}", player_manager.players.len()),
                    10 + 5 * render_state.text_renderer.character_width,
                    heading_y,
                    Layer::AfterGauges,
                    TextColor::Green,
                    TextAlignment::Right,
                );
                current_y += 12 + 2;
            }
        }

        let heading_border_y = current_y - 2;

        let mut window_width = 0;

        let mut bottom = self.sliding_view.top + self.sliding_view.size;
        if bottom > self.sorted_players.len() {
            bottom = self.sorted_players.len();
        }

        let top = bottom.saturating_sub(self.sliding_view.size);

        match &self.view {
            StatboxView::Names => {
                for i in top..bottom {
                    let width = self.render_name_row(
                        player_manager,
                        render_state,
                        game_sprites,
                        me,
                        i,
                        BORDER_LEFT_WIDTH + TICKER_WIDTH,
                        current_y,
                    );

                    if width > window_width {
                        window_width = width;
                    }

                    current_y += 12;
                }
            }
            StatboxView::Points | StatboxView::PointSort => {
                let max_points_length = self.calculate_max_length(player_manager, |p| {
                    format_smolstr!("{}", p.get_points()).len()
                });

                let points_width_pixels =
                    max_points_length as i32 * render_state.text_renderer.character_width + 4;

                let heading_x = (MAX_NAME_VIEW_LENGTH as i32
                    * render_state.text_renderer.character_width)
                    + BORDER_LEFT_WIDTH
                    + TICKER_WIDTH
                    + BANNER_WIDTH
                    + SPACING_WIDTH
                    + points_width_pixels;
                let heading_text = if self.view == StatboxView::Points {
                    "Points"
                } else {
                    "Point Sort"
                };

                render_state.text_renderer.draw(
                    &mut render_state.sprite_renderer,
                    &render_state.ui_camera,
                    heading_text,
                    heading_x,
                    heading_y,
                    Layer::AfterGauges,
                    TextColor::Green,
                    TextAlignment::Right,
                );

                for i in top..bottom {
                    let width = self.render_points_row(
                        player_manager,
                        render_state,
                        game_sprites,
                        me,
                        i,
                        BORDER_LEFT_WIDTH + TICKER_WIDTH,
                        current_y,
                        points_width_pixels,
                    );

                    if width > window_width {
                        window_width = width;
                    }

                    current_y += 12;
                }
            }
            StatboxView::TeamSort => {
                let max_points_length = self.calculate_max_length(player_manager, |p| {
                    format_smolstr!("{}", p.get_points()).len()
                });

                let points_width_pixels =
                    max_points_length as i32 * render_state.text_renderer.character_width + 4;

                let heading_x = (MAX_NAME_VIEW_LENGTH as i32
                    * render_state.text_renderer.character_width)
                    + BORDER_LEFT_WIDTH
                    + TICKER_WIDTH
                    + BANNER_WIDTH
                    + SPACING_WIDTH
                    + points_width_pixels;

                render_state.text_renderer.draw(
                    &mut render_state.sprite_renderer,
                    &render_state.ui_camera,
                    "Team Sort",
                    heading_x,
                    heading_y,
                    Layer::AfterGauges,
                    TextColor::Green,
                    TextAlignment::Right,
                );

                let mut prev_freq: u32 = 0xFFFFFFFF;

                if top > 0 {
                    if let Some(player) = player_manager.get_by_id(self.sorted_players[top - 1]) {
                        prev_freq = player.frequency as u32;
                    }
                }

                for i in top..bottom {
                    let Some(player) = player_manager.get_by_id(self.sorted_players[i]) else {
                        continue;
                    };

                    let freq = player.frequency;
                    if freq as u32 != prev_freq {
                        prev_freq = player.frequency as u32;

                        // TODO: Setting visibility
                        let freq_string = if freq < 100 {
                            format_smolstr!("{:04}", freq)
                        } else {
                            format_smolstr!("----")
                        };

                        let start_spacer_width = 2;

                        let width = render_state.text_renderer.draw(
                            &mut render_state.sprite_renderer,
                            &render_state.ui_camera,
                            &freq_string,
                            BORDER_LEFT_WIDTH + start_spacer_width,
                            current_y,
                            Layer::AfterGauges,
                            TextColor::DarkRed,
                            TextAlignment::Left,
                        );

                        let dash_width = render_state.text_renderer.draw(
                            &mut render_state.sprite_renderer,
                            &render_state.ui_camera,
                            "-------------",
                            BORDER_LEFT_WIDTH + start_spacer_width + width,
                            current_y,
                            Layer::AfterGauges,
                            TextColor::DarkRed,
                            TextAlignment::Left,
                        );

                        let spacer_width = 3 * render_state.text_renderer.character_width;

                        render_state.text_renderer.draw(
                            &mut render_state.sprite_renderer,
                            &render_state.ui_camera,
                            &format_smolstr!(
                                "{}",
                                player_manager.get_frequency_count(player.frequency)
                            ),
                            BORDER_LEFT_WIDTH
                                + start_spacer_width
                                + width
                                + dash_width
                                + spacer_width,
                            current_y,
                            Layer::AfterGauges,
                            TextColor::DarkRed,
                            TextAlignment::Right,
                        );

                        current_y += 12;
                    }

                    let width = self.render_teamsort_row(
                        player_manager,
                        render_state,
                        game_sprites,
                        me,
                        i,
                        BORDER_LEFT_WIDTH + TICKER_WIDTH,
                        current_y,
                        points_width_pixels,
                    );

                    if width > window_width {
                        window_width = width;
                    }

                    current_y += 12;
                }
            }
            StatboxView::Full => {
                let font_width = render_state.text_renderer.character_width;

                let max_wins_length = self
                    .calculate_max_length(player_manager, |p| format_smolstr!("{}", p.wins).len());
                let wins_width_pixels = max_wins_length as i32 * font_width;

                let max_losses_length = self.calculate_max_length(player_manager, |p| {
                    format_smolstr!("{}", p.losses).len()
                });
                let losses_width_pixels = max_losses_length as i32 * font_width;

                let max_rating_length = self.calculate_max_length(player_manager, |p| {
                    format_smolstr!("{}", p.get_rating()).len()
                });
                let rating_width_pixels = max_rating_length as i32 * font_width;

                let max_average_length = self.calculate_max_length(player_manager, |p| {
                    format_smolstr!("{:.1}", p.get_average()).len()
                });
                let average_width_pixels = max_average_length.max(3) as i32 * font_width;

                let squad_x = (MAX_NAME_VIEW_LENGTH as i32 * font_width)
                    + BORDER_LEFT_WIDTH
                    + TICKER_WIDTH
                    + BANNER_WIDTH
                    + SPACING_WIDTH;

                let wins_x = squad_x + (MAX_SQUAD_VIEW_LENGTH as i32 + 1) * font_width;
                let losses_x = wins_x + wins_width_pixels + font_width;
                let rating_x = losses_x + losses_width_pixels + font_width;
                let average_x = rating_x + rating_width_pixels + font_width;

                render_state.text_renderer.draw(
                    &mut render_state.sprite_renderer,
                    &render_state.ui_camera,
                    "Squad",
                    squad_x,
                    heading_y,
                    Layer::AfterGauges,
                    TextColor::Green,
                    TextAlignment::Left,
                );

                render_state.text_renderer.draw(
                    &mut render_state.sprite_renderer,
                    &render_state.ui_camera,
                    "W",
                    wins_x + wins_width_pixels,
                    heading_y,
                    Layer::AfterGauges,
                    TextColor::Green,
                    TextAlignment::Right,
                );

                render_state.text_renderer.draw(
                    &mut render_state.sprite_renderer,
                    &render_state.ui_camera,
                    "L",
                    losses_x + losses_width_pixels,
                    heading_y,
                    Layer::AfterGauges,
                    TextColor::Green,
                    TextAlignment::Right,
                );

                render_state.text_renderer.draw(
                    &mut render_state.sprite_renderer,
                    &render_state.ui_camera,
                    "R",
                    rating_x + rating_width_pixels,
                    heading_y,
                    Layer::AfterGauges,
                    TextColor::Green,
                    TextAlignment::Right,
                );

                render_state.text_renderer.draw(
                    &mut render_state.sprite_renderer,
                    &render_state.ui_camera,
                    "Ave",
                    average_x + average_width_pixels,
                    heading_y,
                    Layer::AfterGauges,
                    TextColor::Green,
                    TextAlignment::Right,
                );

                for i in top..bottom {
                    let width = self.render_full_row(
                        player_manager,
                        render_state,
                        game_sprites,
                        me,
                        i,
                        BORDER_LEFT_WIDTH + TICKER_WIDTH,
                        current_y,
                        wins_width_pixels,
                        losses_width_pixels,
                        rating_width_pixels,
                        average_width_pixels,
                    );

                    if width > window_width {
                        window_width = width;
                    }

                    current_y += 12;
                }
            }
            _ => {}
        }

        game_sprites.colors.draw(
            &mut render_state.sprite_renderer,
            &render_state.ui_camera,
            Layer::Gauges,
            crate::render::colors::ColorRenderableKind::BorderInner,
            3,
            heading_border_y,
            window_width + 1,
            1,
        );

        game_sprites.colors.draw_border(
            &mut render_state.sprite_renderer,
            &render_state.ui_camera,
            Layer::Gauges,
            2,
            2,
            window_width + 1,
            current_y + 1,
            true,
        );
    }

    pub fn render(
        &mut self,
        player_manager: &PlayerManager,
        render_state: &mut RenderState,
        game_sprites: &GameSprites,
    ) {
        if self.sorted_players.is_empty() {
            return;
        }

        if self.view == StatboxView::None {
            return;
        }

        self.render_window(player_manager, render_state, game_sprites);
    }

    pub fn set_view(&mut self, player_manager: &PlayerManager, view_kind: StatboxView) {
        self.view = view_kind;

        self.rebuild(player_manager);
    }

    pub fn next_view(&mut self, player_manager: &PlayerManager) {
        self.view = match &self.view {
            StatboxView::Names => StatboxView::Points,
            StatboxView::Points => StatboxView::PointSort,
            StatboxView::PointSort => StatboxView::TeamSort,
            StatboxView::TeamSort => StatboxView::Full,
            StatboxView::Full => StatboxView::None,
            //StatboxView::Frequency => StatboxView::None,
            _ => StatboxView::Names,
        };

        self.rebuild(player_manager);
    }

    pub fn rebuild(&mut self, player_manager: &PlayerManager) {
        log::debug!("Rebuilding statbox");
        self.sort(player_manager);
    }

    fn sort(&mut self, player_manager: &PlayerManager) {
        match &self.view {
            StatboxView::Names | StatboxView::Points | StatboxView::Full => {
                self.sort_by_name(player_manager);
            }
            StatboxView::PointSort => {
                self.sort_by_points(player_manager);
            }
            StatboxView::Frequency | StatboxView::TeamSort => {
                self.sort_by_frequency(player_manager);
            }
            StatboxView::None => {}
        }
    }

    // Add ourself to the sorted list, then our team sorted, then enemies. Returns the index for enemy start.
    fn add_team_to_sort(&mut self, player_manager: &PlayerManager) -> usize {
        let Some(me) = player_manager.get_by_id(player_manager.self_id) else {
            return 0;
        };

        // Always start with self at the top.
        self.sorted_players.push(me.id);

        // Continue with the rest of our frequency
        for player in &player_manager.players {
            if player.id == me.id || player.frequency != me.frequency {
                continue;
            }

            self.sorted_players.push(player.id);
        }

        // Sort our own frequency excluding ourself
        self.sorted_players[1..].sort_by(|left, right| {
            let left_player = player_manager.get_by_id(*left).unwrap();
            let right_player = player_manager.get_by_id(*right).unwrap();

            left_player
                .name
                .to_lowercase_smolstr()
                .cmp(&right_player.name.to_lowercase_smolstr())
        });

        let enemy_begin = self.sorted_players.len();

        // Continue with the rest of the players not on our frequency.
        for player in &player_manager.players {
            if player.frequency == me.frequency {
                continue;
            }

            self.sorted_players.push(player.id);
        }

        enemy_begin
    }

    fn sort_by_name(&mut self, player_manager: &PlayerManager) {
        self.sorted_players.clear();

        let enemy_begin = self.add_team_to_sort(player_manager);
        if enemy_begin == 0 {
            return;
        }

        // Sort other frequency players.
        self.sorted_players[enemy_begin..].sort_by(|left, right| {
            let left_player = player_manager.get_by_id(*left).unwrap();
            let right_player = player_manager.get_by_id(*right).unwrap();

            left_player
                .name
                .to_lowercase_smolstr()
                .cmp(&right_player.name.to_lowercase_smolstr())
        });
    }

    fn sort_by_frequency(&mut self, player_manager: &PlayerManager) {
        self.sorted_players.clear();

        let enemy_begin = self.add_team_to_sort(player_manager);
        if enemy_begin == 0 {
            return;
        }

        // Sort other frequency players.
        self.sorted_players[enemy_begin..].sort_by(|left, right| {
            let left_player = player_manager.get_by_id(*left).unwrap();
            let right_player = player_manager.get_by_id(*right).unwrap();

            if left_player.frequency != right_player.frequency {
                return left_player.frequency.cmp(&right_player.frequency);
            }

            left_player
                .name
                .to_lowercase_smolstr()
                .cmp(&right_player.name.to_lowercase_smolstr())
        });
    }

    fn sort_by_points(&mut self, player_manager: &PlayerManager) {
        self.sorted_players.clear();

        for player in &player_manager.players {
            self.sorted_players.push(player.id);
        }

        self.sorted_players.sort_by(|left, right| {
            let left_player = player_manager.get_by_id(*left).unwrap();
            let right_player = player_manager.get_by_id(*right).unwrap();

            left_player
                .get_points()
                .cmp(&right_player.get_points())
                .reverse()
        });
    }

    fn calculate_max_length<F>(&self, player_manager: &PlayerManager, calculator: F) -> usize
    where
        F: Fn(&Player) -> usize,
    {
        let mut bottom = self.sliding_view.top + self.sliding_view.size;
        if bottom > self.sorted_players.len() {
            bottom = self.sorted_players.len();
        }

        let top = bottom.saturating_sub(self.sliding_view.size);
        let mut max_length = 1;

        for i in top..bottom {
            if let Some(player) = player_manager.get_by_id(self.sorted_players[i]) {
                let length = calculator(player);

                if length > max_length {
                    max_length = length;
                }
            }
        }

        max_length
    }
}
