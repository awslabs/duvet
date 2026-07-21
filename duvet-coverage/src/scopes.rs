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
/// Pass 1 (`match_scope_pairs_events`): walk the ordered scope-event stream with
///         a stack, pushing on each open and popping on each close, to recover
///         the `(open_line, close_line)` pairs (deduplicating identical-extent
///         scopes). A source-ordered stream is balanced by construction here —
///         the dispatcher runs the verified balance gate first.
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
pub fn build_scope_tree(events: &[ScopeEvent], file_length: u64) -> (scopes: Vec<Scope>)
    requires
        file_length < u64::MAX,
        // The classifier emits events in source (byte) order, so their lines
        // never decrease; and every brace sits on a real, bounded line. These
        // are discharged at the unverified classifier boundary (`coverage.rs`),
        // exactly like `file_length < u64::MAX`.
        forall|a: int, b: int|
            0 <= a <= b < events@.len() ==> (#[trigger] events@[a].line) <= (
            #[trigger] events@[b].line),
        forall|k: int|
            0 <= k < events@.len() ==> (#[trigger] events@[k].line) >= 1
                && events@[k].line < u64::MAX,
    ensures
        scopes_well_formed(scopes@),
        // The scope-bound preconditions of `is_annotation_executed` (#3/#4).
        // Stated here so the runtime trust boundary in `executed_status_for`
        // can rely on the *contract*, not on this function's implementation.
        forall|i: int| 0 <= i < scopes@.len() ==>
            (#[trigger] scopes@[i]).open_line >= 1
            && scopes@[i].close_line < u64::MAX,
{
    // Pass 1: collect (open_line, close_line) pairs from the faithful, ordered
    // scope-event stream (full multiplicity per compound line — PR #227).
    let pairs = match_scope_pairs_events(events);

    // A file with no scope delimiters has no pairs; its single whole-file scope
    // is legitimate (spec §1.5). NOTE: unlike the old set-based matcher, an
    // *imbalanced* stream can no longer reach here silently — the dispatcher
    // runs the verified `scope_imbalance_site` on this same event stream first
    // and escalates to `DefeatedClassification`, so a spurious whole-file
    // collapse from a dropped brace is no longer possible (Finding #3, PR #227).
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

/// Match a balanced, source-ordered scope-event stream into `(open_line,
/// close_line)` pairs, faithfully preserving the multiplicity a COMPOUND line
/// carries (PR #227). Consuming one event per brace (rather than a per-line set,
/// which can hold only one `ScopeOpen`/`ScopeClose` per line) is what lets
/// `} finally {}` (close, open, close) and `}}` (close, close) pair correctly.
///
/// Two distinct scopes that share BOTH open and close lines (e.g. `{{` on one
/// line, `}}` on another) yield identical `(open_line, close_line)` pairs. Such
/// duplicates are always emitted consecutively (all same-line opens are
/// contiguous in a source-ordered stream, hence adjacent on the stack, and are
/// popped by contiguous same-line closes), so a single "skip if equal to the
/// previous pair" test dedups them. Dedup is **sound** — the downstream model
/// keys scopes only on `(open_line, close_line)`, so identical-extent scopes are
/// indistinguishable to it — and **required**, since `scopes_well_formed` cannot
/// represent two distinct scopes of identical extent.
fn match_scope_pairs_events(events: &[ScopeEvent]) -> (pairs: Vec<(u64, u64)>)
    requires
        // Source order: events are byte-sorted, so their lines never decrease.
        forall|a: int, b: int|
            0 <= a <= b < events@.len() ==> (#[trigger] events@[a].line) <= (
            #[trigger] events@[b].line),
        // Every brace sits on a real, bounded line.
        forall|k: int|
            0 <= k < events@.len() ==> (#[trigger] events@[k].line) >= 1
                && events@[k].line < u64::MAX,
    ensures
        forall|i: int| 0 <= i < pairs@.len() ==> (#[trigger] pairs@[i]).0 >= 1,
        forall|i: int| 0 <= i < pairs@.len() ==> (#[trigger] pairs@[i]).0 <= pairs@[i].1,
        forall|i: int| 0 <= i < pairs@.len() ==> (#[trigger] pairs@[i]).1 < u64::MAX,
        forall|i: int, j: int|
            0 <= i < pairs@.len() && 0 <= j < pairs@.len() && i != j && (#[trigger] pairs@[i]).0
                < (#[trigger] pairs@[j]).1 && pairs@[j].0 < pairs@[i].1 ==> (pairs@[i].0
                <= pairs@[j].0 && pairs@[j].1 <= pairs@[i].1 && (pairs@[i].0 < pairs@[j].0
                || pairs@[j].1 < pairs@[i].1)) || (pairs@[j].0 <= pairs@[i].0 && pairs@[i].1
                <= pairs@[j].1 && (pairs@[j].0 < pairs@[i].0 || pairs@[i].1 < pairs@[j].1)),
{
    let mut stack: Vec<u64> = Vec::new();
    let mut pairs: Vec<(u64, u64)> = Vec::new();
    let mut i: usize = 0;

    while i < events.len()
        invariant
            0 <= i <= events.len(),
            // Nothing is on the stack until at least one event has been seen.
            i == 0 ==> stack@.len() == 0,
            // No pair can be emitted before the first close, i.e. before i > 0.
            i == 0 ==> pairs@.len() == 0,
            // Source order over the whole slice (carried for reasoning at pops).
            forall|a: int, b: int|
                0 <= a <= b < events@.len() ==> (#[trigger] events@[a].line) <= (
                #[trigger] events@[b].line),
            forall|k: int|
                0 <= k < events@.len() ==> (#[trigger] events@[k].line) >= 1
                    && events@[k].line < u64::MAX,
            // Stack entries are real lines, non-decreasing, and no later than the
            // most recently processed event line.
            forall|k: int| 0 <= k < stack@.len() ==> (#[trigger] stack@[k]) >= 1,
            forall|k: int, l: int|
                0 <= k <= l < stack@.len() ==> (#[trigger] stack@[k]) <= (#[trigger] stack@[l]),
            i > 0 ==> forall|k: int|
                0 <= k < stack@.len() ==> (#[trigger] stack@[k]) <= events@[i as int - 1].line,
            // Emitted pairs are well-shaped.
            forall|m: int| 0 <= m < pairs@.len() ==> (#[trigger] pairs@[m]).0 >= 1,
            forall|m: int| 0 <= m < pairs@.len() ==> (#[trigger] pairs@[m]).0 <= pairs@[m].1,
            forall|m: int|
                0 <= m < pairs@.len() ==> (#[trigger] pairs@[m]).1 < u64::MAX,
            // Emitted pairs are properly nested.
            forall|a: int, b: int|
                0 <= a < pairs@.len() && 0 <= b < pairs@.len() && a != b && (#[trigger] pairs@[a]).0
                    < (#[trigger] pairs@[b]).1 && pairs@[b].0 < pairs@[a].1 ==> (pairs@[a].0
                    <= pairs@[b].0 && pairs@[b].1 <= pairs@[a].1 && (pairs@[a].0 < pairs@[b].0
                    || pairs@[b].1 < pairs@[a].1)) || (pairs@[b].0 <= pairs@[a].0 && pairs@[a].1
                    <= pairs@[b].1 && (pairs@[b].0 < pairs@[a].0 || pairs@[a].1 < pairs@[b].1)),
            // Emitted pairs closed no later than the previous event's line.
            i > 0 ==> forall|m: int|
                0 <= m < pairs@.len() ==> (#[trigger] pairs@[m]).1 <= events@[i as int - 1].line,
            // Bridge: each emitted pair either closed at/before a still-open
            // scope's open line (sibling/earlier), or opened at/after it (nested
            // inside). Lets a newly-closed pair be shown to nest with every
            // existing pair the moment its opener is popped.
            forall|m: int, k: int|
                0 <= m < pairs@.len() && 0 <= k < stack@.len() ==> (#[trigger] pairs@[m]).1 <= (
                #[trigger] stack@[k]) || stack@[k] <= pairs@[m].0,
        decreases events.len() - i,
    {
        let ev = events[i];
        proof {
            // Sortedness at the current step: the previous event's line is no
            // later than this one, so every stack entry and every emitted pair
            // (<= events[i-1].line by the invariants) is also <= events[i].line.
            if i > 0 {
                assert(events@[i as int - 1].line <= events@[i as int].line);
            }
            assert forall|m: int| 0 <= m < pairs@.len() implies (#[trigger] pairs@[m]).1
                <= events@[i as int].line by {
                // pairs nonempty ==> i > 0, then pairs[m].1 <= events[i-1].line
                // <= events[i].line.
            }
        }
        if ev.opens {
            stack.push(ev.line);
        } else if stack.len() > 0 {
            let open = stack[stack.len() - 1];
            let ghost pre_pairs = pairs@;
            let ghost pre_stack = stack@;
            stack.pop();
            proof {
                // After the pop, every remaining stack entry is <= the popped top
                // `open` (the stack is non-decreasing and `open` was its last
                // element), so the bridge's second disjunct (stack[k] <= cand.0)
                // holds for the pair we are about to emit.
                assert(forall|k: int| 0 <= k < stack@.len() ==> stack@[k] == pre_stack[k]);
                assert(forall|k: int| 0 <= k < stack@.len() ==> (#[trigger] stack@[k]) <= open);
            }
            let cand = (open, ev.line);

            // Full-scan dedup: two blocks with identical extent (`{{` … `}}`)
            // would emit the same (open, close), which `scopes_well_formed`
            // cannot represent (two distinct, identical-extent scopes overlap yet
            // neither strictly contains the other). Skipping an already-present
            // candidate keeps pairs distinct — sound because the downstream model
            // keys scopes only on (open_line, close_line).
            let mut dup = false;
            let mut j: usize = 0;
            while j < pairs.len()
                invariant
                    0 <= j <= pairs@.len(),
                    dup == (exists|t: int| 0 <= t < j && pairs@[t] == cand),
                decreases pairs@.len() - j,
            {
                if pairs[j].0 == cand.0 && pairs[j].1 == cand.1 {
                    dup = true;
                }
                j = j + 1;
            }

            if !dup {
                proof {
                    // cand is distinct from every existing pair.
                    assert(forall|t: int| 0 <= t < pairs@.len() ==> pairs@[t] != cand);
                    // Every existing pair nests with cand = (open, ev.line):
                    //   bridge(a): pairs[a].1 <= open  OR  open <= pairs[a].0.
                    //   - If pairs[a].1 <= open = cand.0: they don't overlap
                    //     (overlap needs cand.0 < pairs[a].1), so nesting is vacuous.
                    //   - Else open <= pairs[a].0 = cand.0 <= pairs[a].0, and
                    //     pairs[a].1 <= events[i-1].line <= ev.line = cand.1, so cand
                    //     contains pairs[a]; distinctness gives the strict side.
                    assert(open <= events@[i as int - 1].line);
                    assert(events@[i as int - 1].line <= ev.line);
                    assert forall|a: int|
                        0 <= a < pairs@.len() && cand.0 < (#[trigger] pairs@[a]).1 && pairs@[a].0
                            < cand.1 implies pairs@[a].0 >= cand.0 && pairs@[a].1 <= cand.1
                        && (cand.0 < pairs@[a].0 || pairs@[a].1 < cand.1) by {
                        // bridge with k = old top (== open): first disjunct is
                        // ruled out by the overlap hypothesis cand.0 < pairs[a].1.
                        assert(pairs@[a].1 <= open || open <= pairs@[a].0);
                        assert(pairs@[a].1 <= events@[i as int - 1].line);
                    }
                }
                pairs.push(cand);
            }
        }
        i = i + 1;
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
// This detector characterises imbalance with a depth counter over the ordered
// scope-event stream: each `opens` event increments the depth and each close
// decrements it (a `ScopeClose` while
// the depth is 0 is a stray close (underflow), and a nonzero depth at EOF is a
// set of unclosed opens. Either is an imbalance. A stream with no delimiters at
// all has depth 0 throughout with no underflow — it is *balanced*, and its
// whole-file scope is legitimate, so it is deliberately NOT flagged. This is the
// distinction the naive "single scope spanning the file" heuristic cannot make.
// ---------------------------------------------------------------------------

/// Spec twin of the balance walk over the ordered scope-event stream. Folds the
/// first `n` events with a scope-depth counter, returning `(final_depth,
/// no_underflow)`. An `opens` event increments depth; a close at depth 0 records
/// underflow (and does not drive the depth negative), matching
/// `match_scope_pairs_events`' empty-stack rule. `no_underflow` is absorbing-false:
/// once a stray close is seen it stays false regardless of later events.
///
/// Faithful by construction: because each `ScopeEvent` is a *single* transition
/// in source order, a compound line contributes each of its braces as its own
/// event — the multiplicity the per-line `LineClass` set used to drop (PR #227)
/// is preserved.
pub open spec fn balance_upto(e: Seq<ScopeEvent>, n: int) -> (int, bool)
    decreases n,
{
    if n <= 0 {
        (0int, true)
    } else {
        let prev = balance_upto(e, n - 1);
        let d = prev.0;
        let ok = prev.1;
        let ev = e[n - 1];
        if ev.opens {
            (d + 1, ok)
        } else if d <= 0 {
            (d, false)
        } else {
            (d - 1, ok)
        }
    }
}

/// Spec: the whole scope-event stream is balanced — no stray close, and every
/// open matched by EOF.
pub open spec fn scope_stream_balanced_spec(e: Seq<ScopeEvent>) -> bool {
    let r = balance_upto(e, e.len() as int);
    r.1 && r.0 == 0
}

/// Detect an unbalanced scope-event stream and locate it.
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
pub fn scope_imbalance_site(events: &[ScopeEvent]) -> (result: Option<u64>)
    ensures
        (result is None) <==> scope_stream_balanced_spec(events@),
{
    let mut depth: u64 = 0;
    let mut underflow_line: Option<u64> = None;
    let mut last_open_line: u64 = 0;
    let mut i: usize = 0;

    while i < events.len()
        invariant
            0 <= i <= events.len(),
            // depth never exceeds the number of events processed, so `depth + 1`
            // cannot overflow u64 (i < events.len() <= usize::MAX <= u64::MAX).
            depth as int <= i as int,
            ({
                let r = balance_upto(events@, i as int);
                &&& depth as int == r.0
                &&& (underflow_line is None) <==> r.1
            }),
        decreases events.len() - i,
    {
        let ev = events[i];
        if ev.opens {
            last_open_line = ev.line;
            depth = depth + 1;
        } else if depth == 0 {
            if underflow_line.is_none() {
                underflow_line = Some(ev.line);
            }
        } else {
            depth = depth - 1;
        }
        i = i + 1;
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

    fn ev(line: u64, opens: bool) -> ScopeEvent {
        ScopeEvent { line, opens }
    }

    //= design/query/coverage-model-spec.md#scopes
    //= type=test
    //# A scope is a contiguous range of lines delimited by `ScopeOpen` and
    //# `ScopeClose` properties. Scopes nest.
    #[test]
    fn simple_method_in_class() {
        // class `{` L1, method `{` L2, `}` L4, `}` L5.
        let e = vec![ev(1, true), ev(2, true), ev(4, false), ev(5, false)];
        let sc = build_scope_tree(&e, 5);
        assert_eq!(sc.len(), 2);
        let outer = sc.iter().find(|s| s.open_line == 1).unwrap();
        let inner = sc.iter().find(|s| s.open_line == 2).unwrap();
        assert_eq!(outer.close_line, 5);
        assert_eq!(inner.close_line, 4);
    }

    #[test]
    fn sibling_methods() {
        let e = vec![
            ev(1, true),
            ev(2, true),
            ev(3, false),
            ev(4, true),
            ev(5, false),
            ev(6, false),
        ];
        let sc = build_scope_tree(&e, 6);
        assert_eq!(sc.len(), 3);
    }

    #[test]
    fn unclosed_open_falls_back_to_whole_file() {
        // A single unclosed `{`: no pair is emitted, so build_scope_tree yields
        // the whole-file scope. (The dispatcher's balance gate escalates this
        // case to DefeatedClassification before it ever reaches here.)
        let sc = build_scope_tree(&[ev(1, true)], 2);
        assert!(sc.len() >= 1);
        assert_eq!((sc[0].open_line, sc[0].close_line), (1, 2));
    }

    #[test]
    fn empty_file() {
        assert_eq!(build_scope_tree(&[], 0).len(), 0);
    }

    #[test]
    fn no_events_whole_file_scope() {
        // A file with statements but no braces: no events, one legitimate
        // whole-file scope.
        let sc = build_scope_tree(&[], 4);
        assert_eq!(sc.len(), 1);
        assert_eq!((sc[0].open_line, sc[0].close_line), (1, 4));
    }

    #[test]
    fn four_level_nesting() {
        let e = vec![
            ev(1, true),
            ev(2, true),
            ev(3, true),
            ev(4, true),
            ev(5, false),
            ev(6, false),
            ev(7, false),
            ev(8, false),
        ];
        let sc = build_scope_tree(&e, 8);
        assert_eq!(sc.len(), 4);
    }

    #[test]
    fn catch_creates_sibling_scopes() {
        // ... a `{}`-on-one-line sibling block (open+close on L4).
        let e = vec![
            ev(1, true),
            ev(2, true),
            ev(4, true),
            ev(4, false),
            ev(6, false),
            ev(7, false),
        ];
        let sc = build_scope_tree(&e, 7);
        assert_eq!(sc.len(), 3);
    }

    // --- PR #227: compound lines the per-line set could not represent ---

    #[test]
    fn compound_close_open_close_one_line_pairs_faithfully() {
        // `} finally {}` on line 18: close(try), open(finally), close(finally),
        // inside a method opened L8, class opened L6, both closed L23/L24.
        let e = vec![
            ev(6, true),
            ev(8, true),
            ev(10, true),  // try {
            ev(18, false), // } (closes try)
            ev(18, true),  // { (finally)
            ev(18, false), // } (closes finally)
            ev(23, false),
            ev(24, false),
        ];
        // Balanced: the second close on line 18 is preserved (it was the bug).
        assert_eq!(scope_imbalance_site(&e), None);
        let sc = build_scope_tree(&e, 24);
        // try (10,18), finally (18,18), method (8,23), class (6,24).
        assert_eq!(sc.len(), 4);
        assert!(sc.iter().any(|s| (s.open_line, s.close_line) == (10, 18)));
        assert!(sc.iter().any(|s| (s.open_line, s.close_line) == (18, 18)));
    }

    #[test]
    fn identical_extent_scopes_are_deduped() {
        // `{{` on line 5 … `}}` on line 10: two nested scopes of identical extent
        // (5,10). They are indistinguishable to the line-keyed model, so they
        // collapse to a single scope — sound, and required for well-formedness.
        let e = vec![ev(5, true), ev(5, true), ev(10, false), ev(10, false)];
        assert_eq!(scope_imbalance_site(&e), None);
        let sc = build_scope_tree(&e, 12);
        assert_eq!(sc.len(), 1);
        assert_eq!((sc[0].open_line, sc[0].close_line), (5, 10));
    }

    #[test]
    fn opens_on_one_line_closes_on_separate_lines_nest() {
        // `{{{{{` on line 5, then `}` on lines 7..=11. Five scopes SHARE an open
        // line but have DISTINCT close lines, so they are not identical-extent:
        // no dedup, fully faithful, nested via the close side (innermost first).
        let e = vec![
            ev(5, true),
            ev(5, true),
            ev(5, true),
            ev(5, true),
            ev(5, true),
            ev(7, false),
            ev(8, false),
            ev(9, false),
            ev(10, false),
            ev(11, false),
        ];
        assert_eq!(scope_imbalance_site(&e), None);
        let sc = build_scope_tree(&e, 11);
        assert_eq!(sc.len(), 5);
        assert!(sc.iter().all(|s| s.open_line == 5));
        let mut closes: Vec<u64> = sc.iter().map(|s| s.close_line).collect();
        closes.sort_unstable();
        assert_eq!(closes, vec![7, 8, 9, 10, 11]);
    }

    #[test]
    fn opens_on_separate_lines_closes_on_one_line_nest() {
        // The mirror image: `{` on lines 1..=5, then `}}}}}` on line 10. Five
        // scopes SHARE a close line but have DISTINCT open lines; no dedup,
        // fully faithful, nested via the open side (outermost opened first).
        let e = vec![
            ev(1, true),
            ev(2, true),
            ev(3, true),
            ev(4, true),
            ev(5, true),
            ev(10, false),
            ev(10, false),
            ev(10, false),
            ev(10, false),
            ev(10, false),
        ];
        assert_eq!(scope_imbalance_site(&e), None);
        let sc = build_scope_tree(&e, 10);
        assert_eq!(sc.len(), 5);
        assert!(sc.iter().all(|s| s.close_line == 10));
        let mut opens: Vec<u64> = sc.iter().map(|s| s.open_line).collect();
        opens.sort_unstable();
        assert_eq!(opens, vec![1, 2, 3, 4, 5]);
    }

    // --- scope_imbalance_site: empirical witnesses (spec §1.5 balance) ---
    //
    // The verified `ensures` proves `is None <==> balanced` against the
    // depth-counter spec. These witnesses ground that spec against reality AND
    // demonstrate the tie to `build_scope_tree`: where the detector says
    // balanced (None), the tree is faithful; where it says unbalanced (Some),
    // the tree collapses to the whole-file fallback.

    #[test]
    fn balanced_multiscope_is_not_flagged() {
        let e = vec![ev(1, true), ev(2, true), ev(4, false), ev(5, false)];
        assert_eq!(scope_imbalance_site(&e), None);
        assert_eq!(build_scope_tree(&e, 5).len(), 2);
    }

    #[test]
    fn genuine_whole_file_single_scope_is_not_flagged() {
        // class `{` L1 … `}` L3: ONE balanced scope. Output shape matches the
        // fallback, but it is balanced — the detector must NOT flag it.
        let e = vec![ev(1, true), ev(3, false)];
        assert_eq!(scope_imbalance_site(&e), None);
        let sc = build_scope_tree(&e, 3);
        assert_eq!(sc.len(), 1);
        assert_eq!((sc[0].open_line, sc[0].close_line), (1, 3));
    }

    #[test]
    fn no_delimiters_is_not_flagged() {
        // No scope events at all. Depth stays 0, never underflows: balanced.
        //= design/query/coverage-model-spec.md#property-11-scope-stream-balance-detection
        //= type=test
        //# A stream with no scope delimiters is balanced, and its whole-file scope is
        //# legitimate.
        assert_eq!(scope_imbalance_site(&[]), None);
    }

    #[test]
    fn parse_error_empty_stream_is_not_flagged() {
        // On a parse error the classifier emits an EMPTY event stream (the
        // escalation rides on `has_error`, not on this detector), which is
        // balanced — so the detector does not double-report it.
        assert_eq!(scope_imbalance_site(&[]), None);
    }

    #[test]
    fn stray_close_is_flagged_and_tree_collapses() {
        // A `}` with nothing open: stray close (depth underflow at the event).
        let e = vec![ev(1, false)];
        assert_eq!(scope_imbalance_site(&e), Some(1));
        // Tie: build_scope_tree ignores the stray close, so no pairs → whole-file.
        let sc = build_scope_tree(&e, 2);
        assert_eq!(sc.len(), 1);
        assert_eq!((sc[0].open_line, sc[0].close_line), (1, 2));
    }

    #[test]
    fn unclosed_open_is_flagged_and_tree_collapses() {
        let e = vec![ev(1, true)];
        let site = scope_imbalance_site(&e);
        assert!(
            site.is_some(),
            "unclosed open must be flagged, got {site:?}"
        );
        let sc = build_scope_tree(&e, 2);
        assert_eq!(sc.len(), 1);
        assert_eq!((sc[0].open_line, sc[0].close_line), (1, 2));
    }

    #[test]
    fn close_then_open_at_depth_zero_underflows() {
        // `} else {` at the top level (no enclosing scope): the leading close is
        // processed first and underflows at depth 0, even though opens == closes.
        // A naive net-delta counter would call this balanced; the ordered fold
        // catches it.
        let e = vec![ev(1, false), ev(1, true)];
        assert_eq!(scope_imbalance_site(&e), Some(1));
    }
}
