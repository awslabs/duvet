// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Phase 2: Execution Propagation (spec Section 3).
//!
//! This module contains the `execution_set` algorithm and its proof engineering
//! (loop invariants, case analysis). The spec predicates it references
//! (`in_scope`, `clear_path`, `has_valid_path`, etc.) are defined in
//! [`crate::predicates`] for reviewer accessibility.

#[cfg(verus_keep_ghost)]
pub use crate::predicates::{
    clear_path, has_valid_path, in_scope, propagated_within_scope, scope_has_non_linear_control,
    validly_in_exec_set,
};
use crate::types::*;
use std::collections::BTreeSet;
use vstd::prelude::*;

verus! {

// TRUST BASE (unverified leaf). Verus cannot reason over `BTreeSet::iter`, so
// this body is trusted and only its `ensures` is checked downstream. The spec is
// *membership-only* (both directions) and deliberately says nothing about the
// result's length or element uniqueness. That is sufficient today: the sole
// caller (the `di` loop in `execution_set`) reads only membership. The omission
// is called out here because a partial spec on an `external_body` fn is exactly
// where a future refactor can silently weaken guarantees -- if a caller ever
// depends on length/uniqueness, strengthen this spec rather than assuming it.
#[verifier::external_body]
fn vec_from_btreeset(s: &BTreeSet<u64>) -> (result: Vec<u64>)
    ensures
        forall|line: u64| s@.contains(line) ==> result@.contains(line),
        forall|i: int| 0 <= i < result@.len() ==> s@.contains(result@[i]),
{
    s.iter().copied().collect()
}

// TRUST BASE (unverified leaf). This function *semantically defines* "directly
// executed" for the entire propagation: its `ensures` (the two-way iff with
// `CoverageStatus::Hit`) is the axiom that all of `execution_set`'s reachability
// flows from. Verus can't see through `BTreeMap::iter`, so the body is trusted --
// but that means a change to the body (e.g. filtering on `Miss`, or an off-by-one)
// would keep every proof green while silently changing what the model treats as
// executed. This is the one `external_body` whose correctness actually drives the
// result; keep body and `ensures` in exact correspondence.
#[verifier::external_body]
fn collect_hit_lines(coverage: &CoverageReport) -> (result: BTreeSet<u64>)
    ensures
        forall|line: u64| coverage@.contains_key(line) && coverage@[line] == CoverageStatus::Hit
            ==> result@.contains(line),
        forall|line: u64| result@.contains(line)
            ==> coverage@.contains_key(line) && coverage@[line] == CoverageStatus::Hit,
{
    let mut s = BTreeSet::new();
    for (line, status) in coverage.iter() {
        if matches!(*status, CoverageStatus::Hit) { s.insert(*line); }
    }
    s
}

pub(crate) fn execution_set(
    classifications: &[Option<LineClass>],
    scopes: &[Scope],
    coverage: &CoverageReport,
) -> (result: BTreeSet<u64>)
    requires
        forall|line: u64| coverage@.contains_key(line) ==> (line as int - 1) >= 0 && (line as int - 1) < classifications@.len(),
        forall|i: int| 0 <= i < scopes@.len() ==> (#[trigger] scopes@[i]).close_line < u64::MAX,
        forall|i: int| 0 <= i < scopes@.len() ==> (#[trigger] scopes@[i]).open_line >= 1,
    ensures
        //= design/query/coverage-model-spec.md#property-9-execution-set-containment
        //= type=implication
        // Property 9: every directly-hit line is in the result
        forall|line: u64| coverage@.contains_key(line) && coverage@[line] == CoverageStatus::Hit
            ==> result@.contains(line),
        //= design/query/coverage-model-spec.md#property-1-no-false-positives
        //= type=implication
        // Property 1 (No False Positives): every line in the result is validly there
        forall|line: u64| result@.contains(line)
            ==> validly_in_exec_set(line, classifications, scopes, coverage),
        // Property 4 (Completeness): every line with a valid path is in the result
        forall|line: u64| validly_in_exec_set(line, classifications, scopes, coverage)
            ==> result@.contains(line),
{
    let directly_executed = collect_hit_lines(coverage);

    let mut result: BTreeSet<u64> = directly_executed.clone();

    let mut si: usize = 0;
    while si < scopes.len()
        invariant
            si <= scopes@.len(),
            forall|line: u64| directly_executed@.contains(line) ==> result@.contains(line),
            forall|line: u64| result@.contains(line)
                ==> validly_in_exec_set(line, classifications, scopes, coverage),
            forall|line: u64| directly_executed@.contains(line)
                ==> coverage@.contains_key(line) && coverage@[line] == CoverageStatus::Hit,
            forall|line: u64| coverage@.contains_key(line) && coverage@[line] == CoverageStatus::Hit
                ==> directly_executed@.contains(line),
            forall|line: u64| coverage@.contains_key(line)
                ==> (line as int - 1) >= 0 && (line as int - 1) < classifications@.len(),
            forall|i: int| 0 <= i < scopes@.len() ==> (#[trigger] scopes@[i]).close_line < u64::MAX,
            forall|i: int| 0 <= i < scopes@.len() ==> (#[trigger] scopes@[i]).open_line >= 1,
            // Completeness: for all already-processed scopes
            forall|scope_idx: int, line: u64, hit_line: u64|
                0 <= scope_idx < si
                && #[trigger] has_valid_path(line, hit_line, classifications, scopes, scope_idx, coverage)
                ==> result@.contains(line),
        decreases scopes.len() - si,
    {
        let scope = &scopes[si];
        let mut has_non_linear = false;
        let ghost result_before_scope = result@;

        // Check all lines in scope for NonLinearControl.
        // Loop uses check_line from open_line to close_line+1 (exclusive upper bound).
        // Invariant tracks: all classified lines in [open_line, check_line) lack NonLinearControl.
        if scope.open_line >= 1 {
            let mut check_line = scope.open_line;
            assert(scopes@[si as int].close_line < u64::MAX) by {
                assert(0 <= si as int && (si as int) < scopes@.len());
            };
            let end = scope.close_line + 1;
            while check_line < end
                invariant
                    check_line >= scope.open_line,
                    end == (scope.close_line + 1) as u64,
                    end > scope.close_line,
                    scope.open_line >= 1,
                    has_non_linear ==> check_line >= end,
                    has_non_linear ==> scope_has_non_linear_control(classifications, scopes, si as int),
                    forall|line: u64| directly_executed@.contains(line) ==> result@.contains(line),
                    forall|line: u64| result@.contains(line)
                        ==> validly_in_exec_set(line, classifications, scopes, coverage),
                    forall|line: u64| directly_executed@.contains(line)
                        ==> coverage@.contains_key(line) && coverage@[line] == CoverageStatus::Hit,
                    forall|line: u64| coverage@.contains_key(line)
                        ==> (line as int - 1) >= 0 && (line as int - 1) < classifications@.len(),
                    si < scopes@.len(),
                    scope.open_line == scopes@[si as int].open_line,
                    scope.close_line == scopes@[si as int].close_line,
                    !has_non_linear ==> forall|l: u64|
                        scope.open_line <= l < check_line
                        && (l as int - 1) >= 0
                        && (l as int - 1) < classifications@.len()
                        && (#[trigger] classifications@[l as int - 1]).is_some()
                        ==> !classifications@[l as int - 1].unwrap()@.contains(LineProperty::NonLinearControl),
                decreases end - check_line,
            {
                let idx: usize = ((check_line - 1) as usize);
                if idx < classifications.len() {
                    if let Some(props) = &classifications[idx] {
                        if props.contains(&LineProperty::NonLinearControl) {
                            proof {
                                broadcast use crate::types::lemma_line_property_obeys_cmp_spec;
                                // Lossless u64->usize cast, now *proven* from
                                // `global size_of usize == 8` (lib.rs) rather than
                                // assumed.
                                assert(idx as int == check_line as int - 1);
                                // Now Verus can connect exec-level classifications[idx]
                                // to spec-level classifications@[check_line as int - 1]
                                // and derive scope_has_non_linear_control.
                            }
                            has_non_linear = true;
                            check_line = end; // jump to end so loop exits
                        }
                        proof { broadcast use crate::types::lemma_line_property_obeys_cmp_spec; }
                    }
                }
                if !has_non_linear {
                    check_line = check_line + 1;
                }
            }

            if !has_non_linear {
                // check_line >= end (loop exited normally), end > close_line
                // invariant covers [open_line, check_line) ⊇ [open_line, close_line]
                proof {
                    assert(!scope_has_non_linear_control(classifications, scopes, si as int));
                }
                let de_vec: Vec<u64> = vec_from_btreeset(&directly_executed);
                let mut di: usize = 0;
                while di < de_vec.len()
                    invariant
                        forall|line: u64| directly_executed@.contains(line) ==> result@.contains(line),
                        forall|line: u64| result@.contains(line)
                            ==> validly_in_exec_set(line, classifications, scopes, coverage),
                        forall|i: int| 0 <= i < de_vec@.len() ==> directly_executed@.contains(de_vec@[i]),
                        forall|line: u64| directly_executed@.contains(line)
                            ==> coverage@.contains_key(line) && coverage@[line] == CoverageStatus::Hit,
                        forall|line: u64| coverage@.contains_key(line) && coverage@[line] == CoverageStatus::Hit
                            ==> directly_executed@.contains(line),
                        forall|line: u64| coverage@.contains_key(line)
                            ==> (line as int - 1) >= 0 && (line as int - 1) < classifications@.len(),
                        si < scopes@.len(),
                        scope.open_line == scopes@[si as int].open_line,
                        scope.close_line == scopes@[si as int].close_line,
                        !scope_has_non_linear_control(classifications, scopes, si as int),
                        forall|i: int| 0 <= i < scopes@.len() ==> (#[trigger] scopes@[i]).close_line < u64::MAX,
                        // Subset: result only grows relative to before this scope
                        forall|line: u64| result_before_scope.contains(line) ==> result@.contains(line),
                        // Completeness: for all already-processed exec_lines, all valid-path lines are in result
                        forall|line: u64| directly_executed@.contains(line) ==> de_vec@.contains(line),
                        forall|j: int, line: u64| 0 <= j < di
                            && #[trigger] has_valid_path(line, de_vec@[j], classifications, scopes, si as int, coverage)
                            ==> result@.contains(line),
                    decreases de_vec.len() - di,
                {
                    let exec_line = de_vec[di];
                    let ghost mut final_current: u64 = 0;
                    let ghost mut current_in_result: bool = false;
                    let ghost mut stopped_at_obstacle: bool = false;
                    let ghost result_before_walk = result@;
                    if exec_line >= scope.open_line && exec_line <= scope.close_line && exec_line >= 1 {
                        let mut current = exec_line.wrapping_sub(1);
                        let mut done = false;
                        proof { final_current = current; }
                        while current >= scope.open_line && current >= 1 && !done
                            invariant
                                forall|line: u64| directly_executed@.contains(line) ==> result@.contains(line),
                                forall|line: u64| result@.contains(line)
                                    ==> validly_in_exec_set(line, classifications, scopes, coverage),
                                forall|line: u64| directly_executed@.contains(line)
                                    ==> coverage@.contains_key(line) && coverage@[line] == CoverageStatus::Hit,
                                forall|line: u64| coverage@.contains_key(line) && coverage@[line] == CoverageStatus::Hit
                                    ==> directly_executed@.contains(line),
                                forall|line: u64| coverage@.contains_key(line)
                                    ==> (line as int - 1) >= 0 && (line as int - 1) < classifications@.len(),
                                // Subset: result only grows
                                forall|line: u64| result_before_walk.contains(line) ==> result@.contains(line),
                                current < exec_line,
                                exec_line >= scope.open_line,
                                exec_line <= scope.close_line,
                                directly_executed@.contains(exec_line),
                                si < scopes@.len(),
                                scope.open_line == scopes@[si as int].open_line,
                                scope.close_line == scopes@[si as int].close_line,
                                !scope_has_non_linear_control(classifications, scopes, si as int),
                                forall|l: int| (current as int) < l < (exec_line as int) ==> {
                                    &&& 0 <= l - 1 < classifications@.len()
                                    &&& #[trigger] classifications@[l - 1].is_some()
                                    &&& !classifications@[l - 1].unwrap()@.contains(LineProperty::ScopeClose)
                                    &&& !classifications@[l - 1].unwrap()@.contains(LineProperty::Statement)
                                    &&& !classifications@[l - 1].unwrap()@.contains(LineProperty::ScopeOpen)
                                },
                                // Completeness: every line between current and exec_line is in result
                                forall|l: u64| (current as int) < (l as int) && (l as int) < (exec_line as int)
                                    ==> #[trigger] result@.contains(l),
                                // Completeness for has_valid_path
                                forall|line: u64|
                                    #[trigger] has_valid_path(line, exec_line, classifications, scopes, si as int, coverage)
                                    && (line as int) > (current as int)
                                    ==> result@.contains(line),
                                current_in_result ==> result@.contains(current),
                                // If walk stopped without inserting, current has an obstacle
                                stopped_at_obstacle ==> (
                                    (current as int - 1) >= classifications@.len()
                                    || classifications@[current as int - 1] is None
                                    || classifications@[current as int - 1].unwrap()@.contains(LineProperty::ScopeClose)
                                    || classifications@[current as int - 1].unwrap()@.contains(LineProperty::Statement)
                                ),
                                // done tracks that a break-equivalent fired
                                done ==> (stopped_at_obstacle || current_in_result),
                                // When current was inserted and done was set (not via obstacle),
                                // it was because of ScopeOpen or current <= 1
                                (current_in_result && done && !stopped_at_obstacle) ==> (
                                    ((current as int - 1) >= 0
                                     && (current as int - 1) < classifications@.len()
                                     && classifications@[current as int - 1].is_some()
                                     && classifications@[current as int - 1].unwrap()@.contains(LineProperty::ScopeOpen))
                                    || current <= 1
                                ),
                            decreases current + if done { 0 as u64 } else { 1 as u64 },
                        {
                            let idx: usize = ((current - 1) as usize);
                            if idx >= classifications.len() {
                                proof { stopped_at_obstacle = true; current_in_result = false; }
                                done = true;
                            } else {
                            match &classifications[idx] {
                                None => {
                                    proof { stopped_at_obstacle = true; current_in_result = false; }
                                    done = true;
                                }
                                Some(props) => {
                                    proof { broadcast use crate::types::lemma_line_property_obeys_cmp_spec; }

                                    if props.contains(&LineProperty::ScopeClose) {
                                        proof { stopped_at_obstacle = true; current_in_result = false; }
                                        done = true;
                                    } else if props.contains(&LineProperty::Statement) {
                                        proof { stopped_at_obstacle = true; current_in_result = false; }
                                        done = true;
                                    } else {

                                    // At this point we know:
                                    // - current is classified (Some)
                                    // - current is not ScopeClose, not Statement
                                    // - every line between current and exec_line is clear
                                    // - exec_line is directly hit and in scope
                                    // So current has a valid propagation path to exec_line.
                                    assert(directly_executed@.contains(exec_line));
                                    assert(coverage@.contains_key(exec_line)
                                        && coverage@[exec_line] == CoverageStatus::Hit);

                                    // Establish all preconditions of has_valid_path
                                    assert(0 <= si as int && (si as int) < scopes@.len());
                                    assert(current >= scopes@[si as int].open_line);
                                    assert(current <= scopes@[si as int].close_line);
                                    assert(exec_line >= scopes@[si as int].open_line);
                                    assert(exec_line <= scopes@[si as int].close_line);
                                    assert(exec_line > current);
                                    assert((current as int - 1) >= 0);
                                    assert((exec_line as int - 1) < classifications@.len());
                                    assert(!scope_has_non_linear_control(classifications, scopes, si as int));

                                    assert(has_valid_path(
                                        current, exec_line,
                                        classifications, scopes, si as int, coverage,
                                    ));
                                    assert(validly_in_exec_set(
                                        current, classifications, scopes, coverage,
                                    ));

                                    result.insert(current);
                                    proof { current_in_result = true; stopped_at_obstacle = false; }

                                    if props.contains(&LineProperty::ScopeOpen) {
                                        done = true;
                                    } else if current <= 1 {
                                        done = true;
                                    } else {
                                        proof { current_in_result = false; stopped_at_obstacle = false; }
                                        current = current - 1;
                                    }

                                    }
                                }
                            }
                            }
                        }
                        proof { final_current = current; }
                    }
                    proof {
                        // After the loop, Verus gives us the negation of the condition:
                        // !(current >= scope.open_line && current >= 1 && !done)
                        // i.e.: current < scope.open_line || current < 1 || done
                        //
                        // If done: stopped_at_obstacle || current_in_result (from invariant)
                        // If !done: current < scope.open_line || current < 1
                        assert forall|line: u64|
                            #[trigger] has_valid_path(line, exec_line, classifications, scopes, si as int, coverage)
                        implies result@.contains(line)
                        by {
                            // has_valid_path requires in_scope(exec_line, scopes, si)
                            // which requires exec_line >= scope.open_line && exec_line <= scope.close_line
                            // If exec_line is not in scope, has_valid_path is false (vacuously true)
                            if !(exec_line >= scope.open_line && exec_line <= scope.close_line && exec_line >= 1) {
                                // exec_line not in scope: has_valid_path requires
                                // propagated_within_scope which requires in_scope(exec_line, ...)
                                assert(has_valid_path(line, exec_line, classifications, scopes, si as int, coverage));
                                assert(in_scope(exec_line, scopes, si as int));
                                assert(exec_line >= scopes@[si as int].open_line);
                                assert(exec_line <= scopes@[si as int].close_line);
                                // Contradiction with the guard condition
                                assert(false);
                            } else if (line as int) > (final_current as int) {
                                // Covered by inner loop invariant
                            } else if line == final_current && current_in_result {
                                // current was inserted
                            } else if (line as int) < (final_current as int) {
                                // Sub-case A: line < final_current
                                // has_valid_path requires clear_path(line, exec_line)
                                // clear_path requires all l in (line, exec_line) to be classified,
                                // not ScopeClose, not Statement, not ScopeOpen.
                                // final_current is in (line, exec_line).
                                assert(has_valid_path(line, exec_line, classifications, scopes, si as int, coverage));
                                assert(clear_path(line, exec_line, classifications));
                                assert((line as int) < (final_current as int));
                                assert((final_current as int) < (exec_line as int));
                                // final_current is in the range (line, exec_line)
                                // so clear_path's forall applies to final_current
                                if stopped_at_obstacle {
                                    // stopped_at_obstacle means final_current has an obstacle:
                                    // out of bounds, None, ScopeClose, or Statement
                                    // But clear_path requires final_current to be Some, not ScopeClose,
                                    // not Statement. Contradiction.
                                    assert((final_current as int) > (line as int));
                                    assert((final_current as int) < (exec_line as int));
                                    // Instantiate clear_path's forall at l = final_current
                                    assert(classifications@[final_current as int - 1].is_some());
                                    assert(!classifications@[final_current as int - 1].unwrap()@.contains(LineProperty::ScopeClose));
                                    assert(!classifications@[final_current as int - 1].unwrap()@.contains(LineProperty::Statement));
                                    // But stopped_at_obstacle says one of these is true. Contradiction.
                                    assert(false);
                                } else if current_in_result {
                                    // current_in_result means we inserted final_current, which means
                                    // it was not an obstacle. The line was inserted and is in result.
                                    // But line < final_current, so line != final_current.
                                    // The clear_path between line and exec_line still requires
                                    // final_current to not be ScopeOpen (since we only insert
                                    // non-ScopeClose, non-Statement lines, but ScopeOpen causes done).
                                    // If done was set via ScopeOpen: final_current has ScopeOpen.
                                    // clear_path requires no ScopeOpen between line and exec_line.
                                    // final_current is between them. Contradiction.
                                    // If done was set via current <= 1: final_current <= 1.
                                    // has_valid_path requires line >= scope.open_line >= 1.
                                    // line < final_current <= 1, so line < 1. But line >= 1. Contradiction.
                                    // If !done: the loop continued, so current was decremented.
                                    // But final_current = current after the loop, and the loop
                                    // invariant covers line > current. Since line < final_current = current
                                    // after the loop... wait, final_current is set after the loop.
                                    // Actually final_current IS current after the loop.
                                    // The has_valid_path invariant covers line > current.
                                    // line < final_current = current means line is NOT covered.
                                    // We need to show contradiction.
                                    //
                                    // current_in_result means the last iteration inserted current.
                                    // That means done was set (ScopeOpen or current<=1) OR
                                    // current was decremented (current_in_result was reset to false).
                                    // If current_in_result is true after the loop, done must be true.
                                    // done ==> stopped_at_obstacle || current_in_result (invariant)
                                    // We're in the !stopped_at_obstacle && current_in_result branch.
                                    // done is true. The loop exited because done is true.
                                    // The last iteration set current_in_result and done.
                                    // done was set by ScopeOpen or current <= 1.
                                    // If ScopeOpen: classifications[final_current-1] has ScopeOpen.
                                    //   clear_path(line, exec_line) requires no ScopeOpen at final_current.
                                    //   But final_current is between line and exec_line. Contradiction.
                                    // If current <= 1: final_current <= 1, line < final_current,
                                    //   so line < 1. But has_valid_path requires in_scope which
                                    //   requires line >= scope.open_line. scope.open_line >= 1
                                    //   (from the outer if). So line >= 1. Contradiction.
                                    assert(false);
                                } else {
                                    // !stopped_at_obstacle && !current_in_result
                                    // done invariant: done ==> stopped_at_obstacle || current_in_result
                                    // Neither is set, so !done.
                                    // Loop exited with !done, so condition was false:
                                    // current < scope.open_line || current < 1
                                    // has_valid_path requires in_scope(line, scopes, si)
                                    // which requires line >= scope.open_line
                                    // line < final_current = current < scope.open_line
                                    // so line < scope.open_line. Contradiction.
                                    assert(in_scope(line, scopes, si as int));
                                    assert(line >= scopes@[si as int].open_line);
                                    // final_current < scope.open_line || final_current < 1
                                    // line < final_current, so line < scope.open_line || line < 1
                                    // Either way contradicts line >= scope.open_line >= 1
                                    assert(false);
                                }
                            } else if line == final_current && !current_in_result {
                                if stopped_at_obstacle {
                                    // Obstacle contradicts has_valid_path (already proven)
                                    assert(false);
                                } else {
                                    // !stopped_at_obstacle && !current_in_result => !done (from invariant)
                                    // Loop condition false: final_current < scope.open_line || final_current < 1
                                    // has_valid_path requires line >= scope.open_line and line >= 1
                                    // (from in_scope). line == final_current. Contradiction.
                                    assert(has_valid_path(line, exec_line, classifications, scopes, si as int, coverage));
                                    assert(in_scope(line, scopes, si as int));
                                    assert(line >= scopes@[si as int].open_line);
                                    assert(line == final_current);
                                    // !done, so loop condition was false:
                                    // final_current < scope.open_line || final_current < 1
                                    // But line == final_current >= scope.open_line. Contradiction.
                                    assert(false);
                                }
                            } else {
                                assert(false);
                            }
                        }
                    }
                    // The proof block above proved completeness for exec_line = de_vec[di].
                    // The subset invariant ensures previous j < di results are preserved.
                    proof {
                        assert(exec_line == de_vec@[di as int]);
                    }
                    di = di + 1;
                }
                // After di loop: thread to per-scope completeness
                proof {
                    assert forall|line: u64, hit_line: u64|
                        #[trigger] has_valid_path(line, hit_line, classifications, scopes, si as int, coverage)
                    implies result@.contains(line)
                    by {
                        assert(has_valid_path(line, hit_line, classifications, scopes, si as int, coverage));
                        assert(coverage@.contains_key(hit_line));
                        assert(coverage@[hit_line] == CoverageStatus::Hit);
                        assert(directly_executed@.contains(hit_line));
                        assert(de_vec@.contains(hit_line));
                        let j = choose|j: int| 0 <= j < de_vec@.len() && de_vec@[j] == hit_line;
                    }
                }
            } else {
                // has_non_linear: has_valid_path requires !scope_has_non_linear_control
                // which is false here, so has_valid_path is vacuously false
                // scope_has_non_linear_control derived from loop invariant (no assume here)
                proof {
                    assert(scope_has_non_linear_control(classifications, scopes, si as int));
                    assert forall|line: u64, hit_line: u64|
                        #[trigger] has_valid_path(line, hit_line, classifications, scopes, si as int, coverage)
                    implies result@.contains(line)
                    by { assert(false); }
                }
            }
        } else {
            // scope.open_line < 1 — unreachable by precondition
            proof {
                assert(scopes@[si as int].open_line >= 1);
                assert(false);
            }
        } // if scope.open_line >= 1
        si = si + 1;
    }
    // Completeness: the si loop invariant gives us per-scope completeness.
    proof {
        assert forall|line: u64| validly_in_exec_set(line, classifications, scopes, coverage)
        implies result@.contains(line)
        by {
            if coverage@.contains_key(line) && coverage@[line] == CoverageStatus::Hit {
                assert(directly_executed@.contains(line));
            } else {
                let (hit_line, scope_idx) = choose|hit_line: u64, scope_idx: int|
                    has_valid_path(line, hit_line, classifications, scopes, scope_idx, coverage);
                assert(0 <= scope_idx && scope_idx < scopes@.len() as int);
            }
        }
    }
    result
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

    #[test]
    fn propagates_backward_through_declaration() {
        let c = vec![
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),
            s(&[LineProperty::Declaration]),
            s(&[LineProperty::Statement]),
            s(&[LineProperty::ScopeClose]),
        ];
        let r = execution_set(
            &c,
            &[Scope {
                open_line: 1,
                close_line: 4,
                parent: None,
                children: vec![],
            }],
            &cov_hit(&[3]),
        );
        assert!(r.contains(&1));
        assert!(r.contains(&2));
        assert!(r.contains(&3));
        assert!(!r.contains(&4));
    }
    #[test]
    fn stops_at_statement() {
        let c = vec![
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),
            s(&[LineProperty::Statement]),
            s(&[LineProperty::Declaration]),
            s(&[LineProperty::Statement]),
            s(&[LineProperty::ScopeClose]),
        ];
        let r = execution_set(
            &c,
            &[Scope {
                open_line: 1,
                close_line: 5,
                parent: None,
                children: vec![],
            }],
            &cov_hit(&[4]),
        );
        assert!(r.contains(&3));
        assert!(!r.contains(&2));
    }
    #[test]
    fn stops_at_scope_close() {
        let c = vec![
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),
            s(&[LineProperty::Statement]),
            s(&[LineProperty::ScopeClose]),
            s(&[LineProperty::Whitespace]),
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),
            s(&[LineProperty::Statement]),
            s(&[LineProperty::ScopeClose]),
        ];
        let r = execution_set(
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
            &cov_hit(&[6]),
        );
        assert!(r.contains(&5));
        assert!(!r.contains(&4));
        assert!(!r.contains(&1));
    }
    #[test]
    fn stops_at_unknown() {
        let c = vec![
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::Statement, LineProperty::Declaration]),
            None,
            s(&[LineProperty::Statement]),
            s(&[LineProperty::ScopeClose]),
        ];
        let r = execution_set(
            &c,
            &[Scope {
                open_line: 1,
                close_line: 7,
                parent: None,
                children: vec![],
            }],
            &cov_hit(&[4, 6]),
        );
        assert!(!r.contains(&5));
        assert!(r.contains(&3));
        assert!(r.contains(&1));
    }
    #[test]
    fn no_propagation_with_non_linear_control() {
        let c = vec![
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::Declaration]),
            s(&[LineProperty::NonLinearControl, LineProperty::Statement]),
            s(&[LineProperty::Statement]),
            s(&[LineProperty::NonLinearControl]),
            s(&[LineProperty::Statement]),
            s(&[LineProperty::ScopeClose]),
        ];
        let r = execution_set(
            &c,
            &[Scope {
                open_line: 1,
                close_line: 9,
                parent: None,
                children: vec![],
            }],
            &cov_hit(&[5, 8]),
        );
        assert_eq!(r, BTreeSet::from([5, 8]));
    }
    #[test]
    fn scope_open_included_then_stop() {
        let c = vec![
            s(&[LineProperty::Whitespace]),
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),
            s(&[LineProperty::Declaration]),
            s(&[LineProperty::Statement]),
            s(&[LineProperty::ScopeClose]),
        ];
        let r = execution_set(
            &c,
            &[Scope {
                open_line: 2,
                close_line: 5,
                parent: None,
                children: vec![],
            }],
            &cov_hit(&[4]),
        );
        assert!(r.contains(&2));
        assert!(r.contains(&3));
        assert!(!r.contains(&1));
    }
    //= design/query/coverage-model-spec.md#property-3-conservative-fallback
    //= type=test
    //# If an ancestor scope S contains `NonLinearControl` but a child
    //# scope S' does not, propagation MAY occur through S'.
    #[test]
    fn try_block_propagation_into_parent_scope() {
        use crate::scopes::build_scope_tree;
        let c = vec![
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),
            s(&[LineProperty::Declaration]),
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),
            s(&[LineProperty::Statement]),
            s(&[
                LineProperty::Declaration,
                LineProperty::ScopeOpen,
                LineProperty::ScopeClose,
            ]),
            s(&[LineProperty::Statement]),
            s(&[LineProperty::ScopeClose]),
            s(&[LineProperty::ScopeClose]),
        ];
        let r = execution_set(&c, &build_scope_tree(&c, 8), &cov_hit(&[4]));
        assert!(r.contains(&4));
        assert!(r.contains(&3));
    }
}
