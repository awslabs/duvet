// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use super::{ReportResult, TargetReport, RequirementMode, TargetedRequirement};
use crate::{annotation::AnnotationType, reference::Reference, Result};
use duvet_core::error;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

pub fn report(report: &ReportResult) -> Result {
    report
        .targets
        .iter()
        .try_for_each(|(_source, report)| enforce_source(report))
}

pub fn enforce_source(report: &TargetReport) -> Result {
    // Filter references based on citation and test requirements separately
    let citation_refs = filter_references_for_requirements(&report.references, &report.require_citations, &report.specification);
    let test_refs = filter_references_for_requirements(&report.references, &report.require_tests, &report.specification);

    let mut error_messages = Vec::new();

    // Check citation requirements
    if !citation_refs.is_empty() {
        if let Some(msg) = check_citation_requirements(&citation_refs) {
            error_messages.push(msg);
        }
    }

    // Check test requirements
    if !test_refs.is_empty() {
        if let Some(msg) = check_test_requirements(&test_refs) {
            error_messages.push(msg);
        }
    }

    if !error_messages.is_empty() {
        return Err(error!("{}", error_messages.join("\n")));
    }

    Ok(())
}

fn filter_references_for_requirements<'a>(
    references: &'a [Reference],
    mode: &RequirementMode,
    _specification: &Arc<crate::specification::Specification>,
) -> Vec<&'a Reference> {
    match mode {
        RequirementMode::None => vec![],
        RequirementMode::Global(false) => vec![],
        RequirementMode::Global(true) => references.iter().collect(),
        RequirementMode::Targeted(requirements) => {
            references
                .iter()
                .filter(|reference| reference_matches_requirements(reference, requirements))
                .collect()
        }
    }
}

fn reference_matches_requirements(reference: &Reference, requirements: &[TargetedRequirement]) -> bool {
    for requirement in requirements {
        let target_path_str = reference.target.path.to_string();
        if target_path_matches_str(&target_path_str, &requirement.path) {
            // If no section specified, the entire spec is required
            if requirement.section.is_none() {
                return true;
            }
            
            // If section is specified, check if this reference matches the section
            if let Some(required_section) = &requirement.section {
                if let Some(ref_section) = reference.annotation.target_section() {
                    if ref_section.as_ref() == required_section {
                        return true;
                    }
                }
            }
        }
    }
    false
}

fn check_citation_requirements(references: &[&Reference]) -> Option<String> {
    let mut cited_lines = HashSet::new();
    let mut significant_lines = HashSet::new();
    let mut todo_lines = HashSet::new();
    let mut line_to_references: HashMap<usize, Vec<&Reference>> = HashMap::new();

    // Process the filtered references
    for reference in references {
        let line = reference.line();
        line_to_references.entry(line).or_default().push(reference);
        significant_lines.insert(line);

        match reference.annotation.anno {
            AnnotationType::Citation => {
                cited_lines.insert(line);
            }
            AnnotationType::Exception => {
                // mark exceptions as fully covered
                cited_lines.insert(line);
            }
            AnnotationType::Implication => {
                // mark implication as fully covered
                cited_lines.insert(line);
            }
            AnnotationType::Todo => {
                todo_lines.insert(line);
            }
            _ => {}
        }
    }

    let mut error_parts = Vec::new();

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
        error_parts.push(msg);
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
        error_parts.push(msg);
    }

    if error_parts.is_empty() {
        None
    } else {
        Some(error_parts.join("\n"))
    }
}

fn check_test_requirements(references: &[&Reference]) -> Option<String> {
    let mut cited_lines = HashSet::new();
    let mut tested_lines = HashSet::new();
    let mut significant_lines = HashSet::new();
    let mut todo_lines = HashSet::new();
    let mut line_to_references: HashMap<usize, Vec<&Reference>> = HashMap::new();

    // Process the filtered references
    for reference in references {
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
            _ => {}
        }
    }

    let mut error_parts = Vec::new();

    // Collect implemented requirements missing tests (has citation, no test)
    let implemented_missing_tests: Vec<_> = cited_lines.difference(&tested_lines).collect();
    if !implemented_missing_tests.is_empty() {
        let mut msg = String::from("The following implemented requirements are missing tests:\n\n");
        for &line in &implemented_missing_tests {
            if let Some(references) = line_to_references.get(&line) {
                for reference in references {
                    if reference.annotation.anno == AnnotationType::Citation {
                        msg.push_str(&format_duvet_annotation(reference, "test"));
                        msg.push('\n');
                    }
                }
            }
        }
        error_parts.push(msg);
    }

    // Collect unimplemented requirements missing tests (no citation, no test)
    let unimplemented_missing_tests: Vec<_> = significant_lines
        .difference(&cited_lines)
        .filter(|line| !tested_lines.contains(line) && !todo_lines.contains(line))
        .collect();
    if !unimplemented_missing_tests.is_empty() {
        let mut msg = String::from("The following unimplemented requirements are missing tests:\n\n");
        for &line in &unimplemented_missing_tests {
            if let Some(references) = line_to_references.get(&line) {
                for reference in references {
                    msg.push_str(&format_duvet_annotation(reference, "test"));
                    msg.push('\n');
                }
            }
        }
        error_parts.push(msg);
    }

    if error_parts.is_empty() {
        None
    } else {
        Some(error_parts.join("\n"))
    }
}



fn target_path_matches_str(spec_path_str: &str, requirement_path: &str) -> bool {
    // Handle exact matches
    if spec_path_str == requirement_path {
        return true;
    }
    
    // Handle relative path matches - check if the spec path ends with the requirement path
    if spec_path_str.ends_with(requirement_path) {
        // Ensure it's a proper path boundary (not a partial filename match)
        let prefix_len = spec_path_str.len() - requirement_path.len();
        if prefix_len == 0 || spec_path_str.chars().nth(prefix_len - 1) == Some('/') {
            return true;
        }
    }
    
    false
}

pub fn format_duvet_annotation(reference: &Reference, annotation_type: &str) -> String {
    // Use the annotation target directly, which contains the path as written in the source code
    let mut result = format!("    //= {}\n", reference.annotation.target);
    result.push_str(&format!("    //= type={}\n", annotation_type));

    // Get the requirement text and preserve original line breaks
    let content = reference.text.as_ref();
    
    // Format each line as a comment, preserving the original line structure
    // This keeps multi-line requirements together while respecting the spec's formatting
    for line in content.lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            result.push_str(&format!("    //# {}\n", trimmed));
        }
    }

    result
}
