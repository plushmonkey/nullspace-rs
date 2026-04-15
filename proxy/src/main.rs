use std::{
    net::SocketAddr,
    time::{Duration, Instant},
};

use tokio::net::UdpSocket;
use wtransport::{
    Endpoint, Identity, ServerConfig, VarInt, config::IpBindConfig, endpoint::IncomingSession,
};

const WTRANSPORT_TIMEOUT_SECONDS: u64 = 10;

/*

This is a simple reverse proxy that handles WebTransport connections and passes the data along to a game server.

It currently does nothing more, but in the future I'd like to adjust the initialization stage to pass the client's
connect details to the server so it can handle the connection as if it were direct. That would allow things like
banning and aliasing to continue working by not all being on the same connection.

*/

const ZONE_SERVER_ADDR: &str = "127.0.0.1:5000";

#[tokio::main]
async fn main() {
    let identity = Identity::load_pemfiles("cert.pem", "key.pem")
        .await
        .unwrap();

    println!("Certificate hash (used for local certificate bypass): ");
    let hash = identity.certificate_chain().as_slice()[0]
        .hash()
        .fmt(wtransport::tls::Sha256DigestFmt::BytesArray);

    println!("proxy_hash: {}", &hash[1..hash.len() - 1]);

    // Listen on IPv4 only so the connection data can be sent to the server.
    let config = ServerConfig::builder()
        .with_bind_config(IpBindConfig::InAddrAnyV4, 4433)
        .with_identity(identity)
        .keep_alive_interval(None)
        .max_idle_timeout(Some(Duration::from_secs(WTRANSPORT_TIMEOUT_SECONDS)))
        .unwrap()
        .build();

    let server = Endpoint::server(config).unwrap();

    println!("Listening for connections...");

    loop {
        let incoming_session = server.accept().await;

        tokio::spawn(handle_connection(incoming_session));
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
        println!("socket_send_error: {e}");
    }
}

async fn handle_connection(incoming_session: IncomingSession) {
    let session_request = match incoming_session.await {
        Ok(request) => request,
        Err(e) => {
            println!("{e}");
            return;
        }
    };

    let connection = match session_request.accept().await {
        Ok(connection) => connection,
        Err(e) => {
            println!("{e}");
            return;
        }
    };

    println!("New connection from {:?}.", connection.remote_address());

    // Create a new socket and remote endpoint to the game server for this new connection session.

    let remote_addr = connection.remote_address();
    let socket = match UdpSocket::bind("0.0.0.0:0").await {
        Ok(socket) => socket,
        Err(e) => {
            println!("{e}");
            return;
        }
    };

    if let Err(e) = socket.connect(ZONE_SERVER_ADDR).await {
        println!("{e}");
        return;
    }

    let mut buffer = [0; 1024];
    let mut last_activity = Instant::now();

    loop {
        // Perform timeout manually because Firefox doesn't always close the connection and wtransport doesn't seem to consider it inactive either.
        if last_activity.elapsed() > Duration::from_secs(WTRANSPORT_TIMEOUT_SECONDS) {
            connection.close(VarInt::from_u32(0), &[]);
        }

        tokio::select! {
            e = connection.closed() => {
                println!("connection closed: {e}");
                send_to_server(&socket, &remote_addr, &[0, 7]).await;
                return;
            }
            dgram = connection.receive_datagram() => {
                let dgram = match dgram {
                    Ok(dgram) => dgram,
                    Err(e) => {
                        println!("receive_dgram_error: {e}");
                        send_to_server(&socket, &remote_addr, &[0, 7]).await;
                        return;
                    }
                };

                last_activity = Instant::now();

                send_to_server(&socket, &remote_addr, &dgram).await;
            }
            bytes_recv = socket.recv(&mut buffer) => {
                let bytes_recv = match bytes_recv {
                    Ok(bytes_recv) => bytes_recv,
                    Err(e) => {
                        println!("socket_recv_error: {e}");
                        send_to_server(&socket, &remote_addr, &[0, 7]).await;
                        return;
                    }
                };

                if let Err(e) = connection.send_datagram(&buffer[..bytes_recv]) {
                    println!("send_dgram_error{e}");
                    send_to_server(&socket, &remote_addr, &[0, 7]).await;
                    return;
                }
            }
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(1)) => {}
        }
    }
}
