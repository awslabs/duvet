// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    annotation::{Annotation, },
    target::Target,
    specification::{Specification},
    text::whitespace,
    Result,
    query::{
        engine::ProjectData,
        result::{
            AnnotationCoverage,
        }
    }
};
use std::{
    collections::{ HashMap, },
    sync::Arc,
};

pub mod coverage;

/// Check if a target annotation is covered by a collection annotations
/// A target annotation is covered,
/// if every part of the target's quote is quoted in in the collection.
/// If the target quote is "MUST foo, MUST bar, MUST baz"
/// And the collection has "MUST foo,"; "MUST bar"; "MUST bar, MUST baz"; "MUST run"
/// then the target is said to be covered
pub fn is_annotation_covered(
    target_annotation: &Arc<Annotation>,
    specifications: &Arc<HashMap<Arc<Target>, Arc<Specification>>>,
    annotations: &[Arc<Annotation>],
) -> Result<AnnotationCoverage> {
    if target_annotation.quote.trim().is_empty() {
        return Ok(AnnotationCoverage {
            fully_covered: true,
            target: target_annotation.clone(),
            covering_annotations: Vec::new(),
            covered: Vec::new(),
        });
    }

    // Get the target and find the specification
    let target = target_annotation.target()?;
    let specification = specifications.get(&target).expect("Specification not found for target annotation");
    let target_section = target_annotation.target_section().expect("Section not found for specification");
    let section = specification.section(&target_section).expect("Target section not found in specification");
    let section_contents = section.view();

    // Normalize the sections for our matching
    let normalize_section_contents = whitespace::normalize(&section_contents);
    let normalized_target_quote = whitespace::normalize(&target_annotation.quote);

    // Get the target range.
    // This will only match the first match.
    // If the specification section has duplicate quotes,
    // then this matching will be unexpected.
    // On the good side, duplicate requirements in a section are confusing,
    // and as long as the specification reads nicely
    // degenerate duplicates like `the` won't work well and are not encouraged.
    let target_start = match normalize_section_contents.find(&normalized_target_quote) {
        Some(start) => start,
        None => {
            return Err(
                duvet_core::error!("Exactly matchable quote not found in section")
                .with_source_slice(target_annotation.original_text.clone(), "Quote")
                .with_help("This is likely a multiline comment and the second line is missing `-` or other list operator.")
            );
        }
    };
    let target_end = target_start + normalized_target_quote.len();

    let mut covered = vec![false; normalized_target_quote.len()];
    let mut covering_annotations: Vec<Arc<Annotation>> = Vec::new();
    
    for annotation in annotations
        .iter()
        // An annotation can not be covered by itself
        .filter(|annotation| *annotation != target_annotation)
        // An annotation can only be covered by annotations in the same target (section)
        .filter(|annotation| {
            target_annotation.target == annotation.target
        })
        // Trying to match empty annotations is silly
        .filter(|annotation| {
            !annotation.quote.is_empty()
        })
    {
        let normalized_quote = whitespace::normalize(&annotation.quote);

        let annotation_start = match normalize_section_contents.find(&normalized_quote) {
            Some(start) => start,
            None => {
                return Err(
                    duvet_core::error!("Exactly matchable quote not found in section")
                    .with_source_slice(annotation.original_text.clone(), "Quote")
                    .with_help("This is likely a multiline comment and the second line is missing `-` or other list operator.")
                );
            }
        };
        let annotation_end = annotation_start + normalized_quote.len();

            // Find overlap between the target range and this annotation range
        let overlap_start = std::cmp::max(target_start, annotation_start);
        let overlap_end = std::cmp::min(target_end, annotation_end);
        
        // If there's an overlap, mark those positions as covered
        if overlap_start < overlap_end {
            for pos in overlap_start..overlap_end {
                let index = pos - target_start;
                if index < covered.len() {
                    covered[index] = true;
                }
            }
            covering_annotations.push(annotation.clone());
        }
    }

    Ok(AnnotationCoverage {
        fully_covered: covered.iter().all(|&covered| covered),
        target: target_annotation.clone(),
        covering_annotations,
        covered,
    })
}


#[derive(Debug)]
pub struct ClassifiedCoverage {
    pub complete_coverage: Vec<AnnotationCoverage>,
    pub incomplete_coverage: Vec<AnnotationCoverage>,
    pub no_coverage: Vec<Arc<Annotation>>,
    pub mixed_coverage: Vec<AnnotationCoverage>,
    pub secondary_coverage: Vec<AnnotationCoverage>,
}

pub fn classify_annotation_coverage(
    project_data: &ProjectData,
    annotations: &Vec<Arc<Annotation>>,
    maybe_primary_covering_annotations: &Vec<Arc<Annotation>>,
    maybe_secondary_covering_annotations: &Vec<Arc<Annotation>>,
) -> Result<ClassifiedCoverage> {
    let mut complete_coverage: Vec<AnnotationCoverage> = Vec::new();
    let mut incomplete_coverage: Vec<AnnotationCoverage> = Vec::new();
    let mut no_coverage: Vec<Arc<Annotation>> = Vec::new();
    let mut mixed_coverage: Vec<AnnotationCoverage> = Vec::new();
    let mut secondary_coverage: Vec<AnnotationCoverage> = Vec::new();

    for annotation in annotations {
        let primary_coverage = is_annotation_covered(annotation, &project_data.specifications, &maybe_primary_covering_annotations)?;
        let secondary = is_annotation_covered(annotation, &project_data.specifications, &maybe_secondary_covering_annotations)?;

        let primary_len = primary_coverage.covering_annotations.len();
        let secondary_len = secondary.covering_annotations.len();

        match (primary_coverage.fully_covered, primary_len, secondary_len) {
            // Complete primary coverage
            (true, _, s) if 0 == s => complete_coverage.push(primary_coverage),
            // Complete primary but there is secondary coverage. duplicates?
            (true, _, s) if 0 < s => mixed_coverage.push(primary_coverage.merge(secondary)),
            // Primary is missing something
            (false, p, s) if 0 < p && 0 == s => incomplete_coverage.push(primary_coverage),
            // Mixed primary and secondary. duplicates?
            (false, p, s) if 0 < p && 0 < s => mixed_coverage.push(primary_coverage.merge(secondary)),
            // Only secondary
            (false, p, s) if 0 == p && 0 < s => secondary_coverage.push(secondary),
            // Zero coverage
            _ => no_coverage.push(annotation.clone()),
        }
    }

    Ok(ClassifiedCoverage {
        complete_coverage,
        incomplete_coverage,
        no_coverage,
        mixed_coverage,
        secondary_coverage,
    })

}

