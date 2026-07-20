// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Scope analysis (spec Section 1.5).
//!
//! This module contains the `build_scope_tree` algorithm and its proof
//! engineering. The spec predicates it references (`scopes_well_formed`,
//! `scope_contains`, `scopes_match_classifications`) are defined in
//! [`crate::predicates`] for reviewer accessibility.
//!
//! # What a scope is (spec §1.5, inlined for the reader)
//!
//! A *scope* is a contiguous range of lines delimited by a `ScopeOpen` line and
//! a `ScopeClose` line; scopes nest. For brace languages these are literally the
//! `{` and `}` lines (`public void foo() {` → `{Declaration, ScopeOpen}`, a bare
//! `}` → `{ScopeClose}`); for indentation languages the AST parser marks the
//! first and last line of each block. The tree is recovered by matching opens to
//! closes as *balanced parentheses* — this module's whole job.
//!
//! Two spec rules drive the corner cases below:
//!   - Unknown (`None`) lines do not contribute: an unknown line cannot be a
//!     `ScopeOpen`/`ScopeClose`, so it is simply skipped (spec §1.5).
//!   - A line may carry `ScopeClose` *and* `ScopeOpen` at once — e.g.
//!     `} catch (e) {` → `{ScopeClose, ScopeOpen, Declaration}`. Closes are
//!     therefore processed before opens on the same line, so the two siblings
//!     match correctly.

#[cfg(verus_keep_ghost)]
pub use crate::predicates::{scope_contains, scopes_match_classifications, scopes_well_formed};
use crate::types::*;
use vstd::prelude::*;

verus! {

/// Two-pass scope tree construction (spec §1.5).
///
/// Pass 1 (`match_scope_pairs`): walk the classified lines with a stack, pushing
///         on `ScopeOpen` and popping on `ScopeClose`, to recover the balanced
///         `(open_line, close_line)` pairs. Any imbalance discards the pairs.
/// Pass 2 (`build_from_pairs`): turn each pair into a `Scope`. Containment is
///         expressed purely through `open_line`/`close_line` (see `scope_contains`
///         in predicates.rs), so no parent/children wiring is needed here.
///
/// No mutation of existing elements — each scope is created once with final values.
///
/// Lines the classifier left `None` are skipped (spec §1.5: an unknown line
/// cannot be a scope delimiter). On unbalanced input the tree collapses to a
/// single file-level scope spanning the whole file — see the trust-boundary note
/// at the fallback below for why that is a deliberate, if lossy, choice.
pub fn build_scope_tree(classifications: &[Option<LineClass>], file_length: u64) -> (scopes: Vec<Scope>)
    requires file_length < u64::MAX,
    ensures
        scopes_well_formed(scopes@),
        // The scope-bound preconditions of `is_annotation_executed` (#3/#4).
        // Stated here so the runtime trust boundary in `executed_status_for`
        // can rely on the *contract*, not on this function's implementation.
        forall|i: int| 0 <= i < scopes@.len() ==>
            (#[trigger] scopes@[i]).open_line >= 1
            && scopes@[i].close_line < u64::MAX,
{
    // Pass 1: collect (open_line, close_line) pairs
    let pairs = match_scope_pairs(classifications, file_length);

    // TRUST BOUNDARY: `match_scope_pairs` returns empty on *any* ScopeOpen/
    // ScopeClose imbalance, and we then fall back to one file-level scope below.
    // The proofs guarantee the result is well-formed, but a file-level scope is
    // a well-formed *wrong* tree if the imbalance was spurious: every annotation
    // in the file then resolves against one giant scope, which can turn a real
    // `Structural` (e.g. an interface method) into `NotExecuted` because the
    // whole-file scan finds some unrelated statement. Balanced classifier output
    // is an *assumption* this crate cannot verify -- it is the unverified Java
    // classifier's responsibility (a dropped ScopeClose, e.g. from an over-broad
    // mutual-exclusivity post-pass, would trigger exactly this degradation). If
    // this fallback fires on real code, suspect the classifier, not the model.
    if pairs.len() == 0 {
        if file_length >= 1 {
            let s = vec![Scope { open_line: 1, close_line: file_length, parent: None, children: vec![] }];
            assert(s@.len() == 1);
            return s;
        } else {
            return vec![];
        }
    }

    // Pass 2: build Scope structs with parent/children
    build_from_pairs(&pairs)
}

/// Pass 1: Match balanced ScopeOpen/ScopeClose using a stack.
/// Returns Vec of (open_line, close_line) pairs.
/// Returns empty Vec on unbalanced input (fallback to file-level scope).
///
/// On each classified line, `ScopeClose` is handled *before* `ScopeOpen` so that
/// a line carrying both — e.g. `} catch (e) {` or `} else {` — first closes the
/// scope it ends and then opens the sibling it begins (spec §1.5). A `ScopeClose`
/// with an empty stack, or a leftover open on the stack at EOF, is an imbalance:
/// the pairs are discarded and the caller falls back to a file-level scope.
fn match_scope_pairs(classifications: &[Option<LineClass>], file_length: u64) -> (pairs: Vec<(u64, u64)>)
    requires file_length < u64::MAX,
    ensures
        // Every pair has open >= 1
        forall|i: int| 0 <= i < pairs@.len() ==>
            (#[trigger] pairs@[i]).0 >= 1,
        // Every pair has open <= close
        forall|i: int| 0 <= i < pairs@.len() ==>
            (#[trigger] pairs@[i]).0 <= pairs@[i].1,
        // Every pair has close < u64::MAX (emitted at line_num, and the loop
        // invariant keeps every emitted close < line_num <= file_length < u64::MAX).
        forall|i: int| 0 <= i < pairs@.len() ==>
            (#[trigger] pairs@[i]).1 < u64::MAX,
        // Pairs are properly nested: if two strictly overlap, one contains the other
        forall|i: int, j: int| 0 <= i < pairs@.len() && 0 <= j < pairs@.len() && i != j
            && (#[trigger] pairs@[i]).0 < (#[trigger] pairs@[j]).1
            && pairs@[j].0 < pairs@[i].1
            ==> (pairs@[i].0 <= pairs@[j].0 && pairs@[j].1 <= pairs@[i].1
                 && (pairs@[i].0 < pairs@[j].0 || pairs@[j].1 < pairs@[i].1))
                || (pairs@[j].0 <= pairs@[i].0 && pairs@[i].1 <= pairs@[j].1
                    && (pairs@[j].0 < pairs@[i].0 || pairs@[i].1 < pairs@[j].1)),
{
    let mut stack: Vec<u64> = Vec::new();
    let mut pairs: Vec<(u64, u64)> = Vec::new();
    let mut done = false;
    let mut unbalanced = false;

    let mut line_num: u64 = 1;
    while line_num <= file_length && !done
        invariant
            line_num >= 1,
            file_length < u64::MAX,
            // Stack is sorted strictly ascending
            forall|k: int| 0 <= k < stack@.len() ==> (#[trigger] stack@[k]) >= 1,
            forall|k: int, l: int| 0 <= k < l && l < stack@.len()
                ==> (#[trigger] stack@[k]) < (#[trigger] stack@[l]),
            // All stack entries < line_num (pushed at earlier line_nums)
            forall|k: int| 0 <= k < stack@.len() ==> (#[trigger] stack@[k]) < line_num,
            // All emitted pairs have open >= 1 and open <= close
            forall|i: int| 0 <= i < pairs@.len() ==> (#[trigger] pairs@[i]).0 >= 1,
            forall|i: int| 0 <= i < pairs@.len() ==> (#[trigger] pairs@[i]).0 <= pairs@[i].1,
            // All emitted pairs have close < line_num (emitted at earlier line_nums)
            forall|i: int| 0 <= i < pairs@.len() ==> (#[trigger] pairs@[i]).1 < line_num,
            // Nesting: emitted pairs are properly nested
            forall|i: int, j: int| 0 <= i < pairs@.len() && 0 <= j < pairs@.len() && i != j
                && (#[trigger] pairs@[i]).0 < (#[trigger] pairs@[j]).1
                && pairs@[j].0 < pairs@[i].1
                ==> (pairs@[i].0 <= pairs@[j].0 && pairs@[j].1 <= pairs@[i].1
                     && (pairs@[i].0 < pairs@[j].0 || pairs@[j].1 < pairs@[i].1))
                    || (pairs@[j].0 <= pairs@[i].0 && pairs@[i].1 <= pairs@[j].1
                        && (pairs@[j].0 < pairs@[i].0 || pairs@[i].1 < pairs@[j].1)),
            // Key nesting bridge: every emitted pair is either at or before
            // a stack entry, or strictly contained within it (open > stack entry).
            forall|i: int, k: int| 0 <= i < pairs@.len() && 0 <= k < stack@.len()
                ==> (#[trigger] pairs@[i]).1 <= (#[trigger] stack@[k])
                    || (stack@[k] < pairs@[i].0 && pairs@[i].0 <= pairs@[i].1),
        decreases if done { 0 } else { file_length - line_num + 2 },
    {
        let idx: usize = ((line_num - 1) as usize);
        if idx >= classifications.len() {
            done = true;
        } else {
        match &classifications[idx] {
            None => { }
            Some(props) => {
                proof { broadcast use crate::types::lemma_line_property_obeys_cmp_spec; }

                // Process ScopeClose BEFORE ScopeOpen (handles } catch {, } else {)
                if props.contains(&LineProperty::ScopeClose) {
                    if stack.len() == 0 {
                        // Unbalanced — return empty
                        unbalanced = true;
                        done = true;
                    } else {
                        let open = stack[stack.len() - 1];
                        // Prove the new pair (open, line_num) nests with all existing pairs.
                        // open is the top of stack. All existing pairs with close >= open
                        // have open > some earlier stack entry, meaning they're inside
                        // the scope we're closing. Their close < line_num (invariant).
                        // So they're strictly contained in (open, line_num).
                        proof {
                            assert(open >= 1u64);
                            assert(open < line_num);
                            assert forall|i: int| 0 <= i < pairs@.len()
                                && (#[trigger] pairs@[i]).0 < line_num
                                && open < pairs@[i].1
                            implies
                                (open <= pairs@[i].0 && pairs@[i].1 <= line_num
                                 && (open < pairs@[i].0 || pairs@[i].1 < line_num))
                            by {
                                assert(pairs@[i].1 < line_num);
                            }
                        }
                        let ghost pairs_before = pairs@;
                        let ghost stack_before = stack@;
                        stack.pop();
                        pairs.push((open, line_num));
                        proof {
                            // After pop, remaining stack entries are stack_before[0..len-1].
                            // They were all < open (strictly ascending, open was last).
                            // New pair (open, line_num) satisfies bridge: stack[k] < open = new_pair.0.
                            // Old pairs: bridge held with stack_before, and stack is a prefix of
                            // stack_before (minus the last element), so it still holds.
                            assert forall|i: int, k: int|
                                0 <= i < pairs@.len() && 0 <= k < stack@.len()
                            implies
                                (#[trigger] pairs@[i]).1 <= (#[trigger] stack@[k])
                                || (stack@[k] < pairs@[i].0 && pairs@[i].0 <= pairs@[i].1)
                            by {
                                assert(stack@[k] == stack_before[k]);
                                assert(stack@[k] < open);
                                if i < pairs_before.len() {
                                    // Old pair: bridge held before with stack_before[k]
                                    assert(pairs@[i] == pairs_before[i]);
                                } else {
                                    // New pair: (open, line_num). stack[k] < open.
                                    assert(pairs@[i] == (open, line_num));
                                    assert(stack@[k] < open);
                                }
                            }
                        }
                    }
                }
                if !done && props.contains(&LineProperty::ScopeOpen) {
                    // All existing stack entries < line_num (invariant).
                    // line_num > all stack entries, so sorted order is maintained.
                    proof {
                        // Bridge for new stack entry: for all existing pairs (a,b),
                        // b < line_num (from invariant), so first disjunct holds.
                        assert forall|i: int| 0 <= i < pairs@.len()
                        implies
                            (#[trigger] pairs@[i]).1 <= line_num
                        by {}
                    }
                    stack.push(line_num);
                }
            }
        }
        }
        if !done {
            line_num = line_num + 1;
        }
    }

    if unbalanced || !stack.is_empty() {
        // Unbalanced — return empty
        return vec![];
    }

    pairs
}

/// Pass 2: Build Scope structs from (open, close) pairs.
/// Determines parent/children by containment.
fn build_from_pairs(pairs: &[(u64, u64)]) -> (scopes: Vec<Scope>)
    requires
        forall|i: int| 0 <= i < pairs@.len() ==>
            (#[trigger] pairs@[i]).0 <= pairs@[i].1,
        forall|i: int, j: int| 0 <= i < pairs@.len() && 0 <= j < pairs@.len() && i != j
            && (#[trigger] pairs@[i]).0 < (#[trigger] pairs@[j]).1
            && pairs@[j].0 < pairs@[i].1
            ==> (pairs@[i].0 <= pairs@[j].0 && pairs@[j].1 <= pairs@[i].1
                 && (pairs@[i].0 < pairs@[j].0 || pairs@[j].1 < pairs@[i].1))
                || (pairs@[j].0 <= pairs@[i].0 && pairs@[i].1 <= pairs@[j].1
                    && (pairs@[j].0 < pairs@[i].0 || pairs@[i].1 < pairs@[j].1)),
    ensures
        scopes_well_formed(scopes@),
        scopes@.len() == pairs@.len(),
        forall|i: int| 0 <= i < scopes@.len() ==>
            (#[trigger] scopes@[i]).open_line == pairs@[i].0
            && scopes@[i].close_line == pairs@[i].1,
{
    // Create scopes — one per pair, no mutation after creation
    let mut scopes: Vec<Scope> = Vec::new();
    let mut i: usize = 0;
    while i < pairs.len()
        invariant
            i <= pairs@.len(),
            scopes@.len() == i as int,
            forall|k: int| 0 <= k < i as int ==>
                (#[trigger] scopes@[k]).open_line == pairs@[k].0
                && scopes@[k].close_line == pairs@[k].1,
        decreases pairs.len() - i,
    {
        scopes.push(Scope {
            open_line: pairs[i].0,
            close_line: pairs[i].1,
            parent: None,
            children: vec![],
        });
        i = i + 1;
    }

    // At this point scopes has the right open/close lines.
    // The well-formedness follows from the pairs' nesting property:
    // scopes[i].open_line == pairs[i].0 and scopes[i].close_line == pairs[i].1,
    // and pairs satisfy the nesting precondition.
    assert(scopes@.len() == pairs@.len());

    // Prove well-formedness from the pairs precondition
    assert forall|i: int| 0 <= i < scopes@.len() implies
        (#[trigger] scopes@[i]).open_line <= scopes@[i].close_line
    by {
        assert(scopes@[i].open_line == pairs@[i].0);
        assert(scopes@[i].close_line == pairs@[i].1);
    }

    assert forall|i: int, j: int|
        0 <= i < scopes@.len() && 0 <= j < scopes@.len() && i != j
        && (#[trigger] scopes@[i]).open_line < (#[trigger] scopes@[j]).close_line
        && scopes@[j].open_line < scopes@[i].close_line
    implies
        scope_contains(scopes@, i, j) || scope_contains(scopes@, j, i)
    by {
        assert(scopes@[i].open_line == pairs@[i].0);
        assert(scopes@[i].close_line == pairs@[i].1);
        assert(scopes@[j].open_line == pairs@[j].0);
        assert(scopes@[j].close_line == pairs@[j].1);
    }

    // `parent`/`children` are left `None`/`vec![]`. Nothing reads them: the
    // model navigates scopes purely by `open_line`/`close_line` containment
    // (see `scope_contains` in predicates.rs), and `scopes_well_formed` depends
    // only on those two fields. The fields are reserved for a future
    // tree-shaped API; until something consumes them, leaving them empty is
    // correct and the proofs are indifferent to their values.
    scopes
}

// ---------------------------------------------------------------------------
// Scope-stream balance detection (spec §1.5 Scope Balance Contract).
//
// `build_scope_tree` collapses to a single whole-file scope whenever the
// ScopeOpen/ScopeClose stream fails to match as balanced parentheses. That
// collapse is a well-formed *wrong* tree, and the classifier is the only party
// that can guarantee balance — so the dispatcher must be able to *detect* the
// imbalance and escalate rather than silently score against the collapsed tree.
//
// This detector characterises imbalance with a depth counter, which is exactly
// `match_scope_pairs`' stack *size*: process `ScopeClose` before `ScopeOpen` on
// each classified line (so `} else {` closes then opens); a `ScopeClose` while
// the depth is 0 is a stray close (underflow), and a nonzero depth at EOF is a
// set of unclosed opens. Either is an imbalance. A stream with no delimiters at
// all has depth 0 throughout with no underflow — it is *balanced*, and its
// whole-file scope is legitimate, so it is deliberately NOT flagged. This is the
// distinction the naive "single scope spanning the file" heuristic cannot make.
// ---------------------------------------------------------------------------

/// Spec: the classified line `c` carries `ScopeClose`.
pub open spec fn spec_has_close(c: Option<LineClass>) -> bool {
    match c {
        Some(cls) => cls@.contains(LineProperty::ScopeClose),
        None => false,
    }
}

/// Spec: the classified line `c` carries `ScopeOpen`.
pub open spec fn spec_has_open(c: Option<LineClass>) -> bool {
    match c {
        Some(cls) => cls@.contains(LineProperty::ScopeOpen),
        None => false,
    }
}

/// Spec twin of the balance walk. Folds the first `n` classified lines with a
/// scope-depth counter, returning `(final_depth, no_underflow)`. `ScopeClose` is
/// applied before `ScopeOpen` on each line; a close at depth 0 records underflow
/// (and does not drive the depth negative), matching `match_scope_pairs`'
/// empty-stack rule. `no_underflow` is absorbing-false: once a stray close is
/// seen it stays false regardless of later lines.
pub open spec fn balance_upto(c: Seq<Option<LineClass>>, n: int) -> (int, bool)
    decreases n,
{
    if n <= 0 {
        (0int, true)
    } else {
        let prev = balance_upto(c, n - 1);
        let d = prev.0;
        let ok = prev.1;
        let ci = c[n - 1];
        let after_close: (int, bool) = if spec_has_close(ci) {
            if d <= 0 {
                (d, false)
            } else {
                (d - 1, ok)
            }
        } else {
            (d, ok)
        };
        let d2: int = if spec_has_open(ci) {
            after_close.0 + 1
        } else {
            after_close.0
        };
        (d2, after_close.1)
    }
}

/// Spec: the ScopeOpen/ScopeClose stream over the first `n` classified lines is
/// balanced — no stray close, and every open matched by EOF.
pub open spec fn scope_stream_balanced_spec(c: Seq<Option<LineClass>>, n: int) -> bool {
    let r = balance_upto(c, n);
    r.1 && r.0 == 0
}

/// The number of classified lines the balance walk consumes: `min(file_length,
/// classifications.len())`, mirroring `match_scope_pairs`, which stops at the
/// shorter of `file_length` and the classification vector.
pub open spec fn balance_bound(c: Seq<Option<LineClass>>, file_length: u64) -> int {
    if (file_length as int) < c.len() as int {
        file_length as int
    } else {
        c.len() as int
    }
}

/// Detect an unbalanced scope stream and locate it.
///
/// Returns `None` when the stream is balanced (including the no-delimiters case,
/// which is a legitimate whole-file scope — deliberately NOT an imbalance).
/// Returns `Some(line)` when unbalanced, where `line` is the offending stray
/// `ScopeClose` or, failing that, the last unmatched `ScopeOpen` — a diagnostic
/// aid. The soundness-critical guarantee is the `is None <==> balanced`
/// equivalence in the `ensures`; the witness line itself is not otherwise
/// constrained by the proof.
//= design/query/coverage-model-spec.md#property-11-scope-stream-balance-detection
//= type=implementation
//# The implementation MUST prove that the balance detector returns balanced if and
//# only if the `ScopeOpen`/`ScopeClose` stream over the classified lines is balanced:
//# no `ScopeClose` occurs while the scope depth is zero, and the depth is zero at
//# end of file.
pub fn scope_imbalance_site(classifications: &[Option<LineClass>], file_length: u64) -> (result:
    Option<u64>)
    requires
        file_length < u64::MAX,
    ensures
        (result is None) <==> scope_stream_balanced_spec(
            classifications@,
            balance_bound(classifications@, file_length),
        ),
{
    let mut depth: u64 = 0;
    let mut underflow_line: Option<u64> = None;
    let mut last_open_line: u64 = 0;
    let mut line_num: u64 = 1;

    while line_num <= file_length
        invariant
            file_length < u64::MAX,
            1 <= line_num <= file_length + 1,
            depth as int <= line_num as int - 1,
            ({
                let processed = if (line_num as int - 1) < classifications@.len() as int {
                    line_num as int - 1
                } else {
                    classifications@.len() as int
                };
                let r = balance_upto(classifications@, processed);
                &&& depth as int == r.0
                &&& (underflow_line is None) <==> r.1
            }),
        decreases file_length - line_num + 1,
    {
        let idx: usize = (line_num - 1) as usize;
        if idx >= classifications.len() {
            // Past the classified lines: `match_scope_pairs` stops here, so the
            // consumed count is `classifications.len()` and the walk is complete.
            assert(classifications@.len() as int <= line_num as int - 1);
            return if underflow_line.is_some() {
                underflow_line
            } else if depth == 0 {
                None
            } else {
                Some(last_open_line)
            };
        }
        proof {
            broadcast use crate::types::lemma_line_property_obeys_cmp_spec;
        }
        match &classifications[idx] {
            None => {}
            Some(props) => {
                if props.contains(&LineProperty::ScopeClose) {
                    if depth == 0 {
                        if underflow_line.is_none() {
                            underflow_line = Some(line_num);
                        }
                    } else {
                        depth = depth - 1;
                    }
                }
                if props.contains(&LineProperty::ScopeOpen) {
                    last_open_line = line_num;
                    depth = depth + 1;
                }
            }
        }
        line_num = line_num + 1;
    }

    if underflow_line.is_some() {
        underflow_line
    } else if depth == 0 {
        None
    } else {
        Some(last_open_line)
    }
}

} // verus!

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;
    fn s(props: &[LineProperty]) -> Option<LineClass> {
        Some(line_class(props))
    }

    //= design/query/coverage-model-spec.md#scopes
    //= type=test
    //# A scope is a contiguous range of lines delimited by `ScopeOpen` and
    //# `ScopeClose` properties. Scopes nest.
    #[test]
    fn simple_method_in_class() {
        let c = vec![
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),
            s(&[LineProperty::Statement]),
            s(&[LineProperty::ScopeClose]),
            s(&[LineProperty::ScopeClose]),
        ];
        let sc = build_scope_tree(&c, 5);
        assert_eq!(sc.len(), 2);
        // Find the outer and inner scopes by open_line
        let outer = sc.iter().find(|s| s.open_line == 1).unwrap();
        let inner = sc.iter().find(|s| s.open_line == 2).unwrap();
        assert_eq!(outer.close_line, 5);
        assert_eq!(inner.close_line, 4);
    }
    #[test]
    fn sibling_methods() {
        let c = vec![
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),
            s(&[LineProperty::ScopeClose]),
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),
            s(&[LineProperty::ScopeClose]),
            s(&[LineProperty::ScopeClose]),
        ];
        let sc = build_scope_tree(&c, 6);
        assert_eq!(sc.len(), 3);
    }
    #[test]
    fn unbalanced_fallback() {
        let sc = build_scope_tree(
            &vec![s(&[LineProperty::ScopeOpen]), s(&[LineProperty::Statement])],
            2,
        );
        // Unbalanced: pairs returns empty, so we get file-level scope
        assert!(sc.len() >= 1);
        assert_eq!(sc[0].open_line, 1);
        assert_eq!(sc[0].close_line, 2);
    }
    #[test]
    fn empty_file() {
        assert_eq!(build_scope_tree(&vec![], 0).len(), 0);
    }
    #[test]
    fn unknown_lines_ignored() {
        let c = vec![
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),
            None,
            s(&[LineProperty::Statement]),
            s(&[LineProperty::ScopeClose]),
        ];
        let sc = build_scope_tree(&c, 4);
        assert_eq!(sc.len(), 1);
        assert_eq!(sc[0].open_line, 1);
        assert_eq!(sc[0].close_line, 4);
    }
    #[test]
    fn four_level_nesting() {
        let c = vec![
            s(&[LineProperty::ScopeOpen]),
            s(&[LineProperty::ScopeOpen]),
            s(&[LineProperty::ScopeOpen]),
            s(&[LineProperty::ScopeOpen]),
            s(&[LineProperty::ScopeClose]),
            s(&[LineProperty::ScopeClose]),
            s(&[LineProperty::ScopeClose]),
            s(&[LineProperty::ScopeClose]),
        ];
        let sc = build_scope_tree(&c, 8);
        assert_eq!(sc.len(), 4);
    }
    #[test]
    fn catch_creates_sibling_scopes() {
        let c = vec![
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),
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
        let sc = build_scope_tree(&c, 7);
        assert_eq!(sc.len(), 3);
    }

    // --- scope_imbalance_site: empirical witnesses (spec §1.5 balance) ---
    //
    // The verified `ensures` proves `is None <==> balanced` against the
    // depth-counter spec. These witnesses ground that spec against reality AND
    // demonstrate the tie to `build_scope_tree`: where the detector says
    // balanced (None), the tree is faithful (>1 scope, or a real single scope);
    // where it says unbalanced (Some), the tree collapses to the whole-file
    // fallback. This is the distinction the "single scope spanning the file"
    // heuristic cannot make.

    #[test]
    fn balanced_multiscope_is_not_flagged() {
        // Nested method-in-class: two balanced scopes.
        let c = vec![
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),
            s(&[LineProperty::Statement]),
            s(&[LineProperty::ScopeClose]),
            s(&[LineProperty::ScopeClose]),
        ];
        assert_eq!(scope_imbalance_site(&c, 5), None);
        // Tie: the tree is faithful (two real scopes), not the collapse.
        assert_eq!(build_scope_tree(&c, 5).len(), 2);
    }

    #[test]
    fn genuine_whole_file_single_scope_is_not_flagged() {
        // A class whose `{` is line 1 and `}` is the last line, with only a
        // field inside: ONE balanced scope spanning 1..=3. Output shape is
        // indistinguishable from the fallback, but it is balanced — the detector
        // must NOT flag it. This is the false-positive the shape heuristic hits.
        let c = vec![
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),
            s(&[LineProperty::Statement]),
            s(&[LineProperty::ScopeClose]),
        ];
        assert_eq!(scope_imbalance_site(&c, 3), None);
        // Tie: build_scope_tree recovers the REAL scope (1,3) via a pair, not
        // the collapse fallback — both are `[Scope{1,3}]`, so shape alone can't
        // tell them apart, but balance can.
        let sc = build_scope_tree(&c, 3);
        assert_eq!(sc.len(), 1);
        assert_eq!((sc[0].open_line, sc[0].close_line), (1, 3));
    }

    #[test]
    fn no_delimiters_is_not_flagged() {
        // Pure statements, no scopes. Depth stays 0, never underflows: balanced.
        // The whole-file scope build_scope_tree returns here is legitimate.
        //= design/query/coverage-model-spec.md#property-11-scope-stream-balance-detection
        //= type=test
        //# A stream with no scope delimiters is balanced, and its whole-file scope is
        //# legitimate.
        let c = vec![s(&[LineProperty::Statement]), s(&[LineProperty::Statement])];
        assert_eq!(scope_imbalance_site(&c, 2), None);
    }

    #[test]
    fn all_none_is_not_flagged() {
        // The idx55 parse-error representation (has_error -> all `None`). No
        // delimiters -> balanced -> NOT an imbalance. Confirms the detector does
        // not double-report the parse-error case: that escalation rides on
        // `has_error`, not on this predicate.
        let c: Vec<Option<LineClass>> = vec![None, None, None];
        assert_eq!(scope_imbalance_site(&c, 3), None);
    }

    #[test]
    fn stray_close_is_flagged_and_tree_collapses() {
        // A `}` with nothing open: stray close (depth underflow at line 1).
        let c = vec![s(&[LineProperty::ScopeClose]), s(&[LineProperty::Statement])];
        assert_eq!(scope_imbalance_site(&c, 2), Some(1));
        // Tie: build_scope_tree collapses to the whole-file fallback.
        let sc = build_scope_tree(&c, 2);
        assert_eq!(sc.len(), 1);
        assert_eq!((sc[0].open_line, sc[0].close_line), (1, 2));
    }

    #[test]
    fn unclosed_open_is_flagged_and_tree_collapses() {
        // A `{` never closed: leftover depth at EOF.
        let c = vec![
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),
            s(&[LineProperty::Statement]),
        ];
        let site = scope_imbalance_site(&c, 2);
        assert!(site.is_some(), "unclosed open must be flagged, got {site:?}");
        // Tie: build_scope_tree collapses to the whole-file fallback.
        let sc = build_scope_tree(&c, 2);
        assert_eq!(sc.len(), 1);
        assert_eq!((sc[0].open_line, sc[0].close_line), (1, 2));
    }

    #[test]
    fn close_then_open_at_depth_zero_underflows() {
        // `} else {` at the top level (no enclosing scope): the close is
        // processed first and underflows at depth 0, even though opens == closes.
        // A naive net-delta counter would call this balanced; the close-before-
        // open rule catches it, matching match_scope_pairs' empty-stack check.
        let c = vec![s(&[
            LineProperty::ScopeClose,
            LineProperty::ScopeOpen,
            LineProperty::Declaration,
        ])];
        assert_eq!(scope_imbalance_site(&c, 1), Some(1));
    }
}
