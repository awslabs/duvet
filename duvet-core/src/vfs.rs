// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    contents::Contents,
    dir::Directory,
    file::{BinaryFile, SourceFile},
    path::Path,
    query::Query,
    Result,
};
use core::cell::RefCell;
use std::time::SystemTime;

#[cfg(not(target_family = "wasm"))]
pub mod fs;
#[cfg(not(target_family = "wasm"))]
pub use fs::Fs;

pub mod mem;
pub use mem::Mem;

pub type OrCreate = Query<Result<crate::contents::Contents>>;

/// The default filesystem backend for the current target.
///
/// Native targets use the real filesystem ([`Fs`]); wasm targets have no
/// ambient filesystem, so they default to an empty in-memory filesystem
/// ([`Mem`]) that the host populates via [`setup`].
#[cfg(not(target_family = "wasm"))]
fn default_vfs() -> Box<dyn Vfs + 'static> {
    Box::new(Fs::default())
}

#[cfg(target_family = "wasm")]
fn default_vfs() -> Box<dyn Vfs + 'static> {
    Box::new(Mem::default())
}

thread_local! {
    static VFS: RefCell<Box<dyn Vfs + 'static>> = RefCell::new(default_vfs());
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

    /// Synchronously reads the raw contents of `path`.
    ///
    /// This is the counterpart to [`Vfs::write_file`] for the synchronous
    /// report/serialization boundary, where the surrounding async runtime
    /// cannot be re-entered.
    fn read_sync(&self, path: Path) -> Result<Contents>;

    /// Writes `contents` to `path`, creating any missing parent directories.
    fn write_file(&self, path: Path, contents: Contents) -> Result;

    /// Creates `path` and all of its parent directories.
    fn create_dir_all(&self, path: Path) -> Result;

    /// Returns `true` if `path` exists in the filesystem.
    fn exists(&self, path: Path) -> bool;

    /// Returns `true` if `path` exists and is a regular file.
    fn is_file(&self, path: Path) -> bool;
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

pub fn read_sync<P: Into<Path>>(path: P) -> Result<Contents> {
    vfs(|fs| fs.read_sync(path.into()))
}

pub fn write<P: Into<Path>, C: Into<Contents>>(path: P, contents: C) -> Result {
    vfs(|fs| fs.write_file(path.into(), contents.into()))
}

pub fn create_dir_all<P: Into<Path>>(path: P) -> Result {
    vfs(|fs| fs.create_dir_all(path.into()))
}

pub fn exists<P: Into<Path>>(path: P) -> bool {
    vfs(|fs| fs.exists(path.into()))
}

pub fn is_file<P: Into<Path>>(path: P) -> bool {
    vfs(|fs| fs.is_file(path.into()))
}

#[derive(Clone, Debug)]
pub struct Metadata {
    modified_time: Result<SystemTime>,
    is_dir: bool,
    is_file: bool,
}

impl Metadata {
    /// Builds metadata from raw parts, for filesystem backends that don't
    /// expose a [`std::fs::FileType`] (e.g. the in-memory [`Mem`] backend).
    pub fn new(is_dir: bool, is_file: bool, modified_time: Result<SystemTime>) -> Self {
        Self {
            modified_time,
            is_dir,
            is_file,
        }
    }

    pub fn is_dir(&self) -> bool {
        self.is_dir
    }

    pub fn is_file(&self) -> bool {
        self.is_file
    }

    /// The file's last-modified time, if the backend tracks it.
    ///
    /// The native [`Fs`] backend uses this as a cache-invalidation key; the
    /// in-memory [`Mem`] backend has no clock and returns an error here.
    pub fn modified_time(&self) -> &Result<SystemTime> {
        &self.modified_time
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
