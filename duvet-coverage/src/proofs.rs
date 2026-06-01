// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Correctness properties for the coverage model (spec Section 5).

//= design/query/coverage-model-spec.md#correctness-properties
//# These properties MUST be proven with Verus.

//= design/query/coverage-model-spec.md#correctness-properties
//= type=implication
//# The Verus proof files MUST carry
//# duvet annotations linking each `proof fn` back to the corresponding property
//# section in this document.

#[cfg(verus_keep_ghost)]
use crate::predicates::{
    all_lines_skippable, line_is_skippable, scope_contains, scopes_match_classifications,
    scopes_well_formed,
};
#[cfg(verus_keep_ghost)]
use crate::predicates::{
    clear_path, has_valid_path, in_scope, propagated_within_scope, scope_has_non_linear_control,
    validly_in_exec_set,
};
#[cfg(verus_keep_ghost)]
use crate::{
    annotation_execution::execution_status_of,
    target_resolution::{annotation_target_spec, annotation_target_walk},
};
use crate::{
    annotation_execution::is_annotation_executed, execution_propagation::execution_set,
    target_resolution::annotation_target, types::*,
};
use vstd::prelude::*;

verus! {

//= design/query/coverage-model-spec.md#property-2-no-cross-scope-leakage
//# The implementation MUST prove that for any two lines A and B where A is in
//# scope S1 and B is in scope S2 and S1 ≠ S2 and S1 is not a parent of S2 and
//# S2 is not a parent of S1:
/// Property 2: No Cross-Scope Leakage.
///
/// Lemma: if scopes are well-formed and line B is in the execution set but not
/// directly hit, then B's propagation source must be in the same scope as B.
/// A hit in an unrelated scope cannot cause B to appear in the execution set.
///
/// Proof: execution_set ensures validly_in_exec_set for every line in the result.
/// For non-directly-hit lines, has_valid_path requires propagated_within_scope,
/// which requires in_scope(line, scopes, scope_idx) && in_scope(hit_line, scopes, scope_idx).
/// By scopes_well_formed, if the hit is in scope S1 and line B is in scope S2,
/// and S1 and S2 are unrelated (neither contains the other), then they don't
/// overlap — so no single scope_idx can contain both. Contradiction.
proof fn lemma_no_cross_scope_leakage(
    exec_set: Set<u64>,
    classifications: &[Option<LineClass>],
    scopes: &[Scope],
    coverage: &CoverageReport,
    line_b: u64,
    s2_idx: int,
)
    requires
        // exec_set satisfies execution_set's postcondition
        forall|line: u64| exec_set.contains(line)
            ==> validly_in_exec_set(line, classifications, scopes, coverage),
        // scopes are well-formed
        scopes_well_formed(scopes@),
        scopes_match_classifications(scopes@, classifications),
        // line_b is in scope s2
        in_scope(line_b, scopes, s2_idx),
        // line_b is NOT directly hit
        !(coverage@.contains_key(line_b) && coverage@[line_b] == CoverageStatus::Hit),
        // line_b IS in the execution set
        exec_set.contains(line_b),
    ensures
        // Then there must be a hit within s2 (not from an unrelated scope)
        exists|hit_line: u64|
            coverage@.contains_key(hit_line)
            && coverage@[hit_line] == CoverageStatus::Hit
            && in_scope(hit_line, scopes, s2_idx),
{
    // line_b is in exec_set, so validly_in_exec_set holds
    assert(validly_in_exec_set(line_b, classifications, scopes, coverage));

    // line_b is not directly hit, so the second disjunct must hold:
    // exists|hit_line, scope_idx| has_valid_path(line_b, hit_line, ..., scope_idx, ...)
    // Let Verus pick the witnesses:
    let (hit_line, scope_idx): (u64, int) = choose|hit_line: u64, scope_idx: int|
        has_valid_path(line_b, hit_line, classifications, scopes, scope_idx, coverage);

    // From has_valid_path we get:
    assert(coverage@.contains_key(hit_line));
    assert(coverage@[hit_line] == CoverageStatus::Hit);
    assert(propagated_within_scope(line_b, hit_line, scopes, scope_idx));
    assert(in_scope(line_b, scopes, scope_idx));
    assert(in_scope(hit_line, scopes, scope_idx));

    // We also know in_scope(line_b, scopes, s2_idx) from requires.
    // So line_b is in both scope_idx and s2_idx.

    // If scope_idx == s2_idx, hit_line is in s2 and we're done.
    // If scope_idx != s2_idx, both scopes contain line_b so they overlap.
    // By scopes_well_formed, one contains the other.
    // Since hit_line is in scope_idx, and scope_idx overlaps with s2_idx,
    // hit_line is in the containing scope, which includes s2_idx's range.

    // The witness for the ensures is hit_line itself.
    // We need: in_scope(hit_line, scopes, s2_idx).
    if scope_idx == s2_idx as int {
        assert(in_scope(hit_line, scopes, s2_idx));
    } else {
        // line_b is in both scopes, so they overlap
        assert(scopes@[scope_idx].open_line <= scopes@[s2_idx].close_line);
        assert(scopes@[s2_idx].open_line <= scopes@[scope_idx].close_line);
        // By well-formedness, one contains the other
        assert(scope_contains(scopes@, scope_idx, s2_idx) || scope_contains(scopes@, s2_idx, scope_idx));

        if scope_contains(scopes@, scope_idx, s2_idx) {
            // scope_idx strictly contains s2_idx.
            // hit_line is in scope_idx. We need hit_line in s2_idx.
            // hit_line > line_b (from clear_path). line_b >= s2.open_line.
            // So hit_line > s2.open_line, meaning hit_line >= s2.open_line.
            //
            // Suppose hit_line > s2.close_line. Then s2.close_line is strictly
            // between line_b and hit_line. By scopes_match_classifications,
            // classifications[s2.close_line - 1] has ScopeClose.
            // But clear_path says no ScopeClose between line_b and hit_line.
            // Contradiction. So hit_line <= s2.close_line.
            assert(clear_path(line_b, hit_line, classifications));
            assert(hit_line > line_b);
            assert(line_b >= scopes@[s2_idx].open_line);
            assert(hit_line >= scopes@[s2_idx].open_line);

            if hit_line > scopes@[s2_idx].close_line {
                // s2.close_line is between line_b and hit_line
                let s2_close = scopes@[s2_idx].close_line;
                assert(line_b <= s2_close);
                assert(s2_close < hit_line);
                assert((s2_close as int) > (line_b as int));
                assert((s2_close as int) < (hit_line as int));
                // By clear_path: every line between line_b and hit_line has no ScopeClose
                // s2_close is between them, so it has no ScopeClose
                assert(0 <= s2_close as int - 1 < classifications@.len());
                assert(classifications@[s2_close as int - 1].is_some());
                assert(!classifications@[s2_close as int - 1].unwrap()@.contains(LineProperty::ScopeClose));
                // But scopes_match_classifications says it DOES have ScopeClose
                assert(classifications@[s2_close as int - 1].unwrap()@.contains(LineProperty::ScopeClose));
                // Contradiction — this branch is unreachable
            }
            assert(hit_line <= scopes@[s2_idx].close_line);
            assert(in_scope(hit_line, scopes, s2_idx));
        } else {
            // s2_idx contains scope_idx
            // hit_line is in scope_idx which is inside s2_idx
            // So hit_line is in s2_idx
            assert(scope_contains(scopes@, s2_idx, scope_idx));
            assert(scopes@[s2_idx].open_line <= scopes@[scope_idx].open_line);
            assert(scopes@[scope_idx].close_line <= scopes@[s2_idx].close_line);
            assert(hit_line >= scopes@[scope_idx].open_line);
            assert(hit_line <= scopes@[scope_idx].close_line);
            assert(hit_line >= scopes@[s2_idx].open_line);
            assert(hit_line <= scopes@[s2_idx].close_line);
            assert(in_scope(hit_line, scopes, s2_idx));
        }
    }
}

//= design/query/coverage-model-spec.md#property-3-conservative-fallback
//# The implementation MUST prove that no backward propagation occurs WITHIN a
//# scope that contains a `NonLinearControl` line.
/// Property 3: Conservative Fallback.
///
/// Lemma: if a line is in the execution set via propagation (not directly hit),
/// the propagation scope does NOT have NonLinearControl. This is the core
/// safety property — propagation is disabled in NLC scopes.
///
/// For nested scopes, a line in a NLC parent scope may also be in a non-NLC
/// child scope and get propagated via the child. This is sound because a goto
/// in the parent cannot redirect control flow within the child without first
/// exiting the child (crossing a ScopeClose).
proof fn lemma_conservative_fallback(
    exec_set: Set<u64>,
    classifications: &[Option<LineClass>],
    scopes: &[Scope],
    coverage: &CoverageReport,
    line: u64,
    scope_idx: int,
)
    requires
        forall|l: u64| exec_set.contains(l)
            ==> validly_in_exec_set(l, classifications, scopes, coverage),
        scope_has_non_linear_control(classifications, scopes, scope_idx),
        in_scope(line, scopes, scope_idx),
        exec_set.contains(line),
        !(coverage@.contains_key(line) && coverage@[line] == CoverageStatus::Hit),
    ensures
        // The propagation happened in a different scope that lacks NLC
        exists|hit_line: u64, path_scope_idx: int|
            has_valid_path(line, hit_line, classifications, scopes, path_scope_idx, coverage)
            && path_scope_idx != scope_idx
            && !scope_has_non_linear_control(classifications, scopes, path_scope_idx),
{
    assert(validly_in_exec_set(line, classifications, scopes, coverage));
    let (hit_line, path_scope_idx): (u64, int) = choose|hit_line: u64, path_scope_idx: int|
        has_valid_path(line, hit_line, classifications, scopes, path_scope_idx, coverage);
    assert(!scope_has_non_linear_control(classifications, scopes, path_scope_idx));
    assert(scope_has_non_linear_control(classifications, scopes, scope_idx));
    assert(path_scope_idx != scope_idx);
}

//= design/query/coverage-model-spec.md#property-4-monotonicity
//# The implementation MUST prove that given two coverage reports E1 and E2 where
//# E1 ⊆ E2 (E2 reports all the same hits as E1, plus possibly more):
/// Property 4: Monotonicity.
///
/// Lemma: if E1 ⊆ E2 (every hit in E1 is also a hit in E2), then
/// execution_set(E1) ⊆ execution_set(E2).
///
/// Proof: every line in execution_set(E1) satisfies validly_in_exec_set under E1.
/// If directly hit in E1, it's directly hit in E2 (since E1 ⊆ E2), so it's in
/// execution_set(E2) by Property 9. If it has a valid_path under E1, the same
/// path is valid under E2 (the hit_line is still hit, classifications and scopes
/// are unchanged), so it's in execution_set(E2) by the ensures clause.
proof fn lemma_monotonicity(
    exec_set_1: Set<u64>,
    exec_set_2: Set<u64>,
    classifications: &[Option<LineClass>],
    scopes: &[Scope],
    coverage_e1: &CoverageReport,
    coverage_e2: &CoverageReport,
    line: u64,
)
    requires
        // exec_set_1 is the result of execution_set under E1
        forall|l: u64| exec_set_1.contains(l)
            ==> validly_in_exec_set(l, classifications, scopes, coverage_e1),
        // exec_set_2 contains all directly-hit lines under E2
        forall|l: u64| coverage_e2@.contains_key(l) && coverage_e2@[l] == CoverageStatus::Hit
            ==> exec_set_2.contains(l),
        // exec_set_2 satisfies validly_in_exec_set under E2 (for the path case)
        forall|l: u64| exec_set_2.contains(l)
            ==> validly_in_exec_set(l, classifications, scopes, coverage_e2),
        // exec_set_2 is complete: every valid line is in the set
        forall|l: u64| validly_in_exec_set(l, classifications, scopes, coverage_e2)
            ==> exec_set_2.contains(l),
        // E1 ⊆ E2: every hit in E1 is a hit in E2
        forall|l: u64| coverage_e1@.contains_key(l) && coverage_e1@[l] == CoverageStatus::Hit
            ==> coverage_e2@.contains_key(l) && coverage_e2@[l] == CoverageStatus::Hit,
        // line is in exec_set_1
        exec_set_1.contains(line),
    ensures
        // Then line is in exec_set_2
        exec_set_2.contains(line),
{
    // line is in exec_set_1, so validly_in_exec_set(line, ..., E1) holds
    assert(validly_in_exec_set(line, classifications, scopes, coverage_e1));

    if coverage_e1@.contains_key(line) && coverage_e1@[line] == CoverageStatus::Hit {
        // Case 1: line is directly hit in E1
        // E1 ⊆ E2, so line is directly hit in E2
        assert(coverage_e2@.contains_key(line) && coverage_e2@[line] == CoverageStatus::Hit);
        // By exec_set_2's ensures (Property 9), line is in exec_set_2
        assert(exec_set_2.contains(line));
    } else {
        // Case 2: line has a valid path under E1
        let (hit_line, scope_idx): (u64, int) = choose|hit_line: u64, scope_idx: int|
            has_valid_path(line, hit_line, classifications, scopes, scope_idx, coverage_e1);

        // hit_line is directly hit in E1
        assert(coverage_e1@.contains_key(hit_line) && coverage_e1@[hit_line] == CoverageStatus::Hit);
        // E1 ⊆ E2, so hit_line is directly hit in E2
        assert(coverage_e2@.contains_key(hit_line) && coverage_e2@[hit_line] == CoverageStatus::Hit);

        // The path is the same under E2: same classifications, same scopes, same scope_idx
        // Only the coverage changed, and hit_line is still hit
        assert(propagated_within_scope(line, hit_line, scopes, scope_idx));
        assert(clear_path(line, hit_line, classifications));
        assert(has_valid_path(line, hit_line, classifications, scopes, scope_idx, coverage_e2));
        assert(validly_in_exec_set(line, classifications, scopes, coverage_e2));

        // By completeness ensures on exec_set_2: validly_in_exec_set ==> in the set
        assert(exec_set_2.contains(line));
    }
}

//= design/query/coverage-model-spec.md#property-5-stacking-transitivity
//# The implementation MUST prove that if annotation A (lines a1..a2) is
//# immediately above annotation B (lines b1..b2) with only whitespace,
//# comments, or other annotations between
//# them, and `is_annotation_executed(B, ...) = Executed`, then
//# `is_annotation_executed(A, ...) = Executed`.
/// Helper: skipping a prefix of skippable lines does not change the walk's
/// target. If every line in `[c, d]` is skippable, the forward walk from `c`
/// lands on the same target as the walk from `d + 1`.
proof fn lemma_skippable_prefix_same_walk(
    classifications: &[Option<LineClass>],
    c: u64,
    d: u64,
    file_length: u64,
)
    requires
        d < u64::MAX,
        c <= d + 1,
        all_lines_skippable(classifications, c, d),
    ensures
        annotation_target_walk(classifications, c, file_length)
            == annotation_target_walk(classifications, (d + 1) as u64, file_length),
    decreases d + 1 - c,
{
    if c <= d {
        // `c` is skippable, so the walk steps over it; recurse on `[c+1, d]`.
        assert(line_is_skippable(classifications, c));
        assert(annotation_target_walk(classifications, c, file_length)
            == annotation_target_walk(classifications, (c + 1) as u64, file_length));
        lemma_skippable_prefix_same_walk(classifications, (c + 1) as u64, d, file_length);
    }
}

/// Property 5: Annotation Stacking Transitivity.
///
/// If annotation A ends above annotation B with only skippable lines
/// (whitespace, comments, other annotations) between them, then A and B
/// resolve to the same target and therefore share an execution status —
/// in particular, if B is `Executed`, then so is A.
proof fn lemma_stacking_transitivity(
    ann_a: &AnnotationSpan,
    ann_b: &AnnotationSpan,
    classifications: &[Option<LineClass>],
    scopes: &[Scope],
    coverage: &CoverageReport,
    file_length: u64,
)
    requires
        ann_a.end_line < u64::MAX,
        ann_b.end_line < u64::MAX,
        ann_b.start_line <= ann_b.end_line,
        // A is immediately above B ...
        ann_a.end_line < ann_b.start_line,
        // ... with only skippable lines between A's end and B's end.
        all_lines_skippable(classifications, (ann_a.end_line + 1) as u64, ann_b.end_line),
        // B is Executed.
        execution_status_of(
            annotation_target_spec(ann_b, classifications, file_length),
            classifications, scopes, coverage,
        ) == ExecutionStatus::Executed,
    ensures
        // Then A is Executed too.
        execution_status_of(
            annotation_target_spec(ann_a, classifications, file_length),
            classifications, scopes, coverage,
        ) == ExecutionStatus::Executed,
{
    // A's walk skips through to B's end, then continues from B's end + 1 — the
    // same starting point as B's walk — so the two resolve to the same target.
    lemma_skippable_prefix_same_walk(
        classifications, (ann_a.end_line + 1) as u64, ann_b.end_line, file_length);
}

//= design/query/coverage-model-spec.md#property-6-unknown-safety
//# The implementation MUST prove that unknown lines cannot produce false
//# positives.
// Property 6 (Unknown Safety) is proven inline as a postcondition of
// `is_annotation_executed`: Executed ==> the resolved target line exists and
// is classified. No separate lemma is needed.

//= design/query/coverage-model-spec.md#property-7-target-determinism
//= type=implication
//# `annotation_target` is a pure function:
//# given the same annotation, classifications, and file length,
//# it always returns the same result.
//# This is free in Verus
//# (all `fn` in Verus are deterministic by construction).
// Property 7: Target Determinism — free by construction in Verus.
// All fn in Verus are deterministic (no interior mutability, no randomness).
// No proof fn needed.

// Property 9: Execution Set Containment — execution_set ⊇ directly_hit.
// Proven by the `ensures` clause on `execution_set` (see its citation there).

} // verus!

#[cfg(test)]
// These tests are concrete-input smoke tests for the correctness properties
// already proven by Verus in the surrounding module. They are not redundant
// with the proofs:
//
// - The Verus proofs establish that the property holds for *all* inputs
//   satisfying the `requires` clause.
// - These tests demonstrate the property on specific concrete inputs that a
//   reviewer can read top-to-bottom: the classifications, the scopes, the
//   coverage report, and the expected `ExecutionStatus`.
//
// They serve as runnable documentation of the algorithm's behavior, catch
// regressions in the non-Verus code paths (constant folding, panics on
// degenerate input, public API changes), and give a reviewer who is not yet
// fluent in Verus a way to engage with the model.
mod tests {
    use super::*;
    use crate::types::*;
    fn s(props: &[LineProperty]) -> Option<LineClass> {
        Some(line_class(props))
    }
    fn cov_hit(lines: &[u64]) -> CoverageReport {
        lines.iter().map(|&l| (l, CoverageStatus::Hit)).collect()
    }

    //= design/query/coverage-model-spec.md#correctness-properties
    //= type=test
    //# These properties MUST be proven with Verus.
    //= design/query/coverage-model-spec.md#property-2-no-cross-scope-leakage
    //= type=test
    //# The implementation MUST prove that for any two lines A and B where A is in
    //# scope S1 and B is in scope S2 and S1 ≠ S2 and S1 is not a parent of S2 and
    //# S2 is not a parent of S1:
    #[test]
    fn test_property_2_sibling_scopes() {
        let c = vec![
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),
            s(&[LineProperty::Statement]),
            s(&[LineProperty::ScopeClose]),
            s(&[LineProperty::Whitespace]),
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),
            s(&[LineProperty::Declaration]),
            s(&[LineProperty::ScopeClose]),
        ];
        // Property 2 is proven as a Verus proof fn (lemma_no_cross_scope_leakage).
        // This test verifies the runtime behavior.
        let exec_set = execution_set(
            &c,
            &[
                Scope {
                    open_line: 1,
                    close_line: 3,
                    parent: None,
                    children: vec![],
                },
                Scope {
                    open_line: 5,
                    close_line: 7,
                    parent: None,
                    children: vec![],
                },
            ],
            &cov_hit(&[2]),
        );
        assert!(!exec_set.contains(&5));
        assert!(!exec_set.contains(&6));
    }
    //= design/query/coverage-model-spec.md#property-3-conservative-fallback
    //= type=test
    //# The implementation MUST prove that no backward propagation occurs WITHIN a
    //# scope that contains a `NonLinearControl` line.
    #[test]
    fn test_property_3_goto_scope() {
        let c = vec![
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),
            s(&[LineProperty::Declaration]),
            s(&[LineProperty::NonLinearControl, LineProperty::Statement]),
            s(&[LineProperty::Statement]),
            s(&[LineProperty::ScopeClose]),
        ];
        // Property 3 is proven as a Verus proof fn (lemma_conservative_fallback).
        // This test verifies runtime behavior: in NonLinearControl scopes, only directly-hit lines.
        let r = execution_set(
            &c,
            &[Scope {
                open_line: 1,
                close_line: 5,
                parent: None,
                children: vec![],
            }],
            &cov_hit(&[3, 4]),
        );
        assert!(r.contains(&3));
        assert!(r.contains(&4));
        assert!(!r.contains(&1));
        assert!(!r.contains(&2));
    }
    //= design/query/coverage-model-spec.md#property-4-monotonicity
    //= type=test
    //# The implementation MUST prove that given two coverage reports E1 and E2 where
    //# E1 ⊆ E2 (E2 reports all the same hits as E1, plus possibly more):
    #[test]
    fn test_property_4_monotonicity() {
        let c = vec![
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),
            s(&[LineProperty::Declaration]),
            s(&[LineProperty::Statement]),
            s(&[LineProperty::Statement]),
            s(&[LineProperty::ScopeClose]),
        ];
        let sc = &[Scope {
            open_line: 1,
            close_line: 5,
            parent: None,
            children: vec![],
        }];
        // Property 4 is proven as a Verus proof fn (lemma_monotonicity).
        // This test verifies runtime behavior: E1 ⊆ E2 implies exec_set(E1) ⊆ exec_set(E2).
        let e1 = execution_set(&c, sc, &cov_hit(&[3]));
        let e2 = execution_set(&c, sc, &cov_hit(&[3, 4]));
        for line in e1.iter() {
            assert!(e2.contains(line));
        }
    }
    //= design/query/coverage-model-spec.md#property-5-stacking-transitivity
    //= type=test
    //# The implementation MUST prove that if annotation A (lines a1..a2) is
    //# immediately above annotation B (lines b1..b2) with only whitespace,
    //# comments, or other annotations between
    //# them, and `is_annotation_executed(B, ...) = Executed`, then
    //# `is_annotation_executed(A, ...) = Executed`.
    #[test]
    fn test_property_5_stacking() {
        use crate::annotation_execution::is_annotation_executed;
        let c = vec![
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::Statement]),
            s(&[LineProperty::ScopeClose]),
        ];
        let sc = &[Scope {
            open_line: 1,
            close_line: 7,
            parent: None,
            children: vec![],
        }];
        let cov = cov_hit(&[6]);
        // Property 5 (Stacking Transitivity) is proven over the spec twins in a
        // follow-up commit; this test verifies runtime behavior: stacked
        // annotations both return Executed.
        let status_a = is_annotation_executed(
            &AnnotationSpan {
                start_line: 2,
                end_line: 3,
            },
            &c,
            sc,
            &cov,
            7,
        );
        let status_b = is_annotation_executed(
            &AnnotationSpan {
                start_line: 4,
                end_line: 5,
            },
            &c,
            sc,
            &cov,
            7,
        );
        assert_eq!(status_a, ExecutionStatus::Executed);
        assert_eq!(status_b, ExecutionStatus::Executed);
    }
    //= design/query/coverage-model-spec.md#property-6-unknown-safety
    //= type=test
    //# The implementation MUST prove that unknown lines cannot produce false
    //# positives.
    #[test]
    fn test_property_6_unknown_safety() {
        use crate::{
            annotation_execution::is_annotation_executed, target_resolution::annotation_target,
        };
        // Property 6 is proven by the ensures clause on is_annotation_executed
        // (Executed ==> the resolved target line exists and is classified).
        // This test verifies runtime behavior: Executed implies classified target.
        let c = vec![
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),
            s(&[LineProperty::Statement]),
            s(&[LineProperty::ScopeClose]),
        ];
        let sc = &[Scope {
            open_line: 2,
            close_line: 4,
            parent: None,
            children: vec![],
        }];
        let cov = cov_hit(&[3]);
        let status = is_annotation_executed(
            &AnnotationSpan {
                start_line: 1,
                end_line: 1,
            },
            &c,
            sc,
            &cov,
            4,
        );
        assert_eq!(status, ExecutionStatus::Executed);
        let target = annotation_target(
            &AnnotationSpan {
                start_line: 1,
                end_line: 1,
            },
            &c,
            4,
        );
        assert!(target.is_some());
        assert!(target.unwrap().properties.is_some());
    }
}
