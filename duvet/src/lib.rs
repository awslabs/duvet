// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use std::sync::Arc;

mod annotation;
mod comment;
mod config;
mod convert;
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
    /// Converts a v2 JSON report to the legacy v1 JSON format
    #[command(hide = true)]
    Convert(convert::Convert),
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
            Self::Convert(args) => args.exec().await,
            Self::Merge(args) => args.exec().await,
        }
    }
}

pub async fn run() -> Result {
    arguments().await.exec().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::Arguments;
    use clap::{CommandFactory, Parser};

    #[test]
    fn convert_is_hidden_but_parseable() {
        let help = Arguments::command().render_long_help().to_string();
        assert!(!help.contains("convert"));

        let arguments = Arguments::try_parse_from([
            "duvet",
            "convert",
            "--input",
            "report-v2.json",
            "--output",
            "report-v1.json",
        ])
        .unwrap();
        assert!(matches!(arguments, Arguments::Convert(_)));
    }
}
