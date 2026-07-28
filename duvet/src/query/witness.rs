// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Engine-side witness model (design/witness/spec.md §1.2–§1.6) and the
//! adapter that carries engine data across the trust boundary into the
//! verified quantifier layer (`duvet-coverage/src/witness.rs`, Phase 4).
//!
//! The quantifiers themselves — `binds`, the bound-witness set,
//! same-witness discharge (W1), test execution (W2/W6), global execution
//! (W3) — are NOT implemented here. They are proven in `duvet-coverage`
//! and the engine's verdicts are computed by calling them (glue
//! obligation G2). An earlier engine-side mirror of those quantifiers
//! lived in this module and was deleted when the adapter landed, so no
//! parallel verdict computation exists.
//!
//! What this module still owns is glue, named as such:
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

use crate::{
    annotation::Annotation,
    query::{
        checks::coverage::{
            coverage_path_matches, ClassificationMap, FileClassification, SourceIndex,
        },
        coverage::ExecutionStatus,
    },
    Result,
};
use duvet_coverage::{
    types::{AnnotationSpan, LineClass, Scope},
    witness::{self as verified, ScoringMode},
};
use rustc_hash::FxHashMap;
use std::{collections::BTreeMap, fmt, path::PathBuf, sync::Arc};

/// The verified per-file coverage type a witness carries
/// (spec §1.2: "line → CoverageStatus, the existing verified type").
pub type CoverageReportMap = duvet_coverage::types::CoverageReport;

//= design/witness/spec.md#witness
//= type=implementation
//# A witness is the record of **one act of checking**:
//# one test's execution, or one prover obligation's successful
//# verification.
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
    /// achieved.
    pub files: BTreeMap<String, CoverageReportMap>,
}

//= design/witness/spec.md#claim-rules
//= type=implementation
//# ClaimRule ::= ByExecution | ByRootSpan(file, line_range)
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClaimRule {
    /// Runtime rule: the report cannot record which test produced
    /// it, so the test claims the witness by evidence — its own
    /// lines are executed in it. Sound only under witness
    /// individuation (spec §4.2, a named axiom for runtime
    /// producers).
    ByExecution,
    /// Prover rule: the witness was constructed from the
    /// annotation's own position, so ownership is positional.
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
    /// Prover producers only: the obligation the witness was
    /// constructed from.
    pub discharge_unit: Option<String>,
    pub strength: Strength,
}

/// The (label, strength) pair verdict output must carry
/// (spec §3; decisions.md Decision 7).
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

/// A test annotation's resolved target in engine coordinates:
/// the absolute path of its source file plus the target line the
/// verified target resolution produced (spec §1.1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedTarget {
    pub absolute_file: String,
    pub line: u64,
}

/// One bound witness's contribution to a pair verdict
/// (spec §3: label, strength, and per-witness result).
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DischargeVerdict {
    /// Property W1's verdict, computed by the verified
    /// `report_discharged`.
    pub discharged: bool,
    /// Every bound witness's result, in bound order (spec §3 requires
    /// every one of them in failure output).
    pub per_witness: Vec<PairWitnessResult>,
}

/// The G1 adapter: engine witnesses and annotations translated into the
/// verified model's vocabulary (opaque injective file ids), plus the
/// verdict entry points that call the verified quantifier layer.
///
/// Built once per coverage run. Everything trusted about the translation
/// is stated in the module docs; the quantifier semantics downstream of
/// it are proven (duvet-coverage Properties W1–W6).
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

/// One annotation's scoring context in verified-model coordinates.
struct AnnCtx<'a> {
    file_id: u64,
    span: AnnotationSpan,
    mode: ScoringMode,
    classifications: &'a [Option<LineClass>],
    scopes: &'a [Scope],
    file_length: u64,
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
    //# Positional comparison requires file identity: if the engine's
    //# path-matching relation associates an annotation's file with more
    //# than one witness file (or one witness file with more than one
    //# source file), the engine MUST refuse the bind and report the
    //# ambiguity rather than select — the same posture the producer takes
    //# when translating positions into artifact coordinates.
    ///
    /// A `ByRootSpan` producer-path coordinate that suffix-matches more
    /// than one project source is refused here — at translation, before
    /// any bind — with an error naming the coordinate and every match.
    /// One that matches no project source gets a fresh id no annotation
    /// carries: it binds nothing, by construction rather than by scan.
    pub fn build(
        engine: &'a [Witness],
        matched: &[FxHashMap<PathBuf, &CoverageReportMap>],
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
                        .entries()
                        .iter()
                        .filter(|(_, abs)| coverage_path_matches(abs, file))
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

            let mut files: Vec<(u64, CoverageReportMap)> = Vec::new();
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
                // Degraded files carry no such requirement (direct
                // observation) and keep their maps.
                if let Some(FileClassification::Classified {
                    classifications, ..
                }) = classification.get(path)
                {
                    let len = classifications.len();
                    if !cov.keys().all(|&k| k >= 1 && (k as usize - 1) < len) {
                        continue;
                    }
                }
                files.push((ids[absolute], (*cov).clone()));
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
    /// id, and the scoring mode its file's classification selected (G3).
    /// Trust-boundary refusals — no classification, defeated
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
            mode: ScoringMode::Unscorable,
            classifications: &[],
            scopes: &[],
            file_length: 0,
        };

        let path = annotation.source.to_path_buf();
        let file_id = match self.index.absolute_of(&path).and_then(|abs| self.ids.get(abs)) {
            Some(&id) => id,
            None => return unscorable(NO_FILE),
        };
        if end_line == u64::MAX {
            return unscorable(file_id);
        }
        match self.classification.get(&path) {
            Some(FileClassification::Classified {
                classifications,
                scopes,
                file_length,
            }) => AnnCtx {
                file_id,
                span,
                mode: ScoringMode::Classified,
                classifications,
                scopes,
                file_length: *file_length,
            },
            Some(FileClassification::Degraded {
                classifications,
                file_length,
            }) => AnnCtx {
                file_id,
                span,
                mode: ScoringMode::Degraded,
                classifications,
                scopes: &[],
                file_length: *file_length,
            },
            Some(FileClassification::Defeated { .. }) | None => unscorable(file_id),
        }
    }

    //= design/witness/spec.md#property-w6-unwitnessed-test-annotations
    //= type=implementation
    //# ¬∃ w ∈ witnesses : binds(T, w)   ⟹   T is reported unwitnessed
    ///
    /// The W6/W2 verdict for one test annotation, computed by the verified
    /// `is_unwitnessed` (= ¬`report_test_executed`).
    pub fn is_unwitnessed(&self, t: &Arc<Annotation>) -> bool {
        let ctx = self.ctx_of(t);
        verified::is_unwitnessed(
            ctx.file_id,
            &ctx.span,
            ctx.mode,
            ctx.classifications,
            ctx.scopes,
            ctx.file_length,
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
                    ctx.mode,
                    ctx.classifications,
                    ctx.scopes,
                    ctx.file_length,
                    &self.verified[wi],
                )
            })
            .collect()
    }

    //= design/witness/spec.md#discharge
    //= type=implementation
    //# witnesses_for(T)  =  { w ∈ delivered : binds(T, w) }
    //#
    //# discharged(T, I)  ⟺  witnesses_for(T) ≠ ∅
    //#                       ∧  ∀w ∈ witnesses_for(T) : executed(I, w)
    ///
    /// The W1 verdict for one (T, I) pair, computed by the verified
    /// `report_discharged` over the full delivered set (Decision 14's
    /// universal form: one bound witness that never reaches I fails the
    /// pair — bound witnesses are never outvoted).
    ///
    /// `bound` is the list from [`Self::bound_witnesses`]; the per-witness
    /// results (spec §3: every bound witness, in bound order) use the
    /// verified `is_executed_by` cell for the executed flag and the
    /// caller's diagnostic status (the engine's `Unknown`-carrying cell)
    /// for reporting detail.
    pub fn discharge_verdict(
        &self,
        t: &Arc<Annotation>,
        i: &Arc<Annotation>,
        bound: &[usize],
        mut diagnostic_status: impl FnMut(usize) -> ExecutionStatus,
    ) -> DischargeVerdict {
        let t_ctx = self.ctx_of(t);
        let i_ctx = self.ctx_of(i);
        let discharged = verified::report_discharged(
            t_ctx.file_id,
            &t_ctx.span,
            t_ctx.mode,
            t_ctx.classifications,
            t_ctx.scopes,
            t_ctx.file_length,
            i_ctx.file_id,
            &i_ctx.span,
            i_ctx.mode,
            i_ctx.classifications,
            i_ctx.scopes,
            i_ctx.file_length,
            &self.verified,
        );
        let per_witness = bound
            .iter()
            .map(|&wi| {
                let executed = verified::is_executed_by(
                    i_ctx.file_id,
                    &i_ctx.span,
                    i_ctx.mode,
                    i_ctx.classifications,
                    i_ctx.scopes,
                    i_ctx.file_length,
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

    //= design/witness/spec.md#property-w3-global-execution
    //= type=implementation
    //# report_ever_executed(I, witnesses) = true
    //#     ⟺  ∃ w ∈ witnesses : executed(I, w)
    ///
    /// The W3 verdict, computed by the verified `report_ever_executed`.
    /// Deliberately weaker than W1 (no correlation); never used to
    /// discharge pairs.
    pub fn ever_executed(&self, i: &Arc<Annotation>) -> bool {
        let ctx = self.ctx_of(i);
        verified::report_ever_executed(
            ctx.file_id,
            &ctx.span,
            ctx.mode,
            ctx.classifications,
            ctx.scopes,
            ctx.file_length,
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

    fn cov_hit(lines: &[u64]) -> CoverageReportMap {
        lines.iter().map(|&l| (l, CoverageStatus::Hit)).collect()
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
    #[test]
    fn out_of_bounds_coverage_dropped_only_for_classified_files() {
        let idx = index(&[
            ("c.java", "/proj/c.java"),
            ("d.rs", "/proj/d.rs"),
        ]);
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
        m.insert(PathBuf::from("c.java"), &cov);
        m.insert(PathBuf::from("d.rs"), &cov);

        let witnesses = [witness];
        let matched = vec![m];
        let adapter = VerifiedVerdicts::build(&witnesses, &matched, &classification, &idx)
            .expect("builds");
        let ids: Vec<u64> = adapter.verified[0].files.iter().map(|(id, _)| *id).collect();
        assert_eq!(
            ids,
            vec![1],
            "classified file's out-of-bounds map dropped; degraded file's kept"
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
                        Some(duvet_coverage::types::line_class(&[LineProperty::Annotation])),
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
}
