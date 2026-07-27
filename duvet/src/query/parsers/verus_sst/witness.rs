// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Discharge-unit mapping and witness construction (spec §5.3, §5.5).
//!
//! The types below are the *producer's own* output shapes. They
//! anticipate spec §1.2 (Witness) and §1.3 (Provenance) but are
//! deliberately not the engine's types: engine wiring is a later
//! milestone, and per spec §1.7 the producer-internal artifact
//! format must not escape — these types are the boundary at which
//! it stops.
//!
//! Discharge-unit granularity in this first version is
//! whole-proof-fn (spec §5.5): the discharge unit of a position is
//! the `FunctionSst` node whose extent contains it. Finer units
//! (`:enss` clauses, `LoopInv` nodes) exist in the artifact and may
//! follow without a format change.

use super::closure::{closure, FileLines};
use super::structure::{ObligationGraph, ObligationNode, Span};

/// How a test annotation claims this witness (spec §1.5).
///
/// Prover witnesses are constructed from the annotation's own
/// position, so the claim is positional: the discharge unit's
/// extent. `ByExecution` is a runtime-producer rule and never
/// constructed here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClaimRule {
    ByRootSpan(Span),
}

/// Witness strength (spec §1.3): what kind of claim it supports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Strength {
    /// A runtime act ran these lines. Never produced here.
    Executed,
    /// A prover's elaboration reached these lines (Decision 7:
    /// consulted semantics, execution parity, no stronger).
    Consulted,
}

/// Witness provenance (spec §1.3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Provenance {
    pub producer: &'static str,
    /// Source artifact (the log directory or file set).
    pub artifact: String,
    /// The obligation the witness was constructed from.
    pub discharge_unit: Option<String>,
    pub strength: Strength,
}

/// One act of checking: a prover obligation's successful
/// verification, with everything its elaboration consulted
/// (spec §1.2, restricted to project files).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerusWitness {
    /// Human-readable identity: the obligation's fully-qualified
    /// path.
    pub label: String,
    pub claim: ClaimRule,
    pub provenance: Provenance,
    /// Closed per-file line sets (§4.1): the fixpoint closure of the
    /// discharge unit, projected to project files.
    pub files: FileLines,
}

pub const PRODUCER: &str = "verus-sst";

/// The report sentence for a not-proof-testable position, verbatim
/// from spec §5.2. Distinct from Property W6's "no witness from any
/// configured producer": this position was elaborated, but nothing
/// dischargeable is rooted there.
pub const NOT_PROOF_TESTABLE: &str = "this position carries no dischargeable obligation; \
     it can only be witnessed by an execution-style producer";

/// What a source position is, to the discharge-unit map
/// (spec §5.2/§5.3, Decision 13).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PositionKind<'g> {
    /// In `dom(du)`: at least one obligation is rooted here.
    /// Several when generated obligations share one stamped extent
    /// (Decision 12); under Decision 14 the annotation is held to
    /// ALL of them.
    Rooted(Vec<&'g ObligationNode>),
    /// Elaborated by the verifier but rooting nothing: an
    /// executable body line — proof *ingredient*, not a claim. A
    /// test annotation here gets zero proof witnesses by
    /// definition, and the report says [`NOT_PROOF_TESTABLE`].
    /// Runtime producers witness such positions normally.
    NotProofTestable,
    /// The artifact never elaborated this position at all. The
    /// annotation surfaces through Property W6 if no other
    /// producer witnesses it.
    Unelaborated,
}

/// Classify a position against the artifact (Decision 13).
///
/// The domain check is exact from the start — it is the soundness
/// boundary: a position is in `dom(du)` iff an obligation is
/// *rooted* there, and today rooting means extent containment
/// (fn/lemma headers, plus whatever contract lines the extent
/// spans). Finer proof-element rootings (`:enss` clause spans,
/// `LoopInv` nodes, proof asserts) are present in the artifact and
/// may land incrementally — they can only move positions from
/// `NotProofTestable` to `Rooted`, never the reverse. Loop headers
/// are deliberately excluded (Decision 13: revisitable with
/// evidence).
pub fn classify_position<'g>(
    graph: &'g ObligationGraph,
    file: &str,
    line: u32,
) -> PositionKind<'g> {
    let rooted = find_discharge_units(graph, file, line);
    if !rooted.is_empty() {
        return PositionKind::Rooted(rooted);
    }
    let elaborated = graph
        .nodes
        .values()
        .any(|n| n.spans.get(file).is_some_and(|lines| lines.contains(&line)));
    if elaborated {
        PositionKind::NotProofTestable
    } else {
        PositionKind::Unelaborated
    }
}

/// Map an annotation position to its rooting obligation(s) —
/// `du` (spec §5.3, Decisions 9, 12, 13).
///
/// Attribution is by **extent containment only**: the obligations
/// whose extent contains the position, restricted to those of
/// minimal extent size (strict nesting yields the innermost alone).
/// *Equal* minimal extents are the stamped-generated-obligation
/// situation (the corpus's arrow-accessor pair shares extent
/// types.rs 154..=166 exactly): per Decision 12 every tied node is
/// a unit, the producer MUST NOT select among them, and per
/// Decision 14 the annotation is held to all of them.
///
/// Decision 13 deleted the former own-span-ownership fallback:
/// attributing a body line to the enclosing obligation constructed
/// a witness whose `ByRootSpan` claim could not contain the
/// annotation that requested it — a dead witness from two
/// geometries answering differently. Body lines are not in
/// `dom(du)` at all; [`classify_position`] reports them
/// [`PositionKind::NotProofTestable`].
///
/// Empirical extent shapes (golden corpus, 2026-07-27): proof-mode
/// fns carry whole-body extents (`lemma_no_cross_scope_leakage`,
/// 54..=61 — its ensures lines are in-domain via containment);
/// exec/spec fns carry declaration extents that may span the
/// contract header (`execution_set`, 62..=66) or a single line
/// (`self_contained` in the vacuity fixture) — 31 of 64 unique
/// `duvet_coverage` obligations are single-line. Contract lines
/// outside the extent join the domain when `:enss` rooting lands.
///
/// Order is deterministic (ascending by obligation name).
pub fn find_discharge_units<'g>(
    graph: &'g ObligationGraph,
    file: &str,
    line: u32,
) -> Vec<&'g ObligationNode> {
    let containing: Vec<&ObligationNode> = graph
        .nodes
        .values()
        .filter(|n| n.extent.contains(file, line))
        .collect();
    let Some(min_size) = containing.iter().map(|n| n.extent.line_count()).min() else {
        return Vec::new();
    };
    containing
        .into_iter()
        .filter(|n| n.extent.line_count() == min_size)
        .collect()
}

/// The witness of one obligation: a pure function of
/// (artifact, obligation) — no annotation identity anywhere
/// (spec §1.7: the annotations input is a semantically inert
/// optimization). Shared by [`construct_witnesses`] and
/// [`materialize_all`] so annotation-driven and full-universe
/// production cannot diverge by construction.
fn witness_for_unit(
    graph: &ObligationGraph,
    unit: &ObligationNode,
    artifact: &str,
    project: impl Fn(&str) -> bool,
) -> VerusWitness {
    let closure = closure(graph, &unit.name, project)
        .expect("unit came from this graph; root must exist");
    VerusWitness {
        label: unit.name.clone(),
        claim: ClaimRule::ByRootSpan(unit.extent.clone()),
        provenance: Provenance {
            producer: PRODUCER,
            artifact: artifact.to_string(),
            discharge_unit: Some(unit.name.clone()),
            strength: Strength::Consulted,
        },
        files: closure.files,
    }
}

/// Construct the witnesses for an annotation resolved to `file:line`
/// (spec §5.2 pass 2, §5.5): one witness per rooting obligation.
///
/// Usually a singleton. Equal-extent ties (Decision 12) yield one
/// witness per rooting obligation, each individually A2-sound (one
/// obligation, its own closure, its own root span); under
/// Decision 14 discharge holds the annotation to all of them.
/// Empty when the position is outside `dom(du)` — use
/// [`classify_position`] to distinguish not-proof-testable
/// (Decision 13 report) from unelaborated (Property W6).
pub fn construct_witnesses(
    graph: &ObligationGraph,
    file: &str,
    line: u32,
    artifact: &str,
    project: impl Fn(&str) -> bool,
) -> Vec<VerusWitness> {
    find_discharge_units(graph, file, line)
        .into_iter()
        .map(|unit| witness_for_unit(graph, unit, artifact, &project))
        .collect()
}

/// Materialize-everything mode (spec §1.7): the full witness
/// universe, one witness per obligation in the artifact, in
/// ascending obligation-name order.
///
/// The annotations argument of `produce` only *selects* members of
/// this universe; it never changes their content. This function is
/// the reference the filter-soundness golden tests compare
/// annotation-driven construction against.
pub fn materialize_all(
    graph: &ObligationGraph,
    artifact: &str,
    project: impl Fn(&str) -> bool,
) -> Vec<VerusWitness> {
    graph
        .nodes
        .values()
        .map(|unit| witness_for_unit(graph, unit, artifact, &project))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::parsers::verus_sst::structure::parse_module;

    const TWO_FNS: &str = r#"
(@ "src/a.rs:10:1: 20:2 (#0)"
 (FunctionSst :name (Fun :path c::caller) ((Fun :path c::callee))))
(@ "src/b.rs:5:1: 8:2 (#0)"
 (FunctionSst :name (Fun :path c::callee) ()))
"#;

    fn graph() -> ObligationGraph {
        ObligationGraph::merge([parse_module(TWO_FNS).unwrap()]).unwrap()
    }

    fn unit_names(units: &[&super::super::structure::ObligationNode]) -> Vec<String> {
        units.iter().map(|n| n.name.clone()).collect()
    }

    #[test]
    fn position_maps_to_containing_extent() {
        let g = graph();
        assert_eq!(
            unit_names(&find_discharge_units(&g, "src/a.rs", 15)),
            ["c::caller"]
        );
        assert_eq!(
            unit_names(&find_discharge_units(&g, "src/b.rs", 5)),
            ["c::callee"]
        );
        // Boundary lines are inside.
        assert_eq!(
            unit_names(&find_discharge_units(&g, "src/a.rs", 10)),
            ["c::caller"]
        );
        assert_eq!(
            unit_names(&find_discharge_units(&g, "src/a.rs", 20)),
            ["c::caller"]
        );
        // Outside every extent: no rooting (classification decides
        // between not-proof-testable and unelaborated/W6).
        assert!(find_discharge_units(&g, "src/a.rs", 21).is_empty());
        assert!(find_discharge_units(&g, "src/nope.rs", 1).is_empty());
    }

    #[test]
    fn body_lines_are_not_proof_testable() {
        // Exec-mode shape: declaration-only extent (line 10), body
        // recorded as inner spans (11..=14). Under Decision 13 the
        // body lines root nothing — they are proof ingredients, not
        // claims — so du refuses them and classification names the
        // distinct not-proof-testable outcome, NOT W6.
        let src = r#"
(@ "src/a.rs:10:1: 10:40 (#0)"
 (FunctionSst :name (Fun :path c::sig_fn) :body
  ((@ "src/a.rs:11:5: 14:6 (#0)" (Block)))))
"#;
        let g = ObligationGraph::merge([parse_module(src).unwrap()]).unwrap();
        assert!(find_discharge_units(&g, "src/a.rs", 12).is_empty());
        assert_eq!(
            classify_position(&g, "src/a.rs", 12),
            PositionKind::NotProofTestable
        );
        // The declaration line itself is rooted.
        assert!(matches!(
            classify_position(&g, "src/a.rs", 10),
            PositionKind::Rooted(units) if unit_names(&units) == ["c::sig_fn"]
        ));
        // A line the artifact never elaborated is a different
        // outcome again: W6 territory.
        assert_eq!(
            classify_position(&g, "src/a.rs", 999),
            PositionKind::Unelaborated
        );
        assert_eq!(
            classify_position(&g, "src/nope.rs", 1),
            PositionKind::Unelaborated
        );
    }

    #[test]
    fn shared_consulted_line_roots_nothing() {
        // Two same-file obligations both *consult* line 30 but
        // neither's extent contains it: under Decision 13 there is
        // no ownership fallback — the line roots nothing and is
        // not proof-testable, regardless of merge order.
        let f_first = r#"
(@ "src/a.rs:10:1: 10:40 (#0)"
 (FunctionSst :name (Fun :path c::f) :body
  ((@ "src/a.rs:30:1: 31:2 (#0)" (Consulted)))))
"#;
        let g_first = r#"
(@ "src/a.rs:20:1: 20:40 (#0)"
 (FunctionSst :name (Fun :path c::g) :body
  ((@ "src/a.rs:30:1: 31:2 (#0)" (Consulted)))))
"#;
        for modules in [
            [parse_module(f_first).unwrap(), parse_module(g_first).unwrap()],
            [parse_module(g_first).unwrap(), parse_module(f_first).unwrap()],
        ] {
            let g = ObligationGraph::merge(modules).unwrap();
            assert_eq!(
                classify_position(&g, "src/a.rs", 30),
                PositionKind::NotProofTestable
            );
            assert!(construct_witnesses(&g, "src/a.rs", 30, "x", |_| true).is_empty());
        }
    }

    #[test]
    fn report_text_is_the_normative_sentence() {
        // Spec §5.2 quotes the report sentence; pin it verbatim so
        // a wording drift between spec and producer is a test
        // failure, not a silent divergence.
        assert_eq!(
            NOT_PROOF_TESTABLE,
            "this position carries no dischargeable obligation; \
             it can only be witnessed by an execution-style producer"
        );
    }

    #[test]
    fn containment_short_circuits_ownership() {
        // Line 10 is inside c::inner's extent AND merely consulted
        // by c::outer: containment roots it in c::inner alone —
        // consulted spans never confer rooting (Decision 13).
        let src = r#"
(@ "src/a.rs:10:1: 12:2 (#0)"
 (FunctionSst :name (Fun :path c::inner) :body ()))
(@ "src/a.rs:20:1: 20:40 (#0)"
 (FunctionSst :name (Fun :path c::outer) :body
  ((@ "src/a.rs:10:1: 12:2 (#0)" (ConsultedDecl)))))
"#;
        let g = ObligationGraph::merge([parse_module(src).unwrap()]).unwrap();
        assert_eq!(
            unit_names(&find_discharge_units(&g, "src/a.rs", 10)),
            ["c::inner"]
        );
    }

    #[test]
    fn containment_nesting_vs_equal_extent_ties() {
        // c::wide (1..=20) strictly contains c::narrow (5..=8);
        // c::twin_a and c::twin_b share the extent 5..=8 exactly
        // (the corpus's generated arrow-accessor shape).
        let src = r#"
(@ "src/a.rs:1:1: 20:2 (#0)"
 (FunctionSst :name (Fun :path c::wide) :body ()))
(@ "src/a.rs:5:1: 8:2 (#0)"
 (FunctionSst :name (Fun :path c::narrow) :body ()))
(@ "src/a.rs:5:1: 8:2 (#0)"
 (FunctionSst :name (Fun :path c::twin_a) :body ()))
(@ "src/a.rs:5:1: 8:2 (#0)"
 (FunctionSst :name (Fun :path c::twin_b) :body ()))
"#;
        let g = ObligationGraph::merge([parse_module(src).unwrap()]).unwrap();
        // Strict nesting: only the minimal-extent nodes, not wide.
        // Equal minimal extents: ALL of them — selecting one would
        // be arbitrary (Decision 12).
        assert_eq!(
            unit_names(&find_discharge_units(&g, "src/a.rs", 6)),
            ["c::narrow", "c::twin_a", "c::twin_b"]
        );
        // Outside the twins, wide is the only (hence minimal) unit.
        assert_eq!(
            unit_names(&find_discharge_units(&g, "src/a.rs", 15)),
            ["c::wide"]
        );
    }

    #[test]
    fn witness_carries_root_span_closure_and_provenance() {
        let g = graph();
        let ws = construct_witnesses(&g, "src/a.rs", 15, "logs/", |f| {
            f.starts_with("src/")
        });
        let [w] = ws.as_slice() else {
            panic!("expected exactly one witness, got {}", ws.len())
        };
        assert_eq!(w.label, "c::caller");
        assert_eq!(
            w.claim,
            ClaimRule::ByRootSpan(Span {
                file: "src/a.rs".into(),
                start_line: 10,
                end_line: 20
            })
        );
        assert_eq!(w.provenance.producer, "verus-sst");
        assert_eq!(w.provenance.strength, Strength::Consulted);
        assert_eq!(w.provenance.discharge_unit.as_deref(), Some("c::caller"));
        // Closure pulled the callee's file in.
        assert!(w.files.contains_key("src/b.rs"));

        assert!(
            construct_witnesses(&g, "src/a.rs", 999, "logs/", |_| true).is_empty(),
            "zero discharge units → zero witnesses (feeds W6)"
        );
    }
}
