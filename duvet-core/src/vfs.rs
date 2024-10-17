// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    dir::Directory,
    file::{BinaryFile, SourceFile},
    path::Path,
    query::Query,
    Result,
};
use core::cell::RefCell;
use std::{fs::FileType, time::SystemTime};

pub mod fs;
pub use fs::Fs;

pub type OrCreate = Query<Result<crate::contents::Contents>>;

thread_local! {
    static VFS: RefCell<Box<dyn Vfs + 'static>> = RefCell::new(Box::new(Fs::default()));
}

pub fn setup<F: 'static + Vfs>(f: F) {
    VFS.with(|current| *current.borrow_mut() = Box::new(f));
}

#[inline]
fn vfs<F: FnOnce(&dyn Vfs) -> R, R>(f: F) -> R {
    VFS.with(|current| {
        let current = current.borrow();
        let current: &dyn Vfs = &**current;
        f(current)
    })
}

pub trait Vfs {
    fn read_dir(&self, path: Path) -> Query<Result<Directory>>;
    fn read_file(&self, path: Path, or_create: Option<OrCreate>) -> Query<Result<BinaryFile>>;
    fn read_string(&self, path: Path, or_create: Option<OrCreate>) -> Query<Result<SourceFile>>;
    fn read_metadata(&self, path: Path, or_create: Option<OrCreate>) -> Query<Result<Metadata>>;
}

pub fn read_file<P: Into<Path>>(path: P) -> Query<Result<BinaryFile>> {
    vfs(|fs| fs.read_file(path.into(), None))
}

pub fn read_file_or_create<P: Into<Path>>(
    path: P,
    or_create: OrCreate,
) -> Query<Result<BinaryFile>> {
    vfs(|fs| fs.read_file(path.into(), Some(or_create)))
}

pub fn read_string<P: Into<Path>>(path: P) -> Query<Result<SourceFile>> {
    vfs(|fs| fs.read_string(path.into(), None))
}

pub fn read_string_or_create<P: Into<Path>>(
    path: P,
    or_create: OrCreate,
) -> Query<Result<SourceFile>> {
    vfs(|fs| fs.read_string(path.into(), Some(or_create)))
}

pub fn read_dir<P: Into<Path>>(path: P) -> Query<Result<Directory>> {
    vfs(|fs| fs.read_dir(path.into()))
}

pub fn read_metadata<P: Into<Path>>(path: P) -> Query<Result<Metadata>> {
    vfs(|fs| fs.read_metadata(path.into(), None))
}

pub fn read_metadata_or_create<P: Into<Path>>(
    path: P,
    or_create: OrCreate,
) -> Query<Result<Metadata>> {
    vfs(|fs| fs.read_metadata(path.into(), Some(or_create)))
}

#[derive(Clone, Debug)]
pub struct Metadata {
    modified_time: Result<SystemTime>,
    file_type: FileType,
}

impl Metadata {
    pub fn is_dir(&self) -> bool {
        self.file_type.is_dir()
    }

    pub fn is_file(&self) -> bool {
        self.file_type.is_file()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    #[tokio::test]
    async fn self_read() {
        let file = read_string(file!()).await;

        if let Ok(contents) = file {
            assert!(contents.contains("THIS IS A REALLY UNIQUE STRING"));
        }
    }

    #[tokio::test]
    async fn walk() {
        let dir = read_dir(env!("CARGO_MANIFEST_DIR")).await;

        if let Ok(dir) = dir {
            let glob = "**/*.rs".parse().unwrap();
            let ignore = "__IGNORE__".parse().unwrap();
            let dir = dir.glob(glob, ignore);
            tokio::pin!(dir);

            while let Some(path) = dir.next().await {
                dbg!(path);
            }
        }

        // TODO make some assertions
    }
}
