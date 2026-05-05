use crate::clock::*;
use crate::net::crypt::VieEncrypt;
use crate::net::packet::PacketError;
use crate::net::packet::PacketSendError;
use crate::net::packet::bi::ClockSyncRequestMessage;
use crate::net::packet::bi::ClockSyncResponseMessage;
use crate::net::packet::bi::HugeChunkCancelAckMessage;
use crate::net::packet::bi::ReliableDataMessage;
use crate::net::packet::c2s::EncryptionRequestMessage;
use crate::net::packet::s2c::*;
use crate::net::packet::sequencer::*;
use crate::net::packet::{MAX_PACKET_SIZE, Packet, Serialize};
use crate::net::udp_socket::UdpSocket;
use crate::player::PlayerId;
use thiserror::Error;

use std::net::AddrParseError;

#[cfg(target_arch = "wasm32")]
use crate::net::webtransport_socket::WebTransportSocket;

#[derive(Copy, Clone, PartialEq)]
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

pub struct ClockSyncHistory {
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

    pub fn get_low_ping(&self) -> i32 {
        let max_index = self.index.min(self.results.len());
        let mut lowest_ping = 0;

        for i in 0..max_index {
            if self.results[i].ping < lowest_ping {
                lowest_ping = self.results[i].ping as i32;
            }
        }

        lowest_ping
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

    pub fn get_high_ping(&self) -> i32 {
        let max_index = self.index.min(self.results.len());
        let mut highest_ping = 0;

        for i in 0..max_index {
            if self.results[i].ping > highest_ping {
                highest_ping = self.results[i].ping as i32;
            }
        }

        highest_ping
    }
}

pub enum SocketKind {
    Udp(UdpSocket),
    #[cfg(target_arch = "wasm32")]
    WebTransport(WebTransportSocket),
}

pub struct Connection {
    pub socket: SocketKind,
    pub state: ConnectionState,
    sequencer: PacketSequencer,
    pub player_id: PlayerId,
    pub player_name: String,
    pub crypt: VieEncrypt,

    pub sync_history: ClockSyncHistory,
    pub tick_diff: i32,
    pub current_tick: GameTick,

    pub last_sync_req: GameTick,

    pub ping: i32,
    pub weapons_recv: u32,
    pub packets_sent: u32,
    pub packets_recv: u32,

    pub send_extra_position_info: bool,
}

impl Connection {
    pub fn new(socket: SocketKind) -> Result<Self, ConnectionError> {
        let client_key = VieEncrypt::generate_key();

        let mut result = Self {
            socket,
            state: ConnectionState::Disconnected,
            sequencer: PacketSequencer::new(),
            player_id: PlayerId::invalid(),
            player_name: String::new(),
            crypt: VieEncrypt::new(client_key),
            sync_history: ClockSyncHistory::new(),
            tick_diff: 0,
            current_tick: GameTick::empty(),
            last_sync_req: GameTick::empty(),
            ping: 0,
            weapons_recv: 0,
            packets_sent: 0,
            packets_recv: 0,
            send_extra_position_info: false,
        };

        let encrypt_request = EncryptionRequestMessage::new(client_key);
        result.state = ConnectionState::EncryptionHandshake;
        result.send(&encrypt_request)?;

        Ok(result)
    }

    pub fn tick(&mut self) {
        self.current_tick = self.current_tick + 1;

        if self.current_tick > self.last_sync_req + 500 {
            let sync_request = ClockSyncRequestMessage::new(
                GameTick::now(0),
                self.packets_sent,
                self.packets_recv,
            );

            if let Err(e) = self.send(&sync_request) {
                log::error!("{e}");
            }

            self.last_sync_req = self.current_tick;
        }
    }

    pub fn get_game_tick(&self) -> GameTick {
        self.current_tick
    }

    // This calculates the actual game tick on the server at the moment of calling.
    pub fn get_current_server_tick(&self) -> GameTick {
        GameTick::now(self.tick_diff)
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
        log::trace!("Send {:?}", &packet.data[..packet.size]);

        let mut encrypted = Packet::empty();

        self.crypt.encrypt(buf, &mut encrypted.data[..buf.len()]);

        self.packets_sent = self.packets_sent.wrapping_add(1);

        match &self.socket {
            SocketKind::Udp(socket) => {
                socket.send(&encrypted.data[..buf.len()])?;
            }
            #[cfg(target_arch = "wasm32")]
            SocketKind::WebTransport(socket) => {
                socket.send(&encrypted.data[..buf.len()])?;
            }
        }

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
                    log::error!("Err: {}", e);
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
            log::trace!("Recv {:?}", &packet.data[..packet.size]);

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
                    log::trace!("Initializing encryption with key {}", response.key);
                    if !self.crypt.initialize(response.key) {
                        log::error!("Failed to initialize vie encryption.");
                    }
                }
                CoreServerMessage::ReliableAck(ack) => {
                    self.sequencer.handle_ack(ack.id);
                }
                CoreServerMessage::ReliableData(rel) => {
                    self.sequencer.handle_reliable_message(rel.id, &rel.data);

                    let ack = Packet::empty()
                        .concat_u8(0x00)
                        .concat_u8(0x04)
                        .concat_u32(rel.id);
                    if let Err(e) = self.send_packet(&ack) {
                        log::error!("Error: {}", e);
                    }
                }
                CoreServerMessage::ClockSyncRequest(sync) => {
                    let response = ClockSyncResponseMessage {
                        request_timestamp: sync.local_tick,
                        response_timestamp: GameTick::now(0).value(),
                    };

                    if let Err(e) = self.send(&response) {
                        log::error!("Error: {}", e);
                    }
                }
                CoreServerMessage::ClockSyncResponse(sync) => {
                    let server_timestamp = sync.response_timestamp as i32;
                    let current_timestamp = GameTick::now(0).value() as i32;
                    let rtt = current_timestamp - sync.request_timestamp as i32;
                    let current_ping = (rtt / 2) * 10;

                    log::trace!(
                        "ServerTimestamp: {}, CurrentTimestamp: {}, rtt: {}",
                        server_timestamp,
                        current_timestamp,
                        rtt
                    );

                    let current_time_diff =
                        (rtt / 2 + server_timestamp).wrapping_sub(current_timestamp);

                    let first_sync = self.sync_history.is_empty();

                    self.sync_history.insert(current_ping, current_time_diff);
                    let new_tick_diff = self.sync_history.get_average_time_diff();

                    if first_sync {
                        self.current_tick = GameTick::now(current_time_diff);
                    }

                    self.tick_diff = new_tick_diff;
                    self.ping = self.sync_history.get_average_ping();
                }
                CoreServerMessage::Disconnect => {
                    log::warn!("Got disconnect order.");
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
                        log::error!("Error: {}", e);
                    }
                }
                CoreServerMessage::HugeChunkCancelAck => {
                    self.sequencer.awaiting_huge_cancel = false;
                    self.sequencer.huge_chunk_data.clear();
                }
                CoreServerMessage::Cluster(cluster) => {
                    self.sequencer.handle_cluster(cluster);
                }
            },
            ServerMessage::Game(kind) => match kind {
                GameServerMessage::PlayerId(message) => {
                    self.player_id = message.id;
                }
                GameServerMessage::PlayerEntering(entering) => {
                    for player in &entering.players {
                        if player.player_id == self.player_id {
                            self.player_name = player.name.clone();
                        }
                    }
                }
                GameServerMessage::LargePosition(message) => {
                    if message.weapon != 0 {
                        self.weapons_recv = self.weapons_recv.wrapping_add(1);
                    }
                }
                GameServerMessage::SpectateData(message) => match message {
                    SpectateDataMessage::ExtraPositionInfo(extra_info) => {
                        self.send_extra_position_info = *extra_info;
                    }
                    _ => {}
                },
                GameServerMessage::MapInformation(_) => {
                    self.cancel_downloads();
                }
                _ => {}
            },
        }
    }

    pub fn cancel_downloads(&mut self) {
        if self.state == ConnectionState::MapDownload || !self.sequencer.huge_chunk_data.is_empty()
        {
            let message = crate::net::packet::bi::HugeChunkCancelMessage {};
            self.sequencer.huge_chunk_data.clear();
            self.sequencer.awaiting_huge_cancel = true;

            if let Err(e) = self.send_reliable(&message) {
                log::error!("{e}");
            }
        }
    }

    fn recv_packet(&mut self) -> Result<Option<Packet>, std::io::Error> {
        let packet = match &mut self.socket {
            SocketKind::Udp(socket) => socket.try_recv(),
            #[cfg(target_arch = "wasm32")]
            SocketKind::WebTransport(socket) => socket.try_recv(),
        };

        let packet = match packet {
            Ok(packet) => packet,
            Err(e) => {
                if e.kind() == std::io::ErrorKind::WouldBlock {
                    return Ok(None);
                }

                return Err(e);
            }
        };

        let Some(mut packet) = packet else {
            return Ok(None);
        };

        self.crypt.decrypt(&mut packet.data[..packet.size]);
        self.packets_recv = self.packets_recv.wrapping_add(1);

        Ok(Some(packet))
    }
}
