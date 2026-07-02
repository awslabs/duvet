// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! A library entry point for running duvet's coverage checks and getting an
//! *inspectable* result back, rather than writing reports to disk and exiting
//! the process.
//!
//! This is what the `duvet-wasm` component (and any other embedder) calls: it
//! runs the same pipeline as `duvet report`, then returns the full json_v2
//! report plus a derived pass/fail verdict. Because the report itself carries
//! every requirement's citations/tests/status, callers can inspect *why* a run
//! passed or failed instead of only seeing an exit code.

use crate::{
    project::Project,
    report::{self, ci, json_v2},
    Result,
};

pub use crate::report::ci::{Violation, ViolationKind};

/// The outcome of a checks run.
#[derive(Debug)]
pub struct CheckReport {
    /// Whether the coverage requirements were satisfied for every target
    /// (i.e. what `duvet report --ci` would treat as success). Equivalent to
    /// `violations.is_empty()`.
    pub ok: bool,
    /// The full v2 JSON report — the inspectable record of every requirement,
    /// citation, test, and status that produced `ok`.
    pub report_json: String,
    /// Every coverage violation found, across all targets — not just the
    /// first. Empty when `ok` is `true`.
    pub violations: Vec<Violation>,
}

/// Options for a checks run.
#[derive(Debug, Clone)]
pub struct CheckOptions {
    pub require_citations: bool,
    pub require_tests: bool,
}

impl Default for CheckOptions {
    fn default() -> Self {
        // Mirror the CLI defaults (see `Report::require_citations`/`require_tests`).
        Self {
            require_citations: true,
            require_tests: true,
        }
    }
}

/// Runs the coverage checks for the project rooted at the current
/// [`duvet_core::env::current_dir`], reading everything through the
/// [`duvet_core::vfs`] seam.
///
/// The caller is responsible for installing a [`Vfs`](duvet_core::vfs::Vfs)
/// (e.g. an in-memory one populated with the project files) and setting the
/// working directory via [`duvet_core::env::set_current_dir`] before calling
/// this.
pub async fn check(options: CheckOptions) -> Result<CheckReport> {
    let project = Project::default();

    let report = report::analyze(
        &project,
        options.require_citations,
        options.require_tests,
        None,
        None,
    )
    .await?;

    // Enumerate *every* coverage violation (not just the first, as the CLI's
    // `ci::report` does) so the caller can inspect exactly what failed.
    let violations = ci::violations(&report);
    let ok = violations.is_empty();

    let report_v2 = json_v2::ReportV2::from_report_result(&report);
    let mut report_json = vec![];
    json_v2::write_report_v2_to_writer(&report_v2, &mut report_json)?;
    let report_json = String::from_utf8(report_json)
        .map_err(|e| duvet_core::error!("report was not valid UTF-8: {e}"))?;

    Ok(CheckReport {
        ok,
        report_json,
        violations,
    })
}
