// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    contents::Contents,
    diagnostic::IntoDiagnostic,
    file::{BinaryFile, SourceFile},
    path::Path,
    Cache, Query, Result,
};
use std::sync::Arc;

pub use http::response::Parts;
pub use reqwest::Client;

fn default_headers() -> reqwest::header::HeaderMap {
    let mut map = reqwest::header::HeaderMap::new();

    map.insert("accept", "text/plain".parse().unwrap());

    map
}

pub fn client() -> Query<Client> {
    #[derive(Clone, Copy, Debug, Hash, Eq, PartialEq)]
    struct Q;

    Cache::current().get_or_init(Q, || {
        Query::from(
            Client::builder()
                .user_agent(concat!(
                    env!("CARGO_PKG_NAME"),
                    "/",
                    env!("CARGO_PKG_VERSION")
                ))
                .default_headers(default_headers())
                .build()
                .unwrap(),
        )
    })
}

pub fn get_full<U>(url: U) -> Query<Result<(Arc<Parts>, Contents)>>
where
    U: 'static + Clone + AsRef<str> + Send + Sync,
{
    Cache::current().get_or_init(url.as_ref().to_string(), move || {
        Query::new(async move {
            let client = client().await;

            let resp = client.get(url.as_ref()).send().await.into_diagnostic()?;
            let mut resp = resp.error_for_status().into_diagnostic()?;

            let mut body = vec![];

            while let Some(chunk) = resp.chunk().await.into_diagnostic()? {
                body.extend_from_slice(&chunk);
            }

            let resp: http::Response<reqwest::Body> = resp.into();

            let (headers, _) = resp.into_parts();

            let headers = Arc::new(headers);
            let body = Contents::from(body);

            Ok((headers, body))
        })
    })
}

pub fn get<U>(url: U) -> Query<Result<Contents>>
where
    U: 'static + Clone + AsRef<str> + Send + Sync,
{
    Query::new(async move {
        let resp = get_full(url).await?;
        Ok(resp.1)
    })
}

pub fn get_cached<U, P>(url: U, cached_path: P) -> Query<Result<BinaryFile>>
where
    U: 'static + Clone + AsRef<str> + Send + Sync,
    P: Into<Path>,
{
    crate::vfs::read_file_or_create(cached_path, get(url))
}

pub fn get_cached_string<U, P>(url: U, cached_path: P) -> Query<Result<SourceFile>>
where
    U: 'static + Clone + AsRef<str> + Send + Sync,
    P: Into<Path>,
{
    crate::vfs::read_string_or_create(cached_path, get(url))
}
