use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

use crate::net::{
    connection::ConnectionError,
    packet::{MAX_PACKET_SIZE, Packet, PacketSendError},
};
use std::collections::VecDeque;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::Sender;
use std::sync::mpsc::channel;

use wasm_bindgen::JsValue;

enum ProxyMessage {
    Packet(Option<Packet>),
    Error(ConnectionError),
    WebTransportReady,
    CreateStream(
        Option<(
            web_sys::ReadableStreamDefaultReader,
            web_sys::WritableStreamDefaultWriter,
        )>,
    ),
    StreamData(Vec<u8>),
}

pub struct WebTransportSocket {
    transport: web_sys::WebTransport,
    reader: web_sys::ReadableStreamDefaultReader,
    writer: web_sys::WritableStreamDefaultWriter,

    bi_reader: Option<web_sys::ReadableStreamDefaultReader>,
    bi_writer: Option<web_sys::WritableStreamDefaultWriter>,

    sender: Sender<ProxyMessage>,
    receiver: Receiver<ProxyMessage>,

    stream_data: Vec<u8>,
    stream_packet_queue: VecDeque<Packet>,

    ready: bool,
}

impl WebTransportSocket {
    pub fn new(url: &str, hash: Option<&Vec<u8>>) -> Result<Self, ConnectionError> {
        let options = web_sys::WebTransportOptions::new();

        if let Some(hash) = &hash {
            let cert_hash = web_sys::WebTransportHash::new();
            let hash_array = js_sys::Uint8Array::new_from_slice(hash);

            cert_hash.set_algorithm("sha-256");
            cert_hash.set_value_u8_array(&hash_array);
            options.set_server_certificate_hashes(&[cert_hash]);
        }

        let Ok(transport) = web_sys::WebTransport::new_with_options(url, &options) else {
            return Err(ConnectionError::ProxyConnect);
        };

        let datagrams = transport.datagrams();

        let Ok(reader) = web_sys::ReadableStreamDefaultReader::new(&datagrams.readable()) else {
            return Err(ConnectionError::ProxyConnect);
        };

        let Ok(writer) = web_sys::WritableStreamDefaultWriter::new(&datagrams.writable()) else {
            return Err(ConnectionError::ProxyConnect);
        };

        let (sender, receiver) = channel();

        let ready_sender = sender.clone();

        let ready_promise = transport.ready();

        wasm_bindgen_futures::spawn_local(async move {
            let Ok(_) = JsFuture::from(ready_promise).await else {
                log::error!("WebTransport: Failed to create WebTransport connection.");
                return;
            };

            if let Err(e) = ready_sender.send(ProxyMessage::WebTransportReady) {
                log::error!("{e}");
            }
        });

        let mut result = Self {
            transport,
            reader,
            writer,
            sender,
            receiver,
            bi_reader: None,
            bi_writer: None,
            stream_data: vec![],
            stream_packet_queue: VecDeque::new(),
            ready: false,
        };

        result.spawn_recv_task();

        Ok(result)
    }

    pub fn ready(&self) -> bool {
        self.ready
    }

    fn spawn_recv_task(&mut self) {
        let reader = self.reader.clone();
        let sender = self.sender.clone();

        wasm_bindgen_futures::spawn_local(async move {
            loop {
                let Ok(result) = JsFuture::from(reader.read()).await else {
                    if let Err(e) = sender.send(ProxyMessage::Error(ConnectionError::ProxyRecv)) {
                        log::error!("{e}");
                    }

                    // Unrecoverable error, so terminate task.
                    return;
                };

                let chunk_value =
                    js_sys::Reflect::get(&result, &JsValue::from_str("value")).unwrap();
                let chunk_array: js_sys::Uint8Array = chunk_value.dyn_into().unwrap();
                let chunk = chunk_array.to_vec();

                let mut packet = Packet::empty();

                let size = chunk.len().min(MAX_PACKET_SIZE);

                packet.data[..size].copy_from_slice(&chunk);
                packet.size = size;

                if let Err(e) = sender.send(ProxyMessage::Packet(Some(packet))) {
                    log::error!("{e}");
                }
            }
        });
    }

    fn spawn_bi_recv_task(&mut self) {
        let Some(reader) = &mut self.bi_reader else {
            return;
        };

        let reader = reader.clone();
        let sender = self.sender.clone();

        wasm_bindgen_futures::spawn_local(async move {
            loop {
                let Ok(result) = JsFuture::from(reader.read()).await else {
                    // Unrecoverable error, so terminate task.
                    return;
                };

                let chunk_value =
                    js_sys::Reflect::get(&result, &JsValue::from_str("value")).unwrap();
                let chunk_array: js_sys::Uint8Array = chunk_value.dyn_into().unwrap();
                let chunk = chunk_array.to_vec();

                if let Err(e) = sender.send(ProxyMessage::StreamData(chunk)) {
                    log::error!("{e}");
                }
            }
        });
    }

    pub fn try_recv(&mut self) -> Result<Option<(Packet, bool)>, ConnectionError> {
        let Ok(message) = self.receiver.try_recv() else {
            return Ok(self.process_stream_data());
        };

        match message {
            ProxyMessage::Packet(packet) => {
                let Some(packet) = packet else {
                    // We haven't received any new data, so just return.
                    return Ok(None);
                };

                let packet = packet.clone();

                Ok(Some((packet, true)))
            }
            ProxyMessage::Error(error) => Err(error),
            ProxyMessage::WebTransportReady => {
                let create_promise = self.transport.create_bidirectional_stream();
                let stream_sender = self.sender.clone();

                wasm_bindgen_futures::spawn_local(async move {
                    let mut result = None;

                    let Ok(stream) = JsFuture::from(create_promise).await else {
                        log::error!("WebTransport: Failed to create bidirectional stream.");
                        if let Err(e) = stream_sender.send(ProxyMessage::CreateStream(result)) {
                            log::error!("{e}");
                        }
                        return;
                    };

                    let Ok(reader) = web_sys::ReadableStreamDefaultReader::new(&stream.readable())
                    else {
                        if let Err(e) = stream_sender.send(ProxyMessage::CreateStream(result)) {
                            log::error!("{e}");
                        }
                        return;
                    };

                    let Ok(writer) = web_sys::WritableStreamDefaultWriter::new(&stream.writable())
                    else {
                        if let Err(e) = stream_sender.send(ProxyMessage::CreateStream(result)) {
                            log::error!("{e}");
                        }
                        return;
                    };

                    result = Some((reader, writer));

                    if let Err(e) = stream_sender.send(ProxyMessage::CreateStream(result)) {
                        log::error!("{e}");
                    }
                });

                Ok(None)
            }
            ProxyMessage::CreateStream(stream) => {
                if let Some((bi_reader, bi_writer)) = stream {
                    self.bi_reader = Some(bi_reader);
                    self.bi_writer = Some(bi_writer);

                    self.spawn_bi_recv_task();
                } else {
                    log::error!("Failed to create bi-directional stream.");
                }

                self.ready = true;

                Ok(None)
            }
            ProxyMessage::StreamData(mut data) => {
                self.stream_data.append(&mut data);

                Ok(self.process_stream_data())
            }
        }
    }

    fn process_stream_data(&mut self) -> Option<(Packet, bool)> {
        if !self.stream_packet_queue.is_empty() {
            if let Some(packet) = self.stream_packet_queue.pop_front() {
                return Some((packet, false));
            }
            
            return None;
        }

        if self.stream_data.len() < 5 {
            return None;
        }

        let payload_size = u32::from_le_bytes(self.stream_data[..4].try_into().unwrap()) as usize;
        if self.stream_data.len() < payload_size + 5 {
            return None;
        }

        let control = self.stream_data[4];

        let mut result = None;

        match control {
            // Raw subspace packet
            0 => {
                let mut data = &self.stream_data[5..];
                let mut remaining_size = payload_size;

                while remaining_size > 0 {
                    let packet_size = remaining_size.min(MAX_PACKET_SIZE - 6);

                    let mut packet = Packet::empty();

                    packet.data[0] = 0x00;
                    packet.data[1] = 0x0A;
                    packet.data[2..6].copy_from_slice(&payload_size.to_le_bytes());
                    packet.data[6..packet_size + 6].copy_from_slice(&data[..packet_size]);
                    packet.size = packet_size + 6;

                    self.stream_packet_queue.push_back(packet);

                    data = &data[packet_size..];
                    remaining_size -= packet_size;
                }

                result = Some((self.stream_packet_queue.pop_front().unwrap(), false));
            }
            _ => {
                log::error!("WebTransport: Unknown control value in stream data.");
            }
        }

        self.stream_data.drain(..payload_size + 5);
        result
    }

    pub fn send(&self, data: &[u8]) -> Result<usize, PacketSendError> {
        let buffer = js_sys::Uint8Array::new_from_slice(data);

        let _ = self.writer.write_with_chunk(&buffer);
        Ok(data.len())
    }

    pub fn has_extended_protocol(&self) -> bool {
        self.bi_writer.is_some() && self.bi_reader.is_some()
    }

    pub fn request_map(&mut self, filename: &str, checksum: u32, fallback: &[u8]) {
        if let Some(writer) = &self.bi_writer {
            let mut data = Vec::with_capacity(256);
            let mut full_name = [0; 16];

            for i in 0..filename.as_bytes().len() {
                full_name[i] = filename.as_bytes()[i];
            }

            data.extend_from_slice(&0u32.to_le_bytes());
            data.push(0x01); // RequestMap
            data.extend_from_slice(&full_name);
            data.extend_from_slice(&checksum.to_le_bytes());
            data.extend_from_slice(fallback);

            let payload_size = data.len() as u32 - 5;

            data[..4].copy_from_slice(&payload_size.to_le_bytes());

            let buffer = js_sys::Uint8Array::new_from_slice(&data);
            let _ = writer.write_with_chunk(&buffer);
        }
    }
}
