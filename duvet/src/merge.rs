// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! `duvet merge` subcommand: combine multiple v2 JSON reports into one.
//!
//! Each input must be a v2 JSON report produced by `duvet report --json-v2`
//! (or by a previous merge). The merge is purely deterministic — entity IDs
//! are content hashes, so the same input pair produces the same output
//! regardless of input order.

use crate::{
    report::{json_v2, json_v2_merge},
    Result,
};
use clap::Parser;
use duvet_core::path::Path;

#[derive(Debug, Parser)]
pub struct Merge {
    /// One or more v2 JSON reports to merge. Repeat the flag per input.
    #[clap(long, required = true)]
    input: Vec<Path>,

    /// Output path for the merged v2 JSON report.
    #[clap(long)]
    output: Path,
}

impl Merge {
    pub async fn exec(&self) -> Result {
        let mut reports = Vec::with_capacity(self.input.len());
        for path in &self.input {
            let report = json_v2::read_report_v2(path.as_ref())?;
            reports.push(report);
        }

        let merged = json_v2_merge::merge_reports(reports)?;
        json_v2::write_report_v2(&merged, self.output.as_ref())?;

        Ok(())
    }
}
