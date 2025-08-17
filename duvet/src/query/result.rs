// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use std::fmt;
use crate::{
    annotation::{Annotation, AnnotationSet, AnnotationType},
    comment::{Pattern},
    query::coverage::AnnotationExecutionStatus,
};
use std::{
    sync::Arc,
};
use duvet_core::{
    error,
    info,
};

/// Overall query result
#[derive(Debug)]
pub struct QueryResult {
    pub overall_status: QueryStatus,
    pub checks: Vec<CheckResult>,
}

#[derive(Debug)]
pub enum CheckResult {
    Implementation(ImplementationResult),
    Tests(TestResult), 
    Coverage(CoverageResult),
    Duplicates(DuplicatesResult),
}

#[derive(Debug)]
pub struct ImplementationResult {
    pub status: QueryStatus,
    pub in_scope_requirements: Vec<Arc<Annotation>>,
    pub fully_implemented: Vec<AnnotationCoverage>,
    pub mixed_implementation: Vec<AnnotationCoverage>,
    pub incomplete_implementation: Vec<AnnotationCoverage>,
    pub todo: Vec<AnnotationCoverage>,
    pub not_implemented: Vec<Arc<Annotation>>,
    pub verbose: bool,
}

#[derive(Debug)]
pub struct TestResult {
    pub status: QueryStatus,
    pub in_scope_requirements: Vec<Arc<Annotation>>,
    pub fully_tested: Vec<AnnotationCoverage>,
    pub incomplete_tests: Vec<AnnotationCoverage>,
    pub not_tested: Vec<Arc<Annotation>>,
    pub verbose: bool,
}

#[derive(Debug)]
pub struct CoverageResult {
    pub status: QueryStatus,
    pub executed_tests: AnnotationSet,
    pub executed_implementations: AnnotationSet,
    pub successful: Vec<CoveredTestAnnotation>,
    pub failed: Vec<CoveredTestAnnotation>,
    pub verbose: bool,
}

#[derive(Debug)]
pub struct DuplicatesResult {
    pub status: QueryStatus,
    pub spec: Duplicates,
    pub implementation: Duplicates,
    pub test: Duplicates,
    pub exception: Duplicates,
    pub todo: Duplicates,
    pub implication: Duplicates,
    pub verbose: bool,
}

#[derive(Debug)]
pub struct Duplicates {
    pub duplicates: Vec<AnnotationCoverage>,
    pub some_overlap: Vec<AnnotationCoverage>,
    pub unique: Vec<Arc<Annotation>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum QueryStatus {
    Pass,
    Fail,
}

impl fmt::Display for QueryStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QueryStatus::Pass => write!(f, "✓ PASS"),
            QueryStatus::Fail => write!(f, "✗ FAIL"),
        }
    }
}

#[derive(Debug)]
pub struct AnnotationCoverage {
    pub fully_covered: bool,
    pub target: Arc<Annotation>,
    pub covering_annotations: Vec<Arc<Annotation>>,
    pub covered: Vec<bool>,
}

impl AnnotationCoverage {
    pub fn merge(mut self, other: AnnotationCoverage) -> AnnotationCoverage {
        // MUST have same target - panic if not
        assert!(Arc::ptr_eq(&self.target, &other.target), 
            "Cannot merge AnnotationCoverage with different targets");

        // Extend covering_annotations
        self.covering_annotations.extend(other.covering_annotations);
        
        // OR the covered arrays element-wise
        for (i, other_covered) in other.covered.iter().enumerate() {
            if let Some(self_covered) = self.covered.get_mut(i) {
                *self_covered = *self_covered || *other_covered;
            }
        }
        
        // Do not update fully_covered based on merged covered array
        // This way the original implementation state can be seen
        
        self
    }
}

#[derive(Debug)]
pub struct CoveredTestAnnotation {
    pub test: Arc<Annotation>,
    pub test_execution_status: AnnotationExecutionStatus,
    pub executed_implementations: Vec<Arc<Annotation>>,
    pub not_executed_implementations: Vec<NotExecutedAnnotation>,
}

#[derive(Debug)]
pub struct NotExecutedAnnotation {
    pub annotation: Arc<Annotation>,
    pub status: AnnotationExecutionStatus
}

impl fmt::Display for QueryResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Display each check result
        for check in &self.checks {
            write!(f, "{}", check)?;
        }
        writeln!(f)?;
        // Overall status
        writeln!(f, "Overall: {}", self.overall_status)?;
        
        Ok(())
    }
}

impl fmt::Display for CheckResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CheckResult::Implementation(implementation_result) => write!(f, "{}", implementation_result),
            CheckResult::Tests(test_result) => write!(f, "{}", test_result),
            CheckResult::Coverage(coverage_result) => write!(f, "{}", coverage_result),
            CheckResult::Duplicates(duplicates_result) => write!(f, "{}", duplicates_result),
        }
    }
}

impl fmt::Display for ImplementationResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {

        writeln!(f)?;
        writeln!(f, "Implementation: {}", self.status)?;
        writeln!(f)?;

        // Normal mode: just show counts
        let total = self.in_scope_requirements.len();
        let fully_implemented = self.fully_implemented.len();
        let not_implemented = self.not_implemented.len();
        let incomplete = self.incomplete_implementation.len();
        let mixed = self.mixed_implementation.len();
        let todo = self.todo.len();
        
        writeln!(f, "  Total requirements: {}", total)?;
        writeln!(f, "  Fully implemented: {}", fully_implemented)?;
        writeln!(f, "  Incomplete implementation: {}", incomplete)?;
        writeln!(f, "  Mixed implementation: {}", mixed)?;
        writeln!(f, "  TODO: {}", todo)?;
        writeln!(f, "  Not implemented: {}", not_implemented)?;
        if not_implemented > 0 {
            writeln!(f)?;
            let pattern = Pattern::default();
            for annotation in &self.not_implemented {
                let missing_annotation_comment= format!(
                    "\n{} {}\n{} {}\n{} {}\n",
                    pattern.meta, annotation.target,
                    pattern.meta, "type=implementation",
                    pattern.content, annotation.quote,
                );
                let missing = error!("Missing annotation")
                    .with_help(missing_annotation_comment);
                writeln!(f, "{:?}", missing)?;
            }
        }
        if incomplete > 0 {
            writeln!(f)?;
            for coverage in &self.incomplete_implementation {
                let (first, rest) = coverage.covering_annotations
                    .split_first()
                    .expect("covering_annotations should not be empty");

                let mut incomplete = error!("Incomplete annotation:\n {}", coverage.target.quote);
                incomplete = with_annotation(incomplete, first, "Incomplete");
                incomplete = with_related_annotations(
                    incomplete,
                    &rest,
                    "Incomplete"
                );
                writeln!(f, "{:?}", incomplete)?;
            }
        }
        if mixed > 0 {
            writeln!(f)?;
            for coverage in &self.mixed_implementation {

                let (first, rest) = coverage.covering_annotations
                    .split_first()
                    .expect("covering_annotations should not be empty");

                let (todo, impls): (Vec<Arc<Annotation>>, Vec<Arc<Annotation>>) = rest
                    .iter()
                    .cloned()
                    .partition(|annotation| matches!(annotation.anno, AnnotationType::Todo));

                // The first message will always be an implementation
                let (implementation_message, todo_message) = if coverage.fully_covered {
                        ("Implemented", "Duplicate todo?")
                    } else {
                        ("Incomplete", "Implement this")
                    };

                let mut mixed = error!("Mixed implementation and TODO:\n {}", coverage.target.quote);
                mixed = with_annotation(mixed, first, implementation_message);
                mixed = with_related_annotations(
                    mixed,
                    &impls,
                    implementation_message
                );
                mixed = with_related_annotations(
                    mixed,
                    &todo,
                    todo_message
                );
                writeln!(f, "{:?}", mixed)?;
            }
        }
        if todo > 0 {
            writeln!(f)?;
            for coverage in &self.todo {
                let (first, rest) = coverage.covering_annotations
                    .split_first()
                    .expect("covering_annotations should not be empty");

                let mut todo = error!("Todo annotations");
                todo = with_annotation(todo, first, "Implement this");
                todo = with_related_annotations(
                    todo,
                    &rest,
                    "Implement this"
                );
                writeln!(f, "{:?}", todo)?;
            }
        }
        
        if self.verbose {
            // Verbose mode: show detailed annotations
            if !self.fully_implemented.is_empty() {
                writeln!(f)?;
                for coverage in &self.fully_implemented {
                    let (first, rest) = coverage.covering_annotations
                        .split_first()
                        .expect("covering_annotations should not be empty");

                    // TODO these are implementations, exceptions, and implications
                    // the tag text should reflect this.

                    let mut complete = info!("Fully Implemented");
                    complete = with_annotation(complete, first, "Implemented");
                    complete = with_related_annotations(
                        complete,
                        &rest,
                        "Implemented"
                    );

                    writeln!(f, "{:?}", complete)?;
                }
            }
        }

        Ok(())
    }
}

impl fmt::Display for TestResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f)?;
        writeln!(f, "Tests: {}", self.status)?;
        writeln!(f)?;

        // Normal mode: just show counts
        let total = self.in_scope_requirements.len();
        let fully_tested = self.fully_tested.len();
        let not_tested = self.not_tested.len();
        let incomplete_tests = self.incomplete_tests.len();
        
        writeln!(f, "  Total requirements: {}", total)?;
        writeln!(f, "  Fully tested: {}", fully_tested)?;
        writeln!(f, "  Incomplete tests: {}", incomplete_tests)?;
        writeln!(f, "  Not tested: {}", not_tested)?;
        writeln!(f)?;

        if not_tested > 0 {
            let pattern = Pattern::default();
            for annotation in &self.not_tested {
                let missing_annotation_comment= format!(
                    "\n{} {}\n{} {}\n{} {}\n",
                    pattern.meta, annotation.target,
                    pattern.meta, "type=test",
                    pattern.content, annotation.quote,
                );
                let mut missing = error!("Missing test");
                missing = with_annotation(missing, annotation, "Implementation")
                    .with_help(missing_annotation_comment);
                writeln!(f, "{:?}", missing)?;

            }
            writeln!(f)?;
        }
        
        if incomplete_tests > 0 {
            for coverage in &self.incomplete_tests {
                let mut incomplete = error!("Incomplete test:\n {}", coverage.target.quote);
                incomplete = with_annotation(incomplete, &coverage.target, "Implementation");
                incomplete = with_related_annotations(
                    incomplete,
                    &coverage.covering_annotations,
                    "Incomplete test"
                );
                writeln!(f, "{:?}", incomplete)?;
            }
            writeln!(f)?;
        }
        
        if self.verbose {
            // Verbose mode: show detailed annotations
            if !self.fully_tested.is_empty() {
                for coverage in &self.fully_tested {
                    let (first, rest) = coverage.covering_annotations
                        .split_first()
                        .expect("covering_annotations should not be empty");

                    let mut complete = info!("Fully tested");
                    complete = with_annotation(complete, first, "Test");
                    complete = with_related_annotations(
                        complete,
                        &rest,
                        "Test"
                    );

                    writeln!(f, "{:?}", complete)?;
                }
                writeln!(f)?;
            }
        }
        
        Ok(())
    }
}

impl fmt::Display for CoverageResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f)?;
        writeln!(f, "Coverage: {}", self.status)?;
        writeln!(f)?;

        // Summary counts
        let executed_tests = self.executed_tests.len();
        let executed_implementations = self.executed_implementations.len();
        let successful = self.successful.len();
        let failed = self.failed.len();
        
        writeln!(f, "  Executed tests: {}", executed_tests)?;
        writeln!(f, "  Executed implementations: {}", executed_implementations)?;
        writeln!(f, "  Successful correlations: {}", successful)?;
        writeln!(f, "  Failed correlations: {}", failed)?;
        writeln!(f)?;
        
        // Show failed correlations with detailed diagnostics
        if failed > 0 {
            for correlation in &self.failed {
                let mut error = error!("Failed correlation");

                let test_annotation_message = match correlation.test_execution_status {
                    AnnotationExecutionStatus::Executed => "Executed test",
                    AnnotationExecutionStatus::NotExecuted => "Not executed test",
                    AnnotationExecutionStatus::Unknown => "Not executed because of an unknown not executable line.",
                };
                
                // Add test annotation context
                error = error.with_source_slice(
                    correlation.test.original_text.clone(),
                    test_annotation_message
                );

                error = with_related_annotations(
                    error,
                    &correlation.executed_implementations,
                    "Executed implementation"
                );

                error = with_related_not_executed_annotations(
                    error,
                    &correlation.not_executed_implementations,
                    |status| match status {
                        AnnotationExecutionStatus::NotExecuted => "Not executed implementation",
                        AnnotationExecutionStatus::Unknown => "Not executed because of an unknown not executable line.",
                        AnnotationExecutionStatus::Executed => unreachable!("Executed implementation"), // shouldn't happen
                    }
                );
                
                writeln!(f, "{:?}", error)?;
            }
            writeln!(f)?;
        }
        
        // Show successful correlations in verbose mode
        if self.verbose {
            for correlation in &self.successful {
                let mut info = info!("Successful correlation");
                
                // Add test annotation context
                info = info.with_source_slice(
                    correlation.test.original_text.clone(),
                    "Test annotation"
                );

                info = with_related_annotations(
                    info,
                    &correlation.executed_implementations,
                    "Executed implementation"
                );

                // correlation in successful ==>
                //  correlation.test_execution_status == AnnotationExecutionStatus::Executed ==>
                //  correlation.not_executed_implementations.is_empty()
                
                writeln!(f, "{:?}", info)?;
            }

            let mut executed_annotation = info!("Executed annotations");
            executed_annotation = with_related_annotations(
                executed_annotation,
                &self.executed_tests.iter().cloned().collect::<Vec<_>>(),
                "Executed test"
            );
            executed_annotation = with_related_annotations(
                executed_annotation,
                &self.executed_implementations.iter().cloned().collect::<Vec<_>>(),
                "Executed implementation"
            );
            writeln!(f, "{:?}", executed_annotation)?;

            writeln!(f)?;
        }
        
        Ok(())
    }
}

impl fmt::Display for DuplicatesResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f)?;
        writeln!(f, "Duplicates: {}", self.status)?;
        writeln!(f)?;

        // Helper function to show duplicates for a category
        let show_duplicates = |f: &mut fmt::Formatter, category_name: &str, duplicates: &Duplicates| -> fmt::Result {
            if !duplicates.duplicates.is_empty() {
                for coverage in &duplicates.duplicates {
                    let duplicate_error = coverage2error(
                        coverage,
                        format!("Duplicate {} annotations", category_name.to_lowercase()),
                        "Duplicate".to_string(),
                        "Duplicate".to_string(),
                    );
                    writeln!(f, "{:?}", duplicate_error)?;
                }
            }
            Ok(())
        };

        // Always show duplicates for each category
        show_duplicates(f, "Spec", &self.spec)?;
        show_duplicates(f, "Implementation", &self.implementation)?;  
        show_duplicates(f, "Test", &self.test)?;
        show_duplicates(f, "Exception", &self.exception)?;
        show_duplicates(f, "Todo", &self.todo)?;
        show_duplicates(f, "Implication", &self.implication)?;

        // If verbose, show additional details
        if self.verbose {
            // Helper function to show verbose info for a category
            let show_verbose_info = |f: &mut fmt::Formatter, category_name: &str, duplicates: &Duplicates| -> fmt::Result {
                // Show some_overlap
                if !duplicates.some_overlap.is_empty() {
                    for coverage in &duplicates.some_overlap {
                        let (first, rest) = coverage.covering_annotations
                            .split_first()
                            .expect("covering_annotations should not be empty");

                        let mut overlap_info = info!("{} annotations with some overlap", category_name);
                        overlap_info = with_annotation(overlap_info, first, "Some overlap");
                        overlap_info = with_related_annotations(
                            overlap_info,
                            &rest,
                            "Some overlap"
                        );
                        writeln!(f, "{:?}", overlap_info)?;
                    }
                }

                // Show unique
                if !duplicates.unique.is_empty() {
                    let mut unique_info = info!("Unique {} annotations", category_name.to_lowercase());
                    unique_info = with_related_annotations(
                        unique_info,
                        &duplicates.unique,
                        "Unique"
                    );
                    writeln!(f, "{:?}", unique_info)?;
                }
                Ok(())
            };

            show_verbose_info(f, "Spec", &self.spec)?;
            show_verbose_info(f, "Implementation", &self.implementation)?;
            show_verbose_info(f, "Test", &self.test)?;
            show_verbose_info(f, "Exception", &self.exception)?;
            show_verbose_info(f, "Todo", &self.todo)?;
            show_verbose_info(f, "Implication", &self.implication)?;
        }

        Ok(())
    }
}

fn coverage2error(
    coverage: &AnnotationCoverage,
    error_message: String,
    annotation_message: String,
    related_annotations_message: String,
) -> duvet_core::diagnostic::Error {
    let mut error = error!(error_message);
    error = with_annotation(error, &coverage.target, annotation_message);
    with_related_annotations(
        error,
        &coverage.covering_annotations,
        related_annotations_message,
    )
}

fn with_annotation(
    error: duvet_core::diagnostic::Error,
    annotation: &Arc<Annotation>,
    message: impl AsRef<str>,
) -> duvet_core::diagnostic::Error {
    let message = message.as_ref();
    
    error.with_source_slice(annotation.original_text.clone(), message)
}

fn with_related_annotations(
    mut error: duvet_core::diagnostic::Error,
    annotations: &[Arc<Annotation>],
    message: impl AsRef<str>,
) -> duvet_core::diagnostic::Error {
    let message = message.as_ref();
    for annotation in annotations {
        error = error.with_related_source_slice(annotation.original_text.clone(), message);
    }
    error
}

fn with_related_not_executed_annotations(
    mut error: duvet_core::diagnostic::Error,
    annotations: &[NotExecutedAnnotation],
    message: fn(AnnotationExecutionStatus) -> &'static str,
) -> duvet_core::diagnostic::Error {
    for annotation in annotations {
        error = error.with_related_source_slice(annotation.annotation.original_text.clone(), message(annotation.status));
    }
    error
}
