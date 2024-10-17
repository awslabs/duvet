use blake3::hash;
use core::fmt;
use core::ops::Deref;
use std::sync::Arc;

const HASH_LEN: usize = 32;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Contents(Arc<[u8]>);

impl fmt::Debug for Contents {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = f.debug_struct("Contents");

        // TODO hex encode
        s.field("hash", &self.hash());

        if let Ok(contents) = core::str::from_utf8(self.data()) {
            s.field("contents", &contents);
        } else {
            s.field("contents", &"...");
        }

        s.finish()
    }
}

impl Contents {
    pub fn hash(&self) -> &[u8; HASH_LEN] {
        self.parts().0
    }

    pub fn data(&self) -> &[u8] {
        self.parts().1
    }

    fn parts(&self) -> (&[u8; HASH_LEN], &[u8]) {
        let ptr = self.0.as_ptr();
        let len = self.0.len() - HASH_LEN;
        let hash = unsafe { &*(ptr.add(len) as *const [u8; HASH_LEN]) };
        let data = unsafe { core::slice::from_raw_parts(ptr, len) };
        (hash, data)
    }
}

impl Deref for Contents {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.data()
    }
}

impl AsRef<[u8]> for Contents {
    fn as_ref(&self) -> &[u8] {
        &*self
    }
}

impl From<Vec<u8>> for Contents {
    fn from(mut data: Vec<u8>) -> Contents {
        let hash = *hash(&data).as_bytes();
        data.extend_from_slice(&hash);
        Contents(Arc::from(data))
    }
}

impl From<String> for Contents {
    fn from(value: String) -> Self {
        value.into_bytes().into()
    }
}

impl From<&[u8]> for Contents {
    fn from(value: &[u8]) -> Self {
        let mut vec = Vec::with_capacity(value.len() + HASH_LEN);
        vec.extend_from_slice(value);
        vec.into()
    }
}

impl From<&str> for Contents {
    fn from(value: &str) -> Self {
        value.as_bytes().into()
    }
}
