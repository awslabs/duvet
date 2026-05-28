// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Spec predicates for the coverage model (spec Section 5).
//!
//! This module contains every spec-level predicate used in the correctness
//! properties. A reviewer can read this file top-to-bottom to see the
//! complete spec-to-predicate mapping without encountering proof engineering
//! (loop invariants, case analysis, etc.).
//!
//! The algorithm implementations in [`execution_propagation`], [`scopes`],
//! [`target_resolution`], and [`annotation_execution`] reference these
//! predicates in their `ensures` clauses. The proof functions in [`proofs`]
//! compose them into property statements.

use crate::types::*;
use vstd::prelude::*;

verus! {

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Scope predicates (spec Section 1.5, Property 8)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Spec predicate: scope i strictly contains scope j (i is a parent/ancestor).
pub open spec fn scope_contains(scopes: Seq<Scope>, i: int, j: int) -> bool {
    &&& 0 <= i < scopes.len()
    &&& 0 <= j < scopes.len()
    &&& scopes[i].open_line <= scopes[j].open_line
    &&& scopes[j].close_line <= scopes[i].close_line
    &&& (scopes[i].open_line < scopes[j].open_line || scopes[j].close_line < scopes[i].close_line)
}

//= design/query/coverage-model-spec.md#property-8-scope-well-formedness
//= type=implication
//# `build_scope_tree` produces a well-formed scope tree:
//# - Every scope has `open_line <= close_line`.
//# - If two scopes overlap,
//#   one strictly contains the other (proper nesting).
//#   No partial overlaps.
/// Spec predicate: the scope tree is well-formed.
pub open spec fn scopes_well_formed(scopes: Seq<Scope>) -> bool {
    &&& forall|i: int| 0 <= i < scopes.len() ==>
        (#[trigger] scopes[i]).open_line <= scopes[i].close_line

    &&& forall|i: int, j: int|
        0 <= i < scopes.len() && 0 <= j < scopes.len() && i != j
        && (#[trigger] scopes[i]).open_line < (#[trigger] scopes[j]).close_line
        && scopes[j].open_line < scopes[i].close_line
        ==> scope_contains(scopes, i, j) || scope_contains(scopes, j, i)
}

/// Spec predicate: scope close lines have ScopeClose in classifications.
/// Used by Property 2 (No Cross-Scope Leakage) to derive contradictions
/// when a propagation path would cross a scope boundary.
pub open spec fn scopes_match_classifications(
    scopes: Seq<Scope>,
    classifications: &[Option<LineClass>],
) -> bool {
    forall|i: int| 0 <= i < scopes.len()
        && (scopes[i].close_line as int - 1) >= 0
        && (scopes[i].close_line as int - 1) < classifications@.len()
        ==> (#[trigger] classifications@[scopes[i].close_line as int - 1]).is_some()
            && classifications@[scopes[i].close_line as int - 1].unwrap()@.contains(LineProperty::ScopeClose)
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Execution propagation predicates (spec Section 3, Properties 1–3)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Spec predicate: a line is within a scope's boundaries.
pub open spec fn in_scope(line: u64, scopes: &[Scope], scope_idx: int) -> bool {
    &&& 0 <= scope_idx < scopes@.len()
    &&& line >= scopes@[scope_idx].open_line
    &&& line <= scopes@[scope_idx].close_line
}

//= design/query/coverage-model-spec.md#property-2-no-cross-scope-leakage
//= type=implication
//# The implementation MUST prove that
//# for any two lines A and B
//# where A is in scope S1 and B is in scope S2
//# and S1 ≠ S2
//# and S1 is not a parent of S2
//# and S2 is not a parent of S1:
/// Spec predicate: a propagated line and its source hit line are both
/// within the same scope. Encodes the No Cross-Scope Leakage invariant.
pub open spec fn propagated_within_scope(
    line: u64,
    hit_line: u64,
    scopes: &[Scope],
    scope_idx: int,
) -> bool {
    &&& in_scope(line, scopes, scope_idx)
    &&& in_scope(hit_line, scopes, scope_idx)
}

//= design/query/coverage-model-spec.md#property-1-no-false-positives
//= type=implication
//# The implementation MUST prove that if
//# `is_annotation_executed(annotation, ...) = Executed`,
//# then there exists a line L such that:
/// Spec predicate: every line strictly between `line` and `hit_line` is
/// classified (Some), contains no ScopeClose, no Statement, and no ScopeOpen.
///
/// Encodes the path-clarity requirement of Property 1. The sub-conditions
/// from the spec map to conjuncts as follows:
pub open spec fn clear_path(
    line: u64,
    hit_line: u64,
    classifications: &[Option<LineClass>],
) -> bool {
    //= design/query/coverage-model-spec.md#property-1-no-false-positives
    //= type=implication
    //# - Every line between L and the annotation's target (exclusive)
    //#   is classified (`Some`)
    //#   and has properties that are a subset of
    //#   {Whitespace, Comment, Annotation, Declaration, ScopeOpen}
    &&& hit_line > line
    &&& (line as int - 1) >= 0
    &&& (hit_line as int - 1) < classifications@.len()
    &&& forall|l: int| (line as int) < l < (hit_line as int) ==> {
        &&& 0 <= l - 1 < classifications@.len()
        &&& #[trigger] classifications@[l - 1].is_some()
        //= design/query/coverage-model-spec.md#property-1-no-false-positives
        //= type=implication
        //# - No line between L and the annotation's target
        //#   has the `ScopeClose` property
        &&& !classifications@[l - 1].unwrap()@.contains(LineProperty::ScopeClose)
        &&& !classifications@[l - 1].unwrap()@.contains(LineProperty::Statement)
        //= design/query/coverage-model-spec.md#property-1-no-false-positives
        //= type=implication
        //# - No line between L and the annotation's target
        //#   is unknown (`None`)
        &&& !classifications@[l - 1].unwrap()@.contains(LineProperty::ScopeOpen)
    }
}

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

//= design/query/coverage-model-spec.md#property-3-conservative-fallback
//= type=implication
//# The implementation MUST prove that
//# no backward propagation occurs WITHIN a scope
//# that contains a `NonLinearControl` line.
//= design/query/coverage-model-spec.md#property-3-conservative-fallback
//= type=implication
//# If an ancestor scope S contains `NonLinearControl` but a child
//# scope S' does not, propagation MAY occur through S'.
/// Spec predicate: line was reached via backward propagation from hit_line.
///
/// Composes the sub-properties:
/// - `coverage[hit_line] == Hit` (directly reported as executed)
/// - `propagated_within_scope` (Property 2: no cross-scope leakage)
/// - `clear_path` (Property 1: no false positives)
/// - `!scope_has_non_linear_control` (Property 3: conservative fallback)
/// - The propagated line itself is not ScopeClose or Statement
pub open spec fn has_valid_path(
    line: u64,
    hit_line: u64,
    classifications: &[Option<LineClass>],
    scopes: &[Scope],
    scope_idx: int,
    coverage: &CoverageReport,
) -> bool {
    //= design/query/coverage-model-spec.md#property-1-no-false-positives
    //= type=implication
    //# - `coverage[L] == Hit`
    //#   (L is directly reported as executed)
    &&& coverage@.contains_key(hit_line)
    &&& coverage@[hit_line] == CoverageStatus::Hit
    //= design/query/coverage-model-spec.md#property-1-no-false-positives
    //= type=implication
    //# - L is in the same scope as the annotation's target
    &&& propagated_within_scope(line, hit_line, scopes, scope_idx)
    &&& clear_path(line, hit_line, classifications)
    &&& !scope_has_non_linear_control(classifications, scopes, scope_idx)
    // The propagated line itself is not ScopeClose or Statement (the walk stops before inserting)
    &&& (line as int - 1) < classifications@.len()
    &&& classifications@[line as int - 1].is_some()
    &&& !classifications@[line as int - 1].unwrap()@.contains(LineProperty::ScopeClose)
    &&& !classifications@[line as int - 1].unwrap()@.contains(LineProperty::Statement)
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Composite predicate (spec Section 3.3)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

//= design/query/coverage-model-spec.md#property-9-execution-set-containment
//= type=implication
//# If `coverage[line] == Hit`,
//# then `line ∈ execution_set(classifications, scopes, coverage)`.
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

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Target resolution predicates (spec Section 2, Property 5)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

//= design/query/coverage-model-spec.md#property-5-stacking-transitivity
//= type=implication
//# if annotation A (lines a1..a2) is immediately above
//# annotation B (lines b1..b2)
//# with only whitespace, comments, or other annotations between them
/// Spec predicate: a line is "skippable" by the forward walk —
/// it is classified as pure Whitespace, pure Comment, or contains Annotation.
/// These are exactly the line types that `annotation_target` skips over.
pub open spec fn line_is_skippable(
    classifications: &[Option<LineClass>],
    line: u64,
) -> bool {
    &&& (line as int - 1) >= 0
    &&& (line as int - 1) < classifications@.len()
    &&& classifications@[line as int - 1].is_some()
    &&& (
        // Pure Whitespace (len == 1 && contains Whitespace)
        (classifications@[line as int - 1].unwrap()@.len() == 1
         && classifications@[line as int - 1].unwrap()@.contains(LineProperty::Whitespace))
        // Pure Comment (len == 1 && contains Comment)
        || (classifications@[line as int - 1].unwrap()@.len() == 1
            && classifications@[line as int - 1].unwrap()@.contains(LineProperty::Comment))
        // Contains Annotation
        || classifications@[line as int - 1].unwrap()@.contains(LineProperty::Annotation)
    )
}

/// Spec predicate: all lines in the range [start, end] are skippable.
/// Used by Property 5 to express that annotations A and B have only
/// skippable lines between them.
pub open spec fn all_lines_skippable(
    classifications: &[Option<LineClass>],
    start: u64,
    end: u64,
) -> bool {
    forall|l: u64| start <= l && l <= end ==> line_is_skippable(classifications, l)
}

} // verus!
