// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use core::{cmp::Ordering, fmt};
use serde::Deserialize;
use std::{ffi::OsStr, ops::Deref, path::PathBuf, sync::Arc};

#[derive(Clone, Deserialize)]
#[serde(transparent)]
pub struct Path {
    path: Arc<OsStr>,
}

impl Path {
    pub fn pop(&mut self) -> bool {
        if let Some(parent) = self.parent() {
            *self = parent.into();
            true
        } else {
            false
        }
    }

    pub fn push<V: AsRef<std::path::Path>>(&mut self, component: V) {
        *self = self.join(component);
    }

    pub fn join<V: AsRef<std::path::Path>>(&self, component: V) -> Self {
        self.as_ref().join(component).into()
    }
}

impl PartialEq for Path {
    fn eq(&self, other: &Self) -> bool {
        self.as_ref().eq(other.as_ref())
    }
}

impl Eq for Path {}

impl PartialOrd for Path {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Path {
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_ref().cmp(other.as_ref())
    }
}

impl core::hash::Hash for Path {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.as_ref().hash(state)
    }
}

impl fmt::Debug for Path {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.as_ref().fmt(f)
    }
}

impl fmt::Display for Path {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let path = self.as_ref();
        let path = crate::env::current_dir()
            .ok()
            .and_then(|dir| path.strip_prefix(dir).ok())
            .unwrap_or(path);
        path.display().fmt(f)
    }
}

impl Deref for Path {
    type Target = std::path::Path;

    fn deref(&self) -> &Self::Target {
        std::path::Path::new(&self.path)
    }
}

impl AsRef<std::path::Path> for Path {
    fn as_ref(&self) -> &std::path::Path {
        self
    }
}

impl PartialEq<str> for Path {
    fn eq(&self, other: &str) -> bool {
        self.as_ref().eq(std::path::Path::new(other))
    }
}

impl PartialEq<std::path::Path> for Path {
    fn eq(&self, other: &std::path::Path) -> bool {
        self.as_ref().eq(other)
    }
}

impl From<String> for Path {
    fn from(path: String) -> Self {
        Self {
            path: PathBuf::from(path).into_os_string().into(),
        }
    }
}

impl From<PathBuf> for Path {
    fn from(path: PathBuf) -> Self {
        Self {
            path: path.into_os_string().into(),
        }
    }
}

impl From<&PathBuf> for Path {
    fn from(path: &PathBuf) -> Self {
        path.as_path().into()
    }
}

impl From<&std::path::Path> for Path {
    fn from(path: &std::path::Path) -> Self {
        Self {
            path: path.as_os_str().into(),
        }
    }
}

impl From<Path> for PathBuf {
    fn from(value: Path) -> Self {
        PathBuf::from(&value.path)
    }
}

impl From<&Path> for Path {
    fn from(path: &Path) -> Self {
        Self {
            path: path.as_os_str().into(),
        }
    }
}

impl From<&str> for Path {
    fn from(path: &str) -> Self {
        Self {
            path: std::path::Path::new(path).as_os_str().into(),
        }
    }
}
