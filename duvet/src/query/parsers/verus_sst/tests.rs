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
//!    `DUVET_SST_LOG_DIR`, defaulting to the shared POC dir; the
//!    check-in strategy for these logs is decided at PR time.
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
    closure::{aggregate_map, closure, Closure},
    load_dir,
    structure::{parse_module, ObligationGraph},
    witness::{
        classify_position, construct_witnesses, materialize_all, ClaimRule, PositionKind, Strength,
    },
};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::OnceLock,
};

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

#[test]
fn corpus_node_counts() {
    // Raw per-module block count, before deduplication: imported
    // declarations are re-logged per module, so blocks > unique.
    let mut raw_blocks = 0usize;
    for entry in std::fs::read_dir(corpus_dir()).unwrap() {
        let path = entry.unwrap().path();
        let Some(p) = path.to_str() else { continue };
        let text = if p.ends_with("-sst.vir") {
            std::fs::read_to_string(&path).unwrap()
        } else if p.ends_with("-sst.vir.gz") {
            use std::io::Read;
            let mut text = String::new();
            flate2::read::GzDecoder::new(std::fs::File::open(&path).unwrap())
                .read_to_string(&mut text)
                .unwrap();
            text
        } else {
            continue;
        };
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

#[test]
fn aggregate_map_dominates_every_closure_strictly() {
    let graph = corpus();
    let agg = aggregate_map(graph, is_project);

    assert_eq!(agg.0.len(), 10, "all ten project source files");
    assert_eq!(agg.total_lines(), 1990);

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
            agg.total_lines() > c.total_lines(),
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
    let ClaimRule::ByRootSpan(root) = &w.claim;
    assert_eq!((root.start_line, root.end_line), (54, 61));
    assert_eq!(w.provenance.producer, "verus-sst");
    assert_eq!(w.provenance.strength, Strength::Consulted);
    assert_eq!(w.provenance.discharge_unit.as_deref(), Some(LEMMA));
    assert_eq!(w.files, closure(graph, LEMMA, is_project).unwrap().files);
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
            closure(graph, &w.label, is_project).unwrap().files,
            "each witness carries its own obligation's closure"
        );
    }

    assert_eq!(
        classify_position(graph, file, 167),
        PositionKind::NotProofTestable
    );
    assert!(construct_witnesses(graph, file, 167, "sst-poc", is_project).is_empty());
}

#[test]
fn body_line_is_not_proof_testable() {
    // Decision 13 golden (the redirect's named case): a test
    // annotation targeting `execution_set`'s BODY. Its extent —
    // the declaration, lines 62..=66 — roots the obligation; body
    // line 68 is elaborated (it appears in the aggregate) but roots
    // nothing: proof ingredient, not claim. The outcome is the
    // distinct not-proof-testable report, NOT a du result and NOT
    // W6, and zero witnesses. (The old own-span fallback attributed
    // this line to execution_set and built a witness the
    // annotation could not bind — Decision 13 deleted it.)
    let graph = corpus();
    let file = "duvet-coverage/src/execution_propagation.rs";
    assert!(matches!(
        classify_position(graph, file, 62),
        PositionKind::Rooted(units)
            if units.iter().map(|n| n.name.as_str()).collect::<Vec<_>>()
                == ["duvet_coverage::execution_propagation::execution_set"]
    ));
    assert_eq!(
        classify_position(graph, file, 68),
        PositionKind::NotProofTestable
    );
    assert!(construct_witnesses(graph, file, 68, "sst-poc", is_project).is_empty());
    // Same shape for the declaration-only exec fn that motivated
    // the old fallback: its 42 body lines are now out of domain.
    assert_eq!(
        classify_position(graph, "duvet-coverage/src/annotation_execution.rs", 190),
        PositionKind::NotProofTestable
    );
}

#[test]
fn du_map_domain_split_over_the_aggregate() {
    // Decision 13 partitions the 1990 elaborated project lines
    // exactly: 235 rooted (in dom(du)) and 1755 not-proof-testable
    // (elaborated body/ingredient lines), zero unelaborated —
    // pinned so a rooting change (e.g. :enss/LoopInv landing, which
    // may only move lines NPT → rooted) is a visible diff here, and
    // the refuse-body-lines boundary is exact from the start.
    // Multi-rooted positions: exactly the 13 lines of the
    // arrow-accessor pair's shared extent (types.rs 154..=166),
    // two obligations each.
    let graph = corpus();
    let agg = aggregate_map(graph, is_project);
    let (mut rooted, mut npt) = (0usize, 0usize);
    let mut multi = Vec::new();
    for (file, lines) in &agg.0 {
        for &line in lines {
            match classify_position(graph, file, line) {
                PositionKind::Rooted(units) => {
                    rooted += 1;
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
    assert_eq!((rooted, npt), (235, 1755));
    let expected: Vec<(String, u32, usize)> = (154..=166)
        .map(|l| ("duvet-coverage/src/types.rs".to_string(), l, 2))
        .collect();
    assert_eq!(multi, expected);
}

// ---------------------------------------------------------------
// Real corpus: the annotations input is semantically inert
// (spec §1.7)
// ---------------------------------------------------------------

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

#[test]
fn filter_soundness_annotations_only_select_from_the_universe() {
    // Spec §1.7: the witness universe is defined by the artifact
    // alone; the annotations input only selects which members get
    // materialized. Sweep every possible single-line annotation
    // position (all 1990 elaborated project lines) and check, at
    // each, against full-universe production:
    //
    //   1. every witness materialized for the position is
    //      byte-identical to the universe member with its label
    //      (production never invents or perturbs a witness), and
    //   2. the position's ByRootSpan binding set over the
    //      materialized witnesses equals its binding set over the
    //      whole universe (filtering loses no binder — this is
    //      what forced containment ties to yield ALL tied units;
    //      a smallest-extent tiebreak silently dropped a binder
    //      for types.rs lines 154..=166).
    let graph = corpus();
    let universe = materialize_all(graph, "sst-poc", is_project);
    assert_eq!(
        universe.len(),
        graph.nodes.len(),
        "one witness per obligation"
    );
    let by_label: std::collections::BTreeMap<&str, &super::witness::VerusWitness> =
        universe.iter().map(|w| (w.label.as_str(), w)).collect();
    assert_eq!(by_label.len(), universe.len(), "labels are unique");

    let binds = |w: &super::witness::VerusWitness, file: &str, line: u32| {
        // Single-line resolved target: T binds w iff the target
        // lies inside w's root span (spec §1.5, ByRootSpan).
        let ClaimRule::ByRootSpan(root) = &w.claim;
        root.contains(file, line)
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
            let mut universe_binders: Vec<&str> = universe
                .iter()
                .filter(|w| binds(w, file, line))
                .map(|w| w.label.as_str())
                .collect();
            let mut materialized_binders: Vec<&str> = materialized
                .iter()
                .filter(|w| binds(w, file, line))
                .map(|w| w.label.as_str())
                .collect();
            universe_binders.sort_unstable();
            materialized_binders.sort_unstable();
            assert_eq!(
                universe_binders, materialized_binders,
                "{file}:{line}: binding over materialized set diverges from full universe"
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
    assert_eq!(g.nodes.len(), 8, "5 vacuity fns + main + vstd nodes");
    for name in [
        "vacuity::spec_add_one",
        "vacuity::add_one",
        "vacuity::honest_proof",
        "vacuity::vacuous_proof",
        "vacuity::vacuous_proof_mentioning",
        "vacuity::self_contained",
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
    assert_eq!(lines(&c), (30..=34).collect(), "own extent lines only");
    // Explicitly: no spec_add_one (7..=8), no add_one body, no
    // self_contained body. Closure *size* is not the signal —
    // project-span content is.
    assert!(!lines(&c).contains(&7));
    assert!(!lines(&c).contains(&15));
    assert!(!lines(&c).contains(&55));
}

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
    assert_eq!(lines(&c), [7, 8, 41, 42, 43, 44, 45].into_iter().collect());
}

#[test]
fn scenario_3_self_inclusion_is_credited() {
    // Vacuous ensures, impl annotation inside the same fn's body
    // (line 55): whole-fn discharge units self-include the body, so
    // the pair IS credited. Catching this requires clause-level
    // units AND needed semantics — both out of scope (Decision 7).
    //
    // `self_contained`'s extent is the declaration line alone
    // (52:1..52:41), so under Decision 13 the header line 52 is
    // the proof-testable position. The ensures line 53 is currently
    // not-proof-testable — it joins dom(du) when :enss clause
    // rooting lands (which may only move it NPT → Rooted), at
    // which point the classification assertion below is the test
    // that flips.
    let g = vacuity();
    let ws = construct_witnesses(g, "vacuity.rs", 52, "fixture", vacuity_file);
    let [w] = ws.as_slice() else {
        panic!("expected exactly one witness, got {}", ws.len())
    };
    assert_eq!(w.label, "vacuity::self_contained");
    assert!(
        w.files["vacuity.rs"].contains(&55),
        "witness self-includes the body line carrying the impl annotation"
    );
    assert_eq!(
        classify_position(g, "vacuity.rs", 53),
        PositionKind::NotProofTestable,
        "ensures line: rooted only once :enss rooting lands"
    );
    // The body line itself: proof ingredient, never a test target.
    assert_eq!(
        classify_position(g, "vacuity.rs", 55),
        PositionKind::NotProofTestable
    );
}

#[test]
fn honest_proof_reaches_the_implementation_spec() {
    let c = closure(vacuity(), "vacuity::honest_proof", vacuity_file).unwrap();
    assert!(c.reached.contains("vacuity::spec_add_one"));
    assert_eq!(lines(&c), [7, 8, 22, 23, 24, 25].into_iter().collect());
}
