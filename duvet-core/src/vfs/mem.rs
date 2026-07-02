// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! An in-memory [`Vfs`] backend.
//!
//! This backend keeps every file in a shared `HashMap` rather than touching an
//! ambient filesystem, which makes it suitable for sandboxed / wasm component
//! environments where the host stages the project files in and reads any
//! written outputs back out. Directories are implicit: a directory "exists" if
//! any file has it as a prefix.
//!
//! Unlike the native [`Fs`](super::Fs) backend — which keys cached reads on the
//! file's modified time — this backend has no clock or filesystem metadata, so
//! reads are keyed on the content [`hash`](crate::contents::Contents::hash).

use super::{Directory, Metadata, OrCreate};
use crate::{
    contents::Contents,
    env, error,
    file::{BinaryFile, SourceFile},
    path::Path,
    query::Query,
    Result,
};
use rustc_hash::FxHashMap;
use std::{
    path::{Component, PathBuf},
    sync::{Arc, RwLock},
};

/// Resolves `path` against the current working directory (if relative) and
/// lexically normalizes it (collapsing `.`/`..`), so that the different ways
/// duvet spells the same file — `my-spec.md`, `./my-spec.md`,
/// `/project/my-spec.md` — all map to a single storage key.
///
/// This mirrors how the native filesystem resolves relative paths against the
/// process cwd; the in-memory backend has no ambient cwd, so it uses
/// [`env::current_dir`] instead.
fn normalize(path: &Path) -> Path {
    let base = if path.is_absolute() {
        PathBuf::new()
    } else if let Ok(cwd) = env::current_dir() {
        cwd.into()
    } else {
        PathBuf::new()
    };

    let joined = base.join(path.as_ref());

    let mut out = PathBuf::new();
    for component in joined.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    Path::from(out)
}

/// An in-memory filesystem.
///
/// Cloning is cheap and shares the same underlying storage, so writes made
/// through one handle are visible through its clones (mirroring how the native
/// [`Fs`](super::Fs) handle shares the real filesystem).
#[derive(Clone, Default)]
pub struct Mem {
    files: Arc<RwLock<FxHashMap<Path, Contents>>>,
}

impl Mem {
    /// Creates an empty in-memory filesystem.
    pub fn new() -> Self {
        Self::default()
    }

    /// Installs this filesystem as the current thread's [`Vfs`](super::Vfs).
    pub fn setup_thread(&self) {
        super::setup(self.clone());
    }

    /// Inserts a file, replacing any existing contents at `path`.
    pub fn insert<P: Into<Path>, C: Into<Contents>>(&self, path: P, contents: C) {
        let path = normalize(&path.into());
        self.files.write().unwrap().insert(path, contents.into());
    }

    /// Returns every `(path, contents)` pair currently stored, sorted by path.
    ///
    /// Hosts use this to read back outputs a run produced (reports, extracted
    /// requirements, etc.).
    pub fn snapshot(&self) -> Vec<(Path, Contents)> {
        let files = self.files.read().unwrap();
        let mut out: Vec<_> = files
            .iter()
            .map(|(path, contents)| (path.clone(), contents.clone()))
            .collect();
        out.sort_by(|(a, _), (b, _)| a.cmp(b));
        out
    }

    fn get(&self, path: &Path) -> Option<Contents> {
        let path = normalize(path);
        self.files.read().unwrap().get(&path).cloned()
    }

    /// Returns `true` if any stored file lives (directly or transitively) under
    /// `path` — i.e. `path` behaves like a directory.
    fn is_dir(&self, path: &Path) -> bool {
        let path = normalize(path);
        let files = self.files.read().unwrap();
        files.keys().any(|p| *p != path && p.starts_with(&path))
    }
}

impl super::Vfs for Mem {
    fn read_dir(&self, path: Path) -> Query<Result<Directory>> {
        let path = normalize(&path);
        let mut contents = vec![];
        {
            let files = self.files.read().unwrap();
            for file in files.keys() {
                // yield the immediate children of `path`
                if let Ok(rest) = file.strip_prefix(&path) {
                    if let Some(first) = rest.components().next() {
                        let child = path.join(first.as_os_str());
                        if !contents.contains(&child) {
                            contents.push(child);
                        }
                    }
                }
            }
        }
        contents.sort();
        Query::from(Ok(Directory {
            path,
            contents: contents.into(),
        }))
    }

    fn read_file(&self, path: Path, or_create: Option<OrCreate>) -> Query<Result<BinaryFile>> {
        if let Some(contents) = self.get(&path) {
            return Query::from(Ok(BinaryFile::new(path, contents)));
        }

        let Some(or_create) = or_create else {
            return Query::from(Err(error!("file not found: {path}")));
        };

        let this = self.clone();
        Query::new(async move {
            let contents = or_create.await?;
            this.insert(path.clone(), contents.clone());
            Ok(BinaryFile::new(path, contents))
        })
    }

    fn read_string(&self, path: Path, or_create: Option<OrCreate>) -> Query<Result<SourceFile>> {
        if let Some(contents) = self.get(&path) {
            return Query::from(SourceFile::new(path, contents));
        }

        let Some(or_create) = or_create else {
            return Query::from(Err(error!("file not found: {path}")));
        };

        let this = self.clone();
        Query::new(async move {
            let contents = or_create.await?;
            this.insert(path.clone(), contents.clone());
            SourceFile::new(path, contents)
        })
    }

    fn read_metadata(&self, path: Path, or_create: Option<OrCreate>) -> Query<Result<Metadata>> {
        if self.get(&path).is_some() {
            let modified_time = Err(error!("in-memory files have no modified time"));
            return Query::from(Ok(Metadata::new(false, true, modified_time)));
        }

        if self.is_dir(&path) {
            let modified_time = Err(error!("in-memory files have no modified time"));
            return Query::from(Ok(Metadata::new(true, false, modified_time)));
        }

        let Some(or_create) = or_create else {
            return Query::from(Err(error!("file not found: {path}")));
        };

        let this = self.clone();
        Query::new(async move {
            let contents = or_create.await?;
            this.insert(path.clone(), contents);
            Ok(Metadata::new(
                false,
                true,
                Err(error!("in-memory files have no modified time")),
            ))
        })
    }

    fn read_sync(&self, path: Path) -> Result<Contents> {
        self.get(&path)
            .ok_or_else(|| error!("file not found: {path}"))
    }

    fn write_file(&self, path: Path, contents: Contents) -> Result {
        self.insert(path, contents);
        Ok(())
    }

    fn create_dir_all(&self, _path: Path) -> Result {
        // Directories are implicit in the in-memory backend.
        Ok(())
    }

    fn exists(&self, path: Path) -> bool {
        self.get(&path).is_some() || self.is_dir(&path)
    }

    fn is_file(&self, path: Path) -> bool {
        self.get(&path).is_some()
    }
}
