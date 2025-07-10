// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use super::{ReportResult, TargetReport};
use crate::{annotation::AnnotationType, reference::Reference, Result};
use duvet_core::error;
use std::collections::{HashMap, HashSet};

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
    let mut todo_lines = HashSet::new();

    // Map lines to their references for detailed error reporting
    let mut line_to_references: HashMap<usize, Vec<&Reference>> = HashMap::new();

    // record all references to specific sections
    for reference in &report.references {
        let line = reference.line();

        line_to_references.entry(line).or_default().push(reference);
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
            AnnotationType::Todo => {
                todo_lines.insert(line);
            }
            AnnotationType::Spec => {}
        }
    }

    let mut error_messages = Vec::new();

    if report.require_citations {
        // Collect missing citations (excluding TODO lines)
        let missing_citation_lines: Vec<_> = significant_lines
            .difference(&cited_lines)
            .filter(|line| !todo_lines.contains(line))
            .collect();

        if !missing_citation_lines.is_empty() {
            let mut msg =
                String::from("The following specification requirements are missing citations:\n\n");
            for &line in &missing_citation_lines {
                if let Some(references) = line_to_references.get(&line) {
                    for reference in references {
                        msg.push_str(&format_duvet_annotation(reference, "implementation"));
                        msg.push('\n');
                    }
                }
            }
            error_messages.push(msg);
        }

        // Collect TODO annotations
        if !todo_lines.is_empty() {
            let mut msg =
                String::from("The following specification requirements have TODO annotations:\n\n");
            for &line in &todo_lines {
                if let Some(references) = line_to_references.get(&line) {
                    for reference in references {
                        if reference.annotation.anno == AnnotationType::Todo {
                            msg.push_str(&format_duvet_annotation(reference, "implementation"));
                            msg.push('\n');
                        }
                    }
                }
            }
            error_messages.push(msg);
        }

        // Citations that have no significance.
        if cited_lines.difference(&significant_lines).next().is_some() {
            error_messages.push("Citation for non-existing specification.".to_string());
        }
    }

    if report.require_tests {
        // Collect cited lines without tests
        let missing_test_lines: Vec<_> = cited_lines.difference(&tested_lines).collect();
        if !missing_test_lines.is_empty() {
            let mut msg = String::from("The following citations are missing tests:\n\n");
            for &line in &missing_test_lines {
                if let Some(references) = line_to_references.get(&line) {
                    for reference in references {
                        if reference.annotation.anno == AnnotationType::Citation {
                            msg.push_str(&format_duvet_annotation(reference, "test"));
                            msg.push('\n');
                        }
                    }
                }
            }
            error_messages.push(msg);
        }

        // Tests without citation
        if tested_lines.difference(&cited_lines).next().is_some() {
            error_messages.push("Test for non-existing citation.".to_string());
        }
    }

    if !error_messages.is_empty() {
        return Err(error!("{}", error_messages.join("\n")));
    }

    Ok(())
}

pub fn format_duvet_annotation(reference: &Reference, annotation_type: &str) -> String {
    let target_path = reference.target.path.to_string();
    let section = reference
        .annotation
        .target_section()
        .map(|s| format!("#{}", s))
        .unwrap_or_default();

    let mut result = format!("    //= {}{}\n", target_path, section);
    result.push_str(&format!("    //= type={}\n", annotation_type));

    // Get the requirement text and format each line
    let content = reference.text.as_ref();
    for line in content.lines() {
        if !line.trim().is_empty() {
            result.push_str(&format!("    //# {}\n", line.trim()));
        }
    }

    result
}
