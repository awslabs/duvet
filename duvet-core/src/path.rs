// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use core::fmt;
use serde::Deserialize;
use std::{ffi::OsStr, ops::Deref, path::PathBuf, sync::Arc};

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
#[serde(transparent)]
pub struct Path {
    path: Arc<OsStr>,
}

impl fmt::Debug for Path {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.as_ref().fmt(f)
    }
}

impl fmt::Display for Path {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.as_ref().display().fmt(f)
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
