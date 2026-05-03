use crate::{
    clock::GameTick,
    math::{Position, PositionUnit, Rectangle, Velocity, get_heading_from_direction},
    net::packet::bi::ExtraPositionData,
    ship::ShipKind,
};

#[allow(nonstandard_style)]
pub mod StatusFlags {
    pub const Stealth: u8 = 1 << 0;
    pub const Cloak: u8 = 1 << 1;
    pub const XRadar: u8 = 1 << 2;
    pub const Antiwarp: u8 = 1 << 3;
    pub const Flash: u8 = 1 << 4;
    pub const Safety: u8 = 1 << 5;
    pub const UFO: u8 = 1 << 6;
    pub const Inert: u8 = 1 << 7;
}

#[derive(Clone)]
pub struct PlayerItemSet {
    pub items: u32,
}

impl PlayerItemSet {
    pub fn new(items: u32) -> Self {
        Self { items }
    }

    pub fn has_shields(&self) -> bool {
        (self.items & 1) > 0
    }

    pub fn has_super(&self) -> bool {
        ((self.items >> 1) & 1) > 0
    }

    pub fn bursts(&self) -> u8 {
        ((self.items >> 2) & 0x0F) as u8
    }

    pub fn repels(&self) -> u8 {
        ((self.items >> 6) & 0x0F) as u8
    }

    pub fn thors(&self) -> u8 {
        ((self.items >> 10) & 0x0F) as u8
    }

    pub fn bricks(&self) -> u8 {
        ((self.items >> 14) & 0x0F) as u8
    }

    pub fn decoys(&self) -> u8 {
        ((self.items >> 18) & 0x0F) as u8
    }

    pub fn rockets(&self) -> u8 {
        ((self.items >> 22) & 0x0F) as u8
    }

    pub fn portals(&self) -> u8 {
        ((self.items >> 26) & 0x0F) as u8
    }
}

#[derive(PartialEq, Clone, Copy, Eq, Hash)]
pub struct PlayerId {
    pub value: u16,
}

impl PlayerId {
    pub fn new(value: u16) -> PlayerId {
        PlayerId { value }
    }

    pub fn valid(&self) -> bool {
        self.value != 0xFFFF
    }

    pub fn invalid() -> PlayerId {
        PlayerId::new(0xFFFF)
    }
}

impl From<u16> for PlayerId {
    fn from(value: u16) -> Self {
        Self::new(value)
    }
}

#[derive(Clone)]
pub struct Player {
    pub id: PlayerId,

    pub name: String,
    pub squad: String,

    pub ship_kind: ShipKind,
    pub frequency: u16,

    pub position: Option<Position>,
    pub velocity: Velocity,

    pub lerp_velocity: Velocity,
    pub lerp_remaining_ticks: u32,

    pub direction: u8,

    pub bounty: u16,
    pub status: u8,
    pub ping: u8,

    pub attach_parent: PlayerId,
    pub children: Vec<PlayerId>,

    pub flag_count: u16,

    pub last_position_timestamp: GameTick,
    pub enter_delay: u16,

    pub last_extra_data_timestamp: Option<GameTick>,
    pub extra_position_data: Option<ExtraPositionData>,

    pub flag_points: i32,
    pub kill_points: i32,

    pub wins: u16,
    pub losses: u16,

    pub explosion_remaining_ticks: u32,
    pub flash_remaining_ticks: u32,

    pub carrying_ball: bool,
}

impl Player {
    pub fn new(
        id: PlayerId,
        name: &str,
        squad: &str,
        ship_kind: ShipKind,
        frequency: u16,
        flag_points: i32,
        kill_points: i32,
    ) -> Self {
        Self {
            id,
            name: name.to_owned(),
            squad: squad.to_owned(),

            position: None,
            velocity: Velocity::new(PositionUnit(0), PositionUnit(0)),
            lerp_velocity: Velocity::new(PositionUnit(0), PositionUnit(0)),
            lerp_remaining_ticks: 0,

            direction: 0,

            ship_kind,
            frequency,

            bounty: 0,
            status: 0,
            ping: 0,

            attach_parent: PlayerId::invalid(),
            children: vec![],

            flag_count: 0,

            last_position_timestamp: GameTick::empty(),
            enter_delay: 0,

            last_extra_data_timestamp: None,
            extra_position_data: None,

            flag_points,
            kill_points,

            wins: 0,
            losses: 0,

            explosion_remaining_ticks: 0,
            flash_remaining_ticks: 0,

            carrying_ball: false,
        }
    }

    pub fn get_collider(&self, radius: u16) -> Rectangle {
        if let Some(position) = self.position {
            Rectangle::from_radius(position, PositionUnit(radius as i32 * 1000))
        } else {
            Rectangle::from_radius(Position::empty(), PositionUnit(0))
        }
    }

    pub fn get_heading(&self) -> glam::Vec2 {
        get_heading_from_direction(self.direction)
    }

    pub fn is_dead(&self) -> bool {
        self.enter_delay > 0
    }

    pub fn is_synchronized(&self, current_tick: GameTick) -> bool {
        const PLAYER_SYNC_TIMEOUT: i32 = 200;

        current_tick.diff(&self.last_position_timestamp).abs() < PLAYER_SYNC_TIMEOUT
    }

    pub fn get_points(&self) -> i32 {
        self.flag_points.wrapping_add(self.kill_points)
    }

    pub fn get_rating(&self) -> u32 {
        let wins = self.wins as i64;
        let losses = self.losses as i64;
        let kill_points = self.kill_points as i64;

        let r = ((kill_points + (wins - losses) * 10) * 10) / (wins + 100);

        if r < 0 {
            return 0;
        }

        r as u32
    }

    pub fn get_average(&self) -> f32 {
        if self.wins == 0 {
            return 0.0f32;
        }

        let avg = self.kill_points as f32 / self.wins as f32;

        (avg * 10.0f32).floor() / 10.0f32
    }
}

pub struct PlayerManager {
    pub players: Vec<Player>,
    pub self_id: PlayerId,

    // Fast lookup for player index from PlayerId.
    player_mapping: Vec<u16>,
}

impl PlayerManager {
    const INVALID_PLAYER_INDEX: u16 = 0xFFFF;

    pub fn new() -> Self {
        let mut player_mapping = vec![];

        for _ in 0..65535 {
            player_mapping.push(Self::INVALID_PLAYER_INDEX);
        }

        Self {
            players: vec![],
            self_id: PlayerId::invalid(),
            player_mapping,
        }
    }

    // Inserts a player into active player list.
    // Returns Some(Player) if a player existed with the same player id.
    pub fn add_player(&mut self, player: Player) -> Option<Player> {
        let existed = self.remove_player(player.id);

        self.player_mapping[player.id.value as usize] = self.players.len() as u16;
        self.players.push(player);

        existed
    }

    pub fn remove_player(&mut self, player_id: PlayerId) -> Option<Player> {
        if !player_id.valid() {
            log::error!("Trying to remove invalid PlayerId");
            return None;
        }

        let index = self.player_mapping[player_id.value as usize];
        if index == Self::INVALID_PLAYER_INDEX {
            return None;
        }

        // swap_remove will swap the last player into this removed player's spot, so we need to update the mapping to match.
        let swap_player_id = self.players[self.players.len() - 1].id;
        self.player_mapping[swap_player_id.value as usize] = index;

        self.player_mapping[player_id.value as usize] = Self::INVALID_PLAYER_INDEX;
        let result = self.players.swap_remove(index as usize);

        Some(result)
    }

    pub fn get_by_name(&self, name: &str) -> Option<&Player> {
        self.players.iter().find(|p| p.name == name)
    }

    pub fn get_by_name_mut(&mut self, name: &str) -> Option<&mut Player> {
        self.players.iter_mut().find(|p| p.name == name)
    }

    pub fn get_by_id(&self, player_id: PlayerId) -> Option<&Player> {
        if !player_id.valid() {
            return None;
        }

        let index = self.player_mapping[player_id.value as usize];

        if index == Self::INVALID_PLAYER_INDEX {
            return None;
        }

        Some(&self.players[index as usize])
    }

    pub fn get_by_id_mut(&mut self, player_id: PlayerId) -> Option<&mut Player> {
        if !player_id.valid() {
            return None;
        }

        let index = self.player_mapping[player_id.value as usize];

        if index == Self::INVALID_PLAYER_INDEX {
            return None;
        }

        Some(&mut self.players[index as usize])
    }

    pub fn get_self(&self) -> Option<&Player> {
        self.get_by_id(self.self_id)
    }

    pub fn get_self_mut(&mut self) -> Option<&mut Player> {
        self.get_by_id_mut(self.self_id)
    }

    pub fn get_frequency_count(&self, frequency: u16) -> usize {
        let mut count = 0;

        for player in &self.players {
            if player.frequency == frequency {
                count += 1;
            }
        }

        count
    }

    pub fn attach_player(&mut self, requester_id: PlayerId, parent_id: PlayerId) {
        self.detach_player(requester_id);

        if let Some(requester) = self.get_by_id_mut(requester_id) {
            requester.attach_parent = parent_id;

            if let Some(parent) = self.get_by_id_mut(parent_id) {
                parent.children.push(requester_id);
            }
        }
    }

    // The provided player will have no parent and the parent will remove this player from its children.
    pub fn detach_player(&mut self, player_id: PlayerId) {
        let Some(player) = self.get_by_id_mut(player_id) else {
            return;
        };

        let parent_id = player.attach_parent;
        player.attach_parent = PlayerId::invalid();

        if let Some(parent) = self.get_by_id_mut(parent_id) {
            if let Some(index) = parent.children.iter().position(|id| *id == player_id) {
                parent.children.swap_remove(index);
            }
        }
    }

    // Returns true if self was a child.
    pub fn detach_all_children(&mut self, parent_id: PlayerId) -> bool {
        let mut self_was_child = false;

        for player in &mut self.players {
            if player.attach_parent == parent_id {
                player.attach_parent = PlayerId::invalid();
                if player.id == self.self_id {
                    self_was_child = true;
                }
            }
        }

        if let Some(player) = self.get_by_id_mut(parent_id) {
            player.children.clear();
        }

        self_was_child
    }
}
