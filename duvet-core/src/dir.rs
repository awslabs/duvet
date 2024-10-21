// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::{glob::Glob, path::Path};
use futures::Stream;
use std::sync::Arc;

pub mod walk;

#[derive(Clone)]
pub struct Directory {
    pub(crate) path: Path,
    pub(crate) contents: Arc<[Path]>,
}

impl Directory {
    pub fn iter(&self) -> impl Iterator<Item = &Path> {
        self.contents.iter()
    }

    pub fn walk(&self) -> impl Stream<Item = Path> {
        walk::dir(self.path.clone())
    }

    pub fn glob(&self, include: Glob, ignore: Glob) -> impl Stream<Item = Path> {
        walk::glob(self.path.clone(), include, ignore)
    }
}

impl IntoIterator for Directory {
    type Item = Path;
    type IntoIter = DirIter;

    fn into_iter(self) -> Self::IntoIter {
        DirIter {
            contents: self.contents,
            index: 0,
        }
    }
}

pub struct DirIter {
    contents: Arc<[Path]>,
    index: usize,
}

impl Iterator for DirIter {
    type Item = Path;

    fn next(&mut self) -> Option<Self::Item> {
        let index = self.index;
        self.index += 1;
        self.contents.get(index).cloned()
    }
}
