use std::{
    ffi::CStr,
    net::SocketAddr,
    time::{Duration, Instant},
};

use miniz_oxide::deflate::compress_to_vec_zlib;
use tokio::net::UdpSocket;
use wtransport::{
    Endpoint, Identity, RecvStream, SendStream, ServerConfig, VarInt, config::IpBindConfig,
    endpoint::IncomingSession, error::StreamReadError,
};

use clap::Parser;

use crate::checksum::crc32;

pub mod checksum;

#[derive(Parser, Clone, Debug)]
struct Args {
    #[arg(
        short,
        long,
        default_value = "10",
        help = "Disconnect clients after no data for this many seconds"
    )]
    timeout: u64,

    #[arg(short, long, default_value = "4433", help = "Port for the proxy")]
    listen_port: u16,

    #[arg(long, default_value = "127.0.0.1")]
    game_ip: String,

    #[arg(long, default_value = "5000")]
    game_port: u16,

    #[arg(long, default_value = "cert.pem")]
    cert_path: String,

    #[arg(long, default_value = "key.pem")]
    key_path: String,

    #[arg(
        long,
        default_value = "maps",
        help = "The path to the server's maps folder"
    )]
    map_path: String,
}

/*

This is a simple reverse proxy that handles WebTransport connections and passes the data along to a game server.

It currently does nothing more, but in the future I'd like to adjust the initialization stage to pass the client's
connect details to the server so it can handle the connection as if it were direct. That would allow things like
banning and aliasing to continue working by not all being on the same connection.

*/

#[tokio::main]
async fn main() {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();

    let args = Args::parse();

    let game_server_addr = format!("{}:{}", args.game_ip, args.game_port);

    let identity = Identity::load_pemfiles(args.cert_path, args.key_path)
        .await
        .unwrap();

    println!("Certificate hash (used for local certificate bypass): ");

    println!(
        "let hash = new Uint8Array({});",
        identity.certificate_chain().as_slice()[0]
            .hash()
            .fmt(wtransport::tls::Sha256DigestFmt::BytesArray)
    );

    // Listen on IPv4 only so the connection data can be sent to the server.
    let config = ServerConfig::builder()
        .with_bind_config(IpBindConfig::InAddrAnyV4, args.listen_port)
        .with_identity(identity)
        .keep_alive_interval(None)
        .max_idle_timeout(Some(Duration::from_secs(args.timeout)))
        .unwrap()
        .build();

    let server = Endpoint::server(config).unwrap();

    log::info!("Listening for connections...");

    let mut map_path = args.map_path.clone();

    if !map_path.ends_with('/') && !map_path.ends_with('\\') {
        map_path += "/";
    }

    loop {
        let incoming_session = server.accept().await;

        tokio::spawn(handle_connection(
            incoming_session,
            game_server_addr.clone(),
            args.timeout,
            map_path.clone(),
        ));
    }
}

async fn send_to_server(socket: &UdpSocket, _from: &SocketAddr, data: &[u8]) {
    /*
        let IpAddr::V4(ip) = from.ip() else {
            return;
        };

        let octets = ip.octets();
        let port = from.port();
        const PREFIX_SIZE: usize = 6;

        // Construct a new buffer to hold the ip, port, and payload.
        // The game server needs this information so it can differentiate between player connections instead of everyone having the proxy address.
        let mut full_data = vec![0; data.len() + PREFIX_SIZE].into_boxed_slice();

        full_data[0..4].copy_from_slice(&octets);
        full_data[4..6].copy_from_slice(&port.to_be_bytes());
        full_data[6..].copy_from_slice(data);
    */
    //println!("Sending {:?} to game server.", data);

    if let Err(e) = socket.send(data).await {
        log::error!("socket_send_error: {e}");
    }
}

// This is not a high performance buffer, but it's probably good enough.
pub struct ReceiveBuffer {
    pub buffer: Vec<u8>,
    pub offset: usize,
}

impl ReceiveBuffer {
    pub fn new() -> Self {
        let mut buffer = vec![];

        buffer.resize(4096, 0);

        Self { buffer, offset: 0 }
    }

    pub fn get_insert_slice(&mut self) -> &mut [u8] {
        &mut self.buffer[self.offset..]
    }

    pub fn process(&mut self, bytes_recv: usize) -> Option<Vec<u8>> {
        self.offset += bytes_recv;

        if self.offset < 5 {
            return None;
        }

        let payload_size = u32::from_le_bytes(self.buffer[..4].try_into().unwrap()) as usize;
        let required_buffer_size = payload_size + 5;

        if self.offset < required_buffer_size {
            return None;
        }

        let result_size = self.offset;

        let mut result = self.buffer.clone();
        result.resize(result_size, 0);

        self.offset = 0;

        return Some(result);
    }
}

async fn handle_connection(
    incoming_session: IncomingSession,
    game_server_addr: String,
    timeout: u64,
    map_path: String,
) {
    let session_request = match incoming_session.await {
        Ok(request) => request,
        Err(e) => {
            log::error!("{e}");
            return;
        }
    };

    let connection = match session_request.accept().await {
        Ok(connection) => connection,
        Err(e) => {
            log::error!("{e}");
            return;
        }
    };

    log::debug!("New connection from {:?}.", connection.remote_address());

    // Create a new socket and remote endpoint to the game server for this new connection session.

    let remote_addr = connection.remote_address();
    let socket = match UdpSocket::bind("0.0.0.0:0").await {
        Ok(socket) => socket,
        Err(e) => {
            log::error!("{e}");
            return;
        }
    };

    if let Err(e) = socket.connect(&game_server_addr).await {
        log::error!("{e}");
        return;
    }

    let mut buffer = [0; 1024];
    let mut last_activity = Instant::now();

    let mut bi_stream: Option<(SendStream, RecvStream)> = None;
    let mut bi_buffer = ReceiveBuffer::new();

    loop {
        // Perform timeout manually because Firefox doesn't always close the connection and wtransport doesn't seem to consider it inactive either.
        if last_activity.elapsed() > Duration::from_secs(timeout) {
            connection.close(VarInt::from_u32(0), &[]);
        }

        tokio::select! {
            e = connection.closed() => {
                log::trace!("connection closed: {e}");
                send_to_server(&socket, &remote_addr, &[0, 7]).await;
                return;
            }
            dgram = connection.receive_datagram() => {
                let dgram = match dgram {
                    Ok(dgram) => dgram,
                    Err(_) => {
                        send_to_server(&socket, &remote_addr, &[0, 7]).await;
                        return;
                    }
                };

                last_activity = Instant::now();

                send_to_server(&socket, &remote_addr, &dgram).await;
            }
            stream = connection.accept_bi() => {
                if let Ok((send_stream, recv_stream)) = stream {
                    bi_stream = Some((send_stream, recv_stream));
                }
            }
            bytes_recv = receive_bi(bi_stream.as_mut(), &mut bi_buffer) => {
                let bytes_recv = match bytes_recv {
                    Ok(bytes_recv) => bytes_recv,
                    Err(_) => {
                        continue;
                    }
                };

                if let Some(bytes_recv) = bytes_recv {
                    if let Some(packet) = bi_buffer.process(bytes_recv) {
                        if packet.len() >= 5 && packet[4] == 0x01 {
                            process_map_request(&packet[5..], &map_path, bi_stream.as_mut(), &socket, &remote_addr).await;
                        }
                    }
                }
            }
            bytes_recv = socket.recv(&mut buffer) => {
                let bytes_recv = match bytes_recv {
                    Ok(bytes_recv) => bytes_recv,
                    Err(_) => {
                        send_to_server(&socket, &remote_addr, &[0, 7]).await;
                        return;
                    }
                };

                if let Err(_) = connection.send_datagram(&buffer[..bytes_recv]) {
                    send_to_server(&socket, &remote_addr, &[0, 7]).await;
                    return;
                }
            }
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(1)) => {}
        }
    }
}

async fn process_map_request(
    packet: &[u8],
    map_path: &String,
    streams: Option<&mut (SendStream, RecvStream)>,
    socket: &UdpSocket,
    remote_addr: &SocketAddr,
) {
    let Some((send_stream, _)) = streams else {
        return;
    };

    let filename = &packet[0..16];
    let checksum = u32::from_le_bytes(packet[16..20].try_into().unwrap());
    let index = u16::from_le_bytes(packet[20..22].try_into().unwrap());
    let fallback = &packet[22..];

    let mut terminated_str = [0; 17];
    terminated_str[0..16].copy_from_slice(&filename[0..16]);

    terminated_str[15] = 0;
    if terminated_str[14] == b'.' {
        terminated_str[14] = 0;
    }

    if let Ok(filename) = CStr::from_bytes_until_nul(&terminated_str) {
        if let Ok(filename) = filename.to_str() {
            let path = map_path.clone() + filename;

            if let Some((data, map_checksum)) = read_map(path) {
                if map_checksum == checksum {
                    send_map_data(send_stream, &data, filename, index).await;
                    return;
                }
            }
        }
    }

    // We failed to load the map, so send the fallback request to the server.
    send_to_server(socket, remote_addr, fallback).await;
}

async fn receive_bi(
    streams: Option<&mut (SendStream, RecvStream)>,
    buffer: &mut ReceiveBuffer,
) -> Result<Option<usize>, StreamReadError> {
    let Some((_, recv_stream)) = streams else {
        return Ok(None);
    };

    let slice = buffer.get_insert_slice();

    recv_stream.read(slice).await
}

async fn send_map_data(stream: &mut SendStream, raw_data: &[u8], filename: &str, index: u16) {
    let compressed_data;

    let mut data = raw_data;

    if index == 0 {
        compressed_data = compress_to_vec_zlib(raw_data, 6);
        data = &compressed_data;
    }

    let mut buffer: Vec<u8> = Vec::with_capacity(4 + 1 + 17 + data.len());
    let payload_length = data.len() as u32 + 17;

    buffer.extend_from_slice(&payload_length.to_le_bytes());
    buffer.push(0x00); // Control

    buffer.push(0x2A); // Map data

    let mut full_name = [0; 16];
    for i in 0..filename.as_bytes().len() {
        full_name[i] = filename.as_bytes()[i];
    }

    buffer.extend_from_slice(&full_name);

    buffer.extend_from_slice(&data);

    if let Err(e) = stream.write_all(&buffer).await {
        log::error!("{e}");
    }
}

fn read_map(path: String) -> Option<(Vec<u8>, u32)> {
    if let Ok(data) = std::fs::read(path) {
        let checksum = crc32(&data);

        return Some((data, checksum));
    }

    None
}
