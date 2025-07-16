// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use rustc_hash::FxHashMap;
use crate::Result;
use crate::annotation::{Annotation};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};


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
    pub lines: BTreeMap<u32, u64>,  // line_number -> hit_count
    pub branches: BTreeMap<u32, Vec<bool>>,  // line_number -> [taken, not_taken, ...]
    pub functions: FxHashMap<String, String>,  // function_name -> function_info
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
pub type SourceLineMap = FxHashMap<PathBuf, LineMap>;

/// Trait for parsing coverage reports
pub trait CoverageParser {
    fn parse(&self, file_path: &Path) -> Result<CoverageData>;
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
