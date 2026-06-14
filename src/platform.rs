#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::JsFuture;

use std::sync::mpsc::Receiver;
use std::sync::mpsc::Sender;
use std::sync::mpsc::channel;

#[derive(Copy, Clone)]
pub enum PlatformIoState {
    Pending,
    Complete,
}

#[derive(Clone)]
pub struct PlatformLoadRequest {
    pub state: PlatformIoState,
    pub result: Option<Vec<u8>>,
    pub zone: String,
    pub filename: String,
}

pub enum PlatformChannelMessage {
    Open,
    Load(PlatformLoadRequest),
}

#[derive(Copy, Clone, PartialEq)]
pub enum PlatformState {
    Closed,
    Opened,
}

pub struct Platform {
    #[cfg(not(target_arch = "wasm32"))]
    pub io: NativePlatformIo,
    #[cfg(target_arch = "wasm32")]
    pub io: WebPlatformIo,

    pub load_requests: Vec<PlatformLoadRequest>,

    sender: Sender<PlatformChannelMessage>,
    receiver: Receiver<PlatformChannelMessage>,

    pub state: PlatformState,
}

impl Platform {
    pub fn new() -> Self {
        let (sender, receiver) = channel();

        #[cfg(not(target_arch = "wasm32"))]
        let io = NativePlatformIo {};

        #[cfg(not(target_arch = "wasm32"))]
        let state = PlatformState::Opened;

        #[cfg(target_arch = "wasm32")]
        let io = WebPlatformIo::new();

        #[cfg(target_arch = "wasm32")]
        let state = PlatformState::Closed;

        #[cfg(target_arch = "wasm32")]
        {
            let fut = JsFuture::from(js_sys::Promise::from(io.js.open()));
            let sender = sender.clone();

            wasm_bindgen_futures::spawn_local(async move {
                let _ = fut.await;
                let _ = sender.send(PlatformChannelMessage::Open);
            });
        }

        Self {
            io,
            load_requests: vec![],
            sender,
            receiver,
            state,
        }
    }

    pub fn is_load_complete(&mut self) -> bool {
        #[cfg(target_arch = "wasm32")]
        if self.state != PlatformState::Opened {
            let mesg = match self.receiver.try_recv() {
                Ok(mesg) => mesg,
                Err(_) => {
                    return false;
                }
            };

            match &mesg {
                PlatformChannelMessage::Open => {
                    self.state = PlatformState::Opened;

                    for request in self.load_requests.clone() {
                        self.kickoff_load_request(request);
                    }
                }
                _ => {}
            }
        }

        if self.load_requests.is_empty() {
            return false;
        }

        loop {
            match self.receiver.try_recv() {
                Ok(mesg) => match mesg {
                    PlatformChannelMessage::Load(load_request) => {
                        self.store_load_result(load_request);
                    }
                    _ => {}
                },
                Err(_) => {
                    break;
                }
            }
        }

        for request in &self.load_requests {
            if let PlatformIoState::Pending = request.state {
                return false;
            }
        }

        true
    }

    fn store_load_result(&mut self, load_request: PlatformLoadRequest) {
        for request in &mut self.load_requests {
            if request.filename == load_request.filename {
                *request = load_request;
                break;
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn kickoff_load_request(&mut self, request: PlatformLoadRequest) {
        let mut request = request;

        let sender = self.sender.clone();
        let fut = self.io.load_zone_file(&request.zone, &request.filename);

        wasm_bindgen_futures::spawn_local(async move {
            let result = match fut.await {
                Ok(value) => value,
                Err(_) => {
                    request.state = PlatformIoState::Complete;
                    request.result = None;

                    let _ = sender.send(PlatformChannelMessage::Load(request));
                    return;
                }
            };

            let chunk_array: js_sys::Uint8Array = result.dyn_into().unwrap();
            let chunk = chunk_array.to_vec();

            request.state = PlatformIoState::Complete;
            request.result = Some(chunk);

            let _ = sender.send(PlatformChannelMessage::Load(request));
        });
    }

    pub fn request_file_load(&mut self, zone: &str, filename: &str) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let result = self.io.load_zone_file(zone, filename);

            let _ = self.sender;
            let _ = self.receiver;

            self.load_requests.push(PlatformLoadRequest {
                state: PlatformIoState::Complete,
                result,
                zone: zone.to_string(),
                filename: filename.to_string(),
            });
        }
        #[cfg(target_arch = "wasm32")]
        {
            let request = PlatformLoadRequest {
                state: PlatformIoState::Pending,
                result: None,
                zone: zone.to_string(),
                filename: filename.to_string(),
            };

            self.load_requests.push(PlatformLoadRequest {
                state: PlatformIoState::Pending,
                result: None,
                zone: zone.to_string(),
                filename: filename.to_string(),
            });

            if self.state != PlatformState::Opened {
                return;
            }

            self.kickoff_load_request(request);
        }
    }

    pub fn request_file_save(&mut self, zone: &str, filename: &str, checksum: u32, data: &[u8]) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = checksum;

            self.io.save_zone_file(zone, filename, data);
        }
        #[cfg(target_arch = "wasm32")]
        {
            let fut = self.io.save_zone_file(zone, filename, checksum, data);

            wasm_bindgen_futures::spawn_local(async move {
                let _ = fut.await;
            });
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub struct NativePlatformIo {}

#[cfg(not(target_arch = "wasm32"))]
impl NativePlatformIo {
    fn build_zone_directory(zone: &str) -> Result<(), std::io::Error> {
        std::fs::DirBuilder::new()
            .recursive(true)
            .create(format!("zones/{}", zone))?;
        Ok(())
    }

    fn get_zone_path(zone: &str, filename: &str) -> String {
        format!("zones/{}/{}", zone, filename)
    }

    fn load_zone_file(&mut self, zone: &str, filename: &str) -> Option<Vec<u8>> {
        let path = Self::get_zone_path(zone, filename);

        match std::fs::read(path) {
            Ok(data) => Some(data),
            Err(_) => None,
        }
    }

    fn save_zone_file(&mut self, zone: &str, filename: &str, data: &[u8]) -> bool {
        let map_path = Self::get_zone_path(zone, filename);

        if let Err(e) = Self::build_zone_directory(zone) {
            log::error!("Error creating zone directory: {}", e);
            return false;
        }

        if let Err(e) = std::fs::write(map_path, data) {
            log::error!("Error writing map: {}", e);
            return false;
        }

        true
    }
}

#[cfg(target_arch = "wasm32")]
pub struct WebPlatformIo {
    js: WebPlatformIoJs,
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(module = "/www/scripts/platform.js")]
unsafe extern "C" {
    pub type WebPlatformIoJs;

    #[wasm_bindgen(constructor)]
    pub fn new() -> WebPlatformIoJs;

    #[wasm_bindgen(method)]
    pub fn open(this: &WebPlatformIoJs) -> JsValue;

    #[wasm_bindgen(method)]
    pub fn load_zone_file(this: &WebPlatformIoJs, zone: &str, filename: &str) -> JsValue;

    #[wasm_bindgen(method)]
    pub fn save_zone_file(
        this: &WebPlatformIoJs,
        zone: &str,
        filename: &str,
        checksum: u32,
        data: &[u8],
    ) -> JsValue;
}

#[cfg(target_arch = "wasm32")]
impl WebPlatformIo {
    pub fn new() -> Self {
        let js = WebPlatformIoJs::new();

        Self { js }
    }

    fn load_zone_file(&mut self, zone: &str, filename: &str) -> JsFuture {
        let value = self.js.load_zone_file(zone, filename);
        let promise = js_sys::Promise::from(value);

        JsFuture::from(promise)
    }

    fn save_zone_file(
        &mut self,
        zone: &str,
        filename: &str,
        checksum: u32,
        data: &[u8],
    ) -> JsFuture {
        let value = self.js.save_zone_file(zone, filename, checksum, data);
        let promise = js_sys::Promise::from(value);

        JsFuture::from(promise)
    }
}
