use crate::clock::GameTick;
use crate::net::packet::{Packet, Serialize};

// 0x03
pub struct ReliableDataMessage {
    pub id: u32,
    pub data: Packet,
}

impl Serialize for ReliableDataMessage {
    fn serialize(&self) -> Packet {
        Packet::empty()
            .concat_u8(0x00)
            .concat_u8(0x03)
            .concat_u32(self.id)
            .concat_bytes(&self.data.data[..self.data.size])
    }
}

// 0x04
pub struct ReliableAckMessage {
    pub id: u32,
}

impl Serialize for ReliableAckMessage {
    fn serialize(&self) -> Packet {
        Packet::empty()
            .concat_u8(0x00)
            .concat_u8(0x04)
            .concat_u32(self.id)
    }
}

// 0x05
pub struct SyncRequestMessage {
    pub local_tick: u32,
    pub packets_sent: u32,
    pub packets_recv: u32,
}

impl SyncRequestMessage {
    pub fn new(local_tick: GameTick, packets_sent: u32, packets_recv: u32) -> Self {
        Self {
            local_tick: local_tick.value(),
            packets_sent,
            packets_recv,
        }
    }
}

impl Serialize for SyncRequestMessage {
    fn serialize(&self) -> Packet {
        Packet::empty()
            .concat_u8(0x00)
            .concat_u8(0x05)
            .concat_u32(self.local_tick)
            .concat_u32(self.packets_sent)
            .concat_u32(self.packets_recv)
    }
}

// 0x06
pub struct SyncResponseMessage {
    pub request_timestamp: u32,
    pub response_timestamp: u32,
}

impl Serialize for SyncResponseMessage {
    fn serialize(&self) -> Packet {
        Packet::empty()
            .concat_u8(0x00)
            .concat_u8(0x06)
            .concat_u32(self.request_timestamp)
            .concat_u32(self.response_timestamp)
    }
}

pub struct DisconnectMessage {}

impl Serialize for DisconnectMessage {
    fn serialize(&self) -> Packet {
        Packet::empty().concat_u8(0x00).concat_u8(0x07)
    }
}

// 0x08
pub struct SmallChunkBodyMessage {
    pub data: Packet,
}

// 0x09
pub struct SmallChunkTailMessage {
    pub data: Packet,
}

// 0x0A
pub struct HugeChunkMessage {
    pub total_size: u32,
    pub data: Packet,
}

impl Serialize for HugeChunkMessage {
    fn serialize(&self) -> Packet {
        Packet::empty()
            .concat_u8(0x00)
            .concat_u8(0x0A)
            .concat_u32(self.total_size)
            .concat_bytes(&self.data.data[..self.data.size])
    }
}

// 0x0B
pub struct HugeChunkCancelMessage {}
impl Serialize for HugeChunkCancelMessage {
    fn serialize(&self) -> Packet {
        Packet::empty().concat_u8(0x00).concat_u8(0x0B)
    }
}

// 0x0C
pub struct HugeChunkCancelAckMessage {}
impl Serialize for HugeChunkCancelAckMessage {
    fn serialize(&self) -> Packet {
        Packet::empty().concat_u8(0x00).concat_u8(0x0C)
    }
}
// 0x0E
pub struct ClusterMessage {
    pub data: Packet,
}

impl Serialize for ClusterMessage {
    fn serialize(&self) -> Packet {
        Packet::empty()
            .concat_u8(0x00)
            .concat_u8(0x0E)
            .concat_bytes(&self.data.data[..self.data.size])
    }
}
