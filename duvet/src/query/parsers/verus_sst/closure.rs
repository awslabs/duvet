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

/// Total line count of a per-file line-set map — shared by the
/// aggregate map and per-unit closures.
// Golden-test reference surface: no engine-path consumer.
#[allow(dead_code)]
pub fn total_lines(files: &FileLines) -> usize {
    files.values().map(BTreeSet::len).sum()
}

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
///
/// The reached-set computation and witness-line assembly are the
/// *verified* producer core (`duvet_coverage::producer_core`,
/// Properties P1, P3, and P4 of
/// design/witness/producer-core-spec.md): reached is proven to be
/// exactly the downward-reachable set — reflexive, fixpoint, and
/// transparent — and the witness's line set is proven to equal the
/// union of the closure's per-file spans restricted to project
/// files, with project filtering a view, not a truncation (edge
/// traversal is unfiltered by the verified model's construction).
/// This function is the adapter (glue assumptions PG1/PG2/PG3): it
/// translates node names and file names to dense ids, keeps exactly
/// the edges that resolve to obligation nodes (spec §5.4 —
/// unresolved references are not part of the graph), calls the
/// verified core, and translates the results back. The
/// `project_obligations` count is report metadata computed in the
/// adapter, not part of the verified witness surface. The adapter's
/// faithfulness is what the golden corpus checks.
pub fn closure(
    graph: &ObligationGraph,
    root: &str,
    project: impl Fn(&str) -> bool,
) -> Option<Closure> {
    graph.nodes.get(root)?;

    // PG1: translate the parsed structure into the verified model.
    // Dense node ids in BTreeMap iteration (name) order; only edges
    // that resolve to a node become model edges, so the model is
    // well-formed by construction (`graph_wf`, the verified core's
    // precondition). File names are interned to dense ids, and the
    // project predicate is tabulated per file id (PG3).
    let index: BTreeMap<&str, u64> = graph
        .nodes
        .keys()
        .enumerate()
        .map(|(i, name)| (name.as_str(), i as u64))
        .collect();
    let mut file_ids: BTreeMap<&str, u64> = BTreeMap::new();
    let mut file_names: Vec<&str> = Vec::new();
    let mut model: Vec<Vec<u64>> = Vec::new();
    let mut span_model: Vec<Vec<(u64, u32)>> = Vec::new();
    for node in graph.nodes.values() {
        model.push(
            node.edges
                .iter()
                .filter_map(|e| index.get(e.as_str()).copied())
                .collect(),
        );
        let mut rows = Vec::new();
        for (file, lines) in &node.spans {
            let id = *file_ids.entry(file.as_str()).or_insert_with(|| {
                file_names.push(file.as_str());
                (file_names.len() - 1) as u64
            });
            rows.extend(lines.iter().map(|&l| (id, l)));
        }
        span_model.push(rows);
    }
    let project_flags: Vec<bool> = file_names.iter().map(|f| project(f)).collect();
    let root_id = index[root];

    // PG2: the reached set and the witness's line set come from the
    // verified core alone.
    let reached_mask = duvet_coverage::producer_core::closure_reached(&model, root_id);
    let lines = duvet_coverage::producer_core::assemble_witness_lines(
        &model,
        &span_model,
        root_id,
        &project_flags,
    );

    let mut files: FileLines = BTreeMap::new();
    for (fid, line) in lines {
        files
            .entry(file_names[fid as usize].to_string())
            .or_default()
            .insert(line);
    }

    let mut reached = BTreeSet::new();
    let mut project_obligations = BTreeSet::new();
    for (i, (name, node)) in graph.nodes.iter().enumerate() {
        if !reached_mask[i] {
            continue;
        }
        reached.insert(name.clone());
        if node
            .spans
            .keys()
            .any(|file| files.contains_key(file.as_str()))
        {
            project_obligations.insert(name.clone());
        }
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
        //= design/witness/spec.md#closure
        //= type=test
        //# Only reachable nodes contribute;
        //# nothing outside the reachable set may be included.
        // island is not downward-reachable: never enters.
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
            assert!(total_lines(&agg.0) > total_lines(&c.files));
        }
    }
}
