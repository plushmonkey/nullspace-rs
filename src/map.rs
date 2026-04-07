use thiserror::Error;

use crate::math::Position;

pub type TileId = u8;

pub const TILE_ID_BORDER: TileId = 20;
pub const TILE_ID_FIRST_DOOR: TileId = 162;
pub const TILE_ID_LAST_DOOR: TileId = 169;
pub const TILE_ID_FLAG: TileId = 170;
pub const TILE_ID_SAFE: TileId = 171;
pub const TILE_ID_GOAL: TileId = 172;
pub const TILE_ID_WORMHOLE: TileId = 220;

#[derive(Error, Debug)]
pub enum MapError {
    #[error("{0}")]
    IoError(#[from] std::io::Error),

    #[error("invalid bitmap header")]
    InvalidBitmapHeader,
}

pub struct Map {
    pub filename: String,
    pub tiles: Box<[TileId; 1024 * 1024]>,
    pub checksum: u32,
}

impl Map {
    pub fn load(filename: &str) -> Result<Self, MapError> {
        let data = std::fs::read(filename)?;

        Map::new(filename, &data)
    }

    pub fn new(filename: &str, data: &[u8]) -> Result<Self, MapError> {
        let mut map = Map {
            filename: filename.to_owned(),
            tiles: vec![0; 1024 * 1024].into_boxed_slice().try_into().unwrap(),
            checksum: 0,
        };

        let mut position: usize = 0;

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
            let tile = u32::from_le_bytes(data[position..position + 4].try_into().unwrap());

            let x = (tile >> 0) & 0xFFF;
            let y = (tile >> 12) & 0xFFF;
            let tile_id = ((tile >> 24) & 0xFF) as u8;

            let index: usize = y as usize * 1024 + x as usize;
            map.tiles[index] = tile_id;

            position += 4;
        }

        Ok(map)
    }

    pub fn empty(filename: &str) -> Map {
        Map {
            filename: filename.to_owned(),
            tiles: vec![0; 1024 * 1024].into_boxed_slice().try_into().unwrap(),
            checksum: 0,
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

    pub fn is_solid(&self, x: u16, y: u16) -> bool {
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

        if tile_id >= 242 && tile_id <= 252 {
            return true;
        }

        false
    }

    pub fn is_solid_position(&self, position: Position) -> bool {
        let (tile_x, tile_y) = position.to_tile();
        return self.is_solid(tile_x, tile_y);
    }

    pub fn is_solid_empty_doors(&self, x: u16, y: u16) -> bool {
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

        if tile_id >= 242 && tile_id <= 252 {
            return true;
        }

        false
    }

    pub fn is_solid_empty_doors_position(&self, position: Position) -> bool {
        let (tile_x, tile_y) = position.to_tile();
        return self.is_solid_empty_doors(tile_x, tile_y);
    }

    pub fn can_fit(&self, x: u16, y: u16, radius: u16, _frequency: u16) -> bool {
        let radius = radius as i16;
        for y_offset in -radius..radius {
            for x_offset in -radius..radius {
                if self.is_solid(
                    x.saturating_add_signed(x_offset),
                    y.saturating_add_signed(y_offset),
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

    fn get_index(x: u16, y: u16) -> usize {
        y as usize * 1024 + x as usize
    }
}
