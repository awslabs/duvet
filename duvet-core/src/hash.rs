#[derive(Default)]
pub struct Hasher {
    // TODO
}

impl Hasher {
    pub fn finish(self) -> [u8; 32] {
        [0; 32]
    }
}

impl core::hash::Hasher for Hasher {
    fn write(&mut self, bytes: &[u8]) {
        // TODO
    }

    fn finish(&self) -> u64 {
        panic!()
    }
}
