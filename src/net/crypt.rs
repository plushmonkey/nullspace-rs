use crate::clock::GameTick;
use crate::rng::VieRng;

pub struct VieEncrypt {
    pub session_key: u32,
    pub client_key: u32,
    pub rng: VieRng,
    pub keystream: Box<[u8; 520]>,
}

impl VieEncrypt {
    pub fn new(client_key: u32) -> Self {
        Self {
            session_key: 0,
            client_key,
            rng: VieRng::new(0),
            keystream: vec![0; 520].into_boxed_slice().try_into().unwrap(),
        }
    }

    pub fn initialize(&mut self, server_key: u32) -> bool {
        if !self.is_valid_key(server_key) {
            return false;
        }

        if self.client_key == server_key {
            self.session_key = 0;
            self.keystream = vec![0; 520].into_boxed_slice().try_into().unwrap();
        } else {
            self.session_key = server_key;
            self.rng.seed = self.session_key as i32;

            for i in (0..520).step_by(2) {
                let rand_val = self.rng.next() as u16;

                self.keystream[i..i + 2].copy_from_slice(&rand_val.to_le_bytes());
            }
        }

        true
    }

    pub fn encrypt(&self, pkt: &[u8], dest: &mut [u8]) {
        if self.session_key == 0 {
            dest[..pkt.len()].copy_from_slice(pkt);
            return;
        }

        let mut ksi = 0;
        let mut i = 1;
        let mut iv = self.session_key;

        dest[0] = pkt[0];

        if pkt[0] == 0 {
            if pkt.len() <= 2 {
                dest[..pkt.len()].copy_from_slice(pkt);
                return;
            }

            dest[1] = pkt[1];
            i += 1;
        }

        while i + 4 <= pkt.len() {
            let pkt_data = u32::from_le_bytes(pkt[i..i + 4].try_into().unwrap());
            let keystream: u32 =
                u32::from_le_bytes(self.keystream[ksi..ksi + 4].try_into().unwrap());

            iv = pkt_data ^ keystream ^ iv;
            dest[i..i + 4].copy_from_slice(&iv.to_le_bytes());

            i += 4;
            ksi += 4;
        }

        let diff = pkt.len() - i;
        if diff > 0 {
            let mut remaining_bytes: [u8; 4] = [0; 4];

            remaining_bytes[..diff].copy_from_slice(&pkt[i..i + diff]);
            let remaining = u32::from_le_bytes(remaining_bytes[..4].try_into().unwrap());
            let keystream: u32 =
                u32::from_le_bytes(self.keystream[ksi..ksi + 4].try_into().unwrap());
            let encrypted_slice = remaining ^ keystream ^ iv;
            remaining_bytes[..4].copy_from_slice(&encrypted_slice.to_le_bytes());

            dest[i..i + diff].copy_from_slice(&remaining_bytes[0..diff]);
        }
    }

    pub fn decrypt(&self, pkt: &mut [u8]) {
        if self.session_key == 0 {
            return;
        }

        let mut ksi = 0;
        let mut i = 1;
        let mut iv = self.session_key;

        if pkt[0] == 0 {
            if pkt.len() <= 2 {
                return;
            }

            i += 1;
        }

        while i + 4 <= pkt.len() {
            let edx = u32::from_le_bytes(pkt[i..i + 4].try_into().unwrap());
            let keystream: u32 =
                u32::from_le_bytes(self.keystream[ksi..ksi + 4].try_into().unwrap());
            let decrypted_slice = keystream ^ iv ^ edx;

            pkt[i..i + 4].copy_from_slice(&decrypted_slice.to_le_bytes());

            iv = edx;
            i += 4;
            ksi += 4;
        }

        let diff = pkt.len() - i;

        if diff > 0 {
            let mut remaining_bytes: [u8; 4] = [0; 4];

            remaining_bytes[..diff].copy_from_slice(&pkt[i..i + diff]);
            let remaining = u32::from_le_bytes(remaining_bytes[..4].try_into().unwrap());
            let keystream: u32 =
                u32::from_le_bytes(self.keystream[ksi..ksi + 4].try_into().unwrap());
            let decrypted_slice = remaining ^ keystream ^ iv;

            remaining_bytes[..4].copy_from_slice(&decrypted_slice.to_le_bytes());
            pkt[i..i + diff].copy_from_slice(&remaining_bytes[..diff]);
        }
    }

    pub fn generate_key() -> u32 {
        let edx = GameTick::now(0).value().wrapping_mul(0xCCCCCCCD);

        let seed = GameTick::now(0).value();
        let r1 = VieEncrypt::xorshift(seed) & 0xFFFF;
        let r2 = VieEncrypt::xorshift(seed ^ 0x12345678) & 0xFFFF;

        let mut res = (r1 << 16).wrapping_add(edx >> 3).wrapping_add(r2);

        if res <= 0x7FFFFFFF {
            res = (!res).wrapping_add(1);
        }

        res
    }

    fn is_valid_key(&self, server_key: u32) -> bool {
        server_key == self.session_key
            || server_key == self.client_key
            || server_key == ((!self.client_key).wrapping_add(1))
    }

    fn xorshift(seed: u32) -> u32 {
        let mut seed = seed;

        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;

        seed
    }
}
