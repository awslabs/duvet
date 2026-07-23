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

//= design/query/coverage-model-spec.md#property-2-no-cross-scope-leakage
//= type=implication
//# The implementation MUST prove that for any two lines A and B where A is in
//# scope S1 and B is in scope S2 and S1 ≠ S2 and S1 is not a parent of S2 and
//# S2 is not a parent of S1:
/// Property 2, composed end-to-end over the *public* `is_annotation_executed`.
///
/// This is the P5 treatment applied to Property 2: it calls the real public
/// function and states no-cross-scope-leakage as an observable consequence of
/// the value a caller actually receives. `is_annotation_executed`'s third
/// `ensures` gives `status == Executed ==> validly_in_exec_set(target, ..)` — the
/// same Property-1 postcondition the lemma needs, now discharged all the way
/// through `execution_set` (and across its two `external_body` leaves, whose
/// membership specs are sufficient because that is all the reachability reads).
///
/// The public interface only ever exposes one execution-set line — the
/// annotation's resolved target — so the statement is about that line, not the
/// spec's full "any two lines A and B" set. The full-set form remains proven by
/// `lemma_no_cross_scope_leakage`; this harness is its observable projection.
/// The two well-formedness hypotheses are carried as harness `requires`, exactly
/// as P5 re-declares `is_annotation_executed`'s shared preconditions.
///
/// VACUITY NOTE: the antecedent (Executed, target in scope, not directly hit) is
/// satisfiable — the witness `executed_via_propagation_is_reachable` exhibits an
/// Executed target reached by propagation, so this `ensures` is not vacuous.
fn executed_annotation_has_no_cross_scope_leakage(
    annotation: &AnnotationSpan,
    classifications: &[Option<LineClass>],
    scopes: &[Scope],
    coverage: &CoverageReport,
    file_length: u64,
) -> (status: ExecutionStatus)
    requires
        annotation.end_line < u64::MAX,
        forall|line: u64| coverage@.contains_key(line)
            ==> (line as int - 1) >= 0 && (line as int - 1) < classifications@.len(),
        forall|i: int| 0 <= i < scopes@.len() ==> (#[trigger] scopes@[i]).close_line < u64::MAX,
        forall|i: int| 0 <= i < scopes@.len() ==> (#[trigger] scopes@[i]).open_line >= 1,
        // Well-formedness the lemma needs, beyond is_annotation_executed's ensures.
        scopes_well_formed(scopes@),
        scopes_match_classifications(scopes@, classifications),
    ensures
        status == ExecutionStatus::Executed ==> {
            let target = annotation_target_spec(annotation, classifications, file_length);
            &&& target.is_some()
            &&& forall|s2_idx: int|
                    in_scope(target.unwrap(), scopes, s2_idx)
                    && !(coverage@.contains_key(target.unwrap())
                         && coverage@[target.unwrap()] == CoverageStatus::Hit)
                    ==> exists|hit_line: u64|
                            coverage@.contains_key(hit_line)
                            && coverage@[hit_line] == CoverageStatus::Hit
                            && in_scope(hit_line, scopes, s2_idx)
        },
{
    let status = is_annotation_executed(annotation, classifications, scopes, coverage, file_length);
    proof {
        if status == ExecutionStatus::Executed {
            // is_annotation_executed's ensures #3: the target is validly in the set.
            let target_opt = annotation_target_spec(annotation, classifications, file_length);
            let target = target_opt.unwrap();
            assert(validly_in_exec_set(target, classifications, scopes, coverage));
            // Instantiate the lemma at the single observable line via a singleton
            // set whose only member is `target` — the forall postcondition holds
            // because `target` is validly in the set.
            let es = Set::<u64>::empty().insert(target);
            assert forall|s2_idx: int|
                in_scope(target, scopes, s2_idx)
                && !(coverage@.contains_key(target) && coverage@[target] == CoverageStatus::Hit)
            implies exists|hit_line: u64|
                    coverage@.contains_key(hit_line)
                    && coverage@[hit_line] == CoverageStatus::Hit
                    && in_scope(hit_line, scopes, s2_idx)
            by {
                assert forall|line: u64| es.contains(line)
                    implies validly_in_exec_set(line, classifications, scopes, coverage)
                by {
                    assert(line == target);
                }
                lemma_no_cross_scope_leakage(es, classifications, scopes, coverage, target, s2_idx);
            }
        }
    }
    status
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

//= design/query/coverage-model-spec.md#property-3-conservative-fallback
//= type=implication
//# The implementation MUST prove that no backward propagation occurs WITHIN a
//# scope that contains a `NonLinearControl` line.
/// Property 3, composed end-to-end over the *public* `is_annotation_executed`.
///
/// Stated over the value a caller receives: if an annotation is Executed and its
/// target sits in an NLC scope but was not directly hit, the propagation that
/// carried it happened in a *different*, non-NLC scope. Discharged from
/// `is_annotation_executed`'s Property-1 `ensures` (`Executed ==>
/// validly_in_exec_set(target)`) via `lemma_conservative_fallback`, instantiated
/// at the single observable target line.
///
/// VACUITY NOTE: the antecedent is satisfiable only in the *nested* case — a
/// target in an NLC parent scope reached via a non-NLC child. In a flat NLC scope
/// no line is both in-set and not-directly-hit, so the antecedent is empty. The
/// companion witness `executed_target_in_nlc_parent_is_reachable` exhibits a
/// concrete Executed annotation satisfying the antecedent, so this `ensures` is
/// not vacuously true.
fn executed_annotation_conservative_fallback(
    annotation: &AnnotationSpan,
    classifications: &[Option<LineClass>],
    scopes: &[Scope],
    coverage: &CoverageReport,
    file_length: u64,
) -> (status: ExecutionStatus)
    requires
        annotation.end_line < u64::MAX,
        forall|line: u64| coverage@.contains_key(line)
            ==> (line as int - 1) >= 0 && (line as int - 1) < classifications@.len(),
        forall|i: int| 0 <= i < scopes@.len() ==> (#[trigger] scopes@[i]).close_line < u64::MAX,
        forall|i: int| 0 <= i < scopes@.len() ==> (#[trigger] scopes@[i]).open_line >= 1,
    ensures
        status == ExecutionStatus::Executed ==> {
            let target = annotation_target_spec(annotation, classifications, file_length);
            &&& target.is_some()
            &&& forall|scope_idx: int|
                    scope_has_non_linear_control(classifications, scopes, scope_idx)
                    && in_scope(target.unwrap(), scopes, scope_idx)
                    && !(coverage@.contains_key(target.unwrap())
                         && coverage@[target.unwrap()] == CoverageStatus::Hit)
                    ==> exists|hit_line: u64, path_scope_idx: int|
                            has_valid_path(target.unwrap(), hit_line, classifications, scopes,
                                path_scope_idx, coverage)
                            && path_scope_idx != scope_idx
                            && !scope_has_non_linear_control(classifications, scopes, path_scope_idx)
        },
{
    let status = is_annotation_executed(annotation, classifications, scopes, coverage, file_length);
    proof {
        if status == ExecutionStatus::Executed {
            let target_opt = annotation_target_spec(annotation, classifications, file_length);
            let target = target_opt.unwrap();
            assert(validly_in_exec_set(target, classifications, scopes, coverage));
            let es = Set::<u64>::empty().insert(target);
            assert forall|scope_idx: int|
                scope_has_non_linear_control(classifications, scopes, scope_idx)
                && in_scope(target, scopes, scope_idx)
                && !(coverage@.contains_key(target) && coverage@[target] == CoverageStatus::Hit)
            implies exists|hit_line: u64, path_scope_idx: int|
                    has_valid_path(target, hit_line, classifications, scopes, path_scope_idx, coverage)
                    && path_scope_idx != scope_idx
                    && !scope_has_non_linear_control(classifications, scopes, path_scope_idx)
            by {
                assert forall|line: u64| es.contains(line)
                    implies validly_in_exec_set(line, classifications, scopes, coverage)
                by {
                    assert(line == target);
                }
                lemma_conservative_fallback(es, classifications, scopes, coverage, target, scope_idx);
            }
        }
    }
    status
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

/// Pointwise monotonicity of `validly_in_exec_set` under coverage growth: if a
/// line is validly in the execution set under E1 and E1 ⊆ E2 (every E1 hit is an
/// E2 hit), then it is validly in the set under E2. This is the coverage-facing
/// heart of Property 4 — `has_valid_path` depends on coverage only through the
/// membership of its hit line, so a hit that survives E1 → E2 keeps the whole
/// path valid; the direct-hit case is immediate from E1 ⊆ E2.
proof fn lemma_validly_in_exec_set_monotone(
    line: u64,
    classifications: &[Option<LineClass>],
    scopes: &[Scope],
    coverage_e1: &CoverageReport,
    coverage_e2: &CoverageReport,
)
    requires
        validly_in_exec_set(line, classifications, scopes, coverage_e1),
        forall|l: u64| coverage_e1@.contains_key(l) && coverage_e1@[l] == CoverageStatus::Hit
            ==> coverage_e2@.contains_key(l) && coverage_e2@[l] == CoverageStatus::Hit,
    ensures
        validly_in_exec_set(line, classifications, scopes, coverage_e2),
{
    if coverage_e1@.contains_key(line) && coverage_e1@[line] == CoverageStatus::Hit {
        assert(coverage_e2@.contains_key(line) && coverage_e2@[line] == CoverageStatus::Hit);
    } else {
        let (hit_line, scope_idx): (u64, int) = choose|hit_line: u64, scope_idx: int|
            has_valid_path(line, hit_line, classifications, scopes, scope_idx, coverage_e1);
        assert(coverage_e1@.contains_key(hit_line) && coverage_e1@[hit_line] == CoverageStatus::Hit);
        assert(coverage_e2@.contains_key(hit_line) && coverage_e2@[hit_line] == CoverageStatus::Hit);
        assert(has_valid_path(line, hit_line, classifications, scopes, scope_idx, coverage_e2));
    }
}

//= design/query/coverage-model-spec.md#property-4-monotonicity
//= type=implication
//# The implementation MUST prove that given two coverage reports E1 and E2 where
//# E1 ⊆ E2 (E2 reports all the same hits as E1, plus possibly more):
/// Property 4, composed end-to-end over the *public* `is_annotation_executed`.
///
/// The observable form of monotonicity: running the same annotation against a
/// larger coverage report never revokes an Executed verdict. Calls the public fn
/// twice (E1, then E2 ⊇ E1) and relates the two concrete `ExecutionStatus`
/// values it returns, exactly as the P5 harness relates two statuses.
///
/// Chain: call 1's Property-1 `ensures` gives `validly_in_exec_set(target, E1)`;
/// `lemma_validly_in_exec_set_monotone` carries it to E2; call 2's equivalence
/// `ensures` pins `status_2 == execution_status_of(target, .., E2)`, and the
/// target/classification/NLC branch conditions are coverage-independent (equal to
/// call 1's), so the `Executed` branch is reached under E2 too. Returns the pair
/// so the `ensures` can relate the two real return values.
///
/// VACUITY NOTE: the witness `executed_survives_added_coverage` shows the
/// antecedent (Executed under E1) is reachable with E1 ⊊ E2, so the implication
/// is exercised on a real coverage-growth step, not just the reflexive case.
fn executed_annotation_monotonic(
    annotation: &AnnotationSpan,
    classifications: &[Option<LineClass>],
    scopes: &[Scope],
    coverage_e1: &CoverageReport,
    coverage_e2: &CoverageReport,
    file_length: u64,
) -> (result: (ExecutionStatus, ExecutionStatus))
    requires
        annotation.end_line < u64::MAX,
        forall|line: u64| coverage_e1@.contains_key(line)
            ==> (line as int - 1) >= 0 && (line as int - 1) < classifications@.len(),
        forall|line: u64| coverage_e2@.contains_key(line)
            ==> (line as int - 1) >= 0 && (line as int - 1) < classifications@.len(),
        forall|i: int| 0 <= i < scopes@.len() ==> (#[trigger] scopes@[i]).close_line < u64::MAX,
        forall|i: int| 0 <= i < scopes@.len() ==> (#[trigger] scopes@[i]).open_line >= 1,
        // E1 ⊆ E2: every hit in E1 is a hit in E2.
        forall|l: u64| coverage_e1@.contains_key(l) && coverage_e1@[l] == CoverageStatus::Hit
            ==> coverage_e2@.contains_key(l) && coverage_e2@[l] == CoverageStatus::Hit,
    ensures
        // Executed under E1 ==> Executed under E2.
        result.0 == ExecutionStatus::Executed ==> result.1 == ExecutionStatus::Executed,
{
    let status_1 = is_annotation_executed(annotation, classifications, scopes, coverage_e1, file_length);
    let status_2 = is_annotation_executed(annotation, classifications, scopes, coverage_e2, file_length);
    proof {
        if status_1 == ExecutionStatus::Executed {
            let target_opt = annotation_target_spec(annotation, classifications, file_length);
            let target = target_opt.unwrap();
            // From call 1's ensures #3.
            assert(validly_in_exec_set(target, classifications, scopes, coverage_e1));
            // Carry validity across the coverage growth.
            lemma_validly_in_exec_set_monotone(
                target, classifications, scopes, coverage_e1, coverage_e2);
            assert(validly_in_exec_set(target, classifications, scopes, coverage_e2));
            // status_2 equals the spec twin over the same target (call 2, ensures #1).
            // The pre-validity branch conditions (Some target, classified, not NLC)
            // are coverage-independent and hold from status_1 == Executed; with
            // validity now holding under E2, the twin lands on Executed.
            assert(status_2 == execution_status_of(
                annotation_target_spec(annotation, classifications, file_length),
                classifications, scopes, coverage_e2));
        }
    }
    (status_1, status_2)
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

/// Property 5, composed end-to-end over the *public* `is_annotation_executed`.
///
/// `lemma_stacking_transitivity` (above) proves the property over the spec twin
/// `execution_status_of`. That twin is only meaningful because
/// `is_annotation_executed` is proven equal to it (its first `ensures`). This
/// harness closes that gap explicitly: it *calls the real public function* on
/// both annotations and proves the transitivity over the two concrete
/// `ExecutionStatus` values it actually returns — so the guarantee holds for
/// the API a caller invokes, not just for a ghost function a caller never sees.
///
/// The chain is: A's/B's `ensures` pin each concrete status to
/// `execution_status_of(annotation_target_spec(..), ..)`; the lemma discharges
/// the transitivity over those twins; equality carries it back to the concrete
/// statuses. Verified by construction — if the equivalence `ensures` on
/// `is_annotation_executed` ever weakened, this harness would stop verifying.
///
/// Returns `(status_a, status_b)` so the `ensures` can relate the two real
/// return values (Verus cannot reference an exec fn's call inside `ensures`).
fn stacked_annotations_share_executed(
    ann_a: &AnnotationSpan,
    ann_b: &AnnotationSpan,
    classifications: &[Option<LineClass>],
    scopes: &[Scope],
    coverage: &CoverageReport,
    file_length: u64,
) -> (result: (ExecutionStatus, ExecutionStatus))
    requires
        // Shared preconditions of `is_annotation_executed` (for both A and B).
        ann_a.end_line < u64::MAX,
        ann_b.end_line < u64::MAX,
        forall|line: u64| coverage@.contains_key(line)
            ==> (line as int - 1) >= 0 && (line as int - 1) < classifications@.len(),
        forall|i: int| 0 <= i < scopes@.len() ==> (#[trigger] scopes@[i]).close_line < u64::MAX,
        forall|i: int| 0 <= i < scopes@.len() ==> (#[trigger] scopes@[i]).open_line >= 1,
        // Stacking hypothesis: A ends immediately above B with only skippable
        // lines (whitespace / comments / other annotations) between them.
        ann_b.start_line <= ann_b.end_line,
        ann_a.end_line < ann_b.start_line,
        all_lines_skippable(classifications, (ann_a.end_line + 1) as u64, ann_b.end_line),
    ensures
        // End-to-end Property 5 over the concrete public results: if the lower
        // annotation B is Executed, so is the upper annotation A.
        result.1 == ExecutionStatus::Executed ==> result.0 == ExecutionStatus::Executed,
{
    let status_a = is_annotation_executed(ann_a, classifications, scopes, coverage, file_length);
    let status_b = is_annotation_executed(ann_b, classifications, scopes, coverage, file_length);
    proof {
        // Each call's equivalence `ensures` is now in scope:
        //   status_a == execution_status_of(annotation_target_spec(A,..),..)
        //   status_b == execution_status_of(annotation_target_spec(B,..),..)
        // When B is Executed, its twin is Executed; the lemma then forces A's
        // twin to Executed, and equality carries that back to status_a.
        if status_b == ExecutionStatus::Executed {
            lemma_stacking_transitivity(
                ann_a, ann_b, classifications, scopes, coverage, file_length);
        }
    }
    (status_a, status_b)
}

//= design/query/coverage-model-spec.md#property-6-unknown-safety
//# The implementation MUST prove that unknown lines cannot produce false
//# positives.
// Property 6 (Unknown Safety) is proven inline as two postconditions of
// `is_annotation_executed`, matching the spec's two bullets: (a) Executed ==>
// the resolved target line exists and is classified, and (b) Executed ==> the
// target is validly in the execution set, which forbids `None` on the
// propagation path between the hit line and the target. No separate lemma is
// needed.

//= design/query/coverage-model-spec.md#property-7-target-determinism
//= type=implication
//# `annotation_target` is a pure function:
//# given the same annotation, classifications, and file length,
//# it always returns the same result.
//# This is free in Verus
//# (all `fn` in Verus are deterministic by construction).
// Property 7: Target Determinism. Backed concretely by annotation_target's
// proven equivalence to the pure spec fn `annotation_target_spec`
// (target_resolution.rs): the exec result equals a deterministic function of
// its inputs. (Determinism is also free in Verus — exec fns have no interior
// mutability or randomness.) No separate proof fn needed.

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

    /// Non-vacuity witness for `executed_annotation_has_no_cross_scope_leakage`.
    ///
    /// The P2 harness's `ensures` says something only when the target is in a
    /// scope AND was not directly hit — i.e. it reached the execution set by
    /// propagation. This exhibits exactly that: an annotation whose target (line
    /// 3, a Declaration) is `Executed` via backward propagation from the hit at
    /// line 4, sits inside scope [1,5], and is itself not a coverage hit. Without
    /// it the `!directly_hit` antecedent could be vacuously empty.
    #[test]
    fn executed_via_propagation_is_reachable() {
        use crate::annotation_execution::is_annotation_executed;
        let c = vec![
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]), // 1 scope opens
            s(&[LineProperty::Annotation]),                           // 2 annotation
            s(&[LineProperty::Declaration]),                          // 3 TARGET (propagated)
            s(&[LineProperty::Statement]),                            // 4 HIT
            s(&[LineProperty::ScopeClose]),                           // 5 scope closes
        ];
        let scopes = &[Scope {
            open_line: 1,
            close_line: 5,
            parent: None,
            children: vec![],
        }];
        let cov = cov_hit(&[4]);
        let status = is_annotation_executed(
            &AnnotationSpan {
                start_line: 2,
                end_line: 2,
            },
            &c,
            scopes,
            &cov,
            5,
        );
        // Antecedent is satisfiable: Executed ...
        assert_eq!(status, ExecutionStatus::Executed);
        // ... on a target (line 3) inside scope [1,5] ...
        assert!(3 >= scopes[0].open_line && 3 <= scopes[0].close_line);
        // ... that is NOT directly hit (only line 4 is) ...
        assert!(!cov.contains_key(&3));
        // ... and reached the execution set by propagation.
        let exec_set = execution_set(&c, scopes, &cov);
        assert!(exec_set.contains(&3));
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

    /// Non-vacuity witness for `executed_annotation_conservative_fallback`.
    ///
    /// The public P3 harness's `ensures` antecedent — Executed, target in an NLC
    /// scope, not directly hit — is only satisfiable in the nested case. This test
    /// exhibits it: an annotation whose target (line 5, a Declaration) sits inside
    /// an NLC *parent* scope (1–9, NLC at line 2) yet is `Executed` via propagation
    /// through a non-NLC *child* scope (3–7) from the hit at line 6. Without a
    /// witness like this, the harness's `ensures` would be vacuously true.
    #[test]
    fn executed_target_in_nlc_parent_is_reachable() {
        use crate::annotation_execution::is_annotation_executed;
        let c = vec![
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]), // 1 parent opens
            s(&[LineProperty::NonLinearControl]),                     // 2 parent is NLC
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]), // 3 child opens
            s(&[LineProperty::Annotation]),                           // 4 annotation
            s(&[LineProperty::Declaration]), // 5 TARGET (in child + parent)
            s(&[LineProperty::Statement]),   // 6 HIT
            s(&[LineProperty::ScopeClose]),  // 7 child closes
            s(&[LineProperty::Statement]),   // 8
            s(&[LineProperty::ScopeClose]),  // 9 parent closes
        ];
        let scopes = &[
            Scope {
                open_line: 1,
                close_line: 9,
                parent: None,
                children: vec![],
            }, // NLC parent
            Scope {
                open_line: 3,
                close_line: 7,
                parent: None,
                children: vec![],
            }, // non-NLC child
        ];
        let cov = cov_hit(&[6]);

        // The annotation at line 4 resolves forward to line 5 (the target).
        let status = is_annotation_executed(
            &AnnotationSpan {
                start_line: 4,
                end_line: 4,
            },
            &c,
            scopes,
            &cov,
            9,
        );
        // Antecedent is satisfiable: Executed verdict ...
        assert_eq!(status, ExecutionStatus::Executed);
        // ... on a target (line 5) that is inside the NLC parent scope [1,9] ...
        assert!(5 >= scopes[0].open_line && 5 <= scopes[0].close_line);
        // ... and is NOT directly hit (only line 6 is).
        assert!(!cov.contains_key(&5));
        // The propagation that carried it ran through the non-NLC child [3,7].
        let exec_set = execution_set(&c, scopes, &cov);
        assert!(exec_set.contains(&5));
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

    /// Non-vacuity witness for `executed_annotation_monotonic`.
    ///
    /// The P4 harness's `ensures` (`Executed under E1 ==> Executed under E2`) is
    /// only meaningful when E1 is Executed AND E2 strictly grows the coverage;
    /// otherwise it is tested only at the degenerate E1 == E2 point. This runs the
    /// same annotation under E1 = {hit 4} and E2 = {hit 4, hit 5} (E1 ⊊ E2) and
    /// shows the target stays `Executed` across the growth.
    #[test]
    fn executed_survives_added_coverage() {
        use crate::annotation_execution::is_annotation_executed;
        let c = vec![
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]), // 1 scope opens
            s(&[LineProperty::Annotation]),                           // 2 annotation
            s(&[LineProperty::Declaration]),                          // 3 TARGET
            s(&[LineProperty::Statement]),                            // 4 hit in E1 and E2
            s(&[LineProperty::Statement]),                            // 5 hit only in E2
            s(&[LineProperty::ScopeClose]),                           // 6 scope closes
        ];
        let scopes = &[Scope {
            open_line: 1,
            close_line: 6,
            parent: None,
            children: vec![],
        }];
        let ann = AnnotationSpan {
            start_line: 2,
            end_line: 2,
        };
        let e1 = cov_hit(&[4]);
        let e2 = cov_hit(&[4, 5]);
        // E1 ⊊ E2.
        assert!(!e1.contains_key(&5) && e2.contains_key(&5));
        let status_e1 = is_annotation_executed(&ann, &c, scopes, &e1, 6);
        let status_e2 = is_annotation_executed(&ann, &c, scopes, &e2, 6);
        // Antecedent satisfiable (Executed under E1) and preserved under the larger E2.
        assert_eq!(status_e1, ExecutionStatus::Executed);
        assert_eq!(status_e2, ExecutionStatus::Executed);
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
        // Property 5 (Stacking Transitivity) is proven over the spec twins by
        // `lemma_stacking_transitivity` above; this test verifies runtime
        // behavior: stacked annotations both return Executed.
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
