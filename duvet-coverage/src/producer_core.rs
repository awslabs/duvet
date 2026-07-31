// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Producer core: verified model of the parsed obligation graph
//! (design/witness/producer-core-spec.md).
//!
//! The model (producer-core-spec.md §1 #model): nodes are opaque ids
//! `0..n`, edges are in-range adjacency rows (`Vec<Vec<u64>>`), and
//! the closure of a root is its downward-reachable set. This module
//! proves Property P1 (closure fixpoint): the worklist closure marks
//! exactly the reachable set — reflexive, transitive to fixpoint,
//! and transparent (nothing outside the reachable set enters).
//!
//! Named glue assumptions (trusted base, NOT verified here) are
//! specified in producer-core-spec.md §1: **PG1** (graph
//! translation — the adapter constructs the model faithfully from
//! the parsed structure: injective node ids, exactly the resolving
//! edges; golden-corpus-checked), **PG2** (call obligation — the
//! producer computes every closure by calling this layer), and
//! **PG3** (project view). The adapter side lives in
//! `duvet/src/query/parsers/verus_sst/closure.rs`.
//!
//! The `requires` on the exec functions (graph well-formedness,
//! root in range) are the adapter's obligation to establish at the
//! trust boundary, exactly as the engine adapter establishes the
//! witness layer's `requires` before calling it.

use vstd::prelude::*;

verus! {

// ---------------------------------------------------------------------------
// Spec vocabulary (producer-core-spec.md #model, #property-p1-closure-fixpoint)
// ---------------------------------------------------------------------------

/// Graph well-formedness: every edge lands on a node. The adapter
/// establishes this by construction (PG1): symbolic references that
/// resolve to no obligation node are not part of the obligation
/// graph (spec.md §5.4) and never become model edges.
//= design/witness/producer-core-spec.md#model
//= type=implementation
//# **Edges** are the symbolic references that resolve to a block in
//# the artifact; references to names with no block are not part of
//# the obligation graph ([spec §5.4](spec.md#closure)) and MUST NOT
//# appear as model edges.
pub open spec fn graph_wf(g: Seq<Vec<u64>>) -> bool {
    forall|m: int, j: int|
        0 <= m < g.len() && 0 <= j < g[m]@.len() ==> (#[trigger] g[m]@[j]) < g.len()
}

/// Spec: `t` is reachable from `root` in at most `k` edge hops.
/// Reflexive at every hop count: zero hops reach the root itself.
pub open spec fn reachable_within(g: Seq<Vec<u64>>, root: u64, t: u64, k: nat) -> bool
    decreases k,
{
    if t == root {
        true
    } else if k == 0 {
        false
    } else {
        exists|m: u64|
            m < g.len() && reachable_within(g, root, m, (k - 1) as nat) && #[trigger] g[m
                as int]@.contains(t)
    }
}

/// Spec: the downward-reachable set — some hop count suffices.
/// This is the definitional right-hand side of Property P1.
//= design/witness/producer-core-spec.md#property-p1-closure-fixpoint
//= type=implementation
//# where `reachable` is the reflexive-transitive reach along reference
//# edges.
pub open spec fn reachable(g: Seq<Vec<u64>>, root: u64, t: u64) -> bool {
    exists|k: nat| reachable_within(g, root, t, k)
}

// ---------------------------------------------------------------------------
// Counting marked nodes (termination measure for the worklist)
// ---------------------------------------------------------------------------

/// Spec: number of `true` entries in a boolean sequence.
pub open spec fn count_true(s: Seq<bool>) -> int
    decreases s.len(),
{
    if s.len() == 0 {
        0
    } else {
        count_true(s.drop_last()) + if s.last() {
            1int
        } else {
            0int
        }
    }
}

proof fn lemma_count_true_bound(s: Seq<bool>)
    ensures
        0 <= count_true(s) <= s.len(),
    decreases s.len(),
{
    if s.len() > 0 {
        lemma_count_true_bound(s.drop_last());
    }
}

proof fn lemma_count_true_update(s: Seq<bool>, i: int)
    requires
        0 <= i < s.len(),
        !s[i],
    ensures
        count_true(s.update(i, true)) == count_true(s) + 1,
    decreases s.len(),
{
    let u = s.update(i, true);
    if i == s.len() - 1 {
        assert(u.drop_last() =~= s.drop_last());
    } else {
        assert(u.drop_last() =~= s.drop_last().update(i, true));
        lemma_count_true_update(s.drop_last(), i);
    }
}

// ---------------------------------------------------------------------------
// Reachability lemmas
// ---------------------------------------------------------------------------

/// One edge extends reachability by one hop.
proof fn lemma_reachable_step(g: Seq<Vec<u64>>, root: u64, m: u64, t: u64)
    requires
        reachable(g, root, m),
        m < g.len(),
        g[m as int]@.contains(t),
    ensures
        reachable(g, root, t),
{
    if t == root {
        assert(reachable_within(g, root, t, 0));
    } else {
        reveal_with_fuel(reachable_within, 2);
        let k = choose|k: nat| reachable_within(g, root, m, k);
        let k1: nat = k + 1;
        assert(((k1 - 1) as nat) == k);
        assert(reachable_within(g, root, m, (k1 - 1) as nat));
        assert(reachable_within(g, root, t, k1));
    }
}

/// The closed-set principle behind P1's completeness direction: a
/// set that contains the root and is closed under the edge relation
/// contains every reachable node. Instantiated with the worklist's
/// final marking (root marked, no frontier left).
proof fn lemma_closed_set_contains_reachable(
    g: Seq<Vec<u64>>,
    root: u64,
    s: Seq<bool>,
    t: u64,
    k: nat,
)
    requires
        graph_wf(g),
        s.len() == g.len(),
        root < g.len(),
        s[root as int],
        forall|m: u64, j: int|
            m < g.len() && s[m as int] && 0 <= j < g[m as int]@.len() ==> s[(
            #[trigger] g[m as int]@[j]) as int],
        reachable_within(g, root, t, k),
        t < g.len(),
    ensures
        s[t as int],
    decreases k,
{
    if t != root {
        reveal_with_fuel(reachable_within, 2);
        assert(k > 0);
        let m = choose|m: u64|
            m < g.len() && reachable_within(g, root, m, (k - 1) as nat) && #[trigger] g[m
                as int]@.contains(t);
        assert((m as int) < g.len());
        lemma_closed_set_contains_reachable(g, root, s, m, (k - 1) as nat);
        let j = choose|j: int| 0 <= j < g[m as int]@.len() && g[m as int]@[j] == t;
        assert(s[(g[m as int]@[j]) as int]);
    }
}

// ---------------------------------------------------------------------------
// Property P1: Closure Fixpoint
// ---------------------------------------------------------------------------

/// Property P1: the worklist closure marks exactly the
/// downward-reachable set of the obligation graph.
///
/// Proof shape: the loop maintains soundness (every marked node and
/// every worklist entry is reachable — the "no more" / transparency
/// direction) and a frontier invariant (every marked node's
/// successors are marked or on the worklist). When the worklist
/// drains, the marking is a closed set containing the root, and the
/// closed-set principle delivers completeness (the "no less" /
/// fixpoint direction).
//
// Placement: the annotation block is the LAST comment block before
// the fn header so its resolved target is the header (this file has
// no language classifier; see spec §1.1's placement note).
//= design/witness/producer-core-spec.md#property-p1-closure-fixpoint
//= type=test
//# The implementation MUST prove that the computed closure of a
//# discharge unit's root is exactly the downward-reachable set of the
//# obligation graph — the least fixpoint of the edge relation
//# containing the root:
pub fn closure_reached(g: &Vec<Vec<u64>>, root: u64) -> (reached: Vec<bool>)
    requires
        graph_wf(g@),
        (root as int) < g@.len(),
    ensures
        reached@.len() == g@.len(),
        forall|t: u64|
            (t as int) < g@.len() ==> (#[trigger] reached@[t as int] <==> reachable(
                g@,
                root,
                t,
            )),
{
    //= design/witness/producer-core-spec.md#property-p1-closure-fixpoint
    //= type=implementation
    //# The implementation MUST prove that the computed closure of a
    //# discharge unit's root is exactly the downward-reachable set of the
    //# obligation graph — the least fixpoint of the edge relation
    //# containing the root:
    let n = g.len();
    let mut reached: Vec<bool> = Vec::new();
    let mut i: usize = 0;
    while i < n
        invariant
            i <= n,
            reached@.len() == i,
            forall|t: int| 0 <= t < i ==> !(#[trigger] reached@[t]),
        decreases n - i,
    {
        reached.push(false);
        i = i + 1;
    }

    let mut work: Vec<u64> = Vec::new();
    work.push(root);
    proof {
        assert(reachable_within(g@, root, root, 0));
        assert(work@[0] == root);
        assert(work@.contains(root));
    }

    while work.len() > 0
        invariant
            graph_wf(g@),
            (root as int) < g@.len(),
            n == g@.len(),
            reached@.len() == g@.len(),
            // Every worklist entry is in range and reachable.
            forall|i: int|
                0 <= i < work@.len() ==> ((#[trigger] work@[i]) as int) < g@.len() && reachable(
                    g@,
                    root,
                    work@[i],
                ),
            // Soundness (transparency): every marked node is reachable.
            forall|t: u64|
                (t as int) < g@.len() && #[trigger] reached@[t as int] ==> reachable(g@, root, t),
            // The root is marked or pending.
            reached@[root as int] || work@.contains(root),
            // Frontier: every marked node's successors are marked or pending.
            forall|m: u64, j: int|
                (m as int) < g@.len() && reached@[m as int] && 0 <= j < g@[m as int]@.len()
                    ==> reached@[(#[trigger] g@[m as int]@[j]) as int] || work@.contains(
                    g@[m as int]@[j],
                ),
        decreases g@.len() - count_true(reached@), work@.len(),
    {
        let ghost prev_work = work@;
        let x = work.pop().unwrap();
        proof {
            // Popping only removed (one occurrence of) x: everything
            // else that was pending is still pending.
            assert forall|v: u64| prev_work.contains(v) && v != x implies work@.contains(v) by {
                let i = choose|i: int| 0 <= i < prev_work.len() && prev_work[i] == v;
                assert(work@[i] == v);
            }
            lemma_count_true_bound(reached@);
        }
        let xi = x as usize;
        if !reached[xi] {
            let ghost before = reached@;
            reached.set(xi, true);
            proof {
                lemma_count_true_update(before, xi as int);
            }
            let mut j: usize = 0;
            while j < g[xi].len()
                invariant
                    graph_wf(g@),
                    (root as int) < g@.len(),
                    n == g@.len(),
                    reached@.len() == g@.len(),
                    (x as int) < g@.len(),
                    xi == x as usize,
                    j <= g@[xi as int]@.len(),
                    reached@[xi as int],
                    count_true(reached@) == count_true(before) + 1,
                    !before[xi as int],
                    reached@ == before.update(xi as int, true),
                    forall|i: int|
                        0 <= i < work@.len() ==> ((#[trigger] work@[i]) as int) < g@.len()
                            && reachable(g@, root, work@[i]),
                    forall|t: u64|
                        (t as int) < g@.len() && #[trigger] reached@[t as int] ==> reachable(
                            g@,
                            root,
                            t,
                        ),
                    reached@[root as int] || work@.contains(root),
                    // Frontier for every marked node other than x.
                    forall|m: u64, jj: int|
                        (m as int) < g@.len() && reached@[m as int] && m != x && 0 <= jj < g@[m
                            as int]@.len() ==> reached@[(#[trigger] g@[m as int]@[jj]) as int]
                            || work@.contains(g@[m as int]@[jj]),
                    // Partial frontier for x: successors pushed so far.
                    forall|jj: int|
                        0 <= jj < j ==> reached@[(#[trigger] g@[xi as int]@[jj]) as int]
                            || work@.contains(g@[xi as int]@[jj]),
                decreases g@[xi as int]@.len() - j,
            {
                let ghost pre_push = work@;
                let e = g[xi][j];
                work.push(e);
                proof {
                    lemma_reachable_step(g@, root, x, e);
                    assert forall|v: u64| pre_push.contains(v) implies work@.contains(v) by {
                        let i = choose|i: int| 0 <= i < pre_push.len() && pre_push[i] == v;
                        assert(work@[i] == v);
                    }
                    assert(work@[work@.len() - 1] == e);
                }
                j = j + 1;
            }
        }
        proof {
            // Non-negativity of the termination measure's first
            // component after (possibly) marking a node.
            lemma_count_true_bound(reached@);
        }
    }

    proof {
        // The worklist is empty: the marking is closed and contains
        // the root, so it contains every reachable node.
        assert forall|t: u64| (t as int) < g@.len() && reachable(g@, root, t) implies reached@[t
            as int] by {
            let k = choose|k: nat| reachable_within(g@, root, t, k);
            lemma_closed_set_contains_reachable(g@, root, reached@, t, k);
        }
    }
    reached
}

// ---------------------------------------------------------------------------
// Property P2: Most-Specific-Wins Rooting
// ---------------------------------------------------------------------------

/// A discharge-unit candidate in the verified model: its span in
/// model coordinates (opaque file id — PG1/PG3) plus its
/// specificity level. Two levels exist (spec.md §5.3): clause-kind
/// units (ensures clauses, loop invariants, proof asserts) are
/// finer; obligation extents are the fallback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitSpan {
    pub file_id: u64,
    pub start_line: u32,
    pub end_line: u32,
    pub is_clause: bool,
}

/// Named precondition: every unit's span is a non-empty line range.
/// The adapter's obligation (the artifact's spans satisfy it by
/// parsing; the golden corpus is the check).
pub open spec fn units_wf(units: Seq<UnitSpan>) -> bool {
    forall|i: int| 0 <= i < units.len() ==> (#[trigger] units[i]).start_line <= units[i].end_line
}

/// Spec: the unit's span contains the position (inclusive range,
/// same file).
pub open spec fn unit_contains(u: UnitSpan, file_id: u64, line: u32) -> bool {
    u.file_id == file_id && u.start_line <= line <= u.end_line
}

/// Spec: extent size in lines.
pub open spec fn unit_line_count(u: UnitSpan) -> int {
    u.end_line - u.start_line + 1
}

/// Spec: some clause-kind unit contains the position — the finest
/// populated specificity level is then the clause level, and extent
/// units never apply.
pub open spec fn clause_level_populated(units: Seq<UnitSpan>, file_id: u64, line: u32) -> bool {
    exists|j: int|
        0 <= j < units.len() && (#[trigger] units[j]).is_clause && unit_contains(
            units[j],
            file_id,
            line,
        )
}

/// Spec: unit `i` is selected for the position — it contains the
/// position, sits at the finest populated specificity level, and no
/// containing unit at that level has a strictly smaller extent.
/// This is the definitional right-hand side of Property P2.
//= design/witness/producer-core-spec.md#property-p2-most-specific-wins
//= type=implementation
//# Consequences the proof MUST deliver: every selected unit contains
//# the position; no containing unit at a strictly finer specificity
//# level exists when an extent-level unit is selected; and ties at the
//# winning level and minimal extent are all selected — never chosen
//# among (decisions.md, [Decision 12](decisions.md#decision-12)).
pub open spec fn selected(units: Seq<UnitSpan>, file_id: u64, line: u32, i: int) -> bool {
    &&& 0 <= i < units.len()
    &&& unit_contains(units[i], file_id, line)
    &&& (units[i].is_clause <==> clause_level_populated(units, file_id, line))
    &&& forall|j: int|
        0 <= j < units.len() && unit_contains(#[trigger] units[j], file_id, line)
            && units[j].is_clause == units[i].is_clause ==> unit_line_count(units[i])
            <= unit_line_count(units[j])
}

/// Property P2: the selection mask marks exactly the most-specific
/// containing units.
///
/// Proof shape: pass 1 decides the winning specificity level
/// (clause iff any clause-kind unit contains the position); pass 2
/// computes the minimal extent among containing units at that
/// level; pass 3 marks exactly the containing units at the winning
/// level achieving the minimum. A unit is `selected` iff it
/// survives all three, and minimality-as-equality-with-the-minimum
/// coincides with minimality-against-all because pass 2's minimum
/// is both achieved and a lower bound.
//
// Placement: the annotation block is the LAST comment block before
// the fn header so its resolved target is the header.
//= design/witness/producer-core-spec.md#property-p2-most-specific-wins
//= type=test
//# The implementation MUST prove that the units selected for a
//# position are exactly the minimal-extent containing units at the
//# finest populated specificity level
pub fn select_units(units: &Vec<UnitSpan>, file_id: u64, line: u32) -> (sel: Vec<bool>)
    requires
        units_wf(units@),
    ensures
        sel@.len() == units@.len(),
        forall|i: int|
            0 <= i < units@.len() ==> (#[trigger] sel@[i] <==> selected(units@, file_id, line, i)),
{
    //= design/witness/producer-core-spec.md#property-p2-most-specific-wins
    //= type=implementation
    //# The implementation MUST prove that the units selected for a
    //# position are exactly the minimal-extent containing units at the
    //# finest populated specificity level
    let n = units.len();

    // Pass 1: the winning specificity level.
    let mut clause_present = false;
    let mut i: usize = 0;
    while i < n
        invariant
            i <= n,
            n == units@.len(),
            units_wf(units@),
            clause_present <==> exists|j: int|
                0 <= j < i && (#[trigger] units@[j]).is_clause && unit_contains(
                    units@[j],
                    file_id,
                    line,
                ),
        decreases n - i,
    {
        let u = &units[i];
        if u.is_clause && u.file_id == file_id && u.start_line <= line && line <= u.end_line {
            clause_present = true;
        }
        i = i + 1;
    }

    // Pass 2: the minimal extent among containing units at the
    // winning level. `found` tracks whether any such unit exists.
    let mut found = false;
    let mut min_count: u64 = 0;
    let mut i: usize = 0;
    while i < n
        invariant
            i <= n,
            n == units@.len(),
            units_wf(units@),
            clause_present <==> clause_level_populated(units@, file_id, line),
            found <==> exists|j: int|
                0 <= j < i && unit_contains(#[trigger] units@[j], file_id, line)
                    && units@[j].is_clause == clause_present,
            found ==> exists|j: int|
                0 <= j < i && unit_contains(#[trigger] units@[j], file_id, line)
                    && units@[j].is_clause == clause_present && unit_line_count(units@[j])
                    == min_count,
            found ==> forall|j: int|
                0 <= j < i && unit_contains(#[trigger] units@[j], file_id, line)
                    && units@[j].is_clause == clause_present ==> min_count <= unit_line_count(
                    units@[j],
                ),
        decreases n - i,
    {
        let u = &units[i];
        if u.file_id == file_id && u.start_line <= line && line <= u.end_line && u.is_clause
            == clause_present {
            let count = (u.end_line - u.start_line) as u64 + 1;
            if !found || count < min_count {
                min_count = count;
                found = true;
            }
        }
        i = i + 1;
    }

    // Pass 3: mark exactly the containing units at the winning
    // level whose extent achieves the minimum.
    let mut sel: Vec<bool> = Vec::new();
    let mut i: usize = 0;
    while i < n
        invariant
            i <= n,
            n == units@.len(),
            units_wf(units@),
            sel@.len() == i,
            clause_present <==> clause_level_populated(units@, file_id, line),
            found <==> exists|j: int|
                0 <= j < n && unit_contains(#[trigger] units@[j], file_id, line)
                    && units@[j].is_clause == clause_present,
            found ==> exists|j: int|
                0 <= j < n && unit_contains(#[trigger] units@[j], file_id, line)
                    && units@[j].is_clause == clause_present && unit_line_count(units@[j])
                    == min_count,
            found ==> forall|j: int|
                0 <= j < n && unit_contains(#[trigger] units@[j], file_id, line)
                    && units@[j].is_clause == clause_present ==> min_count <= unit_line_count(
                    units@[j],
                ),
            forall|k: int|
                0 <= k < i ==> (#[trigger] sel@[k] <==> selected(units@, file_id, line, k)),
        decreases n - i,
    {
        let u = &units[i];
        let mark = if u.file_id == file_id && u.start_line <= line && line <= u.end_line
            && u.is_clause == clause_present {
            let count = (u.end_line - u.start_line) as u64 + 1;
            count == min_count
        } else {
            false
        };
        proof {
            // Minimality-as-equality coincides with the spec's
            // minimality-against-all: the minimum is a lower bound
            // (pass 2's invariant) and unit i achieving it makes it
            // minimal; conversely a minimal containing unit at the
            // winning level achieves the minimum because the
            // minimum is achieved by some containing unit.
            if mark {
                assert(selected(units@, file_id, line, i as int));
            } else if unit_contains(units@[i as int], file_id, line) && units@[i as int].is_clause
                == clause_present {
                // Containing at the winning level, but not achieving
                // the minimum: the achieving witness is strictly
                // smaller, so unit i is not minimal.
                let j0 = choose|j: int|
                    0 <= j < n && unit_contains(#[trigger] units@[j], file_id, line)
                        && units@[j].is_clause == clause_present && unit_line_count(units@[j])
                        == min_count;
                assert(unit_line_count(units@[j0]) < unit_line_count(units@[i as int]));
                assert(!selected(units@, file_id, line, i as int));
            } else {
                assert(!selected(units@, file_id, line, i as int));
            }
        }
        sel.push(mark);
        i = i + 1;
    }
    sel
}

// ---------------------------------------------------------------------------
// Properties P3 and P4: Witness Assembly and Filter Soundness
// ---------------------------------------------------------------------------

/// Spec: `file_id` is a project file under the project view
/// (PG3). Out-of-range ids are non-project by definition.
pub open spec fn project_file(project: Seq<bool>, file_id: u64) -> bool {
    (file_id as int) < project.len() && project[file_id as int]
}

/// Spec: the closure of `root` consulted line `(f, l)` — some
/// reachable node's spans contain it. Deliberately project-free:
/// this is the P4 sentence's unfiltered traversal, in the model's
/// vocabulary.
//= design/witness/producer-core-spec.md#property-p4-filter-soundness
//= type=implementation
//# Edge traversal MUST NOT be filtered — `closure_reached` takes no
//# project view, so the reached set is project-independent by the
//# model's construction, and only the span projection is restricted.
pub open spec fn consulted_line(
    g: Seq<Vec<u64>>,
    spans: Seq<Vec<(u64, u32)>>,
    root: u64,
    f: u64,
    l: u32,
) -> bool {
    exists|t: u64|
        (t as int) < g.len() && reachable(g, root, t) && #[trigger] spans[t as int]@.contains(
            (f, l),
        )
}

/// Spec: line `(f, l)` is in the assembled witness — consulted by
/// the closure and in a project file. This is the definitional
/// right-hand side of Property P3.
//= design/witness/producer-core-spec.md#property-p3-witness-assembly
//= type=implementation
//# The implementation MUST prove that a witness's line set equals the
//# union of the closure's per-file spans restricted to project files:
pub open spec fn assembled_line(
    g: Seq<Vec<u64>>,
    spans: Seq<Vec<(u64, u32)>>,
    root: u64,
    project: Seq<bool>,
    f: u64,
    l: u32,
) -> bool {
    project_file(project, f) && consulted_line(g, spans, root, f, l)
}

/// Push extends membership by exactly the pushed element.
proof fn lemma_push_contains<T>(s: Seq<T>, x: T)
    ensures
        forall|v: T| s.push(x).contains(v) <==> s.contains(v) || v == x,
{
    assert forall|v: T| s.push(x).contains(v) <==> s.contains(v) || v == x by {
        if s.contains(v) {
            let i = choose|i: int| 0 <= i < s.len() && s[i] == v;
            assert(s.push(x)[i] == v);
        }
        if v == x {
            assert(s.push(x)[s.len() as int] == v);
        }
        if s.push(x).contains(v) {
            let i = choose|i: int| 0 <= i < s.push(x).len() && s.push(x)[i] == v;
            if i < s.len() {
                assert(s[i] == v);
            }
        }
    }
}

/// Property P3: the assembled line set is exactly the union of the
/// closure's spans restricted to project files.
///
/// Proof shape: the reached mask is the verified closure (P1's
/// iff-ensures ties it to `reachable`); the two nested loops then
/// maintain a membership iff over the emitted pairs — everything
/// emitted was consulted-and-project, and every consulted project
/// line of a visited node was emitted.
//
// Placement: the annotation block is the LAST comment block before
// the fn header so its resolved target is the header.
//= design/witness/producer-core-spec.md#property-p3-witness-assembly
//= type=test
//# The implementation MUST prove that a witness's line set equals the
//# union of the closure's per-file spans restricted to project files:
pub fn assemble_witness_lines(
    g: &Vec<Vec<u64>>,
    spans: &Vec<Vec<(u64, u32)>>,
    root: u64,
    project: &Vec<bool>,
) -> (out: Vec<(u64, u32)>)
    requires
        graph_wf(g@),
        (root as int) < g@.len(),
        spans@.len() == g@.len(),
    ensures
        forall|f: u64, l: u32|
            #[trigger] out@.contains((f, l)) <==> assembled_line(g@, spans@, root, project@, f, l),
{
    let reached = closure_reached(g, root);
    let n = spans.len();
    let mut out: Vec<(u64, u32)> = Vec::new();
    let mut t: usize = 0;
    while t < n
        invariant
            t <= n,
            n == spans@.len(),
            spans@.len() == g@.len(),
            graph_wf(g@),
            (root as int) < g@.len(),
            reached@.len() == g@.len(),
            forall|u: u64|
                (u as int) < g@.len() ==> (#[trigger] reached@[u as int] <==> reachable(
                    g@,
                    root,
                    u,
                )),
            forall|f: u64, l: u32|
                #[trigger] out@.contains((f, l)) <==> (project_file(project@, f) && exists|u: int|
                    0 <= u < t && reached@[u] && #[trigger] spans@[u]@.contains((f, l))),
        decreases n - t,
    {
        if reached[t] {
            let row = &spans[t];
            let mut j: usize = 0;
            while j < row.len()
                invariant
                    t < n,
                    n == spans@.len(),
                    spans@.len() == g@.len(),
                    graph_wf(g@),
                    (root as int) < g@.len(),
                    reached@.len() == g@.len(),
                    j <= spans@[t as int]@.len(),
                    row == &spans[t as int],
                    reached@[t as int],
                    forall|u: u64|
                        (u as int) < g@.len() ==> (#[trigger] reached@[u as int] <==> reachable(
                            g@,
                            root,
                            u,
                        )),
                    forall|f: u64, l: u32|
                        #[trigger] out@.contains((f, l)) <==> (project_file(project@, f) && ((
                        exists|u: int|
                            0 <= u < t && reached@[u] && #[trigger] spans@[u]@.contains((f, l)))
                            || (exists|jj: int|
                            0 <= jj < j && spans@[t as int]@[jj] == (f, l)))),
                decreases spans@[t as int]@.len() - j,
            {
                let (f, l) = row[j];
                let fi = f as usize;
                let is_project = fi < project.len() && project[fi];
                let ghost old_out = out@;
                if is_project {
                    out.push((f, l));
                }
                proof {
                    assert(is_project == project_file(project@, f));
                    lemma_push_contains(old_out, (f, l));
                    assert(spans@[t as int]@[j as int] == (f, l));
                    // Re-establish the membership iff at j+1 with
                    // explicit witness transport in both directions.
                    assert forall|f2: u64, l2: u32|
                        #[trigger] out@.contains((f2, l2)) <==> (project_file(project@, f2) && ((
                        exists|u: int|
                            0 <= u < t && reached@[u] && #[trigger] spans@[u]@.contains((f2, l2)))
                            || (exists|jj: int|
                            0 <= jj < j + 1 && spans@[t as int]@[jj] == (f2, l2)))) by {
                        if out@.contains((f2, l2)) {
                            if old_out.contains((f2, l2)) {
                                if exists|jj: int|
                                    0 <= jj < j && spans@[t as int]@[jj] == (f2, l2) {
                                    let jj = choose|jj: int|
                                        0 <= jj < j && spans@[t as int]@[jj] == (f2, l2);
                                    assert(0 <= jj < j + 1 && spans@[t as int]@[jj] == (f2, l2));
                                }
                            } else {
                                // Freshly pushed: it is row[j], and
                                // it passed the project check.
                                assert((f2, l2) == (f, l));
                                assert(spans@[t as int]@[j as int] == (f2, l2));
                            }
                        }
                        if project_file(project@, f2) && ((exists|u: int|
                            0 <= u < t && reached@[u] && #[trigger] spans@[u]@.contains((f2, l2)))
                            || (exists|jj: int|
                            0 <= jj < j + 1 && spans@[t as int]@[jj] == (f2, l2))) {
                            if exists|jj: int|
                                0 <= jj < j + 1 && spans@[t as int]@[jj] == (f2, l2) {
                                let jj = choose|jj: int|
                                    0 <= jj < j + 1 && spans@[t as int]@[jj] == (f2, l2);
                                if jj == j {
                                    // Row[j] itself: pushed this
                                    // iteration (project holds).
                                    assert((f2, l2) == (f, l));
                                    assert(project_file(project@, f));
                                    assert(is_project);
                                    assert(out@ == old_out.push((f, l)));
                                    assert(old_out.push((f, l))[old_out.len() as int] == (f, l));
                                    assert(out@[old_out.len() as int] == (f2, l2));
                                    assert(out@.contains((f2, l2)));
                                } else {
                                    assert(0 <= jj < j && spans@[t as int]@[jj] == (f2, l2));
                                    assert(old_out.contains((f2, l2)));
                                }
                            } else {
                                assert(old_out.contains((f2, l2)));
                            }
                        }
                    }
                }
                j = j + 1;
            }
            proof {
                // The row is exhausted: fold node t into the outer
                // accumulation (exists over the full row is
                // Seq::contains; index t is a reached witness).
                assert forall|f2: u64, l2: u32|
                    #[trigger] out@.contains((f2, l2)) <==> (project_file(project@, f2) && exists|
                        u: int,
                    |
                        0 <= u < t + 1 && reached@[u] && #[trigger] spans@[u]@.contains(
                            (f2, l2),
                        )) by {
                    if out@.contains((f2, l2)) {
                        if exists|jj: int|
                            0 <= jj < spans@[t as int]@.len() && spans@[t as int]@[jj] == (
                            f2,
                            l2,
                            ) {
                            assert(spans@[t as int]@.contains((f2, l2)));
                            assert(0 <= (t as int) < t + 1 && reached@[t as int]
                                && spans@[t as int]@.contains((f2, l2)));
                        }
                    }
                    if project_file(project@, f2) && exists|u: int|
                        0 <= u < t + 1 && reached@[u] && #[trigger] spans@[u]@.contains((f2, l2)) {
                        let u = choose|u: int|
                            0 <= u < t + 1 && reached@[u] && #[trigger] spans@[u]@.contains(
                                (f2, l2),
                            );
                        if u == t {
                            assert(j == spans@[t as int]@.len());
                            assert(spans@[t as int]@.contains((f2, l2)));
                            let jj = choose|jj: int|
                                0 <= jj < spans@[t as int]@.len() && spans@[t as int]@[jj] == (
                                f2,
                                l2,
                                );
                            assert(0 <= jj < j && spans@[t as int]@[jj] == (f2, l2));
                            assert(out@.contains((f2, l2)));
                        } else {
                            assert(0 <= u < t && reached@[u] && spans@[u]@.contains((f2, l2)));
                        }
                    }
                }
            }
        } else {
            proof {
                // Node t is not reached: extending the index bound
                // adds nothing.
                assert forall|f2: u64, l2: u32|
                    #[trigger] out@.contains((f2, l2)) <==> (project_file(project@, f2) && exists|
                        u: int,
                    |
                        0 <= u < t + 1 && reached@[u] && #[trigger] spans@[u]@.contains(
                            (f2, l2),
                        )) by {
                    if project_file(project@, f2) && exists|u: int|
                        0 <= u < t + 1 && reached@[u] && #[trigger] spans@[u]@.contains((f2, l2)) {
                        let u = choose|u: int|
                            0 <= u < t + 1 && reached@[u] && #[trigger] spans@[u]@.contains(
                                (f2, l2),
                            );
                        assert(u != t);
                    }
                }
            }
        }
        t = t + 1;
    }
    proof {
        // The per-index accumulation coincides with P3's spec: a
        // consulted witness node is a reached index and vice versa
        // (P1's iff), and index/id quantifiers transport.
        assert forall|f: u64, l: u32|
            #[trigger] out@.contains((f, l)) <==> assembled_line(
                g@,
                spans@,
                root,
                project@,
                f,
                l,
            ) by {
            if out@.contains((f, l)) {
                let u = choose|u: int|
                    0 <= u < n && reached@[u] && #[trigger] spans@[u]@.contains((f, l));
                assert((u as u64) as int == u);
                assert(reachable(g@, root, u as u64));
                assert(spans@[(u as u64) as int]@.contains((f, l)));
            }
            if assembled_line(g@, spans@, root, project@, f, l) {
                let u = choose|u: u64|
                    (u as int) < g@.len() && reachable(g@, root, u)
                        && #[trigger] spans@[u as int]@.contains((f, l));
                assert(reached@[u as int]);
            }
        }
    }
    out
}

/// Named precondition for P4: every span's file id is within the
/// project view's domain (the adapter builds the file table, so
/// every interned id is in range by construction).
pub open spec fn spans_files_in_range(spans: Seq<Vec<(u64, u32)>>, nfiles: int) -> bool {
    forall|t: int, j: int|
        0 <= t < spans.len() && 0 <= j < spans[t]@.len() ==> ((#[trigger] spans[t]@[j]).0 as int)
            < nfiles
}

/// Property P4: project filtering is a view, not a truncation —
/// the assembly under a project view equals the unfiltered assembly
/// (the all-true view over the file table) intersected with the
/// project files. Filtering removes only non-project lines.
//
// Placement: the annotation block is the LAST comment block before
// the fn header so its resolved target is the header.
//= design/witness/producer-core-spec.md#property-p4-filter-soundness
//= type=test
//# The implementation MUST prove that project filtering is a view,
//# not a truncation (decisions.md,
//# [Decision 7](decisions.md#decision-7)'s consulted semantics;
//# [spec §5.4](spec.md#closure)'s unfiltered traversal):
pub proof fn filter_soundness(
    g: Seq<Vec<u64>>,
    spans: Seq<Vec<(u64, u32)>>,
    root: u64,
    project: Seq<bool>,
    top: Seq<bool>,
)
    requires
        spans.len() == g.len(),
        spans_files_in_range(spans, top.len() as int),
        forall|i: int| 0 <= i < top.len() ==> #[trigger] top[i],
    ensures
        forall|f: u64, l: u32|
            #[trigger] assembled_line(g, spans, root, project, f, l) <==> (assembled_line(
                g,
                spans,
                root,
                top,
                f,
                l,
            ) && project_file(project, f)),
{
    assert forall|f: u64, l: u32|
        #[trigger] assembled_line(g, spans, root, project, f, l) <==> (assembled_line(
            g,
            spans,
            root,
            top,
            f,
            l,
        ) && project_file(project, f)) by {
        if assembled_line(g, spans, root, project, f, l) {
            // The consulted line's file id is in the table, so the
            // unfiltered (all-true) view accepts it.
            let t = choose|t: u64|
                (t as int) < g.len() && reachable(g, root, t) && #[trigger] spans[t
                    as int]@.contains((f, l));
            let j = choose|j: int|
                0 <= j < spans[t as int]@.len() && spans[t as int]@[j] == (f, l);
            assert(((spans[t as int]@[j]).0 as int) < top.len());
            assert(project_file(top, f));
        }
    }
}

} // verus!
