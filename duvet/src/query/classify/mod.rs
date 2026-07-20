// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Line classification for source files.
//!
//! Provides the `LineClassifier` trait and language-specific implementations
//! that map source lines to `LineClass` values using tree-sitter parsing.

pub mod java;

use duvet_coverage::types::{line_class, LineClass, LineProperty};
use std::path::Path;

/// Classifies source lines into `Option<LineClass>` values (spec Section 1.3).
///
/// Each element in the returned `Vec` corresponds to a source line (1-indexed).
/// `None` means the classifier could not determine the line's properties.
/// `Some(s)` means the line has property set `s` (Decision 9).
pub trait LineClassifier {
    fn classify(&self, source: &str) -> Vec<Option<LineClass>>;
}

/// Universal fallback classifier for source files that have no language-specific
/// (tree-sitter) classifier.
///
/// It certifies only what is language-agnostic: a blank line is `Whitespace`,
/// every other line is `None` (unclassified). This is exactly the minimal input
/// the verified degraded coverage path
/// ([`duvet_coverage::degraded::degraded_execution_status`]) is designed for: it
/// resolves an annotation's target to the first non-skippable line and reads
/// coverage directly on it, so the only classification it needs is "blank vs.
/// not." Annotation lines are stamped separately by the caller
/// (`apply_annotation_override`), so they are not this classifier's concern.
pub struct DefaultClassifier;

impl LineClassifier for DefaultClassifier {
    fn classify(&self, source: &str) -> Vec<Option<LineClass>> {
        source
            .lines()
            .map(|line| {
                if line.trim().is_empty() {
                    Some(line_class(&[LineProperty::Whitespace]))
                } else {
                    None
                }
            })
            .collect()
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_classifier_marks_only_blank_lines() {
        // Blank lines (empty or whitespace-only) -> Whitespace; everything else
        // -> None. This is the exact minimal input the degraded path consumes.
        let source = "fn work() {}\n\n    \nlet x = 1;";
        let out = DefaultClassifier.classify(source);
        assert_eq!(out.len(), 4);
        assert!(out[0].is_none(), "code line must be unclassified (None)");
        assert!(
            out[1].as_ref().unwrap().contains(&LineProperty::Whitespace),
            "empty line must be Whitespace"
        );
        assert!(
            out[2].as_ref().unwrap().contains(&LineProperty::Whitespace),
            "whitespace-only line must be Whitespace"
        );
        assert!(out[3].is_none(), "code line must be unclassified (None)");
    }

    #[test]
    fn default_classifier_whitespace_is_pure() {
        // The degraded target walk treats a line as skippable only when it is
        // *pure* Whitespace (len == 1). Guard that the stamp carries nothing else.
        let out = DefaultClassifier.classify("   ");
        let props = out[0].as_ref().unwrap();
        assert_eq!(props.len(), 1);
        assert!(props.contains(&LineProperty::Whitespace));
    }

    #[test]
    fn no_language_classifier_for_non_java() {
        // Rust/Kotlin/etc. have no tree-sitter classifier: these route to the
        // verified degraded path, not a refusal.
        assert!(classifier_for_path(Path::new("src/Other.rs")).is_none());
        assert!(classifier_for_path(Path::new("Main.kt")).is_none());
        assert!(classifier_for_path(Path::new("Foo.java")).is_some());
    }
}
