use smol_str::{StrExt, format_smolstr};

use crate::{
    player::{PlayerId, PlayerManager},
    render::{
        game_sprites::GameSprites,
        layer::Layer,
        render_state::RenderState,
        text_renderer::{TextAlignment, TextColor},
    },
};

struct TextView {
    color: TextColor,
    text: String,
}

struct NamesView {
    entries: Vec<TextView>,
}

struct PointsView {
    entries: Vec<(TextView, String)>,
}

struct PointSortView {
    entries: Vec<(TextView, String)>,
}

struct TeamSortView {
    entries: Vec<(TextView, String, u16)>,
}

struct FullEntry {
    name: TextView,
    squad: TextView,
    wins: (TextColor, u16),
    losses: (TextColor, u16),
    rating: (TextColor, u16),
    average: (TextColor, f32),
}

struct FullView {
    entries: Vec<FullEntry>,
}

struct FrequencyEntry {
    frequency: (TextColor, u16),
    points: (TextColor, i32),
    wins: (TextColor, u16),
    losses: (TextColor, u16),
    flags: (TextColor, u16),
}

struct FrequencyView {
    entries: Vec<FrequencyEntry>,
}

struct SlidingView {
    pub top: usize,
    pub size: usize,
}

impl SlidingView {
    pub fn new(size: usize) -> Self {
        Self { top: 0, size }
    }
}

enum StatboxView {
    Names(NamesView),
    Points(PointsView),
    PointSort(PointSortView),
    TeamSort(TeamSortView),
    Full(FullView),
    Frequency(FrequencyView),
    None,
}

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

    pub fn render(&mut self, render_state: &mut RenderState, game_sprites: &GameSprites) {
        if self.sorted_players.is_empty() {
            return;
        }

        let current_x = 5;
        let mut current_y = 5;

        let mut window_width = 0;

        match &self.view {
            StatboxView::Names(view) => {
                let mut bottom = self.sliding_view.top + self.sliding_view.size;
                if bottom > view.entries.len() {
                    bottom = view.entries.len();
                }

                let top = bottom.saturating_sub(self.sliding_view.size);

                for i in top..bottom {
                    let entry = &view.entries[i];

                    let width = render_state.text_renderer.draw(
                        &mut render_state.sprite_renderer,
                        &render_state.ui_camera,
                        &entry.text,
                        current_x,
                        current_y,
                        Layer::AfterGauges,
                        entry.color,
                        TextAlignment::Left,
                    );

                    if width > window_width {
                        window_width = width;
                    }

                    current_y += 12;
                }
            }
            StatboxView::Points(view) => {
                let mut bottom = self.sliding_view.top + self.sliding_view.size;
                if bottom > view.entries.len() {
                    bottom = view.entries.len();
                }

                let top = bottom.saturating_sub(self.sliding_view.size);

                for i in top..bottom {
                    let entry = &view.entries[i];
                    let color = entry.0.color;
                    let name = &entry.0.text;

                    render_state.text_renderer.draw(
                        &mut render_state.sprite_renderer,
                        &render_state.ui_camera,
                        name,
                        current_x,
                        current_y,
                        Layer::AfterGauges,
                        color,
                        TextAlignment::Left,
                    );

                    // TODO: Determine window size and render on the right

                    render_state.text_renderer.draw(
                        &mut render_state.sprite_renderer,
                        &render_state.ui_camera,
                        &entry.1,
                        185,
                        current_y,
                        Layer::AfterGauges,
                        color,
                        TextAlignment::Right,
                    );

                    window_width = 185;

                    current_y += 12;
                }
            }
            StatboxView::PointSort(view) => {
                let mut bottom = self.sliding_view.top + self.sliding_view.size;
                if bottom > view.entries.len() {
                    bottom = view.entries.len();
                }

                let top = bottom.saturating_sub(self.sliding_view.size);

                for i in top..bottom {
                    let entry = &view.entries[i];
                    let color = entry.0.color;
                    let name = &entry.0.text;

                    render_state.text_renderer.draw(
                        &mut render_state.sprite_renderer,
                        &render_state.ui_camera,
                        name,
                        current_x,
                        current_y,
                        Layer::AfterGauges,
                        color,
                        TextAlignment::Left,
                    );

                    // TODO: Determine window size and render on the right

                    render_state.text_renderer.draw(
                        &mut render_state.sprite_renderer,
                        &render_state.ui_camera,
                        &entry.1,
                        185,
                        current_y,
                        Layer::AfterGauges,
                        color,
                        TextAlignment::Right,
                    );

                    window_width = 185;

                    current_y += 12;
                }
            }
            StatboxView::TeamSort(view) => {
                let mut bottom = self.sliding_view.top + self.sliding_view.size;
                if bottom > view.entries.len() {
                    bottom = view.entries.len();
                }

                let top = bottom.saturating_sub(self.sliding_view.size);
                let mut prev_freq: u32 = 0xFFFFFFFF;

                for i in top..bottom {
                    let entry = &view.entries[i];
                    let color = entry.0.color;
                    let name = &entry.0.text;

                    if entry.2 as u32 != prev_freq {
                        prev_freq = entry.2 as u32;

                        let freq_string = if entry.2 < 100 {
                            format_smolstr!("{:04}", entry.2)
                        } else {
                            format_smolstr!("----")
                        };

                        let width = render_state.text_renderer.draw(
                            &mut render_state.sprite_renderer,
                            &render_state.ui_camera,
                            &freq_string,
                            current_x,
                            current_y,
                            Layer::AfterGauges,
                            TextColor::DarkRed,
                            TextAlignment::Left,
                        );

                        render_state.text_renderer.draw(
                            &mut render_state.sprite_renderer,
                            &render_state.ui_camera,
                            "-------------",
                            current_x + width,
                            current_y,
                            Layer::AfterGauges,
                            TextColor::DarkRed,
                            TextAlignment::Left,
                        );

                        current_y += 12;
                    }

                    render_state.text_renderer.draw(
                        &mut render_state.sprite_renderer,
                        &render_state.ui_camera,
                        name,
                        current_x + 8,
                        current_y,
                        Layer::AfterGauges,
                        color,
                        TextAlignment::Left,
                    );

                    // TODO: Determine window size and render on the right

                    render_state.text_renderer.draw(
                        &mut render_state.sprite_renderer,
                        &render_state.ui_camera,
                        &entry.1,
                        185,
                        current_y,
                        Layer::AfterGauges,
                        color,
                        TextAlignment::Right,
                    );

                    window_width = 185;

                    current_y += 12;
                }
            }
            StatboxView::Full(view) => {
                let mut bottom = self.sliding_view.top + self.sliding_view.size;
                if bottom > view.entries.len() {
                    bottom = view.entries.len();
                }

                let top = bottom.saturating_sub(self.sliding_view.size);

                for i in top..bottom {
                    let entry = &view.entries[i];

                    let _ = entry.name;
                    let _ = entry.squad;
                    let _ = entry.wins;
                    let _ = entry.losses;
                    let _ = entry.rating;
                    let _ = entry.average;
                }
            }
            StatboxView::Frequency(view) => {
                let mut bottom = self.sliding_view.top + self.sliding_view.size;
                if bottom > view.entries.len() {
                    bottom = view.entries.len();
                }

                let top = bottom.saturating_sub(self.sliding_view.size);

                for i in top..bottom {
                    let entry = &view.entries[i];
                    let _ = entry.frequency;
                    let _ = entry.points;
                    let _ = entry.wins;
                    let _ = entry.losses;
                    let _ = entry.flags;
                }
            }
            _ => {}
        }

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

    pub fn next_view(&mut self, player_manager: &PlayerManager) {
        self.view = match &self.view {
            StatboxView::Names(_) => StatboxView::Points(PointsView { entries: vec![] }),
            StatboxView::Points(_) => StatboxView::PointSort(PointSortView { entries: vec![] }),
            StatboxView::PointSort(_) => StatboxView::TeamSort(TeamSortView { entries: vec![] }),
            StatboxView::TeamSort(_) => StatboxView::Full(FullView { entries: vec![] }),
            StatboxView::Full(_) => StatboxView::Frequency(FrequencyView { entries: vec![] }),
            StatboxView::Frequency(_) => StatboxView::None,
            _ => StatboxView::Names(NamesView { entries: vec![] }),
        };

        self.rebuild(player_manager);
    }

    pub fn rebuild(&mut self, player_manager: &PlayerManager) {
        let self_id = player_manager.self_id;
        let Some(me) = player_manager.get_by_id(self_id) else {
            return;
        };

        log::debug!("Rebuilding statbox");
        self.sort(player_manager);

        match &mut self.view {
            StatboxView::Names(view) => {
                view.entries.clear();

                for id in &self.sorted_players {
                    let player = player_manager.get_by_id(*id).unwrap();

                    let color = if player.frequency == me.frequency {
                        TextColor::Yellow
                    } else {
                        TextColor::White
                    };

                    let text_view = TextView {
                        color,
                        text: player.name.clone(),
                    };

                    view.entries.push(text_view);
                }
            }
            StatboxView::Points(view) => {
                view.entries.clear();

                for id in &self.sorted_players {
                    let player = player_manager.get_by_id(*id).unwrap();

                    let color = if player.frequency == me.frequency {
                        TextColor::Yellow
                    } else {
                        TextColor::White
                    };

                    let text_view = TextView {
                        color,
                        text: player.name.clone(),
                    };

                    view.entries
                        .push((text_view, player.get_points().to_string()));
                }
            }
            StatboxView::PointSort(view) => {
                view.entries.clear();

                for id in &self.sorted_players {
                    let player = player_manager.get_by_id(*id).unwrap();

                    let color = if player.frequency == me.frequency {
                        TextColor::Yellow
                    } else {
                        TextColor::White
                    };

                    let text_view = TextView {
                        color,
                        text: player.name.clone(),
                    };

                    view.entries
                        .push((text_view, player.get_points().to_string()));
                }
            }
            StatboxView::TeamSort(view) => {
                view.entries.clear();

                for id in &self.sorted_players {
                    let player = player_manager.get_by_id(*id).unwrap();

                    let color = if player.frequency == me.frequency {
                        TextColor::Yellow
                    } else {
                        TextColor::White
                    };

                    let text_view = TextView {
                        color,
                        text: player.name.clone(),
                    };

                    view.entries.push((
                        text_view,
                        player.get_points().to_string(),
                        player.frequency,
                    ));
                }
            }
            _ => {}
        }
    }

    fn sort(&mut self, player_manager: &PlayerManager) {
        match &self.view {
            StatboxView::Names(_) | StatboxView::Points(_) | StatboxView::Full(_) => {
                self.sort_by_name(player_manager);
            }
            StatboxView::PointSort(_) => {
                self.sort_by_points(player_manager);
            }
            StatboxView::Frequency(_) | StatboxView::TeamSort(_) => {
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
}
