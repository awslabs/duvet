// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Degraded (classifier-less) annotation execution status.
//!
//! When a source file has no tree-sitter classifier (e.g. a Kotlin/Scala/Groovy
//! source named in a JVM-wide JaCoCo report), the two-phase model in
//! [`crate::annotation_execution`] cannot run: with no line classifications there
//! is no scope tree and every line resolves to `Unknown`. This module provides a
//! *verified* fallback that reuses the same forward target walk
//! ([`annotation_target`]) but decides the status by consulting coverage directly
//! on the resolved line, instead of the classification/scope model.
//!
//! # Governance definition (Option B)
//!
//! Degraded mode proves target-correctness against the **forward-nearest**
//! definition of governance: *the code a requirement governs is the first line
//! below the annotation that the coverage report has an opinion about, reached
//! over only skippable (whitespace / annotation) lines.* This is weaker than the
//! classified model's scope-based governance but needs no classifier, and — where
//! it emits a verdict on an ordinary hit statement — it agrees with the
//! classified model (see [`lemma_degraded_agrees_with_v2_on_hit_statement`]).
//!
//! Under Option B we deliberately do **not** assume "a coverage opinion implies
//! the line is an executable statement." We take the nearest coverage-opinionated
//! line to *be* the governed line by definition. The one place this can diverge
//! from the classified model — a covered *structural* line (e.g. a brace) that a
//! classifier would have skipped — is a documented, not silent, edge.
//!
//! # Why this is sound without propagation
//!
//! Every degraded verdict is a **direct observation** of the resolved line's own
//! coverage — never an inference propagated from a hit elsewhere. Non-linear
//! control flow only endangers *propagation* (Property 3), and degraded mode does
//! not propagate, so its blindness to `NonLinearControl` cannot cause an unsound
//! inference. It reports only what coverage directly says about the target line.

#[cfg(verus_keep_ghost)]
use crate::{
    annotation_execution::execution_status_of, predicates::validly_in_exec_set,
    target_resolution::annotation_target_spec,
};
use crate::{target_resolution::annotation_target, types::*};
use vstd::prelude::*;

verus! {

/// Spec twin of [`degraded_execution_status`]'s status computation: a pure
/// function of the resolved target line and coverage. `degraded_execution_status`
/// is proven equal to this (see its `ensures`), so the status depends on the
/// annotation only through `annotation_target_spec` — the basis for Property 5
/// (stacking transitivity), proven in [`lemma_degraded_stacking`].
///
/// `None` target (the walk reached EOF over only skippable lines — no observable
/// code below the annotation) yields `Unknown` with the sentinel line number `0`
/// (Decision D1: nothing observable is genuine ignorance, not a declarative
/// `Structural` construct).
pub open spec fn degraded_status_of(
    target: Option<u64>,
    coverage: &CoverageReport,
) -> ExecutionStatus {
    match target {
        None => ExecutionStatus::Unknown { line_number: 0 },
        Some(line) => {
            if coverage@.contains_key(line) {
                if coverage@[line] == CoverageStatus::Hit {
                    ExecutionStatus::Executed
                } else {
                    ExecutionStatus::NotExecuted
                }
            } else {
                ExecutionStatus::Unknown { line_number: line }
            }
        }
    }
}

// TRUST BASE (unverified leaf). Verus cannot reason over `BTreeMap::get`, so this
// body is trusted and only its `ensures` is checked downstream. The spec is a
// total membership+value read: `Some(v)` exactly when the key is present with
// value `v`, `None` otherwise. This mirrors the existing trusted leaves
// `collect_hit_lines` / `vec_from_btreeset` in `execution_propagation`. Keep body
// and `ensures` in exact correspondence: a change to the body (a wrong key, a
// dropped `copied`) would keep every proof green while silently changing what the
// degraded model observes.
#[verifier::external_body]
fn coverage_status_at(coverage: &CoverageReport, line: u64) -> (result: Option<CoverageStatus>)
    ensures
        result == if coverage@.contains_key(line) {
            Some(coverage@[line])
        } else {
            None::<CoverageStatus>
        },
{
    coverage.get(&line).copied()
}

/// Degraded annotation execution status for a file without a classifier.
///
/// Reuses the verified forward target walk ([`annotation_target`]) to resolve the
/// nearest non-skippable line below the annotation, then consults `coverage`
/// directly on that line. Requires no scopes and no meaningful classifications
/// (callers pass the minimal universal classification: `Some({Whitespace})` on
/// blank lines, `Some({Annotation})` on annotation lines, `None` elsewhere).
///
/// Proven properties:
/// - **P9 (spec-twin equivalence):** equals [`degraded_status_of`] of the
///   resolved target and coverage.
/// - **P5 (grounded verdicts / direct observation):** `Executed` only when the
///   resolved target is directly `Hit`; `NotExecuted` only when directly `Miss`.
///   Missing coverage never yields a verdict (it yields `Unknown`).
/// - **P3 (target below annotation):** any decided target lies strictly below
///   `annotation.end_line`.
//= design/query/coverage-model-spec.md#property-d1-direct-observation
//= type=implication
//# The implementation MUST prove that the degraded status is a direct
//# observation of the target line's own coverage, never an inference propagated
//# from another line.
//= design/query/coverage-model-spec.md#property-d2-degraded-target-bounds
//= type=implication
//# The implementation MUST prove that any `Executed` or `NotExecuted` degraded
//# status resolves a target strictly below the annotation
//# (`target > annotation.end_line`).
pub fn degraded_execution_status(
    annotation: &AnnotationSpan,
    classifications: &[Option<LineClass>],
    coverage: &CoverageReport,
    file_length: u64,
) -> (status: ExecutionStatus)
    requires
        annotation.end_line < u64::MAX,
    ensures
        // P9: spec-twin equivalence.
        status == degraded_status_of(
            annotation_target_spec(annotation, classifications, file_length),
            coverage,
        ),
        // P5 + P3: a `Executed` verdict is a direct hit on a target below the annotation.
        status == ExecutionStatus::Executed ==> {
            let t = annotation_target_spec(annotation, classifications, file_length);
            &&& t.is_some()
            &&& t.unwrap() > annotation.end_line
            &&& coverage@.contains_key(t.unwrap())
            &&& coverage@[t.unwrap()] == CoverageStatus::Hit
        },
        // P5 + P3: a `NotExecuted` verdict is a direct miss on a target below the annotation.
        status == ExecutionStatus::NotExecuted ==> {
            let t = annotation_target_spec(annotation, classifications, file_length);
            &&& t.is_some()
            &&& t.unwrap() > annotation.end_line
            &&& coverage@.contains_key(t.unwrap())
            &&& coverage@[t.unwrap()] == CoverageStatus::Miss
        },
{
    let target = annotation_target(annotation, classifications, file_length);
    match target {
        None => {
            proof {
                // annotation_target: result.is_some() <==> spec.is_some().
                assert(annotation_target_spec(annotation, classifications, file_length).is_none());
            }
            ExecutionStatus::Unknown { line_number: 0 }
        }
        Some(t) => {
            proof {
                // annotation_target: result.is_some() <==> spec.is_some(), and when
                // some, result.line_number == spec.unwrap().
                assert(annotation_target_spec(annotation, classifications, file_length).is_some());
                assert(annotation_target_spec(annotation, classifications, file_length)
                    == Some(t.line_number));
            }
            match coverage_status_at(coverage, t.line_number) {
                Some(CoverageStatus::Hit) => ExecutionStatus::Executed,
                Some(CoverageStatus::Miss) => ExecutionStatus::NotExecuted,
                None => ExecutionStatus::Unknown { line_number: t.line_number },
            }
        }
    }
}

/// **P7 (stacking transitivity).** The degraded status depends on the annotation
/// only through its resolved target line: two annotations that resolve to the
/// same target receive the same status. Immediate from the functional form of
/// [`degraded_status_of`].
//= design/query/coverage-model-spec.md#property-d3-degraded-stacking
//= type=implication
//# The implementation MUST prove that the degraded status depends on the
//# annotation only through its resolved target: two annotations that resolve to
//# the same target receive the same status.
pub proof fn lemma_degraded_stacking(
    a: &AnnotationSpan,
    b: &AnnotationSpan,
    classifications: &[Option<LineClass>],
    coverage: &CoverageReport,
    file_length: u64,
)
    requires
        annotation_target_spec(a, classifications, file_length)
            == annotation_target_spec(b, classifications, file_length),
    ensures
        degraded_status_of(annotation_target_spec(a, classifications, file_length), coverage)
            == degraded_status_of(annotation_target_spec(b, classifications, file_length), coverage),
{
}

/// **P8 (agreement with the classified model on the decided domain).** Where the
/// degraded model emits a verdict on a directly-`Hit` line that a full classifier
/// would mark as a plain `Statement` (not `NonLinearControl`), it returns exactly
/// what the classified model [`execution_status_of`] returns: `Executed`. This is
/// the formal justification that a degraded verdict needs no provenance marker on
/// this domain — it *means* the same thing.
///
/// The Option B divergence set (`NonLinearControl` and `Declaration` targets, and
/// covered structural lines) is exactly what this lemma's hypotheses exclude.
//= design/query/coverage-model-spec.md#property-d4-agreement-with-classified
//= type=implication
//# The implementation MUST prove that where the degraded path emits a verdict on
//# a directly-hit line that a full classifier would mark as a plain `Statement`
//# (and not `NonLinearControl`), it returns the same result the classified model
//# `is_annotation_executed` returns, namely `Executed`.
pub proof fn lemma_degraded_agrees_with_v2_on_hit_statement(
    line: u64,
    classifications: &[Option<LineClass>],
    scopes: &[Scope],
    coverage: &CoverageReport,
)
    requires
        (line as int - 1) >= 0,
        (line as int - 1) < classifications@.len(),
        classifications@[line as int - 1].is_some(),
        classifications@[line as int - 1].unwrap()@.contains(LineProperty::Statement),
        !classifications@[line as int - 1].unwrap()@.contains(LineProperty::NonLinearControl),
        coverage@.contains_key(line),
        coverage@[line] == CoverageStatus::Hit,
    ensures
        execution_status_of(Some(line), classifications, scopes, coverage)
            == ExecutionStatus::Executed,
        degraded_status_of(Some(line), coverage) == ExecutionStatus::Executed,
{
    // Directly-hit lines are validly in the execution set by the first disjunct,
    // so `execution_status_of` takes the `Executed` branch (the target is
    // classified, not NonLinearControl, and in the execution set).
    assert(validly_in_exec_set(line, classifications, scopes, coverage));
}

} // verus!

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;

    fn s(props: &[LineProperty]) -> Option<LineClass> {
        Some(line_class(props))
    }
    fn cov_hit(lines: &[u64]) -> CoverageReport {
        lines.iter().map(|&l| (l, CoverageStatus::Hit)).collect()
    }
    fn cov_miss(lines: &[u64]) -> CoverageReport {
        lines.iter().map(|&l| (l, CoverageStatus::Miss)).collect()
    }

    // Minimal universal classification: whitespace + annotation known, rest None.
    #[test]
    fn hit_on_nearest_line_is_executed() {
        // annotation lines 1-2, blank line 3, covered code line 4.
        let c = vec![
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::Whitespace]),
            None,
        ];
        assert_eq!(
            degraded_execution_status(
                &AnnotationSpan {
                    start_line: 1,
                    end_line: 2
                },
                &c,
                &cov_hit(&[4]),
                4
            ),
            ExecutionStatus::Executed
        );
    }

    #[test]
    fn miss_on_nearest_line_is_not_executed() {
        let c = vec![s(&[LineProperty::Annotation]), None];
        assert_eq!(
            degraded_execution_status(
                &AnnotationSpan {
                    start_line: 1,
                    end_line: 1
                },
                &c,
                &cov_miss(&[2]),
                2
            ),
            ExecutionStatus::NotExecuted
        );
    }

    #[test]
    fn no_opinion_on_nearest_line_is_unknown() {
        // Line 2 is unclassified (could be a comment or uninstrumented code) and
        // coverage has no opinion on it: genuinely ambiguous -> Unknown.
        let c = vec![s(&[LineProperty::Annotation]), None, None];
        assert_eq!(
            degraded_execution_status(
                &AnnotationSpan {
                    start_line: 1,
                    end_line: 1
                },
                &c,
                &cov_hit(&[3]),
                3
            ),
            ExecutionStatus::Unknown { line_number: 2 }
        );
    }

    #[test]
    fn eof_target_is_unknown_sentinel() {
        // Annotation at EOF with only whitespace below: no observable code (D1).
        let c = vec![
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::Whitespace]),
        ];
        assert_eq!(
            degraded_execution_status(
                &AnnotationSpan {
                    start_line: 1,
                    end_line: 1
                },
                &c,
                &CoverageReport::new(),
                2
            ),
            ExecutionStatus::Unknown { line_number: 0 }
        );
    }

    #[test]
    fn stacked_annotations_same_status() {
        // Two stacked annotations resolve to the same covered target (line 5).
        let c = vec![
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::Annotation]),
            None,
        ];
        let cov = cov_hit(&[5]);
        let first = degraded_execution_status(
            &AnnotationSpan {
                start_line: 1,
                end_line: 2,
            },
            &c,
            &cov,
            5,
        );
        let second = degraded_execution_status(
            &AnnotationSpan {
                start_line: 3,
                end_line: 4,
            },
            &c,
            &cov,
            5,
        );
        assert_eq!(first, second);
        assert_eq!(first, ExecutionStatus::Executed);
    }
}
