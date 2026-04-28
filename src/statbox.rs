use smol_str::{StrExt, format_smolstr};

use crate::{
    input::{InputAction, InputModifier, InputState},
    player::{Player, PlayerId, PlayerManager},
    render::{
        game_sprites::{GameSpriteKind, GameSprites},
        layer::Layer,
        render_state::RenderState,
        text_renderer::{TextAlignment, TextColor},
    },
    select_box::SelectBox,
    ship::ShipKind,
};

#[derive(Copy, Clone)]
struct SlidingView {
    pub top: usize,
    pub size: usize,
    pub max_size: usize,
}

impl SlidingView {
    pub fn new(size: usize) -> Self {
        Self {
            top: 0,
            size,
            max_size: size,
        }
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
const SPACING_WIDTH: i32 = 2;

struct FrequencyViewEntry {
    freq: u16,
    points: i64,
    wins: u64,
    losses: u64,
    flags: u64,
}

// TODO: This allocates a lot more than it needs to. It could end up being really slow with many players.
// Could use smolstr to allocate on the stack when rendering instead of building strings for everything.
pub struct Statbox {
    view: StatboxView,
    sorted_players: Vec<PlayerId>,

    sliding_view: SlidingView,
    selected_index: usize,
    selected_player_id: PlayerId,

    frequency_view_entries: Vec<FrequencyViewEntry>,
    restore_selected_index: usize,

    select_box: Option<Box<SelectBox>>,
}

impl Statbox {
    pub fn new() -> Self {
        Self {
            view: StatboxView::None,
            sorted_players: vec![],

            sliding_view: SlidingView::new(15),
            selected_index: 0,
            selected_player_id: PlayerId::invalid(),

            frequency_view_entries: vec![],
            restore_selected_index: 0,

            select_box: None,
        }
    }

    pub fn handle_input(&mut self, input_state: &InputState, player_manager: &PlayerManager) {
        if input_state.is_triggered(InputAction::StatboxCycle) {
            self.next_view(player_manager);
        }

        if input_state.is_triggered(InputAction::StatboxUp) {
            self.move_selected(
                player_manager,
                -1,
                input_state.is_modifier_down(InputModifier::Shift),
            );
        }

        if input_state.is_triggered(InputAction::StatboxDown) {
            self.move_selected(
                player_manager,
                1,
                input_state.is_modifier_down(InputModifier::Shift),
            );
        }
    }

    pub fn display_select_box(&mut self, select_box: Box<SelectBox>) {
        self.select_box = Some(select_box);
    }

    pub fn activate_select_box(&mut self) -> Option<String> {
        if let Some(select_box) = &mut self.select_box {
            let result = select_box.select();

            self.select_box = None;

            return Some(result);
        }

        None
    }

    pub fn cancel_select_box(&mut self) -> bool {
        if self.select_box.is_some() {
            self.select_box = None;
            return true;
        }

        false
    }

    pub fn get_selected_player_id(&self) -> PlayerId {
        self.selected_player_id
    }

    pub fn get_first_playing_id(&self, player_manager: &PlayerManager) -> Option<PlayerId> {
        for player_id in &self.sorted_players {
            if let Some(player) = player_manager.get_by_id(*player_id) {
                if player.ship_kind != ShipKind::Spectator {
                    return Some(*player_id);
                }
            }
        }

        None
    }

    pub fn reset(&mut self) {
        self.selected_index = 0;
        self.restore_selected_index = 0;
        self.selected_player_id = PlayerId::invalid();
        self.sliding_view.top = 0;
        self.sorted_players.clear();
    }

    pub fn set_view(&mut self, player_manager: &PlayerManager, view_kind: StatboxView) {
        if view_kind == StatboxView::Frequency {
            self.restore_selected_index = self.selected_index;
            self.selected_index = 0;
        } else if self.view == StatboxView::Frequency {
            self.selected_index = self.restore_selected_index;
        } else {
            if self.selected_index < self.sorted_players.len() {
                self.selected_player_id = self.sorted_players[self.selected_index];
            }
        }

        self.view = view_kind;

        self.rebuild(player_manager);

        if self.view != StatboxView::Frequency {
            self.restore_selected_index();
        }
    }

    // Adjust our selected into so we are pointing to the same player in the new view.
    fn restore_selected_index(&mut self) {
        if self.view == StatboxView::Frequency {
            return;
        }

        if let Some(new_selected_index) = self
            .sorted_players
            .iter()
            .position(|id| *id == self.selected_player_id)
        {
            self.selected_index = new_selected_index;

            if self.selected_index < self.sliding_view.top {
                self.sliding_view.top = self.selected_index;
            } else if self.selected_index > self.sliding_view.top + self.sliding_view.size {
                self.sliding_view.top = self.selected_index.saturating_sub(self.sliding_view.size);
            }
        }
    }

    pub fn next_view(&mut self, player_manager: &PlayerManager) {
        match &self.view {
            StatboxView::Names => self.set_view(player_manager, StatboxView::Points),
            StatboxView::Points => self.set_view(player_manager, StatboxView::PointSort),
            StatboxView::PointSort => self.set_view(player_manager, StatboxView::TeamSort),
            StatboxView::TeamSort => self.set_view(player_manager, StatboxView::Full),
            StatboxView::Full => self.set_view(player_manager, StatboxView::Frequency),
            StatboxView::Frequency => self.set_view(player_manager, StatboxView::None),
            _ => self.set_view(player_manager, StatboxView::Names),
        };

        self.rebuild(player_manager);
    }

    pub fn rebuild(&mut self, player_manager: &PlayerManager) {
        log::debug!("Rebuilding statbox");
        self.sort(player_manager);
        self.restore_selected_index();
    }

    pub fn render(
        &mut self,
        player_manager: &PlayerManager,
        render_state: &mut RenderState,
        game_sprites: &GameSprites,
    ) {
        // TODO: Select box and statbox should be handled by some interface controller, but this is fine for now.
        if let Some(select_box) = &mut self.select_box {
            select_box.render(render_state, game_sprites);
        }

        if self.sorted_players.is_empty() {
            return;
        }

        if self.view == StatboxView::None {
            return;
        }

        self.updating_sliding_view(player_manager);

        self.render_window(player_manager, render_state, game_sprites);
    }

    pub fn move_selected(&mut self, player_manager: &PlayerManager, direction: i32, shift: bool) {
        if let Some(select_box) = &mut self.select_box {
            select_box.move_selected(direction, shift);
            return;
        }

        if player_manager.players.is_empty() {
            return;
        }

        if self.view == StatboxView::None {
            self.view = StatboxView::Names;
            return;
        }

        if shift {
            if direction < 0 {
                self.selected_index = self.selected_index.saturating_sub(self.sliding_view.size);
                if self.sliding_view.top > self.selected_index {
                    self.sliding_view.top = self.selected_index;
                }
            } else if direction > 0 {
                let max_count = player_manager.players.len();

                self.selected_index += self.sliding_view.size;

                if self.selected_index >= max_count {
                    self.selected_index = max_count - 1;
                }

                self.sliding_view.top = self.selected_index.saturating_sub(self.sliding_view.size);
            }
        } else {
            if direction < 0 {
                if self.selected_index > 0 {
                    self.selected_index -= 1;
                }
            } else if direction > 0 {
                if self.selected_index < player_manager.players.len() - 1 {
                    self.selected_index += 1;
                }
            }
        }

        if self.view == StatboxView::Frequency {
            if !self.frequency_view_entries.is_empty()
                && self.selected_index > self.frequency_view_entries.len() - 1
            {
                self.selected_index = self.frequency_view_entries.len() - 1;
            }
        } else if self.selected_index < self.sorted_players.len() {
            self.selected_player_id = self.sorted_players[self.selected_index];
        }
    }

    fn updating_sliding_view(&mut self, player_manager: &PlayerManager) {
        if self.sliding_view.size >= self.sliding_view.max_size {
            self.sliding_view.size = self.sliding_view.max_size;
        }

        let player_count = player_manager.players.len();

        if player_count < self.sliding_view.top + self.sliding_view.size {
            self.sliding_view.top = player_count.saturating_sub(self.sliding_view.size);
        }

        // Adjusts the sliding view to match our selected index move.
        if self.selected_index >= self.sliding_view.top + self.sliding_view.size {
            self.sliding_view.top += 1;
        } else if self.selected_index < self.sliding_view.top {
            self.sliding_view.top -= 1;
        }
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
        let mut max_length = 1;

        for player in &player_manager.players {
            let length = calculator(player);

            if length > max_length {
                max_length = length;
            }
        }

        max_length
    }

    fn render_name_row(
        &mut self,
        player_manager: &PlayerManager,
        render_state: &mut RenderState,
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
        me: &Player,
        i: usize,
        current_x: i32,
        current_y: i32,
        points_x: i32,
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

        render_state.text_renderer.draw(
            &mut render_state.sprite_renderer,
            &render_state.ui_camera,
            &format_smolstr!("{}", player.get_points()),
            points_x,
            current_y,
            Layer::AfterGauges,
            color,
            TextAlignment::Right,
        );

        points_x
    }

    fn render_full_row(
        &mut self,
        player_manager: &PlayerManager,
        render_state: &mut RenderState,
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

        let namelen = player.name.len().min(MAX_NAME_VIEW_LENGTH);

        render_state.text_renderer.draw(
            &mut render_state.sprite_renderer,
            &render_state.ui_camera,
            &player.name[..namelen],
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

    fn render_ticker(
        &self,
        player_manager: &PlayerManager,
        render_state: &mut RenderState,
        game_sprites: &GameSprites,
        current_y: i32,
        i: usize,
        render_spectator: bool,
    ) {
        if let Some(spectate_sprites) = game_sprites.get_set(GameSpriteKind::Spectate) {
            let spectating = if let Some(player) = player_manager.get_by_id(self.sorted_players[i])
            {
                player.ship_kind == ShipKind::Spectator
            } else {
                false
            } && render_spectator;

            let selected = i == self.selected_index;

            if spectating || selected {
                let index = match (spectating, selected) {
                    (false, false) => 0,
                    (false, true) => 0,
                    (true, false) => 1,
                    (true, true) => 2,
                };

                let renderable = &spectate_sprites.renderables[index];

                render_state.sprite_renderer.draw(
                    &render_state.ui_camera,
                    renderable,
                    BORDER_LEFT_WIDTH,
                    current_y + renderable.size[1] as i32 / 2 - 1,
                    Layer::AfterGauges,
                );
            }
        }
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
                current_y += 12 + 2;
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
                        me,
                        i,
                        BORDER_LEFT_WIDTH + TICKER_WIDTH,
                        current_y,
                    );

                    self.render_ticker(
                        player_manager,
                        render_state,
                        game_sprites,
                        current_y,
                        i,
                        true,
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
                        me,
                        i,
                        BORDER_LEFT_WIDTH + TICKER_WIDTH,
                        current_y,
                        points_width_pixels,
                    );

                    self.render_ticker(
                        player_manager,
                        render_state,
                        game_sprites,
                        current_y,
                        i,
                        true,
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

                const FREQ_END_PIXELS: i32 = 166 + 8;

                let mut points_x = (MAX_NAME_VIEW_LENGTH as i32
                    * render_state.text_renderer.character_width)
                    + BORDER_LEFT_WIDTH
                    + TICKER_WIDTH
                    + BANNER_WIDTH
                    + SPACING_WIDTH
                    + points_width_pixels;

                if points_x < FREQ_END_PIXELS {
                    points_x = FREQ_END_PIXELS;
                }

                render_state.text_renderer.draw(
                    &mut render_state.sprite_renderer,
                    &render_state.ui_camera,
                    "Team Sort",
                    points_x,
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
                        me,
                        i,
                        BORDER_LEFT_WIDTH + TICKER_WIDTH,
                        current_y,
                        points_x,
                    );

                    self.render_ticker(
                        player_manager,
                        render_state,
                        game_sprites,
                        current_y,
                        i,
                        true,
                    );

                    if width > window_width {
                        window_width = width;
                    }

                    current_y += 12;
                }
            }
            StatboxView::Full => {
                const MIN_COL_LEN: usize = 4;

                let font_width = render_state.text_renderer.character_width;

                let max_wins_length = self
                    .calculate_max_length(player_manager, |p| format_smolstr!("{}", p.wins).len())
                    .max(MIN_COL_LEN);
                let wins_width_pixels = max_wins_length as i32 * font_width;

                let max_losses_length = self
                    .calculate_max_length(player_manager, |p| format_smolstr!("{}", p.losses).len())
                    .max(MIN_COL_LEN);
                let losses_width_pixels = max_losses_length as i32 * font_width;

                let max_rating_length = self
                    .calculate_max_length(player_manager, |p| {
                        format_smolstr!("{}", p.get_rating()).len()
                    })
                    .max(MIN_COL_LEN);
                let rating_width_pixels = max_rating_length as i32 * font_width;

                let max_average_length = self
                    .calculate_max_length(player_manager, |p| {
                        format_smolstr!("{:.1}", p.get_average()).len()
                    })
                    .max(MIN_COL_LEN);
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
                        me,
                        i,
                        BORDER_LEFT_WIDTH + TICKER_WIDTH,
                        current_y,
                        wins_width_pixels,
                        losses_width_pixels,
                        rating_width_pixels,
                        average_width_pixels,
                    );

                    self.render_ticker(
                        player_manager,
                        render_state,
                        game_sprites,
                        current_y,
                        i,
                        true,
                    );

                    if width > window_width {
                        window_width = width;
                    }

                    current_y += 12;
                }
            }
            StatboxView::Frequency => {
                let (points_length, wins_length, losses_length, flags_length) =
                    self.build_frequency_view(player_manager);

                let font_width = render_state.text_renderer.character_width;

                let points_width_pixels = points_length as i32 * font_width;
                let wins_width_pixels = wins_length as i32 * font_width;
                let losses_width_pixels = losses_length as i32 * font_width;
                let flags_width_pixels = flags_length as i32 * font_width;

                let points_x =
                    BORDER_LEFT_WIDTH + TICKER_WIDTH + 8 * font_width + points_width_pixels;
                let wins_x = points_x + wins_width_pixels;
                let losses_x = wins_x + losses_width_pixels;
                let flags_x = losses_x + flags_width_pixels;

                render_state.text_renderer.draw(
                    &mut render_state.sprite_renderer,
                    &render_state.ui_camera,
                    "Freq",
                    BORDER_LEFT_WIDTH + TICKER_WIDTH,
                    heading_y,
                    Layer::AfterGauges,
                    TextColor::Green,
                    TextAlignment::Left,
                );

                render_state.text_renderer.draw(
                    &mut render_state.sprite_renderer,
                    &render_state.ui_camera,
                    "Points",
                    points_x,
                    heading_y,
                    Layer::AfterGauges,
                    TextColor::Green,
                    TextAlignment::Right,
                );

                render_state.text_renderer.draw(
                    &mut render_state.sprite_renderer,
                    &render_state.ui_camera,
                    "Win",
                    wins_x,
                    heading_y,
                    Layer::AfterGauges,
                    TextColor::Green,
                    TextAlignment::Right,
                );

                render_state.text_renderer.draw(
                    &mut render_state.sprite_renderer,
                    &render_state.ui_camera,
                    "Lose",
                    losses_x,
                    heading_y,
                    Layer::AfterGauges,
                    TextColor::Green,
                    TextAlignment::Right,
                );

                render_state.text_renderer.draw(
                    &mut render_state.sprite_renderer,
                    &render_state.ui_camera,
                    "Flag",
                    flags_x,
                    heading_y,
                    Layer::AfterGauges,
                    TextColor::Green,
                    TextAlignment::Right,
                );

                self.sliding_view.size;

                if bottom >= self.frequency_view_entries.len() {
                    bottom = self.frequency_view_entries.len();
                }

                let top = bottom.saturating_sub(self.sliding_view.size);

                for i in top..bottom {
                    self.render_ticker(
                        player_manager,
                        render_state,
                        game_sprites,
                        current_y,
                        i,
                        false,
                    );

                    let entry = &self.frequency_view_entries[i];

                    let color = if me.frequency == entry.freq {
                        TextColor::Yellow
                    } else {
                        TextColor::White
                    };

                    let freq_str = if entry.freq < 100 {
                        format_smolstr!("{}", entry.freq)
                    } else {
                        format_smolstr!("----")
                    };

                    render_state.text_renderer.draw(
                        &mut render_state.sprite_renderer,
                        &render_state.ui_camera,
                        &freq_str,
                        BORDER_LEFT_WIDTH + TICKER_WIDTH,
                        current_y,
                        Layer::AfterGauges,
                        color,
                        TextAlignment::Left,
                    );

                    render_state.text_renderer.draw(
                        &mut render_state.sprite_renderer,
                        &render_state.ui_camera,
                        &format_smolstr!("{}", entry.points),
                        points_x,
                        current_y,
                        Layer::AfterGauges,
                        color,
                        TextAlignment::Right,
                    );

                    render_state.text_renderer.draw(
                        &mut render_state.sprite_renderer,
                        &render_state.ui_camera,
                        &format_smolstr!("{}", entry.wins),
                        wins_x,
                        current_y,
                        Layer::AfterGauges,
                        color,
                        TextAlignment::Right,
                    );

                    render_state.text_renderer.draw(
                        &mut render_state.sprite_renderer,
                        &render_state.ui_camera,
                        &format_smolstr!("{}", entry.losses),
                        losses_x,
                        current_y,
                        Layer::AfterGauges,
                        color,
                        TextAlignment::Right,
                    );

                    render_state.text_renderer.draw(
                        &mut render_state.sprite_renderer,
                        &render_state.ui_camera,
                        &format_smolstr!("{}", entry.flags),
                        flags_x,
                        current_y,
                        Layer::AfterGauges,
                        color,
                        TextAlignment::Right,
                    );

                    current_y += 12;
                }

                window_width = flags_x + 1;
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

    fn build_frequency_view(
        &mut self,
        player_manager: &PlayerManager,
    ) -> (usize, usize, usize, usize) {
        const MIN_COL_LEN: usize = 6;

        self.frequency_view_entries.clear();

        if self.sorted_players.is_empty() {
            return (MIN_COL_LEN, MIN_COL_LEN, MIN_COL_LEN, MIN_COL_LEN);
        }

        let Some(start_player) = player_manager.get_by_id(self.sorted_players[0]) else {
            return (MIN_COL_LEN, MIN_COL_LEN, MIN_COL_LEN, MIN_COL_LEN);
        };

        let mut current_freq = start_player.frequency;
        let mut current_points: i64 = 0;
        let mut current_wins: u64 = 0;
        let mut current_losses: u64 = 0;
        let mut current_flags: u64 = 0;

        let mut highest_points_len = 0;
        let mut highest_wins_len = 0;
        let mut highest_losses_len = 0;
        let mut highest_flags_len = 0;

        for id in &self.sorted_players {
            let Some(player) = player_manager.get_by_id(*id) else {
                continue;
            };

            if player.frequency != current_freq {
                highest_points_len =
                    highest_points_len.max(format_smolstr!("{}", current_points).len() + 1);
                highest_wins_len =
                    highest_wins_len.max(format_smolstr!("{}", current_wins).len() + 1);
                highest_losses_len =
                    highest_losses_len.max(format_smolstr!("{}", current_losses).len() + 1);
                highest_flags_len =
                    highest_flags_len.max(format_smolstr!("{}", current_flags).len() + 1);

                self.frequency_view_entries.push(FrequencyViewEntry {
                    freq: current_freq,
                    points: current_points,
                    wins: current_wins,
                    losses: current_losses,
                    flags: current_flags,
                });

                current_freq = player.frequency;
                current_points = 0;
                current_wins = 0;
                current_losses = 0;
                current_flags = 0;
            }

            current_points += player.get_points() as i64;
            current_wins += player.wins as u64;
            current_losses += player.losses as u64;
            current_flags += player.flag_count as u64;
        }

        self.frequency_view_entries.push(FrequencyViewEntry {
            freq: current_freq,
            points: current_points,
            wins: current_wins,
            losses: current_losses,
            flags: current_flags,
        });

        (
            highest_points_len
                .max(format_smolstr!("{}", current_points).len() + 1)
                .max(MIN_COL_LEN),
            highest_wins_len
                .max(format_smolstr!("{}", current_wins).len() + 1)
                .max(MIN_COL_LEN),
            highest_losses_len
                .max(format_smolstr!("{}", current_losses).len() + 1)
                .max(MIN_COL_LEN),
            highest_flags_len
                .max(format_smolstr!("{}", current_flags).len() + 1)
                .max(MIN_COL_LEN),
        )
    }
}
