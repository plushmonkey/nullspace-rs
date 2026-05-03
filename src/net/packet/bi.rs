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
pub struct ClockSyncRequestMessage {
    pub local_tick: u32,
    pub packets_sent: u32,
    pub packets_recv: u32,
}

impl ClockSyncRequestMessage {
    pub fn new(local_tick: GameTick, packets_sent: u32, packets_recv: u32) -> Self {
        Self {
            local_tick: local_tick.value(),
            packets_sent,
            packets_recv,
        }
    }
}

impl Serialize for ClockSyncRequestMessage {
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
pub struct ClockSyncResponseMessage {
    pub request_timestamp: u32,
    pub response_timestamp: u32,
}

impl Serialize for ClockSyncResponseMessage {
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

#[derive(Copy, Clone)]
pub struct ItemSet {
    pub shield_active: bool,
    pub super_active: bool,
    pub bursts: u8,
    pub repels: u8,
    pub thors: u8,
    pub bricks: u8,
    pub decoys: u8,
    pub rockets: u8,
    pub portals: u8,
}

impl ItemSet {
    pub fn empty() -> ItemSet {
        ItemSet {
            shield_active: false,
            super_active: false,
            bursts: 0,
            repels: 0,
            thors: 0,
            bricks: 0,
            decoys: 0,
            rockets: 0,
            portals: 0,
        }
    }

    pub fn parse(data: u32) -> ItemSet {
        ItemSet {
            shield_active: (data & 1) != 0,
            super_active: ((data >> 1) & 1) != 0,
            bursts: ((data >> 2) & 0x0F) as u8,
            repels: ((data >> 6) & 0x0F) as u8,
            thors: ((data >> 10) & 0x0F) as u8,
            bricks: ((data >> 14) & 0x0F) as u8,
            decoys: ((data >> 18) & 0x0F) as u8,
            rockets: ((data >> 22) & 0x0F) as u8,
            portals: ((data >> 26) & 0x0F) as u8,
        }
    }

    pub fn pack(&self) -> u32 {
        let portals = (self.portals as u32 & 0x0F) << 26;
        let rockets = (self.rockets as u32 & 0x0F) << 22;
        let decoys = (self.decoys as u32 & 0x0F) << 18;
        let bricks = (self.bricks as u32 & 0x0F) << 14;
        let thors = (self.thors as u32 & 0x0F) << 10;
        let repels = (self.repels as u32 & 0x0F) << 6;
        let bursts = (self.bursts as u32 & 0x0F) << 2;
        let super_active = (self.super_active as u32) << 1;
        let shield_active = (self.shield_active as u32) << 0;

        portals | rockets | decoys | bricks | thors | repels | bursts | super_active | shield_active
    }
}

#[derive(Copy, Clone)]
pub struct ExtraPositionData {
    pub energy: u16,
    pub s2c_latency: u16,
    pub flag_timer: u16,
    pub items: ItemSet,
}
