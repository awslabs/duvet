// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    annotation::{Annotation, AnnotationSet, AnnotationType},
    comment::Pattern,
    query::{
        coverage::ExecutionStatus,
        parsers::verus_sst::witness::NOT_PROOF_TESTABLE,
        witness::{PairWitnessResult, WitnessRef},
    },
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
    pub missing_implementation: Vec<MissingImplementationTest>,
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
    /// implementation is universal over this set (spec §1.6).
    pub bound_witnesses: Vec<WitnessRef>,
    /// Implementations every bound witness executed (discharged).
    pub executed_implementations: Vec<Arc<Annotation>>,
    pub not_executed_implementations: Vec<NotExecutedAnnotation>,
}

#[derive(Debug)]
//= design/witness/spec.md#verdict-output
//= type=implementation
//# For every pair that fails because a bound witness did not execute
//# the implementation (W1's universal clause), the output MUST list
//# **every** bound witness with its per-witness result
//# (executed I / did not execute I) and strength,
//# so the failing claim is identifiable —
pub struct NotExecutedAnnotation {
    pub annotation: Arc<Annotation>,
    pub status: ExecutionStatus,
    /// Every bound witness's result for this pair, in bound order —
    /// recomputed from the same verified cell (`is_executed_by`) that
    /// the discharge verdict evaluated internally, which is what
    /// preserves glue obligation G2 (spec §4.4: diagnostics derive
    /// from the same verified cells; no parallel verdict computation).
    pub per_witness: Vec<PairWitnessResult>,
}

#[derive(Debug)]
//= design/witness/spec.md#verdict-output
//= type=implementation
//# For every unwitnessed test annotation (W6), the output MUST
//# identify the annotation and state that no configured producer
//# yielded a witness for it.
pub struct UnwitnessedTestAnnotation {
    pub test: Arc<Annotation>,
    /// The test's own execution status folded across all delivered
    /// witnesses — diagnostic detail only (Unknown carries a line).
    pub diagnostic_status: ExecutionStatus,
    /// A prover producer elaborated this test's resolved target but no
    /// obligation is rooted there (spec §5.2): no proof
    /// witness can exist for it by definition. Refines the report —
    /// distinct from W6's "no configured producer yielded a witness" —
    /// never the verdict.
    pub not_proof_testable: bool,
}

/// A test annotation reported under "no correlated implementation"
/// (design §2.4). W6 quantifies over EVERY test annotation, including
/// these — they fail before reaching the unwitnessed check, so the
/// "no producer witnessed this" fact rides on this report rather
/// than being lost.
#[derive(Debug)]
pub struct MissingImplementationTest {
    pub test: Arc<Annotation>,
    /// No delivered witness binds this test either (property W6):
    /// carried as a fact on this report, not a separate category.
    pub unwitnessed: bool,
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
        // means unwitnessed by ALL of them (spec W6).
        if unwitnessed > 0 {
            for entry in &self.unwitnessed {
                let detail = if entry.not_proof_testable {
                    "Not proof-testable test target"
                } else {
                    match entry.diagnostic_status {
                        // Not a contradiction: coverage of the test's LINES is
                        // evidence-claiming (ByExecution) only. A prover
                        // witness bound elsewhere routinely consults these
                        // lines in its closure without binding this
                        // annotation — binding for ByRootSpan witnesses is
                        // positional (spec §1.5).
                        ExecutionStatus::Executed => {
                            "Test lines reached only by witnesses bound elsewhere"
                        }
                        status => status_message(status, MessageRole::Test),
                    }
                };
                let mut error = error!("Unwitnessed test annotation")
                    .with_source_slice(entry.test.original_text.clone(), detail);
                if let ExecutionStatus::Unknown { line_number } = entry.diagnostic_status {
                    if let Some(line_slice) = get_line_slice(&entry.test, line_number) {
                        error = error.with_related_source_slice(line_slice, "Problematic line");
                    }
                }
                //= design/witness/spec.md#two-pass-construction
                //= type=implementation
                //# and the report MUST identify the annotation as
                //# *not proof-testable* ("this position carries no dischargeable
                //# obligation; it can only be witnessed by an execution-style
                //# producer") — a report distinct from Property W6's
                //# "no witness from any configured producer."
                if entry.not_proof_testable {
                    error = error.with_help(format!(
                        "A prover elaborated this annotation's target, but \
                         {NOT_PROOF_TESTABLE} (design/witness/spec.md §5.2). \
                         Move the annotation to a proof-element position \
                         (fn/lemma header, ensures clause, loop invariant, \
                         proof assert), or configure an execution-style \
                         coverage source that runs this code.",
                    ));
                } else {
                    error = error.with_help(
                        "No configured coverage source yielded a witness for this \
                         test annotation (design/witness/spec.md, property W6). If \
                         its checking act runs under a producer not configured in \
                         this invocation, add that coverage source; otherwise this \
                         annotation points at behavior nothing checked.",
                    );
                }
                writeln!(f, "{error:?}")?;
            }
            writeln!(f)?;
        }

        // Tests that cite a spec section nobody implements (design §2.4).
        if missing_implementation > 0 {
            for entry in &self.missing_implementation {
                const MISSING_IMPL_HELP: &str = "This test cites a specification section that no \
                     implementation/citation annotation references. Add an \
                     implementation annotation for the same section, or fix \
                     the test's target.";
                let error = error!("Test has no correlated implementation")
                    .with_source_slice(entry.test.original_text.clone(), "Test annotation");
                // Spec W6 quantifies over every test annotation: the
                // unwitnessed fact is stated here rather than lost to the
                // missing-implementation failure.
                let error = if entry.unwitnessed {
                    error.with_help(format!(
                        "{MISSING_IMPL_HELP} Additionally, no configured \
                         coverage source yielded a witness for this test \
                         annotation (design/witness/spec.md, property W6)."
                    ))
                } else {
                    error.with_help(MISSING_IMPL_HELP)
                };
                writeln!(f, "{error:?}")?;
            }
            writeln!(f)?;
        }

        // Show failed correlations with detailed diagnostics
        if failed > 0 {
            for correlation in &self.failed {
                let mut error = error!("Failed correlation");

                let test_annotation_message =
                    status_message(correlation.test_execution_status, MessageRole::Test);

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
                        ExecutionStatus::Executed => unreachable!("Executed implementation"), // shouldn't happen
                        status => status_message(status, MessageRole::Implementation),
                    },
                );

                // ✓/✗ per witness, in bound order, per failing
                // implementation.
                //= design/witness/spec.md#verdict-output
                //# and the disagreement MUST never be silent
                //# (decisions.md, [Decision 14](decisions.md#decision-14);
                //# legibility improvements are tracked in
                //# [Follow-ups](decisions.md#follow-ups) and never weaken the verdict).
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
                         design/witness/spec.md §1.6):\n{}",
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

                // Spec §3: for every discharged pair, name
                // EVERY bound witness (label + strength) — the discharge
                // claim is that all of them executed the implementation.
                // Scoped to verbose output by §3:
                // success detail is verbose-gated; failure detail never is.
                //
                // A witness label may name a clause-level unit; the strength
                // qualifier rendered beside every label is what keeps unit
                // granularity from reading as evidence granularity:
                //= design/witness/spec.md#closure
                //= type=implementation
                //# Reports and documentation MUST NOT present clause-level units as
                //# implying clause-level evidence.
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

/// Which kind of annotation a diagnostic message describes. The
/// [`ExecutionStatus`] wording differs by role (a `Structural` test
/// target reads differently from a `Structural` implementation target),
/// so the role is part of the lookup key.
#[derive(Clone, Copy)]
enum MessageRole {
    Test,
    Implementation,
}

/// Single source of truth for the human-readable message attached to an
/// annotation with a given execution status. Every display site routes
/// through this mapping so the wording cannot drift between sites.
/// (Context-specific `Executed` messages — the unwitnessed report's
/// "reached only by witnesses bound elsewhere", the failing-pair
/// closure's unreachable arm — stay at their sites; everything else is
/// this table.)
fn status_message(status: ExecutionStatus, role: MessageRole) -> &'static str {
    match (status, role) {
        (ExecutionStatus::Executed, MessageRole::Test) => "Executed test",
        (ExecutionStatus::Executed, MessageRole::Implementation) => "Executed implementation",
        (ExecutionStatus::NotExecuted, MessageRole::Test) => "Not executed test",
        (ExecutionStatus::NotExecuted, MessageRole::Implementation) => {
            "Not executed implementation"
        }
        (ExecutionStatus::Structural, MessageRole::Test) => {
            "Test target is purely declarative; no executable code to verify"
        }
        (ExecutionStatus::Structural, MessageRole::Implementation) => {
            "Structural implementation target — no executable code to verify"
        }
        (ExecutionStatus::Unknown { .. }, _) => {
            "Not executed because of an unknown not executable line."
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{annotation::AnnotationLevel, query::witness::Strength};
    use duvet_core::file::SourceFile as CoreSourceFile;
    use std::collections::BTreeSet;

    fn annotation(path: &str, anno: AnnotationType) -> Arc<Annotation> {
        let contents = "//= spec#s\ncode();\n";
        let source = CoreSourceFile::new(path, contents).unwrap();
        let text = source.substr_range(0..10).unwrap();
        let target = source.substr_range(4..10).unwrap();
        let quote = source.substr_range(11..18).unwrap();
        Arc::new(Annotation {
            source: source.path().clone(),
            anno_line: 1,
            original_target: target,
            original_text: text,
            original_quote: quote,
            anno,
            target: "spec#s".to_string(),
            quote: String::new(),
            comment: String::new(),
            manifest_dir: source.path().clone(),
            level: AnnotationLevel::Auto,
            format: crate::specification::Format::Auto,
            tracking_issue: String::new(),
            feature: String::new(),
            tags: Default::default(),
            blob_link: None,
        })
    }

    fn witness_ref(label: &str, strength: Strength) -> WitnessRef {
        WitnessRef {
            label: label.to_string(),
            strength,
        }
    }

    fn coverage_result() -> CoverageResult {
        CoverageResult {
            status: QueryStatus::Pass,
            report_count: 1,
            executed_tests: Arc::new(BTreeSet::new()),
            executed_implementations: Arc::new(BTreeSet::new()),
            successful: vec![],
            failed: vec![],
            missing_implementation: vec![],
            unwitnessed: vec![],
            verbose: false,
        }
    }

    /// Rendered diagnostics wrap and indent; squash whitespace so the
    /// assertions are insensitive to the renderer's line-breaking.
    fn squash(rendered: &str) -> String {
        rendered.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// Spec §3, failure clause: the failing pair's output lists EVERY
    /// bound witness with its per-witness result and strength — the ✓
    /// witness that executed the implementation AND the ✗ witness that
    /// did not — so the failing claim is identifiable, and the
    /// disagreement between them is printed, never silent.
    #[test]
    fn failing_pair_output_lists_every_bound_witness() {
        let mut result = coverage_result();
        result.status = QueryStatus::Fail;
        result.failed = vec![CoveredTestAnnotation {
            test: annotation("t.rs", AnnotationType::Test),
            test_execution_status: ExecutionStatus::Executed,
            bound_witnesses: vec![
                witness_ref("a.xml", Strength::Executed),
                witness_ref("c::obl", Strength::Consulted),
            ],
            executed_implementations: vec![],
            not_executed_implementations: vec![NotExecutedAnnotation {
                annotation: annotation("i.rs", AnnotationType::Citation),
                status: ExecutionStatus::NotExecuted,
                per_witness: vec![
                    PairWitnessResult {
                        witness: witness_ref("a.xml", Strength::Executed),
                        executed: true,
                        status: ExecutionStatus::Executed,
                    },
                    PairWitnessResult {
                        witness: witness_ref("c::obl", Strength::Consulted),
                        executed: false,
                        status: ExecutionStatus::NotExecuted,
                    },
                ],
            }],
        }];
        let out = squash(&format!("{result}"));
        // Every bound witness, its per-witness result, and its strength:
        //= design/witness/spec.md#verdict-output
        //= type=test
        //# For every pair that fails because a bound witness did not execute
        //# the implementation (W1's universal clause), the output MUST list
        //# **every** bound witness with its per-witness result
        //# (executed I / did not execute I) and strength,
        //# so the failing claim is identifiable —
        assert!(
            out.contains("✓ a.xml (executed): executed the implementation"),
            "missing the executing witness's result: {out}"
        );
        assert!(
            out.contains("✗ c::obl (consulted): did not execute the implementation"),
            "missing the failing witness's result: {out}"
        );
        // The disagreement is printed with the universal-discharge rule:
        //= design/witness/spec.md#verdict-output
        //= type=test
        //# and the disagreement MUST never be silent
        //# (decisions.md, [Decision 14](decisions.md#decision-14);
        //# legibility improvements are tracked in
        //# [Follow-ups](decisions.md#follow-ups) and never weaken the verdict).
        assert!(
            out.contains("ALL bound witnesses executed the implementation"),
            "missing the discharge rule statement: {out}"
        );
    }

    /// Spec §3, success clause: verbose output names every bound witness
    /// (label) and its strength for a discharged pair.
    #[test]
    fn verbose_output_names_every_bound_witness_for_discharged_pairs() {
        let mut result = coverage_result();
        result.verbose = true;
        result.successful = vec![CoveredTestAnnotation {
            test: annotation("t.rs", AnnotationType::Test),
            test_execution_status: ExecutionStatus::Executed,
            bound_witnesses: vec![
                witness_ref("a.xml", Strength::Executed),
                witness_ref("c::obl", Strength::Consulted),
            ],
            executed_implementations: vec![annotation("i.rs", AnnotationType::Citation)],
            not_executed_implementations: vec![],
        }];
        let out = squash(&format!("{result}"));
        //= design/witness/spec.md#verdict-output
        //= type=test
        //# For every discharged pair, the output MUST name, in verbose
        //# output, every bound witness (its label) and its strength ([§1.3](#provenance)).
        assert!(
            out.contains("discharged by a.xml (executed)"),
            "missing the runtime witness's name and strength: {out}"
        );
        assert!(
            out.contains("discharged by c::obl (consulted)"),
            "missing the prover witness's name and strength: {out}"
        );

        // Non-verbose output does NOT name them (the naming is
        // verbose-gated; failure output never is — see the failing-pair
        // test above, which runs with verbose off).
        result.verbose = false;
        let out = squash(&format!("{result}"));
        assert!(!out.contains("discharged by a.xml"));
    }

    /// Spec §3, W6 clause: the unwitnessed report identifies the
    /// annotation (its source slice is rendered) and states that no
    /// configured producer yielded a witness for it.
    #[test]
    fn unwitnessed_output_identifies_annotation_and_producer_absence() {
        let mut result = coverage_result();
        result.status = QueryStatus::Fail;
        result.unwitnessed = vec![UnwitnessedTestAnnotation {
            test: annotation("t.rs", AnnotationType::Test),
            diagnostic_status: ExecutionStatus::NotExecuted,
            not_proof_testable: false,
        }];
        let out = squash(&format!("{result}"));
        //= design/witness/spec.md#verdict-output
        //= type=test
        //# For every unwitnessed test annotation (W6), the output MUST
        //# identify the annotation and state that no configured producer
        //# yielded a witness for it.
        assert!(
            out.contains("Unwitnessed test annotation"),
            "missing the report category: {out}"
        );
        // The annotation is identified by its rendered source:
        assert!(out.contains("t.rs"), "missing the annotation's file: {out}");
        assert!(
            out.contains("No configured coverage source yielded a witness"),
            "missing the producer-absence statement: {out}"
        );
    }

    /// Producer-sections slice helpers (grafted at integration).
    fn annotation_t(path: &str) -> Arc<Annotation> {
        annotation(path, AnnotationType::Test)
    }

    /// The two unwitnessed reports are DISTINCT: the not-proof-testable
    /// entry carries the producer's verbatim reason sentence, the plain
    /// W6 entry carries the no-configured-producer help — and neither
    /// text appears on the other entry.
    #[test]
    fn unwitnessed_report_distinguishes_not_proof_testable_from_w6() {
        let mut result = coverage_result();
        result.unwitnessed = vec![
            UnwitnessedTestAnnotation {
                test: annotation_t("npt.rs"),
                diagnostic_status: ExecutionStatus::Executed,
                not_proof_testable: true,
            },
            UnwitnessedTestAnnotation {
                test: annotation_t("w6.rs"),
                diagnostic_status: ExecutionStatus::NotExecuted,
                not_proof_testable: false,
            },
        ];
        let rendered = format!("{result}");
        // The error renderer wraps help text; collapse whitespace before
        // matching the normative sentence.
        let normalized = rendered.split_whitespace().collect::<Vec<_>>().join(" ");

        // Both entries are reported (never silently dropped).
        assert!(rendered.contains("npt.rs"), "{rendered}");
        assert!(rendered.contains("w6.rs"), "{rendered}");

        // The NPT entry carries the producer's verbatim sentence …
        //= design/witness/spec.md#two-pass-construction
        //= type=test
        //# and the report MUST identify the annotation as
        //# *not proof-testable* ("this position carries no dischargeable
        //# obligation; it can only be witnessed by an execution-style
        //# producer") — a report distinct from Property W6's
        //# "no witness from any configured producer."
        assert!(
            normalized.contains(NOT_PROOF_TESTABLE),
            "not-proof-testable report must quote the normative sentence:\n{rendered}"
        );
        assert!(
            rendered.contains("Not proof-testable test target"),
            "{rendered}"
        );
        // … and the plain-W6 wording is present for the other entry.
        assert!(
            normalized.contains("No configured coverage source yielded a witness"),
            "plain W6 entry keeps its own distinct wording:\n{rendered}"
        );
        // Distinctness: exactly one entry of each kind, so each help
        // text appears exactly once.
        assert_eq!(
            normalized.matches(NOT_PROOF_TESTABLE).count(),
            1,
            "{rendered}"
        );
        assert_eq!(
            normalized
                .matches("No configured coverage source yielded a witness")
                .count(),
            1,
            "{rendered}"
        );
    }

    /// Every witness reference in the verdict output carries its
    /// strength qualifier next to the label — a clause-level unit label
    /// like `c::f ensures[0]` is always presented as `(consulted)`
    /// evidence, never bare.
    #[test]
    fn witness_references_always_carry_their_strength_qualifier() {
        let clause_label = "c::f ensures[0]";
        let witness_ref = || WitnessRef {
            label: clause_label.to_string(),
            strength: Strength::Consulted,
        };

        // Success path (verbose): "discharged by <label> (<strength>)".
        let mut result = coverage_result();
        result.verbose = true;
        result.successful = vec![CoveredTestAnnotation {
            test: annotation_t("ok.rs"),
            test_execution_status: ExecutionStatus::Executed,
            bound_witnesses: vec![witness_ref()],
            executed_implementations: vec![],
            not_executed_implementations: vec![],
        }];
        let rendered = format!("{result}");
        //= design/witness/spec.md#closure
        //= type=test
        //# Reports and documentation MUST NOT present clause-level units as
        //# implying clause-level evidence.
        assert!(
            rendered.contains("discharged by c::f ensures[0] (consulted)"),
            "clause-unit label must carry its strength qualifier:\n{rendered}"
        );

        // Failure path (never verbose-gated): per-witness ✗ lines.
        let mut result = coverage_result();
        result.failed = vec![CoveredTestAnnotation {
            test: annotation_t("fail.rs"),
            test_execution_status: ExecutionStatus::Executed,
            bound_witnesses: vec![witness_ref()],
            executed_implementations: vec![],
            not_executed_implementations: vec![NotExecutedAnnotation {
                annotation: annotation_t("impl.rs"),
                status: ExecutionStatus::NotExecuted,
                per_witness: vec![PairWitnessResult {
                    witness: witness_ref(),
                    executed: false,
                    status: ExecutionStatus::NotExecuted,
                }],
            }],
        }];
        let rendered = format!("{result}");
        assert!(
            rendered.contains("c::f ensures[0] (consulted)"),
            "per-witness failure line must carry the strength qualifier:\n{rendered}"
        );
    }
}
