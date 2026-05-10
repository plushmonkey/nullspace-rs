use std::{
    ffi::CStr,
    net::SocketAddr,
    time::{Duration, Instant},
};

use miniz_oxide::deflate::compress_to_vec_zlib;
use tokio::net::UdpSocket;
use wtransport::{
    Endpoint, Identity, RecvStream, SendStream, ServerConfig, VarInt, config::IpBindConfig,
    endpoint::IncomingSession,
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

    let mut download_list: Vec<(String, u32, u32)> = vec![];
    let mut bi_stream: Option<(SendStream, RecvStream)> = None;

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

                if let Some((download_request, reliable_id)) = get_packet_kind(0x0C, &dgram, 0xFFFFFFFF) {
                     let index = if download_request.len() >= 3 {
                        u16::from_le_bytes(download_request[1..3].try_into().unwrap()) as usize
                    } else {
                        0
                    };

                    if let Some((send_stream, _)) = &mut bi_stream {
                        if index < download_list.len() {
                            let (filename, checksum, _) = &download_list[index];

                            let path = map_path.clone() + filename;

                            if let Some((data, map_checksum)) = read_map(path) {
                                if map_checksum == *checksum {
                                    send_map_data(send_stream, &data, filename).await;

                                    // The client requested this reliably, so we need to send something to the server so it responds with an ack.
                                    if reliable_id != 0xFFFFFFFF {
                                        let mut msg = [0, 3, 0, 0, 0, 0, 0x00, 0x0E];
                                        msg[2..6].copy_from_slice(&reliable_id.to_le_bytes());

                                        send_to_server(&socket, &remote_addr, &msg).await;
                                    }
                                    continue;
                                }
                            }
                        }
                    }
                }

                send_to_server(&socket, &remote_addr, &dgram).await;
            }
            stream = connection.accept_bi() => {
                if let Ok((send_stream, recv_stream)) = stream {
                    bi_stream = Some((send_stream, recv_stream));
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

                if let Some((map_info, _)) = get_packet_kind(0x29, &buffer[..bytes_recv], 0xFFFFFFFF) {
                    download_list = get_download_list(map_info);
                }

                if let Err(_) = connection.send_datagram(&buffer[..bytes_recv]) {
                    send_to_server(&socket, &remote_addr, &[0, 7]).await;
                    return;
                }
            }
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(1)) => {}
        }
    }
}

async fn send_map_data(stream: &mut SendStream, raw_data: &[u8], filename: &str) {
    let data = compress_to_vec_zlib(raw_data, 6);

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

// This is a hacky way of intercepting simple packet kinds.
// Doesn't work with chunked data.
fn get_packet_kind(kind: u8, buffer: &[u8], reliable_id: u32) -> Option<(&[u8], u32)> {
    if buffer.is_empty() {
        return None;
    }

    if buffer[0] == kind {
        return Some((&buffer, reliable_id));
    }

    // Reliable
    if buffer.len() > 6 && buffer[0] == 0x00 && buffer[1] == 0x03 {
        let reliable_id = u32::from_le_bytes(buffer[2..6].try_into().unwrap());
        return get_packet_kind(kind, &buffer[6..], reliable_id);
    }

    // Cluster
    if buffer.len() > 3 && buffer[0] == 0x00 && buffer[1] == 0x0E {
        let mut buffer = &buffer[2..];

        while buffer.len() > 1 {
            let len = buffer[0] as usize;
            let subpacket = &buffer[1..len + 1];

            if let Some(packet) = get_packet_kind(kind, subpacket, reliable_id) {
                return Some(packet);
            }

            buffer = &buffer[len + 1..];
        }
    }

    None
}

fn get_download_list(buffer: &[u8]) -> Vec<(String, u32, u32)> {
    let mut info_data = &buffer[1..];
    let mut download_list = vec![];

    while info_data.len() >= 20 {
        let mut terminated_str = [0; 17];
        terminated_str[0..16].copy_from_slice(&buffer[1..17]);

        if let Ok(filename) = CStr::from_bytes_until_nul(&terminated_str) {
            if let Ok(filename) = filename.to_str() {
                let filename = filename.to_owned();

                let checksum = u32::from_le_bytes(info_data[16..20].try_into().unwrap());
                let size = if info_data.len() >= 24 {
                    u32::from_le_bytes(info_data[20..24].try_into().unwrap())
                } else {
                    0
                };

                download_list.push((filename, checksum, size));
            }
        }

        if info_data.len() < 24 {
            return download_list;
        }

        info_data = &info_data[24..];
    }

    download_list
}
