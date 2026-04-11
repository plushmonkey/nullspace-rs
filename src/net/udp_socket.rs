use std::net::{IpAddr, SocketAddr};

use crate::net::{
    connection::ConnectionError,
    packet::{Packet, PacketSendError},
};

pub struct UdpSocket {
    pub remote_addr: SocketAddr,
    pub socket: std::net::UdpSocket,
}

impl UdpSocket {
    pub fn new(remote_ip: &str, remote_port: u16) -> Result<Self, ConnectionError> {
        use std::str::FromStr;

        let remote_addr = std::net::Ipv4Addr::from_str(remote_ip)?;
        let remote_addr = SocketAddr::new(IpAddr::V4(remote_addr), remote_port);
        let socket = std::net::UdpSocket::bind("0.0.0.0:0")?;

        socket.set_nonblocking(true)?;

        Ok(Self {
            remote_addr,
            socket,
        })
    }

    pub fn try_recv(&self) -> Result<Option<Packet>, std::io::Error> {
        let mut packet: Packet = Packet::empty();

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

        Ok(Some(packet))
    }

    pub fn send(&self, data: &[u8]) -> Result<usize, PacketSendError> {
        Ok(self.socket.send_to(&data, &self.remote_addr)?)
    }
}
