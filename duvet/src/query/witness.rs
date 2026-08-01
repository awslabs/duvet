// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Engine-side witness model (design/witness/spec.md §1.2–§1.6) and the
//! adapter that carries engine data across the trust boundary into the
//! verified quantifier layer (`duvet-coverage/src/witness.rs`, Phase 4).
//!
//! The quantifiers themselves — `binds`, the bound-witness set, and the
//! spec's engine properties (design/witness/spec.md#engine-properties) —
//! are NOT implemented here. They are proven in `duvet-coverage`
//! and the engine's verdicts are computed by calling them (glue
//! obligation G2, cited below).
//!
//! What this module still owns is glue, named in spec §4.4:
//!
//! - the engine's witness data model (labels, provenance, producer-path
//!   coordinates — everything the verified projection deliberately
//!   omits);
//! - [`VerifiedVerdicts`]: the G1 adapter. It assigns opaque `u64` file
//!   ids keyed by absolute project path (injective by construction),
//!   translates witness coverage maps and `ByRootSpan` coordinates into
//!   those ids — REFUSING ambiguous producer-path suffix matches rather
//!   than selecting (spec §1.5's bind-time refusal) — and routes each
//!   annotation's file to the [`ScoringMode`] its classification
//!   selected (G3). It also establishes the verified functions'
//!   preconditions at the boundary: ill-formed contexts route to
//!   `Unscorable`, and a witness's out-of-bounds coverage map for a
//!   classified file is dropped (the same input the engine's diagnostic
//!   path refuses with `Unknown`; both directions verdict `false`).

//= design/witness/spec.md#engine-glue
//# - **G2 (call obligation).** The engine MUST compute every pair,
//# test, and global verdict (Properties
//# [W1](#property-w1-same-witness-discharge)–[W4](#property-w4-monotonicity),
//# [W6](#property-w6-unwitnessed-test-annotations)) by calling the
//# verified layer's functions, and MUST derive per-witness
//# failure diagnostics ([§3](#verdict-output)) from the same verified cells;
//# no parallel engine-side verdict computation may exist.

use crate::{
    annotation::Annotation,
    query::{
        checks::coverage::{span_in_model, ClassificationMap, ScoringView, SourceIndex},
        coverage::ExecutionStatus,
    },
    Result,
};
use duvet_coverage::{types::AnnotationSpan, witness as verified};
use rustc_hash::FxHashMap;
use std::{collections::BTreeMap, fmt, path::PathBuf, sync::Arc};

/// The verified per-file coverage type a witness carries.
//= design/witness/spec.md#witness
//#     files:      Map<FilePath, CoverageReport>,
//#                                    -- per file: line → CoverageStatus,
//#                                    -- the existing verified type
pub type CoverageReportMap = duvet_coverage::types::CoverageReport;

/// The engine-side witness record (spec §1.2). The definitional
/// citation ("one act of checking") lives on the verified projection in
/// `duvet-coverage/src/witness.rs`; this mirror carries the engine
/// concerns the projection deliberately omits (label, provenance,
/// path-keyed maps). Single-type unification across the PG1 boundary is
/// a pending design discussion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Witness {
    /// Human-readable identity (report path; obligation path).
    pub label: String,
    /// How a test annotation claims this witness (spec §1.5).
    pub claim: ClaimRule,
    /// Where the witness came from (spec §1.3).
    pub provenance: Provenance,
    /// Per file: line → CoverageStatus. Closedness (spec §4.1) is a
    /// producer obligation; the engine never learns how it was
    /// achieved. `Arc`-shared with the verified adapter's witnesses
    /// (same maps, one resident copy — see `VerifiedVerdicts::build`).
    pub files: BTreeMap<String, Arc<CoverageReportMap>>,
}

// Mirrors the verified `ClaimRule` (spec §1.5) in engine coordinates
// (producer path strings instead of opaque file ids). The spec text is
// quoted once, at the verified type it declares
// (`duvet_coverage::witness::ClaimRule`); unifying the two types is a
// pending design discussion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClaimRule {
    /// The runtime rule: the test claims the witness by evidence — its
    /// own lines are executed in it (spec §1.5).
    /// A named axiom for runtime producers (spec §4.2).
    ByExecution,
    /// The prover rule: ownership is positional and holds by
    /// construction (spec §1.5).
    /// `file` is in producer (artifact) coordinates; the adapter
    /// translates it to a file identity by the suffix rule, refusing
    /// ambiguity (spec §1.5).
    ByRootSpan {
        file: String,
        start_line: u64,
        end_line: u64,
    },
}

//= design/witness/spec.md#provenance
//= type=implementation
//# `strength` records what kind of claim the witness supports:
//# `Executed` (a runtime act ran these lines) or
//# `Consulted` (a prover's elaboration reached these lines).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Strength {
    Executed,
    Consulted,
}

impl fmt::Display for Strength {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Strength::Executed => write!(f, "executed"),
            Strength::Consulted => write!(f, "consulted"),
        }
    }
}

/// Witness provenance (spec §1.3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Provenance {
    pub producer: String,
    pub artifact: String,
    //= design/witness/spec.md#provenance
    //#     discharge_unit: Option<String>,
    //#                               -- prover producers only: the obligation
    //#                               -- the witness was constructed from
    pub discharge_unit: Option<String>,
    pub strength: Strength,
}

/// The (label, strength) pair of one bound witness in verdict output
/// (decisions.md, Decision 7).
//= design/witness/spec.md#verdict-output
//# For every discharged pair, the output MUST name, in verbose
//# output, every bound witness (its label) and its strength ([§1.3](#provenance)).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WitnessRef {
    pub label: String,
    pub strength: Strength,
}

impl From<&Witness> for WitnessRef {
    fn from(w: &Witness) -> Self {
        WitnessRef {
            label: w.label.clone(),
            strength: w.provenance.strength,
        }
    }
}

/// One bound witness's contribution to a pair verdict — the per-witness
/// result (executed I / did not execute I) and strength that spec §3's
/// failure listing is built from (the citation lives on the listing
/// struct, `NotExecutedAnnotation` in `result.rs`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PairWitnessResult {
    pub witness: WitnessRef,
    /// Whether this witness executed the implementation — the verified
    /// `is_executed_by` cell.
    pub executed: bool,
    /// The engine's diagnostic status for the same cell (`Unknown`
    /// carries a line number). Reporting detail only; the verdict never
    /// consults it.
    pub status: ExecutionStatus,
}

/// The verdict for one (T, I) pair over the bound-witness set.
//= design/witness/spec.md#verdict-output
//# the engine computes all of these to evaluate the verdict,
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DischargeVerdict {
    /// Property W1's verdict, computed by the verified
    /// `report_discharged`.
    pub discharged: bool,
    /// Every bound witness's result, in bound order (failure output
    /// lists every one of them).
    pub per_witness: Vec<PairWitnessResult>,
}

/// The G1 adapter: engine witnesses and annotations translated into the
/// verified model's vocabulary (opaque injective file ids), plus the
/// verdict entry points that call the verified quantifier layer.
///
/// Built once per coverage run. Everything trusted about the translation
/// is stated in the module docs; the quantifier semantics downstream of
/// it are proven in duvet-coverage against the spec's engine properties
/// (design/witness/spec.md#engine-properties).
pub struct VerifiedVerdicts<'a> {
    /// Verified-model witnesses, index-aligned with the engine's witness
    /// vector.
    verified: Vec<verified::Witness>,
    /// The engine witnesses (labels/strengths for report output).
    engine: &'a [Witness],
    /// file absolute path → opaque id. Injective: keyed by full absolute
    /// path, ids assigned by enumeration.
    ids: FxHashMap<String, u64>,
    classification: &'a ClassificationMap,
    index: &'a SourceIndex,
}

/// One annotation's scoring context in verified-model coordinates: its
/// file id plus the shared [`ScoringView`] its file's classification
/// selected (the same accessor the engine's diagnostic path routes
/// through).
struct AnnCtx<'a> {
    file_id: u64,
    span: AnnotationSpan,
    view: ScoringView<'a>,
}

/// Sentinel file id for an annotation whose file is not a project source
/// (cannot happen for parsed annotations; guarded anyway). Distinct from
/// every assigned id, which starts at 0 and stays far below.
const NO_FILE: u64 = u64::MAX;

impl<'a> VerifiedVerdicts<'a> {
    /// Translate the engine's witnesses into the verified model.
    ///
    /// `matched` is the per-witness map from project source path to that
    /// witness's coverage for it, as produced by
    /// `SourceIndex::match_witness_files` (which already refused both
    /// ambiguity directions for coverage-map paths).
    ///
    //= design/witness/spec.md#claim-rules
    //= type=implementation
    //# If the engine's path-matching relation associates an annotation's
    //# file with more than one witness file (or one witness file with
    //# more than one source file), the engine MUST refuse the bind and
    //# report the ambiguity rather than select — the same posture the
    //# producer takes when translating positions into artifact
    //# coordinates.
    ///
    /// A `ByRootSpan` producer-path coordinate that suffix-matches more
    /// than one project source is refused here — at translation, before
    /// any bind — with an error naming the coordinate and every match.
    /// One that matches no project source gets a fresh id no annotation
    /// carries: it binds nothing, by construction rather than by scan.
    pub fn build(
        engine: &'a [Witness],
        matched: &[FxHashMap<PathBuf, Arc<CoverageReportMap>>],
        classification: &'a ClassificationMap,
        index: &'a SourceIndex,
    ) -> Result<Self> {
        // Ids for every project source up front: enumeration order of the
        // index, injective because absolute paths are distinct keys.
        let mut ids: FxHashMap<String, u64> = FxHashMap::default();
        for (_, absolute) in index.entries() {
            let next = ids.len() as u64;
            ids.entry(absolute.clone()).or_insert(next);
        }
        // Fresh ids for producer paths matching no project source, kept
        // injective past the project range.
        let mut next_unmatched = ids.len() as u64;

        let mut verified = Vec::with_capacity(engine.len());
        for (wi, w) in engine.iter().enumerate() {
            let claim = match &w.claim {
                ClaimRule::ByExecution => verified::ClaimRule::ByExecution,
                ClaimRule::ByRootSpan {
                    file,
                    start_line,
                    end_line,
                } => {
                    let mut hits: Vec<&str> = index
                        .matching_entries(file)
                        .map(|(_, abs)| abs.as_str())
                        .collect();
                    let file_id = match hits.len() {
                        0 => {
                            // Rooted outside the project: a fresh id that no
                            // annotation's file carries, so nothing binds it.
                            let id = next_unmatched;
                            next_unmatched += 1;
                            id
                        }
                        1 => ids[hits[0]],
                        _ => {
                            hits.sort_unstable();
                            return Err(duvet_core::error!(
                                "witness '{}' is rooted at '{}', which is ambiguous: it \
                                 matches multiple project files ({}). duvet cannot tell \
                                 which file the root span refers to, and refuses to guess \
                                 (a wrong guess could bind a test annotation to another \
                                 file's witness).",
                                w.label,
                                file,
                                hits.join(", ")
                            ));
                        }
                    };
                    verified::ClaimRule::ByRootSpan {
                        file_id,
                        start_line: *start_line,
                        end_line: *end_line,
                    }
                }
            };

            let mut files: Vec<(u64, Arc<CoverageReportMap>)> = Vec::new();
            for (path, cov) in &matched[wi] {
                let Some(absolute) = index.absolute_of(path) else {
                    // matched keys come from the index; unreachable, but a
                    // dropped map only withholds credit.
                    continue;
                };
                // Precondition establishment (trust boundary): the verified
                // two-phase scorer requires coverage keys within the file's
                // classification bounds. A map violating them for a
                // classified file is dropped — the engine's diagnostic path
                // refuses the same input with `Unknown`; both verdict false.
                // Same predicate on both paths
                // (`ScoringView::coverage_in_bounds`): degraded files carry
                // no such requirement (direct observation) and keep their
                // maps.
                if let Some(c) = classification.get(path) {
                    if !c.scoring_view().coverage_in_bounds(cov) {
                        continue;
                    }
                }
                files.push((ids[absolute], Arc::clone(cov)));
            }
            // Deterministic order; keys unique (matched is a map), so
            // first-match lookup semantics never engage (G1).
            files.sort_by_key(|(id, _)| *id);

            verified.push(verified::Witness { claim, files });
        }

        Ok(Self {
            verified,
            engine,
            ids,
            classification,
            index,
        })
    }

    /// The annotation's scoring context in verified coordinates: its file
    /// id, and the [`ScoringView`] its file's classification selected
    /// (G3). Trust-boundary refusals — no classification, defeated
    /// classification, ill-formed annotation range — route to
    /// `Unscorable`, which binds nothing and executes nothing.
    fn ctx_of(&self, annotation: &Arc<Annotation>) -> AnnCtx<'_> {
        let (start_line, end_line) = annotation.line_range();
        let span = AnnotationSpan {
            start_line,
            end_line,
        };
        let unscorable = |file_id: u64| AnnCtx {
            file_id,
            span: span.clone(),
            view: ScoringView::UNSCORABLE,
        };

        let path = annotation.source.to_path_buf();
        let file_id = match self
            .index
            .absolute_of(&path)
            .and_then(|abs| self.ids.get(abs))
        {
            Some(&id) => id,
            None => return unscorable(NO_FILE),
        };
        if !span_in_model(end_line) {
            return unscorable(file_id);
        }
        match self.classification.get(&path) {
            // Defeated classifications flatten to `Unscorable` here (the
            // shared routing decision — see `FileClassification::scoring_view`).
            Some(classification) => AnnCtx {
                file_id,
                span,
                view: classification.scoring_view(),
            },
            None => unscorable(file_id),
        }
    }

    /// The W6/W2 verdict for one test annotation, computed by the verified
    /// `is_unwitnessed` (= ¬`report_test_executed`). The W6 citation
    /// ("¬∃ w binds ⟹ reported unwitnessed") lives on the engine's
    /// reporting branch in `engine.rs` — the site that does the reporting.
    pub fn is_unwitnessed(&self, t: &Arc<Annotation>) -> bool {
        let ctx = self.ctx_of(t);
        verified::is_unwitnessed(
            ctx.file_id,
            &ctx.span,
            ctx.view.mode,
            ctx.view.classifications,
            ctx.view.scopes,
            ctx.view.file_length,
            &self.verified,
        )
    }

    //= design/witness/spec.md#property-w2-test-execution
    //= type=implementation
    //# report_test_executed(T, witnesses) = true
    //#     ⟺  ∃ w ∈ witnesses : binds(T, w)
    ///
    /// The indices of the witnesses binding T — the verified `is_bound_by`
    /// cell per witness. W2's verdict is `is_unwitnessed`'s negation; this
    /// list is the report detail (which acts of checking claimed T).
    pub fn bound_witnesses(&self, t: &Arc<Annotation>) -> Vec<usize> {
        let ctx = self.ctx_of(t);
        (0..self.verified.len())
            .filter(|&wi| {
                verified::is_bound_by(
                    ctx.file_id,
                    &ctx.span,
                    ctx.view.mode,
                    ctx.view.classifications,
                    ctx.view.scopes,
                    ctx.view.file_length,
                    &self.verified[wi],
                )
            })
            .collect()
    }

    /// The W1 verdict for one (T, I) pair, computed by the verified
    /// `report_discharged_given_bound` — spec §1.6's discharge over the
    /// full delivered set, with the `binds(T, ·)` scan hoisted out. (The
    /// §1.6 citation lives on the verified `discharged` spec fn in
    /// `duvet-coverage/src/witness.rs`, the definitional site.)
    ///
    //= design/witness/spec.md#engine-glue
    //= type=implementation
    //# The exactness precondition MUST be established the G3 way:
    //# the set is assembled from the verified binding cells' results
    //# for the same test context and witness list, never recomputed
    //# engine-side.
    ///
    /// `bound` is the list from [`Self::bound_witnesses`], which collects
    /// exactly the indices where the verified `is_bound_by` cell (ensures:
    /// result ⟺ `binds`) returned true, over this same `self.verified`
    /// list and the same `ctx_of(t)` context — establishing the sound-
    /// and-complete-by-index precondition of the verified entry point,
    /// whose `ensures` then equals `report_discharged`'s verbatim
    /// (Decision 14's universal form: one bound witness that never
    /// reaches I fails the pair — bound witnesses are never outvoted).
    ///
    /// The per-witness results (spec §3: every bound witness, in bound
    /// order) use the verified `is_executed_by` cell for the executed
    /// flag and the caller's diagnostic status (the engine's
    /// `Unknown`-carrying cell) for reporting detail.
    pub fn discharge_verdict(
        &self,
        t: &Arc<Annotation>,
        i: &Arc<Annotation>,
        bound: &[usize],
        mut diagnostic_status: impl FnMut(usize) -> ExecutionStatus,
    ) -> DischargeVerdict {
        let t_ctx = self.ctx_of(t);
        let i_ctx = self.ctx_of(i);
        let discharged = verified::report_discharged_given_bound(
            t_ctx.file_id,
            &t_ctx.span,
            t_ctx.view.mode,
            t_ctx.view.classifications,
            t_ctx.view.scopes,
            t_ctx.view.file_length,
            i_ctx.file_id,
            &i_ctx.span,
            i_ctx.view.mode,
            i_ctx.view.classifications,
            i_ctx.view.scopes,
            i_ctx.view.file_length,
            &self.verified,
            bound,
        );
        let per_witness = bound
            .iter()
            .map(|&wi| {
                let executed = verified::is_executed_by(
                    i_ctx.file_id,
                    &i_ctx.span,
                    i_ctx.view.mode,
                    i_ctx.view.classifications,
                    i_ctx.view.scopes,
                    i_ctx.view.file_length,
                    &self.verified[wi],
                );
                PairWitnessResult {
                    witness: WitnessRef::from(&self.engine[wi]),
                    executed,
                    status: diagnostic_status(wi),
                }
            })
            .collect();
        DischargeVerdict {
            discharged,
            per_witness,
        }
    }

    /// The W3 verdict, computed by the verified `report_ever_executed`.
    /// Deliberately weaker than W1 (no correlation); never used to
    /// discharge pairs.
    pub fn ever_executed(&self, i: &Arc<Annotation>) -> bool {
        let ctx = self.ctx_of(i);
        verified::report_ever_executed(
            ctx.file_id,
            &ctx.span,
            ctx.view.mode,
            ctx.view.classifications,
            ctx.view.scopes,
            ctx.view.file_length,
            &self.verified,
        )
    }

    /// Decision 12's diagnostic shape: when a pair fails
    /// bound-but-not-discharged, the report lists the bound witnesses by
    /// name (label + strength) so the reader can see which acts of
    /// checking claimed the test without reaching the implementation.
    pub fn witness_refs(&self, bound: &[usize]) -> Vec<WitnessRef> {
        bound
            .iter()
            .map(|&wi| WitnessRef::from(&self.engine[wi]))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::checks::coverage::FileClassification;
    use duvet_coverage::types::{CoverageStatus, LineProperty};

    fn index(entries: &[(&str, &str)]) -> SourceIndex {
        SourceIndex::from_entries(
            entries
                .iter()
                .map(|(p, abs)| (PathBuf::from(p), abs.to_string()))
                .collect(),
        )
    }

    fn root_witness(label: &str, file: &str, start: u64, end: u64) -> Witness {
        Witness {
            label: label.to_string(),
            claim: ClaimRule::ByRootSpan {
                file: file.to_string(),
                start_line: start,
                end_line: end,
            },
            provenance: Provenance {
                producer: "verus-sst".into(),
                artifact: "logs/".into(),
                discharge_unit: Some(label.to_string()),
                strength: Strength::Consulted,
            },
            files: BTreeMap::new(),
        }
    }

    fn cov_hit(lines: &[u64]) -> Arc<CoverageReportMap> {
        Arc::new(lines.iter().map(|&l| (l, CoverageStatus::Hit)).collect())
    }

    /// Finding 5 pinning test — the two-files-same-suffix scenario.
    ///
    /// Two project files end in the same `/`-boundary suffix
    /// (`a/src/x.rs`, `b/src/x.rs`). A witness rooted at the shared
    /// suffix `src/x.rs` could bind a test annotation in EITHER file;
    /// under ∀-discharge a wrong association is usually a false failure,
    /// and when closures overlap it can manufacture a false discharge.
    /// Spec §1.5: the engine MUST refuse the bind and report the
    /// ambiguity rather than select. The adapter refuses at translation
    /// time — before any bind — naming the coordinate and both matches.
    //= design/witness/spec.md#claim-rules
    //= type=test
    //# If the engine's path-matching relation associates an annotation's
    //# file with more than one witness file (or one witness file with
    //# more than one source file), the engine MUST refuse the bind and
    //# report the ambiguity rather than select — the same posture the
    //# producer takes when translating positions into artifact
    //# coordinates.
    #[test]
    fn ambiguous_root_span_suffix_is_refused_not_selected() {
        let idx = index(&[
            ("a/src/x.rs", "/proj/a/src/x.rs"),
            ("b/src/x.rs", "/proj/b/src/x.rs"),
        ]);
        let classification = ClassificationMap::default();
        let witnesses = [root_witness("c::obligation", "src/x.rs", 1, 10)];
        let matched = vec![FxHashMap::default()];

        let err = VerifiedVerdicts::build(&witnesses, &matched, &classification, &idx)
            .err()
            .expect("ambiguous root span must refuse, never select");
        let msg = err.to_string();
        assert!(msg.contains("src/x.rs"), "names the coordinate: {msg}");
        assert!(
            msg.contains("/proj/a/src/x.rs") && msg.contains("/proj/b/src/x.rs"),
            "names every match: {msg}"
        );
        assert!(msg.contains("c::obligation"), "names the witness: {msg}");
    }

    /// The unambiguous cases around the refusal: a root span naming one
    /// file fully binds only annotations in that file, and a root span
    /// matching nothing in the project binds nothing (fresh id, no scan).
    #[test]
    fn unambiguous_root_span_translates_and_unmatched_binds_nothing() {
        let idx = index(&[
            ("a/src/x.rs", "/proj/a/src/x.rs"),
            ("b/src/y.rs", "/proj/b/src/y.rs"),
        ]);
        let classification = ClassificationMap::default();
        let witnesses = [
            root_witness("c::in_a", "a/src/x.rs", 1, 10),
            root_witness("c::elsewhere", "src/other.rs", 1, 10),
        ];
        let matched = vec![FxHashMap::default(), FxHashMap::default()];

        let adapter = VerifiedVerdicts::build(&witnesses, &matched, &classification, &idx)
            .expect("unambiguous coordinates translate");
        // File ids follow index enumeration order: a=0, b=1. The witness
        // rooted in `a` carries a's id; the unmatched one carries a fresh
        // id outside the project range.
        match adapter.verified[0].claim {
            verified::ClaimRule::ByRootSpan { file_id, .. } => assert_eq!(file_id, 0),
            _ => unreachable!(),
        }
        match adapter.verified[1].claim {
            verified::ClaimRule::ByRootSpan { file_id, .. } => assert_eq!(file_id, 2),
            _ => unreachable!(),
        }
    }

    /// Precondition establishment at the trust boundary: a witness map
    /// whose coverage keys exceed a CLASSIFIED file's bounds is dropped
    /// (the verified two-phase scorer's precondition; the engine's
    /// diagnostic path refuses the same input with `Unknown` — both
    /// verdict false). A DEGRADED file's map is kept: direct observation
    /// has no bounds requirement, and dropping it would flip real
    /// degraded verdicts.
    //= design/witness/spec.md#engine-glue
    //= type=test
    //# The adapter MUST establish
    //# the verified functions' preconditions at the boundary —
    //# filter or degrade before calling, never assume.
    #[test]
    fn out_of_bounds_coverage_dropped_only_for_classified_files() {
        let idx = index(&[("c.java", "/proj/c.java"), ("d.rs", "/proj/d.rs")]);
        let mut classification = ClassificationMap::default();
        classification.insert(
            PathBuf::from("c.java"),
            FileClassification::Classified {
                classifications: vec![None, None],
                scopes: vec![],
                file_length: 2,
            },
        );
        classification.insert(
            PathBuf::from("d.rs"),
            FileClassification::Degraded {
                classifications: vec![None, None],
                file_length: 2,
            },
        );

        let witness = Witness {
            label: "report.xml".into(),
            claim: ClaimRule::ByExecution,
            provenance: Provenance {
                producer: "jacoco".into(),
                artifact: "report.xml".into(),
                discharge_unit: None,
                strength: Strength::Executed,
            },
            files: BTreeMap::new(), // adapter reads `matched`, not this
        };
        // Both files carry a key beyond their 2-line classification.
        let cov = cov_hit(&[1, 99]);
        let mut m = FxHashMap::default();
        m.insert(PathBuf::from("c.java"), Arc::clone(&cov));
        m.insert(PathBuf::from("d.rs"), Arc::clone(&cov));

        let witnesses = [witness];
        let matched = vec![m];
        let adapter =
            VerifiedVerdicts::build(&witnesses, &matched, &classification, &idx).expect("builds");
        let ids: Vec<u64> = adapter.verified[0]
            .files
            .iter()
            .map(|(id, _)| *id)
            .collect();
        assert_eq!(
            ids,
            vec![1],
            "classified file's out-of-bounds map dropped; degraded file's kept"
        );
    }

    /// The two trust-boundary responses to the SAME ill-formed input — a
    /// witness map with coverage keys beyond a classified file's bounds —
    /// agree on the verdict (design/witness/spec.md#engine-glue, G3:
    /// establish the verified functions' preconditions at the boundary):
    /// the adapter path drops the map before the verified layer, so no
    /// witness executed the annotation; the engine's diagnostic path
    /// (`executed_status`) refuses the same input with `Unknown`. Both
    /// verdict false — the annotation counts as executed on neither path.
    #[test]
    fn out_of_bounds_map_verdicts_false_on_both_paths() {
        use crate::{
            annotation::{Annotation, AnnotationLevel, AnnotationType},
            query::checks::coverage::executed_status,
        };
        use duvet_core::file::SourceFile as CoreSourceFile;
        use std::path::Path;

        let idx = index(&[("c.java", "/proj/c.java")]);
        let mut classification = ClassificationMap::default();
        classification.insert(
            PathBuf::from("c.java"),
            FileClassification::Classified {
                classifications: vec![None, None],
                scopes: vec![],
                file_length: 2,
            },
        );

        // Ill-formed input: key 99 exceeds the 2-line classification.
        let cov = cov_hit(&[1, 99]);

        let witness = Witness {
            label: "report.xml".into(),
            claim: ClaimRule::ByExecution,
            provenance: Provenance {
                producer: "jacoco".into(),
                artifact: "report.xml".into(),
                discharge_unit: None,
                strength: Strength::Executed,
            },
            files: BTreeMap::new(), // adapter reads `matched`, not this
        };
        let mut m = FxHashMap::default();
        m.insert(PathBuf::from("c.java"), Arc::clone(&cov));
        let witnesses = [witness];
        let matched = vec![m];
        let adapter =
            VerifiedVerdicts::build(&witnesses, &matched, &classification, &idx).expect("builds");

        let contents = "//= spec#s\ncode();\n";
        let source = CoreSourceFile::new("c.java", contents).unwrap();
        let text = source.substr_range(0..10).unwrap();
        let target = source.substr_range(4..10).unwrap();
        let quote = source.substr_range(11..18).unwrap();
        let annotation = Arc::new(Annotation {
            source: source.path().clone(),
            anno_line: 1,
            original_target: target,
            original_text: text,
            original_quote: quote,
            anno: AnnotationType::Citation,
            target: "spec#s".to_string(),
            quote: String::new(),
            comment: String::new(),
            manifest_dir: source.path().clone(),
            level: AnnotationLevel::Auto,
            format: crate::specification::Format::Auto,
            tracking_issue: String::new(),
            feature: String::new(),
            tags: Default::default(),
            blob_link: None,
        });

        // Adapter path: the out-of-bounds map was dropped, so no witness
        // executed the annotation.
        assert!(
            !adapter.ever_executed(&annotation),
            "adapter path: dropped map means the witness never executed I"
        );

        // Engine diagnostic path: the same coverage refused with `Unknown`
        // — not `Executed`, the same false verdict.
        let status = executed_status(
            &annotation,
            classification.get(Path::new("c.java")),
            Some(&cov),
        );
        assert!(
            matches!(status, ExecutionStatus::Unknown { .. }),
            "diagnostic path: out-of-bounds coverage refused with Unknown, got {status:?}"
        );
    }

    /// End-to-end through the verified layer: a witness rooted in file
    /// `a` binds a test annotation only when the annotation lives in `a`
    /// — same-suffix file `b` never silently borrows it. (The ambiguous
    /// coordinate case is refused at build; this pins the unambiguous
    /// positive and negative binds through the id translation.)
    #[test]
    fn root_span_binds_only_the_named_file_through_translation() {
        use crate::annotation::{Annotation, AnnotationLevel, AnnotationType};
        use duvet_core::file::SourceFile as CoreSourceFile;

        let idx = index(&[
            ("a/src/x.rs", "/proj/a/src/x.rs"),
            ("b/src/x.rs", "/proj/b/src/x.rs"),
        ]);
        // Both files: line 1 annotation, line 2 code (degraded
        // classification, the dogfood shape).
        let mut classification = ClassificationMap::default();
        for p in ["a/src/x.rs", "b/src/x.rs"] {
            classification.insert(
                PathBuf::from(p),
                FileClassification::Degraded {
                    classifications: vec![
                        Some(duvet_coverage::types::line_class(&[
                            LineProperty::Annotation,
                        ])),
                        None,
                    ],
                    file_length: 2,
                },
            );
        }
        // Root span names `a/src/x.rs` unambiguously and spans the target.
        let witnesses = [root_witness("c::in_a", "a/src/x.rs", 1, 10)];
        let matched = vec![FxHashMap::default()];
        let adapter =
            VerifiedVerdicts::build(&witnesses, &matched, &classification, &idx).expect("builds");

        let annotation = |path: &str| -> Arc<Annotation> {
            let contents = "//= spec#s\ncode();\n";
            let source = CoreSourceFile::new(path, contents).unwrap();
            // Byte offsets into `contents`: the `//= spec#s` meta line,
            // its `spec#s` target, and the `code();` quote line.
            let text = source.substr_range(0..10).unwrap();
            let target = source.substr_range(4..10).unwrap();
            let quote = source.substr_range(11..18).unwrap();
            Arc::new(Annotation {
                source: source.path().clone(),
                anno_line: 1,
                original_target: target,
                original_text: text,
                original_quote: quote,
                anno: AnnotationType::Test,
                target: "spec#s".to_string(),
                quote: String::new(),
                comment: String::new(),
                manifest_dir: source.path().clone(),
                level: AnnotationLevel::Auto,
                format: crate::specification::Format::Auto,
                tracking_issue: String::new(),
                feature: String::new(),
                tags: Default::default(),
                blob_link: None,
            })
        };

        assert!(
            !adapter.is_unwitnessed(&annotation("a/src/x.rs")),
            "annotation in the named file binds the witness"
        );
        assert!(
            adapter.is_unwitnessed(&annotation("b/src/x.rs")),
            "annotation in the same-suffix OTHER file must not borrow the witness"
        );
    }

    /// Shared constructor for the e2e tests below: a minimal annotation in
    /// `path` whose file is `//= spec#s` on line 1 and `code();` on line 2,
    /// so degraded resolution targets line 2.
    fn e2e_annotation(path: &str, anno: crate::annotation::AnnotationType) -> Arc<Annotation> {
        use crate::annotation::AnnotationLevel;
        use duvet_core::file::SourceFile as CoreSourceFile;
        let contents = "//= spec#s\ncode();\n";
        let source = CoreSourceFile::new(path, contents).unwrap();
        let text = source.substr_range(0..10).unwrap();
        let target = source.substr_range(4..10).unwrap();
        let quote = source.substr_range(11..18).unwrap();
        Arc::new(Annotation {
            source: source.path().clone(),
            anno_line: 1,
            original_target: target,
            original_text: text,
            original_quote: quote,
            anno,
            target: "spec#s".to_string(),
            quote: String::new(),
            comment: String::new(),
            manifest_dir: source.path().clone(),
            level: AnnotationLevel::Auto,
            format: crate::specification::Format::Auto,
            tracking_issue: String::new(),
            feature: String::new(),
            tags: Default::default(),
            blob_link: None,
        })
    }

    fn exec_witness(label: &str, strength: Strength) -> Witness {
        Witness {
            label: label.to_string(),
            claim: ClaimRule::ByExecution,
            provenance: Provenance {
                producer: "jacoco".into(),
                artifact: label.to_string(),
                discharge_unit: None,
                strength,
            },
            files: BTreeMap::new(), // adapter reads `matched`, not this
        }
    }

    /// G2 end-to-end: every verdict the engine reports for this scenario —
    /// W2/W6 boundness, the W1 pair verdict over a cell-assembled bound
    /// set, W3 ever-executed, and the per-witness failure diagnostics —
    /// comes out of one adapter whose entry points call the verified
    /// layer's functions. The Decision 14 shape is asserted through that
    /// path: a witness carrying both files discharges the pair; split
    /// evidence binds the test but fails the pair (and W3 still reports
    /// the implementation ever-executed, deliberately weaker).
    //= design/witness/spec.md#engine-glue
    //= type=test
    //# The verified layer's guarantees ([§2](#engine-properties)) reach the user only through
    //# unverified engine glue. Each glue component is named, bounded,
    //# and unit-tested (the posture [§4.3](#obligation-testing) takes for producers):
    //= design/witness/spec.md#engine-glue
    //= type=test
    //# - **G2 (call obligation).** The engine MUST compute every pair,
    //# test, and global verdict (Properties
    //# [W1](#property-w1-same-witness-discharge)–[W4](#property-w4-monotonicity),
    //# [W6](#property-w6-unwitnessed-test-annotations)) by calling the
    //# verified layer's functions, and MUST derive per-witness
    //# failure diagnostics ([§3](#verdict-output)) from the same verified cells;
    //# no parallel engine-side verdict computation may exist.
    //= design/witness/spec.md#engine-glue
    //= type=test
    //# The exactness precondition MUST be established the G3 way:
    //# the set is assembled from the verified binding cells' results
    //# for the same test context and witness list, never recomputed
    //# engine-side.
    //= design/witness/spec.md#verdict-output
    //= type=test
    //# the engine computes all of these to evaluate the verdict,
    #[test]
    fn g2_verdicts_flow_through_the_verified_layer() {
        use crate::annotation::AnnotationType;

        let idx = index(&[("t.rs", "/proj/t.rs"), ("i.rs", "/proj/i.rs")]);
        let mut classification = ClassificationMap::default();
        for p in ["t.rs", "i.rs"] {
            classification.insert(
                PathBuf::from(p),
                FileClassification::Degraded {
                    classifications: vec![
                        Some(duvet_coverage::types::line_class(&[
                            duvet_coverage::types::LineProperty::Annotation,
                        ])),
                        None,
                    ],
                    file_length: 2,
                },
            );
        }
        let t = e2e_annotation("t.rs", AnnotationType::Test);
        let i = e2e_annotation("i.rs", AnnotationType::Citation);

        let matched_for = |paths: &[&str]| {
            let mut m = FxHashMap::default();
            for p in paths {
                m.insert(PathBuf::from(*p), cov_hit(&[2]));
            }
            m
        };

        // One witness that saw both sides: the pair is discharged, and the
        // per-witness diagnostics for a failing sibling pair would come from
        // the same cells (asserted in the split case below).
        let witnesses = [exec_witness("run-both", Strength::Executed)];
        let matched = vec![matched_for(&["t.rs", "i.rs"])];
        let adapter =
            VerifiedVerdicts::build(&witnesses, &matched, &classification, &idx).expect("builds");
        assert!(!adapter.is_unwitnessed(&t));
        let bound = adapter.bound_witnesses(&t);
        assert_eq!(bound, vec![0], "bound set assembled from the verified cells");
        let verdict = adapter.discharge_verdict(&t, &i, &bound, |_| ExecutionStatus::Executed);
        assert!(verdict.discharged);
        assert_eq!(verdict.per_witness.len(), 1);
        assert!(verdict.per_witness[0].executed);
        assert_eq!(verdict.per_witness[0].witness.label, "run-both");

        // Split evidence: the test is witnessed, but its bound witness never
        // reached the implementation — the pair fails (Decision 14), and the
        // per-witness diagnostic names the failing claim. W3 stays true.
        let witnesses = [
            exec_witness("run-t-only", Strength::Executed),
            exec_witness("run-i-only", Strength::Executed),
        ];
        let matched = vec![matched_for(&["t.rs"]), matched_for(&["i.rs"])];
        let adapter =
            VerifiedVerdicts::build(&witnesses, &matched, &classification, &idx).expect("builds");
        assert!(!adapter.is_unwitnessed(&t));
        let bound = adapter.bound_witnesses(&t);
        assert_eq!(bound, vec![0], "only the witness that executed T binds it");
        let verdict = adapter.discharge_verdict(&t, &i, &bound, |_| ExecutionStatus::NotExecuted);
        assert!(!verdict.discharged, "split evidence never discharges");
        assert_eq!(verdict.per_witness.len(), 1);
        assert!(!verdict.per_witness[0].executed);
        assert_eq!(verdict.per_witness[0].witness.label, "run-t-only");
        assert!(adapter.ever_executed(&i), "W3 is deliberately weaker than W1");
    }

    /// Spec §1.3: `strength` distinguishes the two kinds of claim a
    /// witness supports, `discharge_unit` names the prover obligation a
    /// proof witness was constructed from (and is absent for runtime
    /// witnesses). Both propagate into the report vocabulary
    /// (`WitnessRef`) unchanged, and the two strengths render distinctly.
    //= design/witness/spec.md#provenance
    //= type=test
    //# `strength` records what kind of claim the witness supports:
    //# `Executed` (a runtime act ran these lines) or
    //# `Consulted` (a prover's elaboration reached these lines).
    //= design/witness/spec.md#provenance
    //= type=test
    //#     discharge_unit: Option<String>,
    //#                               -- prover producers only: the obligation
    //#                               -- the witness was constructed from
    #[test]
    fn provenance_strength_and_discharge_unit_record_the_claim_kind() {
        let runtime = exec_witness("report.xml", Strength::Executed);
        assert_eq!(runtime.provenance.discharge_unit, None);
        let prover = root_witness("c::obligation", "src/x.rs", 1, 10);
        assert_eq!(prover.provenance.strength, Strength::Consulted);
        assert_eq!(
            prover.provenance.discharge_unit.as_deref(),
            Some("c::obligation"),
            "prover witnesses carry the obligation they were constructed from"
        );

        // Strength propagates into the report vocabulary unchanged...
        assert_eq!(WitnessRef::from(&runtime).strength, Strength::Executed);
        assert_eq!(WitnessRef::from(&prover).strength, Strength::Consulted);
        // ...and the two claim kinds render distinctly.
        assert_eq!(Strength::Executed.to_string(), "executed");
        assert_eq!(Strength::Consulted.to_string(), "consulted");
    }

    /// Spec §1.2: a witness's per-file map is the existing verified
    /// coverage type — the same value (one `Arc`, no translation) is an
    /// engine map and a verified-model map, and the verified layer scores
    /// it directly.
    //= design/witness/spec.md#witness
    //= type=test
    //#     files:      Map<FilePath, CoverageReport>,
    //#                                    -- per file: line → CoverageStatus,
    //#                                    -- the existing verified type
    #[test]
    fn witness_files_carry_the_verified_coverage_type() {
        use duvet_coverage::types::{AnnotationSpan, LineProperty};

        // One map, engine-keyed by path and verified-keyed by file id.
        let cov: Arc<CoverageReportMap> = cov_hit(&[2]);
        let mut engine_files: BTreeMap<String, Arc<CoverageReportMap>> = BTreeMap::new();
        engine_files.insert("src/x.rs".to_string(), Arc::clone(&cov));
        let vw = verified::Witness {
            claim: verified::ClaimRule::ByExecution,
            files: vec![(0, Arc::clone(engine_files.get("src/x.rs").unwrap()))],
        };
        // The verified layer scores the engine's map value directly:
        // degraded resolution of an annotation on line 1 targets line 2,
        // which the map records as Hit.
        let classifications = vec![
            Some(duvet_coverage::types::line_class(&[LineProperty::Annotation])),
            None,
        ];
        assert!(verified::is_executed_by(
            0,
            &AnnotationSpan {
                start_line: 1,
                end_line: 1,
            },
            verified::ScoringMode::Degraded,
            &classifications,
            &[],
            2,
            &vw,
        ));
    }

    /// Perf harness (run explicitly: `cargo test --release -p duvet
    /// bench_adapter_build -- --ignored --nocapture`): total
    /// `VerifiedVerdicts::build` cost vs the price of deep-cloning every
    /// matched map once — the latter is what build paid per witness
    /// before the maps became `Arc`-shared, kept as the reference the
    /// item-4 win is measured against.
    #[test]
    #[ignore = "perf harness, run explicitly with --ignored --nocapture"]
    fn bench_adapter_build_clone_share() {
        // 200 witnesses x 5 files x 2000 covered lines each.
        let n_wit = 200usize;
        let n_files = 5usize;
        let lines: Vec<u64> = (1..=2000u64).collect();
        let entries: Vec<(PathBuf, String)> = (0..n_files)
            .map(|f| {
                (
                    PathBuf::from(format!("src/f{f}.rs")),
                    format!("/proj/src/f{f}.rs"),
                )
            })
            .collect();
        let idx = SourceIndex::from_entries(entries.clone());
        let cov = cov_hit(&lines);
        let witnesses: Vec<Witness> = (0..n_wit)
            .map(|i| {
                let mut w = root_witness(&format!("c::w{i}"), "src/f0.rs", 1, 10);
                w.files = (0..n_files)
                    .map(|f| (format!("src/f{f}.rs"), cov.clone()))
                    .collect();
                w
            })
            .collect();
        let matched: Vec<FxHashMap<PathBuf, Arc<CoverageReportMap>>> = witnesses
            .iter()
            .map(|w| idx.match_witness_files(&w.files).expect("unambiguous"))
            .collect();
        let mut classification = ClassificationMap::default();
        for (p, _) in &entries {
            classification.insert(
                p.clone(),
                FileClassification::Degraded {
                    classifications: vec![None; 2001],
                    file_length: 2001,
                },
            );
        }
        let iters = 10u32;
        let t0 = std::time::Instant::now();
        for _ in 0..iters {
            std::hint::black_box(
                VerifiedVerdicts::build(&witnesses, &matched, &classification, &idx)
                    .expect("builds"),
            );
        }
        let build = t0.elapsed();
        let t1 = std::time::Instant::now();
        for _ in 0..iters {
            for m in &matched {
                for cov in m.values() {
                    std::hint::black_box(CoverageReportMap::clone(cov));
                }
            }
        }
        let clones = t1.elapsed();
        println!(
            "bench_adapter_build_clone_share: witnesses={n_wit} files/witness={n_files} \
             lines/file={} iters={iters} build-total={build:?} clone-only={clones:?}",
            lines.len(),
        );
    }
}
