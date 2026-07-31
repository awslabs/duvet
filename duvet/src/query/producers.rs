// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Coverage producers: the boundary at which artifacts become
//! witnesses (design/witness/spec.md §1.7). The engine consumes
//! only `Vec<Witness>`; no producer-internal format escapes this
//! module.
//!
//! Trusted-base note (spec §4): producers are unverified glue at
//! the trust boundary. The runtime arm's closedness and
//! individuation are named axioms (§4.1/§4.2); the prover arm's
//! closure computation is `verus_sst`'s code, golden-tested there.
//! This module is unit-tested per §4.3.

use super::{
    coverage::CoverageParser,
    parsers::{verus_sst, JacocoParser},
    witness::{ClaimRule, Provenance, Strength, Witness},
};
use crate::Result;
use duvet_core::diagnostic::IntoDiagnostic;
use glob::glob;
use std::collections::BTreeMap;

/// Which producer interprets a source's artifacts (Decision 10:
/// a coverage source is a (producer, artifacts) pair, declared
/// repeatably; N producers is N declarations).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CoverageProducer {
    /// JaCoCo XML report files. One report file → one witness.
    JacocoXml,
    /// Verus `--log vir-sst` log directories. One log dir → one
    /// witness per discharge unit that certifies a requested
    /// position (spec §5).
    VerusSst,
}

/// The legacy `-f`/`--coverage-format` vocabulary (Decision 10: the
/// `-r`/`-f` pair is the one-source degenerate shorthand for
/// `--coverage-source`). Report formats only — prover producers take
/// log directories, not report files, so they are deliberately not
/// accepted here.
#[derive(Clone, Debug, clap::ValueEnum)]
pub enum CoverageFormat {
    JacocoXml,
    // Future: Lcov, Clover
}

impl CoverageFormat {
    /// The producer that interprets reports of this format.
    pub fn producer(&self) -> CoverageProducer {
        match self {
            CoverageFormat::JacocoXml => CoverageProducer::JacocoXml,
        }
    }
}

impl CoverageProducer {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "jacoco-xml" => Some(Self::JacocoXml),
            "verus-sst" => Some(Self::VerusSst),
            _ => None,
        }
    }

    fn name(&self) -> &'static str {
        match self {
            Self::JacocoXml => "jacoco",
            Self::VerusSst => "verus-sst",
        }
    }
}

//= design/witness/spec.md#producer
//= type=implementation
//# A producer maps declared artifacts to witnesses:
//#
//# ```
//# produce : (artifacts, annotations) → Vec<Witness>
//# ```
#[derive(Clone, Debug)]
pub struct CoverageSource {
    pub producer: CoverageProducer,
    /// Artifact paths or globs (report files for runtime producers;
    /// log directories for verus-sst).
    pub globs: Vec<String>,
}

/// A test annotation's resolved target in engine coordinates: the
/// absolute path of its source file plus the target line the verified
/// target resolution produced (spec §1.1). This is also the shape a
/// prover producer is asked to witness — positions, not annotations,
/// cross that boundary:
//= design/witness/spec.md#producer
//# Normatively: every delivered witness MUST be a function of
//# (artifact, obligation) only — identical regardless of which
//# annotation caused its materialization, carrying no annotation
//# identity
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RequestedPosition {
    pub absolute_file: String,
    pub line: u64,
}

/// What one declared source produced: its witnesses, plus the
/// pass-1 facts that are not witnesses.
//= design/witness/spec.md#producer-obligations
//# **producers deliver facts and never render verdicts.**
//# Delivering zero witnesses for an annotation is a fact
//# (possibly with a reason attached, [§5.2](#two-pass-construction)'s not-proof-testable),
//# not a failure
#[derive(Debug, Default)]
pub struct Produced {
    pub witnesses: Vec<Witness>,
    //= design/witness/spec.md#two-pass-construction
    //= type=implementation
    //# If a live annotation's resolved target is not proof-testable,
    //# the producer MUST deliver no witness for it,
    //# and the report MUST identify the annotation as
    //# *not proof-testable*
    /// Requested positions a prover artifact elaborated but which
    /// root no obligation (Decision 13): proof ingredients, not
    /// claims. No witness exists for them by definition; the report
    /// identifies them distinctly from Property W6's "no witness
    /// from any configured producer."
    pub not_proof_testable: Vec<RequestedPosition>,
}

impl Produced {
    fn merge(&mut self, other: Produced) {
        self.witnesses.extend(other.witnesses);
        self.not_proof_testable.extend(other.not_proof_testable);
    }
}

/// Expand artifact globs to concrete paths, preserving declaration
/// order (glob results are sorted within each pattern).
pub fn expand_globs(patterns: &[String]) -> Result<Vec<String>> {
    let mut expanded = Vec::new();
    for pattern in patterns {
        for entry in glob(pattern).into_diagnostic()? {
            let path = entry.into_diagnostic()?;
            expanded.push(path.to_string_lossy().to_string());
        }
    }
    Ok(expanded)
}

/// Produce the witnesses of one declared source.
///
/// `positions` is spec §1.7's `annotations` argument: runtime
/// producers ignore it (their witnesses pre-exist in the artifact);
/// the prover arm uses it.
//= design/witness/spec.md#producer
//# The `annotations` argument is a **semantically inert
//# optimization**, never a semantic input.
//# The witness universe is defined by the artifact alone —
//# conceptually one potential witness per obligation —
//# and the argument only selects which members are materialized,
//# so that producers need not close every obligation to serve a few
//# annotations.
pub async fn produce(
    source: &CoverageSource,
    positions: &[RequestedPosition],
    path_matches: impl Fn(&str, &str) -> bool,
    project: impl Fn(&str) -> bool,
) -> Result<Produced> {
    let artifacts = expand_globs(&source.globs)?;
    if artifacts.is_empty() {
        return Err(duvet_core::error!(
            "coverage source ({}) matched no artifacts: {}",
            source.producer.name(),
            source.globs.join(", ")
        ));
    }
    match source.producer {
        CoverageProducer::JacocoXml => {
            let mut produced = Produced::default();
            for artifact in &artifacts {
                produced.witnesses.push(jacoco_witness(artifact).await?);
            }
            Ok(produced)
        }
        CoverageProducer::VerusSst => {
            let mut produced = Produced::default();
            for artifact in &artifacts {
                let graph = verus_sst::load_dir(std::path::Path::new(artifact))
                    .map_err(|e| duvet_core::error!("verus-sst: {e}"))?;
                produced.merge(verus_witnesses_from_graph(
                    &graph,
                    artifact,
                    positions,
                    &path_matches,
                    &project,
                )?);
            }
            Ok(produced)
        }
    }
}

/// Wrap one JaCoCo report file as one witness: claim `ByExecution`,
/// strength `Executed`, per-file maps exactly as the report states
/// them (Decision 2: JaCoCo/LCOV is one file → one witness).
/// Named axiom:
//= design/witness/spec.md#obligation-individuation
//# Runtime producers: individuation is the operator's
//# responsibility — one instrumented run per test.
//# The artifact does not record how it was produced,
//# so duvet does not attempt detection
async fn jacoco_witness(artifact: &str) -> Result<Witness> {
    let data = JacocoParser.parse(std::path::Path::new(artifact)).await?;
    let generic = data.as_generic();
    let mut files = BTreeMap::new();
    for (path, file_coverage) in &generic.files {
        files.insert(
            path.clone(),
            std::sync::Arc::new(file_coverage.to_coverage_report()),
        );
    }
    Ok(Witness {
        label: artifact.to_string(),
        claim: ClaimRule::ByExecution,
        provenance: Provenance {
            producer: "jacoco".into(),
            artifact: artifact.to_string(),
            discharge_unit: None,
            strength: Strength::Executed,
        },
        files,
    })
}

/// Materialize witnesses for the requested positions from a parsed
/// obligation graph (spec §5.2, via the producer's public
/// `classify_position` and `construct_witnesses`).
///
/// Pass 1 (spec §5.2) here is `classify_position` per requested
/// position: `Rooted` positions get witnesses (pass 2),
/// `NotProofTestable` positions are reported as such — elaborated
/// but rooting nothing dischargeable, so no witness exists for them
/// by definition (Decision 13) — and `Unelaborated` positions get
/// nothing (they surface through Property W6 if no other producer
/// witnesses them). The aggregate-map liveness screen as a
/// *pre-filter* is deliberately skipped: verdicts are identical
/// (spec §1.7: the annotations argument is a semantically inert
/// optimization).
///
/// Witnesses are deduplicated by discharge-unit label: two positions
/// rooting the same discharge unit yield one witness — well-defined
/// per §1.7's witness-identity requirement (cited on
/// [`RequestedPosition`]). Output order is ascending by label within
/// one artifact — deterministic regardless of position order.
fn verus_witnesses_from_graph(
    graph: &verus_sst::structure::ObligationGraph,
    artifact: &str,
    positions: &[RequestedPosition],
    path_matches: impl Fn(&str, &str) -> bool,
    project: impl Fn(&str) -> bool,
) -> Result<Produced> {
    // Every file string the artifact mentions, for translating an
    // engine (absolute) position into artifact coordinates. Bucketed
    // by suffix key so each position consults only same-filename
    // candidates: `path_matches` MUST imply equal final path
    // components (both production matchers — coverage_path_matches
    // and the tests' suffix rule — are component-suffix relations, so
    // they do; see `suffix_key`), which makes the bucket lookup
    // candidate-preserving: every match AND every ambiguity the full
    // scan would see is still seen. BTreeSet iteration keeps each
    // bucket sorted, so refusal messages list candidates in the same
    // order as the full scan did.
    let mut graph_files: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    for node in graph.nodes.values() {
        graph_files.insert(node.extent.file.as_str());
        for file in node.spans.keys() {
            graph_files.insert(file.as_str());
        }
    }
    let mut files_by_suffix: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for f in &graph_files {
        files_by_suffix
            .entry(crate::query::checks::coverage::suffix_key(f))
            .or_default()
            .push(f);
    }

    let mut by_label: BTreeMap<String, Witness> = BTreeMap::new();
    let mut not_proof_testable: Vec<RequestedPosition> = Vec::new();
    // Closures are pure per root (spec §5.4 fixpoint), so one memo
    // is shared across every position of this (graph, project) run.
    let mut memo = verus_sst::witness::ClosureMemo::new();
    for position in positions {
        let Ok(line) = u32::try_from(position.line) else {
            // No real file has 2^32 lines: reaching this means corrupt
            // input or an upstream coordinate-type change. Loud in
            // debug so it cannot fire silently; in release the position
            // degrades to no-witness (W6) rather than aborting the run.
            debug_assert!(
                false,
                "verus-sst: position {}:{} exceeds the u32 line space",
                position.absolute_file, position.line
            );
            continue;
        };
        // Translate to artifact coordinates by the suffix rule; refuse
        // a genuine ambiguity rather than guessing (same posture as
        // report path matching). Only the position's suffix-key bucket
        // can contain matches (see files_by_suffix above).
        let candidates: Vec<&str> = files_by_suffix
            .get(crate::query::checks::coverage::suffix_key(
                &position.absolute_file,
            ))
            .map(Vec::as_slice)
            .unwrap_or_default()
            .iter()
            .copied()
            .filter(|f| path_matches(&position.absolute_file, f))
            .collect();
        let file = match candidates.as_slice() {
            [] => continue, // artifact never mentions the file; W6
            [one] => *one,
            many => {
                return Err(duvet_core::error!(
                    "verus-sst: position {}:{} is ambiguous: it matches multiple \
                     paths recorded in the log ({}). duvet cannot tell which one \
                     refers to this file.",
                    position.absolute_file,
                    position.line,
                    many.join(", ")
                ));
            }
        };
        // Pass 1 (spec §5.2, Decision 13): what is this position to
        // the discharge-unit map? `Rooted` carries the rooting units
        // straight into witness construction (pass 2) — the payload
        // IS `find_discharge_units(file, line)`, so no re-derivation;
        // `NotProofTestable` is a fact the report must carry (distinct
        // from W6); `Unelaborated` feeds W6.
        let units = match verus_sst::witness::classify_position(graph, file, line) {
            verus_sst::witness::PositionKind::Rooted(units) => units,
            verus_sst::witness::PositionKind::NotProofTestable => {
                not_proof_testable.push(position.clone());
                continue;
            }
            verus_sst::witness::PositionKind::Unelaborated => continue,
        };
        for unit in &units {
            // A witness is a pure function of (artifact, unit) and the
            // label identifies it (spec §1.7), so a label already in
            // the map means the identical witness was materialized by
            // an earlier position: skip before paying for the closure.
            if by_label.contains_key(&unit.label) {
                continue;
            }
            let w =
                verus_sst::witness::witness_for_unit(graph, unit, artifact, &project, &mut memo);
            by_label.insert(w.label.clone(), w);
        }
    }
    Ok(Produced {
        witnesses: by_label.into_values().collect(),
        not_proof_testable,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::parsers::verus_sst::structure::{parse_module, ObligationGraph};
    use duvet_coverage::types::CoverageStatus;

    fn suffix_matches(absolute: &str, short: &str) -> bool {
        absolute == short || absolute.ends_with(&format!("/{short}"))
    }

    const TWO_FNS: &str = r#"
(@ "src/a.rs:10:1: 20:2 (#0)"
 (FunctionSst :name (Fun :path c::caller) ((Fun :path c::callee))))
(@ "src/b.rs:5:1: 8:2 (#0)"
 (FunctionSst :name (Fun :path c::callee) ()))
"#;

    fn graph() -> ObligationGraph {
        ObligationGraph::merge([parse_module(TWO_FNS).unwrap()]).unwrap()
    }

    fn position(file: &str, line: u64) -> RequestedPosition {
        RequestedPosition {
            absolute_file: file.to_string(),
            line,
        }
    }

    #[tokio::test]
    async fn jacoco_report_becomes_one_by_execution_witness() {
        use std::io::Write;
        let mut path = std::env::temp_dir();
        path.push(format!(
            "duvet_producer_{}_{}.xml",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::File::create(&path)
            .unwrap()
            .write_all(
                br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<!DOCTYPE report PUBLIC "-//JACOCO//DTD Report 1.1//EN" "report.dtd">
<report name="A">
    <package name="com/example">
        <sourcefile name="Impl.java">
            <line nr="8" mi="0" ci="3" mb="0" cb="0"/>
            <line nr="9" mi="3" ci="0" mb="0" cb="0"/>
        </sourcefile>
    </package>
</report>
"#,
            )
            .unwrap();

        let artifact = path.to_string_lossy().to_string();
        let w = jacoco_witness(&artifact).await.unwrap();
        let _ = std::fs::remove_file(&path);

        assert_eq!(w.label, artifact);
        assert_eq!(w.claim, ClaimRule::ByExecution);
        assert_eq!(w.provenance.producer, "jacoco");
        assert_eq!(w.provenance.strength, Strength::Executed);
        assert_eq!(w.provenance.discharge_unit, None);
        let report = &w.files["com/example/Impl.java"];
        assert_eq!(report.get(&8), Some(&CoverageStatus::Hit));
        assert_eq!(report.get(&9), Some(&CoverageStatus::Miss));
    }

    #[test]
    fn verus_positions_become_root_span_witnesses_with_hit_maps() {
        let g = graph();
        let ws = verus_witnesses_from_graph(
            &g,
            "logs/",
            &[position("/proj/src/a.rs", 15)],
            suffix_matches,
            |f| f.starts_with("src/"),
        )
        .unwrap()
        .witnesses;
        let [w] = ws.as_slice() else {
            panic!("expected one witness, got {}", ws.len())
        };
        assert_eq!(w.label, "c::caller");
        assert_eq!(
            w.claim,
            ClaimRule::ByRootSpan {
                file: "src/a.rs".into(),
                start_line: 10,
                end_line: 20,
            }
        );
        assert_eq!(w.provenance.producer, "verus-sst");
        assert_eq!(w.provenance.strength, Strength::Consulted);
        assert_eq!(w.provenance.discharge_unit.as_deref(), Some("c::caller"));
        // The closure pulled the callee's file in, as Hit lines.
        assert_eq!(w.files["src/b.rs"].get(&5), Some(&CoverageStatus::Hit));
    }

    #[test]
    fn same_obligation_from_two_positions_dedupes_to_one_witness() {
        let g = graph();
        let ws = verus_witnesses_from_graph(
            &g,
            "logs/",
            &[
                position("/proj/src/a.rs", 12),
                position("/proj/src/a.rs", 15),
            ],
            suffix_matches,
            |f| f.starts_with("src/"),
        )
        .unwrap()
        .witnesses;
        assert_eq!(
            ws.iter().map(|w| w.label.as_str()).collect::<Vec<_>>(),
            ["c::caller"],
            "a witness is a function of (artifact, obligation) only — spec §1.7"
        );
    }

    #[test]
    fn position_no_obligation_certifies_yields_no_witness() {
        let g = graph();
        let produced = verus_witnesses_from_graph(
            &g,
            "logs/",
            &[
                position("/proj/src/a.rs", 999),
                position("/proj/nope.rs", 1),
            ],
            suffix_matches,
            |_| true,
        )
        .unwrap();
        assert!(
            produced.witnesses.is_empty(),
            "zero discharge units → zero witnesses (feeds W6)"
        );
        assert!(
            produced.not_proof_testable.is_empty(),
            "unelaborated positions are W6's, not the producer's fact to report"
        );
    }

    #[test]
    fn elaborated_unrooted_position_is_reported_not_proof_testable() {
        // Line 25 is elaborated (a `@@` sub-span of c::caller's body)
        // but outside every obligation extent: a proof ingredient, not
        // a claim (spec §5.2 pass 1, Decision 13). The producer
        // delivers no witness for it and reports the position as not
        // proof-testable — a fact, never a verdict (spec §4).
        const BODY_SPAN: &str = r#"
(@ "src/a.rs:10:1: 20:2 (#0)"
 (FunctionSst :name (Fun :path c::caller) :body
  (@@ "src/a.rs:25:5: 26:9 (#0)" (Exp))))
"#;
        let g = ObligationGraph::merge([parse_module(BODY_SPAN).unwrap()]).unwrap();
        let produced = verus_witnesses_from_graph(
            &g,
            "logs/",
            &[
                position("/proj/src/a.rs", 25),
                position("/proj/src/a.rs", 15),
            ],
            suffix_matches,
            |f| f.starts_with("src/"),
        )
        .unwrap();
        assert_eq!(
            produced.not_proof_testable,
            [position("/proj/src/a.rs", 25)],
            "elaborated-but-unrooted position must be reported not proof-testable"
        );
        // The rooted position still gets its witness; NPT never leaks
        // into the witness set.
        assert_eq!(
            produced
                .witnesses
                .iter()
                .map(|w| w.label.as_str())
                .collect::<Vec<_>>(),
            ["c::caller"]
        );
    }

    #[test]
    fn ambiguous_artifact_path_translation_is_refused() {
        // The artifact mentions two path strings that both suffix-match
        // the same absolute file: refuse rather than guess.
        const AMBIG: &str = r#"
(@ "src/a.rs:10:1: 20:2 (#0)"
 (FunctionSst :name (Fun :path c::f) ()))
(@ "other/src/a.rs:10:1: 20:2 (#0)"
 (FunctionSst :name (Fun :path c::g) ()))
"#;
        let g = ObligationGraph::merge([parse_module(AMBIG).unwrap()]).unwrap();
        let err = verus_witnesses_from_graph(
            &g,
            "logs/",
            &[position("/proj/other/src/a.rs", 15)],
            suffix_matches,
            |_| true,
        )
        .unwrap_err();
        assert!(format!("{err:?}").contains("ambiguous"));
    }

    #[test]
    #[ignore = "perf harness, run explicitly with --ignored --nocapture"]
    fn bench_verus_witnesses_from_graph_corpus() {
        // Full producer pipeline over the checked-in SST corpus, one
        // requested position per discharge unit (the annotation-heavy
        // worst case): measures classify + construct + dedup together.
        use crate::query::parsers::verus_sst;
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/query/parsers/verus_sst/testdata/corpus");
        let graph = verus_sst::load_dir(&dir).expect("corpus must load");
        let is_project = |f: &str| f.starts_with("duvet-coverage/");
        let positions: Vec<RequestedPosition> = verus_sst::witness::all_units(&graph)
            .iter()
            .map(|u| {
                position(
                    &format!("/abs/{}", u.span.file),
                    u64::from(u.span.start_line),
                )
            })
            .collect();
        // Warmup + shape sanity.
        let produced =
            verus_witnesses_from_graph(&graph, "bench", &positions, suffix_matches, is_project)
                .unwrap();
        let iters = 20u32;
        let start = std::time::Instant::now();
        for _ in 0..iters {
            std::hint::black_box(
                verus_witnesses_from_graph(&graph, "bench", &positions, suffix_matches, is_project)
                    .unwrap(),
            );
        }
        let total = start.elapsed();
        println!(
            "bench_verus_witnesses_from_graph: positions={} witnesses={} iters={} total={:?} per-iter={:?}",
            positions.len(),
            produced.witnesses.len(),
            iters,
            total,
            total / iters
        );
    }
}
