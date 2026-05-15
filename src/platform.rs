pub trait Platform {
    fn load_zone_file(&mut self, zone: &str, filename: &str) -> Option<Vec<u8>>;
    fn save_zone_file(&mut self, zone: &str, filename: &str, data: &[u8]) -> bool;
}

#[cfg(not(target_arch = "wasm32"))]
pub struct NativePlatform {}

#[cfg(not(target_arch = "wasm32"))]
impl NativePlatform {
    fn build_zone_directory(zone: &str) -> Result<(), std::io::Error> {
        std::fs::DirBuilder::new()
            .recursive(true)
            .create(format!("zones/{}", zone))?;
        Ok(())
    }

    fn get_zone_path(zone: &str, filename: &str) -> String {
        format!("zones/{}/{}", zone, filename)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Platform for NativePlatform {
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
pub struct WebPlatform {}

#[cfg(target_arch = "wasm32")]
impl Platform for WebPlatform {
    fn load_zone_file(&mut self, zone: &str, filename: &str) -> Option<Vec<u8>> {
        let _ = zone;
        let _ = filename;

        log::warn!("WebPlatform load_zone_file not implemented.");
        None
    }

    fn save_zone_file(&mut self, zone: &str, filename: &str, data: &[u8]) -> bool {
        let _ = zone;
        let _ = filename;
        let _ = data;

        log::warn!("WebPlatform save_zone_file not implemented.");
        false
    }
}
