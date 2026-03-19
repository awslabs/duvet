// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Core types for the coverage model v2.

use vstd::prelude::*;
use std::collections::{BTreeMap, BTreeSet};

verus! {

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
    Executed,
    NotExecuted,
    Structural,
    Unknown,
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
