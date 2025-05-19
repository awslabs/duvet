// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use std::sync::Arc;

mod annotation;
mod comment;
mod config;
mod extract;
mod init;
mod mcp;
mod project;
mod reference;
mod report;
mod source;
mod specification;
mod target;
mod text;
pub mod tracing;

pub use duvet_core::{Result, diagnostic::Error};

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Parser)]
pub enum Arguments {
    /// Initializes a duvet project
    Init(init::Init),
    /// Extracts requirements out of a specification
    Extract(extract::Extract),
    /// Generates reports for the project
    Report(report::Report),
    /// Starts the MCP server for AI model interaction
    Mcp(mcp::Mcp),
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
            Self::Mcp(args) => args.execute().await,
        }
    }
}

pub async fn run() -> Result {
    arguments().await.exec().await?;
    Ok(())
}
