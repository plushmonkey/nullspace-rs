#[derive(Copy, Clone, Debug)]
pub struct VieRng {
    pub seed: i32,
}

impl VieRng {
    pub fn new(seed: i32) -> Self {
        Self {
            seed,
        }
    }

    // Computes next value and mutates seed.
    pub fn next(&mut self) -> u32 {
        self.seed = self.peek_next();
        
        self.seed as u32
    }

    // Computes next value without mutating seed.
    pub fn peek_next(&self) -> i32 {
        let new_seed = self.seed as u32;
        let mut new_seed = (new_seed % 0x1F31D).wrapping_mul(0x41A7).wrapping_sub((new_seed / 0x1F31D).wrapping_mul(0xB14)).wrapping_add(0x7B);

        if (new_seed >> 31) > 0 {
            new_seed = new_seed.wrapping_add(0x7FFFFFFF);
        }

        new_seed as i32
    }

    pub fn next_encrypt(&mut self) -> u16 {
        let old_seed: u64 = self.seed as u64;

        let new_seed = (old_seed.wrapping_mul(0x834E0B5F) >> 48) as u32;
        let new_seed = new_seed.wrapping_add(new_seed >> 31);

        let old_seed = self.seed as u32;
        let mut new_seed = (old_seed % 0x1F31D).wrapping_mul(0x41A7).wrapping_sub(new_seed.wrapping_mul(0xB14)).wrapping_add(0x7B);

        if (new_seed >> 31) > 0 {
            new_seed = new_seed.wrapping_add(0x7FFFFFFF);
        }

        self.seed = new_seed as i32;

        self.seed as u16
    }
}
