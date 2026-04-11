use std::cmp::{Ord, Ordering, PartialOrd};
use std::convert::From;
use std::ops::{Add, Sub};

#[derive(PartialEq, Eq, Copy, Clone)]
pub struct GameTick {
    value: u32,
}

impl GameTick {
    pub fn now(offset: i32) -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        let tick: u128 = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("Now must be later than unix epoch")
            .as_millis();
        #[cfg(target_arch = "wasm32")]
        let tick = web_sys::window().unwrap().performance().unwrap().now() as u128;

        let tick = (tick / 10) as u32;

        Self::new(tick, offset)
    }

    pub fn new(value: u32, offset: i32) -> Self {
        let v = value as i32;
        let value = v.wrapping_add(offset) as u32;

        Self {
            value: value & 0x7FFFFFFF,
        }
    }

    pub fn empty() -> Self {
        Self { value: 0 }
    }

    pub fn from_mini(now: Self, value: u16) -> Self {
        let now_bottom = now.value() as i16;
        let delta = now_bottom.wrapping_sub(value as i16) as i32;
        let combined = (now.value() as i32).wrapping_add(delta);

        Self {
            value: combined as u32,
        }
    }

    pub fn from_batched(now: Self, value: u16) -> Self {
        let now_bottom = (now.value() & 0x3FF) as i16;
        let delta = now_bottom.wrapping_sub((value & 0x3FF) as i16) as i32;
        let combined = (now.value() as i32).wrapping_add(delta);

        Self {
            value: combined as u32,
        }
    }

    pub fn value(&self) -> u32 {
        self.value
    }

    pub fn diff(&self, other: &Self) -> i32 {
        let a_value = (self.value << 1) as i32;
        let b_value = (other.value << 1) as i32;

        (a_value.wrapping_sub(b_value)) >> 1
    }
}

impl PartialOrd for GameTick {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        let a_value = (self.value << 1) as i32;
        let b_value = (other.value << 1) as i32;

        a_value.partial_cmp(&b_value)
    }

    fn lt(&self, other: &Self) -> bool {
        self.diff(other) < 0
    }

    fn le(&self, other: &Self) -> bool {
        self.diff(other) <= 0
    }

    fn gt(&self, other: &Self) -> bool {
        self.diff(other) > 0
    }

    fn ge(&self, other: &Self) -> bool {
        self.diff(other) >= 0
    }
}

impl Ord for GameTick {
    fn cmp(&self, other: &Self) -> Ordering {
        let a_value = (self.value << 1) as i32;
        let b_value = (other.value << 1) as i32;

        a_value.cmp(&b_value)
    }

    fn max(self, other: Self) -> Self
    where
        Self: Sized,
    {
        if self.gt(&other) { self } else { other }
    }

    fn min(self, other: Self) -> Self
    where
        Self: Sized,
    {
        if self.lt(&other) { self } else { other }
    }

    fn clamp(self, min: Self, max: Self) -> Self
    where
        Self: Sized,
    {
        let mut result = Self { value: self.value };

        if result.lt(&min) {
            result.value = min.value;
        }

        if result.gt(&max) {
            result.value = max.value;
        }

        result
    }
}

impl From<u32> for GameTick {
    fn from(value: u32) -> Self {
        Self::new(value, 0)
    }
}

impl Add<i32> for GameTick {
    type Output = Self;

    fn add(self, rhs: i32) -> Self::Output {
        let v = self.value() as i32;
        let new_v = v.wrapping_add(rhs) as u32;

        Self::new(new_v, 0)
    }
}

impl Sub for GameTick {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        let v = self.value() as i32;
        let new_v = v.wrapping_sub(rhs.value() as i32) as u32;

        Self::new(new_v, 0)
    }
}

impl Sub<i32> for GameTick {
    type Output = Self;

    fn sub(self, rhs: i32) -> Self::Output {
        let v = self.value() as i32;
        let new_v = v.wrapping_sub(rhs) as u32;

        Self::new(new_v, 0)
    }
}
