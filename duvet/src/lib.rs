// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use std::sync::Arc;

mod annotation;
pub mod api;
mod comment;
mod config;
mod extract;
pub(crate) mod ids;
mod init;
mod merge;
mod project;
mod reference;
mod report;
mod source;
mod specification;
mod target;
mod text;

pub use duvet_core::{diagnostic::Error, Result};

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Parser)]
pub enum Arguments {
    /// Initializes a duvet project
    Init(init::Init),
    /// Extracts requirements out of a specification
    Extract(extract::Extract),
    /// Generates reports for the project
    Report(report::Report),
    /// Merges multiple v2 JSON reports into one
    Merge(merge::Merge),
}

#[duvet_core::query(cache)]
pub async fn arguments() -> Arc<Arguments> {
    Arc::new(Arguments::parse())
}

impl Arguments {
    pub async fn exec(&self) -> Result {
        match self {
            Self::Init(args) => args.exec().await,
            Self::Extract(args) => args.exec().await,
            Self::Report(args) => args.exec().await,
            Self::Merge(args) => args.exec().await,
        }
    }
}

pub async fn run() -> Result {
    arguments().await.exec().await?;
    Ok(())
}
