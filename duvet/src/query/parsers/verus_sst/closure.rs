// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Pass-1 aggregate projection and pass-2 closure traversal over the
//! [`ObligationGraph`] (spec §5.2, §5.4).
//!
//! Both functions take a `project` predicate that selects project
//! files; spans outside it (vstd, registry crates) never enter the
//! output. Edge *traversal* is not filtered: the closure is the full
//! downward fixpoint of the obligation graph, with only the span
//! *projection* restricted. Filtering edges instead was the SST POC's
//! shortcut, and it truncated real closures (POC regexes could not
//! even name `impl&%N::` obligations); this module deliberately does
//! not reproduce that.

//= design/witness/spec.md#closure
//# Reachability is transitive: the closure follows the obligation
//# graph's reference edges through any number of call or reference
//# hops — a lemma reaching a fn reaching a fn reaching a fn: all of
//# them enter — until a fixpoint.
//= design/witness/spec.md#closure
//# the closure MUST be a fixpoint (no truncation at a depth bound),

use super::structure::ObligationGraph;
use std::collections::{BTreeMap, BTreeSet};

/// Per-file line sets: `{file → set of lines}`.
pub type FileLines = BTreeMap<String, BTreeSet<u32>>;

/// Pass-1 scaffolding: every project line any obligation elaborated.
///
/// This is a newtype on purpose. Its only legitimate consumer is
/// liveness determination (which annotations are worth constructing
/// witnesses for); keeping it a distinct type means it cannot be
/// passed where witness file maps go without an explicit, visible
/// unwrap.
//= design/witness/spec.md#verus-producer
//# The aggregate executability map MUST NOT be delivered as a
//# witness: it is many obligations wearing one map, and delivering
//# it would violate [§4.2](#obligation-individuation) by
//# construction; it exists only as pass-1 scaffolding inside the
//# producer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AggregateExecutabilityMap(pub FileLines);

impl AggregateExecutabilityMap {
    // Golden-test reference surface: no engine-path consumer.
    #[allow(dead_code)]
    pub fn total_lines(&self) -> usize {
        self.0.values().map(BTreeSet::len).sum()
    }
}

/// Project every node's spans to one aggregate map (spec §5.2 pass 1).
// Golden-test reference surface (pass-1 aggregate map): no
// engine-path consumer.
#[allow(dead_code)]
pub fn aggregate_map(
    graph: &ObligationGraph,
    project: impl Fn(&str) -> bool,
) -> AggregateExecutabilityMap {
    let mut out: FileLines = BTreeMap::new();
    for node in graph.nodes.values() {
        for (file, lines) in &node.spans {
            if project(file) {
                out.entry(file.clone()).or_default().extend(lines);
            }
        }
    }
    AggregateExecutabilityMap(out)
}

/// The downward-reachable set from one discharge unit (spec §5.4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Closure {
    /// Every node reached, including nodes contributing no project
    /// spans (vstd obligations reached through broadcast groups).
    pub reached: BTreeSet<String>,
    /// The subset of `reached` contributing at least one project
    /// span — the "obligations" count in golden-test terms.
    pub project_obligations: BTreeSet<String>,
    /// Union of project-file spans over `reached`: the witness's
    /// `files` content (before conversion to coverage types).
    pub files: FileLines,
}

impl Closure {
    // Golden-test reference surface: no engine-path consumer.
    #[allow(dead_code)]
    pub fn total_lines(&self) -> usize {
        self.files.values().map(BTreeSet::len).sum()
    }
}

/// Compute the closure from `root` to fixpoint. `None` if `root` is
/// not a node in the graph.
///
/// Every symbolic reference is followed, whether or not the solver
/// needed it (Decision 7):
//= design/witness/spec.md#closure
//# and the semantics is *consulted* (strength `Consulted`, [§1.3](#provenance)),
//# not load-bearing dependency.
///
/// References to names with no `FunctionSst` block in the artifact
/// (externals with no logged body) are not part of the obligation
/// graph and do not appear in `reached`.
pub fn closure(
    graph: &ObligationGraph,
    root: &str,
    project: impl Fn(&str) -> bool,
) -> Option<Closure> {
    graph.nodes.get(root)?;

    let mut reached = BTreeSet::new();
    let mut project_obligations = BTreeSet::new();
    let mut files: FileLines = BTreeMap::new();
    let mut work = vec![root.to_string()];

    while let Some(name) = work.pop() {
        let Some(node) = graph.nodes.get(&name) else {
            continue;
        };
        if !reached.insert(name.clone()) {
            continue;
        }
        let mut contributes = false;
        for (file, lines) in &node.spans {
            if project(file) {
                contributes = true;
                files.entry(file.clone()).or_default().extend(lines);
            }
        }
        if contributes {
            project_obligations.insert(name.clone());
        }
        work.extend(node.edges.iter().cloned());
    }

    Some(Closure {
        reached,
        project_obligations,
        files,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::parsers::verus_sst::structure::parse_module;

    const DIAMOND: &str = r#"
(@ "src/a.rs:1:1: 2:2 (#0)"
 (FunctionSst :name (Fun :path c::top)
  ((Fun :path c::left) (Fun :path c::right))))
(@ "src/a.rs:10:1: 11:2 (#0)"
 (FunctionSst :name (Fun :path c::left) ((Fun :path c::bottom))))
(@ "src/a.rs:20:1: 21:2 (#0)"
 (FunctionSst :name (Fun :path c::right) ((Fun :path c::bottom))))
(@ "vstd/x.rs:5:1: 6:2 (#0)"
 (FunctionSst :name (Fun :path c::bottom) ((Fun :path c::top))))
(@ "src/b.rs:1:1: 9:2 (#0)"
 (FunctionSst :name (Fun :path c::island) ()))
"#;

    fn graph() -> ObligationGraph {
        ObligationGraph::merge([parse_module(DIAMOND).unwrap()]).unwrap()
    }

    #[test]
    fn closure_is_a_fixpoint_and_tolerates_cycles() {
        // bottom → top closes a cycle; the worklist must terminate
        // and reach all four nodes exactly once.
        let g = graph();
        let c = closure(&g, "c::top", |f| f.starts_with("src/")).unwrap();
        assert_eq!(c.reached.len(), 4);
        // bottom's span is non-project: reached but not counted.
        assert_eq!(c.project_obligations.len(), 3);
        assert_eq!(
            c.files["src/a.rs"],
            [1, 2, 10, 11, 20, 21].into_iter().collect()
        );
        assert!(!c.files.contains_key("vstd/x.rs"));
        // island is not downward-reachable: never enters (spec §5.4
        // "nothing outside the reachable set may be included").
        assert!(!c.reached.contains("c::island"));
    }

    #[test]
    fn closure_of_unknown_root_is_none() {
        assert!(closure(&graph(), "c::nope", |_| true).is_none());
    }

    #[test]
    fn aggregate_covers_every_closure() {
        let g = graph();
        let project = |f: &str| f.starts_with("src/");
        let agg = aggregate_map(&g, project);
        for root in g.nodes.keys() {
            let c = closure(&g, root, project).unwrap();
            for (file, lines) in &c.files {
                assert!(
                    lines.is_subset(&agg.0[file]),
                    "closure of {root} escapes the aggregate map"
                );
            }
        }
        // Strictly larger than each closure here: island (src/b.rs)
        // is in no other node's closure, and no node reaches all.
        for root in g.nodes.keys() {
            let c = closure(&g, root, project).unwrap();
            assert!(agg.total_lines() > c.total_lines());
        }
    }
}
