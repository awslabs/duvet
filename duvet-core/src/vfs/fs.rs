// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::{
    cache::Cache,
    contents::Contents,
    diagnostic::{self, IntoDiagnostic},
    query::Query,
};
use miette::Context;
use tokio::{fs, io::AsyncReadExt, sync::Semaphore};

#[cfg(target_os = "linux")]
/// Linux usually defaults to 1024 but we lower it just in case it's lower
const MAX_CONCURRENCY: usize = 256;

#[cfg(not(target_os = "linux"))]
/// If it's not Linux, assume the `ulimit -n` is low.
///
/// For example, macOS seems to default to 256.
const MAX_CONCURRENCY: usize = 64;

/// Limits the number of open files
static CONCURRENCY: Semaphore = Semaphore::const_new(MAX_CONCURRENCY);

#[derive(Clone, Default)]
pub struct Fs(());

impl Fs {
    pub fn setup_thread(&self) {
        super::setup(self.clone());
    }
}

impl Vfs for Fs {
    fn read_dir(&self, path: Path) -> Query<Result<Directory>> {
        let fs = self.clone();
        let cache = Cache::current();
        cache.clone().get_or_init_tmp(path.clone(), || {
            Query::delegate(async move {
                let metadata = fs.read_metadata(path.clone(), None);
                let metadata = match metadata.get().await.as_ref() {
                    Ok(v) => v,
                    Err(e) => return Query::from(Err(e.clone())),
                };

                let modified_time = metadata.modified_time.as_ref().ok().copied();

                cache.get_or_init((path.clone(), modified_time), || {
                    Query::new(async move {
                        let mut dir = fs::read_dir(&path)
                            .await
                            .into_diagnostic()
                            .wrap_err_with(|| path.clone())?;

                        let mut contents = vec![];
                        while let Some(Ok(entry)) = dir.next_entry().await.transpose() {
                            contents.push(Path::from(entry.path()));
                        }

                        Ok(Directory {
                            path,
                            contents: contents.into(),
                        })
                    })
                })
            })
        })
    }

    fn read_file(&self, path: Path, or_create: Option<OrCreate>) -> Query<Result<BinaryFile>> {
        let fs = self.clone();
        let cache = Cache::current();
        cache.clone().get_or_init_tmp(path.clone(), || {
            Query::delegate(async move {
                let metadata = fs.read_metadata(path.clone(), or_create);
                let metadata = match metadata.get().await.as_ref() {
                    Ok(v) => v,
                    Err(e) => return Query::from(Err(e.clone())),
                };

                let modified_time = metadata.modified_time.as_ref().ok().copied();

                cache.get_or_init((path.clone(), modified_time), || {
                    Query::spawn(async move {
                        let concurrency = CONCURRENCY.acquire().await.unwrap();

                        let mut file = fs::OpenOptions::new()
                            .read(true)
                            .open(&path)
                            .await
                            .into_diagnostic()
                            .wrap_err_with(|| path.clone())?;

                        let mut data = vec![];
                        file.read_to_end(&mut data)
                            .await
                            .into_diagnostic()
                            .wrap_err_with(|| path.clone())?;

                        drop(file);
                        drop(concurrency);

                        let contents = Contents::from(data);

                        Result::<_, diagnostic::Error>::Ok(BinaryFile { path, contents })
                    })
                })
            })
        })
    }

    fn read_string(&self, path: Path, or_create: Option<OrCreate>) -> Query<Result<SourceFile>> {
        let fs = self.clone();
        let cache = Cache::current();
        cache.clone().get_or_init_tmp(path.clone(), || {
            Query::new(async move {
                let file = fs.read_file(path.clone(), or_create).get_cloned().await?;

                let contents = file.contents.clone();
                cache
                    .get_or_init(*contents.hash(), || {
                        Query::spawn(async move { core::str::from_utf8(&contents).map(|_| ()) })
                    })
                    .get_cloned()
                    .await
                    .into_diagnostic()
                    .wrap_err_with(|| path.clone())?;

                Ok(SourceFile {
                    path,
                    contents: file.contents,
                })
            })
        })
    }

    fn read_metadata(&self, path: Path, or_create: Option<OrCreate>) -> Query<Result<Metadata>> {
        Cache::current().get_or_init_tmp(path.clone(), || {
            Query::new(async move {
                let mut meta = fs::metadata(&path).await;

                if let Err(err) = meta.as_ref() {
                    if err.kind() == std::io::ErrorKind::NotFound {
                        if let Some(or_create) = or_create {
                            let contents = or_create
                                .await
                                .into_diagnostic()
                                .wrap_err_with(|| path.clone())?;

                            if let Some(parent) = path.parent() {
                                fs::create_dir_all(parent)
                                    .await
                                    .into_diagnostic()
                                    .wrap_err_with(|| path.clone())?;
                            }

                            fs::write(&path, &contents)
                                .await
                                .into_diagnostic()
                                .wrap_err_with(|| path.clone())?;

                            meta = fs::metadata(&path).await;
                        }
                    }
                }

                let meta = meta.into_diagnostic().wrap_err_with(|| path.clone())?;

                Ok(Metadata {
                    modified_time: meta.modified().into_diagnostic().map_err(From::from),
                    file_type: meta.file_type(),
                })
            })
        })
    }
}
