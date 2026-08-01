// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Golden tests for the Verus SST producer (spec §4.3: every
//! producer MUST be unit-tested against golden artifacts).
//!
//! Two artifact sets:
//!
//! 1. The **real corpus**: 10 per-module SST logs of
//!    `duvet-coverage` (2026-07-26, Verus 0.2026.05.24.ecee80a,
//!    189 obligations under the POC's naming — see the
//!    reconciliation test). Location comes from
//!    `DUVET_SST_LOG_DIR`, defaulting to the checked-in gzipped
//!    corpus (`testdata/corpus/`, tracked with git-lfs — see
//!    `corpus_dir` below for regeneration).
//! 2. The **vacuity fixture** (`testdata/vacuity/`), checked in,
//!    self-regenerable (see its README).
//!
//! Golden numbers: the SST POC's FINDINGS.md reported 10 vs 18
//! closure obligations for the discrimination pair. Those numbers
//! are artifacts of the POC's regexes: its name charset
//! (`[A-Za-z0-9_:]`) could not name `impl&%N::` obligations, it
//! only followed `duvet_coverage::`-prefixed edges, and its block
//! segmentation absorbed interleaved `trait_impl`/`group`
//! top-levels into the preceding block. The numbers pinned here are
//! re-derived by this structural parser over the full graph
//! (fixpoint per spec §5.4) and supersede FINDINGS.md:
//! project-obligation counts 10 vs 25, with the *discrimination
//! property itself* — the lemma's closure never reaches
//! `annotation_execution.rs`; the executed-annotation variant's
//! does — unchanged. Every corpus golden below was cross-checked
//! against an independent computation (python, correct top-level
//! segmentation) before pinning (2026-07-27); the two agree
//! exactly on all counts.

use super::{
    closure::{aggregate_map, closure, total_lines, Closure, FileLines},
    load_dir,
    structure::{parse_module, ObligationGraph, UnitKind},
    witness::{all_units, classify_position, construct_witnesses, materialize_all, PositionKind},
};
use crate::query::witness::{ClaimRule, CoverageReportMap, Strength, Witness};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
};

/// A closure's per-file line sets in the emitted witness shape
/// (consulted lines as `Hit` — see `ClosureMemo::get`), for equality
/// assertions against `Witness::files`.
fn hit_files(files: &FileLines) -> std::collections::BTreeMap<String, Arc<CoverageReportMap>> {
    files
        .iter()
        .map(|(file, lines)| {
            (
                file.clone(),
                Arc::new(
                    lines
                        .iter()
                        .map(|&l| (u64::from(l), duvet_coverage::types::CoverageStatus::Hit))
                        .collect(),
                ),
            )
        })
        .collect()
}

const LEMMA: &str = "duvet_coverage::proofs::lemma_no_cross_scope_leakage";
const EXECUTED: &str = "duvet_coverage::proofs::executed_annotation_has_no_cross_scope_leakage";

fn corpus_dir() -> PathBuf {
    // Override for validating against freshly generated logs
    // (e.g. after a Verus version bump):
    //   DUVET_SST_LOG_DIR=/path/to/logs cargo test
    if let Ok(dir) = std::env::var("DUVET_SST_LOG_DIR") {
        return PathBuf::from(dir);
    }
    // Default: the checked-in gzipped corpus (load_dir reads
    // *-sst.vir.gz transparently), generated 2026-07-26 from
    //   cargo verus build -p duvet-coverage -- --log vir-sst
    // (Verus 0.2026.05.24.ecee80a). Regenerate with:
    //   gzip -9 -c <log>/<m>-sst.vir > testdata/corpus/<m>-sst.vir.gz
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src/query/parsers/verus_sst/testdata/corpus")
}

fn corpus() -> &'static ObligationGraph {
    static GRAPH: OnceLock<ObligationGraph> = OnceLock::new();
    GRAPH.get_or_init(|| load_dir(&corpus_dir()).expect("corpus must load"))
}

/// Spec §5.2's abort-don't-skip posture, end to end: a module log
/// whose clause span does not parse aborts the whole directory
/// load — it must not degrade into a graph whose annotations
/// silently fall back to extent rooting (the hoisting §5.3
/// forbids).
#[test]
fn malformed_clause_span_aborts_the_load() {
    let dir = std::env::temp_dir().join(format!(
        "duvet-sst-malformed-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("bad-sst.vir"),
        r#"
(@ "src/u.rs:10:1: 12:2 (#0)"
 (FunctionSst :name (Fun :path crate::bad)
  :decl (FuncDeclSst :reqs ()
   :enss (tuple
    ((@@ "no location" (Exp Const (Constant Bool true))))
    ()))))
"#,
    )
    .unwrap();
    let result = load_dir(&dir);
    std::fs::remove_dir_all(&dir).unwrap();
    let err = result.expect_err("load must abort on a malformed clause span");
    let message = err.to_string();
    assert!(
        message.contains("ensures clause span \"no location\" does not parse"),
        "error must name the malformed span: {message}"
    );
}

fn is_project(file: &str) -> bool {
    file.starts_with("duvet-coverage/")
}

fn file_line_counts(c: &Closure) -> Vec<(&str, usize)> {
    c.files
        .iter()
        .map(|(f, lines)| (f.as_str(), lines.len()))
        .collect()
}

// ---------------------------------------------------------------
// Real corpus: parser round-trip sanity
// ---------------------------------------------------------------

//= design/witness/spec.md#verus-producer
//= type=test
//# A flat span inventory of one block covers essentially
//# only its own extent, so the Verus producer MUST parse its
//# artifact once into a structure of obligation nodes and
//# reference edges and derive everything else from that
//# structure — parse-into-structure is a MUST, not an
//# optimization.
#[test]
fn corpus_node_counts() {
    // Raw per-module block count, before deduplication: imported
    // declarations are re-logged per module, so blocks > unique.
    let mut raw_blocks = 0usize;
    for entry in std::fs::read_dir(corpus_dir()).unwrap() {
        let path = entry.unwrap().path();
        let Some(p) = path.to_str() else { continue };
        if !(p.ends_with("-sst.vir") || p.ends_with("-sst.vir.gz")) {
            continue;
        }
        let text = super::read_log(&path).unwrap();
        raw_blocks += parse_module(&text).unwrap().len();
    }
    assert_eq!(raw_blocks, 1065, "top-level FunctionSst blocks, 10 files");

    let graph = corpus();
    assert_eq!(graph.nodes.len(), 330, "unique fully-qualified names");
    assert_eq!(
        graph
            .nodes
            .keys()
            .filter(|n| n.starts_with("duvet_coverage::"))
            .count(),
        64,
        "unique duvet_coverage obligations"
    );
}

// The verifier-record axiom, made checkable (spec §4.1(a)): the
// parsed record is reconciled against an independent investigation
// of the same artifact, so a record-faithfulness drift fails here.
//= design/witness/spec.md#obligation-closedness
//= type=test
//# - Prover producers: closedness splits into
//# (a) the verifier's record faithfully reflects what elaboration
//# consulted — **axiom**, same category as trusting the verifier
//# itself — and
#[test]
fn corpus_reconciliation_with_findings_md() {
    // Lineage back to the SST POC: FINDINGS.md counted "189
    // FunctionSst nodes" with a `[A-Za-z0-9_:]` name regex. The 141
    // names it could not see are `impl&%N::` paths and similar.
    let simple = |name: &str| {
        name.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == ':')
    };
    assert_eq!(
        corpus().nodes.keys().filter(|n| simple(n)).count(),
        189,
        "unique names under the POC charset == FINDINGS.md's node count"
    );
}

#[test]
fn corpus_lemma_node_identity_and_extent() {
    let node = &corpus().nodes[LEMMA];
    assert_eq!(node.extent.file, "duvet-coverage/src/proofs.rs");
    assert_eq!((node.extent.start_line, node.extent.end_line), (54, 61));
}

// ---------------------------------------------------------------
// Real corpus: discrimination (spec §5.4 consulted closure)
// ---------------------------------------------------------------

#[test]
fn discrimination_lemma_closure_never_reaches_the_algorithm() {
    let c = closure(corpus(), LEMMA, is_project).unwrap();

    // The property (normative): a test annotation on this lemma must
    // never discharge a pair against the algorithm's implementation.
    assert!(
        !c.files
            .contains_key("duvet-coverage/src/annotation_execution.rs"),
        "lemma closure must not reach annotation_execution.rs"
    );

    // The goldens (pinned, superseding FINDINGS.md's regex-grade
    // 10 obligations / same file set — project count agrees here).
    assert_eq!(c.project_obligations.len(), 10);
    assert_eq!(c.reached.len(), 22);
    assert_eq!(
        file_line_counts(&c),
        [
            ("duvet-coverage/src/predicates.rs", 112),
            ("duvet-coverage/src/proofs.rs", 105),
        ]
    );
}

#[test]
fn discrimination_executed_variant_reaches_the_algorithm() {
    let c = closure(corpus(), EXECUTED, is_project).unwrap();

    // The property (normative).
    assert!(
        c.files
            .contains_key("duvet-coverage/src/annotation_execution.rs"),
        "executed-annotation closure must reach annotation_execution.rs"
    );

    // The goldens (pinned; FINDINGS.md's 18 obligations / 4 files
    // was truncated by the POC's charset — it could not follow
    // edges through `types::impl&%N::` obligations, whose closures
    // pull in execution_propagation.rs and types.rs).
    assert_eq!(c.project_obligations.len(), 25);
    assert_eq!(c.reached.len(), 70);
    assert_eq!(
        file_line_counts(&c),
        [
            ("duvet-coverage/src/annotation_execution.rs", 200),
            ("duvet-coverage/src/execution_propagation.rs", 471),
            ("duvet-coverage/src/predicates.rs", 155),
            ("duvet-coverage/src/proofs.rs", 162),
            ("duvet-coverage/src/target_resolution.rs", 119),
            ("duvet-coverage/src/types.rs", 16),
        ]
    );
}

// ---------------------------------------------------------------
// Real corpus: aggregate executability map (spec §5.2 pass 1)
// ---------------------------------------------------------------

// Delivered witnesses carry per-unit closures, each STRICTLY
// smaller than the aggregate on this corpus — a delivered
// aggregate-as-witness would fail the strict-dominance assertion.
//= design/witness/spec.md#verus-producer
//= type=test
//# - The aggregate executability map MUST NOT be delivered as a
//# witness: it is many obligations wearing one map, and delivering
//# it would violate [§4.2](#obligation-individuation) by
//# construction; it exists only as pass-1 scaffolding inside the
//# producer.
#[test]
fn aggregate_map_dominates_every_closure_strictly() {
    let graph = corpus();
    let agg = aggregate_map(graph, is_project);

    assert_eq!(agg.0.len(), 10, "all ten project source files");
    assert_eq!(total_lines(&agg.0), 1990);

    // Every closure is a projection of a reachable subset, so it
    // must embed in the aggregate; and no single obligation's
    // closure elaborates the whole crate, so containment is strict.
    for root in graph.nodes.keys() {
        let c = closure(graph, root, is_project).unwrap();
        for (file, lines) in &c.files {
            assert!(
                lines.is_subset(&agg.0[file]),
                "closure of {root} escapes the aggregate map"
            );
        }
        assert!(
            total_lines(&agg.0) > total_lines(&c.files),
            "aggregate must be strictly larger than the closure of {root}"
        );
    }
}

// ---------------------------------------------------------------
// Real corpus: witness construction (spec §5.2 pass 2, §5.5)
// ---------------------------------------------------------------

#[test]
fn witness_for_annotation_inside_lemma() {
    let graph = corpus();
    // An annotation resolved to proofs.rs:57 — inside the lemma's
    // extent (54..=61). Containment: exactly one witness.
    let ws = construct_witnesses(
        graph,
        "duvet-coverage/src/proofs.rs",
        57,
        "sst-poc",
        is_project,
    );
    let [w] = ws.as_slice() else {
        panic!("expected exactly one witness, got {}", ws.len())
    };
    assert_eq!(w.label, LEMMA);
    let ClaimRule::ByRootSpan {
        start_line,
        end_line,
        ..
    } = &w.claim
    else {
        panic!("prover witnesses claim by root span")
    };
    assert_eq!((*start_line, *end_line), (54, 61));
    assert_eq!(w.provenance.producer, "verus-sst");
    assert_eq!(w.provenance.strength, Strength::Consulted);
    assert_eq!(w.provenance.discharge_unit.as_deref(), Some(LEMMA));
    assert_eq!(
        w.files,
        hit_files(&closure(graph, LEMMA, is_project).unwrap().files)
    );
}

#[test]
fn no_discharge_unit_no_witness() {
    // Line 1 of proofs.rs is the license header: never elaborated,
    // outside every obligation. Zero witnesses AND classified
    // Unelaborated — this is W6 territory ("no witness from any
    // configured producer"), distinct from not-proof-testable.
    let graph = corpus();
    assert!(
        construct_witnesses(graph, "duvet-coverage/src/proofs.rs", 1, "x", is_project).is_empty()
    );
    assert_eq!(
        classify_position(graph, "duvet-coverage/src/proofs.rs", 1),
        PositionKind::Unelaborated
    );
    assert_eq!(
        classify_position(graph, "duvet-coverage/src/nonexistent.rs", 10),
        PositionKind::Unelaborated
    );
}

//= design/witness/spec.md#discharge-unit
//= type=test
//# Attribution MAY be ambiguous *within one specificity level*:
//# provers stamp generated obligations with the source range of the
//# declaration they were generated from,
//# so one proof-testable position can root several obligations at
//# the same level.
//= design/witness/spec.md#discharge-unit
//= type=test
//# When N obligations root a position at the chosen level, the
//# producer MUST deliver one witness per rooting obligation and
//# MUST NOT select among them (decisions.md, Decision 12).
#[test]
fn equal_extent_tie_yields_one_witness_per_rooting_obligation() {
    // The generated arrow-accessor pair on `types::ExecutionStatus`
    // shares the extent types.rs 154..=166 EXACTLY, so any line in
    // it roots both obligations. Per Decision 12 the producer
    // delivers BOTH witnesses — each with its own closure and
    // provenance, never selecting — and per Decision 14 an
    // annotation there is held to both.
    //
    // Line 167 (the enum's closing region, formerly attributed via
    // the deleted own-span fallback) is consulted-but-unrooted:
    // not proof-testable under Decision 13.
    let graph = corpus();
    let file = "duvet-coverage/src/types.rs";
    let ws = construct_witnesses(graph, file, 160, "sst-poc", is_project);
    let labels: Vec<&str> = ws.iter().map(|w| w.label.as_str()).collect();
    assert_eq!(
        labels,
        [
            "duvet_coverage::types::impl&%49::arrow_Unknown_line_number",
            "duvet_coverage::types::impl&%49::arrow_line_number",
        ],
        "all rooting obligations, deterministic order"
    );
    for w in &ws {
        assert_eq!(
            w.provenance.discharge_unit.as_deref(),
            Some(w.label.as_str())
        );
        assert_eq!(
            w.files,
            hit_files(&closure(graph, &w.label, is_project).unwrap().files),
            "each witness carries its own obligation's closure"
        );
    }

    assert_eq!(
        classify_position(graph, file, 167),
        PositionKind::NotProofTestable
    );
    assert!(construct_witnesses(graph, file, 167, "sst-poc", is_project).is_empty());
}

//= design/witness/spec.md#discharge-unit
//= type=test
//# Executable body lines MUST NOT be in the domain:
//# no obligation is rooted at a body line —
//# body lines are material a proof consults,
//# not claims a prover discharges —
//# so a test annotation there is category-mismatched,
//# and is reported *not proof-testable* rather than unwitnessed
//# (decisions.md, [Decision 13](decisions.md#decision-13)).
#[test]
fn body_line_is_not_proof_testable() {
    // Decision 13 golden (the redirect's named case): a test
    // annotation targeting `execution_set`'s BODY. Its extent —
    // the declaration, lines 62..=66 — roots the obligation; body
    // line 90 (`let directly_executed = ...`) is elaborated (it
    // appears in the aggregate) but roots nothing: proof
    // ingredient, not claim. The outcome is the distinct
    // not-proof-testable report, NOT a du result and NOT W6, and
    // zero witnesses.
    let graph = corpus();
    let file = "duvet-coverage/src/execution_propagation.rs";
    assert!(matches!(
        classify_position(graph, file, 62),
        PositionKind::Rooted(units)
            if units.iter().map(|u| u.node.name.as_str()).collect::<Vec<_>>()
                == ["duvet_coverage::execution_propagation::execution_set"]
    ));
    assert_eq!(
        classify_position(graph, file, 90),
        PositionKind::NotProofTestable
    );
    assert!(construct_witnesses(graph, file, 90, "sst-poc", is_project).is_empty());
    // A requires clause line: elaborated, but requires clauses are
    // not one of the four unit kinds (spec §5.3), so it roots
    // nothing either.
    assert_eq!(
        classify_position(graph, file, 68),
        PositionKind::NotProofTestable
    );
    // Compiler-generated overflow/bounds checks are stamped as
    // asserts with a diagnostic `Message` (not `None`): they sit
    // on executable body lines and are not user proof elements,
    // so they root nothing. annotation_execution.rs:197 carries a
    // generated index-bounds assert and stays out of the domain.
    assert_eq!(
        classify_position(graph, "duvet-coverage/src/annotation_execution.rs", 197),
        PositionKind::NotProofTestable
    );
}

//= design/witness/spec.md#discharge-unit
//= type=test
//# Its domain (`dom(du)`) MUST contain only positions where an
//# obligation is *rooted* — proof-element positions,
//# at every granularity the artifact demonstrably records
//# (decisions.md, Decisions 13, 17, 18):
//# fn/lemma headers (obligation extents), ensures clauses,
//# loop invariants, and proof asserts.
//= design/witness/spec.md#verus-producer
//= type=test
//# - The Verus producer MUST treat `FunctionSst` blocks as closure
//# nodes and `Fun :path` references as edges, and MUST support
//# discharge units of all four kinds: obligation extents,
//# `:enss` clause spans, `LoopInv` spans, and proof-assert spans
//# (decisions.md, Decision 18).
#[test]
fn du_map_domain_split_over_the_aggregate() {
    // Decisions 13, 18, 19 partition the 1990 elaborated project
    // lines exactly: 752 rooted (in dom(du)) and 1238
    // not-proof-testable (elaborated body/ingredient lines), zero
    // unelaborated. Winning-unit-kind breakdown of the rooted
    // lines: 230 extent, 150 ensures, 185 loop-invariant, 187
    // proof-assert. Cross-checked against an independent
    // computation (python, own S-expression reader, 2026-07-29);
    // the two agree exactly on all counts.
    // Multi-rooted positions: exactly the 13 lines of the
    // arrow-accessor pair's shared extent (types.rs 154..=166),
    // two extent units each — most-specific-wins does not disturb
    // Decision 12's tie handling.
    let graph = corpus();
    let agg = aggregate_map(graph, is_project);
    let (mut rooted, mut npt) = (0usize, 0usize);
    let mut by_kind: std::collections::BTreeMap<UnitKind, usize> = Default::default();
    let mut multi = Vec::new();
    for (file, lines) in &agg.0 {
        for &line in lines {
            match classify_position(graph, file, line) {
                PositionKind::Rooted(units) => {
                    rooted += 1;
                    *by_kind.entry(units[0].kind).or_default() += 1;
                    if units.len() > 1 {
                        multi.push((file.clone(), line, units.len()));
                    }
                }
                PositionKind::NotProofTestable => npt += 1,
                PositionKind::Unelaborated => {
                    panic!("{file}:{line} is in the aggregate but classified unelaborated")
                }
            }
        }
    }
    assert_eq!((rooted, npt), (752, 1238));
    assert_eq!(
        by_kind.into_iter().collect::<Vec<_>>(),
        [
            (UnitKind::Extent, 230),
            (UnitKind::Ensures, 150),
            (UnitKind::LoopInvariant, 185),
            (UnitKind::ProofAssert, 187),
        ]
    );
    let expected: Vec<(String, u32, usize)> = (154..=166)
        .map(|l| ("duvet-coverage/src/types.rs".to_string(), l, 2))
        .collect();
    assert_eq!(multi, expected);
}

// ---------------------------------------------------------------
// Real corpus: the annotations input is semantically inert
// (spec §1.7)
// ---------------------------------------------------------------

//= design/witness/spec.md#producer
//= type=test
//# Normatively: every delivered witness MUST be a function of
//# (artifact, obligation) only — identical regardless of which
//# annotation caused its materialization, carrying no annotation
//# identity
#[test]
fn annotation_independence_same_obligation_identical_witness() {
    // Two different annotation positions resolving to the same
    // obligation yield identical witness objects — a witness is a
    // function of (artifact, obligation) only, so materialization
    // for many annotations dedupes naturally. Witness equality here
    // is full structural equality (label, claim, provenance,
    // files), which is also the proof that no annotation identity
    // is embedded anywhere in the witness: if it were, these two
    // could not compare equal.
    let graph = corpus();
    // proofs.rs:55 and proofs.rs:60 — distinct positions, both
    // inside the lemma's extent (54..=61).
    let a = construct_witnesses(
        graph,
        "duvet-coverage/src/proofs.rs",
        55,
        "sst-poc",
        is_project,
    );
    let b = construct_witnesses(
        graph,
        "duvet-coverage/src/proofs.rs",
        60,
        "sst-poc",
        is_project,
    );
    assert_eq!(a.len(), 1);
    assert_eq!(a, b, "same obligation → byte-identical witnesses");
    assert_eq!(a[0].label, LEMMA);
}

//= design/witness/spec.md#producer
//= type=test
//# The `annotations` argument is a **semantically inert
//# optimization**, never a semantic input.
//# The witness universe is defined by the artifact alone —
//# conceptually one potential witness per obligation —
//# and the argument only selects which members are materialized,
//# so that producers need not close every obligation to serve a few
//# annotations.
//= design/witness/spec.md#producer
//= type=test
//# — and the filtering MUST be sound:
//# for every requested annotation, binding and discharge verdicts
//# over the materialized set MUST equal the verdicts over the full
//# universe.
//
// One witness per discharge unit, each the record of that unit's
// single checking act (labels unique over the 775-unit universe):
//= design/witness/spec.md#obligation-individuation
//= type=test
//# Every delivered witness MUST be the record of exactly one act of
//# checking.
#[test]
fn filter_soundness_annotations_only_select_from_the_universe() {
    // Spec §1.7: the witness universe is defined by the artifact
    // alone; the annotations input only selects which members get
    // materialized. The universe has one witness per discharge
    // unit (spec §5.3, Decision 18): 330 obligation extents plus
    // 445 clause-kind units (182 ensures clauses, 98 loop
    // invariants, 165 user proof asserts) = 775.
    //
    // Sweep every possible single-line annotation position (all
    // 1990 elaborated project lines) and check, at each, against
    // full-universe production:
    //
    //   1. every witness materialized for the position is
    //      byte-identical to the universe member with its label
    //      (production never invents or perturbs a witness), and
    //   2. the materialized set equals the most-specific level of
    //      the position's ByRootSpan binding set over the whole
    //      universe: clause-kind binders of minimal span if any
    //      exist, else extent binders of minimal span (rooting is
    //      most-specific-wins, spec §5.3, Decision 19 — a bound
    //      coarser witness is deliberately NOT materialized:
    //      hoisting to the enclosing function is refused, and ties
    //      at the chosen level all materialize, Decision 12).
    let graph = corpus();
    let universe = materialize_all(graph, "sst-poc", is_project);
    let units = all_units(graph);
    assert_eq!(universe.len(), units.len(), "one witness per unit");
    assert_eq!(universe.len(), 775, "330 extents + 445 clause units");
    let by_label: std::collections::BTreeMap<&str, &Witness> =
        universe.iter().map(|w| (w.label.as_str(), w)).collect();
    assert_eq!(by_label.len(), universe.len(), "labels are unique");

    // (kind, size, label) of every universe member whose root span
    // contains the position (spec §1.5 ByRootSpan binding for a
    // single-line target).
    let binders = |file: &str, line: u32| -> Vec<(UnitKind, u64, &str)> {
        units
            .iter()
            .zip(universe.iter())
            .filter(|(u, _)| u.span.contains(file, line))
            .map(|(u, w)| (u.kind, u.span.line_count(), w.label.as_str()))
            .collect()
    };

    let agg = aggregate_map(graph, is_project);
    for (file, lines) in &agg.0 {
        for &line in lines {
            let materialized = construct_witnesses(graph, file, line, "sst-poc", is_project);
            for w in &materialized {
                assert_eq!(
                    Some(w),
                    by_label.get(w.label.as_str()).copied(),
                    "{file}:{line}: materialized witness diverges from universe member"
                );
            }
            let all = binders(file, line);
            let clause: Vec<_> = all
                .iter()
                .filter(|(k, _, _)| *k != UnitKind::Extent)
                .collect();
            let level: Vec<_> = if clause.is_empty() {
                all.iter().collect()
            } else {
                clause
            };
            let min = level.iter().map(|(_, s, _)| *s).min();
            let mut expected: Vec<&str> = level
                .iter()
                .filter(|(_, s, _)| Some(*s) == min)
                .map(|(_, _, l)| *l)
                .collect();
            let mut materialized_labels: Vec<&str> =
                materialized.iter().map(|w| w.label.as_str()).collect();
            expected.sort_unstable();
            materialized_labels.sort_unstable();
            assert_eq!(
                materialized_labels, expected,
                "{file}:{line}: materialized set diverges from the most-specific \
                 level of the universe binding set"
            );
        }
    }
}

// ---------------------------------------------------------------
// Vacuity fixture: the honest boundary of consulted semantics
// (spec §5.4, Decision 7; testdata/vacuity/README.md)
// ---------------------------------------------------------------

fn vacuity() -> &'static ObligationGraph {
    static GRAPH: OnceLock<ObligationGraph> = OnceLock::new();
    GRAPH.get_or_init(|| {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/query/parsers/verus_sst/testdata/vacuity");
        load_dir(&dir).expect("vacuity fixture must load")
    })
}

fn vacuity_file(file: &str) -> bool {
    file == "vacuity.rs"
}

fn lines(c: &Closure) -> BTreeSet<u32> {
    c.files.get("vacuity.rs").cloned().unwrap_or_default()
}

#[test]
fn vacuity_fixture_shape() {
    let g = vacuity();
    assert_eq!(g.nodes.len(), 9, "6 vacuity fns + main + vstd nodes");
    for name in [
        "vacuity::spec_add_one",
        "vacuity::add_one",
        "vacuity::honest_proof",
        "vacuity::vacuous_proof",
        "vacuity::vacuous_proof_mentioning",
        "vacuity::self_contained",
        "vacuity::noted_loop",
    ] {
        assert!(g.nodes.contains_key(name), "missing {name}");
    }
}

#[test]
fn scenario_1_vacuous_proof_closure_is_itself_only() {
    // `requires x != x`, never mentions the implementation. Its
    // block has zero project edges, so the closure's project spans
    // are its own lines only and a pair against ANY impl annotation
    // fails to discharge. This is the vacuity defense consulted
    // semantics really does provide (FINDINGS.md hypothesis,
    // confirmed).
    let c = closure(vacuity(), "vacuity::vacuous_proof", vacuity_file).unwrap();
    assert_eq!(lines(&c), (32..=36).collect(), "own extent lines only");
    // Explicitly: no spec_add_one (9..=10), no add_one body, no
    // self_contained body. Closure *size* is not the signal —
    // project-span content is.
    assert!(!lines(&c).contains(&9));
    assert!(!lines(&c).contains(&17));
    assert!(!lines(&c).contains(&57));
}

//= design/witness/spec.md#closure
//= type=test
//# and the semantics is *consulted* (strength `Consulted`, [§1.3](#provenance)),
//# not load-bearing dependency.
#[test]
fn scenario_2_mention_without_need_is_credited() {
    // `requires x != x` but the ensures textually mentions
    // `spec_add_one`: elaboration recorded the mention even though
    // the solver proved from false. Under consulted semantics
    // (Decision 7, deliberately) this closure includes the spec fn,
    // so the pair IS credited. Pinned as the honest boundary: this
    // is the flagship case for needed-semantics strengthening, and
    // if that ever lands, this test is the one to flip.
    let c = closure(vacuity(), "vacuity::vacuous_proof_mentioning", vacuity_file).unwrap();
    assert!(
        c.reached.contains("vacuity::spec_add_one"),
        "the mention is followed"
    );
    assert_eq!(lines(&c), [9, 10, 43, 44, 45, 46, 47].into_iter().collect());
}

#[test]
fn scenario_3_self_inclusion_is_credited() {
    // Vacuous ensures, impl annotation inside the same fn's body
    // (line 57): discharge units carry their function's closure,
    // which self-includes the body, so the pair IS credited.
    // Catching this now requires needed semantics (Decision 7 —
    // clause-level units alone do not shrink the closure; spec
    // §5.4's closure-ceiling statement).
    //
    // `self_contained`'s extent is the declaration line alone
    // (54:1..54:41); the ensures clause (line 55) is its own
    // discharge unit now that :enss rooting has landed
    // (spec §5.3, Decision 18 — this is the test that flipped, as
    // its pre-flip text anticipated). The clause-rooted witness
    // carries the same function-level closure and still
    // self-includes body line 57.
    let g = vacuity();
    let ws = construct_witnesses(g, "vacuity.rs", 54, "fixture", vacuity_file);
    let [w] = ws.as_slice() else {
        panic!("expected exactly one witness, got {}", ws.len())
    };
    assert_eq!(w.label, "vacuity::self_contained");
    assert!(
        w.files["vacuity.rs"].contains_key(&57),
        "witness self-includes the body line carrying the impl annotation"
    );
    // The ensures line roots the clause unit — and ONLY the clause
    // unit: hoisting to the enclosing function is refused
    // (Decision 19), and the label is span identity, never
    // proof_note (Decision 20's ensures hazard rule).
    let ws = construct_witnesses(g, "vacuity.rs", 55, "fixture", vacuity_file);
    let [w55] = ws.as_slice() else {
        panic!("expected exactly one witness, got {}", ws.len())
    };
    assert_eq!(w55.label, "vacuity::self_contained ensures[0]");
    let ClaimRule::ByRootSpan {
        start_line,
        end_line,
        ..
    } = &w55.claim
    else {
        panic!("prover witnesses claim by root span")
    };
    assert_eq!((*start_line, *end_line), (55, 55));
    assert_eq!(
        w55.provenance.discharge_unit.as_deref(),
        Some("vacuity::self_contained")
    );
    assert_eq!(
        w55.files, w.files,
        "clause unit carries the function-level closure (spec §5.4 ceiling)"
    );
    assert!(
        w55.files["vacuity.rs"].contains_key(&57),
        "so the vacuous clause still self-includes the body: needed \
         semantics, not granularity, is what catches this"
    );
    // The body line itself: proof ingredient, never a test target.
    assert_eq!(
        classify_position(g, "vacuity.rs", 57),
        PositionKind::NotProofTestable
    );
}

#[test]
fn honest_proof_reaches_the_implementation_spec() {
    let c = closure(vacuity(), "vacuity::honest_proof", vacuity_file).unwrap();
    assert!(c.reached.contains("vacuity::spec_add_one"));
    assert_eq!(lines(&c), [9, 10, 24, 25, 26, 27].into_iter().collect());
}

// ---------------------------------------------------------------
// Vacuity fixture: unit labels from the artifact (spec §5.5,
// Decision 20)
// ---------------------------------------------------------------

//= design/witness/spec.md#verus-producer
//= type=test
//# Unit labels (decisions.md, Decision 20): `ProofNoteLabel` text
//# when the artifact records it, otherwise span identity
//# (function path + unit kind + clause index).
//= design/witness/spec.md#verus-producer
//= type=test
//# Until the upstream `proof_note`-on-ensures defect is fixed,
//# ensures-clause labels MUST come from span identity.
#[test]
fn labels_proof_note_when_recorded_span_identity_otherwise() {
    // `noted_loop` carries a proof_note'd invariant ("i stays
    // bounded", lines 72..=73 — the attribute line is part of the
    // recorded span) alongside an unnoted one (line 74), and a
    // proof_note'd assert ("loop exit bound", 80..=81) alongside an
    // unnoted one (line 82). Each label comes from the artifact:
    // ProofNoteLabel text when recorded, span identity otherwise —
    // never from duvet reading source text.
    let g = vacuity();
    let label_at = |line: u32| -> Vec<String> {
        construct_witnesses(g, "vacuity.rs", line, "fixture", vacuity_file)
            .into_iter()
            .map(|w| w.label)
            .collect()
    };
    assert_eq!(label_at(73), ["i stays bounded"]);
    assert_eq!(label_at(74), ["vacuity::noted_loop loop_invariant[1]"]);
    assert_eq!(label_at(81), ["loop exit bound"]);
    assert_eq!(label_at(82), ["vacuity::noted_loop proof_assert[1]"]);
    // The ensures clause (line 67): span identity even though loop
    // invariants and asserts in the same fn carry notes — the
    // upstream proof_note-on-ensures defect makes note consumption
    // for ensures a build breaker, so the producer never reads it
    // (Decision 20 hazard rule).
    assert_eq!(label_at(67), ["vacuity::noted_loop ensures[0]"]);
    // The loop header line (70): excluded from dom(du) — the
    // invariant clauses are units; the `while` line is not.
    assert_eq!(
        classify_position(g, "vacuity.rs", 70),
        PositionKind::NotProofTestable
    );
}

// ---------------------------------------------------------------
// Closure reflexivity at every unit grain (spec §5.4)
// ---------------------------------------------------------------

#[test]
fn every_constructed_witness_contains_its_own_root_span() {
    //= design/witness/spec.md#closure
    //= type=test
    //# The closure MUST be **reflexive**: it includes the discharge
    //# unit's own root span (`root ∈ closure(root)`), so that a witness
    //# always scores its own annotation as executed and `ByRootSpan`
    //# binding implies execution ([§1.5](#claim-rules)'s claim rules are thereby
    //# instances of one predicate; [Property W5](#property-w5-claim-refinement)'s refinement claim
    //# depends on this).

    //= design/witness/spec.md#closure
    //# Producers MUST carry a unit test asserting reflexivity for every
    //# constructed witness.
    //= design/witness/spec.md#closure
    //= type=test
    //# Producers MUST carry a unit test asserting reflexivity for every
    //# constructed witness.
    // This is that test, over the full universe of both artifact
    // sets, at clause grain (every unit kind, not just extents).
    for (graph, project) in [
        (corpus(), is_project as fn(&str) -> bool),
        (vacuity(), vacuity_file as fn(&str) -> bool),
    ] {
        for w in materialize_all(graph, "x", project) {
            let ClaimRule::ByRootSpan {
                file,
                start_line,
                end_line,
            } = &w.claim
            else {
                panic!("prover witnesses claim by root span")
            };
            if !project(file) {
                continue; // vstd-rooted units: projected out
            }
            let lines = w.files.get(file).unwrap_or_else(|| {
                panic!("{}: root file {file} missing from witness map", w.label)
            });
            for line in *start_line..=*end_line {
                assert!(
                    lines.contains_key(&line),
                    "{}: root line {file}:{line} not in own closure",
                    w.label,
                );
            }
        }
    }
}

// ---------------------------------------------------------------
// Integration-toml fixture parity (the embedded-copy divergence
// class)
// ---------------------------------------------------------------

/// Extracts the embedded `vacuity.rs` virtual-file contents from an
/// integration toml. String-based on purpose: the duvet crate has no
/// TOML dependency, and the tomls under `integration/` are our own,
/// with a fixed shape (no escape sequences inside the block).
fn embedded_vacuity<'a>(toml_text: &'a str, toml_name: &str) -> Vec<&'a str> {
    let marker = "path = \"vacuity.rs\"";
    let start = toml_text
        .find(marker)
        .unwrap_or_else(|| panic!("{toml_name}: no vacuity.rs virtual file"));
    let open = "contents = \"\"\"\n";
    let start = toml_text[start..]
        .find(open)
        .map(|i| start + i + open.len())
        .unwrap_or_else(|| panic!("{toml_name}: vacuity.rs entry has no contents block"));
    let end = toml_text[start..]
        .find("\"\"\"")
        .map(|i| start + i)
        .unwrap_or_else(|| panic!("{toml_name}: unterminated contents block"));
    toml_text[start..end].lines().collect()
}

/// Duvet-annotation runs in an embedded source and the code line each
/// one resolves to: a run is a maximal block of `//=`/`//#` lines; the
/// target is the first non-comment line after it (plain `//` filler
/// lines are skipped, matching duvet's resolution).
fn annotation_targets(lines: &[&str]) -> BTreeSet<u32> {
    let is_annotation = |l: &str| {
        let l = l.trim_start();
        l.starts_with("//=") || l.starts_with("//#")
    };
    let mut targets = BTreeSet::new();
    let mut i = 0;
    while i < lines.len() {
        if is_annotation(lines[i]) {
            while i < lines.len() && is_annotation(lines[i]) {
                i += 1;
            }
            while i < lines.len() && lines[i].trim_start().starts_with("//") {
                i += 1;
            }
            if i < lines.len() {
                targets.insert(i as u32 + 1);
            }
        } else {
            i += 1;
        }
    }
    targets
}

#[test]
fn integration_tomls_embed_line_true_copies_of_the_fixture() {
    // The witness integration tomls embed copies of the vacuity
    // fixture's source with line-count-preserving annotation swaps,
    // because the checked-in SST log's spans are line-keyed against
    // testdata/vacuity/vacuity.rs. The duplication is a divergence
    // class: regenerating the fixture shifts every span and silently
    // strands the embedded annotations on unrooted lines — the
    // integration snapshots then fail with a cryptic verdict diff
    // (this broke PR #247's CI when the fixture gained its copyright
    // header and scenario D). This test pins the class in the unit
    // suite, where the failure message says exactly what drifted:
    //
    //   1. line-count parity between each embedded copy and the
    //      fixture;
    //   2. every embedded line is byte-identical to the fixture's
    //      same-numbered line, or is a comment line (the only legal
    //      swap);
    //   3. the embedded annotations resolve to exactly the target
    //      lines each toml's documented intent names, those targets
    //      carry the same code as the fixture, and the SST classifies
    //      each one as the toml expects (rooted binder, closure
    //      membership, or not-proof-testable).
    enum Expect {
        /// Test-annotation target: binds exactly this witness.
        RootedWitness(&'static str),
        /// Impl-annotation target: credited by this unit's closure.
        InClosureOf(&'static str),
        /// Test-annotation target on an executable body line.
        NotProofTestable,
    }
    use Expect::*;

    let common: &[(u32, &str, Expect)] = &[
        (
            9,
            "pub open spec fn spec_add_one",
            InClosureOf("vacuity::honest_proof"),
        ),
        (
            24,
            "pub proof fn honest_proof",
            RootedWitness("vacuity::honest_proof"),
        ),
    ];
    let npt_only: &[(u32, &str, Expect)] = &[
        (
            32,
            "pub proof fn vacuous_proof(",
            InClosureOf("vacuity::vacuous_proof"),
        ),
        (
            54,
            "pub fn self_contained",
            RootedWitness("vacuity::self_contained"),
        ),
        (57, "    let y = x;", NotProofTestable),
    ];

    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fixture = std::fs::read_to_string(
        root.join("src/query/parsers/verus_sst/testdata/vacuity/vacuity.rs"),
    )
    .expect("fixture source must be readable");
    let fixture: Vec<&str> = fixture.lines().collect();
    let g = vacuity();

    for (name, extra) in [
        ("query-witness-mixed-producers", false),
        ("query-witness-verus-discharge", false),
        ("query-witness-not-proof-testable", true),
    ] {
        let toml_path = root.join(format!("../integration/{name}.toml"));
        let toml_text = std::fs::read_to_string(&toml_path)
            .unwrap_or_else(|e| panic!("{}: {e}", toml_path.display()));
        let embedded = embedded_vacuity(&toml_text, name);

        assert_eq!(
            embedded.len(),
            fixture.len(),
            "{name}: embedded vacuity.rs lost line-count parity with the \
             fixture — its annotations no longer land where the SST's \
             spans root; re-author the embedded copy against the current \
             fixture layout"
        );
        for (i, (e, f)) in embedded.iter().zip(fixture.iter()).enumerate() {
            assert!(
                e == f || e.trim_start().starts_with("//"),
                "{name}: line {}: embedded copy diverges from the fixture \
                 with non-comment content\n  embedded: {e}\n  fixture:  {f}",
                i + 1
            );
        }

        let expectations = common.iter().chain(if extra { npt_only } else { &[] });
        let expected_targets: BTreeSet<u32> = common
            .iter()
            .chain(if extra { npt_only } else { &[] })
            .map(|(line, ..)| *line)
            .collect();
        assert_eq!(
            annotation_targets(&embedded),
            expected_targets,
            "{name}: the embedded annotations do not resolve to the target \
             lines the toml's intent documents"
        );

        for (line, prefix, expect) in expectations {
            let idx = (*line - 1) as usize;
            assert_eq!(
                embedded[idx], fixture[idx],
                "{name}: target line {line} differs from the fixture"
            );
            assert!(
                embedded[idx].starts_with(prefix),
                "{name}: target line {line} is not the intended code: \
                 {:?} (expected prefix {prefix:?})",
                embedded[idx]
            );
            match expect {
                RootedWitness(label) => {
                    let ws = construct_witnesses(g, "vacuity.rs", *line, "guard", vacuity_file);
                    let [w] = ws.as_slice() else {
                        panic!(
                            "{name}: line {line}: expected exactly one \
                             witness, got {}",
                            ws.len()
                        )
                    };
                    assert_eq!(w.label, *label, "{name}: line {line} binder");
                }
                InClosureOf(unit) => {
                    let c = closure(g, unit, vacuity_file).unwrap();
                    assert!(
                        lines(&c).contains(line),
                        "{name}: line {line} is no longer in {unit}'s \
                         closure — the impl side of the pair is stranded"
                    );
                }
                NotProofTestable => {
                    assert_eq!(
                        classify_position(g, "vacuity.rs", *line),
                        PositionKind::NotProofTestable,
                        "{name}: line {line} must be not-proof-testable"
                    );
                }
            }
        }
    }
}

// ---------------------------------------------------------------
// Perf measurement harness (not correctness tests; run explicitly:
//   cargo test --release -p duvet verus_sst::tests::bench \
//     -- --ignored --nocapture
// ) — reports wall time over the checked-in corpus so optimization
// claims carry before/after numbers.
// ---------------------------------------------------------------

// Every unit's witness equals a fresh closure of its FUNCTION root:
// units inside one function share the identical function-level
// consulted closure — the artifact's demonstrated ceiling.
//= design/witness/spec.md#closure
//= type=test
//# **The closure ceiling MUST be stated per producer, because it
//# bounds what unit granularity buys.** Where a prover checks a
//# function's obligations in one solver query and assumes callee
//# contracts whole (Verus does both, §5.5), every discharge unit
//# inside a function carries the identical function-level consulted
//# closure: finer units buy precise identity and legible failures,
//# not smaller witnesses, and discharge verdicts within one function
//# do not differ across its units at Consulted strength.
//= design/witness/spec.md#closure
//= type=test
//# The closure MUST be computed at the finest granularity the
//# artifact demonstrably supports
//# (decisions.md, [Decision 17](decisions.md#decision-17) — deferral is legitimate only at the
//# artifact's ceiling or across a named architectural boundary);
//# what is normative now:
#[test]
fn memoized_universe_matches_fresh_per_unit_closures() {
    // Equivalence anchor for the closure memoization: every witness
    // in the materialized universe must carry exactly the files an
    // independent, unmemoized `closure()` call computes for its
    // unit's root. `closure` is pure in (graph, root, project)
    // (spec §5.4 — the closure is *the* fixpoint), so any divergence
    // here is a memoization bug, full stop.
    let graph = corpus();
    let universe = materialize_all(graph, "eq", is_project);
    let units = all_units(graph);
    assert_eq!(universe.len(), units.len(), "one witness per unit");
    for (w, u) in universe.iter().zip(units.iter()) {
        assert_eq!(w.label, u.label, "universe order is unit order");
        let fresh = closure(graph, &u.node.name, is_project).expect("root must exist");
        assert_eq!(
            w.files,
            hit_files(&fresh.files),
            "memoized closure for unit {} (root {}) diverged from a \
             fresh computation",
            u.label,
            u.node.name
        );
    }
}

#[test]
#[ignore = "perf harness, run explicitly with --ignored --nocapture"]
fn bench_merge_unit_heavy_relogged_nodes() {
    // The structure.rs merge unit-union path at adversarial scale: one
    // node with 500 clause units, re-logged in 10 modules (the corpus
    // itself has few units per node, so this path is invisible in
    // bench_load_dir_corpus). Old shape: contains() + re-sort per
    // duplicate — O(units²) scans.
    use super::structure::{ClauseUnit, ObligationNode, Span, UnitKind};
    use std::collections::BTreeMap;
    let node = || {
        let units: Vec<ClauseUnit> = (0..500)
            .map(|i| ClauseUnit {
                kind: UnitKind::ProofAssert,
                index: i,
                span: Span {
                    file: "src/a.rs".into(),
                    start_line: 10 + i as u32,
                    end_line: 10 + i as u32,
                },
                note: None,
            })
            .collect();
        ObligationNode {
            name: "c::heavy".into(),
            extent: Span {
                file: "src/a.rs".into(),
                start_line: 1,
                end_line: 1000,
            },
            units,
            spans: BTreeMap::new(),
            edges: std::collections::BTreeSet::new(),
        }
    };
    let iters = 20u32;
    let start = std::time::Instant::now();
    for _ in 0..iters {
        let modules: Vec<Vec<ObligationNode>> = (0..10).map(|_| vec![node()]).collect();
        std::hint::black_box(ObligationGraph::merge(modules).unwrap());
    }
    let total = start.elapsed();
    println!(
        "bench_merge_unit_heavy: units=500 relogs=10 iters={} total={:?} per-iter={:?}",
        iters,
        total,
        total / iters
    );
}

#[test]
#[ignore = "perf harness, run explicitly with --ignored --nocapture"]
fn bench_load_dir_corpus() {
    // Decompress + parse + merge of the checked-in 10-module corpus:
    // covers the structure.rs merge path and the one-text-at-a-time
    // load restructuring.
    let dir = corpus_dir();
    let g = load_dir(&dir).expect("corpus must load");
    let iters = 10u32;
    let start = std::time::Instant::now();
    for _ in 0..iters {
        std::hint::black_box(load_dir(&dir).expect("corpus must load"));
    }
    let total = start.elapsed();
    println!(
        "bench_load_dir_corpus: nodes={} iters={} total={:?} per-iter={:?}",
        g.nodes.len(),
        iters,
        total,
        total / iters
    );
}

#[test]
#[ignore = "perf harness, run explicitly with --ignored --nocapture"]
fn bench_materialize_all() {
    let graph = corpus();
    // Warmup + shape sanity.
    let universe = materialize_all(graph, "bench", is_project);
    let units = all_units(graph).len();
    let iters = 20u32;
    let start = std::time::Instant::now();
    for _ in 0..iters {
        std::hint::black_box(materialize_all(graph, "bench", is_project));
    }
    let total = start.elapsed();
    println!(
        "bench_materialize_all: nodes={} units={} witnesses={} iters={} total={:?} per-iter={:?}",
        graph.nodes.len(),
        units,
        universe.len(),
        iters,
        total,
        total / iters
    );
}

#[test]
#[ignore = "perf harness, run explicitly with --ignored --nocapture"]
fn bench_construct_witnesses_all_rooted_positions() {
    // Simulates annotation-driven production: one construct call per
    // rooted position in the corpus (every unit start line), the
    // worst case for per-position recomputation.
    let graph = corpus();
    let positions: Vec<(String, u32)> = all_units(graph)
        .iter()
        .map(|u| (u.span.file.clone(), u.span.start_line))
        .collect();
    let iters = 20u32;
    let start = std::time::Instant::now();
    for _ in 0..iters {
        for (file, line) in &positions {
            std::hint::black_box(construct_witnesses(graph, file, *line, "bench", is_project));
        }
    }
    let total = start.elapsed();
    println!(
        "bench_construct_witnesses: positions={} iters={} total={:?} per-iter={:?}",
        positions.len(),
        iters,
        total,
        total / iters
    );
}
