// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use core::{fmt, ops::Deref};

#[derive(Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[repr(transparent)]
pub struct Hash([u8; HASH_LEN]);

impl fmt::Debug for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x")?;
        for byte in &self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl Deref for Hash {
    type Target = [u8; HASH_LEN];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<[u8]> for Hash {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl AsRef<[u8; HASH_LEN]> for Hash {
    fn as_ref(&self) -> &[u8; HASH_LEN] {
        &self.0
    }
}

impl From<blake3::Hash> for Hash {
    fn from(value: blake3::Hash) -> Self {
        Self(value.into())
    }
}

pub const HASH_LEN: usize = 32;

#[derive(Default)]
pub struct Hasher {
    inner: blake3::Hasher,
}

impl Hasher {
    pub fn hash<T: AsRef<[u8]>>(v: T) -> Hash {
        use core::hash::Hasher as _;
        let mut hash = Self::default();
        hash.write(v.as_ref());
        hash.finish()
    }

    pub fn finish(&self) -> Hash {
        self.inner.finalize().into()
    }
}

impl core::hash::Hasher for Hasher {
    fn write(&mut self, bytes: &[u8]) {
        self.inner.update(bytes);
    }

    fn finish(&self) -> u64 {
        let mut out = [0; 8];
        let hash = self.inner.finalize();
        out.copy_from_slice(&hash.as_bytes()[..8]);
        u64::from_le_bytes(out)
    }
}
