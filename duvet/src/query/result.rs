// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    annotation::{Annotation, AnnotationSet, AnnotationType},
    comment::Pattern,
    query::{coverage::ExecutionStatus, witness::{PairWitnessResult, WitnessRef}},
};
use duvet_core::{error, info};
use serde::{Deserialize, Serialize};
use std::{fmt, sync::Arc};

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

impl CheckResult {
    pub fn status(&self) -> &QueryStatus {
        match self {
            CheckResult::Implementation(r) => &r.status,
            CheckResult::Tests(r) => &r.status,
            CheckResult::Coverage(r) => &r.status,
            CheckResult::Duplicates(r) => &r.status,
        }
    }
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
    pub report_count: usize,
    pub executed_tests: AnnotationSet,
    pub executed_implementations: AnnotationSet,
    pub successful: Vec<CoveredTestAnnotation>,
    pub failed: Vec<CoveredTestAnnotation>,
    /// Tests whose covered spec text has no correlated implementation
    /// annotation anywhere (design §2.4). Reported as failures distinct from
    /// "test ran, implementation did not".
    pub missing_implementation: Vec<Arc<Annotation>>,
    //= design/witness/spec.md#property-w6-unwitnessed-test-annotations
    //= type=implementation
    //# The engine MUST report every test annotation for which no
    //# delivered witness binds it —
    //# across ALL configured producers —
    //# as a failure, never silently
    pub unwitnessed: Vec<UnwitnessedTestAnnotation>,
    pub verbose: bool,
}

#[derive(Debug)]
pub struct DuplicatesResult {
    pub status: QueryStatus,
    pub categories: Vec<(&'static str, Duplicates)>,
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
        // Uses pointer identity rather than value equality. This is correct because
        // all AnnotationCoverage instances for the same annotation share the same
        // Arc clone from the AnnotationSet — they always point to the same allocation.
        assert!(
            Arc::ptr_eq(&self.target, &other.target),
            "Cannot merge AnnotationCoverage with different targets"
        );

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
    pub test_execution_status: ExecutionStatus,
    /// The witnesses bound to this test. The verdict for each covering
    /// implementation is universal over this set (Decision 14).
    pub bound_witnesses: Vec<WitnessRef>,
    /// Implementations every bound witness executed (discharged).
    pub executed_implementations: Vec<Arc<Annotation>>,
    pub not_executed_implementations: Vec<NotExecutedAnnotation>,
}

//= design/witness/spec.md#verdict-output
//= type=implementation
//# For every pair that fails because a bound witness did not execute
//# the implementation (W1's universal clause), the output MUST list
//# **every** bound witness with its per-witness result
//# (executed I / did not execute I) and strength,
//# so the failing claim is identifiable
#[derive(Debug)]
pub struct NotExecutedAnnotation {
    pub annotation: Arc<Annotation>,
    pub status: ExecutionStatus,
    /// Every bound witness's result for this pair, in bound order
    /// (already computed to evaluate the verdict — surfaced, never
    /// silent).
    pub per_witness: Vec<PairWitnessResult>,
}

//= design/witness/spec.md#verdict-output
//= type=implementation
//# For every unwitnessed test annotation (W6), the output MUST
//# identify the annotation and state that no configured producer
//# yielded a witness for it.
#[derive(Debug)]
pub struct UnwitnessedTestAnnotation {
    pub test: Arc<Annotation>,
    /// The test's own execution status folded across all delivered
    /// witnesses — diagnostic detail only (Unknown carries a line).
    pub diagnostic_status: ExecutionStatus,
}

impl fmt::Display for QueryResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Display each check result
        for check in &self.checks {
            write!(f, "{check}")?;
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
            CheckResult::Implementation(implementation_result) => {
                write!(f, "{implementation_result}")
            }
            CheckResult::Tests(test_result) => write!(f, "{test_result}"),
            CheckResult::Coverage(coverage_result) => write!(f, "{coverage_result}"),
            CheckResult::Duplicates(duplicates_result) => write!(f, "{duplicates_result}"),
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

        writeln!(f, "  Total requirements: {total}")?;
        writeln!(f, "  Fully implemented: {fully_implemented}")?;
        writeln!(f, "  Incomplete implementation: {incomplete}")?;
        writeln!(f, "  Mixed implementation: {mixed}")?;
        writeln!(f, "  TODO: {todo}")?;
        writeln!(f, "  Not implemented: {not_implemented}")?;
        if not_implemented > 0 {
            writeln!(f)?;
            let pattern = Pattern::default();
            for annotation in &self.not_implemented {
                let missing_annotation_comment = format!(
                    "\n{} {}\n{} {}\n{} {}\n",
                    pattern.meta,
                    annotation.target,
                    pattern.meta,
                    "type=implementation",
                    pattern.content,
                    annotation.quote,
                );
                let missing = error!("Missing annotation").with_help(missing_annotation_comment);
                writeln!(f, "{missing:?}")?;
            }
        }
        if incomplete > 0 {
            writeln!(f)?;
            for coverage in &self.incomplete_implementation {
                let (first, rest) = coverage
                    .covering_annotations
                    .split_first()
                    .expect("covering_annotations should not be empty");

                let mut incomplete = error!("Incomplete annotation:\n {}", coverage.target.quote);
                incomplete = with_annotation(incomplete, first, "Incomplete");
                incomplete = with_related_annotations(incomplete, rest, "Incomplete");
                writeln!(f, "{incomplete:?}")?;
            }
        }
        if mixed > 0 {
            writeln!(f)?;
            for coverage in &self.mixed_implementation {
                let (first, rest) = coverage
                    .covering_annotations
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

                let mut mixed =
                    error!("Mixed implementation and TODO:\n {}", coverage.target.quote);
                mixed = with_annotation(mixed, first, implementation_message);
                mixed = with_related_annotations(mixed, &impls, implementation_message);
                mixed = with_related_annotations(mixed, &todo, todo_message);
                writeln!(f, "{mixed:?}")?;
            }
        }
        if todo > 0 {
            writeln!(f)?;
            for coverage in &self.todo {
                let (first, rest) = coverage
                    .covering_annotations
                    .split_first()
                    .expect("covering_annotations should not be empty");

                let mut todo = error!("Todo annotations");
                todo = with_annotation(todo, first, "Implement this");
                todo = with_related_annotations(todo, rest, "Implement this");
                writeln!(f, "{todo:?}")?;
            }
        }

        if self.verbose {
            // Verbose mode: show detailed annotations
            if !self.fully_implemented.is_empty() {
                writeln!(f)?;
                for coverage in &self.fully_implemented {
                    // Covering annotations may be citations, exceptions, or implications —
                    // the label should reflect the actual type, but for now we use "Implemented".
                    let mut complete = info!("Fully Implemented");
                    match coverage.covering_annotations.split_first() {
                        Some((first, rest)) => {
                            complete = with_annotation(complete, first, "Implemented");
                            complete = with_related_annotations(complete, rest, "Implemented");
                        }
                        // A target with a whitespace-only quote is trivially "covered"
                        // with no covering annotations (see is_annotation_covered). Render
                        // the target itself rather than panicking on an empty slice.
                        None => {
                            complete = with_annotation(complete, &coverage.target, "Implemented");
                        }
                    }

                    writeln!(f, "{complete:?}")?;
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

        writeln!(f, "  Total requirements: {total}")?;
        writeln!(f, "  Fully tested: {fully_tested}")?;
        writeln!(f, "  Incomplete tests: {incomplete_tests}")?;
        writeln!(f, "  Not tested: {not_tested}")?;
        writeln!(f)?;

        if not_tested > 0 {
            let pattern = Pattern::default();
            for annotation in &self.not_tested {
                let missing_annotation_comment = format!(
                    "\n{} {}\n{} {}\n{} {}\n",
                    pattern.meta,
                    annotation.target,
                    pattern.meta,
                    "type=test",
                    pattern.content,
                    annotation.quote,
                );
                let mut missing = error!("Missing test");
                missing = with_annotation(missing, annotation, "Implementation")
                    .with_help(missing_annotation_comment);
                writeln!(f, "{missing:?}")?;
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
                    "Incomplete test",
                );
                writeln!(f, "{incomplete:?}")?;
            }
            writeln!(f)?;
        }

        if self.verbose {
            // Verbose mode: show detailed annotations
            if !self.fully_tested.is_empty() {
                for coverage in &self.fully_tested {
                    let mut complete = info!("Fully tested");
                    match coverage.covering_annotations.split_first() {
                        Some((first, rest)) => {
                            complete = with_annotation(complete, first, "Test");
                            complete = with_related_annotations(complete, rest, "Test");
                        }
                        // Whitespace-only quote: trivially covered, no coverers. Render
                        // the target rather than panicking on an empty slice.
                        None => {
                            complete = with_annotation(complete, &coverage.target, "Test");
                        }
                    }

                    writeln!(f, "{complete:?}")?;
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
        let reports_count = self.report_count;
        let executed_tests = self.executed_tests.len();
        let executed_implementations = self.executed_implementations.len();
        let successful = self.successful.len();
        let failed = self.failed.len();
        let missing_implementation = self.missing_implementation.len();
        let unwitnessed = self.unwitnessed.len();

        writeln!(f, "  Coverage reports checked: {reports_count}")?;
        writeln!(f, "  Executed tests: {executed_tests}")?;
        writeln!(f, "  Executed implementations: {executed_implementations}")?;
        writeln!(f, "  Successful correlations: {successful}")?;
        writeln!(f, "  Failed correlations: {failed}")?;
        writeln!(
            f,
            "  Tests with no implementation: {missing_implementation}"
        )?;
        writeln!(f, "  Unwitnessed tests: {unwitnessed}")?;
        writeln!(f)?;

        // Test annotations no configured coverage source yielded a witness
        // for (spec W6): reported distinctly from bound-but-not-discharged,
        // never silently. With multiple producers configured, "unwitnessed"
        // means unwitnessed by ALL of them (Decision 8).
        if unwitnessed > 0 {
            for entry in &self.unwitnessed {
                let detail = match entry.diagnostic_status {
                    ExecutionStatus::Executed => unreachable!("an executed test is bound"),
                    ExecutionStatus::NotExecuted => "Not executed test",
                    ExecutionStatus::Structural => {
                        "Test target is purely declarative; no executable code to verify"
                    }
                    ExecutionStatus::Unknown { .. } => {
                        "Not executed because of an unknown not executable line."
                    }
                };
                let mut error = error!("Unwitnessed test annotation")
                    .with_source_slice(entry.test.original_text.clone(), detail);
                if let ExecutionStatus::Unknown { line_number } = entry.diagnostic_status {
                    if let Some(line_slice) = get_line_slice(&entry.test, line_number) {
                        error = error.with_related_source_slice(line_slice, "Problematic line");
                    }
                }
                error = error.with_help(
                    "No configured coverage source yielded a witness for this \
                     test annotation (design/witness/spec.md, property W6). If \
                     its checking act runs under a producer not configured in \
                     this invocation, add that coverage source; otherwise this \
                     annotation points at behavior nothing checked.",
                );
                writeln!(f, "{error:?}")?;
            }
            writeln!(f)?;
        }

        // Tests that cite a spec section nobody implements (design §2.4).
        if missing_implementation > 0 {
            for test in &self.missing_implementation {
                let error = error!("Test has no correlated implementation")
                    .with_source_slice(test.original_text.clone(), "Test annotation")
                    .with_help(
                        "This test cites a specification section that no \
                         implementation/citation annotation references. Add an \
                         implementation annotation for the same section, or fix \
                         the test's target.",
                    );
                writeln!(f, "{error:?}")?;
            }
            writeln!(f)?;
        }

        // Show failed correlations with detailed diagnostics
        if failed > 0 {
            for correlation in &self.failed {
                let mut error = error!("Failed correlation");

                let test_annotation_message = match correlation.test_execution_status {
                    ExecutionStatus::Executed => "Executed test",
                    ExecutionStatus::NotExecuted => "Not executed test",
                    ExecutionStatus::Structural => {
                        "Test target is purely declarative; no executable code to verify"
                    }
                    ExecutionStatus::Unknown { .. } => {
                        "Not executed because of an unknown not executable line."
                    }
                };

                // Add test annotation context
                error = error.with_source_slice(
                    correlation.test.original_text.clone(),
                    test_annotation_message,
                );

                // If test execution status is Unknown, add the problematic line
                if let ExecutionStatus::Unknown { line_number } = correlation.test_execution_status
                {
                    if let Some(line_slice) = get_line_slice(&correlation.test, line_number) {
                        error = error.with_related_source_slice(line_slice, "Problematic line");
                    }
                }

                error = with_related_annotations(
                    error,
                    &correlation.executed_implementations,
                    "Executed implementation",
                );

                error = with_related_not_executed_annotations(
                    error,
                    &correlation.not_executed_implementations,
                    |status| match status {
                        ExecutionStatus::NotExecuted => "Not executed implementation",
                        ExecutionStatus::Structural => {
                            "Structural implementation target — no executable code to verify"
                        }
                        ExecutionStatus::Unknown { .. } => {
                            "Not executed because of an unknown not executable line."
                        }
                        ExecutionStatus::Executed => unreachable!("Executed implementation"), // shouldn't happen
                    },
                );

                // Spec §3 (Decision 14): for every failing pair, list
                // EVERY bound witness with its per-witness result and
                // strength — the verdict is universal, and the
                // disagreement must never be silent. ✓/✗ per witness,
                // in bound order, per failing implementation.
                let mut help_lines: Vec<String> = Vec::new();
                for not_executed in &correlation.not_executed_implementations {
                    for result in &not_executed.per_witness {
                        help_lines.push(format!(
                            "{} {} ({}): {}",
                            if result.executed { "✓" } else { "✗" },
                            result.witness.label,
                            result.witness.strength,
                            if result.executed {
                                "executed the implementation"
                            } else {
                                "did not execute the implementation"
                            },
                        ));
                    }
                }
                if !help_lines.is_empty() {
                    error = error.with_help(format!(
                        "Every witness bound to this test, with its \
                         per-witness result (a pair is discharged only when \
                         ALL bound witnesses executed the implementation — \
                         design/witness/decisions.md, Decision 14):\n{}",
                        help_lines.join("\n")
                    ));
                }

                writeln!(f, "{error:?}")?;
            }
            writeln!(f)?;
        }

        // Show successful correlations in verbose mode
        if self.verbose {
            for correlation in &self.successful {
                let mut info = info!("Successful correlation");

                // Add test annotation context
                info = info
                    .with_source_slice(correlation.test.original_text.clone(), "Test annotation");

                info = with_related_annotations(
                    info,
                    &correlation.executed_implementations,
                    "Executed implementation",
                );

                // Spec §3 (Decision 14): for every discharged pair, name
                // EVERY bound witness (label + strength) — the discharge
                // claim is that all of them executed the implementation.
                let discharged_by = correlation
                    .bound_witnesses
                    .iter()
                    .map(|w| format!("discharged by {} ({})", w.label, w.strength))
                    .collect::<Vec<_>>()
                    .join("\n");
                info = info.with_help(discharged_by);

                // correlation in successful ==>
                //  correlation.test_execution_status == ExecutionStatus::Executed ==>
                //  correlation.not_executed_implementations.is_empty()

                writeln!(f, "{info:?}")?;
            }

            let mut executed_annotation = info!("Executed annotations");
            executed_annotation = with_related_annotations(
                executed_annotation,
                &self.executed_tests.iter().cloned().collect::<Vec<_>>(),
                "Executed test",
            );
            executed_annotation = with_related_annotations(
                executed_annotation,
                &self
                    .executed_implementations
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>(),
                "Executed implementation",
            );
            writeln!(f, "{executed_annotation:?}")?;

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

        for (category_name, duplicates) in &self.categories {
            if !duplicates.duplicates.is_empty() {
                for coverage in &duplicates.duplicates {
                    let duplicate_error = coverage2error(
                        coverage,
                        format!("Duplicate {} annotations", category_name.to_lowercase()),
                        "Duplicate".to_string(),
                        "Duplicate".to_string(),
                    );
                    writeln!(f, "{duplicate_error:?}")?;
                }
            }
        }

        if self.verbose {
            for (category_name, duplicates) in &self.categories {
                if !duplicates.some_overlap.is_empty() {
                    for coverage in &duplicates.some_overlap {
                        let mut overlap_info =
                            info!("{} annotations with some overlap", category_name);
                        match coverage.covering_annotations.split_first() {
                            Some((first, rest)) => {
                                overlap_info = with_annotation(overlap_info, first, "Some overlap");
                                overlap_info =
                                    with_related_annotations(overlap_info, rest, "Some overlap");
                            }
                            // Whitespace-only quote: trivially covered, no coverers. Render
                            // the target rather than panicking on an empty slice.
                            None => {
                                overlap_info =
                                    with_annotation(overlap_info, &coverage.target, "Some overlap");
                            }
                        }
                        writeln!(f, "{overlap_info:?}")?;
                    }
                }

                if !duplicates.unique.is_empty() {
                    let mut unique_info =
                        info!("Unique {} annotations", category_name.to_lowercase());
                    unique_info =
                        with_related_annotations(unique_info, &duplicates.unique, "Unique");
                    writeln!(f, "{unique_info:?}")?;
                }
            }
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
    message: fn(ExecutionStatus) -> &'static str,
) -> duvet_core::diagnostic::Error {
    for annotation in annotations {
        // Always show the annotation context first
        error = error.with_related_source_slice(
            annotation.annotation.original_text.clone(),
            message(annotation.status),
        );

        // If we have line number information for Unknown status, add specific line context
        if let ExecutionStatus::Unknown { line_number } = annotation.status {
            if let Some(line_slice) = get_line_slice(&annotation.annotation, line_number) {
                error = error.with_related_source_slice(line_slice, "Problematic line");
            }
        }
    }
    error
}

/// Helper function to get a slice for a specific line
fn get_line_slice(
    annotation: &Arc<Annotation>,
    line_number: u64,
) -> Option<duvet_core::file::Slice<duvet_core::file::SourceFile>> {
    // Get the source file from the annotation
    let idx = usize::try_from(line_number).ok()?.checked_sub(1)?;
    annotation
        .original_text
        .file()
        .lines_slices()
        // nth is 0 based, but line numbers in source are 1 based.
        .nth(idx)
}
