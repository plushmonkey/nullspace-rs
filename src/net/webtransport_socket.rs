use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

use crate::net::{
    connection::ConnectionError,
    packet::{MAX_PACKET_SIZE, Packet, PacketSendError},
};
use std::sync::mpsc::Receiver;
use std::sync::mpsc::Sender;
use std::sync::mpsc::channel;

use wasm_bindgen::JsValue;

pub struct WebTransportSocket {
    _transport: web_sys::WebTransport,
    reader: web_sys::ReadableStreamDefaultReader,
    writer: web_sys::WritableStreamDefaultWriter,

    sender: Sender<Option<Packet>>,
    receiver: Receiver<Option<Packet>>,
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

        // The browser or web_sys doesn't seem to handle the error as Err, so just unwrap.
        let transport = web_sys::WebTransport::new_with_options(url, &options).unwrap();
        let datagrams = transport.datagrams();
        let reader = web_sys::ReadableStreamDefaultReader::new(&datagrams.readable()).unwrap();
        let writer = web_sys::WritableStreamDefaultWriter::new(&datagrams.writable()).unwrap();

        let (sender, receiver) = channel();

        let mut result = Self {
            _transport: transport,
            reader,
            writer,
            sender,
            receiver,
        };

        result.spawn_recv_task();

        Ok(result)
    }

    fn spawn_recv_task(&mut self) {
        let reader = self.reader.clone();
        let sender = self.sender.clone();

        wasm_bindgen_futures::spawn_local(async move {
            loop {
                let result = JsFuture::from(reader.read()).await.unwrap();

                let chunk_value =
                    js_sys::Reflect::get(&result, &JsValue::from_str("value")).unwrap();
                let chunk_array: js_sys::Uint8Array = chunk_value.dyn_into().unwrap();
                let chunk = chunk_array.to_vec();

                let mut packet = Packet::empty();

                let size = chunk.len().min(MAX_PACKET_SIZE);

                packet.data[..size].copy_from_slice(&chunk);
                packet.size = size;

                if let Err(e) = sender.send(Some(packet)) {
                    log::error!("{e}");
                }
            }
        });
    }

    pub fn try_recv(&mut self) -> Result<Option<Packet>, std::io::Error> {
        let Ok(packet) = self.receiver.try_recv() else {
            // Nothing in the process queue, so just return.
            return Ok(None);
        };

        let Some(packet) = packet else {
            // We haven't received any new data, so just return.
            return Ok(None);
        };

        let packet = packet.clone();

        Ok(Some(packet))
    }

    pub fn send(&self, data: &[u8]) -> Result<usize, PacketSendError> {
        let buffer = js_sys::Uint8Array::new_from_slice(data);

        let _ = self.writer.write_with_chunk(&buffer);
        Ok(data.len())
    }
}
