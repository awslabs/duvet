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
