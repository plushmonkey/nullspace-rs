use thiserror::Error;

use crate::{
    arena_settings::ArenaSettings,
    clock::GameTick,
    math::{PixelUnit, Position, ray_box_intersect},
    rng::VieRng,
};

pub type TileId = u8;

pub const TILE_ID_BORDER: TileId = 20;
pub const TILE_ID_FIRST_DOOR: TileId = 162;
pub const TILE_ID_LAST_DOOR: TileId = 169;
pub const TILE_ID_FLAG: TileId = 170;
pub const TILE_ID_SAFE: TileId = 171;
pub const TILE_ID_GOAL: TileId = 172;
pub const TILE_ID_WORMHOLE: TileId = 220;
pub const TILE_ID_FAKE_BRICK: TileId = 250;

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct Tile {
    value: u32,
}

impl Tile {
    pub fn new(value: u32) -> Self {
        Self { value }
    }

    pub fn from_parts(id: TileId, x: u16, y: u16) -> Self {
        let value = ((id as u32) << 24) | ((y as u32) << 12) | (x as u32);

        Self { value }
    }

    pub fn id(&self) -> TileId {
        ((self.value >> 24) & 0xFF) as TileId
    }

    pub fn x(&self) -> u16 {
        ((self.value >> 0) & 0xFFF) as u16
    }

    pub fn y(&self) -> u16 {
        ((self.value >> 12) & 0xFFF) as u16
    }
}

#[derive(Copy, Clone, Debug)]
pub enum AnimatedTileKind {
    Goal,
    AsteroidSmall,
    AsteroidSmall2,
    AsteroidLarge,
    SpaceStation,
    Wormhole,
    Flag,
}
pub const ANIMATED_TILE_KIND_COUNT: usize = 7;

impl AnimatedTileKind {
    pub fn get_tile_size(&self) -> u16 {
        match self {
            AnimatedTileKind::Goal => 1,
            AnimatedTileKind::AsteroidSmall => 1,
            AnimatedTileKind::AsteroidSmall2 => 1,
            AnimatedTileKind::AsteroidLarge => 2,
            AnimatedTileKind::SpaceStation => 6,
            AnimatedTileKind::Wormhole => 5,
            AnimatedTileKind::Flag => 1,
        }
    }

    pub fn try_from_id(id: TileId) -> Option<Self> {
        match id {
            172 => Some(AnimatedTileKind::Goal),
            216 => Some(AnimatedTileKind::AsteroidSmall),
            218 => Some(AnimatedTileKind::AsteroidSmall2),
            217 => Some(AnimatedTileKind::AsteroidLarge),
            219 => Some(AnimatedTileKind::SpaceStation),
            220 => Some(AnimatedTileKind::Wormhole),
            170 => Some(AnimatedTileKind::Flag),
            _ => None,
        }
    }
}

pub struct AnimatedTileSet {
    pub tiles: Vec<Tile>,
}

#[derive(Error, Debug)]
pub enum MapError {
    #[error("{0}")]
    IoError(#[from] std::io::Error),

    #[error("invalid bitmap header")]
    InvalidBitmapHeader,
}

#[derive(Copy, Clone, Debug)]
pub struct DoorRng {
    pub rng: VieRng,
    pub last_tick: GameTick,
    pub current_mode: u8,
    pub last_mode: u8,
}

impl DoorRng {
    pub fn new(seed: u32, last_tick: GameTick, current_mode: u8, last_mode: u8) -> Self {
        Self {
            rng: VieRng::new(seed as i32),
            last_tick,
            current_mode,
            last_mode,
        }
    }
}

pub struct Brick {
    pub frequency: u16,
    pub end_tick: GameTick,
    pub tile: Tile,
}

pub struct Map {
    pub filename: String,
    pub tiles: Box<[TileId; 1024 * 1024]>,
    pub checksum: u32,
    pub animated_tiles: [Vec<Tile>; ANIMATED_TILE_KIND_COUNT],

    pub doors: Vec<Tile>,
    pub door_rng: Option<DoorRng>,

    pub bricks: Vec<Brick>,
}

impl Map {
    pub fn load(filename: &str, door_rng: Option<DoorRng>) -> Result<Self, MapError> {
        let data = std::fs::read(filename)?;

        Map::new(filename, &data, door_rng)
    }

    pub fn new(filename: &str, data: &[u8], door_rng: Option<DoorRng>) -> Result<Self, MapError> {
        let mut map = Self::empty(filename);
        let mut position: usize = 0;

        map.door_rng = door_rng;

        if data.len() >= 4 {
            // If we have a bitmap header, jump to tile data by reading header.
            if data[0] == b'B' && data[1] == b'M' {
                position = u32::from_le_bytes(data[2..6].try_into().unwrap()) as usize;
            }
        }

        if position >= data.len() {
            return Err(MapError::InvalidBitmapHeader);
        }

        let tile_count = (data.len() - position) / size_of::<u32>();

        for _ in 0..tile_count {
            let tile = Tile::new(u32::from_le_bytes(
                data[position..position + 4].try_into().unwrap(),
            ));

            let x = tile.x();
            let y = tile.y();
            let tile_id = tile.id();

            let index: usize = y as usize * 1024 + x as usize;
            map.tiles[index] = tile_id;

            if let Some(animated_tile) = AnimatedTileKind::try_from_id(tile_id) {
                let size = animated_tile.get_tile_size();

                map.animated_tiles[animated_tile as usize].push(tile);

                for oy in 0..size {
                    let cy = y + oy;

                    for ox in 0..size {
                        let cx = x + ox;

                        let index: usize = cy as usize * 1024 + cx as usize;
                        map.tiles[index] = tile_id;
                    }
                }
            } else if tile_id >= TILE_ID_FIRST_DOOR && tile_id <= TILE_ID_LAST_DOOR {
                map.doors.push(tile);
            }

            position += 4;
        }

        Ok(map)
    }

    pub fn empty(filename: &str) -> Map {
        Map {
            filename: filename.to_owned(),
            tiles: vec![0; 1024 * 1024].into_boxed_slice().try_into().unwrap(),
            checksum: 0,
            animated_tiles: [(); ANIMATED_TILE_KIND_COUNT].map(|_| Vec::new()),
            doors: vec![],
            door_rng: None,
            bricks: vec![],
        }
    }

    pub fn get_tile(&self, x: u16, y: u16) -> TileId {
        if x >= 1024 || y >= 1024 {
            return TILE_ID_BORDER;
        }

        let index = Map::get_index(x, y);

        self.tiles[index]
    }

    pub fn get_tile_from_position(&self, position: &Position) -> TileId {
        let (tile_x, tile_y) = position.to_tile();
        return self.get_tile(tile_x, tile_y);
    }

    pub fn is_door(&self, x: u16, y: u16) -> bool {
        let tile_id = self.get_tile(x, y);
        tile_id >= TILE_ID_FIRST_DOOR && tile_id <= TILE_ID_LAST_DOOR
    }

    pub fn is_door_position(&self, position: Position) -> bool {
        let (tile_x, tile_y) = position.to_tile();
        return self.is_door(tile_x, tile_y);
    }

    pub fn is_solid(&self, x: u16, y: u16, frequency: u16) -> bool {
        let tile_id = self.get_tile(x, y);

        if tile_id == 0 {
            return false;
        }

        if tile_id < 170 {
            return true;
        }

        if tile_id == 220 {
            return false;
        }

        if tile_id >= 192 && tile_id <= 240 {
            return true;
        }

        if tile_id == 250 {
            if let Some(brick) = self.get_brick(x, y) {
                return brick.frequency != frequency;
            }
        }

        if tile_id >= 242 && tile_id <= 252 {
            return true;
        }

        false
    }

    pub fn is_solid_position(&self, position: Position, frequency: u16) -> bool {
        let (tile_x, tile_y) = position.to_tile();
        return self.is_solid(tile_x, tile_y, frequency);
    }

    pub fn is_solid_empty_doors(&self, x: u16, y: u16, frequency: u16) -> bool {
        let tile_id = self.get_tile(x, y);

        if tile_id == 0 {
            return false;
        }

        if tile_id >= TILE_ID_FIRST_DOOR && tile_id <= TILE_ID_LAST_DOOR {
            return false;
        }

        if tile_id < 170 {
            return true;
        }

        if tile_id == 220 {
            return false;
        }

        if tile_id >= 192 && tile_id <= 240 {
            return true;
        }

        if tile_id == 250 {
            if let Some(brick) = self.get_brick(x, y) {
                return brick.frequency != frequency;
            }
        }

        if tile_id >= 242 && tile_id <= 252 {
            return true;
        }

        false
    }

    pub fn is_solid_empty_doors_position(&self, position: Position, frequency: u16) -> bool {
        let (tile_x, tile_y) = position.to_tile();
        return self.is_solid_empty_doors(tile_x, tile_y, frequency);
    }

    pub fn can_fit(&self, x: u16, y: u16, radius: u16, frequency: u16) -> bool {
        let radius = radius as i16;
        for y_offset in -radius..radius {
            for x_offset in -radius..radius {
                if self.is_solid(
                    x.saturating_add_signed(x_offset),
                    y.saturating_add_signed(y_offset),
                    frequency,
                ) {
                    return false;
                }
            }
        }

        true
    }

    pub fn can_fit_position(&self, position: Position, radius: u16, frequency: u16) -> bool {
        let (x, y) = position.to_tile();

        self.can_fit(x, y, radius, frequency)
    }

    pub fn tick(&mut self, settings: &ArenaSettings, current_tick: GameTick) {
        self.tick_bricks(current_tick);

        if self.doors.is_empty() {
            return;
        }

        let Some(door_rng) = &mut self.door_rng else {
            return;
        };

        let tick_diff = current_tick.diff(&door_rng.last_tick);

        let delay = if settings.door_delay > 0 {
            settings.door_delay as i32
        } else {
            1
        };

        let update_count = tick_diff / delay;

        for _ in 0..update_count {
            let new_door_mode = Self::update_door_seed(settings.door_mode, door_rng);

            door_rng.last_mode = door_rng.current_mode;
            door_rng.current_mode = new_door_mode as u8;

            Self::apply_door_mode(&mut self.doors, &mut self.tiles[..], new_door_mode as u32);
        }

        door_rng.last_tick = door_rng.last_tick + update_count * delay;
    }

    pub fn clear_bricks(&mut self) {
        for brick in &self.bricks {
            let tile_index = Self::get_index(brick.tile.x(), brick.tile.y());
            self.tiles[tile_index] = brick.tile.id();
        }

        self.bricks.clear();
    }

    pub fn insert_brick(&mut self, x: u16, y: u16, frequency: u16, end_tick: GameTick) {
        if x >= 1024 || y >= 1024 {
            return;
        }

        let tile_index = Self::get_index(x, y);

        for index in 0..self.bricks.len() {
            if self.bricks[index].tile.x() == x && self.bricks[index].tile.y() == y {
                self.tiles[tile_index] = self.bricks[index].tile.id();
                self.bricks.swap_remove(index);
                break;
            }
        }

        let tile_id = self.get_tile(x, y);
        let tile = Tile::from_parts(tile_id, x, y);

        self.bricks.push(Brick {
            frequency,
            end_tick,
            tile,
        });

        self.tiles[tile_index] = TILE_ID_FAKE_BRICK;
    }

    fn get_brick(&self, x: u16, y: u16) -> Option<&Brick> {
        // This is probably the fastest method in average game since there's so few bricks.
        // Could switch to a hashmap with x, y as lookup key, but it's probably slower or irrelevant.
        for brick in &self.bricks {
            if brick.tile.x() == x && brick.tile.y() == y {
                return Some(brick);
            }
        }

        None
    }

    fn tick_bricks(&mut self, current_tick: GameTick) {
        let mut brick_index = 0;

        loop {
            if brick_index >= self.bricks.len() {
                break;
            }

            let brick = &self.bricks[brick_index];

            if current_tick >= brick.end_tick {
                let tile_index = Self::get_index(brick.tile.x(), brick.tile.y());

                assert!(self.tiles[tile_index] == TILE_ID_FAKE_BRICK);
                self.tiles[tile_index] = brick.tile.id();

                self.bricks.swap_remove(brick_index);
                continue;
            }

            brick_index = brick_index + 1;
        }
    }

    // This mutates the door seed and returns the new door mode.
    fn update_door_seed(door_mode: i16, door_rng: &mut DoorRng) -> i32 {
        let mut seed = door_rng.rng.seed;

        if door_mode == -2 {
            seed = door_rng.rng.next() as i32;
        } else if door_mode == -1 {
            let mut table: [u32; 7] = [0, 0, 0, 0, 0, 0, 0];

            for i in 0..7 {
                table[i] = door_rng.rng.next();
            }

            table[6] &= 0x8000000F;
            if (table[6] as i32) < 0 {
                table[6] = ((table[6].wrapping_sub(1)) | 0xFFFFFFF0).wrapping_add(1);
            }
            table[6] = (-(((table[6] as i32) != 0) as i32) & 0x80) as u32;

            table[5] &= 0x80000007;
            if (table[5] as i32) < 0 {
                table[5] = ((table[5].wrapping_sub(1)) | 0xFFFFFFF8).wrapping_add(1);
            }
            table[5] = (-(((table[5] as i32) != 0) as i32) & 0x40) as u32;

            table[4] &= 0x80000003;
            if (table[4] as i32) < 0 {
                table[4] = ((table[4].wrapping_sub(1)) | 0xFFFFFFFC).wrapping_add(1);
            }
            table[4] = (-(((table[4] as i32) != 0) as i32) & 0x20) as u32;

            table[3] &= 0x8000000F;
            if (table[3] as i32) < 0 {
                table[3] = ((table[3].wrapping_sub(1)) | 0xFFFFFFF0).wrapping_add(1);
            }
            table[3] = (-(((table[3] as i32) != 0) as i32) & 0x08) as u32;

            table[2] &= 0x80000007;
            if (table[2] as i32) < 0 {
                table[2] = ((table[2].wrapping_sub(1)) | 0xFFFFFFF8).wrapping_add(1);
            }
            table[2] = (-(((table[2] as i32) != 0) as i32) & 0x04) as u32;

            table[1] &= 0x80000003;
            if (table[1] as i32) < 0 {
                table[1] = ((table[1].wrapping_sub(1)) | 0xFFFFFFFC).wrapping_add(1);
            }
            table[1] = (-(((table[1] as i32) != 0) as i32) & 0x02) as u32;

            table[0] &= 0x80000001;
            if (table[0] as i32) < 0 {
                table[0] = ((table[0].wrapping_sub(1)) | 0xFFFFFFFE).wrapping_add(1);
            }
            table[0] = (-(((table[0] as i32) != 0) as i32) & 0x11) as u32;

            seed = table[6]
                .wrapping_add(table[5])
                .wrapping_add(table[4])
                .wrapping_add(table[3])
                .wrapping_add(table[2])
                .wrapping_add(table[1])
                .wrapping_add(table[0]) as i32;
        } else if door_mode >= 0 {
            seed = door_mode as i32;
        }

        seed
    }

    pub fn set_door_seed(&mut self, seed: u32, timestamp: GameTick) {
        if let Some(door_rng) = &mut self.door_rng {
            if timestamp <= door_rng.last_tick {
                //  We updated our timestamp in the time it took for the security packet to arrive.
                // Revert to the previous one and set the new timer.

                Self::apply_door_mode(
                    &mut self.doors,
                    &mut self.tiles[..],
                    door_rng.last_mode as u32,
                );

                door_rng.current_mode = door_rng.last_mode;
                door_rng.last_tick = timestamp;
                door_rng.rng.seed = seed as i32;

                return;
            }
        }

        let (current_mode, last_mode) = if let Some(door_rng) = &self.door_rng {
            (door_rng.current_mode, door_rng.last_mode)
        } else {
            (0, 0)
        };

        self.door_rng = Some(DoorRng::new(seed, timestamp, current_mode, last_mode));
    }

    pub fn set_door_mode(&mut self, door_mode: u8) {
        Self::apply_door_mode(&mut self.doors, &mut self.tiles[..], door_mode as u32);

        if let Some(door_rng) = &mut self.door_rng {
            door_rng.last_mode = door_rng.current_mode;
            door_rng.current_mode = door_mode;
        }
    }

    fn apply_door_mode(doors: &mut Vec<Tile>, tiles: &mut [u8], door_mode: u32) {
        let bottom = door_mode & 0xFF;

        let make_bits = |value: u32, bit: u32| -(((value & bit) != 0) as i32) as u32;

        let table: [u32; 8] = [
            ((!bottom & 1) << 3) | 0xA2,
            (make_bits(bottom, 2) & 0xF9) + 0xAA,
            (make_bits(bottom, 4) & 0xFA) + 0xAA,
            (make_bits(bottom, 8) & 0xFB) + 0xAA,
            (make_bits(bottom, 0x10) & 0xFC) + 0xAA,
            (make_bits(bottom, 0x20) & 0xFD) + 0xAA,
            (!(bottom >> 5) & 2) | 0xA8,
            0xAA - ((bottom & 0x80) != 0) as u32,
        ];

        for door_tile in doors {
            let index = (door_tile.id() - TILE_ID_FIRST_DOOR) as usize;
            let id = table[index] as u8;

            let tile_index = Self::get_index(door_tile.x(), door_tile.y());

            tiles[tile_index] = id;
        }
    }

    fn get_index(x: u16, y: u16) -> usize {
        y as usize * 1024 + x as usize
    }

    // max_distance is in tile-space
    pub fn cast(
        &self,
        from: Position,
        direction: glam::Vec2,
        max_distance: f32,
        frequency: u16,
    ) -> CastResult {
        let mut result = CastResult {
            hit: false,
            distance: 0.0f32,
            position: Position::empty(),
        };

        if self.is_solid_position(from, frequency) {
            result.hit = true;
            result.distance = 0.0f32;
            result.position = from;
            return result;
        }

        let unit_step = glam::Vec2::new(
            (1.0f32 + (direction.y / direction.x) * (direction.y / direction.x)).sqrt(),
            (1.0f32 + (direction.x / direction.y) * (direction.x / direction.y)).sqrt(),
        );
        let from = glam::Vec2::new(from.x.0 as f32 / 16000.0f32, from.y.0 as f32 / 16000.0f32);

        let mut check = glam::Vec2::new(from.x.floor(), from.y.floor());
        let mut step = glam::Vec2::ZERO;
        let mut travel = glam::Vec2::ZERO;

        if direction.x < 0.0f32 {
            step.x = -1.0f32;
            travel.x = (from.x - check.x) * unit_step.x;
        } else {
            step.x = 1.0f32;
            travel.x = (check.x + 1.0f32 - from.x) * unit_step.x;
        }

        if direction.y < 0.0f32 {
            step.y = -1.0f32;
            travel.y = (from.y - check.y) * unit_step.y;
        } else {
            step.y = 1.0f32;
            travel.y = (check.y + 1.0f32 - from.y) * unit_step.y;
        }

        let mut distance = 0.0f32;

        while distance < max_distance {
            let clear_distance = distance;

            if travel.x < travel.y {
                check.x += step.x;
                distance = travel.x;
                travel.x += unit_step.x;
            } else {
                check.y += step.y;
                distance = travel.y;
                travel.y += unit_step.y;
            }

            if self.is_solid(check.x.floor() as u16, check.y.floor() as u16, frequency) {
                result.hit = true;
                result.distance = clear_distance;
                break;
            }
        }

        if result.hit {
            if let Some(dist) =
                ray_box_intersect(from, direction, check, glam::Vec2::new(1.0f32, 1.0f32))
            {
                if dist < max_distance {
                    result.distance = dist;
                    let hit_position_tile = from + direction * dist;
                    result.position = Position::from_pixels(
                        PixelUnit((hit_position_tile.x * 16.0f32) as i32),
                        PixelUnit((hit_position_tile.y * 16.0f32) as i32),
                    );

                    return result;
                }
            }
        }

        result.hit = false;
        result.distance = max_distance;

        let hit_position_tile = from + direction * max_distance;

        result.position = Position::from_pixels(
            PixelUnit((hit_position_tile.x * 16.0f32) as i32),
            PixelUnit((hit_position_tile.y * 16.0f32) as i32),
        );

        result
    }
}

pub struct CastResult {
    pub hit: bool,
    pub distance: f32,
    pub position: Position,
}
