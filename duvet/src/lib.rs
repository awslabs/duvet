// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use std::sync::Arc;

mod annotation;
mod comment;
mod extract;
mod project;
mod report;
mod source;
mod specification;
mod target;
mod text;

#[cfg(test)]
mod tests;

pub use duvet_core::{diagnostic::Error, Result};

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Parser)]
pub enum Arguments {
    Extract(extract::Extract),
    Report(report::Report),
}

#[duvet_core::query(cache)]
pub async fn arguments() -> Arc<Arguments> {
    Arc::new(Arguments::parse())
}

impl Arguments {
    pub async fn exec(&self) -> Result {
        match self {
            Self::Extract(args) => args.exec().await,
            Self::Report(args) => args.exec().await,
        }
    }
}

pub async fn run() -> Result {
    arguments().await.exec().await?;
    Ok(())
}

pub(crate) fn fnv<H: core::hash::Hash + ?Sized>(value: &H) -> u64 {
    use core::hash::Hasher;
    let mut hasher = fnv::FnvHasher::default();
    value.hash(&mut hasher);
    hasher.finish()
}
