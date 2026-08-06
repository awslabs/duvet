// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Line classification for source files.
//!
//! Provides the `LineClassifier` trait and language-specific implementations
//! that map source lines to `LineClass` values using tree-sitter parsing.

pub mod java;
pub mod kotlin;

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

/// The pre-post-pass outcome of a classifier's CST walk: raw per-line
/// property sets plus the code-start record the verified post-pass needs to
/// disambiguate `{code, Comment}` lines. Produced by
/// [`LineClassifier::classify_raw`]; consumed only by the provided
/// [`LineClassifier::classify`], which applies the post-pass.
pub enum RawClassification {
    /// Raw classification: `classifications[i]` is line `i+1`;
    /// `code_start[i]` is whether a code/structural node *starts* on line
    /// `i+1`.
    Classified {
        classifications: Vec<Option<LineClass>>,
        code_start: Vec<bool>,
    },
    /// Same shape and meaning as [`Classification::Unclassifiable`].
    Unclassifiable {
        first: ClassifierIssue,
        rest: Vec<ClassifierIssue>,
    },
}

impl RawClassification {
    /// Build an `Unclassifiable` from a non-empty list of issue lines sharing
    /// a reason — the raw twin of [`Classification::unclassifiable`], same
    /// non-emptiness-by-construction contract.
    pub fn unclassifiable(reason: ClassifierFailure, lines: Vec<u64>) -> Self {
        match Classification::unclassifiable(reason, lines) {
            Classification::Unclassifiable { first, rest } => {
                RawClassification::Unclassifiable { first, rest }
            }
            Classification::Classified(_) => unreachable!(),
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
    /// The language-specific walk, *before* the mutual-exclusivity post-pass.
    /// Implementations return raw property sets plus the code-start record;
    /// they do not (and cannot usefully) call the post-pass themselves.
    fn classify_raw(&self, source: &str) -> RawClassification;

    /// The classification every caller consumes. Provided — not overridable
    /// by convention — so the verified mutual-exclusivity post-pass is
    /// applied at the dispatch boundary and a classifier cannot forget it:
    /// the second classifier instance is exactly when that mistake becomes
    /// possible, so the structure now rules it out.
    ///
    //= design/classifiers/kotlin-spec.md#pre-and-post-pass
    //= type=implementation
    //# After the CST walk, the classifier MUST apply the verified
    //# mutual-exclusivity post-pass (`clean_classifications`)
    //# to every classification it returns.
    fn classify(&self, source: &str) -> Classification {
        match self.classify_raw(source) {
            RawClassification::Classified {
                mut classifications,
                code_start,
            } => {
                // The verified `clean_classifications` guarantees STRUCTURAL
                // PRESERVATION: ScopeOpen/ScopeClose are never stripped (the
                // false-Executed fix, PR #227); only semantic properties are
                // removed from non-code-start Comment/Whitespace/Annotation
                // lines.
                duvet_coverage::classify_postpass::clean_classifications(
                    &mut classifications,
                    &code_start,
                );
                Classification::Classified(classifications)
            }
            RawClassification::Unclassifiable { first, rest } => {
                Classification::Unclassifiable { first, rest }
            }
        }
    }

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
    fn classify_raw(&self, source: &str) -> RawClassification {
        // The universal fallback cannot fail: blank-line detection is total, so
        // it always yields a `Classified` result (never `Unclassifiable`). Its
        // lines are already pure, so the post-pass applied by `classify` is a
        // no-op — running it anyway keeps one uniform path for every
        // classifier.
        let classifications: Vec<Option<LineClass>> = source
            .lines()
            .map(|line| {
                if line.trim().is_empty() {
                    Some(line_class(&[LineProperty::Whitespace]))
                } else {
                    None
                }
            })
            .collect();
        let code_start = vec![false; classifications.len()];
        RawClassification::Classified {
            classifications,
            code_start,
        }
    }
}

/// Returns a classifier for the given file extension, if one exists.
pub fn classifier_for_extension(ext: &str) -> Option<Box<dyn LineClassifier>> {
    match ext {
        "java" => Some(Box::new(java::JavaClassifier)),
        "kt" => Some(Box::new(kotlin::KotlinClassifier)),
        _ => None,
    }
}

/// Returns a classifier for the given file path, if one exists.
pub fn classifier_for_path(path: &Path) -> Option<Box<dyn LineClassifier>> {
    path.extension()
        .and_then(|ext| ext.to_str())
        .and_then(classifier_for_extension)
}

// Assembled at compile time so this source file never contains the literal
// annotation prefixes: duvet's annotation parser also reads string literals,
// and files under duvet/src that are scanned as duvet sources must stay free
// of anything shaped like an annotation.
const ANNOTATION_META: &str = concat!("//", "=");
const ANNOTATION_CONTENT: &str = concat!("//", "#");

/// Language-agnostic pre-pass, shared by every tree-sitter classifier: mark
/// blank lines `Whitespace` and duvet annotation lines (meta or content
/// comments, after leading whitespace) `Annotation`, before the CST walk.
/// `line_props`/`visited` are the classifier's 1-indexed working arrays
/// (index 0 unused).
///
/// Kotlin spec §2 and the Java classifier state the same obligation; both
/// classifiers call this one implementation so the two languages cannot
/// drift. Note the annotation-comment syntax itself is language-specific in
/// general; today both supported languages use `//`-style comments, so one
/// implementation serves. A language with different comment syntax gets its
/// own pre-pass and does not call this one.
pub(crate) fn mark_blank_and_annotation_lines(
    lines: &[&str],
    line_props: &mut [std::collections::BTreeSet<LineProperty>],
    visited: &mut [bool],
) {
    for (i, line) in lines.iter().enumerate() {
        let line_num = i + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            line_props[line_num].insert(LineProperty::Whitespace);
            visited[line_num] = true;
        } else if trimmed.starts_with(ANNOTATION_META) || trimmed.starts_with(ANNOTATION_CONTENT) {
            line_props[line_num].insert(LineProperty::Annotation);
            visited[line_num] = true;
        }
    }
}

/// Collect the 1-based start line of every `ERROR`/`MISSING` node in a
/// tree-sitter parse tree — the located facts for the defeated-commitment
/// diagnostic (coverage-model spec §1.5). Reporting *all* of them, not just
/// the first, lets the user see the whole set in one `query` run. Shared by
/// every tree-sitter classifier.
pub(crate) fn collect_parse_error_lines(node: &tree_sitter::Node, out: &mut Vec<u64>) {
    if node.is_error() || node.is_missing() {
        out.push(node.start_position().row as u64 + 1);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_parse_error_lines(&child, out);
    }
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
        // Rust/etc. have no tree-sitter classifier: these route to the
        // verified degraded path, not a refusal.
        assert!(classifier_for_path(Path::new("src/Other.rs")).is_none());
        // An arbitrary, meaningless extension stands in for "any language duvet
        // has no classifier for" — including ones we will never add. `.xyzzy` (the
        // magic word from Colossal Cave Adventure: "nothing happens") is not a
        // real source language and is exceedingly unlikely to become one, so it
        // exercises the fallback precondition without the risk that a future
        // classifier (Rust, Python, ...) silently reclassifies the case. The
        // end-to-end degraded routing this precondition gates is covered by
        // `unknown_extension_routes_to_verified_degraded_path` in
        // `query/checks/coverage.rs`.
        assert!(classifier_for_path(Path::new("thing.xyzzy")).is_none());
        assert!(classifier_for_path(Path::new("Foo.java")).is_some());
        assert!(classifier_for_path(Path::new("Main.kt")).is_some());
    }
}
