// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Phase 4: Witness Quantifier Layer (design/witness/spec.md Section 2).
//!
//! The quantifier layer over the existing per-annotation cells: Phases 1–3
//! score ONE annotation against ONE coverage map (`is_annotation_executed`);
//! this phase states and proves the load-bearing quantifiers over a set of
//! delivered witnesses — the spec's engine properties
//! (design/witness/spec.md#engine-properties). `executed(X, w)` is exactly
//! the existing
//! verified scoring applied to w's map for X's file (spec §1.4); nothing in
//! Phases 1–3 is re-specified here.
//!
//! Named glue assumptions (trusted base, NOT verified here) are specified
//! in design/witness/spec.md §4.4 (#engine-glue): **G1** (file identity —
//! the adapter
//! delivers injective, duplicate-free file ids), **G2** (call obligation —
//! the engine computes every verdict by calling this layer's functions;
//! no parallel verdict computation exists), and **G3** (mode routing —
//! each annotation's file is scored in the [`ScoringMode`] its
//! classification actually selected).
//! Producer obligations A1 (closedness) and A2 (individuation) per spec §4.
//!
//! The `requires` on the report functions (coverage keys within
//! classification bounds for `Classified`-mode scoring; scope line bounds)
//! are the engine adapter's obligation to establish at the trust boundary —
//! filter/degrade before calling, never assume
//! (design/witness/spec.md#engine-glue, G3).

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
/// - `Unscorable` — the engine's trust-boundary refusals (defeated
///   classification, `end_line == u64::MAX`, unclassified file): the
///   annotation resolves nothing, so it executes in no witness and binds no
///   witness (its `Unknown` diagnostic is engine reporting; the verdict
///   contribution is uniformly `false`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScoringMode {
    Classified,
    Degraded,
    Unscorable,
}

/// How a test annotation claims a witness (spec §1.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaimRule {
    /// Runtime rule: T claims w by evidence — T's own lines are executed
    /// in w. Sound only under witness individuation (spec §4.2, axiom A2).
    ByExecution,
    /// Prover rule: ownership is positional — T's resolved target falls
    /// within the discharge unit's extent. File identity is an opaque id
    /// (glue assumption G1); the line range is inclusive.
    ByRootSpan { file_id: u64, start_line: u64, end_line: u64 },
}

/// The record of ONE act of checking (spec §1.2), verified-model projection:
/// claim rule plus per-file coverage. Label and provenance are engine
/// concerns and deliberately absent. `files` maps an opaque file id (G1) to
/// the existing verified `CoverageReport` type; the multi-file map lives
/// INSIDE the verified witness so that W1's same-witness conjunction is over
/// one object — projecting per-file in glue would reintroduce the
/// correlation bug's shape (decisions.md, Relationship section).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Witness {
    pub claim: ClaimRule,
    pub files: Vec<(u64, CoverageReport)>,
}

// ---------------------------------------------------------------------------
// Spec vocabulary (spec §1.4–§1.6, in Verus)
// ---------------------------------------------------------------------------

/// Spec: first-match lookup of `file_id` in a witness's files vector,
/// scanning from index `i`. FIRST-MATCH SEMANTICS: if the vector held a
/// duplicate file id (forbidden by G1), the earliest entry wins and later
/// entries are dead. `None` when no entry matches.
pub open spec fn witness_file_lookup_from(
    files: Seq<(u64, CoverageReport)>,
    file_id: u64,
    i: int,
) -> Option<CoverageReport>
    decreases files.len() - i,
{
    if i < 0 || i >= files.len() {
        None
    } else if files[i].0 == file_id {
        Some(files[i].1)
    } else {
        witness_file_lookup_from(files, file_id, i + 1)
    }
}

/// Spec: the coverage report a witness carries for `file_id`, if any.
/// First-match semantics (see `witness_file_lookup_from`).
pub open spec fn witness_file_lookup(
    files: Seq<(u64, CoverageReport)>,
    file_id: u64,
) -> Option<CoverageReport> {
    witness_file_lookup_from(files, file_id, 0)
}

/// Spec §1.4: `executed(X, w)` — the coverage model scores X's resolved
/// target Executed against w's map for X's file, through the scoring path
/// the engine's routing selected for that file ([`ScoringMode`], G3):
/// the existing verified Phases 1–3 (`execution_status_of`, the proven spec
/// twin of `is_annotation_executed`) for classified files, the verified
/// degraded path (`degraded_status_of`) for classifier-less files. This
/// phase adds no per-annotation scoring semantics; `Unscorable` is the
/// trust-boundary refusal and never executes.
///
/// A witness with no map for X's file cannot have executed X: `Executed`
/// requires a Hit line, and an absent map carries none, so `None => false`
/// coincides with scoring against an empty report (definitional choice,
/// recorded here).
pub open spec fn executed_by(
    file_id: u64,
    annotation: &AnnotationSpan,
    mode: ScoringMode,
    classifications: &[Option<LineClass>],
    scopes: &[Scope],
    file_length: u64,
    w: Witness,
) -> bool {
    match witness_file_lookup(w.files@, file_id) {
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

/// Spec §1.5: `binds(T, w)` — total over (annotation, witness) pairs.
///
/// - `ByExecution` → `executed(T, w)` (the runtime evidence rule).
/// - `ByRootSpan(f, r)` → T's resolved target EXISTS and falls within r in
///   file f. An annotation with no resolved target binds no ByRootSpan
///   witness — empty-target containment MUST NOT bind vacuously (spec §1.5).
///   Resolution yields at most one target line in the current model, so
///   containment is membership of that single line. An `Unscorable`
///   annotation has no trustworthy resolution and binds nothing (either
///   arm): the ByRootSpan conjunct makes the refusal explicit.
//= design/witness/spec.md#property-w5-claim-refinement
//= type=implementation
//# The implementation MUST prove that binding under `ByExecution`
//# implies execution of the test in the same witness:
//= design/witness/spec.md#property-w7-positional-binding-map-independence
//= type=implementation
//# The implementation MUST prove that binding under `ByRootSpan` does
//# not depend on the witness's coverage maps:
pub open spec fn binds(
    file_id: u64,
    annotation: &AnnotationSpan,
    mode: ScoringMode,
    classifications: &[Option<LineClass>],
    scopes: &[Scope],
    file_length: u64,
    w: Witness,
) -> bool {
    match w.claim {
        ClaimRule::ByExecution => executed_by(
            file_id, annotation, mode, classifications, scopes, file_length, w,
        ),
        ClaimRule::ByRootSpan { file_id: span_file, start_line, end_line } => {
            let target = annotation_target_spec(annotation, classifications, file_length);
            &&& !(mode is Unscorable)
            &&& file_id == span_file
            &&& target.is_some()
            &&& start_line <= target.unwrap() <= end_line
        },
    }
}

/// Spec §1.6 (Decision 14): `witnesses_for(T) = { w ∈ delivered : binds(T, w) }`,
/// and
///
/// `discharged(T, I) ⟺ witnesses_for(T) ≠ ∅ ∧ ∀w ∈ witnesses_for(T) : executed(I, w)`
///
/// The test binds at least one witness and EVERY witness it binds executed
/// the implementation. Each bound witness individually must see both sides;
/// one bound witness that never reaches I is a vacuous claim and fails the
/// pair (the goal is no vacuous test annotations, not at least one executed
/// test annotation).
//= design/witness/spec.md#property-w4-monotonicity
//= type=implementation
//# The implementation MUST prove that adding a witness never flips a
//# failing pair to passing:
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
    &&& exists|k: int|
        0 <= k < witnesses.len()
        && binds(t_file_id, t_annotation, t_mode, t_classifications, t_scopes, t_file_length,
            #[trigger] witnesses[k])
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
            return Some(&w.files[k].1);
        }
        k = k + 1;
    }
    None
}

/// `executed(X, w)` as executable code: the verified scorer the file's
/// routing selected (`is_annotation_executed` for classified files,
/// `degraded_execution_status` for classifier-less ones) applied to w's map
/// for X's file, proven equivalent to the `executed_by` spec.
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
        result <==> binds(file_id, annotation, mode, classifications, scopes, file_length, *w),
{
    match &w.claim {
        ClaimRule::ByExecution => {
            is_executed_by(file_id, annotation, mode, classifications, scopes, file_length, w)
        },
        ClaimRule::ByRootSpan { file_id: span_file, start_line, end_line } => {
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

/// Property W1: Universal Same-Witness Discharge (Decision 14 form).
///
/// `binds(T, w)` and `executed(I, w)` are evaluated against the SAME loop
/// element `w`, so the per-witness correlation holds by construction, and
/// the iff `ensures` certifies the universal form: no pair is discharged
/// without a common witness, and no pair is discharged while ANY witness
/// bound to its test failed to reach its implementation. A bound witness
/// that did not execute I fails the pair (early `false` return).
//
// Placement: the annotation block is the LAST comment block before the fn
// header so its resolved target is the header (this file has no language
// classifier; comment lines are unclassified in degraded resolution and
// would otherwise become the target — see spec §1.1's placement note).
//= design/witness/spec.md#property-w1-same-witness-discharge
//= type=test
//# The implementation MUST prove that it reports a pair (T, I)
//# discharged if and only if at least one delivered witness binds T
//# and every delivered witness that binds T executed I:
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
                return false;
            }
            any_bound = true;
        }
        k = k + 1;
    }
    any_bound
}

/// Property W2: Test Execution.
//= design/witness/spec.md#property-w2-test-execution
//= type=test
//# The implementation MUST prove that a test annotation is reported
//# executed if and only if some delivered witness binds it:
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

/// Property W3: Global Execution. Deliberately weaker than W1 (no
/// correlation, no `binds`); MUST NOT be used to discharge pairs.
//= design/witness/spec.md#property-w3-global-execution
//= type=test
//# The implementation MUST prove that an implementation annotation is
//# reported ever-executed if and only if some delivered witness
//# executed it:
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
    //# The implementation MUST prove that an implementation annotation is
    //# reported ever-executed if and only if some delivered witness
    //# executed it:
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
//# The engine MUST report every test annotation for which no
//# delivered witness binds it —
//# across ALL configured producers —
//# as a failure, never silently:
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
        result <==> !exists|k: int| 0 <= k < witnesses@.len()
            && binds(t_file_id, t_annotation, t_mode, t_classifications, t_scopes, t_file_length,
                #[trigger] witnesses@[k]),
{
    //= design/witness/spec.md#property-w6-unwitnessed-test-annotations
    //= type=implementation
    //# The engine MUST report every test annotation for which no
    //# delivered witness binds it —
    //# across ALL configured producers —
    //# as a failure, never silently:
    !report_test_executed(t_file_id, t_annotation, t_mode, t_classifications, t_scopes,
        t_file_length, witnesses)
}

// ---------------------------------------------------------------------------
// Proof-only properties (no report function; design/witness/spec.md#engine-properties)
// ---------------------------------------------------------------------------

/// Spec: every witness of `ws` occurs (as an equal value) in `ws2`.
/// Witness sets are `Seq`-valued with no uniqueness assumption
/// (decisions.md, Decision 12): duplicates and multiple witnesses per
/// annotation are allowed, and membership is all the ∃-quantified
/// properties ever inspect.
pub open spec fn witness_subset(ws: Seq<Witness>, ws2: Seq<Witness>) -> bool {
    forall|k: int| 0 <= k < ws.len()
        ==> exists|j: int| 0 <= j < ws2.len() && ws2[j] == #[trigger] ws[k]
}

/// Property W4: Failure Monotonicity (Decision 14 form — this INVERTS the
/// original monotonicity direction; the old "adding never un-discharges" is
/// now false by design, since an added witness that binds T but misses I is
/// exactly the vacuity being caught).
///
/// Statement: `ws ⊆ ws' ⟹ (discharged(ws') ⟹ discharged(ws) ∨ T unwitnessed
/// in ws)`. Equivalently (the form proved here, by case split on whether T
/// is witnessed in ws): the only way adding witnesses turns a non-discharged
/// pair into a discharged one is by witnessing a previously *unwitnessed*
/// test — never by outvoting a bound witness that failed.
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
//= design/witness/spec.md#property-w5-claim-refinement
//= type=test
//# The implementation MUST prove that binding under `ByExecution`
//# implies execution of the test in the same witness:
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
        executed_by(file_id, annotation, mode, classifications, scopes, file_length, w),
{
    // Definitional: with `w.claim == ByExecution`, `binds` unfolds to
    // `executed_by` — nothing to prove beyond the unfold. Mode-uniform:
    // the refinement holds under every scoring mode.
}

/// W5's complement, and the property that makes cross-witness capture
/// unrepresentable for proof witnesses: which annotations a `ByRootSpan`
/// witness binds is a function of its root span alone — the witness's
/// coverage maps have no influence. Enlarging a proof witness's closure
/// can never extend the set of test annotations it witnesses; a map that
/// Hit-covers another test annotation's lines still does not witness it.
/// REGRESSION TRIPWIRE: if `binds`'s `ByRootSpan` arm ever grows a
/// map-consulting conjunct, this proof breaks loudly.
//= design/witness/spec.md#property-w7-positional-binding-map-independence
//= type=test
//# The implementation MUST prove that binding under `ByRootSpan` does
//# not depend on the witness's coverage maps:
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
    fn cov_hit(lines: &[u64]) -> CoverageReport {
        lines.iter().map(|&l| (l, CoverageStatus::Hit)).collect()
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

    /// W1 (universal form, Decision 14): one bound witness that executes I
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
                file_id,
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
        assert!(report_discharged(
            T_FILE,
            &t,
            ScoringMode::Classified,
            &c,
            &[],
            3,
            I_FILE,
            &i_annotation(),
            ScoringMode::Classified,
            &i_classifications(),
            &[],
            2,
            &[root(T_FILE, 2, 4)],
        ));
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
    ///   trench coat — and under Decision 14 the bound witness that never
    ///   reaches I fails the pair.
    ///
    /// The difference between the fat proof witness and the trench coat is
    /// exactly the claim rule the witness carries.
    #[test]
    fn fat_witness_map_containing_t_does_not_capture_t_by_root_span() {
        let t = t_annotation();
        let c = t_classifications();
        // T's target is line 3 in T_FILE; the map Hit-covers it.
        let fat_map = vec![(T_FILE, cov_hit(&[3])), (I_FILE, cov_hit(&[]))];
        let by_root_elsewhere = Witness {
            claim: ClaimRule::ByRootSpan {
                file_id: T_FILE,
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
        assert!(!report_discharged(
            T_FILE,
            &t,
            ScoringMode::Classified,
            &c,
            &[],
            3,
            I_FILE,
            &i_annotation(),
            ScoringMode::Classified,
            &i_classifications(),
            &[],
            2,
            &[by_root_elsewhere],
        ));
        // The SAME map under ByExecution is the trench coat: it binds T by
        // evidence, and having never reached I, fails the pair (Decision 14).
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
        assert!(!report_discharged(
            T_FILE,
            &t,
            ScoringMode::Classified,
            &c,
            &[],
            3,
            I_FILE,
            &i_annotation(),
            ScoringMode::Classified,
            &i_classifications(),
            &[],
            2,
            &[trench_coat],
        ));
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
                file_id: T_FILE,
                start_line: 1,
                end_line: 100,
            },
            files: vec![],
        };
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

    /// W4 (Failure Monotonicity, Decision 14): adding a witness that does
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

    /// Decision 14 ambiguity ruling: at a position rooting N obligations
    /// (N ByRootSpan witnesses binding the same T), ALL N must execute I —
    /// no best-of-N.
    #[test]
    fn decision_14_all_bound_root_span_witnesses_must_execute() {
        let root_with_i = Witness {
            claim: ClaimRule::ByRootSpan {
                file_id: T_FILE,
                start_line: 2,
                end_line: 4,
            },
            files: vec![(I_FILE, cov_hit(&[2]))],
        };
        let root_without_i = Witness {
            claim: ClaimRule::ByRootSpan {
                file_id: T_FILE,
                start_line: 2,
                end_line: 4,
            },
            files: vec![],
        };
        // One bound obligation-witness reaching I: discharged.
        assert!(discharged_verdict(std::slice::from_ref(&root_with_i)));
        // Two obligations own the position; only one reaches I: FAILS.
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
            files: vec![(T_FILE, CoverageReport::new()), (T_FILE, cov_hit(&[3]))],
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
        assert!(verdict(&[w_both()]));
        assert!(!verdict(&[w_t_only(), w_i_only()]));
        assert!(!verdict(&[w_both(), w_t_only()]));
        // A miss on the forward-nearest line is a direct-observation
        // NotExecuted: binds nothing under ByExecution.
        let w_miss = Witness {
            claim: ClaimRule::ByExecution,
            files: vec![(T_FILE, [(3u64, CoverageStatus::Miss)].into_iter().collect())],
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
                file_id: T_FILE,
                start_line: 1,
                end_line: 100,
            },
            files: vec![(I_FILE, cov_hit(&[2]))],
        };
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
}
