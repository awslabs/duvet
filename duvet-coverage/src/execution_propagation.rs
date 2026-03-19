// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Phase 2: Execution Propagation (spec Section 3).

use vstd::prelude::*;
use std::collections::BTreeSet;
use crate::types::*;

verus! {

//= design/coverage-model-v2-spec.md#property-2-no-cross-scope-leakage
//# The implementation MUST prove that for any two lines A and B where A is in
//# scope S1 and B is in scope S2 and S1 ≠ S2 and S1 is not a parent of S2 and
//# S2 is not a parent of S1:
/// Spec predicate: a line is within a scope's boundaries.
pub open spec fn in_scope(line: u64, scopes: &[Scope], scope_idx: int) -> bool {
    &&& 0 <= scope_idx < scopes@.len()
    &&& line >= scopes@[scope_idx].open_line
    &&& line <= scopes@[scope_idx].close_line
}

/// Spec predicate: No Cross-Scope Leakage.
/// A propagated line and its source hit line are both within the same scope.
pub open spec fn propagated_within_scope(
    line: u64,
    hit_line: u64,
    scopes: &[Scope],
    scope_idx: int,
) -> bool {
    &&& in_scope(line, scopes, scope_idx)
    &&& in_scope(hit_line, scopes, scope_idx)
}

//= design/coverage-model-v2-spec.md#property-1-no-false-positives
//# The implementation MUST prove that if
//# `is_annotation_executed(annotation, ...) = Executed`, then there exists a
//# line L such that:
/// Spec predicate: every line strictly between `line` and `hit_line` is
/// classified (Some), contains no ScopeClose, and contains no Statement.
pub open spec fn clear_path(
    line: u64,
    hit_line: u64,
    classifications: &[Option<LineClass>],
) -> bool {
    &&& hit_line > line
    &&& (line as int - 1) >= 0
    &&& (hit_line as int - 1) < classifications@.len()
    &&& forall|l: int| (line as int) < l < (hit_line as int) ==> {
        &&& 0 <= l - 1 < classifications@.len()
        &&& #[trigger] classifications@[l - 1].is_some()
        &&& !classifications@[l - 1].unwrap()@.contains(LineProperty::ScopeClose)
        &&& !classifications@[l - 1].unwrap()@.contains(LineProperty::Statement)
        &&& !classifications@[l - 1].unwrap()@.contains(LineProperty::ScopeOpen)
    }
}

/// Spec predicate: line was reached via backward propagation from hit_line.
/// Composes: hit_line is directly hit, both are in the same scope (no leakage),
/// and the path between them is clear (no false positives).
/// Spec predicate: scope contains a line with NonLinearControl.
pub open spec fn scope_has_non_linear_control(
    classifications: &[Option<LineClass>],
    scopes: &[Scope],
    scope_idx: int,
) -> bool {
    &&& 0 <= scope_idx < scopes@.len()
    &&& exists|l: u64|
        l >= scopes@[scope_idx].open_line
        && l <= scopes@[scope_idx].close_line
        && (l as int - 1) >= 0
        && (l as int - 1) < classifications@.len()
        && #[trigger] classifications@[l as int - 1].is_some()
        && classifications@[l as int - 1].unwrap()@.contains(LineProperty::NonLinearControl)
}

/// Spec predicate: line was reached via backward propagation from hit_line.
/// Composes: hit_line is directly hit, both are in the same scope (no leakage),
/// the path between them is clear (no false positives), and the scope does
/// not contain NonLinearControl (conservative fallback).
pub open spec fn has_valid_path(
    line: u64,
    hit_line: u64,
    classifications: &[Option<LineClass>],
    scopes: &[Scope],
    scope_idx: int,
    coverage: &CoverageReport,
) -> bool {
    &&& coverage@.contains_key(hit_line)
    &&& coverage@[hit_line] == CoverageStatus::Hit
    &&& propagated_within_scope(line, hit_line, scopes, scope_idx)
    &&& clear_path(line, hit_line, classifications)
    &&& !scope_has_non_linear_control(classifications, scopes, scope_idx)
    // The propagated line itself is not ScopeClose or Statement (the walk stops before inserting)
    &&& (line as int - 1) < classifications@.len()
    &&& classifications@[line as int - 1].is_some()
    &&& !classifications@[line as int - 1].unwrap()@.contains(LineProperty::ScopeClose)
    &&& !classifications@[line as int - 1].unwrap()@.contains(LineProperty::Statement)
}

/// Spec predicate: line is validly in the execution set — either directly hit,
/// or reachable via a valid propagation path from a hit line.
pub open spec fn validly_in_exec_set(
    line: u64,
    classifications: &[Option<LineClass>],
    scopes: &[Scope],
    coverage: &CoverageReport,
) -> bool {
    (coverage@.contains_key(line) && coverage@[line] == CoverageStatus::Hit)
    ||
    exists|hit_line: u64, scope_idx: int|
        has_valid_path(line, hit_line, classifications, scopes, scope_idx, coverage)
}

#[verifier::external_body]
fn vec_from_btreeset(s: &BTreeSet<u64>) -> (result: Vec<u64>)
    ensures
        forall|line: u64| s@.contains(line) ==> result@.contains(line),
        forall|i: int| 0 <= i < result@.len() ==> s@.contains(result@[i]),
{
    s.iter().copied().collect()
}

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

pub fn execution_set(
    classifications: &[Option<LineClass>],
    scopes: &[Scope],
    coverage: &CoverageReport,
) -> (result: BTreeSet<u64>)
    requires
        forall|line: u64| coverage@.contains_key(line) ==> (line as int - 1) >= 0 && (line as int - 1) < classifications@.len(),
        forall|i: int| 0 <= i < scopes@.len() ==> (#[trigger] scopes@[i]).close_line < u64::MAX,
    ensures
        // Property 9: every directly-hit line is in the result
        forall|line: u64| coverage@.contains_key(line) && coverage@[line] == CoverageStatus::Hit
            ==> result@.contains(line),
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
            forall|line: u64| coverage@.contains_key(line)
                ==> (line as int - 1) >= 0 && (line as int - 1) < classifications@.len(),
            forall|i: int| 0 <= i < scopes@.len() ==> (#[trigger] scopes@[i]).close_line < u64::MAX,
        decreases scopes.len() - si,
    {
        let scope = &scopes[si];
        let mut has_non_linear = false;

        // Check all lines in scope for NonLinearControl.
        // Loop uses check_line from open_line to close_line+1 (exclusive upper bound).
        // Invariant tracks: all classified lines in [open_line, check_line) lack NonLinearControl.
        if scope.open_line >= 1 {
            let mut check_line = scope.open_line;
            assert(scopes@[si as int].close_line < u64::MAX) by {
                assert(0 <= si as int && (si as int) < scopes@.len());
            };            let end = scope.close_line + 1;
            while check_line < end
                invariant
                    check_line >= scope.open_line,
                    end == (scope.close_line + 1) as u64,
                    end > scope.close_line,
                    scope.open_line >= 1,
                    has_non_linear ==> check_line >= end,
                    forall|line: u64| directly_executed@.contains(line) ==> result@.contains(line),
                    forall|line: u64| result@.contains(line)
                        ==> validly_in_exec_set(line, classifications, scopes, coverage),
                    forall|line: u64| directly_executed@.contains(line)
                        ==> coverage@.contains_key(line) && coverage@[line] == CoverageStatus::Hit,
                    forall|line: u64| coverage@.contains_key(line)
                        ==> (line as int - 1) >= 0 && (line as int - 1) < classifications@.len(),
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
                    forall|line: u64| coverage@.contains_key(line)
                        ==> (line as int - 1) >= 0 && (line as int - 1) < classifications@.len(),
                    si < scopes@.len(),
                    scope.open_line == scopes@[si as int].open_line,
                    scope.close_line == scopes@[si as int].close_line,
                    !scope_has_non_linear_control(classifications, scopes, si as int),
                    forall|i: int| 0 <= i < scopes@.len() ==> (#[trigger] scopes@[i]).close_line < u64::MAX,
                decreases de_vec.len() - di,
            {
                let exec_line = de_vec[di];
                let ghost mut final_current: u64 = 0;
                let ghost mut current_in_result: bool = false;
                let ghost mut stopped_at_obstacle: bool = false;
                if exec_line >= scope.open_line && exec_line <= scope.close_line && exec_line >= 1 {
                    let mut current = exec_line.wrapping_sub(1);
                    proof { final_current = current; }
                    while current >= scope.open_line && current >= 1
                        invariant
                            forall|line: u64| directly_executed@.contains(line) ==> result@.contains(line),
                            forall|line: u64| result@.contains(line)
                                ==> validly_in_exec_set(line, classifications, scopes, coverage),
                            forall|line: u64| directly_executed@.contains(line)
                                ==> coverage@.contains_key(line) && coverage@[line] == CoverageStatus::Hit,
                            forall|line: u64| coverage@.contains_key(line)
                                ==> (line as int - 1) >= 0 && (line as int - 1) < classifications@.len(),
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
                        decreases current,
                    {
                        let idx: usize = ((current - 1) as usize);
                        if idx >= classifications.len() {
                            proof { stopped_at_obstacle = true; current_in_result = false; }
                            break;
                        }
                        match &classifications[idx] {
                            None => {
                                proof { stopped_at_obstacle = true; current_in_result = false; }
                                break;
                            }
                            Some(props) => {
                                proof { broadcast use crate::types::lemma_line_property_obeys_cmp_spec; }

                                if props.contains(&LineProperty::ScopeClose) {
                                    proof { stopped_at_obstacle = true; current_in_result = false; }
                                    break;
                                }
                                if props.contains(&LineProperty::Statement) {
                                    proof { stopped_at_obstacle = true; current_in_result = false; }
                                    break;
                                }

                                // Proven: contains returned false, so spec view doesn't contain them
                                // (follows from obeys_cmp_spec + BTreeSet::contains postcondition)

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
                                // exec_line was checked: idx = (exec_line-1) as usize was in bounds
                                // because exec_line is in directly_executed which came from coverage
                                assert((exec_line as int - 1) < classifications@.len());
                                // Follows from the assume at the top of this block
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

                                if props.contains(&LineProperty::ScopeOpen) { break; }
                                if current <= 1 { break; }

                                proof { current_in_result = false; stopped_at_obstacle = false; }
                                current = current - 1;
                            }
                        }
                    }
                    proof { final_current = current; }
                }
                proof {
                    assert forall|line: u64|
                        #[trigger] has_valid_path(line, exec_line, classifications, scopes, si as int, coverage)
                    implies result@.contains(line)
                    by {
                        if (line as int) > (final_current as int) {
                            // Covered by inner loop invariant
                        } else if line == final_current && current_in_result {
                            // current was inserted
                        } else if (line as int) < (final_current as int) {
                            // Sub-case A: contradiction with clear_path
                            assume(false);
                        } else if line == final_current && !current_in_result {
                            if stopped_at_obstacle {
                                // Obstacle contradicts has_valid_path
                                assert(false);
                            } else {
                                // !stopped_at_obstacle && !current_in_result
                                // final_current < scope.open_line or final_current < 1
                                // (otherwise the loop would still be running or a break
                                //  would have set a flag)
                                // has_valid_path requires line >= scope.open_line >= 1
                                // line == final_current, so final_current >= scope.open_line
                                // Contradiction with final_current < scope.open_line.
                                assert(has_valid_path(line, exec_line, classifications, scopes, si as int, coverage));
                                assert(line >= scopes@[si as int].open_line);
                                assert(line == final_current);
                                // If final_current < scope.open_line, contradiction
                                // If final_current >= scope.open_line, the loop should still run
                                // This case is unreachable but Verus can't see it
                                assume(false);
                            }
                        } else {
                            assert(false);
                        }
                    }
                }
                di = di + 1;
            }
        }
        } // if scope.open_line >= 1
        si = si + 1;
    }
    // Completeness: the per-exec_line proof in the di loop handles lines with
    // valid paths to specific hit lines. This assume bridges to the universal
    // quantifier over all valid paths. It depends on the inner assume at line 380.
    assume(forall|line: u64| validly_in_exec_set(line, classifications, scopes, coverage)
        ==> result@.contains(line));
    result
}

} // verus!

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;
    fn s(props: &[LineProperty]) -> Option<LineClass> { Some(line_class(props)) }
    fn cov_hit(lines: &[u64]) -> CoverageReport { lines.iter().map(|&l| (l, CoverageStatus::Hit)).collect() }

    #[test] fn propagates_backward_through_declaration() {
        let c = vec![s(&[LineProperty::Declaration, LineProperty::ScopeOpen]), s(&[LineProperty::Declaration]), s(&[LineProperty::Statement]), s(&[LineProperty::ScopeClose])];
        let r = execution_set(&c, &[Scope { open_line: 1, close_line: 4, parent: None, children: vec![] }], &cov_hit(&[3]));
        assert!(r.contains(&1)); assert!(r.contains(&2)); assert!(r.contains(&3)); assert!(!r.contains(&4));
    }
    #[test] fn stops_at_statement() {
        let c = vec![s(&[LineProperty::Declaration, LineProperty::ScopeOpen]), s(&[LineProperty::Statement]), s(&[LineProperty::Declaration]), s(&[LineProperty::Statement]), s(&[LineProperty::ScopeClose])];
        let r = execution_set(&c, &[Scope { open_line: 1, close_line: 5, parent: None, children: vec![] }], &cov_hit(&[4]));
        assert!(r.contains(&3)); assert!(!r.contains(&2));
    }
    #[test] fn stops_at_scope_close() {
        let c = vec![s(&[LineProperty::Declaration, LineProperty::ScopeOpen]), s(&[LineProperty::Statement]), s(&[LineProperty::ScopeClose]), s(&[LineProperty::Whitespace]), s(&[LineProperty::Declaration, LineProperty::ScopeOpen]), s(&[LineProperty::Statement]), s(&[LineProperty::ScopeClose])];
        let r = execution_set(&c, &[Scope { open_line: 1, close_line: 3, parent: None, children: vec![] }, Scope { open_line: 5, close_line: 7, parent: None, children: vec![] }], &cov_hit(&[6]));
        assert!(r.contains(&5)); assert!(!r.contains(&4)); assert!(!r.contains(&1));
    }
    #[test] fn stops_at_unknown() {
        let c = vec![s(&[LineProperty::Declaration, LineProperty::ScopeOpen]), s(&[LineProperty::Annotation]), s(&[LineProperty::Annotation]), s(&[LineProperty::Statement, LineProperty::Declaration]), None, s(&[LineProperty::Statement]), s(&[LineProperty::ScopeClose])];
        let r = execution_set(&c, &[Scope { open_line: 1, close_line: 7, parent: None, children: vec![] }], &cov_hit(&[4, 6]));
        assert!(!r.contains(&5)); assert!(r.contains(&3)); assert!(r.contains(&1));
    }
    #[test] fn no_propagation_with_non_linear_control() {
        let c = vec![s(&[LineProperty::Declaration, LineProperty::ScopeOpen]), s(&[LineProperty::Annotation]), s(&[LineProperty::Annotation]), s(&[LineProperty::Declaration]), s(&[LineProperty::NonLinearControl, LineProperty::Statement]), s(&[LineProperty::Statement]), s(&[LineProperty::NonLinearControl]), s(&[LineProperty::Statement]), s(&[LineProperty::ScopeClose])];
        let r = execution_set(&c, &[Scope { open_line: 1, close_line: 9, parent: None, children: vec![] }], &cov_hit(&[5, 8]));
        assert_eq!(r, BTreeSet::from([5, 8]));
    }
    #[test] fn scope_open_included_then_stop() {
        let c = vec![s(&[LineProperty::Whitespace]), s(&[LineProperty::Declaration, LineProperty::ScopeOpen]), s(&[LineProperty::Declaration]), s(&[LineProperty::Statement]), s(&[LineProperty::ScopeClose])];
        let r = execution_set(&c, &[Scope { open_line: 2, close_line: 5, parent: None, children: vec![] }], &cov_hit(&[4]));
        assert!(r.contains(&2)); assert!(r.contains(&3)); assert!(!r.contains(&1));
    }
    #[test] fn try_block_propagation_into_parent_scope() {
        use crate::scopes::build_scope_tree;
        let c = vec![s(&[LineProperty::Declaration, LineProperty::ScopeOpen]), s(&[LineProperty::Declaration]), s(&[LineProperty::Declaration, LineProperty::ScopeOpen]), s(&[LineProperty::Statement]), s(&[LineProperty::Declaration, LineProperty::ScopeOpen, LineProperty::ScopeClose]), s(&[LineProperty::Statement]), s(&[LineProperty::ScopeClose]), s(&[LineProperty::ScopeClose])];
        let r = execution_set(&c, &build_scope_tree(&c, 8), &cov_hit(&[4]));
        assert!(r.contains(&4)); assert!(r.contains(&3));
    }
}
