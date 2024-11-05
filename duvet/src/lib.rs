// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Error;
use duvet_core::Result;

mod annotation;
mod arguments;
mod comment;
mod config;
mod extract;
mod manifest;
mod project;
mod report;
mod source;
mod sourcemap;
mod specification;
mod target;
mod text;

#[cfg(test)]
mod tests;

pub async fn run() -> Result {
    arguments::get().await.exec().await?;
    Ok(())
}

pub(crate) fn fnv<H: core::hash::Hash + ?Sized>(value: &H) -> u64 {
    use core::hash::Hasher;
    let mut hasher = fnv::FnvHasher::default();
    value.hash(&mut hasher);
    hasher.finish()
}
