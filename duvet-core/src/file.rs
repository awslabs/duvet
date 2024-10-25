// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::{contents::Contents, diagnostic::IntoDiagnostic, path::Path, Result};
use core::{
    fmt,
    ops::{Deref, Range},
};
use miette::{SourceCode, WrapErr};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BinaryFile {
    pub(crate) path: Path,
    pub(crate) contents: Contents,
}

impl BinaryFile {
    pub fn new<P, C>(path: P, contents: C) -> Self
    where
        P: Into<Path>,
        C: Into<Contents>,
    {
        let path = path.into();
        let contents = contents.into();
        Self { path, contents }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn hash(&self) -> &[u8; 32] {
        self.contents.hash()
    }
}

impl Deref for BinaryFile {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.contents
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceFile {
    pub(crate) path: Path,
    pub(crate) contents: Contents,
}

impl SourceFile {
    pub fn new<P, C>(path: P, contents: C) -> Result<Self>
    where
        P: Into<Path>,
        C: Into<Contents>,
    {
        let path = path.into();
        let contents = contents.into();
        let _ = core::str::from_utf8(&contents)
            .into_diagnostic()
            .wrap_err_with(|| path.clone())?;
        Ok(Self { path, contents })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn hash(&self) -> &[u8; 32] {
        self.contents.hash()
    }

    pub async fn as_toml<T>(&self) -> crate::Result<Arc<T>>
    where
        T: 'static + Send + Sync + serde::de::DeserializeOwned,
    {
        let path = self.path.clone();
        let contents = self.contents.clone();
        // TODO can we get better errors by mapping string ranges?
        crate::Cache::current()
            .get_or_init(*self.hash(), move || {
                crate::Query::from(
                    toml_edit::de::from_slice(contents.data())
                        .map(Arc::new)
                        .into_diagnostic()
                        .wrap_err(path)
                        .map_err(|err| err.into()),
                )
            })
            .await
    }

    pub async fn as_json<T>(&self) -> crate::Result<Arc<T>>
    where
        T: 'static + Send + Sync + serde::de::DeserializeOwned,
    {
        let path = self.path.clone();
        let contents = self.contents.clone();
        // TODO can we get better errors by mapping string ranges?
        crate::Cache::current()
            .get_or_init(*self.hash(), move || {
                crate::Query::from(
                    serde_json::from_slice(contents.data())
                        .map(Arc::new)
                        .into_diagnostic()
                        .wrap_err(path)
                        .map_err(|err| err.into()),
                )
            })
            .await
    }

    pub fn substr(&self, v: &str) -> Option<Slice<SourceFile>> {
        unsafe {
            let beginning = self.as_bytes().as_ptr();
            let end = beginning.add(self.as_bytes().len());

            if !(beginning..=end).contains(&v.as_ptr()) {
                return None;
            }

            Some(self.substr_unchecked(v))
        }
    }

    /// # Safety
    ///
    /// Callers should ensure that the `v` is a slice of `self`
    pub unsafe fn substr_unchecked(&self, v: &str) -> Slice<SourceFile> {
        let start = v.as_bytes().as_ptr() as usize - self.as_bytes().as_ptr() as usize;
        Slice {
            file: self.clone(),
            start,
            end: start + v.len(),
        }
    }
}

impl Deref for SourceFile {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        unsafe {
            // Safety: this validity was checked at creation on SourceFile
            core::str::from_utf8_unchecked(&self.contents)
        }
    }
}

impl AsRef<str> for SourceFile {
    fn as_ref(&self) -> &str {
        self
    }
}

impl SourceCode for SourceFile {
    fn read_span<'a>(
        &'a self,
        span: &miette::SourceSpan,
        context_lines_before: usize,
        context_lines_after: usize,
    ) -> Result<Box<dyn miette::SpanContents<'a> + 'a>, miette::MietteError> {
        use miette::MietteSpanContents;

        let contents = (**self).read_span(span, context_lines_before, context_lines_after)?;

        let path = std::env::current_dir()
            .ok()
            .and_then(|cwd| self.path.strip_prefix(cwd).ok())
            .unwrap_or(&self.path)
            .display()
            .to_string();

        Ok(Box::new(MietteSpanContents::new_named(
            path,
            contents.data(),
            *contents.span(),
            contents.line(),
            contents.column(),
            contents.line_count(),
        )))
    }
}

#[derive(Clone, PartialEq, PartialOrd, Hash, Eq, Ord)]
pub struct Slice<File = SourceFile> {
    file: File,
    start: usize,
    end: usize,
}

impl<F: File> Slice<F> {
    pub fn path(&self) -> &Path {
        self.file.path()
    }

    pub fn file(&self) -> &F {
        &self.file
    }

    pub fn range(&self) -> Range<usize> {
        self.start..self.end
    }
}

impl Slice<SourceFile> {
    pub fn error<E>(&self, e: E, label: impl AsRef<str>) -> crate::diagnostic::Error
    where
        E: Into<crate::diagnostic::Error>,
    {
        e.into().with_source_slice(self.clone(), label)
    }
}

impl fmt::Debug for Slice<BinaryFile> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self[..].fmt(f)
    }
}

impl fmt::Debug for Slice<SourceFile> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self[..].fmt(f)
    }
}

impl fmt::Display for Slice<SourceFile> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self[..].fmt(f)
    }
}

impl Deref for Slice<BinaryFile> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        unsafe {
            // Safety: range validatity was checked at slice creation time
            self.file.get_unchecked(self.start..self.end)
        }
    }
}

impl AsRef<[u8]> for Slice<BinaryFile> {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        self
    }
}

impl Deref for Slice<SourceFile> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        unsafe {
            // Safety: range validatity was checked at slice creation time
            self.file.get_unchecked(self.start..self.end)
        }
    }
}

impl AsRef<str> for Slice<SourceFile> {
    #[inline]
    fn as_ref(&self) -> &str {
        self
    }
}

impl PartialEq<[u8]> for Slice<BinaryFile> {
    fn eq(&self, other: &[u8]) -> bool {
        (**self).eq(other)
    }
}

impl PartialEq<[u8]> for Slice<SourceFile> {
    fn eq(&self, other: &[u8]) -> bool {
        (**self).as_bytes().eq(other)
    }
}

impl PartialEq<str> for Slice<SourceFile> {
    fn eq(&self, other: &str) -> bool {
        (**self).eq(other)
    }
}

pub trait File {
    fn path(&self) -> &Path;
}

impl File for SourceFile {
    fn path(&self) -> &Path {
        &self.path
    }
}

impl File for BinaryFile {
    fn path(&self) -> &Path {
        &self.path
    }
}
