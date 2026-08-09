// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Phase 4: Witness Quantifier Layer (design/witness/spec.md Section 2).
//!
//! The quantifier layer over the existing per-annotation cells: Phases 1–3
//! score ONE annotation against ONE coverage map (`is_annotation_executed`);
//! this phase states and proves the load-bearing quantifiers over a set of
//! delivered witnesses — the spec's engine properties
//! (design/witness/spec.md#engine-properties). Nothing in Phases 1–3 is
//! re-specified here (see `executed_by`).
//!
//! The named trusted-base glue assumptions (G1 file identity, G2 call
//! obligation, G3 mode routing) are NOT verified here; the engine side of
//! the glue lives in `duvet/src/query/witness.rs` (`VerifiedVerdicts`).
//! The `requires` on the report functions (coverage keys within
//! classification bounds for `Classified`-mode scoring; scope line bounds)
//! are the engine adapter's obligation to establish at the trust boundary
//! (design/witness/spec.md#engine-glue, G3).

// What this layer is (spec §1.4): exactly the existing verified
// Phases 1–3 applied to one witness's coverage maps — the citation
// lives on `executed_by`, the definitional site. What it rests on
// (spec §4): the verified layer's guarantees (§2) reach the user only
// through unverified engine glue, each component named, bounded, and
// unit-tested. The "adapter MUST establish the verified functions'
// preconditions" citation lives on the engine adapter itself
// (`duvet/src/query/witness.rs`, `VerifiedVerdicts::ctx_of`), the code
// that filters or degrades before calling.
//
// Closedness (spec §4.1, "Every delivered `files` map MUST be closed…")
// is owned by the verified constructor: the annotation lives inside
// `producer_core::assemble_witness_lines`, whose `ensures` proves it.
//
// The properties below ARE the Verus phase the spec's §2 describes;
// the dogfood requirement (witness spec §2, cited on the CI verify
// step) gates the proofs and this file's citations in CI.

use crate::{
    annotation_execution::is_annotation_executed, degraded::degraded_execution_status,
    target_resolution::annotation_target, types::*,
};
// Ghost-only imports: spec twins referenced from spec fns and `ensures`.
#[cfg(verus_keep_ghost)]
use crate::annotation_execution::execution_status_of;
#[cfg(verus_keep_ghost)]
use crate::degraded::degraded_status_of;
#[cfg(verus_keep_ghost)]
use crate::target_resolution::annotation_target_spec;
use verus_builtin_macros::verus;
// vstd is ghost-only here (see lib.rs): gated so it stays out of the
// published dependency graph.
#[cfg(feature = "verify")]
use vstd::prelude::*;

verus! {

/// Which verified scoring path the engine's per-file classifier routing
/// selected for an annotation's file (glue assumption G3, made explicit in
/// the model so the quantifier layer quantifies over the cell the engine
/// actually ships, not only the classified path):
///
/// - `Classified` — the language-aware two-phase model
///   (`is_annotation_executed`), for files with a classifier.
/// - `Degraded` — the verified classifier-less path
///   (`degraded_execution_status`), for files without one.
/// - `Unscorable` — the trust-boundary refusals: defeated
///   classification, `end_line == u64::MAX`, unclassified file (its
///   `Unknown` diagnostic is engine reporting; the verdict
///   contribution is uniformly `false`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
//= design/witness/spec.md#executed
//# The verified Phase 4 layer implements the "or" per file: a
//# `ScoringMode` routes each file to the classified or the degraded
//# scorer, and engine trust-boundary refusals are encoded as
//# `Unscorable` — binds nothing, executes nothing
pub enum ScoringMode {
    Classified,
    Degraded,
    Unscorable,
}

/// How a test annotation claims a witness (spec §1.5), generic over the
/// file coordinate `F` because the claim's SHAPE is coordinate-independent
/// — only where the rule points differs across the G1 trust boundary. The
/// verified layer instantiates `F = u64` (opaque injective ids, the
/// vocabulary the proofs quantify over); the engine instantiates
/// `F = String` (producer path coordinates, pre-translation). One
/// definition, two coordinate systems: the instantiations are distinct
/// types the compiler keeps apart, and a variant added here exists on
/// both sides of the boundary by construction — no mirror to drift.
#[derive(Debug, Clone, PartialEq, Eq)]
//= design/witness/spec.md#claim-rules
//# ClaimRule ::= ByExecution | ByRootSpan(file, line_range)
pub enum ClaimRule<F> {
    // The rule definitions (`ByExecution` is the runtime rule / `ByRootSpan`
    // is the prover rule) are quoted at their definitional site — the match
    // arms of the `binds` spec fn below — where the witness's consulted
    // span can reach them. The variants declare; `binds` defines.
    ByExecution,
    /// `file` is the claimed file in `F` coordinates — for the verified
    /// instantiation, an opaque id (glue assumption G1). The line range
    /// is inclusive.
    ByRootSpan { file: F, start_line: u64, end_line: u64 },
}

/// Verified-model projection of a witness: claim rule plus per-file
/// coverage. Label and provenance are engine concerns and deliberately
/// absent. `files` maps an opaque file id (G1) to the existing verified
/// `CoverageReport` type; the multi-file map lives INSIDE the verified
/// witness so that W1's same-witness conjunction is over one object —
/// projecting per-file in glue would reintroduce the correlation bug's
/// shape (spec §6, Relationship).
///
/// The maps are `Arc`-shared so the glue can hand the SAME report to the
/// engine's diagnostic cells and the verified witness without doubling
/// resident coverage data. Semantically inert: in spec land an `Arc<T>`
/// is its value (the lookup specs deref it), and `==`/`PartialEq` remain
/// value equality — W4's transport-by-equality argument is unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
//= design/witness/spec.md#witness
//# A witness is the record of **one act of checking**:
//# one test's execution, or one prover obligation's successful
//# verification.
pub struct Witness {
    pub claim: ClaimRule<u64>,
    pub files: Vec<(u64, std::sync::Arc<CoverageReport>)>,
}

// ---------------------------------------------------------------------------
// Spec vocabulary (spec §1.4–§1.6, in Verus)
// ---------------------------------------------------------------------------

/// Spec: first-match lookup of `file_id` in a witness's files vector,
/// scanning from index `i`. FIRST-MATCH SEMANTICS: if the vector held a
/// duplicate file id (forbidden by G1), the earliest entry wins and later
/// entries are dead. `None` when no entry matches.
pub open spec fn witness_file_lookup_from(
    files: Seq<(u64, std::sync::Arc<CoverageReport>)>,
    file_id: u64,
    i: int,
) -> Option<CoverageReport>
    decreases files.len() - i,
{
    if i < 0 || i >= files.len() {
        None
    } else if files[i].0 == file_id {
        Some(*files[i].1)
    } else {
        witness_file_lookup_from(files, file_id, i + 1)
    }
}

/// Spec: the coverage report a witness carries for `file_id`, if any.
/// First-match semantics (see `witness_file_lookup_from`).
pub open spec fn witness_file_lookup(
    files: Seq<(u64, std::sync::Arc<CoverageReport>)>,
    file_id: u64,
) -> Option<CoverageReport> {
    witness_file_lookup_from(files, file_id, 0)
}

/// Spec §1.4 `executed(X, w)`, through the scoring path the engine's
/// routing selected for X's file ([`ScoringMode`], G3). Phases 1–3 here
/// are `execution_status_of` (the proven spec twin of
/// `is_annotation_executed`) for classified files and `degraded_status_of`
/// for classifier-less files; `Unscorable` never executes.
///
/// `None => false` on the map lookup is coherent with scoring: `Executed`
/// requires a Hit line, and an absent map carries none, so it coincides
/// with scoring against an empty report.
//= design/witness/spec.md#executed
//# For an annotation X and witness w:
//#
//# ```
//# executed(X, w)  ⟺  the coverage model scores X's resolved target
//#                     Executed against w.files
//# ```
//#
//# This is exactly the existing verified Phases 1–3
//# (`is_annotation_executed`, or the degraded path),
//# applied to one witness's coverage maps.
//# This specification adds no new per-annotation scoring semantics.
pub open spec fn executed_by(
    file_id: u64,
    annotation: &AnnotationSpan,
    mode: ScoringMode,
    classifications: &[Option<LineClass>],
    scopes: &[Scope],
    file_length: u64,
    w: Witness,
) -> bool {
    //= design/witness/spec.md#claim-rules
    //# `ByExecution` is the runtime rule:
    //# the report cannot record which test produced it,
    //# so the test claims the witness by evidence —
    //# its own lines are executed in it.
    //# This rule is sound only under witness individuation ([§4.2](#obligation-individuation)).
    match witness_file_lookup(w.files@, file_id) {
        //= design/witness/spec.md#executed
        //# If w's `files` contains no map for X's file at all,
        //# `executed(X, w)` is false.
        None => false,
        Some(cov) => match mode {
            ScoringMode::Classified => execution_status_of(
                annotation_target_spec(annotation, classifications, file_length),
                classifications,
                scopes,
                &cov,
            ) == ExecutionStatus::Executed,
            ScoringMode::Degraded => degraded_status_of(
                annotation_target_spec(annotation, classifications, file_length),
                &cov,
            ) == ExecutionStatus::Executed,
            ScoringMode::Unscorable => false,
        },
    }
}

/// Spec §1.5 `binds(T, w)`. Local refusal note: an `Unscorable`
/// annotation has no trustworthy resolution and binds nothing (either
/// arm) — the ByRootSpan conjunct makes the refusal explicit.
// The grammar line is quoted at the `ClaimRule` type it declares; the
// empty-target refusal sentence at the engine's resolution guard
// (duvet/src/query/engine.rs) — here it is the `target.is_some()`
// conjunct of the ByRootSpan arm.
//= design/witness/spec.md#claim-rules
//# Each witness carries one claim rule;
//# `binds` is total over (annotation, witness) pairs:
pub open spec fn binds(
    file_id: u64,
    annotation: &AnnotationSpan,
    mode: ScoringMode,
    classifications: &[Option<LineClass>],
    scopes: &[Scope],
    file_length: u64,
    w: Witness,
) -> bool {
    //= design/witness/spec.md#claim-rules
    //# binds(T, w)  ⟺  match w.claim:
    //#     ByExecution        → executed(T, w)
    //#     ByRootSpan(f, r)   → T's resolved target EXISTS and falls
    //#                           within r in file f
    match w.claim {
        //= design/witness/spec.md#property-w5-claim-refinement
        //= type=implementation
        //# The implementation MUST prove that binding under `ByExecution`
        //# implies execution of the test in the same witness:
        ClaimRule::ByExecution => executed_by(
            file_id, annotation, mode, classifications, scopes, file_length, w,
        ),
        //= design/witness/spec.md#property-w7-positional-binding-map-independence
        //= type=implementation
        //# The implementation MUST prove that binding under `ByRootSpan` does
        //# not depend on the witness's coverage maps:
        ClaimRule::ByRootSpan { file: span_file, start_line, end_line } => {
            let target = annotation_target_spec(annotation, classifications, file_length);
            &&& !(mode is Unscorable)
            &&& file_id == span_file
            //= design/witness/spec.md#claim-rules
            //= type=implication
            //# A resolution model that yields multi-line targets MUST change
            //# that specification and this binding rule explicitly.
            &&& target.is_some()
            //= design/witness/spec.md#claim-rules
            //# `ByRootSpan` is the prover rule:
            //# the witness was constructed from the annotation's own position
            //# ([§5](#prover-producers)), so ownership is positional and holds by
            //# construction
            &&& start_line <= target.unwrap() <= end_line
        },
    }
}

/// Spec §1.6.
//= design/witness/spec.md#property-w4-monotonicity
//= type=implementation
//# Adding a witness MAY newly fail a previously-discharged pair —
//# that is deliberate: the added witness is a claim T now makes, and
//# if it does not reach I it is the vacuity being caught
//# (decisions.md, [Decision 14](decisions.md#decision-14)).
pub open spec fn discharged(
    t_file_id: u64,
    t_annotation: &AnnotationSpan,
    t_mode: ScoringMode,
    t_classifications: &[Option<LineClass>],
    t_scopes: &[Scope],
    t_file_length: u64,
    i_file_id: u64,
    i_annotation: &AnnotationSpan,
    i_mode: ScoringMode,
    i_classifications: &[Option<LineClass>],
    i_scopes: &[Scope],
    i_file_length: u64,
    witnesses: Seq<Witness>,
) -> bool {
    //= design/witness/spec.md#discharge
    //# ```
    //# witnesses_for(T)  =  { w ∈ delivered : binds(T, w) }
    //#
    //# discharged(T, I)  ⟺  witnesses_for(T) ≠ ∅
    //#                       ∧  ∀w ∈ witnesses_for(T) : executed(I, w)
    //# ```
    //#
    //# In words: a pair (T, I) is discharged when the test binds at
    //# least one witness and **every** witness it binds executed the
    //# implementation. Each bound witness individually must see both
    //# sides; one bound witness that never reaches the implementation is
    //# a vacuous claim and fails the pair
    &&& exists|k: int|
        0 <= k < witnesses.len()
        && binds(t_file_id, t_annotation, t_mode, t_classifications, t_scopes, t_file_length,
            #[trigger] witnesses[k])
    //= design/witness/spec.md#property-w4-monotonicity
    //= type=implementation
    //# The implementation MUST prove that adding a witness never flips a
    //# failing pair to passing:
    &&& forall|k: int|
        0 <= k < witnesses.len()
        && binds(t_file_id, t_annotation, t_mode, t_classifications, t_scopes, t_file_length,
            #[trigger] witnesses[k])
        ==> executed_by(i_file_id, i_annotation, i_mode, i_classifications, i_scopes,
            i_file_length, witnesses[k])
}

// ---------------------------------------------------------------------------
// Well-formedness preconditions (engine adapter obligations, visible in
// signatures — zero `assume`)
// ---------------------------------------------------------------------------

/// The bounds the selected scorer requires of one annotation's scoring
/// context (annotation span headroom; scope line bounds). `Unscorable`
/// scores nothing, so it demands nothing — the engine routes ill-formed
/// contexts (e.g. `end_line == u64::MAX`) there instead of asserting
/// well-formedness it cannot establish.
pub open spec fn scoring_ctx_wf(
    mode: ScoringMode,
    annotation: &AnnotationSpan,
    scopes: &[Scope],
) -> bool {
    mode is Unscorable || {
        &&& annotation.end_line < u64::MAX
        &&& forall|i: int| 0 <= i < scopes@.len() ==> (#[trigger] scopes@[i]).close_line < u64::MAX
        &&& forall|i: int| 0 <= i < scopes@.len() ==> (#[trigger] scopes@[i]).open_line >= 1
    }
}

/// Every key of `cov` is a line within `classifications`' bounds — the
/// coverage precondition of `is_annotation_executed`, verbatim.
pub open spec fn coverage_in_bounds(
    cov: CoverageReport,
    classifications: &[Option<LineClass>],
) -> bool {
    forall|line: u64| cov@.contains_key(line)
        ==> (line as int - 1) >= 0 && (line as int - 1) < classifications@.len()
}

/// The witness's map for `file_id` (if any) is within `classifications`'
/// bounds — required only for `Classified`-mode scoring (the two-phase
/// model's precondition). The degraded scorer reads the target line
/// directly and needs no bounds; `Unscorable` reads nothing. Maps for
/// other files are unconstrained — they are never scored against these
/// classifications.
pub open spec fn witness_coverage_in_bounds(
    w: Witness,
    file_id: u64,
    mode: ScoringMode,
    classifications: &[Option<LineClass>],
) -> bool {
    mode is Classified ==> match witness_file_lookup(w.files@, file_id) {
        None => true,
        Some(cov) => coverage_in_bounds(cov, classifications),
    }
}

/// `witness_coverage_in_bounds` lifted over a delivered witness set.
pub open spec fn witnesses_coverage_in_bounds(
    ws: Seq<Witness>,
    file_id: u64,
    mode: ScoringMode,
    classifications: &[Option<LineClass>],
) -> bool {
    forall|k: int| 0 <= k < ws.len()
        ==> witness_coverage_in_bounds(#[trigger] ws[k], file_id, mode, classifications)
}

// ---------------------------------------------------------------------------
// Executable layer: per-witness cells
// ---------------------------------------------------------------------------

/// First-match lookup of a witness's coverage report for `file_id`,
/// proven equivalent to `witness_file_lookup`.
fn witness_coverage<'a>(w: &'a Witness, file_id: u64) -> (result: Option<&'a CoverageReport>)
    ensures
        result.is_some() <==> witness_file_lookup(w.files@, file_id).is_some(),
        result.is_some() ==> witness_file_lookup(w.files@, file_id) == Some(*result.unwrap()),
{
    let mut k: usize = 0;
    while k < w.files.len()
        invariant
            0 <= k <= w.files@.len(),
            witness_file_lookup(w.files@, file_id)
                == witness_file_lookup_from(w.files@, file_id, k as int),
        decreases w.files@.len() - k,
    {
        if w.files[k].0 == file_id {
            return Some(&*w.files[k].1);
        }
        k = k + 1;
    }
    None
}

/// `executed(X, w)` as executable code, proven equivalent to the
/// `executed_by` spec.
//
// The Verus `ensures` below is the machine-checked evidence for spec
// §1.4's definition: the reported result is exactly `executed_by`,
// whose arms are the proven spec twins of Phases 1–3
// (`execution_status_of` / `degraded_status_of`) and whose missing-map
// arm is `false`.
//= design/witness/spec.md#executed
//= type=test
//# For an annotation X and witness w:
//#
//# ```
//# executed(X, w)  ⟺  the coverage model scores X's resolved target
//#                     Executed against w.files
//# ```
//#
//# This is exactly the existing verified Phases 1–3
//# (`is_annotation_executed`, or the degraded path),
//# applied to one witness's coverage maps.
//# This specification adds no new per-annotation scoring semantics.
pub fn is_executed_by(
    file_id: u64,
    annotation: &AnnotationSpan,
    mode: ScoringMode,
    classifications: &[Option<LineClass>],
    scopes: &[Scope],
    file_length: u64,
    w: &Witness,
) -> (result: bool)
    requires
        scoring_ctx_wf(mode, annotation, scopes),
        witness_coverage_in_bounds(*w, file_id, mode, classifications),
    ensures
        result <==> executed_by(file_id, annotation, mode, classifications, scopes, file_length,
            *w),
{
    match mode {
        ScoringMode::Unscorable => {
            // Spec `executed_by` is false in this mode whether or not the
            // witness carries a map for the file.
            proof {
                assert(!executed_by(
                    file_id, annotation, mode, classifications, scopes, file_length, *w));
            }
            false
        },
        ScoringMode::Classified => match witness_coverage(w, file_id) {
            //= design/witness/spec.md#executed
            //= type=test
            //# If w's `files` contains no map for X's file at all,
            //# `executed(X, w)` is false.
            None => false,
            Some(cov) => {
                let status = is_annotation_executed(
                    annotation, classifications, scopes, cov, file_length);
                proof {
                    // The looked-up spec value is the map we scored against, so
                    // the spec-side status (over `&lookup.unwrap()`) equals the
                    // exec-side status (over `cov`) by congruence.
                    assert(witness_file_lookup(w.files@, file_id).unwrap() == *cov);
                }
                match status {
                    ExecutionStatus::Executed => true,
                    _ => false,
                }
            },
        },
        ScoringMode::Degraded => match witness_coverage(w, file_id) {
            None => false,
            Some(cov) => {
                let status = degraded_execution_status(
                    annotation, classifications, cov, file_length);
                proof {
                    assert(witness_file_lookup(w.files@, file_id).unwrap() == *cov);
                }
                match status {
                    ExecutionStatus::Executed => true,
                    _ => false,
                }
            },
        },
    }
}

/// `binds(T, w)` as executable code, proven equivalent to the `binds` spec.
//
// The test citations below follow the executable-carries-the-spec
// template: the `ensures result <==> binds(..)` postcondition is the
// verified evidence that this function decides spec §1.5 binding for
// EVERY (annotation, witness) pair, both claim arms — Verus is the
// checker that ran it.
//= design/witness/spec.md#claim-rules
//= type=test
//# Each witness carries one claim rule;
//# `binds` is total over (annotation, witness) pairs:
pub fn is_bound_by(
    file_id: u64,
    annotation: &AnnotationSpan,
    mode: ScoringMode,
    classifications: &[Option<LineClass>],
    scopes: &[Scope],
    file_length: u64,
    w: &Witness,
) -> (result: bool)
    requires
        scoring_ctx_wf(mode, annotation, scopes),
        witness_coverage_in_bounds(*w, file_id, mode, classifications),
    ensures
        //= design/witness/spec.md#claim-rules
        //= type=test
        //# binds(T, w)  ⟺  match w.claim:
        //#     ByExecution        → executed(T, w)
        //#     ByRootSpan(f, r)   → T's resolved target EXISTS and falls
        //#                           within r in file f
        result <==> binds(file_id, annotation, mode, classifications, scopes, file_length, *w),
{
    match &w.claim {
        ClaimRule::ByExecution => {
            is_executed_by(file_id, annotation, mode, classifications, scopes, file_length, w)
        },
        ClaimRule::ByRootSpan { file: span_file, start_line, end_line } => {
            // Unscorable annotations bind nothing (spec §1.5 refusal); the
            // guard also stands in front of `annotation_target`, whose
            // precondition (`end_line < u64::MAX`) only holds for scorable
            // contexts.
            match mode {
                ScoringMode::Unscorable => {
                    return false;
                },
                _ => {},
            }
            if file_id != *span_file {
                return false;
            }
            // Resolved-target existence and membership (spec §1.5: no
            // vacuous empty-target binding).
            match annotation_target(annotation, classifications, file_length) {
                None => false,
                Some(t) => *start_line <= t.line_number && t.line_number <= *end_line,
            }
        },
    }
}

// ---------------------------------------------------------------------------
// The report functions (engine-callable verdicts; design/witness/spec.md#engine-properties)
// ---------------------------------------------------------------------------

/// Property W1: Universal Same-Witness Discharge.
///
/// `binds(T, w)` and `executed(I, w)` are evaluated against the SAME loop
/// element `w`, so the per-witness correlation holds by construction, and
/// the iff `ensures` certifies the universal form: no pair is discharged
/// without a common witness, and no pair is discharged while ANY witness
/// bound to its test failed to reach its implementation. A bound witness
/// that did not execute I fails the pair (early `false` return).
//
// Placement: the annotation blocks are the LAST comment blocks before the
// fn header so their resolved target is the header (this file has no
// language classifier; comment lines are unclassified in degraded
// resolution and would otherwise become the target — see spec §1.1's
// placement note). The same `ensures` is the machine-checked evidence
// for spec §1.6: the reported verdict is exactly the `discharged` spec
// fn, which transcribes the quoted definition.
//= design/witness/spec.md#discharge
//= type=test
//# ```
//# witnesses_for(T)  =  { w ∈ delivered : binds(T, w) }
//#
//# discharged(T, I)  ⟺  witnesses_for(T) ≠ ∅
//#                       ∧  ∀w ∈ witnesses_for(T) : executed(I, w)
//# ```
//#
//# In words: a pair (T, I) is discharged when the test binds at
//# least one witness and **every** witness it binds executed the
//# implementation. Each bound witness individually must see both
//# sides; one bound witness that never reaches the implementation is
//# a vacuous claim and fails the pair
pub fn report_discharged(
    t_file_id: u64,
    t_annotation: &AnnotationSpan,
    t_mode: ScoringMode,
    t_classifications: &[Option<LineClass>],
    t_scopes: &[Scope],
    t_file_length: u64,
    i_file_id: u64,
    i_annotation: &AnnotationSpan,
    i_mode: ScoringMode,
    i_classifications: &[Option<LineClass>],
    i_scopes: &[Scope],
    i_file_length: u64,
    witnesses: &[Witness],
) -> (result: bool)
    requires
        scoring_ctx_wf(t_mode, t_annotation, t_scopes),
        scoring_ctx_wf(i_mode, i_annotation, i_scopes),
        witnesses_coverage_in_bounds(witnesses@, t_file_id, t_mode, t_classifications),
        witnesses_coverage_in_bounds(witnesses@, i_file_id, i_mode, i_classifications),
    ensures
        //= design/witness/spec.md#property-w1-same-witness-discharge
        //= type=test
        //# The implementation MUST prove that it reports a pair (T, I)
        //# discharged if and only if at least one delivered witness binds T
        //# and every delivered witness that binds T executed I:
        result <==> discharged(
            t_file_id, t_annotation, t_mode, t_classifications, t_scopes, t_file_length,
            i_file_id, i_annotation, i_mode, i_classifications, i_scopes, i_file_length,
            witnesses@),
{
    //= design/witness/spec.md#property-w1-same-witness-discharge
    //= type=implementation
    //# The implementation MUST prove that it reports a pair (T, I)
    //# discharged if and only if at least one delivered witness binds T
    //# and every delivered witness that binds T executed I:
    let mut any_bound = false;
    let mut k: usize = 0;
    while k < witnesses.len()
        invariant
            0 <= k <= witnesses@.len(),
            scoring_ctx_wf(t_mode, t_annotation, t_scopes),
            scoring_ctx_wf(i_mode, i_annotation, i_scopes),
            witnesses_coverage_in_bounds(witnesses@, t_file_id, t_mode, t_classifications),
            witnesses_coverage_in_bounds(witnesses@, i_file_id, i_mode, i_classifications),
            // Some witness so far binds T <==> any_bound.
            any_bound <==> exists|j: int| 0 <= j < k
                && binds(t_file_id, t_annotation, t_mode, t_classifications, t_scopes,
                    t_file_length, #[trigger] witnesses@[j]),
            // Every bound witness so far executed I (else we returned false).
            //= design/witness/spec.md#property-w1-same-witness-discharge
            //= type=test
            //# Evidence assembled from two different witnesses
            //# (T bound by one, I executed by another) MUST NOT discharge;
            //# a bound witness that did not execute I MUST fail the pair
            //# (decisions.md, [Decision 14](decisions.md#decision-14)).
            forall|j: int| 0 <= j < k
                && binds(t_file_id, t_annotation, t_mode, t_classifications, t_scopes,
                    t_file_length, #[trigger] witnesses@[j])
                ==> executed_by(i_file_id, i_annotation, i_mode, i_classifications, i_scopes,
                    i_file_length, witnesses@[j]),
        decreases witnesses@.len() - k,
    {
        let w = &witnesses[k];
        if is_bound_by(t_file_id, t_annotation, t_mode, t_classifications, t_scopes,
            t_file_length, w)
        {
            if !is_executed_by(i_file_id, i_annotation, i_mode, i_classifications, i_scopes,
                i_file_length, w)
            {
                // Witness k binds T and did not execute I: the universal
                // conjunct of `discharged` is violated by witness k.
                //= design/witness/spec.md#property-w1-same-witness-discharge
                //= type=implementation
                //# Evidence assembled from two different witnesses
                //# (T bound by one, I executed by another) MUST NOT discharge;
                //# a bound witness that did not execute I MUST fail the pair
                //# (decisions.md, [Decision 14](decisions.md#decision-14)).
                return false;
            }
            any_bound = true;
        }
        k = k + 1;
    }
    any_bound
}

/// Property W1 over a precomputed bound set: `report_discharged` with
/// the `binds` scan hoisted out. The engine evaluates one test against
/// many implementations; the bound set depends only on the test, so
/// the glue computes it once (from the verified `is_bound_by` cells)
/// and discharges each pair by checking `executed(I, ·)` over that set
/// alone.
///
/// The `ensures` is verbatim `report_discharged`'s: the verdict equals
/// spec §1.6 `discharged` over the FULL delivered set. The bound-set
/// exactness (`bound` is sound and complete for `binds(T, ·)`, by
/// index) is a precondition the glue establishes from the verified
/// cells' postconditions — never by engine-side recomputation.
// The `ensures`/`requires` pair below is the machine-checked evidence
// for G4's proviso: the verdict equals §1.6 discharge over the FULL
// delivered set, given a sound-and-complete-by-index bound set (the
// runtime half is `g4_given_bound_equals_report_discharged`).
#[allow(clippy::too_many_arguments)]
//= design/witness/spec.md#engine-glue
//= type=implementation
//# - **G4 (bound-set precomputation).** The glue MAY compute a test's
//# bound-witness set once — each membership decided by the verified
//# binding cell — and pass it to a bound-set-taking verified
//# discharge entry point, provided the verified layer proves that
//# entry point's verdict equal to [§1.6](#discharge) discharge over
//# the full delivered set whenever the supplied set is exactly
//# `witnesses_for(T)`, sound and complete by index.
pub fn report_discharged_given_bound(
    t_file_id: u64,
    t_annotation: &AnnotationSpan,
    t_mode: ScoringMode,
    t_classifications: &[Option<LineClass>],
    t_scopes: &[Scope],
    t_file_length: u64,
    i_file_id: u64,
    i_annotation: &AnnotationSpan,
    i_mode: ScoringMode,
    i_classifications: &[Option<LineClass>],
    i_scopes: &[Scope],
    i_file_length: u64,
    witnesses: &[Witness],
    bound: &[usize],
) -> (result: bool)
    requires
        scoring_ctx_wf(i_mode, i_annotation, i_scopes),
        witnesses_coverage_in_bounds(witnesses@, i_file_id, i_mode, i_classifications),
        // Sound: every listed index is in range and binds T.
        forall|j: int| 0 <= j < bound@.len() ==> (#[trigger] bound@[j]) < witnesses@.len(),
        forall|j: int| 0 <= j < bound@.len()
            ==> binds(t_file_id, t_annotation, t_mode, t_classifications, t_scopes,
                t_file_length, witnesses@[#[trigger] bound@[j] as int]),
        // Complete: every witness binding T is listed.
        forall|k: int| 0 <= k < witnesses@.len()
            && binds(t_file_id, t_annotation, t_mode, t_classifications, t_scopes,
                t_file_length, #[trigger] witnesses@[k])
            ==> exists|j: int| 0 <= j < bound@.len() && bound@[j] == k,
    ensures
        //= design/witness/spec.md#engine-glue
        //= type=test
        //# - **G4 (bound-set precomputation).** The glue MAY compute a test's
        //# bound-witness set once — each membership decided by the verified
        //# binding cell — and pass it to a bound-set-taking verified
        //# discharge entry point, provided the verified layer proves that
        //# entry point's verdict equal to [§1.6](#discharge) discharge over
        //# the full delivered set whenever the supplied set is exactly
        //# `witnesses_for(T)`, sound and complete by index.
        result <==> discharged(
            t_file_id, t_annotation, t_mode, t_classifications, t_scopes, t_file_length,
            i_file_id, i_annotation, i_mode, i_classifications, i_scopes, i_file_length,
            witnesses@),
{
    if bound.len() == 0 {
        // Completeness: nothing binds T, so `discharged`'s existential
        // conjunct is false.
        proof {
            assert forall|k: int| 0 <= k < witnesses@.len()
                implies !binds(t_file_id, t_annotation, t_mode, t_classifications, t_scopes,
                    t_file_length, #[trigger] witnesses@[k]) by {
                if binds(t_file_id, t_annotation, t_mode, t_classifications, t_scopes,
                    t_file_length, witnesses@[k]) {
                    let j = choose|j: int| 0 <= j < bound@.len() && bound@[j] == k;
                    assert(false);
                }
            }
        }
        return false;
    }
    let mut j: usize = 0;
    while j < bound.len()
        invariant
            0 <= j <= bound@.len(),
            bound@.len() > 0,
            scoring_ctx_wf(i_mode, i_annotation, i_scopes),
            witnesses_coverage_in_bounds(witnesses@, i_file_id, i_mode, i_classifications),
            forall|a: int| 0 <= a < bound@.len() ==> (#[trigger] bound@[a]) < witnesses@.len(),
            forall|a: int| 0 <= a < bound@.len()
                ==> binds(t_file_id, t_annotation, t_mode, t_classifications, t_scopes,
                    t_file_length, witnesses@[#[trigger] bound@[a] as int]),
            forall|k: int| 0 <= k < witnesses@.len()
                && binds(t_file_id, t_annotation, t_mode, t_classifications, t_scopes,
                    t_file_length, #[trigger] witnesses@[k])
                ==> exists|b: int| 0 <= b < bound@.len() && bound@[b] == k,
            // Every bound index checked so far executed I.
            forall|a: int| 0 <= a < j
                ==> executed_by(i_file_id, i_annotation, i_mode, i_classifications, i_scopes,
                    i_file_length, witnesses@[#[trigger] bound@[a] as int]),
        decreases bound@.len() - j,
    {
        let wi = bound[j];
        if !is_executed_by(i_file_id, i_annotation, i_mode, i_classifications, i_scopes,
            i_file_length, &witnesses[wi])
        {
            // bound[j] binds T (soundness) and did not execute I: the
            // universal conjunct of `discharged` is violated at index
            // bound[j].
            proof {
                let k = bound@[j as int] as int;
                assert(binds(t_file_id, t_annotation, t_mode, t_classifications, t_scopes,
                    t_file_length, witnesses@[k]));
                assert(!executed_by(i_file_id, i_annotation, i_mode, i_classifications, i_scopes,
                    i_file_length, witnesses@[k]));
            }
            return false;
        }
        j = j + 1;
    }
    // All bound indices executed I; bound is non-empty and sound, so the
    // existential conjunct holds at bound[0]; completeness carries the
    // universal conjunct from bound indices to ALL binding witnesses.
    proof {
        let k0 = bound@[0] as int;
        assert(binds(t_file_id, t_annotation, t_mode, t_classifications, t_scopes,
            t_file_length, witnesses@[k0]));
        assert forall|k: int| 0 <= k < witnesses@.len()
            && binds(t_file_id, t_annotation, t_mode, t_classifications, t_scopes,
                t_file_length, #[trigger] witnesses@[k])
            implies executed_by(i_file_id, i_annotation, i_mode, i_classifications, i_scopes,
                i_file_length, witnesses@[k]) by {
            let b = choose|b: int| 0 <= b < bound@.len() && bound@[b] == k;
            assert(executed_by(i_file_id, i_annotation, i_mode, i_classifications, i_scopes,
                i_file_length, witnesses@[bound@[b] as int]));
        }
    }
    true
}

/// Property W2: Test Execution.
//= design/witness/spec.md#property-w2-test-execution
//= type=test
//# The implementation MUST prove that a test annotation is reported
//# executed if and only if some delivered witness binds it:
//#
//# ```
//# report_test_executed(T, witnesses) = true
//#     ⟺  ∃ w ∈ witnesses : binds(T, w)
//# ```
pub fn report_test_executed(
    t_file_id: u64,
    t_annotation: &AnnotationSpan,
    t_mode: ScoringMode,
    t_classifications: &[Option<LineClass>],
    t_scopes: &[Scope],
    t_file_length: u64,
    witnesses: &[Witness],
) -> (result: bool)
    requires
        scoring_ctx_wf(t_mode, t_annotation, t_scopes),
        witnesses_coverage_in_bounds(witnesses@, t_file_id, t_mode, t_classifications),
    ensures
        result <==> exists|k: int| 0 <= k < witnesses@.len()
            && binds(t_file_id, t_annotation, t_mode, t_classifications, t_scopes, t_file_length,
                #[trigger] witnesses@[k]),
{
    //= design/witness/spec.md#property-w2-test-execution
    //= type=implementation
    //# The implementation MUST prove that a test annotation is reported
    //# executed if and only if some delivered witness binds it:
    //#
    //# ```
    //# report_test_executed(T, witnesses) = true
    //#     ⟺  ∃ w ∈ witnesses : binds(T, w)
    //# ```
    let mut k: usize = 0;
    while k < witnesses.len()
        invariant
            0 <= k <= witnesses@.len(),
            scoring_ctx_wf(t_mode, t_annotation, t_scopes),
            witnesses_coverage_in_bounds(witnesses@, t_file_id, t_mode, t_classifications),
            forall|j: int| 0 <= j < k ==> !binds(
                t_file_id, t_annotation, t_mode, t_classifications, t_scopes, t_file_length,
                #[trigger] witnesses@[j]),
        decreases witnesses@.len() - k,
    {
        if is_bound_by(t_file_id, t_annotation, t_mode, t_classifications, t_scopes,
            t_file_length, &witnesses[k])
        {
            return true;
        }
        k = k + 1;
    }
    false
}

/// Property W3: Global Execution.
//= design/witness/spec.md#property-w3-global-execution
//= type=test
//# The implementation MUST prove that an implementation annotation is
//# reported ever-executed if and only if some delivered witness
//# executed it:
//#
//# ```
//# report_ever_executed(I, witnesses) = true
//#     ⟺  ∃ w ∈ witnesses : executed(I, w)
//# ```
//#
//# This is a global property requiring no correlation;
//# it is deliberately weaker than [W1](#property-w1-same-witness-discharge)
//# and MUST NOT be used to
//# discharge pairs.
pub fn report_ever_executed(
    i_file_id: u64,
    i_annotation: &AnnotationSpan,
    i_mode: ScoringMode,
    i_classifications: &[Option<LineClass>],
    i_scopes: &[Scope],
    i_file_length: u64,
    witnesses: &[Witness],
) -> (result: bool)
    requires
        scoring_ctx_wf(i_mode, i_annotation, i_scopes),
        witnesses_coverage_in_bounds(witnesses@, i_file_id, i_mode, i_classifications),
    ensures
        result <==> exists|k: int| 0 <= k < witnesses@.len()
            && executed_by(i_file_id, i_annotation, i_mode, i_classifications, i_scopes,
                i_file_length, #[trigger] witnesses@[k]),
{
    //= design/witness/spec.md#property-w3-global-execution
    //= type=implementation
    //# This is a global property requiring no correlation;
    //# it is deliberately weaker than [W1](#property-w1-same-witness-discharge)
    //# and MUST NOT be used to
    //# discharge pairs.
    let mut k: usize = 0;
    while k < witnesses.len()
        invariant
            0 <= k <= witnesses@.len(),
            scoring_ctx_wf(i_mode, i_annotation, i_scopes),
            witnesses_coverage_in_bounds(witnesses@, i_file_id, i_mode, i_classifications),
            forall|j: int| 0 <= j < k ==> !executed_by(
                i_file_id, i_annotation, i_mode, i_classifications, i_scopes, i_file_length,
                #[trigger] witnesses@[j]),
        decreases witnesses@.len() - k,
    {
        //= design/witness/spec.md#property-w3-global-execution
        //= type=implementation
        //# The implementation MUST prove that an implementation annotation is
        //# reported ever-executed if and only if some delivered witness
        //# executed it:
        if is_executed_by(i_file_id, i_annotation, i_mode, i_classifications, i_scopes,
            i_file_length, &witnesses[k])
        {
            return true;
        }
        k = k + 1;
    }
    false
}

/// Property W6, verified half: the unwitnessed predicate. True iff NO
/// delivered witness binds T (exactly ¬W2). The reporting obligation —
/// surfacing every unwitnessed test annotation as a failure, never
/// silently — is engine behavior (glue assumption G2 names the engine's
/// duty to call this predicate for its verdicts).
//= design/witness/spec.md#property-w6-unwitnessed-test-annotations
//= type=test
//# Consequence (intended): running a subset of producers MAY fail a
//# test annotation that the full set passes;
//# that behavior is correct
//# (decisions.md, [Decision 8](decisions.md#decision-8)).
pub fn is_unwitnessed(
    t_file_id: u64,
    t_annotation: &AnnotationSpan,
    t_mode: ScoringMode,
    t_classifications: &[Option<LineClass>],
    t_scopes: &[Scope],
    t_file_length: u64,
    witnesses: &[Witness],
) -> (result: bool)
    requires
        scoring_ctx_wf(t_mode, t_annotation, t_scopes),
        witnesses_coverage_in_bounds(witnesses@, t_file_id, t_mode, t_classifications),
    ensures
        //= design/witness/spec.md#property-w6-unwitnessed-test-annotations
        //= type=test
        //# The engine MUST report every test annotation for which no
        //# delivered witness binds it —
        //# across ALL configured producers —
        //# as a failure, never silently:
        //#
        //# ```
        //# ¬∃ w ∈ witnesses : binds(T, w)   ⟹   T is reported unwitnessed
        //# ```
        result <==> !exists|k: int| 0 <= k < witnesses@.len()
            //= design/witness/spec.md#property-w6-unwitnessed-test-annotations
            //= type=implementation
            //# Consequence (intended): running a subset of producers MAY fail a
            //# test annotation that the full set passes;
            //# that behavior is correct
            //# (decisions.md, [Decision 8](decisions.md#decision-8)).
            && binds(t_file_id, t_annotation, t_mode, t_classifications, t_scopes, t_file_length,
                #[trigger] witnesses@[k]),
{
    //= design/witness/spec.md#property-w6-unwitnessed-test-annotations
    //= type=implementation
    //# The engine MUST report every test annotation for which no
    //# delivered witness binds it —
    //# across ALL configured producers —
    //# as a failure, never silently:
    //#
    //# ```
    //# ¬∃ w ∈ witnesses : binds(T, w)   ⟹   T is reported unwitnessed
    //# ```
    !report_test_executed(t_file_id, t_annotation, t_mode, t_classifications, t_scopes,
        t_file_length, witnesses)
}

// ---------------------------------------------------------------------------
// Proof-only properties (no report function; design/witness/spec.md#engine-properties)
// ---------------------------------------------------------------------------

/// Spec: every witness of `ws` occurs (as an equal value) in `ws2`.
/// Witness sets are `Seq`-valued with no uniqueness assumption
/// (spec §5.3 ties): duplicates and multiple witnesses per
/// annotation are allowed, and membership is all the ∃-quantified
/// properties ever inspect.
pub open spec fn witness_subset(ws: Seq<Witness>, ws2: Seq<Witness>) -> bool {
    forall|k: int| 0 <= k < ws.len()
        ==> exists|j: int| 0 <= j < ws2.len() && ws2[j] == #[trigger] ws[k]
}

/// Property W4: Failure Monotonicity (the deliberate inversion —
/// spec W4). The form proved here is the equivalent
/// case split on whether T is witnessed in `ws`.
///
/// Proof shape (falls out of the ∀, as the design analysis predicted): if T
/// is witnessed in `ws` and the pair fails in `ws`, the failure is some
/// bound witness in `ws` that did not execute I (the ∃-conjunct holds, so
/// the ∀-conjunct must be what failed). That witness transports into `ws'`
/// by subset, still binds, still misses I — so it violates `ws'`'s
/// ∀-conjunct too, and the pair fails in `ws'`.
//= design/witness/spec.md#property-w4-monotonicity
//= type=test
//# The implementation MUST prove that adding a witness never flips a
//# failing pair to passing:
//#
//# ```
//# witnesses ⊆ witnesses'  ⟹
//#     (report_discharged(T, I, witnesses')
//#         ⟹ report_discharged(T, I, witnesses)
//#            ∨ ¬∃ w ∈ witnesses : binds(T, w))
//# ```
//#
//# Adding a witness MAY newly fail a previously-discharged pair —
//# that is deliberate: the added witness is a claim T now makes, and
//# if it does not reach I it is the vacuity being caught
//# (decisions.md, [Decision 14](decisions.md#decision-14)).
//# The only way adding witnesses turns a non-discharged pair into a
//# discharged one is by witnessing a previously *unwitnessed* test
//# (the `witnesses_for(T) = ∅` case), never by outvoting a bound
//# witness that failed.
pub proof fn failure_monotonicity(
    t_file_id: u64,
    t_annotation: &AnnotationSpan,
    t_mode: ScoringMode,
    t_classifications: &[Option<LineClass>],
    t_scopes: &[Scope],
    t_file_length: u64,
    i_file_id: u64,
    i_annotation: &AnnotationSpan,
    i_mode: ScoringMode,
    i_classifications: &[Option<LineClass>],
    i_scopes: &[Scope],
    i_file_length: u64,
    ws: Seq<Witness>,
    ws2: Seq<Witness>,
)
    requires
        witness_subset(ws, ws2),
        discharged(t_file_id, t_annotation, t_mode, t_classifications, t_scopes, t_file_length,
            i_file_id, i_annotation, i_mode, i_classifications, i_scopes, i_file_length, ws2),
    ensures
        discharged(t_file_id, t_annotation, t_mode, t_classifications, t_scopes, t_file_length,
            i_file_id, i_annotation, i_mode, i_classifications, i_scopes, i_file_length, ws)
        || !exists|k: int| 0 <= k < ws.len()
            && binds(t_file_id, t_annotation, t_mode, t_classifications, t_scopes, t_file_length,
                #[trigger] ws[k]),
{
    if exists|k: int| 0 <= k < ws.len()
        && binds(t_file_id, t_annotation, t_mode, t_classifications, t_scopes, t_file_length,
            #[trigger] ws[k])
    {
        // T is witnessed in ws: show discharged(ws). The ∃-conjunct holds by
        // hypothesis; for the ∀-conjunct, every bound witness of ws occurs
        // (as an equal value) in ws2, where discharged(ws2)'s ∀-conjunct
        // forces it to have executed I.
        assert forall|k: int|
            0 <= k < ws.len()
            && binds(t_file_id, t_annotation, t_mode, t_classifications, t_scopes, t_file_length,
                #[trigger] ws[k])
        implies executed_by(i_file_id, i_annotation, i_mode, i_classifications, i_scopes,
            i_file_length, ws[k])
        by {
            // Transport ws[k] into ws2 via the subset hypothesis.
            assert(exists|j: int| 0 <= j < ws2.len() && ws2[j] == ws[k]);
            let j = choose|j: int| 0 <= j < ws2.len() && ws2[j] == ws[k];
            // ws2[j] binds T (substitution), so ws2's ∀-conjunct applies.
            assert(binds(t_file_id, t_annotation, t_mode, t_classifications, t_scopes,
                t_file_length, ws2[j]));
            assert(executed_by(i_file_id, i_annotation, i_mode, i_classifications, i_scopes,
                i_file_length, ws2[j]));
        }
    }
}

/// Property W5: Claim Refinement.
///
/// DEFINITIONAL in this formalization, trivially true: `binds` under
/// `ByExecution` is *defined as* `executed_by` (spec §1.5's match arm,
/// transcribed). The proof is a one-line unfold. Stated anyway as a
/// REGRESSION TRIPWIRE: if `binds`'s `ByExecution` arm ever changes so that
/// positional claiming stops being a refinement of evidence claiming, this
/// proof breaks loudly instead of the property silently weakening.
// The same proof is the verified evidence for the rule's definition in
// spec §1.5: binding under `ByExecution` IS evidence — the test's own
// lines executed in the witness — never anything weaker.
//= design/witness/spec.md#claim-rules
//= type=test
//# `ByExecution` is the runtime rule:
//# the report cannot record which test produced it,
//# so the test claims the witness by evidence —
//# its own lines are executed in it.
//# This rule is sound only under witness individuation ([§4.2](#obligation-individuation)).
pub proof fn by_execution_binding_implies_executed(
    file_id: u64,
    annotation: &AnnotationSpan,
    mode: ScoringMode,
    classifications: &[Option<LineClass>],
    scopes: &[Scope],
    file_length: u64,
    w: Witness,
)
    requires
        w.claim == ClaimRule::ByExecution,
        binds(file_id, annotation, mode, classifications, scopes, file_length, w),
    ensures
        //= design/witness/spec.md#property-w5-claim-refinement
        //= type=test
        //# The implementation MUST prove that binding under `ByExecution`
        //# implies execution of the test in the same witness:
        executed_by(file_id, annotation, mode, classifications, scopes, file_length, w),
{
    // Definitional: with `w.claim == ByExecution`, `binds` unfolds to
    // `executed_by` — nothing to prove beyond the unfold. Mode-uniform:
    // the refinement holds under every scoring mode.
}

/// REGRESSION TRIPWIRE: if `binds`'s `ByRootSpan` arm ever grows a
/// map-consulting conjunct, this proof breaks loudly.
// The same proof is the verified evidence for the rule's definition in
// spec §1.5: positional ownership is a function of the root span alone
// — "by construction" made checkable.
//= design/witness/spec.md#claim-rules
//= type=test
//# `ByRootSpan` is the prover rule:
//# the witness was constructed from the annotation's own position
//# ([§5](#prover-producers)), so ownership is positional and holds by
//# construction
pub proof fn positional_binding_is_map_independent(
    file_id: u64,
    annotation: &AnnotationSpan,
    mode: ScoringMode,
    classifications: &[Option<LineClass>],
    scopes: &[Scope],
    file_length: u64,
    w1: Witness,
    w2: Witness,
)
    requires
        w1.claim == w2.claim,
        w1.claim is ByRootSpan,
    ensures
        //= design/witness/spec.md#property-w7-positional-binding-map-independence
        //= type=test
        //# The implementation MUST prove that binding under `ByRootSpan` does
        //# not depend on the witness's coverage maps:
        //#
        //# ```
        //# w.claim = w'.claim = ByRootSpan(f, r)
        //#     ⟹  (binds(T, w) ⟺ binds(T, w'))
        //# ```
        //#
        //# Which annotations a positional witness binds is a function of its
        //# root span alone. Consequences: enlarging a proof witness's closure
        //# never extends the set of test annotations it witnesses, and a
        //# witness whose maps Hit-cover another test annotation's lines still
        //# does not witness it — one proof's witness cannot capture another
        //# proof's test annotation, however deep the dependency chain between
        //# the proofs. This is [W5](#property-w5-claim-refinement)'s
        //# complement: `ByExecution` binding is exactly map evidence;
        //# `ByRootSpan` binding is exactly geometry.
        binds(file_id, annotation, mode, classifications, scopes, file_length, w1)
            == binds(file_id, annotation, mode, classifications, scopes, file_length, w2),
{
    // Definitional: the `ByRootSpan` arm of `binds` mentions only the
    // claim's fields and the annotation's resolved target — never
    // `w.files` — so equal claims give equal binding verdicts.
}

} // verus!

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;

    fn s(props: &[LineProperty]) -> Option<LineClass> {
        Some(line_class(props))
    }
    fn cov_hit(lines: &[u64]) -> std::sync::Arc<CoverageReport> {
        std::sync::Arc::new(lines.iter().map(|&l| (l, CoverageStatus::Hit)).collect())
    }

    // File 1 (the test annotation's file):
    //   1: Annotation, 2: Annotation, 3: Statement  <- T's target
    const T_FILE: u64 = 1;
    fn t_classifications() -> Vec<Option<LineClass>> {
        vec![
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::Statement]),
        ]
    }
    fn t_annotation() -> AnnotationSpan {
        AnnotationSpan {
            start_line: 1,
            end_line: 2,
        }
    }

    // File 2 (the implementation annotation's file):
    //   1: Annotation, 2: Statement  <- I's target
    const I_FILE: u64 = 2;
    fn i_classifications() -> Vec<Option<LineClass>> {
        vec![
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::Statement]),
        ]
    }
    fn i_annotation() -> AnnotationSpan {
        AnnotationSpan {
            start_line: 1,
            end_line: 1,
        }
    }

    fn w_both() -> Witness {
        Witness {
            claim: ClaimRule::ByExecution,
            files: vec![(T_FILE, cov_hit(&[3])), (I_FILE, cov_hit(&[2]))],
        }
    }
    fn w_t_only() -> Witness {
        Witness {
            claim: ClaimRule::ByExecution,
            files: vec![(T_FILE, cov_hit(&[3]))],
        }
    }
    fn w_i_only() -> Witness {
        Witness {
            claim: ClaimRule::ByExecution,
            files: vec![(I_FILE, cov_hit(&[2]))],
        }
    }

    fn discharged_verdict(witnesses: &[Witness]) -> bool {
        report_discharged(
            T_FILE,
            &t_annotation(),
            ScoringMode::Classified,
            &t_classifications(),
            &[],
            3,
            I_FILE,
            &i_annotation(),
            ScoringMode::Classified,
            &i_classifications(),
            &[],
            2,
            witnesses,
        )
    }

    /// W1 (universal form): one bound witness that executes I
    /// discharges; evidence split across two witnesses does NOT; and a
    /// bound witness that misses I fails the pair even when another bound
    /// witness covers it — no outvoting.
    #[test]
    fn w1_universal_same_witness_discharge() {
        assert!(discharged_verdict(&[w_both()]));
        // Split evidence: w_t_only binds T but never reaches I.
        assert!(!discharged_verdict(&[w_t_only(), w_i_only()]));
        // Unwitnessed T.
        assert!(!discharged_verdict(&[]));
        // The new behavior the quantifier flip exists for: adding a bound
        // witness that misses I fails a pair that w_both alone discharged.
        assert!(discharged_verdict(&[w_both()]));
        assert!(!discharged_verdict(&[w_both(), w_t_only()]));
    }

    /// W3 is deliberately weaker than W1: the split-witness set that fails
    /// discharge still reports I as ever-executed.
    #[test]
    fn w3_global_execution_no_correlation() {
        let ws = [w_t_only(), w_i_only()];
        assert!(!discharged_verdict(&ws));
        assert!(report_ever_executed(
            I_FILE,
            &i_annotation(),
            ScoringMode::Classified,
            &i_classifications(),
            &[],
            2,
            &ws,
        ));
        assert!(!report_ever_executed(
            I_FILE,
            &i_annotation(),
            ScoringMode::Classified,
            &i_classifications(),
            &[],
            2,
            &[w_t_only()],
        ));
    }

    /// W2 / W6: bound iff some witness binds; unwitnessed is the negation.
    //
    // The w_i_only cases are the own-witness requirement's test evidence:
    // a delivered witness that executed something ELSE does not count for
    // T — T is unwitnessed until a witness binds T itself.
    #[test]
    fn w2_w6_test_execution_and_unwitnessed() {
        let t = t_annotation();
        let c = t_classifications();
        assert!(report_test_executed(
            T_FILE,
            &t,
            ScoringMode::Classified,
            &c,
            &[],
            3,
            &[w_t_only()]
        ));
        //= design/witness/spec.md#claim-rules
        //= type=test
        //# A test annotation must find *its own* witness.
        assert!(!report_test_executed(
            T_FILE,
            &t,
            ScoringMode::Classified,
            &c,
            &[],
            3,
            &[w_i_only()]
        ));
        assert!(is_unwitnessed(
            T_FILE,
            &t,
            ScoringMode::Classified,
            &c,
            &[],
            3,
            &[]
        ));
        assert!(is_unwitnessed(
            T_FILE,
            &t,
            ScoringMode::Classified,
            &c,
            &[],
            3,
            &[w_i_only()]
        ));
        assert!(!is_unwitnessed(
            T_FILE,
            &t,
            ScoringMode::Classified,
            &c,
            &[],
            3,
            &[w_t_only()]
        ));
    }

    /// ByRootSpan binds iff the resolved target exists, the file matches,
    /// and the target falls within the (inclusive) range.
    #[test]
    fn by_root_span_binding() {
        let t = t_annotation();
        let c = t_classifications();
        let root = |file_id, start_line, end_line| Witness {
            claim: ClaimRule::ByRootSpan {
                file: file_id,
                start_line,
                end_line,
            },
            files: vec![(I_FILE, cov_hit(&[2]))],
        };
        // T's target is line 3 in file 1.
        assert!(report_test_executed(
            T_FILE,
            &t,
            ScoringMode::Classified,
            &c,
            &[],
            3,
            &[root(T_FILE, 2, 4)]
        ));
        assert!(report_test_executed(
            T_FILE,
            &t,
            ScoringMode::Classified,
            &c,
            &[],
            3,
            &[root(T_FILE, 3, 3)]
        ));
        // Outside the range, or wrong file: no bind.
        assert!(!report_test_executed(
            T_FILE,
            &t,
            ScoringMode::Classified,
            &c,
            &[],
            3,
            &[root(T_FILE, 4, 9)]
        ));
        assert!(!report_test_executed(
            T_FILE,
            &t,
            ScoringMode::Classified,
            &c,
            &[],
            3,
            &[root(I_FILE, 2, 4)]
        ));
        // A ByRootSpan witness carrying I's coverage discharges the pair
        // (prover-shaped witness: positional claim + closure map).
        assert!(discharged_verdict(&[root(T_FILE, 2, 4)]));
    }

    /// The fat-witness capture question, both claim rules head-to-head
    /// (ryanemer, 2026-07-31). One coverage map — T's own lines Hit
    /// (consulted by some other proof's closure), I's lines absent — carried
    /// by two witnesses differing ONLY in claim rule:
    ///
    /// - `ByRootSpan` rooted elsewhere: `binds` is pure geometry and never
    ///   reads the map, so the witness does not witness T and does not
    ///   enter the pair's ∀-set. T stays unwitnessed; the pair is neither
    ///   passed nor failed by it.
    /// - `ByExecution`: the same map DOES bind T (evidence rule) — the
    ///   trench coat — and under W1 the bound witness that never
    ///   reaches I fails the pair.
    ///
    /// The difference between the fat proof witness and the trench coat is
    /// exactly the claim rule the witness carries.
    //
    // That head-to-head contrast — one map, the two grammar alternatives,
    // opposite binding verdicts — is the test evidence for the ClaimRule
    // grammar itself: the two rules exist and are semantically distinct.
    #[test]
    // The whole test is the contrast that verifies the grammar — no single
    // assertion discharges it — so the quote sits on the fn.
    //= design/witness/spec.md#claim-rules
    //= type=test
    //# ClaimRule ::= ByExecution | ByRootSpan(file, line_range)
    fn fat_witness_map_containing_t_does_not_capture_t_by_root_span() {
        let t = t_annotation();
        let c = t_classifications();
        // T's target is line 3 in T_FILE; the map Hit-covers it.
        let fat_map = vec![(T_FILE, cov_hit(&[3])), (I_FILE, cov_hit(&[]))];
        let by_root_elsewhere = Witness {
            claim: ClaimRule::ByRootSpan {
                file: T_FILE,
                start_line: 7,
                end_line: 9,
            },
            files: fat_map.clone(),
        };
        // executed(T, w) is TRUE for this witness...
        assert!(is_executed_by(
            T_FILE,
            &t,
            ScoringMode::Classified,
            &c,
            &[],
            3,
            &by_root_elsewhere
        ));
        // ...but it does not witness T (binding never consults the map)...
        assert!(!report_test_executed(
            T_FILE,
            &t,
            ScoringMode::Classified,
            &c,
            &[],
            3,
            std::slice::from_ref(&by_root_elsewhere)
        ));
        // ...so the pair is untouched: not discharged (unwitnessed), and
        // the fat witness cannot fail it either — it is not in the ∀-set.
        assert!(!discharged_verdict(std::slice::from_ref(
            &by_root_elsewhere
        )));
        // The SAME map under ByExecution is the trench coat: it binds T by
        // evidence, and having never reached I, fails the pair (W1).
        let trench_coat = Witness {
            claim: ClaimRule::ByExecution,
            files: fat_map,
        };
        assert!(report_test_executed(
            T_FILE,
            &t,
            ScoringMode::Classified,
            &c,
            &[],
            3,
            std::slice::from_ref(&trench_coat)
        ));
        assert!(!discharged_verdict(&[trench_coat]));
    }

    /// Spec §1.5 no-vacuous-binding: an annotation with no resolved target
    /// (pure scope-close follows it) binds NO ByRootSpan witness, even one
    /// whose range would contain the annotation's own lines.
    #[test]
    fn by_root_span_never_binds_empty_target() {
        // 1: Annotation, 2: pure ScopeClose -> no resolved target.
        let c = vec![
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::ScopeClose]),
        ];
        let t = AnnotationSpan {
            start_line: 1,
            end_line: 1,
        };
        let w = Witness {
            claim: ClaimRule::ByRootSpan {
                file: T_FILE,
                start_line: 1,
                end_line: 100,
            },
            files: vec![],
        };
        //= design/witness/spec.md#claim-rules
        //= type=test
        //# An annotation with no resolved target (e.g. a Structural
        //# annotation) binds no ByRootSpan witness —
        //# empty-target containment MUST NOT bind vacuously.
        assert!(!report_test_executed(
            T_FILE,
            &t,
            ScoringMode::Classified,
            &c,
            &[],
            2,
            &[w]
        ));
    }

    /// W4 (Failure Monotonicity): adding a witness that does
    /// NOT bind T preserves discharge; adding one that binds T and misses I
    /// deliberately fails the pair (the inversion of the old property); and
    /// the only pass-creating addition is witnessing a previously
    /// unwitnessed T. The general theorem is `failure_monotonicity`
    /// (proof fn); this exercises the concrete report path.
    #[test]
    fn w4_failure_monotonicity() {
        // Non-binding additions never flip the verdict.
        assert!(discharged_verdict(&[w_both()]));
        assert!(discharged_verdict(&[w_both(), w_i_only()]));
        // A binding addition that misses I fails the pair — by design.
        assert!(!discharged_verdict(&[w_both(), w_t_only()]));
        // A failing-but-witnessed pair stays failing in every superset.
        assert!(!discharged_verdict(&[w_t_only()]));
        assert!(!discharged_verdict(&[w_t_only(), w_both(), w_i_only()]));
        // The one legitimate fail->pass transition: T was unwitnessed.
        assert!(!discharged_verdict(&[w_i_only()]));
        assert!(discharged_verdict(&[w_i_only(), w_both()]));
    }

    /// The ambiguity ruling (spec §5.3): at a position rooting N obligations
    /// (N ByRootSpan witnesses binding the same T), ALL N must execute I —
    /// no best-of-N.
    ///
    /// This is also the load-bearing consequence of witness individuation
    /// (spec §1.2): each witness records ONE act of checking, so two
    /// obligations at one position are two witnesses, and each must
    /// individually reach I. If acts were merged into one record, this
    /// scenario would be inexpressible and the assertion would flip.
    #[test]
    fn decision_14_all_bound_root_span_witnesses_must_execute() {
        let root_with_i = Witness {
            claim: ClaimRule::ByRootSpan {
                file: T_FILE,
                start_line: 2,
                end_line: 4,
            },
            files: vec![(I_FILE, cov_hit(&[2]))],
        };
        let root_without_i = Witness {
            claim: ClaimRule::ByRootSpan {
                file: T_FILE,
                start_line: 2,
                end_line: 4,
            },
            files: vec![],
        };
        // One bound obligation-witness reaching I: discharged.
        assert!(discharged_verdict(std::slice::from_ref(&root_with_i)));
        // Two obligations own the position; only one reaches I: FAILS.
        //= design/witness/spec.md#witness
        //= type=test
        //# A witness is the record of **one act of checking**:
        //# one test's execution, or one prover obligation's successful
        //# verification.
        assert!(!discharged_verdict(&[root_with_i, root_without_i]));
    }

    /// First-match lookup: the earliest entry for a file id wins. (G1
    /// forbids duplicates; this pins the documented semantics if one ever
    /// slips through — the later map is dead, so T scores against the
    /// first, empty map and is NOT executed.)
    #[test]
    fn duplicate_file_id_first_match_wins() {
        let w = Witness {
            claim: ClaimRule::ByExecution,
            files: vec![
                (T_FILE, std::sync::Arc::new(CoverageReport::new())),
                (T_FILE, cov_hit(&[3])),
            ],
        };
        assert!(!report_test_executed(
            T_FILE,
            &t_annotation(),
            ScoringMode::Classified,
            &t_classifications(),
            &[],
            3,
            &[w],
        ));
    }

    /// G3 mode routing, degraded arm: a file with no classifier scores
    /// through the verified degraded path (forward-nearest governance).
    /// The same witness set that discharges under Classified discharges
    /// under Degraded when the coverage hits the forward-nearest lines,
    /// and the split-evidence case still fails (the quantifier layer is
    /// mode-uniform).
    #[test]
    fn degraded_mode_scores_via_the_degraded_path() {
        // Degraded classification: annotation lines known, code lines None.
        let t_c = vec![
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::Annotation]),
            None,
        ];
        let i_c = vec![s(&[LineProperty::Annotation]), None];
        let verdict = |witnesses: &[Witness]| {
            report_discharged(
                T_FILE,
                &t_annotation(),
                ScoringMode::Degraded,
                &t_c,
                &[],
                3,
                I_FILE,
                &i_annotation(),
                ScoringMode::Degraded,
                &i_c,
                &[],
                2,
                witnesses,
            )
        };
        //= design/witness/spec.md#executed
        //= type=test
        //# The verified Phase 4 layer implements the "or" per file: a
        //# `ScoringMode` routes each file to the classified or the degraded
        //# scorer, and
        assert!(verdict(&[w_both()]));
        assert!(!verdict(&[w_t_only(), w_i_only()]));
        assert!(!verdict(&[w_both(), w_t_only()]));
        // A miss on the forward-nearest line is a direct-observation
        // NotExecuted: binds nothing under ByExecution.
        let w_miss = Witness {
            claim: ClaimRule::ByExecution,
            files: vec![(
                T_FILE,
                std::sync::Arc::new([(3u64, CoverageStatus::Miss)].into_iter().collect()),
            )],
        };
        assert!(is_unwitnessed(
            T_FILE,
            &t_annotation(),
            ScoringMode::Degraded,
            &t_c,
            &[],
            3,
            &[w_miss],
        ));
    }

    /// G3 mode routing, unscorable arm: the trust-boundary refusal binds
    /// nothing and executes nothing — even when the witness carries a map
    /// for the file or a root span that would otherwise contain the
    /// target. The verdict contribution is uniformly false; the pair is
    /// unwitnessed, never discharged.
    #[test]
    fn unscorable_binds_nothing_and_executes_nothing() {
        let root = Witness {
            claim: ClaimRule::ByRootSpan {
                file: T_FILE,
                start_line: 1,
                end_line: 100,
            },
            files: vec![(I_FILE, cov_hit(&[2]))],
        };
        //= design/witness/spec.md#executed
        //= type=test
        //# and engine trust-boundary refusals are encoded as
        //# `Unscorable` — binds nothing, executes nothing
        assert!(is_unwitnessed(
            T_FILE,
            &t_annotation(),
            ScoringMode::Unscorable,
            &[],
            &[],
            0,
            &[w_both(), root.clone()],
        ));
        assert!(!report_discharged(
            T_FILE,
            &t_annotation(),
            ScoringMode::Unscorable,
            &[],
            &[],
            0,
            I_FILE,
            &i_annotation(),
            ScoringMode::Classified,
            &i_classifications(),
            &[],
            2,
            &[w_both(), root],
        ));
        // Unscorable on the implementation side: a bound witness can never
        // execute I, so the pair fails.
        assert!(!report_discharged(
            T_FILE,
            &t_annotation(),
            ScoringMode::Classified,
            &t_classifications(),
            &[],
            3,
            I_FILE,
            &i_annotation(),
            ScoringMode::Unscorable,
            &[],
            &[],
            0,
            &[w_both()],
        ));
        assert!(!report_ever_executed(
            I_FILE,
            &i_annotation(),
            ScoringMode::Unscorable,
            &[],
            &[],
            0,
            &[w_both()],
        ));
    }

    /// The glue's bound-set assembly, mirrored exactly (spec §4.4 G4):
    /// the indices where the verified `is_bound_by` cell returns true,
    /// over the same test context and witness list. The context is
    /// constructed once, as the glue's cached `ctx_of` does.
    fn bound_from_cells(witnesses: &[Witness]) -> Vec<usize> {
        let t = t_annotation();
        let c = t_classifications();
        (0..witnesses.len())
            .filter(|&wi| {
                is_bound_by(
                    T_FILE,
                    &t,
                    ScoringMode::Classified,
                    &c,
                    &[],
                    3,
                    &witnesses[wi],
                )
            })
            .collect()
    }

    fn discharged_given_bound(witnesses: &[Witness], bound: &[usize]) -> bool {
        report_discharged_given_bound(
            T_FILE,
            &t_annotation(),
            ScoringMode::Classified,
            &t_classifications(),
            &[],
            3,
            I_FILE,
            &i_annotation(),
            ScoringMode::Classified,
            &i_classifications(),
            &[],
            2,
            witnesses,
            bound,
        )
    }

    /// G4 equivalence, runtime half (the proved half is the identical
    /// `ensures` of the two entry points): over every witness-set shape
    /// the W1 tests exercise, `report_discharged_given_bound` with the
    /// cell-assembled bound set returns exactly `report_discharged`.
    #[test]
    fn g4_given_bound_equals_report_discharged() {
        let sets: Vec<Vec<Witness>> = vec![
            vec![],
            vec![w_both()],
            vec![w_t_only()],
            vec![w_i_only()],
            vec![w_t_only(), w_i_only()],
            vec![w_both(), w_t_only()],
            vec![w_both(), w_i_only()],
            vec![w_i_only(), w_both(), w_t_only(), w_both()],
        ];
        for ws in &sets {
            let bound = bound_from_cells(ws);
            assert_eq!(
                discharged_given_bound(ws, &bound),
                discharged_verdict(ws),
                "verdicts diverged for witness set of len {} (bound {bound:?})",
                ws.len()
            );
        }
    }

    /// Perf harness (run explicitly: `cargo test --release -p
    /// duvet-coverage bench_discharge -- --ignored --nocapture`): the
    /// engine's per-test shape — k implementations against one test —
    /// as k full-scan `report_discharged` calls vs one bound scan plus
    /// k `report_discharged_given_bound` calls.
    #[test]
    #[ignore = "perf harness, run explicitly with --ignored --nocapture"]
    fn bench_discharge_bound_hoisting() {
        // 400 witnesses, 4 of which bind T. The non-binding ones still
        // carry a T-file map (a miss at the target line), so each binds
        // check pays the real scoring cost — as engine witnesses do —
        // rather than short-circuiting on a missing file entry.
        let w_t_miss = || Witness {
            claim: ClaimRule::ByExecution,
            files: vec![
                (
                    T_FILE,
                    std::sync::Arc::new([(3u64, CoverageStatus::Miss)].into_iter().collect()),
                ),
                (I_FILE, cov_hit(&[2])),
            ],
        };
        let mut ws: Vec<Witness> = Vec::new();
        for i in 0..400 {
            ws.push(if i % 100 == 0 { w_both() } else { w_t_miss() });
        }
        let k = 25u32;
        let iters = 50u32;

        let t0 = std::time::Instant::now();
        for _ in 0..iters {
            for _ in 0..k {
                std::hint::black_box(discharged_verdict(&ws));
            }
        }
        let full = t0.elapsed();

        let t1 = std::time::Instant::now();
        for _ in 0..iters {
            let bound = bound_from_cells(&ws);
            for _ in 0..k {
                std::hint::black_box(discharged_given_bound(&ws, &bound));
            }
        }
        let hoisted = t1.elapsed();
        println!(
            "bench_discharge_bound_hoisting: witnesses={} impls={} iters={} \
             full-scan={:?} bound-hoisted={:?}",
            ws.len(),
            k,
            iters,
            full,
            hoisted
        );
    }
}
