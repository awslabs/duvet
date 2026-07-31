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

} // verus!
