// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Engine-side witness model (design/witness/spec.md §1.2–§1.6).
//!
//! This module is the pure quantifier layer over the verified
//! per-annotation cells: `binds`, the bound-witness set, and
//! same-witness discharge. It deliberately mirrors the shape of the
//! spec's Properties W1/W2/W6 so that milestone 3's verified
//! "Phase 4" in `duvet-coverage` can replace or check these
//! functions one-for-one. Everything format- or producer-specific
//! stays out: no producer appears in this vocabulary (spec §2).
//!
//! Trusted-base note: the `executed(X, w)` cells consumed here are
//! computed by the existing verified Phases 1–3 (spec §1.4) applied
//! per witness by `checks::coverage`; this module only quantifies
//! over them.

use crate::query::coverage::ExecutionStatus;
use std::{collections::BTreeMap, fmt};

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
    /// `file` is in producer (artifact) coordinates; the engine
    /// matches it against project paths by the same suffix rule
    /// used for coverage report paths.
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

//= design/witness/spec.md#claim-rules
//= type=implementation
//# binds(T, w)  ⟺  match w.claim:
//#     ByExecution        → executed(T, w)
//#     ByRootSpan(f, r)   → T's resolved target EXISTS and falls
//#                           within r in file f
///
/// `binds` is total over (annotation, witness) pairs. The
/// `executed(T, w)` cell is supplied by the caller (it is the
/// existing verified model applied to this one witness's maps —
/// spec §1.4) and is only consulted for the `ByExecution` arm, so
/// callers may pass a lazy closure. `resolved_target` is `None`
/// when T's target could not be resolved (defeated classification,
/// resolution walked off the file): an unresolvable target binds no
/// positional witness and the annotation surfaces through W6.
pub fn binds(
    witness: &Witness,
    executed_t: impl FnOnce() -> ExecutionStatus,
    resolved_target: Option<&ResolvedTarget>,
    path_matches: impl Fn(&str, &str) -> bool,
) -> bool {
    match &witness.claim {
        ClaimRule::ByExecution => matches!(executed_t(), ExecutionStatus::Executed),
        ClaimRule::ByRootSpan {
            file,
            start_line,
            end_line,
        } => resolved_target.is_some_and(|t| {
            path_matches(&t.absolute_file, file) && *start_line <= t.line && t.line <= *end_line
        }),
    }
}

//= design/witness/spec.md#property-w2-test-execution
//= type=implementation
//# report_test_executed(T, witnesses) = true
//#     ⟺  ∃ w ∈ witnesses : binds(T, w)
///
/// The bound-witness set for one test annotation. W2's
/// `report_test_executed` is `!bound.is_empty()`; W6's
/// "unwitnessed" is `bound.is_empty()`.
///
/// Generic over a carrier `W` (the engine pairs each witness with
/// its matched-file map; tests use `Witness` directly with an
/// identity accessor) so that both run this exact code path.
pub fn bound_witnesses<'w, W>(
    witnesses: impl IntoIterator<Item = &'w W>,
    witness_of: impl Fn(&W) -> &Witness,
    mut executed_t: impl FnMut(&'w W) -> ExecutionStatus,
    resolved_target: Option<&ResolvedTarget>,
    path_matches: impl Fn(&str, &str) -> bool,
) -> Vec<&'w W> {
    witnesses
        .into_iter()
        .filter(|w| binds(witness_of(w), || executed_t(w), resolved_target, &path_matches))
        .collect()
}

//= design/witness/spec.md#property-w1-same-witness-discharge
//= type=implementation
//# report_discharged(T, I, witnesses) = true
//#     ⟺  (∃ w ∈ witnesses : binds(T, w))
//#         ∧ (∀ w ∈ witnesses : binds(T, w) ⟹ executed(I, w))
///
/// The universal discharge verdict (Decision 14): a pair is
/// discharged when the test binds at least one witness and EVERY
/// bound witness executed the implementation. One bound witness
/// that never reaches the implementation is a vacuous claim and
/// fails the pair — bound witnesses are never outvoted.
///
/// Quantifying over `bound` (not all witnesses) realizes the
/// `binds(T, w) ⟹` guard; the caller supplies the bound set from
/// [`bound_witnesses`]. Returns the per-witness results in bound
/// order — spec §3 requires every one of them in failure output,
/// and the verdict is their conjunction, so nothing extra is
/// computed to report them.
pub fn discharge_verdict<'w, W>(
    bound: &[&'w W],
    witness_of: impl Fn(&W) -> &Witness,
    mut executed_i: impl FnMut(&'w W) -> ExecutionStatus,
) -> DischargeVerdict {
    let per_witness: Vec<PairWitnessResult> = bound
        .iter()
        .map(|w| {
            let status = executed_i(w);
            PairWitnessResult {
                witness: WitnessRef::from(witness_of(w)),
                executed: matches!(status, ExecutionStatus::Executed),
                status,
            }
        })
        .collect();
    let discharged = !per_witness.is_empty() && per_witness.iter().all(|r| r.executed);
    DischargeVerdict {
        discharged,
        per_witness,
    }
}

/// One bound witness's contribution to a pair verdict
/// (spec §3: label, strength, and per-witness result).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PairWitnessResult {
    pub witness: WitnessRef,
    /// Whether this witness executed the implementation.
    pub executed: bool,
    /// The full per-witness cell, kept for diagnostics
    /// (`Unknown` carries a line number).
    pub status: ExecutionStatus,
}

/// The verdict for one (T, I) pair over the bound-witness set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DischargeVerdict {
    pub discharged: bool,
    /// Every bound witness's result, in bound order.
    pub per_witness: Vec<PairWitnessResult>,
}

/// Decision 12's diagnostic shape: when a pair fails
/// bound-but-not-discharged, the report lists the bound witnesses
/// by name (label + strength) so the reader can see which acts of
/// checking claimed the test without reaching the implementation.
pub fn witness_refs<W>(bound: &[&W], witness_of: impl Fn(&W) -> &Witness) -> Vec<WitnessRef> {
    bound
        .iter()
        .map(|w| WitnessRef::from(witness_of(w)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Suffix-at-`/`-boundary matcher, same rule as
    /// `checks::coverage::coverage_path_matches`; inlined here so the
    /// quantifier-layer tests stay free of the glue module.
    fn suffix_matches(absolute: &str, short: &str) -> bool {
        absolute == short || absolute.ends_with(&format!("/{short}"))
    }

    fn runtime_witness(label: &str) -> Witness {
        Witness {
            label: label.to_string(),
            claim: ClaimRule::ByExecution,
            provenance: Provenance {
                producer: "jacoco".into(),
                artifact: label.to_string(),
                discharge_unit: None,
                strength: Strength::Executed,
            },
            files: BTreeMap::new(),
        }
    }

    fn proof_witness(label: &str, file: &str, start: u64, end: u64) -> Witness {
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

    fn target(file: &str, line: u64) -> ResolvedTarget {
        ResolvedTarget {
            absolute_file: file.to_string(),
            line,
        }
    }

    /// Status oracle keyed by witness label: the executed(X, w) cell
    /// matrix, with no coverage machinery. Everything not listed is
    /// NotExecuted.
    fn cells<'a>(executed_in: &'a [&'a str]) -> impl FnMut(&Witness) -> ExecutionStatus + 'a {
        move |w: &Witness| {
            if executed_in.contains(&w.label.as_str()) {
                ExecutionStatus::Executed
            } else {
                ExecutionStatus::NotExecuted
            }
        }
    }

    #[test]
    fn binds_by_execution_requires_executed_in_that_witness() {
        let w = runtime_witness("report-a.xml");
        assert!(binds(&w, || ExecutionStatus::Executed, None, suffix_matches));
        for status in [
            ExecutionStatus::NotExecuted,
            ExecutionStatus::Structural,
            ExecutionStatus::Unknown { line_number: 3 },
        ] {
            assert!(
                !binds(&w, || status, None, suffix_matches),
                "{status:?} must not bind: ByExecution claims by evidence only"
            );
        }
    }

    #[test]
    fn binds_by_root_span_is_positional_and_inclusive() {
        let w = proof_witness("c::honest_proof", "vacuity.rs", 21, 24);
        let abs = "/proj/vacuity.rs";
        // The executed cell must be irrelevant to the positional arm:
        // pass a poisoned closure that would fail the test if consulted.
        let poisoned = || panic!("ByRootSpan must not consult executed(T, w)");
        assert!(binds(&w, poisoned, Some(&target(abs, 21)), suffix_matches));
        assert!(binds(&w, poisoned, Some(&target(abs, 24)), suffix_matches));
        assert!(binds(&w, poisoned, Some(&target(abs, 22)), suffix_matches));
        assert!(!binds(&w, poisoned, Some(&target(abs, 20)), suffix_matches));
        assert!(!binds(&w, poisoned, Some(&target(abs, 25)), suffix_matches));
        // Wrong file: same line range, different path.
        assert!(!binds(
            &w,
            poisoned,
            Some(&target("/proj/other.rs", 22)),
            suffix_matches
        ));
        // Unresolvable target binds nothing (feeds W6).
        assert!(!binds(&w, poisoned, None, suffix_matches));
    }

    #[test]
    fn binds_by_root_span_uses_suffix_boundary_matching() {
        let w = proof_witness("c::p", "vacuity.rs", 1, 10);
        let poisoned = || panic!("positional arm");
        // Suffix at a `/` boundary matches...
        assert!(binds(
            &w,
            poisoned,
            Some(&target("/a/b/vacuity.rs", 5)),
            suffix_matches
        ));
        // ...a non-boundary suffix must not.
        assert!(!binds(
            &w,
            poisoned,
            Some(&target("/a/b/myvacuity.rs", 5)),
            suffix_matches
        ));
    }

    /// Property W1's discriminating case — the correlation bug, at the
    /// quantifier layer: T bound by one witness, I executed only in a
    /// different witness. The bound witness did not reach I, so no
    /// discharge; and the diagnostic names the bound witness with its
    /// per-witness result (spec §3).
    #[test]
    fn w1_cross_witness_evidence_does_not_discharge_and_diagnostic_names_bound() {
        let wa = runtime_witness("report-a.xml"); // T executed here
        let wb = runtime_witness("report-b.xml"); // I executed here
        let witnesses = [wa, wb];

        let bound = bound_witnesses(&witnesses, |w| w, cells(&["report-a.xml"]), None, suffix_matches);
        assert_eq!(
            bound.iter().map(|w| w.label.as_str()).collect::<Vec<_>>(),
            ["report-a.xml"]
        );

        // I executed in report-b only: the one bound witness failed to
        // reach I.
        let verdict = discharge_verdict(&bound, |w| w, cells(&["report-b.xml"]));
        assert!(
            !verdict.discharged,
            "evidence assembled from two witnesses must not discharge (W1)"
        );
        // Spec §3: every bound witness with its per-witness result.
        assert_eq!(verdict.per_witness.len(), 1);
        assert_eq!(verdict.per_witness[0].witness.label, "report-a.xml");
        assert!(!verdict.per_witness[0].executed);

        // Decision 12's diagnostic shape: the failing pair lists the
        // bound witnesses by label + strength.
        assert_eq!(
            witness_refs(&bound, |w| w),
            [WitnessRef {
                label: "report-a.xml".into(),
                strength: Strength::Executed,
            }]
        );
    }

    /// Decision 14: the verdict is universal over bound witnesses. A
    /// bound witness that did not execute I fails the pair even when
    /// another bound witness did — bound witnesses are never outvoted
    /// (the masking case Decision 14 exists to catch). All bound
    /// witnesses executing I discharges.
    #[test]
    fn w1_universal_one_bound_witness_missing_i_fails_the_pair() {
        let wa = runtime_witness("report-a.xml");
        let wb = runtime_witness("report-b.xml");
        let witnesses = [wa, wb];

        // T executed in both; I executed in report-b only.
        let bound = bound_witnesses(
            &witnesses,
            |w| w,
            cells(&["report-a.xml", "report-b.xml"]),
            None,
            suffix_matches,
        );
        assert_eq!(bound.len(), 2);

        let verdict = discharge_verdict(&bound, |w| w, cells(&["report-b.xml"]));
        assert!(
            !verdict.discharged,
            "a bound witness that did not execute I MUST fail the pair (Decision 14)"
        );
        // Per-witness results, in bound order, disagreement visible.
        assert_eq!(
            verdict
                .per_witness
                .iter()
                .map(|r| (r.witness.label.as_str(), r.executed))
                .collect::<Vec<_>>(),
            [("report-a.xml", false), ("report-b.xml", true)]
        );

        // Both bound witnesses reaching I discharges.
        let verdict =
            discharge_verdict(&bound, |w| w, cells(&["report-a.xml", "report-b.xml"]));
        assert!(verdict.discharged);
        assert!(verdict.per_witness.iter().all(|r| r.executed));
    }

    //= design/witness/spec.md#property-w4-monotonicity
    //= type=implementation
    //# Adding a witness MAY newly fail a previously-discharged pair —
    //# that is deliberate: the added witness is a claim T now makes, and
    //# if it does not reach I it is the vacuity being caught
    /// Failure monotonicity, example-level spot check: adding a bound
    /// witness never flips a failing pair to passing; it may flip a
    /// passing pair to failing. (Thread 5 proves this universally.)
    #[test]
    fn w4_adding_a_witness_never_rescues_a_failing_pair() {
        let wa = runtime_witness("report-a.xml");
        let wb = runtime_witness("report-b.xml");
        let one = [wa.clone()];
        let two = [wa, wb];

        // I executed in report-b only.
        let i_cells = &["report-b.xml"];

        // With only report-a bound: failing.
        let bound_one = bound_witnesses(&one, |w| w, cells(&["report-a.xml"]), None, suffix_matches);
        assert!(!discharge_verdict(&bound_one, |w| w, cells(i_cells)).discharged);
        // Adding report-b (which DID reach I) does not rescue it.
        let bound_two = bound_witnesses(
            &two,
            |w| w,
            cells(&["report-a.xml", "report-b.xml"]),
            None,
            suffix_matches,
        );
        assert!(
            !discharge_verdict(&bound_two, |w| w, cells(i_cells)).discharged,
            "adding witnesses must never outvote a bound witness that failed (W4)"
        );

        // The inverse direction is deliberate: report-b alone passes,
        // and adding report-a (a new claim that misses I) newly fails it.
        let bound_b = bound_witnesses(
            &two[1..],
            |w| w,
            cells(&["report-b.xml"]),
            None,
            suffix_matches,
        );
        assert!(discharge_verdict(&bound_b, |w| w, cells(i_cells)).discharged);
    }

    #[test]
    fn w6_no_binding_witness_yields_empty_bound_across_both_claim_arms() {
        let runtime = runtime_witness("report-a.xml");
        let proof = proof_witness("c::p", "vacuity.rs", 1, 10);
        let witnesses = [runtime, proof];

        // T executed nowhere and its target resolves outside the root
        // span's file: neither arm binds.
        let bound = bound_witnesses(
            &witnesses,
            |w| w,
            cells(&[]),
            Some(&target("/proj/lib.rs", 5)),
            suffix_matches,
        );
        assert!(bound.is_empty(), "W6: unwitnessed, never silent");
    }

    /// Decision 12 (N obligations own one position → N witnesses, all
    /// bound) under Decision 14's universal verdict: ALL owning
    /// obligations must reach I to discharge; one that does not fails
    /// the pair, with per-witness results naming which.
    #[test]
    fn decision12_all_owning_obligations_must_execute_i() {
        let acc0 = proof_witness("c::accessor0", "types.rs", 160, 167);
        let acc1 = proof_witness("c::accessor1", "types.rs", 160, 167);
        let witnesses = [acc0, acc1];

        let t = target("/proj/types.rs", 167);
        let poisoned = |_: &Witness| -> ExecutionStatus { panic!("positional arm") };
        let bound = bound_witnesses(&witnesses, |w| w, poisoned, Some(&t), suffix_matches);
        assert_eq!(bound.len(), 2, "one witness per owning obligation");

        // Only accessor1's closure reached I: FAIL (Decision 14), and
        // the per-witness results identify the failing claim.
        let verdict = discharge_verdict(&bound, |w| w, cells(&["c::accessor1"]));
        assert!(!verdict.discharged);
        assert_eq!(
            verdict
                .per_witness
                .iter()
                .map(|r| (r.witness.label.as_str(), r.executed))
                .collect::<Vec<_>>(),
            [("c::accessor0", false), ("c::accessor1", true)]
        );
        assert_eq!(verdict.per_witness[1].witness.strength, Strength::Consulted);

        // Both closures reached I: discharged, every bound witness named.
        let verdict =
            discharge_verdict(&bound, |w| w, cells(&["c::accessor0", "c::accessor1"]));
        assert!(verdict.discharged);
        assert_eq!(verdict.per_witness.len(), 2);

        // Neither reached I: bound-but-not-discharged, and the
        // diagnostic lists BOTH bound obligations by name
        // (Decision 12: "report lists the bound obligations by name,
        // none of which executed the implementation").
        assert!(!discharge_verdict(&bound, |w| w, cells(&[])).discharged);
        assert_eq!(
            witness_refs(&bound, |w| w)
                .iter()
                .map(|r| r.label.as_str())
                .collect::<Vec<_>>(),
            ["c::accessor0", "c::accessor1"]
        );
    }
}
