use std::ops::{Add, AddAssign, Sub};

pub const MAX_POSITION: i32 = 1024 * 16000;

#[derive(Debug, Copy, Clone)]
pub struct PixelUnit(pub i32);
#[derive(Debug, Copy, Clone)]
pub struct PositionUnit(pub i32);

impl From<PixelUnit> for PositionUnit {
    fn from(value: PixelUnit) -> Self {
        Self { 0: value.0 * 1000 }
    }
}

impl From<PositionUnit> for PixelUnit {
    fn from(value: PositionUnit) -> Self {
        Self { 0: value.0 / 1000 }
    }
}

impl Add<PositionUnit> for PositionUnit {
    type Output = Self;

    fn add(self, rhs: PositionUnit) -> Self::Output {
        Self::Output {
            0: self.0.saturating_add(rhs.0),
        }
    }
}

impl Sub<PositionUnit> for PositionUnit {
    type Output = Self;

    fn sub(self, rhs: PositionUnit) -> Self::Output {
        Self::Output {
            0: self.0.saturating_sub(rhs.0),
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct Position {
    pub x: PositionUnit,
    pub y: PositionUnit,
}

impl Position {
    pub fn empty() -> Self {
        Self {
            x: PositionUnit(0),
            y: PositionUnit(0),
        }
    }

    pub fn new(x: PositionUnit, y: PositionUnit) -> Self {
        Self { x, y }
    }

    pub fn from_pixels(x: PixelUnit, y: PixelUnit) -> Self {
        Self {
            x: x.into(),
            y: y.into(),
        }
    }

    pub fn from_tile(x: i32, y: i32) -> Self {
        Self {
            x: PositionUnit(x * 16000),
            y: PositionUnit(y * 16000),
        }
    }

    pub fn to_tile(&self) -> (u16, u16) {
        let x_tile = (self.x.0 / 16000) as u16;
        let y_tile = (self.y.0 / 16000) as u16;

        (x_tile, y_tile)
    }
}

impl Add<Velocity> for Position {
    type Output = Self;

    fn add(self, rhs: Velocity) -> Self::Output {
        let x = std::cmp::max(std::cmp::min(self.x.0 - rhs.x.0, MAX_POSITION), 0);
        let y = std::cmp::max(std::cmp::min(self.y.0 - rhs.y.0, MAX_POSITION), 0);

        Position::new(PositionUnit(x), PositionUnit(y))
    }
}

impl AddAssign<Velocity> for Position {
    fn add_assign(&mut self, rhs: Velocity) {
        *self = self.add(rhs);
    }
}

#[derive(Debug, Copy, Clone)]
pub struct Velocity {
    pub x: PositionUnit,
    pub y: PositionUnit,
}

impl Velocity {
    pub fn new(x: PositionUnit, y: PositionUnit) -> Self {
        Self { x, y }
    }

    pub fn clear(&mut self) {
        self.x = PositionUnit(0);
        self.y = PositionUnit(0);
    }
}

pub fn radians(degrees: f32) -> f32 {
    const DEGREES_TO_RADIANS: f32 = std::f32::consts::PI / 180.0f32;

    degrees * DEGREES_TO_RADIANS
}

pub fn degrees(radians: f32) -> f32 {
    const RADIANS_TO_DEGREES: f32 = 180.0f32 / std::f32::consts::PI;

    radians * RADIANS_TO_DEGREES
}

#[derive(Copy, Clone, Debug)]
pub struct Vector2i {
    pub x: i32,
    pub y: i32,
}

impl Vector2i {
    pub fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

impl Add for Vector2i {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self::Output {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
        }
    }
}

impl Sub for Vector2i {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self::Output {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub struct Rectangle {
    pub min: Position,
    pub max: Position,
}

impl Rectangle {
    pub fn new(min: Position, max: Position) -> Self {
        Self { min, max }
    }

    pub fn empty() -> Self {
        Self {
            min: Position::new(PositionUnit(0), PositionUnit(0)),
            max: Position::new(PositionUnit(0), PositionUnit(0)),
        }
    }

    pub fn invalid() -> Self {
        Self {
            min: Position::new(PositionUnit(i32::MAX), PositionUnit(i32::MAX)),
            max: Position::new(PositionUnit(i32::MIN), PositionUnit(i32::MIN)),
        }
    }

    pub fn from_radius(center: Position, radius: PixelUnit) -> Self {
        let min_x = center.x - radius.into();
        let min_y = center.y - radius.into();
        let max_x = center.x + radius.into();
        let max_y = center.y + radius.into();

        Self {
            min: Position::new(min_x, min_y),
            max: Position::new(max_x, max_y),
        }
    }
}
