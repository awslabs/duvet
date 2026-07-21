// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Line classification for source files.
//!
//! Provides the `LineClassifier` trait and language-specific implementations
//! that map source lines to `LineClass` values using tree-sitter parsing.

pub mod java;

use duvet_coverage::types::{line_class, LineClass, LineProperty, ScopeEvent};
use std::path::Path;

/// Why a classifier could not produce a trustworthy classification for a file it
/// was selected for. Each reason is a *fact the classifier observed*, not a
/// decision about what to do next — the dispatcher owns the response policy
/// (spec §1.5 / §8). Extend as new classifiers surface new failure modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassifierFailure {
    /// The parser (e.g. tree-sitter) reported a syntax error/`MISSING` node —
    /// the file may not be this language, or the grammar has a gap.
    ParseError,
    /// The classification produced an unbalanced `ScopeOpen`/`ScopeClose` stream
    /// (detected downstream by the verified `scope_imbalance_site`). Carried
    /// here so both defeated-commitment causes share one representation.
    UnbalancedScopes,
}

/// A single located problem the classifier (or a downstream verified check)
/// found. The location is **required**: a failure you cannot point at is not
/// actionable, so every issue names a line (1-based). If a cause is truly
/// file-global, it is reported against line 1 rather than "nowhere".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClassifierIssue {
    pub reason: ClassifierFailure,
    pub line: u64,
}

/// The outcome of classifying a file: either a line classification, or a
/// **non-empty** set of located issues explaining why no trustworthy
/// classification exists.
///
/// The `Unclassifiable` variant uses a `first` + `rest` shape so that
/// non-emptiness is guaranteed *by construction* for every caller — verified or
/// not, well-intentioned or not. "Unclassifiable with zero reasons" is not a
/// representable state. The classifier reports these facts and stops; it makes
/// no routing decision (spec §1.5 / §8: the dispatcher maps outcome → action).
#[derive(Debug, Clone)]
pub enum Classification {
    /// The classifier produced a per-line classification.
    Classified(Vec<Option<LineClass>>),
    /// The classifier was selected for this file but could not produce a
    /// trustworthy classification. At least one located issue, always.
    Unclassifiable {
        first: ClassifierIssue,
        rest: Vec<ClassifierIssue>,
    },
}

impl Classification {
    /// Build an `Unclassifiable` from a non-empty list of issue lines sharing a
    /// reason. Panics only on an empty input, which callers must never produce —
    /// the whole point of the variant is "at least one problem". Callers that
    /// detect a failure always have a witness (see `ClassifierIssue`).
    pub fn unclassifiable(reason: ClassifierFailure, mut lines: Vec<u64>) -> Self {
        lines.sort_unstable();
        lines.dedup();
        let mut it = lines.into_iter();
        let first_line = it
            .next()
            .expect("Classification::unclassifiable requires at least one issue line");
        Classification::Unclassifiable {
            first: ClassifierIssue {
                reason,
                line: first_line,
            },
            rest: it.map(|line| ClassifierIssue { reason, line }).collect(),
        }
    }
}

/// Classifies source lines into `Option<LineClass>` values (spec Section 1.3),
/// or reports a non-empty set of located issues when it cannot (spec §1.5).
///
/// Each `Some(_)` element corresponds to a source line (1-indexed); `None` means
/// the classifier could not determine that line's properties. The classifier
/// reports *facts* about its outcome — it never decides how the caller should
/// react to a failure.
pub trait LineClassifier {
    fn classify(&self, source: &str) -> Classification;

    /// The ordered scope-delimiter stream for this file, in source order (spec
    /// §1.5). Feeds the verified `scope_imbalance_site` and (in future) the
    /// scope-tree builder. Unlike the per-line `LineClass` set — which can hold
    /// at most one `ScopeOpen`/`ScopeClose` per line and so silently drops a
    /// brace on a COMPOUND line (`} finally {}`, `}}`) — this stream carries
    /// every transition with full multiplicity and order (PR #227 fix).
    ///
    /// Default: no delimiters. The `DefaultClassifier` (degraded, non-language
    /// path) builds no scope tree, so it emits an empty stream; only
    /// language-aware classifiers override this.
    fn scope_events(&self, _source: &str) -> Vec<ScopeEvent> {
        Vec::new()
    }
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
    fn classify(&self, source: &str) -> Classification {
        // The universal fallback cannot fail: blank-line detection is total, so
        // it always yields a `Classified` result (never `Unclassifiable`).
        Classification::Classified(
            source
                .lines()
                .map(|line| {
                    if line.trim().is_empty() {
                        Some(line_class(&[LineProperty::Whitespace]))
                    } else {
                        None
                    }
                })
                .collect(),
        )
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

    fn classified(c: Classification) -> Vec<Option<LineClass>> {
        match c {
            Classification::Classified(v) => v,
            Classification::Unclassifiable { .. } => {
                panic!("expected Classified, got Unclassifiable")
            }
        }
    }

    #[test]
    fn default_classifier_marks_only_blank_lines() {
        // Blank lines (empty or whitespace-only) -> Whitespace; everything else
        // -> None. This is the exact minimal input the degraded path consumes.
        let source = "fn work() {}\n\n    \nlet x = 1;";
        let out = classified(DefaultClassifier.classify(source));
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
        let out = classified(DefaultClassifier.classify("   "));
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
