// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Line classification for source files.
//!
//! Provides the `LineClassifier` trait and language-specific implementations
//! that map source lines to `LineClass` values using tree-sitter parsing.

pub mod java;

use crate::query::coverage_model::types::LineClass;
use std::path::Path;

/// Classifies source lines into `Option<LineClass>` values (spec Section 1.3).
///
/// Each element in the returned `Vec` corresponds to a source line (1-indexed).
/// `None` means the classifier could not determine the line's properties.
/// `Some(s)` means the line has property set `s` (Decision 9).
pub trait LineClassifier {
    fn classify(&self, source: &str) -> Vec<Option<LineClass>>;
}

/// Returns a classifier for the given file extension, if one exists.
pub fn classifier_for_extension(ext: &str) -> Option<Box<dyn LineClassifier>> {
    match ext {
        "java" => Some(Box::new(java::JavaClassifier)),
        _ => None,
    }
}

/// Returns a classifier for the given file path, if one exists.
pub fn classifier_for_path(path: &Path) -> Option<Box<dyn LineClassifier>> {
    path.extension()
        .and_then(|ext| ext.to_str())
        .and_then(classifier_for_extension)
}
