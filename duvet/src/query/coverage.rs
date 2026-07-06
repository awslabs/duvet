// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::{annotation::Annotation, Result};
use rustc_hash::FxHashMap;
use std::{collections::BTreeMap, path::Path, sync::Arc};

/// Coverage data abstraction
#[derive(Clone, Debug)]
pub enum CoverageData {
    Generic(GenericCoverageData),
}

impl CoverageData {
    pub fn as_generic(&self) -> &GenericCoverageData {
        match self {
            CoverageData::Generic(data) => data,
        }
    }
}

/// Generic (aggregate) coverage data
#[derive(Clone, Debug)]
pub struct GenericCoverageData {
    pub files: FxHashMap<String, FileCoverage>,
}

impl GenericCoverageData {
    pub fn new() -> Self {
        Self {
            files: FxHashMap::default(),
        }
    }
}

/// Coverage data for a single file
#[derive(Clone, Debug)]
pub struct FileCoverage {
    pub lines: BTreeMap<u32, u64>,            // line_number -> hit_count
    pub branches: BTreeMap<u32, Vec<bool>>,   // line_number -> [taken, not_taken, ...]
    pub functions: FxHashMap<String, String>, // function_name -> function_info
}

impl FileCoverage {
    /// Convert to the coverage report format used by the verified duvet-coverage crate.
    ///
    /// `lines` and `branches` are disjoint by construction in the JaCoCo parser
    /// (a `<line>` is filed as either a statement or a branch, never both). This
    /// merge does not rely on that: it is Hit-priority, so even if a line number
    /// ever appeared in both maps — or a future parser produced overlaps — a
    /// `Hit` is never overwritten by a `Miss`. Recording a covered line as
    /// missed would be strictly worse than the reverse, so we bias toward Hit.
    pub fn to_coverage_report(&self) -> duvet_coverage::types::CoverageReport {
        use duvet_coverage::types::CoverageStatus;
        let mut report: BTreeMap<u64, CoverageStatus> = BTreeMap::new();

        let mut record = |line_num: u64, status: CoverageStatus| {
            report
                .entry(line_num)
                .and_modify(|existing| {
                    // Hit wins: only upgrade Miss -> Hit, never downgrade.
                    if status == CoverageStatus::Hit {
                        *existing = CoverageStatus::Hit;
                    }
                })
                .or_insert(status);
        };

        for (&line_num, &hit_count) in &self.lines {
            let status = if hit_count > 0 {
                CoverageStatus::Hit
            } else {
                CoverageStatus::Miss
            };
            record(line_num as u64, status);
        }
        for (&line_num, branches) in &self.branches {
            let status = if branches.iter().any(|&taken| taken) {
                CoverageStatus::Hit
            } else {
                CoverageStatus::Miss
            };
            record(line_num as u64, status);
        }
        report
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum LineInfo {
    Executed(ExecutionType),
    NotExecuted(ExecutionType),
    Annotation(Arc<Annotation>), // Use Arc<Annotation> from AnnotationSet
    Whitespace,
    Unknown,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExecutionType {
    // TODO: JaCoCo MethodBoundary information is just the first executed `Line`
    // This means that matching on this information is complicated
    // and it may not be portable to other coverage formats.
    // MethodBoundary,
    Branch,
    Line,
}

pub type LineMap = BTreeMap<u64, LineInfo>;

/// Re-export of the verified [`duvet_coverage::types::ExecutionStatus`] used
/// throughout the query subsystem. The `Structural` variant marks targets that
/// are purely declarative (e.g. interface bodies with no executable code);
/// reporting code currently treats `Structural` the same as `NotExecuted`, but
/// the distinction is preserved at the type level so it can surface in future
/// diagnostics.
pub use duvet_coverage::types::ExecutionStatus;

/// Trait for parsing coverage reports
pub trait CoverageParser {
    async fn parse(&self, file_path: &Path) -> Result<CoverageData>;
}

/// Coverage parsing errors
#[derive(Debug, thiserror::Error)]
pub enum CoverageError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("XML parsing error: {0}")]
    Xml(#[from] quick_xml::Error),

    #[error("Invalid coverage data: {0}")]
    InvalidData(String),

    #[error("Unsupported coverage format")]
    UnsupportedFormat,
}

impl From<CoverageError> for crate::Error {
    fn from(err: CoverageError) -> Self {
        duvet_core::error!("Coverage error: {}", err)
    }
}
