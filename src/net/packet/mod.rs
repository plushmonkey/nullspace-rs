use crate::clock::GameTick;
use crate::player::PlayerId;
use std::fmt;
use std::{ffi::FromBytesUntilNulError, str::Utf8Error};
use thiserror::Error;

pub mod bi;
pub mod c2s;
pub mod s2c;
pub mod sequencer;

pub const MAX_PACKET_SIZE: usize = 520;

#[derive(Error, Debug)]
pub enum PacketSendError {
    #[error("{0}")]
    IoError(#[from] std::io::Error),

    #[error("invalid packet send size")]
    InvalidPacketSize,

    #[error("reliable packet payload must not be empty")]
    EmptyReliablePacket,

    #[error("reliable packet payload too large")]
    OverflowReliablePacket,
}

#[derive(Error, Debug)]
pub enum PacketError {
    #[error("{0}")]
    IoError(#[from] std::io::Error),

    #[error("{0}")]
    StringError(#[from] Utf8Error),

    #[error("{0}")]
    TerminatedStringError(#[from] FromBytesUntilNulError),

    #[error("invalid core packet kind {0}")]
    InvalidCorePacketKind(u8),

    #[error("invalid game packet kind {0}")]
    InvalidGamePacketKind(u8),

    #[error("invalid core packet size for kind {0}")]
    InvalidCorePacketSize(u8),

    #[error("invalid game packet size for kind {0}")]
    InvalidGamePacketSize(u8),
}

#[derive(Copy, Clone)]
pub struct Packet {
    pub data: [u8; MAX_PACKET_SIZE],
    pub size: usize,
}

impl Packet {
    pub fn empty() -> Self {
        Self {
            data: [0; MAX_PACKET_SIZE],
            size: 0,
        }
    }

    pub fn new(message: &[u8]) -> Self {
        let size = message.len();
        let mut data = [0; MAX_PACKET_SIZE];

        data[..size].copy_from_slice(message);

        Self { data, size }
    }

    pub fn new_reliable(id: u32, message: &[u8]) -> Self {
        let size = message.len() + 6;
        let mut data = [0; MAX_PACKET_SIZE];

        data[0] = 0x00;
        data[1] = 0x03;
        data[2..6].copy_from_slice(&id.to_le_bytes());
        data[6..message.len() + 6].copy_from_slice(message);

        Self { data, size }
    }

    pub fn new_reliable_ack(id: u32) -> Self {
        let size = 6;
        let mut data = [0; MAX_PACKET_SIZE];

        data[0] = 0x00;
        data[1] = 0x04;
        data[2..6].copy_from_slice(&id.to_le_bytes());

        Self { data, size }
    }

    pub fn new_sync_response(local_timestamp: GameTick, recv_timestamp: GameTick) -> Self {
        let size = 10;
        let mut data = [0; MAX_PACKET_SIZE];

        data[0] = 0x00;
        data[1] = 0x06;
        data[2..6].copy_from_slice(&recv_timestamp.value().to_le_bytes());
        data[6..10].copy_from_slice(&local_timestamp.value().to_le_bytes());

        Self { data, size }
    }

    pub fn data(&self) -> &[u8] {
        &self.data[..self.size]
    }

    pub fn concat_u8(self, val: u8) -> Self {
        let mut result = Self {
            data: self.data,
            size: self.size + 1,
        };

        result.data[self.size] = val;

        result
    }

    pub fn concat_u16(self, val: u16) -> Self {
        let mut result = Self {
            data: self.data,
            size: self.size + 2,
        };

        result.data[self.size..self.size + 2].copy_from_slice(&val.to_le_bytes());
        result
    }

    pub fn concat_player_id(self, val: PlayerId) -> Self {
        self.concat_u16(val.value)
    }

    pub fn concat_u32(self, val: u32) -> Self {
        let mut result = Self {
            data: self.data,
            size: self.size + 4,
        };
        result.data[self.size..self.size + 4].copy_from_slice(&val.to_le_bytes());
        result
    }

    pub fn concat_i8(self, val: i8) -> Self {
        let mut result = Self {
            data: self.data,
            size: self.size + 1,
        };

        result.data[self.size] = val as u8;

        result
    }

    pub fn concat_i16(self, val: i16) -> Self {
        let mut result = Self {
            data: self.data,
            size: self.size + 2,
        };

        result.data[self.size..self.size + 2].copy_from_slice(&val.to_le_bytes());
        result
    }

    pub fn concat_i32(self, val: i32) -> Self {
        let mut result = Self {
            data: self.data,
            size: self.size + 4,
        };
        result.data[self.size..self.size + 4].copy_from_slice(&val.to_le_bytes());
        result
    }

    pub fn concat_str(self, val: &str) -> Self {
        let mut result = Self {
            data: self.data,
            size: self.size + val.len() + 1,
        };

        result.data[self.size..self.size + val.len()].copy_from_slice(val.as_bytes());
        result.data[result.size] = 0;
        result
    }

    pub fn concat_bytes(self, val: &[u8]) -> Self {
        let mut result = Self {
            data: self.data,
            size: self.size + val.len(),
        };

        result.data[self.size..self.size + val.len()].copy_from_slice(val);
        result
    }

    pub fn write_u8(&mut self, val: u8) {
        self.data[self.size] = val;
        self.size += 1;
    }

    pub fn write_u16(&mut self, val: u16) {
        self.data[self.size..self.size + 2].copy_from_slice(&val.to_le_bytes());
        self.size += 2;
    }

    pub fn write_player_id(&mut self, val: PlayerId) {
        self.write_u16(val.value)
    }

    pub fn write_u32(&mut self, val: u32) {
        self.data[self.size..self.size + 4].copy_from_slice(&val.to_le_bytes());
        self.size += 4;
    }

    pub fn write_i8(&mut self, val: i8) {
        self.data[self.size] = val as u8;
        self.size += 1;
    }

    pub fn write_i16(&mut self, val: i16) {
        self.data[self.size..self.size + 2].copy_from_slice(&val.to_le_bytes());
        self.size += 2;
    }

    pub fn write_i32(&mut self, val: i32) {
        self.data[self.size..self.size + 4].copy_from_slice(&val.to_le_bytes());
        self.size += 4;
    }

    pub fn write_str(&mut self, val: &str) {
        self.data[self.size..self.size + val.len()].copy_from_slice(val.as_bytes());
        self.size += val.len() + 1;
        self.data[self.size] = 0;
    }

    pub fn write_bytes(&mut self, val: &[u8]) {
        self.data[self.size..self.size + val.len()].copy_from_slice(val);
        self.size += val.len();
    }

    pub fn write_fixed_str(&mut self, val: &str, size: usize) {
        let write_bytes = if val.len() > size { size } else { val.len() };

        self.data[self.size..self.size + write_bytes]
            .copy_from_slice(&val.as_bytes()[..write_bytes]);
        self.size += write_bytes;

        let remaining = size - write_bytes;

        for i in 0..remaining {
            self.data[self.size + i] = 0;
        }

        self.size += remaining;
    }

    pub fn remaining(&self) -> usize {
        MAX_PACKET_SIZE - self.size
    }
}

impl fmt::Debug for Packet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Packet {{ data={:?} size={} }}",
            &self.data[..self.size],
            self.size
        )
    }
}

pub trait Serialize {
    fn serialize(&self) -> Packet;
}
