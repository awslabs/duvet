// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Discharge-unit mapping and witness construction (spec §5.3, §5.5).
//!
//! The types below are the *producer's own* output shapes. They
//! anticipate spec §1.2 (Witness) and §1.3 (Provenance) but are
//! deliberately not the engine's types: engine wiring lives in
//! `producers.rs`, and per spec §1.7 the producer-internal artifact
//! format must not escape — these types are the boundary at which
//! it stops.
//!
//! Discharge units exist at every granularity the artifact records
//! (spec §5.3): obligation extents, ensures clauses, loop
//! invariants, and proof asserts. Every unit inside a function
//! carries the function's consulted closure (spec §5.4 — the
//! closure ceiling).

use super::{
    closure::{closure, FileLines},
    structure::{ClauseUnit, ObligationGraph, ObligationNode, Span, UnitKind},
};

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
///
/// A prover producer only ever delivers `Consulted`. The runtime
/// rung (`Executed`) lives in the engine's `Strength` taxonomy —
/// the conversion in `producers.rs` maps this type into it — and
/// deliberately has no mirror variant here: a variant this module
/// can never construct would only be unreachable conversion code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Strength {
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
/// verification, scoped to one discharge unit, with everything the
/// obligation's elaboration consulted (spec §1.2, restricted to
/// project files).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerusWitness {
    /// Human-readable identity of the discharge unit
    /// (spec §5.5, Decision 20): the obligation's fully-qualified
    /// path for extent units; `proof_note` text or span identity
    /// for clause-kind units.
    pub label: String,
    pub claim: ClaimRule,
    pub provenance: Provenance,
    /// Closed per-file line sets (§4.1): the fixpoint closure of the
    /// discharge unit, projected to project files.
    pub files: FileLines,
}

pub const PRODUCER: &str = "verus-sst";

/// The report sentence for a not-proof-testable position: this
/// position was elaborated, but nothing dischargeable is rooted
/// there.
//= design/witness/spec.md#two-pass-construction
//# and the report MUST identify the annotation as
//# *not proof-testable* ("this position carries no dischargeable
//# obligation; it can only be witnessed by an execution-style
//# producer") — a report distinct from Property W6's
//# "no witness from any configured producer."
pub const NOT_PROOF_TESTABLE: &str = "this position carries no dischargeable obligation; \
     it can only be witnessed by an execution-style producer";

/// What a source position is, to the discharge-unit map
/// (spec §5.2/§5.3, Decision 13).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PositionKind<'g> {
    /// In `dom(du)`: at least one discharge unit is rooted here.
    /// Several when byte-identical spans tie at one specificity
    /// level (Decision 12); under Decision 14 the annotation is
    /// held to ALL of them.
    Rooted(Vec<DischargeUnit<'g>>),
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

/// One discharge unit, resolved against its owning obligation: the
/// pair (obligation, unit span) plus the unit's kind and report
/// label. The obligation is the closure root; the unit span is the
/// `ByRootSpan` claim.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DischargeUnit<'g> {
    /// The owning obligation (the `FunctionSst` node).
    pub node: &'g ObligationNode,
    pub kind: UnitKind,
    /// The unit's own span: the node extent for `Extent` units, the
    /// clause/invariant/assert span otherwise.
    pub span: Span,
    /// Report label (spec §5.5, Decision 20).
    pub label: String,
}

impl<'g> DischargeUnit<'g> {
    fn extent(node: &'g ObligationNode) -> Self {
        DischargeUnit {
            node,
            kind: UnitKind::Extent,
            span: node.extent.clone(),
            label: node.name.clone(),
        }
    }

    //= design/witness/spec.md#verus-producer
    //= type=implementation
    //# Unit labels (decisions.md, Decision 20): `ProofNoteLabel` text
    //# when the artifact records it, otherwise span identity
    //# (function path + unit kind + clause index).
    fn clause(node: &'g ObligationNode, unit: &ClauseUnit) -> Self {
        let label = match &unit.note {
            Some(text) => text.clone(),
            None => format!("{} {}[{}]", node.name, unit.kind.token(), unit.index),
        };
        DischargeUnit {
            node,
            kind: unit.kind,
            span: unit.span.clone(),
            label,
        }
    }
}

/// Classify a position against the artifact (Decision 13).
///
/// The domain check is exact — it is the soundness boundary: a
/// position is in `dom(du)` iff a discharge unit is *rooted* there.
///
//= design/witness/spec.md#discharge-unit
//= type=implementation
//# Its domain (`dom(du)`) MUST contain only positions where an
//# obligation is *rooted* — proof-element positions,
//# at every granularity the artifact demonstrably records
//# (decisions.md, Decisions 13, 17, 18):
//# fn/lemma headers (obligation extents), ensures clauses,
//# loop invariants, and proof asserts.
///
//= design/witness/spec.md#discharge-unit
//= type=implementation
//# Executable body lines MUST NOT be in the domain:
//# no obligation is rooted at a body line —
//# body lines are material a proof consults,
//# not claims a prover discharges —
//# so a test annotation there is category-mismatched,
//# and is reported *not proof-testable* rather than unwitnessed
//# (decisions.md, [Decision 13](decisions.md#decision-13)).
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

/// Map an annotation position to the discharge unit(s) it roots —
/// `du` (spec §5.3, Decisions 9, 12, 13, 18, 19).
///
//= design/witness/spec.md#discharge-unit
//= type=implementation
//# **Rooting is most-specific-wins** (decisions.md, Decision 19):
//# an annotation roots the finest unit whose span contains its
//# resolved position; the enclosing extent is the fallback for
//# positions inside no finer unit. A producer MUST NOT hoist an
//# annotation placed on a clause, invariant, or assert to the
//# enclosing function's unit.
///
/// Two specificity levels: clause-kind units (ensures clauses,
/// loop invariants, proof asserts) and obligation extents. A
/// position inside any clause-kind unit roots at the clause level
/// and the extent level never applies; the extent level is the
/// fallback. Within the winning level, nesting resolves to the
/// units of minimal line extent.
///
//= design/witness/spec.md#discharge-unit
//= type=implementation
//# When N obligations root a position at the chosen level, the
//# producer MUST deliver one witness per rooting obligation and
//# MUST NOT select among them (decisions.md, Decision 12).
///
/// Order is deterministic: ascending by obligation name, then unit
/// kind, then clause index (via label).
pub fn find_discharge_units<'g>(
    graph: &'g ObligationGraph,
    file: &str,
    line: u32,
) -> Vec<DischargeUnit<'g>> {
    let minimal = |mut units: Vec<DischargeUnit<'g>>| -> Vec<DischargeUnit<'g>> {
        let Some(min_size) = units.iter().map(|u| u.span.line_count()).min() else {
            return Vec::new();
        };
        units.retain(|u| u.span.line_count() == min_size);
        units
    };

    let clause_level: Vec<DischargeUnit<'g>> = graph
        .nodes
        .values()
        .flat_map(|n| n.units.iter().map(move |u| (n, u)))
        .filter(|(_, u)| u.span.contains(file, line))
        .map(|(n, u)| DischargeUnit::clause(n, u))
        .collect();
    if !clause_level.is_empty() {
        return minimal(clause_level);
    }

    minimal(
        graph
            .nodes
            .values()
            .filter(|n| n.extent.contains(file, line))
            .map(DischargeUnit::extent)
            .collect(),
    )
}

/// The witness of one discharge unit: a pure function of
/// (artifact, unit) — no annotation identity anywhere
/// (spec §1.7: the annotations input is a semantically inert
/// optimization). Shared by [`construct_witnesses`] and
/// [`materialize_all`] so annotation-driven and full-universe
/// production cannot diverge by construction.
///
//= design/witness/spec.md#closure
//= type=implementation
//# **The closure ceiling MUST be stated per producer, because it
//# bounds what unit granularity buys.** Where a prover checks a
//# function's obligations in one solver query and assumes callee
//# contracts whole (Verus does both, §5.5), every discharge unit
//# inside a function carries the identical function-level consulted
//# closure: finer units buy precise identity and legible failures,
//# not smaller witnesses, and discharge verdicts within one function
//# do not differ across its units at Consulted strength.
fn witness_for_unit(
    graph: &ObligationGraph,
    unit: &DischargeUnit<'_>,
    artifact: &str,
    project: impl Fn(&str) -> bool,
) -> VerusWitness {
    let closure = closure(graph, &unit.node.name, project)
        .expect("unit came from this graph; root must exist");
    VerusWitness {
        label: unit.label.clone(),
        claim: ClaimRule::ByRootSpan(unit.span.clone()),
        provenance: Provenance {
            producer: PRODUCER,
            artifact: artifact.to_string(),
            discharge_unit: Some(unit.node.name.clone()),
            strength: Strength::Consulted,
        },
        files: closure.files,
    }
}

/// Construct the witnesses for an annotation resolved to `file:line`
/// (spec §5.2 pass 2, §5.5): one witness per rooted discharge unit.
///
/// Usually a singleton. Byte-identical span ties at one specificity
/// level (Decision 12) yield one witness per rooting unit, each
/// individually A2-sound (one unit, its function's closure, its own
/// root span); under Decision 14 discharge holds the annotation to
/// all of them. Empty when the position is outside `dom(du)` — use
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
        .map(|unit| witness_for_unit(graph, &unit, artifact, &project))
        .collect()
}

/// Every discharge unit the artifact records, in ascending
/// (obligation name, unit kind, clause index) order: obligation
/// extents plus all clause-kind units of every node (spec §5.3,
/// Decision 18).
// Golden-test reference surface (full-universe entry point): no
// engine-path consumer.
#[allow(dead_code)]
pub fn all_units(graph: &ObligationGraph) -> Vec<DischargeUnit<'_>> {
    graph
        .nodes
        .values()
        .flat_map(|n| {
            std::iter::once(DischargeUnit::extent(n))
                .chain(n.units.iter().map(move |u| DischargeUnit::clause(n, u)))
        })
        .collect()
}

/// Materialize-everything mode (spec §1.7): the full witness
/// universe, one witness per discharge unit in the artifact.
///
/// The annotations argument of `produce` only *selects* members of
/// this universe; it never changes their content. This function is
/// the reference the filter-soundness golden tests compare
/// annotation-driven construction against.
// Golden-test reference surface (filter-soundness comparison
// reference): no engine-path consumer.
#[allow(dead_code)]
pub fn materialize_all(
    graph: &ObligationGraph,
    artifact: &str,
    project: impl Fn(&str) -> bool,
) -> Vec<VerusWitness> {
    all_units(graph)
        .iter()
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

    fn unit_names(units: &[DischargeUnit<'_>]) -> Vec<String> {
        units.iter().map(|u| u.node.name.clone()).collect()
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
            [
                parse_module(f_first).unwrap(),
                parse_module(g_first).unwrap(),
            ],
            [
                parse_module(g_first).unwrap(),
                parse_module(f_first).unwrap(),
            ],
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
        let ws = construct_witnesses(&g, "src/a.rs", 15, "logs/", |f| f.starts_with("src/"));
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
