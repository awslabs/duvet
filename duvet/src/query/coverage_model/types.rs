// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Core types for the coverage model v2.
//!
//! Implements the type definitions from spec Sections 1.1–1.6.
//! These types are designed to be Verus-compatible when the `verus` feature is enabled.

use std::collections::{BTreeMap, BTreeSet};

/// Line properties from spec Section 1.2.
///
/// Each line in a source file has a set of properties. A line may have multiple
/// properties simultaneously.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum LineProperty {
    /// Executable code (assignment, call, return, throw, etc.)
    Statement,
    /// Structural definition (method sig, class decl, field, import, etc.)
    Declaration,
    /// Opens a new lexical scope
    ScopeOpen,
    /// Closes a lexical scope
    ScopeClose,
    /// Non-annotation comment text
    Comment,
    /// A duvet annotation line
    Annotation,
    /// Blank or whitespace-only
    Whitespace,
    /// goto, label, or non-linear control flow
    NonLinearControl,
}

/// A line's classification is the set of all its properties (spec Section 1.2).
///
/// `None` means the classifier could not determine the line's properties — the line
/// is unknown (spec Section 1.3).
pub type LineClass = BTreeSet<LineProperty>;

/// Annotation span from spec Section 1.6.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnnotationSpan {
    pub start_line: u64,
    pub end_line: u64,
}

/// Target line from spec Section 2.2.
///
/// The target line's properties are `Option<LineClass>` to account for unknown lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetLine {
    pub line_number: u64,
    pub properties: Option<LineClass>,
}

/// Scope from spec Section 1.5.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scope {
    pub open_line: u64,
    pub close_line: u64,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
}

/// Coverage status from spec Section 1.4.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoverageStatus {
    /// Line was executed at least once
    Hit,
    /// Line is executable but was not executed
    Miss,
}

/// Coverage report from spec Section 1.4.
pub type CoverageReport = BTreeMap<u64, CoverageStatus>;

/// Execution status from spec Section 4.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionStatus {
    /// Target line is in the execution set
    Executed,
    /// Target line is reachable but not in the execution set
    NotExecuted,
    /// Target is purely declarative with no executable code in its scope
    Structural,
    /// Cannot determine (unclassified line, non-linear control flow, etc.)
    Unknown,
}

/// Helper to create a `LineClass` from a slice of properties.
pub fn line_class(props: &[LineProperty]) -> LineClass {
    props.iter().copied().collect()
}
