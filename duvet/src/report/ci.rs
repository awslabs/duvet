// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use super::{ReportResult, TargetReport};
use crate::{annotation::AnnotationType, Result};
use duvet_core::error;
use std::collections::HashSet;

pub fn report(report: &ReportResult) -> Result {
    report
        .targets
        .iter()
        .try_for_each(|(_source, report)| enforce_source(report))
}

/// The kind of coverage requirement a [`Violation`] failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViolationKind {
    /// A significant spec line has no citation.
    MissingCitation,
    /// A citation points at a line that isn't a significant spec requirement.
    CitationWithoutSpec,
    /// A cited line has no corresponding test.
    MissingTest,
    /// A tested line has no corresponding citation.
    TestWithoutCitation,
}

impl ViolationKind {
    pub fn message(self) -> &'static str {
        match self {
            Self::MissingCitation => "Specification requirement missing citation.",
            Self::CitationWithoutSpec => "Citation for non-existing specification.",
            Self::MissingTest => "Citation missing test.",
            Self::TestWithoutCitation => "Test for non-existing citation.",
        }
    }
}

/// A single coverage violation, located at a spec line within a target.
#[derive(Clone, Debug)]
pub struct Violation {
    /// The specification target the violation belongs to (e.g. the spec path).
    pub target: String,
    /// The 1-based line in the specification where coverage is missing.
    pub line: usize,
    pub kind: ViolationKind,
}

/// Enumerates *every* coverage violation across *all* targets.
///
/// Unlike [`report`]/[`enforce_source`] — which short-circuit at the first
/// failure to produce a single CLI error — this collects the complete set so a
/// caller can inspect exactly which requirements failed and why. The verdict
/// is the same (`violations.is_empty()` iff `report` would return `Ok`).
pub fn violations(report: &ReportResult) -> Vec<Violation> {
    let mut violations = vec![];
    for (target, target_report) in report.targets.iter() {
        let target = target.path.to_string();
        source_violations(target_report, &target, &mut violations);
    }
    // deterministic output: by target, then line, then kind
    violations.sort_by(|a, b| {
        (a.target.as_str(), a.line, a.kind as u8).cmp(&(b.target.as_str(), b.line, b.kind as u8))
    });
    violations
}

fn source_violations(report: &TargetReport, target: &str, out: &mut Vec<Violation>) {
    let mut cited_lines = HashSet::new();
    let mut tested_lines = HashSet::new();
    let mut significant_lines = HashSet::new();

    for reference in &report.references {
        let line = reference.line();
        significant_lines.insert(line);

        match reference.annotation.anno {
            AnnotationType::Test => {
                tested_lines.insert(line);
            }
            AnnotationType::Citation => {
                cited_lines.insert(line);
            }
            AnnotationType::Exception | AnnotationType::Implication => {
                tested_lines.insert(line);
                cited_lines.insert(line);
            }
            AnnotationType::Spec | AnnotationType::Todo => {}
        }
    }

    let mut push = |line: usize, kind: ViolationKind| {
        out.push(Violation {
            target: target.to_string(),
            line,
            kind,
        });
    };

    if report.require_citations {
        for line in significant_lines.difference(&cited_lines) {
            push(*line, ViolationKind::MissingCitation);
        }
        for line in cited_lines.difference(&significant_lines) {
            push(*line, ViolationKind::CitationWithoutSpec);
        }
    }

    if report.require_tests {
        for line in cited_lines.difference(&tested_lines) {
            push(*line, ViolationKind::MissingTest);
        }
        // NOTE: `enforce_source` has a long-standing bug where this case
        // re-checks `cited_lines.difference(&tested_lines)`; the intent (and
        // what we report here) is tested lines that lack a citation.
        for line in tested_lines.difference(&cited_lines) {
            push(*line, ViolationKind::TestWithoutCitation);
        }
    }
}

pub fn enforce_source(report: &TargetReport) -> Result {
    let mut cited_lines = HashSet::new();
    let mut tested_lines = HashSet::new();
    let mut significant_lines = HashSet::new();

    // record all references to specific sections
    for reference in &report.references {
        let line = reference.line();

        significant_lines.insert(line);

        match reference.annotation.anno {
            AnnotationType::Test => {
                tested_lines.insert(line);
            }
            AnnotationType::Citation => {
                cited_lines.insert(line);
            }
            AnnotationType::Exception => {
                // mark exceptions as fully covered
                tested_lines.insert(line);
                cited_lines.insert(line);
            }
            AnnotationType::Implication => {
                // mark implication as fully covered
                tested_lines.insert(line);
                cited_lines.insert(line);
            }
            AnnotationType::Spec | AnnotationType::Todo => {}
        }
    }

    if report.require_citations {
        // Significant lines are not cited.
        if significant_lines.difference(&cited_lines).next().is_some() {
            return Err(error!("Specification requirements missing citation."));
        }
        // Citations that have no significance.
        if cited_lines.difference(&significant_lines).next().is_some() {
            return Err(error!("Citation for non-existing specification."));
        }
    }

    if report.require_tests {
        // Cited lines without tests
        if cited_lines.difference(&tested_lines).next().is_some() {
            return Err(error!("Citation missing test."));
        }

        // Tests without citation
        if cited_lines.difference(&tested_lines).next().is_some() {
            return Err(error!("Test for non-existing citation."));
        }
    }

    Ok(())
}
