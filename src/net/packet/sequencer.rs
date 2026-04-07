use crate::clock::GameTick;
use crate::net::packet::PacketError;
use crate::net::packet::bi::{ClusterMessage, HugeChunkMessage};
use crate::net::packet::s2c::ServerMessage;
use crate::net::packet::{MAX_PACKET_SIZE, Packet};
use std::collections::VecDeque;

pub struct ReliableMessage {
    pub id: u32,
    pub timestamp: GameTick,
    pub size: usize,
    pub message: [u8; MAX_PACKET_SIZE],
}

impl ReliableMessage {
    pub fn new(timestamp: GameTick, id: u32, message: &[u8]) -> Self {
        let len = message.len();

        let mut new_message: [u8; MAX_PACKET_SIZE] = [0; MAX_PACKET_SIZE];
        new_message[..len].copy_from_slice(&message);

        Self {
            id,
            timestamp,
            size: len,
            message: new_message,
        }
    }
}

pub struct PacketSequencer {
    pub next_process_id: u32,
    pub next_reliable_gen_id: u32,
    pub reliable_sent: Vec<ReliableMessage>,
    pub reliable_queue: Vec<ReliableMessage>,
    // This queue stores clustered packets and coalesced packets such as small and huge chunks.
    // A deque is used to reduce the amount of work for processing the queue in order.
    pub process_queue: VecDeque<Vec<u8>>,
    pub chunk_data: Vec<u8>,

    pub tick: GameTick,
}

impl PacketSequencer {
    pub fn new() -> Self {
        Self {
            next_process_id: 0,
            next_reliable_gen_id: 0,
            reliable_sent: Vec::new(),
            reliable_queue: Vec::new(),
            process_queue: VecDeque::new(),
            chunk_data: Vec::new(),
            tick: GameTick::empty(),
        }
    }

    pub fn tick(&mut self, tick: GameTick) -> Option<Packet> {
        const RESEND_DELAY: i32 = 300;

        self.tick = tick;

        let resend_timestamp = tick - RESEND_DELAY;

        // Find the first message that needs to be resent.
        if let Some(rel) = self
            .reliable_sent
            .iter_mut()
            .find(|msg| msg.timestamp.le(&resend_timestamp))
        {
            rel.timestamp = tick;
            return Some(Packet::new(&rel.message[..rel.size]));
        }
        None
    }

    pub fn push_reliable_sent(&mut self, id: u32, message: &[u8]) {
        let reliable = ReliableMessage::new(self.tick, id, message);
        self.reliable_sent.push(reliable);
    }

    pub fn pop_process_queue(&mut self) -> Result<Option<ServerMessage>, PacketError> {
        // Fully process the queue before we process reliable messages.
        // This ensures clustered and coalesced messages are processed in order.
        if let Some(data) = self.process_queue.pop_front() {
            return ServerMessage::parse(&data[..]);
        }

        if let Some(index) = self
            .reliable_queue
            .iter()
            .position(|msg| msg.id == self.next_process_id)
        {
            self.next_process_id = self.next_process_id.wrapping_add(1);

            let rel = self.reliable_queue.swap_remove(index);
            return ServerMessage::parse(&rel.message[..rel.size]);
        }

        Ok(None)
    }

    pub fn handle_reliable_message(&mut self, id: u32, packet: &Packet) {
        let reliable_message = ReliableMessage::new(self.tick, id, &packet.data[..packet.size]);

        self.reliable_queue.push(reliable_message);
    }

    pub fn handle_ack(&mut self, id: u32) {
        if let Some(index) = self.reliable_sent.iter().position(|msg| msg.id == id) {
            self.reliable_sent.swap_remove(index);
        }
    }

    pub fn handle_cluster(&mut self, cluster: &ClusterMessage) {
        let mut data = &cluster.data.data[..cluster.data.size];

        while !data.is_empty() {
            let size = data[0] as usize;
            let current_data = &data[1..size + 1];

            let mut process_data = Vec::new();
            process_data.extend(current_data.iter());

            self.process_queue.push_back(process_data);

            data = &data[size + 1..];
        }
    }

    pub fn handle_small_chunk_body(&mut self, packet: &Packet) {
        let data = &packet.data[..packet.size];
        self.chunk_data.extend(data.iter());
    }

    pub fn handle_small_chunk_tail(&mut self, packet: &Packet) {
        let data = &packet.data[..packet.size];
        self.chunk_data.extend(data.iter());
        self.process_queue.push_back(self.chunk_data.clone());
        self.chunk_data.clear();
    }

    pub fn handle_huge_chunk(&mut self, chunk: &HugeChunkMessage) {
        let data = &chunk.data.data[..chunk.data.size];
        self.chunk_data.extend(data.iter());

        println!("Downloading {}/{}", self.chunk_data.len(), chunk.total_size);

        if self.chunk_data.len() >= chunk.total_size as usize {
            self.process_queue.push_back(self.chunk_data.clone());
            self.chunk_data.clear();
        }
    }

    pub fn handle_huge_chunk_cancel(&mut self) {
        self.chunk_data.clear();
    }

    pub fn increment_id(&mut self) {
        self.next_reliable_gen_id = self.next_reliable_gen_id.wrapping_add(1);
    }
}
