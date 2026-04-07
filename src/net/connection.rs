use crate::clock::*;
use crate::net::crypt::VieEncrypt;
use crate::net::packet::PacketError;
use crate::net::packet::PacketSendError;
use crate::net::packet::bi::HugeChunkCancelAckMessage;
use crate::net::packet::bi::ReliableDataMessage;
use crate::net::packet::bi::SyncResponseMessage;
use crate::net::packet::c2s::EncryptionRequestMessage;
use crate::net::packet::s2c::*;
use crate::net::packet::sequencer::*;
use crate::net::packet::{MAX_PACKET_SIZE, Packet, Serialize};
use crate::player::PlayerId;
use thiserror::Error;

use std::net::AddrParseError;
use std::{
    net::{IpAddr, SocketAddr, UdpSocket},
    str::FromStr,
};

pub enum ConnectionState {
    EncryptionHandshake,
    Authentication,
    Registering,
    ArenaLogin,
    MapDownload,
    Playing,
    Disconnected,
}

#[derive(Error, Debug)]
pub enum ConnectionError {
    #[error("{0}")]
    IoError(#[from] std::io::Error),

    #[error("{0}")]
    AddressParseError(#[from] AddrParseError),

    #[error("{0}")]
    SendError(#[from] PacketSendError),

    #[error("{0}")]
    RecvError(#[from] PacketError),
}

#[derive(Copy, Clone, Debug)]
struct ClockSyncResult {
    pub ping: i32,
    pub time_diff: i32,
}

struct ClockSyncHistory {
    results: [ClockSyncResult; 16],
    index: usize,
}

impl ClockSyncHistory {
    pub fn new() -> Self {
        Self {
            results: [ClockSyncResult {
                ping: 0,
                time_diff: 0,
            }; 16],
            index: 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.index == 0
    }

    pub fn insert(&mut self, ping: i32, time_diff: i32) {
        self.results[self.index % self.results.len()] = ClockSyncResult { ping, time_diff };
        self.index = self.index.wrapping_add(1);
    }

    pub fn get_average_time_diff(&self) -> i32 {
        let max_index = self.index.min(self.results.len());

        if max_index == 0 {
            return 0;
        }

        let mut total_diff: i64 = 0;

        for i in 0..max_index {
            total_diff += self.results[i].time_diff as i64;
        }

        (total_diff / max_index as i64) as i32
    }

    pub fn get_average_ping(&self) -> i32 {
        let max_index = self.index.min(self.results.len());

        if max_index == 0 {
            return 0;
        }

        let mut total_ping: i64 = 0;

        for i in 0..max_index {
            total_ping += self.results[i].ping as i64;
        }

        (total_ping / max_index as i64) as i32
    }
}

pub struct Connection {
    pub remote_addr: SocketAddr,
    pub socket: UdpSocket,
    pub state: ConnectionState,
    sequencer: PacketSequencer,
    pub player_id: PlayerId,
    pub crypt: VieEncrypt,

    sync_history: ClockSyncHistory,
    pub tick_diff: i32,
    pub current_tick: GameTick,
    pub ping: i32,
}

impl Connection {
    pub fn new(remote_ip: &str, remote_port: u16) -> Result<Self, ConnectionError> {
        let remote_addr = std::net::Ipv4Addr::from_str(remote_ip)?;
        let remote_addr = SocketAddr::new(IpAddr::V4(remote_addr), remote_port);
        let socket = UdpSocket::bind("0.0.0.0:0")?;

        socket.set_nonblocking(true)?;

        let client_key = VieEncrypt::generate_key();

        let mut result = Self {
            remote_addr,
            socket,
            state: ConnectionState::Disconnected,
            sequencer: PacketSequencer::new(),
            player_id: PlayerId::invalid(),
            crypt: VieEncrypt::new(client_key),
            sync_history: ClockSyncHistory::new(),
            tick_diff: 0,
            current_tick: GameTick::empty(),
            ping: 0,
        };

        let encrypt_request = EncryptionRequestMessage::new(client_key);
        result.state = ConnectionState::EncryptionHandshake;
        result.send(&encrypt_request)?;

        Ok(result)
    }

    pub fn get_game_tick(&self) -> GameTick {
        self.current_tick
    }

    // This gets the local timestamp for processed ticks by basing it off of the server tick.
    // This is not the immediate local tick. Get that from GameTick::now(0)
    pub fn get_local_tick(&self) -> GameTick {
        GameTick::new(self.current_tick.value(), -self.tick_diff)
    }

    pub fn send<T>(&mut self, message: &T) -> Result<(), PacketSendError>
    where
        T: Serialize,
    {
        self.send_packet(&message.serialize())
    }

    pub fn send_reliable<T>(&mut self, message: &T) -> Result<(), PacketSendError>
    where
        T: Serialize,
    {
        self.send_reliable_packet(&message.serialize())
    }

    pub fn send_packet(&mut self, packet: &Packet) -> Result<(), PacketSendError> {
        if packet.size == 0 {
            return Err(PacketSendError::InvalidPacketSize);
        }

        if packet.size > MAX_PACKET_SIZE {
            return self.send_reliable_packet(packet);
        }

        let buf = packet.data();
        let mut encrypted = Packet::empty();

        self.crypt.encrypt(buf, &mut encrypted.data[..buf.len()]);

        //println!("Sending {:02x?}", buf);
        //println!("Sending {:02x?}", &encrypted.data[..buf.len()]);

        self.socket
            .send_to(&encrypted.data[..buf.len()], &self.remote_addr)?;

        Ok(())
    }

    pub fn send_reliable_data(&mut self, data: &[u8]) -> Result<(), PacketSendError> {
        if data.len() == 0 {
            return Err(PacketSendError::EmptyReliablePacket);
        }

        const RELIABLE_HEADER_SIZE: usize = 6;
        if data.len() + RELIABLE_HEADER_SIZE > MAX_PACKET_SIZE {
            const CHUNK_HEADER_SIZE: usize = 2;
            let mut data = data;

            // Break our packet up into subpackets that are sent reliably as chunked (0x08/0x09) data.
            while !data.is_empty() {
                let mut subpacket = Packet::empty();

                subpacket.size = data.len() + CHUNK_HEADER_SIZE;
                if subpacket.size > MAX_PACKET_SIZE - RELIABLE_HEADER_SIZE {
                    subpacket.size = MAX_PACKET_SIZE - RELIABLE_HEADER_SIZE;
                }

                let payload_size = subpacket.size - CHUNK_HEADER_SIZE;

                subpacket.data[CHUNK_HEADER_SIZE..subpacket.size]
                    .copy_from_slice(&data[..payload_size]);

                data = &data[payload_size..];

                subpacket.data[0] = 0x00;
                subpacket.data[1] = if data.is_empty() { 0x09 } else { 0x08 };

                if let Err(e) = self.send_reliable_packet(&subpacket) {
                    println!("Err: {}", e);
                }
            }

            return Ok(());
        }

        self.send_reliable_packet(&Packet::new(data))
    }

    pub fn send_reliable_packet(&mut self, packet: &Packet) -> Result<(), PacketSendError> {
        if packet.size == 0 {
            return Err(PacketSendError::EmptyReliablePacket);
        }

        const RELIABLE_HEADER_SIZE: usize = 6;
        if packet.size + RELIABLE_HEADER_SIZE > MAX_PACKET_SIZE {
            return Err(PacketSendError::OverflowReliablePacket);
        }

        let id = self.sequencer.next_reliable_gen_id;
        self.sequencer.increment_id();

        let reliable = ReliableDataMessage {
            id,
            data: packet.clone(),
        };

        let packet = reliable.serialize();
        let buf = packet.data();

        self.sequencer.push_reliable_sent(id, buf);

        self.send_packet(&packet)
    }

    pub fn receive_message(&mut self) -> Result<Option<ServerMessage>, ConnectionError> {
        if let Some(message) = self.sequencer.tick(self.current_tick) {
            self.send_packet(&message)?;
        }

        let packet = self.recv_packet()?;

        // If we received a packet and it got processed into a complete message, return it.
        if let Some(packet) = packet {
            let result = ServerMessage::parse(&packet.data[..packet.size])?;
            if let Some(message) = &result {
                self.process_packet(&message);
            }

            return Ok(result);
        }

        // Grab the next reliable message / cluster message off of the queue if possible.
        let sequence_message = self.sequencer.pop_process_queue()?;

        if let Some(message) = &sequence_message {
            self.process_packet(&message);
            return Ok(sequence_message);
        }

        Ok(None)
    }

    fn process_packet(&mut self, message: &ServerMessage) {
        match message {
            ServerMessage::Core(kind) => match kind {
                CoreServerMessage::EncryptionResponse(response) => {
                    println!("Initializing encryption with key {}", response.key);
                    if !self.crypt.initialize(response.key) {
                        println!("Failed to initialize vie encryption.");
                    }
                }
                CoreServerMessage::ReliableAck(ack) => {
                    self.sequencer.handle_ack(ack.id);
                    // println!("Got reliable ack {}", ack.id);
                }
                CoreServerMessage::ReliableData(rel) => {
                    self.sequencer.handle_reliable_message(rel.id, &rel.data);
                    // println!("Got reliable data {:?}", &rel.data.data[..rel.data.size]);
                    let ack = Packet::empty()
                        .concat_u8(0x00)
                        .concat_u8(0x04)
                        .concat_u32(rel.id);
                    if let Err(e) = self.send_packet(&ack) {
                        println!("Error: {}", e);
                    }
                }
                CoreServerMessage::SyncRequest(sync) => {
                    let response = SyncResponseMessage {
                        request_timestamp: sync.local_tick,
                        response_timestamp: GameTick::now(0).value(),
                    };

                    if let Err(e) = self.send(&response) {
                        println!("Error: {}", e);
                    }
                }
                CoreServerMessage::SyncResponse(sync) => {
                    let server_timestamp = sync.response_timestamp as i32;
                    let current_timestamp = GameTick::now(0).value() as i32;
                    let rtt = current_timestamp - sync.request_timestamp as i32;
                    let current_ping = (rtt / 2) * 10;

                    println!(
                        "ServerTimestamp: {}, CurrentTimestamp: {}, rtt: {}",
                        server_timestamp, current_timestamp, rtt
                    );

                    let current_time_diff = ((rtt * 3) / 5) + server_timestamp - current_timestamp;

                    if self.sync_history.is_empty() {
                        self.current_tick = GameTick::now(current_time_diff);
                    }

                    self.sync_history.insert(current_ping, current_time_diff);

                    self.tick_diff = self.sync_history.get_average_time_diff();
                    self.ping = self.sync_history.get_average_ping();
                }
                CoreServerMessage::Disconnect => {
                    println!("Got disconnect order.");
                    self.state = ConnectionState::Disconnected;
                }
                CoreServerMessage::SmallChunkBody(chunk) => {
                    self.sequencer.handle_small_chunk_body(&chunk.data);
                }
                CoreServerMessage::SmallChunkTail(tail) => {
                    self.sequencer.handle_small_chunk_tail(&tail.data);
                }
                CoreServerMessage::HugeChunk(chunk) => {
                    self.sequencer.handle_huge_chunk(chunk);
                }
                CoreServerMessage::HugeChunkCancel => {
                    self.sequencer.handle_huge_chunk_cancel();

                    let cancel = HugeChunkCancelAckMessage {};
                    if let Err(e) = self.send(&cancel) {
                        println!("Error: {}", e);
                    }
                }
                CoreServerMessage::HugeChunkCancelAck => {
                    //
                }
                CoreServerMessage::Cluster(cluster) => {
                    self.sequencer.handle_cluster(cluster);
                }
            },
            ServerMessage::Game(kind) => match kind {
                GameServerMessage::PlayerId(message) => {
                    self.player_id = message.id;
                }
                _ => {}
            },
        }
    }

    fn recv_packet(&self) -> Result<Option<Packet>, std::io::Error> {
        let mut packet = Packet::empty();

        let (size, _) = match self.socket.recv_from(&mut packet.data[..]) {
            Ok(r) => r,
            Err(e) => {
                if e.kind() == std::io::ErrorKind::WouldBlock {
                    return Ok(None);
                }

                return Err(e);
            }
        };

        packet.size = size;

        //println!("RecvRaw: {:02x?}", &packet.data[..size]);

        self.crypt.decrypt(&mut packet.data[..packet.size]);

        //println!("Recv: {:02x?}", &packet.data[..size]);

        Ok(Some(packet))
    }
}
