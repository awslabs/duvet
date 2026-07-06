// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Core types for the coverage model.

use std::collections::{BTreeMap, BTreeSet};
use vstd::prelude::*;

verus! {

// TRUST BASE: the *declaration order* of these variants is load-bearing for the
// proofs. `LineClass = BTreeSet<LineProperty>`, so BTreeSet ordering depends on
// the derived `Ord`, which Verus cannot reason about; `line_property_discriminant`
// (below, ghost-only) re-specifies that order by hand as 0..=7 and the proofs
// reason against *that*. Nothing machine-checks that the hand-written discriminant
// still equals `derive(Ord)`. Reorder, insert, or remove a variant here and the
// two silently diverge — the proofs would then verify against a stale order while
// the runtime BTreeSet uses the new one. `tests::discriminant_matches_derived_ord`
// is the runtime guard that keeps them honest; update it in lockstep with any
// change to this order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum LineProperty {
    Statement,
    Declaration,
    ScopeOpen,
    ScopeClose,
    Comment,
    Annotation,
    Whitespace,
    NonLinearControl,
}

pub type LineClass = BTreeSet<LineProperty>;

} // verus!

// Verus spec implementations for LineProperty's Ord — only compiled under Verus
#[cfg(verus_keep_ghost)]
verus! {

use core::cmp::Ordering;

/// Spec: discriminant value for LineProperty (matches Rust's derive(Ord) order).
pub open spec fn line_property_discriminant(p: LineProperty) -> int {
    match p {
        LineProperty::Statement => 0,
        LineProperty::Declaration => 1,
        LineProperty::ScopeOpen => 2,
        LineProperty::ScopeClose => 3,
        LineProperty::Comment => 4,
        LineProperty::Annotation => 5,
        LineProperty::Whitespace => 6,
        LineProperty::NonLinearControl => 7,
    }
}

impl vstd::std_specs::cmp::PartialEqSpecImpl for LineProperty {
    open spec fn obeys_eq_spec() -> bool { true }
    open spec fn eq_spec(&self, other: &LineProperty) -> bool {
        line_property_discriminant(*self) == line_property_discriminant(*other)
    }
}

impl vstd::std_specs::cmp::PartialOrdSpecImpl for LineProperty {
    open spec fn obeys_partial_cmp_spec() -> bool { true }
    open spec fn partial_cmp_spec(&self, other: &LineProperty) -> Option<Ordering> {
        let a = line_property_discriminant(*self);
        let b = line_property_discriminant(*other);
        if a < b { Some(Ordering::Less) }
        else if a > b { Some(Ordering::Greater) }
        else { Some(Ordering::Equal) }
    }
}

impl vstd::std_specs::cmp::OrdSpecImpl for LineProperty {
    open spec fn obeys_cmp_spec() -> bool { true }
    open spec fn cmp_spec(&self, other: &LineProperty) -> Ordering {
        let a = line_property_discriminant(*self);
        let b = line_property_discriminant(*other);
        if a < b { Ordering::Less }
        else if a > b { Ordering::Greater }
        else { Ordering::Equal }
    }
}

pub broadcast proof fn lemma_line_property_obeys_cmp_spec()
    ensures
        #[trigger] vstd::laws_cmp::obeys_cmp_spec::<LineProperty>(),
{
    broadcast use vstd::laws_eq::group_laws_eq;
    reveal(vstd::laws_eq::obeys_eq_spec_properties::<LineProperty>);
    reveal(vstd::laws_cmp::obeys_partial_cmp_spec_properties::<LineProperty>);
    reveal(vstd::laws_cmp::obeys_cmp_partial_ord::<LineProperty>);
    reveal(vstd::laws_cmp::obeys_cmp_ord::<LineProperty>);
    reveal(vstd::laws_cmp::obeys_cmp_spec::<LineProperty>);

    // Each sub-property holds because our spec fns are defined via discriminants
    assert(vstd::laws_eq::obeys_eq_spec::<LineProperty>());
    assert(vstd::laws_cmp::obeys_partial_cmp_spec_properties::<LineProperty>());
    assert(vstd::laws_cmp::obeys_cmp_partial_ord::<LineProperty>());
    assert(vstd::laws_cmp::obeys_cmp_ord::<LineProperty>());
}

} // verus! (cfg verus_keep_ghost)

verus! {

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnnotationSpan {
    pub start_line: u64,
    pub end_line: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetLine {
    pub line_number: u64,
    pub properties: Option<LineClass>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scope {
    pub open_line: u64,
    pub close_line: u64,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoverageStatus {
    Hit,
    Miss,
}

pub type CoverageReport = BTreeMap<u64, CoverageStatus>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionStatus {
    /// The annotation's target line is in the execution set.
    Executed,
    /// The annotation's target line is reachable but was not executed.
    NotExecuted,
    /// The annotation's target is purely declarative (e.g., interface
    /// method declaration with no body) and cannot be verified by execution.
    Structural,
    /// Execution status cannot be determined. Either the target line is
    /// unclassified (`None`), or it is classified `NonLinearControl`.
    /// `line_number` identifies which line prevented determination, for
    /// diagnostic reporting.
    Unknown { line_number: u64 },
}

pub fn line_class(props: &[LineProperty]) -> (result: LineClass)
{
    let mut s = BTreeSet::new();
    let mut i: usize = 0;
    while i < props.len()
        decreases props.len() - i,
    {
        s.insert(props[i]);
        i = i + 1;
    }
    s
}

} // verus!

#[cfg(test)]
mod tests {
    use super::*;

    /// Guard for the trust base noted at the `LineProperty` declaration: the
    /// ghost `line_property_discriminant` hand-mirrors `derive(Ord)`, and the
    /// proofs reason against the discriminant. This test independently pins the
    /// expected 0..=7 order and asserts the *derived* `Ord` (the one the runtime
    /// BTreeSet actually uses) agrees for every ordered pair. If someone reorders
    /// the enum without updating `line_property_discriminant`, the discriminant
    /// spec and this list diverge and one of them fails to match `derive(Ord)`.
    #[test]
    fn discriminant_matches_derived_ord() {
        use LineProperty::*;
        // MUST match `line_property_discriminant`'s arm order exactly.
        let order = [
            Statement,        // 0
            Declaration,      // 1
            ScopeOpen,        // 2
            ScopeClose,       // 3
            Comment,          // 4
            Annotation,       // 5
            Whitespace,       // 6
            NonLinearControl, // 7
        ];
        for (i, a) in order.iter().enumerate() {
            for (j, b) in order.iter().enumerate() {
                assert_eq!(
                    a.cmp(b),
                    i.cmp(&j),
                    "derive(Ord) disagrees with discriminant order for {a:?} vs {b:?}: \
                     enum declaration order and line_property_discriminant have drifted"
                );
            }
        }
    }
}
