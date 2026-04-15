use std::ops::{Add, Sub};

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

impl Add<Position> for Position {
    type Output = Self;

    fn add(self, rhs: Position) -> Self::Output {
        let x = std::cmp::max(std::cmp::min(self.x.0 + rhs.x.0, MAX_POSITION), 0);
        let y = std::cmp::max(std::cmp::min(self.y.0 + rhs.y.0, MAX_POSITION), 0);

        Position::new(PositionUnit(x), PositionUnit(y))
    }
}

impl Sub<Position> for Position {
    type Output = Self;

    fn sub(self, rhs: Position) -> Self::Output {
        let x = std::cmp::max(std::cmp::min(self.x.0 - rhs.x.0, MAX_POSITION), 0);
        let y = std::cmp::max(std::cmp::min(self.y.0 - rhs.y.0, MAX_POSITION), 0);

        Position::new(PositionUnit(x), PositionUnit(y))
    }
}

impl Into<glam::Vec2> for Position {
    fn into(self) -> glam::Vec2 {
        let x_pixels = self.x.0 / 1000;
        let x_tile = x_pixels as f32 / 16.0f32;

        let y_pixels = self.y.0 / 1000;
        let y_tile = y_pixels as f32 / 16.0f32;

        glam::Vec2::new(x_tile, y_tile)
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

    pub fn from_radius(center: Position, radius: PositionUnit) -> Self {
        let min_x = center.x - radius;
        let min_y = center.y - radius;
        let max_x = center.x + radius;
        let max_y = center.y + radius;

        Self {
            min: Position::new(min_x, min_y),
            max: Position::new(max_x, max_y),
        }
    }

    pub fn intersects(&self, other: &Rectangle) -> bool {
        let first_min_x = self.min.x.0;
        let first_min_y = self.min.y.0;
        let first_max_x = self.max.x.0;
        let first_max_y = self.max.y.0;

        let second_min_x = other.min.x.0;
        let second_min_y = other.min.y.0;
        let second_max_x = other.max.x.0;
        let second_max_y = other.max.y.0;

        first_max_x > second_min_x
            && first_min_x < second_max_x
            && first_max_y > second_min_y
            && first_min_y < second_max_y
    }

    pub fn contains(&self, other: Position) -> bool {
        let ox = other.x.0;
        let oy = other.y.0;

        ox > self.min.x.0 && ox < self.max.x.0 && oy > self.min.y.0 && oy < self.max.y.0
    }
}

pub fn rotate_vec2(vec: glam::Vec2, rads: f32) -> glam::Vec2 {
    let cos_a = f32::cos(rads);
    let sin_a = f32::sin(rads);

    glam::Vec2::new(cos_a * vec.x - sin_a * vec.y, sin_a * vec.x + cos_a * vec.y)
}
