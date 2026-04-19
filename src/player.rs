use crate::{
    clock::GameTick,
    math::{Position, PositionUnit, Rectangle, Velocity, get_heading_from_direction},
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
    pub flag_count: u16,

    pub last_position_timestamp: GameTick,
    pub enter_delay: u16,

    pub energy: Option<u32>,
    pub s2c_latency: Option<u16>,
    pub flag_timer: Option<u16>,
    pub items: Option<PlayerItemSet>,

    pub flag_points: u32,
    pub kill_points: u32,

    pub explosion_remaining_ticks: u32,
    pub flash_remaining_ticks: u32,
}

impl Player {
    pub fn new(
        id: PlayerId,
        name: &str,
        squad: &str,
        ship_kind: ShipKind,
        frequency: u16,
        flag_points: u32,
        kill_points: u32,
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
            flag_count: 0,

            last_position_timestamp: GameTick::empty(),
            enter_delay: 0,

            energy: None,
            s2c_latency: None,
            flag_timer: None,
            items: None,

            flag_points,
            kill_points,

            explosion_remaining_ticks: 0,
            flash_remaining_ticks: 0,
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

    pub fn get_points(&self) -> u32 {
        self.flag_points.wrapping_add(self.kill_points)
    }
}

pub struct PlayerManager {
    pub players: Vec<Player>,
    pub self_id: PlayerId,
}

impl PlayerManager {
    pub fn new() -> Self {
        Self {
            players: vec![],
            self_id: PlayerId::invalid(),
        }
    }

    // Inserts a player into active player list.
    // Returns Some(Player) if a player existed with the same player id.
    pub fn add_player(&mut self, player: Player) -> Option<Player> {
        let existed = self.remove_player(player.id);

        self.players.push(player);

        existed
    }

    pub fn remove_player(&mut self, player_id: PlayerId) -> Option<Player> {
        if let Some(idx) = self.players.iter().position(|p| p.id == player_id) {
            Some(self.players.swap_remove(idx))
        } else {
            None
        }
    }

    pub fn get_by_name(&self, name: &str) -> Option<&Player> {
        self.players.iter().find(|p| p.name == name)
    }

    pub fn get_by_name_mut(&mut self, name: &str) -> Option<&mut Player> {
        self.players.iter_mut().find(|p| p.name == name)
    }

    pub fn get_by_id(&self, player_id: PlayerId) -> Option<&Player> {
        self.players.iter().find(|p| p.id == player_id)
    }

    pub fn get_by_id_mut(&mut self, player_id: PlayerId) -> Option<&mut Player> {
        self.players.iter_mut().find(|p| p.id == player_id)
    }
}
