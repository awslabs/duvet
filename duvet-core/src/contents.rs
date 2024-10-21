// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::hash::{Hash, Hasher, HASH_LEN};
use core::{fmt, ops::Deref};
use std::sync::Arc;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Contents(Arc<[u8]>);

impl fmt::Debug for Contents {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = f.debug_struct("Contents");

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
    pub fn hash(&self) -> &Hash {
        self.parts().0
    }

    pub fn data(&self) -> &[u8] {
        self.parts().1
    }

    fn parts(&self) -> (&Hash, &[u8]) {
        let ptr = self.0.as_ptr();
        let len = self.0.len() - HASH_LEN;
        let hash = unsafe { &*(ptr.add(len) as *const Hash) };
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
        self
    }
}

impl From<Vec<u8>> for Contents {
    fn from(mut data: Vec<u8>) -> Contents {
        data.extend_from_slice(&*Hasher::hash(&data));
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
