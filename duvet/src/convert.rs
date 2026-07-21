// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! `duvet convert` subcommand: convert a v2 JSON report to legacy v1 JSON.

use crate::{
    report::{json_v1, json_v2, json_v2_to_v1},
    Result,
};
use clap::Parser;
use duvet_core::path::Path;

#[derive(Debug, Parser)]
pub struct Convert {
    /// Input v2 JSON report.
    #[clap(long)]
    input: Path,

    /// Output v1 JSON report.
    #[clap(long)]
    output: Path,

    /// Override the issue tracker link in the converted report.
    #[clap(long)]
    issue_link: Option<String>,

    /// Compare the converted report with a directly generated v1 report.
    #[clap(long)]
    validate_against: Option<Path>,
}

impl Convert {
    pub async fn exec(&self) -> Result {
        let input = json_v2::read_report_v2(self.input.as_ref())?;
        let compare_issue_link = self.issue_link.is_some() || input.issue_links.len() <= 1;
        let (converted, warning) = json_v2_to_v1::convert(&input, self.issue_link.as_deref())?;

        if let Some(warning) = warning {
            eprintln!("warning: {warning}");
        }

        json_v1::write(&converted, self.output.as_ref())?;

        if let Some(path) = &self.validate_against {
            let direct = json_v1::read(path.as_ref())?;
            json_v2_to_v1::validate_semantics(&direct, &converted, compare_issue_link)?;
        }

        Ok(())
    }
}
