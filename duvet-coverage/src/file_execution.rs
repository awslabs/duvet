// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! A verified, sealed pairing of a file's coverage inputs with their
//! execution set.
//!
//! [`FileExecution`] exists to eliminate a trusted-base axiom that previously
//! lived in duvet's unverified query glue: "the stored `exec_set` is
//! [`execution_set`] of the stored (classifications, scopes, coverage)".
//! `is_annotation_executed_with_exec_set` `requires` that pairing, but a
//! `requires` compiles away for unverified callers, so a caller that stored
//! the set beside its inputs had to maintain the pairing by discipline —
//! and a desync would silently flip Executed/NotExecuted verdicts with every
//! proof still green.
//!
//! Here the pairing is a machine-checked *type invariant* instead:
//! [`FileExecution::new`] is the only constructor, it computes the execution
//! set from the very inputs it stores (checking the runtime-checkable
//! preconditions instead of assuming them), and the fields are private with
//! no mutating methods and no interior mutability. Verus enforces the
//! invariant at every construction and mutation point inside this (verified)
//! crate; Rust's privacy makes construction and mutation outside this module
//! impossible. So every `FileExecution` value any caller can obtain satisfies
//! the invariant, and [`FileExecution::annotation_status`] discharges the
//! verified checker's exec-set `requires` from the invariant rather than
//! from the caller's discipline.

use crate::{
    annotation_execution::is_annotation_executed_with_exec_set,
    execution_propagation::execution_set, types::*,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
// Ghost-only imports; they exist only when Verus is processing the crate,
// referenced from the type invariant and `ensures`.
#[cfg(verus_keep_ghost)]
use crate::annotation_execution::execution_status_of;
#[cfg(verus_keep_ghost)]
use crate::predicates::validly_in_exec_set;
#[cfg(verus_keep_ghost)]
use crate::target_resolution::annotation_target_spec;
use verus_builtin_macros::verus;
// Ghost-only import; see the note in `lib.rs`.
#[cfg(feature = "verify")]
use vstd::prelude::*;

verus! {

/// Spec: every scope's bounds satisfy the verified checkers' scope
/// preconditions (`open_line >= 1`, `close_line < u64::MAX`).
pub open spec fn scopes_in_bounds(scopes: Seq<Scope>) -> bool {
    forall|i: int| 0 <= i < scopes.len() ==>
        (#[trigger] scopes[i]).open_line >= 1 && scopes[i].close_line < u64::MAX
}

/// Spec: every coverage key maps to a valid 0-based classification index.
pub open spec fn pairs_in_bounds(pairs: Seq<(u64, CoverageStatus)>, len: int) -> bool {
    forall|i: int| 0 <= i < pairs.len() ==>
        1 <= (#[trigger] pairs[i]).0 && pairs[i].0 - 1 < len
}

/// A file's per-report execution model: the classified line properties, the
/// scope tree, the coverage map, and the execution set — with the guarantee,
/// carried by the type itself, that the execution set is exactly
/// [`execution_set`] of the other three.
///
/// The report-independent inputs (`classifications`, `scopes`) are held via
/// `Arc` so a caller scoring one file against many coverage reports shares
/// them across every per-report `FileExecution` instead of cloning them.
///
/// Residual trusted base for the pairing guarantee (everything else is
/// machine-checked): Rust's field privacy — no code outside this module can
/// construct a value or mutate a field — and the absence of interior
/// mutability in the field types. There is deliberately no `Clone`: Verus
/// checks even derived constructions against the type invariant, and callers
/// share instances by reference or `Arc` instead.
#[derive(Debug)]
pub struct FileExecution {
    classifications: Arc<Vec<Option<LineClass>>>,
    scopes: Arc<Vec<Scope>>,
    coverage: CoverageReport,
    exec_set: BTreeSet<u64>,
    file_length: u64,
}

impl FileExecution {
    /// Spec view of the stored classifications. `closed`: the field is the
    /// module's private business; callers compose against the name.
    pub closed spec fn spec_classifications(self) -> Seq<Option<LineClass>> {
        self.classifications@
    }

    /// Spec view of the stored scope tree.
    pub closed spec fn spec_scopes(self) -> Seq<Scope> {
        self.scopes@
    }

    /// Spec view of the stored coverage map (as the reference the underlying
    /// predicates take).
    pub closed spec fn spec_coverage(&self) -> &CoverageReport {
        &self.coverage
    }

    /// Spec view of the stored file length.
    pub closed spec fn spec_file_length(self) -> u64 {
        self.file_length
    }

    /// The pairing invariant. Verus enforces this at every construction point
    /// in verified code; privacy prevents construction anywhere else. The
    /// exec-set clauses are quantified over every slice whose view equals the
    /// stored vectors' views — slices with equal views are equal values, so
    /// this is exactly "the pairing holds for the stored data", phrased so
    /// both the constructor (which proves it about `as_slice()` of its
    /// arguments) and `annotation_status` (which instantiates it at
    /// `as_slice()` of the fields) can use it directly.
    #[verifier::type_invariant]
    spec fn wf(self) -> bool {
        &&& forall|line: u64| #[trigger] self.coverage@.contains_key(line)
            ==> (line as int - 1) >= 0 && (line as int - 1) < self.classifications@.len()
        &&& scopes_in_bounds(self.scopes@)
        &&& forall|line: u64| self.coverage@.contains_key(line)
            && self.coverage@[line] == CoverageStatus::Hit
            ==> self.exec_set@.contains(line)
        &&& forall|c: &[Option<LineClass>], s: &[Scope]|
            #![trigger c@, s@]
            c@ == self.classifications@ && s@ == self.scopes@
            ==> forall|line: u64|
                #![trigger self.exec_set@.contains(line)]
                #![trigger validly_in_exec_set(line, c, s, &self.coverage)]
                self.exec_set@.contains(line)
                    <==> validly_in_exec_set(line, c, s, &self.coverage)
    }

    /// Sole constructor. Checks the runtime-checkable preconditions of the
    /// verified model — every coverage key maps to a valid classification
    /// index, and every scope's bounds are in range — and computes the
    /// verified execution set from the inputs it stores. Returns `None` when
    /// a precondition fails: source/coverage drift or a JaCoCo `<line nr=...>`
    /// past EOF violates the coverage bound, and then no verified verdict is
    /// trustworthy for this file+report, so the caller should report the
    /// conservative `Unknown` rather than consult the model.
    ///
    /// `coverage_pairs` is taken as (line, status) pairs rather than a
    /// prebuilt map so the bounds check and the map construction are one
    /// verified loop. Later duplicates overwrite earlier ones; callers that
    /// need a merge policy (duvet's Hit-priority merge) apply it before
    /// handing the pairs over.
    ///
    /// `file_length` participates in target resolution only; no verified
    /// precondition constrains it, so it is stored as given.
    pub fn new(
        classifications: Arc<Vec<Option<LineClass>>>,
        scopes: Arc<Vec<Scope>>,
        coverage_pairs: &[(u64, CoverageStatus)],
        file_length: u64,
    ) -> (result: Option<FileExecution>)
        ensures
            result is Some <==> {
                &&& pairs_in_bounds(coverage_pairs@, classifications@.len() as int)
                &&& scopes_in_bounds(scopes@)
            },
    {
        broadcast use vstd::slice::group_slice_axioms;

        let n = classifications.len();

        // Scope-bounds precondition, checked rather than assumed.
        let mut si: usize = 0;
        while si < scopes.len()
            invariant
                si <= scopes@.len(),
                forall|i: int| 0 <= i < si ==>
                    (#[trigger] scopes@[i]).open_line >= 1 && scopes@[i].close_line < u64::MAX,
            decreases scopes@.len() - si,
        {
            let s = &scopes[si];
            if !(s.open_line >= 1 && s.close_line < u64::MAX) {
                return None;
            }
            si += 1;
        }

        // Coverage-keys precondition, checked while building the map.
        let mut coverage: CoverageReport = BTreeMap::new();
        let mut i: usize = 0;
        while i < coverage_pairs.len()
            invariant
                i <= coverage_pairs@.len(),
                n == classifications@.len(),
                forall|j: int| 0 <= j < i ==>
                    1 <= (#[trigger] coverage_pairs@[j]).0
                    && coverage_pairs@[j].0 - 1 < classifications@.len(),
                forall|line: u64| #[trigger] coverage@.contains_key(line)
                    ==> (line as int - 1) >= 0 && (line as int - 1) < classifications@.len(),
            decreases coverage_pairs@.len() - i,
        {
            let (line, status) = coverage_pairs[i];
            if !(line >= 1 && ((line - 1) as usize) < n) {
                return None;
            }
            coverage.insert(line, status);
            i += 1;
        }

        // The pairing, by construction: compute the set from the stored data.
        let c0: &[Option<LineClass>] = classifications.as_slice();
        let s0: &[Scope] = scopes.as_slice();
        let exec_set = execution_set(c0, s0, &coverage);

        proof {
            // `execution_set`'s ensures give the pairing for (c0, s0);
            // generalize to every view-equal slice pair, since slices with
            // equal views are equal values.
            assert forall|c: &[Option<LineClass>], s: &[Scope]|
                #![trigger c@, s@]
                c@ == classifications@ && s@ == scopes@
                implies forall|line: u64|
                    #![trigger exec_set@.contains(line)]
                    #![trigger validly_in_exec_set(line, c, s, &coverage)]
                    exec_set@.contains(line)
                        <==> validly_in_exec_set(line, c, s, &coverage) by {
                assert(c =~= c0);
                assert(s =~= s0);
            }
        }

        Some(FileExecution {
            classifications,
            scopes,
            coverage,
            exec_set,
            file_length,
        })
    }

    /// The annotation-execution verdict for one annotation span, against this
    /// file+report. Same contract as
    /// [`is_annotation_executed_with_exec_set`] — the spec-twin equivalence
    /// and both Unknown-Safety bullets, stated for every slice pair whose
    /// views equal the stored inputs — but the exec-set `requires` are
    /// discharged from the type invariant instead of from the caller.
    ///
    /// The one remaining `requires` is runtime-checkable per annotation;
    /// unverified callers must check `end_line < u64::MAX` before calling
    /// (it compiles away otherwise).
    pub fn annotation_status(&self, annotation: &AnnotationSpan) -> (status: ExecutionStatus)
        requires
            annotation.end_line < u64::MAX,
        ensures
            forall|c: &[Option<LineClass>], s: &[Scope]|
                #![trigger c@, s@]
                c@ == self.spec_classifications() && s@ == self.spec_scopes() ==> {
                    // Equivalence with the status spec twin (basis for Property 5).
                    &&& status == execution_status_of(
                        annotation_target_spec(annotation, c, self.spec_file_length()),
                        c, s, self.spec_coverage())
                    // Property 6 (Unknown Safety), bullet (a).
                    &&& status == ExecutionStatus::Executed ==> {
                        let line = annotation_target_spec(annotation, c, self.spec_file_length());
                        &&& line.is_some()
                        &&& c@[line.unwrap() as int - 1].is_some()
                    }
                    // Property 6, bullet (b).
                    &&& status == ExecutionStatus::Executed ==> {
                        let line = annotation_target_spec(annotation, c, self.spec_file_length());
                        &&& line.is_some()
                        &&& validly_in_exec_set(line.unwrap(), c, s, self.spec_coverage())
                    }
                },
    {
        broadcast use vstd::slice::group_slice_axioms;

        proof {
            use_type_invariant(self);
        }
        let c0: &[Option<LineClass>] = self.classifications.as_slice();
        let s0: &[Scope] = self.scopes.as_slice();
        proof {
            assert(c0@ == self.classifications@);
            assert(s0@ == self.scopes@);
        }
        let status = is_annotation_executed_with_exec_set(
            annotation,
            c0,
            s0,
            &self.coverage,
            self.file_length,
            &self.exec_set,
        );
        proof {
            // Transfer the callee's ensures from (c0, s0) to every view-equal
            // slice pair: slices with equal views are equal values.
            assert forall|c: &[Option<LineClass>], s: &[Scope]|
                #![trigger c@, s@]
                c@ == self.classifications@ && s@ == self.scopes@
                implies c == c0 && s == s0 by {
                assert(c =~= c0);
                assert(s =~= s0);
            }
        }
        status
    }
}

} // verus!
