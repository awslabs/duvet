// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    annotation::{Annotation, AnnotationSet, AnnotationType},
    query::{
        classify::{
            classifier_for_path, Classification, ClassifierFailure, ClassifierIssue,
            DefaultClassifier, LineClassifier,
        },
        coverage::{CoverageData, CoverageParser},
        parsers::JacocoParser,
    },
    source::SourceFile,
    Result,
};
use duvet_coverage::{
    annotation_execution::is_annotation_executed,
    degraded::degraded_execution_status,
    scopes::{build_scope_tree, scope_imbalance_site},
    types::{
        AnnotationSpan, CoverageReport as CoverageReportMap, ExecutionStatus, LineClass,
        LineProperty, Scope,
    },
};
use rustc_hash::FxHashMap;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone, Debug, clap::ValueEnum)]
pub enum CoverageFormat {
    JacocoXml,
    // Future: Lcov, Clover
}

/// Coverage-independent classification of one source file — the expensive,
/// witness-invariant half of [`FileExecutionData`]. Classification depends
/// only on the file's content and the annotation set, never on any coverage
/// report, so it is computed once per file and shared across every witness
/// (the per-file cache above the witness loop: witness count can exceed
/// report count by orders of magnitude for prover producers).
#[derive(Debug, Clone)]
pub enum FileClassification {
    /// Tree-sitter classifier present: verified two-phase model inputs.
    Classified {
        classifications: Vec<Option<LineClass>>,
        scopes: Vec<Scope>,
        file_length: u64,
    },
    /// No classifier: verified degraded-path inputs.
    Degraded {
        classifications: Vec<Option<LineClass>>,
        file_length: u64,
    },
    /// Defeated commitment (spec §1.5): no trustworthy classification.
    Defeated { issues: Vec<ClassifierIssue> },
}

/// Per-file classification cache.
pub type ClassificationMap = FxHashMap<PathBuf, FileClassification>;

/// Classify a set of files once, in parallel. The same routing as
/// [`build_file_execution_data`] (classifier → two-phase inputs; none →
/// degraded inputs; parse error / unbalanced scopes → defeated), minus the
/// coverage half, which is per-witness.
pub async fn classify_files(
    annotations: &AnnotationSet,
    paths: impl IntoIterator<Item = PathBuf>,
) -> Result<ClassificationMap> {
    let futures: Vec<_> = paths
        .into_iter()
        .map(|path| {
            let annotations = annotations.clone();
            async move {
                let data = classify_file(&path, &annotations).await?;
                Result::<_, crate::Error>::Ok((path, data))
            }
        })
        .collect();
    let results = futures::future::try_join_all(futures).await?;
    Ok(results.into_iter().collect())
}

/// Classify a single source file (coverage-independent). See
/// [`build_file_execution_data`] for the routing rationale; the two share
/// the classifier/scope-tree/annotation-override pipeline.
pub async fn classify_file(
    duvet_path: &Path,
    annotations: &AnnotationSet,
) -> Result<FileClassification> {
    let source_file = duvet_core::vfs::read_string(duvet_path).await?;
    let file_content = source_file.to_string();
    let line_count = file_content.lines().count() as u64;

    if let Some(classifier) = classifier_for_path(duvet_path) {
        let mut classifications = match classifier.classify(&file_content) {
            Classification::Classified(c) => c,
            Classification::Unclassifiable { first, rest } => {
                let mut issues = Vec::with_capacity(rest.len() + 1);
                issues.push(first);
                issues.extend(rest);
                return Ok(FileClassification::Defeated { issues });
            }
        };

        // See `build_file_execution_data` for why the scope tree is built
        // from the pristine CST event stream, before the annotation
        // override, and for the precondition discharge notes.
        debug_assert!(line_count < u64::MAX);

        let scope_events = classifier.scope_events(&file_content);
        if let Some(witness_line) = scope_imbalance_site(&scope_events) {
            return Ok(FileClassification::Defeated {
                issues: vec![ClassifierIssue {
                    reason: ClassifierFailure::UnbalancedScopes,
                    line: witness_line,
                }],
            });
        }
        debug_assert!(scope_events.windows(2).all(|w| w[0].line <= w[1].line));
        debug_assert!(scope_events
            .iter()
            .all(|e| e.line >= 1 && e.line < u64::MAX));

        let scopes = build_scope_tree(&scope_events, line_count);

        apply_annotation_override(&mut classifications, annotations, duvet_path);

        Ok(FileClassification::Classified {
            classifications,
            scopes,
            file_length: line_count,
        })
    } else {
        let mut classifications = match DefaultClassifier.classify(&file_content) {
            Classification::Classified(c) => c,
            Classification::Unclassifiable { .. } => {
                unreachable!("DefaultClassifier never returns Unclassifiable")
            }
        };
        apply_annotation_override(&mut classifications, annotations, duvet_path);
        Ok(FileClassification::Degraded {
            classifications,
            file_length: line_count,
        })
    }
}

/// Project sources with their absolute paths, computed once per run.
/// Absolutizing is what lets a producer-recorded path (JaCoCo's
/// package-relative tail, an SST log's cwd-relative path) be matched by the
/// single suffix rule regardless of where duvet ran; see
/// [`coverage_path_matches`].
pub struct SourceIndex {
    entries: Vec<(PathBuf, String)>,
}

impl SourceIndex {
    pub fn build(project_sources: &HashSet<SourceFile>) -> Result<Self> {
        let mut entries = Vec::new();
        for source_file in project_sources {
            let duvet_path = match source_file {
                SourceFile::Text { path, .. } => &**path,
                SourceFile::Toml(_) => continue,
            };
            let absolute = std::path::absolute(duvet_path).map_err(|err| {
                duvet_core::error!(
                    "could not resolve absolute path for {}: {err}",
                    duvet_path.display()
                )
            })?;
            entries.push((duvet_path.to_path_buf(), absolute.to_string_lossy().into_owned()));
        }
        Ok(Self { entries })
    }

    /// The absolute path of a project source, if it is one.
    pub fn absolute_of(&self, path: &Path) -> Option<&str> {
        self.entries
            .iter()
            .find(|(p, _)| p == path)
            .map(|(_, abs)| abs.as_str())
    }

    /// Every project source with its absolute path, in build order. The
    /// witness adapter enumerates these to translate producer-recorded
    /// paths into file identities (and to refuse ambiguous matches).
    pub fn entries(&self) -> &[(PathBuf, String)] {
        &self.entries
    }

    /// Test-only constructor from explicit (project path, absolute path)
    /// pairs, so adapter tests can pin path-identity behavior without
    /// touching the filesystem.
    #[cfg(test)]
    pub fn from_entries(entries: Vec<(PathBuf, String)>) -> Self {
        Self { entries }
    }

    /// Whether a producer-recorded path refers to any project source
    /// (the `project` predicate for prover-producer closures).
    pub fn matches_any(&self, coverage_path: &str) -> bool {
        self.entries
            .iter()
            .any(|(_, abs)| coverage_path_matches(abs, coverage_path))
    }

    /// Match one witness's per-file maps to project sources by the suffix
    /// rule, refusing both ambiguity directions (one source matching two
    /// entries; one entry claimed by two sources) rather than guessing —
    /// the same refusals `build_execution_data` applies per report.
    pub fn match_witness_files<'a>(
        &self,
        files: &'a std::collections::BTreeMap<String, duvet_coverage::types::CoverageReport>,
    ) -> Result<FxHashMap<PathBuf, &'a duvet_coverage::types::CoverageReport>> {
        let mut matched: FxHashMap<PathBuf, &'a duvet_coverage::types::CoverageReport> =
            FxHashMap::default();
        let mut files_for_coverage: FxHashMap<&str, Vec<&Path>> = FxHashMap::default();

        for (duvet_path, absolute) in &self.entries {
            let mut hits: Vec<&str> = Vec::new();
            for (coverage_path, report) in files {
                if coverage_path_matches(absolute, coverage_path) {
                    hits.push(coverage_path.as_str());
                    files_for_coverage
                        .entry(coverage_path.as_str())
                        .or_default()
                        .push(duvet_path);
                    matched.insert(duvet_path.clone(), report);
                }
            }
            if hits.len() > 1 {
                hits.sort_unstable();
                let entries = hits.join(", ");
                return Err(duvet_core::error!(
                    "coverage is ambiguous for {}: its path matches multiple report \
                     entries ({}). duvet cannot tell which entry refers to this file.",
                    duvet_path.display(),
                    entries
                ));
            }
        }

        for (coverage_path, sources) in &files_for_coverage {
            if sources.len() > 1 {
                let mut names = sources
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>();
                names.sort();
                let names = names.join(", ");
                return Err(duvet_core::error!(
                    "coverage report entry '{}' is ambiguous: it matches multiple \
                     source files ({}). duvet cannot tell which file the report \
                     refers to.",
                    coverage_path,
                    names
                ));
            }
        }

        Ok(matched)
    }
}

/// Score one annotation against one witness's coverage for its file — the
/// `executed(X, w)` cell of spec §1.4: the existing verified Phases 1–3
/// applied to one witness's maps. Same routing and trust-boundary guards as
/// [`executed_status_for`], with the classification supplied from the
/// per-file cache and the coverage from the witness.
///
/// `coverage` is `None` when the witness does not touch the annotation's
/// file: `NotExecuted`, exactly as a report that does not name the file.
pub fn executed_status(
    annotation: &Arc<Annotation>,
    classification: Option<&FileClassification>,
    coverage: Option<&CoverageReportMap>,
) -> ExecutionStatus {
    if matches!(annotation.anno, AnnotationType::Spec | AnnotationType::Todo) {
        return ExecutionStatus::NotExecuted;
    }
    let Some(coverage) = coverage else {
        return ExecutionStatus::NotExecuted;
    };
    let (start_line, end_line) = annotation.line_range();
    match classification {
        Some(FileClassification::Classified {
            classifications,
            scopes,
            file_length,
        }) => {
            let ann_span = AnnotationSpan {
                start_line,
                end_line,
            };
            // Trust boundary: see `executed_status_for` / 
            // `classified_preconditions_hold` for why ill-formed inputs
            // fall back to `Unknown` rather than reaching the verified fn.
            if !classified_preconditions_hold(end_line, coverage, classifications.len()) {
                ExecutionStatus::Unknown {
                    line_number: start_line,
                }
            } else {
                is_annotation_executed(&ann_span, classifications, scopes, coverage, *file_length)
            }
        }
        Some(FileClassification::Degraded {
            classifications,
            file_length,
        }) => {
            if end_line == u64::MAX {
                ExecutionStatus::Unknown {
                    line_number: start_line,
                }
            } else {
                let ann_span = AnnotationSpan {
                    start_line,
                    end_line,
                };
                degraded_execution_status(&ann_span, classifications, coverage, *file_length)
            }
        }
        Some(FileClassification::Defeated { issues }) => {
            let line_number = issues.first().map(|i| i.line).unwrap_or(0);
            ExecutionStatus::Unknown { line_number }
        }
        // The witness covers the file but nothing classified it. The engine
        // classifies every witness-matched file, so this arm means a caller
        // bug; conservative `Unknown` rather than a panic in release.
        None => ExecutionStatus::Unknown {
            line_number: start_line,
        },
    }
}

/// Resolve an annotation's target line via the verified target resolution
/// (spec §1.1: "resolved to the source lines it governs by the coverage
/// model's target resolution, including the degraded path"). This is
/// resolution only — no scoring — and is what the `ByRootSpan` claim rule
/// consumes (spec §1.5: "T's resolved target lines ⊆ r in file f").
///
/// `None` when the file's classification is defeated, the annotation's
/// range is degenerate, or the walk finds no target: an unresolvable
/// target binds no positional witness (the annotation surfaces via W6).
pub fn resolve_target_line(
    annotation: &Arc<Annotation>,
    classification: &FileClassification,
) -> Option<u64> {
    let (start_line, end_line) = annotation.line_range();
    let _ = start_line;
    // Trust boundary: `annotation_target` requires `end_line < u64::MAX`.
    if end_line == u64::MAX {
        return None;
    }
    let ann_span = AnnotationSpan {
        start_line,
        end_line,
    };
    let (classifications, file_length) = match classification {
        FileClassification::Classified {
            classifications,
            file_length,
            ..
        } => (classifications, *file_length),
        FileClassification::Degraded {
            classifications,
            file_length,
        } => (classifications, *file_length),
        FileClassification::Defeated { .. } => return None,
    };
    duvet_coverage::target_resolution::annotation_target(&ann_span, classifications, file_length)
        .map(|t| t.line_number)
}

/// Override annotation lines using duvet's authoritative parsed annotation data.
/// The classifier's heuristic prefix detection (e.g., `//=` for Java) serves as a
/// first pass; this override ensures correctness across all comment styles. Only
/// target resolution and execution propagation read the overridden classifications;
/// scope construction intentionally does not.
///
/// This MUST run *after* `build_scope_tree`: an annotation trailing a structural
/// line would otherwise clobber that line's `ScopeOpen`/`ScopeClose`, unbalance
/// the scope stream, and collapse the tree to a single whole-file scope.
///
/// The stamped range (`annotation.line_range()`) is guaranteed to cover only
/// annotation-comment lines, never real code, so stamping `{Annotation}` cannot
/// erase a `Statement`/`ScopeClose` mid-scope. `line_range()` is derived from
/// `original_text`, which the comment parser builds as the `min..max` span over a
/// *contiguous* run of `//=` / `//#` lines: `on_token` flushes the block on any
/// line-number gap (`comment/parser.rs`), and the tokenizer only emits a token for
/// a line whose trimmed start matches the meta/content prefix (`comment/tokenizer.rs`).
/// So the last line of the range is always a comment line. This is the parser-side
/// twin of the classifier-purity guarantee pinned by
/// `annotation_line_is_pure_even_across_multiline_span` (duvet/src/query/classify/java.rs)
/// and relied on by `duvet_coverage`'s `line_is_skippable`. Pinned here by
/// `annotation_line_range_covers_only_comment_lines`.
fn apply_annotation_override(
    classifications: &mut [Option<LineClass>],
    annotations: &AnnotationSet,
    duvet_path: &Path,
) {
    for annotation in annotations.iter() {
        if annotation.source == *duvet_path {
            stamp_annotation_range(classifications, annotation.line_range());
        }
    }
}

/// Stamp `{Annotation}` over an inclusive 1-based `(start, end)` line range.
fn stamp_annotation_range(classifications: &mut [Option<LineClass>], range: (u64, u64)) {
    let (start_line, end_line) = range;
    for line_num in start_line..=end_line {
        let idx = (line_num - 1) as usize;
        if idx < classifications.len() {
            classifications[idx] = Some(duvet_coverage::types::line_class(&[
                LineProperty::Annotation,
            ]));
        }
    }
}

/// Whether `coverage_path` (a file path as named by a coverage report) refers to
/// the duvet source file whose absolute on-disk path is `absolute_duvet_path`.
///
/// The rule is a single test: **is `coverage_path` a suffix of the absolute
/// duvet path, ending at a `/` boundary?** This is deterministic, direction-free,
/// and dominates the old four-strategy `paths_match`:
///
///   - It subsumes exact-match and duvet-is-longer (the report names the whole
///     path, or a package-relative tail of it).
///   - It subsumes coverage-is-longer / nested-`.duvet` (duvet was run from
///     inside the package so its glob returned a short path): absolutizing
///     restores the real package directories, so the report's longer path is a
///     suffix of the real file — anchored to the actual package, not a bare
///     filename.
///   - It relies only on the one invariant every coverage format shares — a
///     file is a real file on disk — rather than on JaCoCo's package quirk, so
///     it generalizes cleanly to LCOV/Clover/etc. (which name files by path).
///
/// The `/` boundary is what stops `Foo.java` from matching `MyFoo.java` and
/// `com/example/Foo.java` from matching `xcom/example/Foo.java`. Absolutizing the
/// duvet side (which carries full package context) also prevents a bare filename
/// from colliding across packages — the residual multi-module same-tail collision
/// is caught as an ambiguity by the caller, never silently resolved.
///
/// Report paths are normalized to `/` separators for the comparison; duvet
/// absolute paths already use the platform separator, which is `/` here.
pub(crate) fn coverage_path_matches(absolute_duvet_path: &str, coverage_path: &str) -> bool {
    let coverage_path = coverage_path.replace('\\', "/");
    let absolute = absolute_duvet_path.replace('\\', "/");

    let Some(prefix_len) = absolute.len().checked_sub(coverage_path.len()) else {
        return false;
    };
    if !absolute.ends_with(&coverage_path) {
        return false;
    }
    // Suffix must begin at a path-separator boundary (or at the very start).
    prefix_len == 0 || absolute.as_bytes()[prefix_len - 1] == b'/'
}

/// Parse coverage data from file.
pub async fn parse_coverage_data(
    coverage_path: &String,
    format: &CoverageFormat,
) -> Result<CoverageData> {
    match format {
        CoverageFormat::JacocoXml => {
            let parser = JacocoParser;
            parser.parse(Path::new(coverage_path)).await
        }
    }
}

/// Whether the classified inputs satisfy `is_annotation_executed`'s `requires`
/// clauses. Pure so it can be tested without constructing a full `Annotation`.
/// Mirrors, exactly, the two runtime-checkable preconditions:
///   - `annotation.end_line < u64::MAX`
///   - every coverage key `K` maps to a valid 0-based index: `1 <= K` and
///     `K - 1 < classifications_len`
///
/// (The scope-bounds invariants in the third/fourth `requires` are guaranteed
/// by `build_scope_tree`'s postcondition and need no runtime check here.
///
/// Property 2's `scopes_match_classifications` hypothesis is *not* a
/// `requires` of `is_annotation_executed` and is likewise not checked here —
/// but not because of `build_scope_tree`: that postcondition governs the
/// scope *tree*, while propagation reads the per-line classification *set*,
/// and their silent disagreement was a real bug (a `} // comment` line lost
/// `ScopeClose` to the mutual-exclusivity post-pass, letting backward
/// propagation cross the brace). It is discharged upstream by construction:
/// the verified post-pass (`classify_postpass::clean_classifications`) proves
/// `ScopeOpen`/`ScopeClose` are never stripped, and the classifier property
/// test proves boundary lines carry them in the first place.)
fn classified_preconditions_hold(
    end_line: u64,
    coverage: &CoverageReportMap,
    classifications_len: usize,
) -> bool {
    end_line < u64::MAX
        && coverage
            .keys()
            .all(|&k| k >= 1 && (k as usize - 1) < classifications_len)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::classify::{java::JavaClassifier, Classification, LineClassifier};
    use crate::query::coverage::FileCoverage;
    use duvet_coverage::types::{CoverageStatus, LineProperty};

    fn coverage_with_keys(keys: &[u64]) -> CoverageReportMap {
        keys.iter().map(|&k| (k, CoverageStatus::Hit)).collect()
    }

    fn count_scope_opens(classifications: &[Option<LineClass>]) -> usize {
        classifications
            .iter()
            .flatten()
            .filter(|c| c.contains(&LineProperty::ScopeOpen))
            .count()
    }

    /// The scope tree is derived from the classifier's CST scope-event stream,
    /// so an annotation override that clobbers a `ScopeClose` on the per-line
    /// classification set can no longer unbalance the tree or collapse the file
    /// to a single whole-file scope. The hazard the old pristine-ordering
    /// guarded is eliminated by construction (PR #227).
    #[test]
    fn scope_tree_survives_annotation_on_closing_brace() {
        // A two-method class. An annotation ends on `bar`'s closing brace
        // (line 8): the annotation spans lines 6-8 and its last line is the `}`.
        let source = "\
public class Two {
    public void foo() {
        doFoo();
    }
    public void bar() {
        //= spec.md#section-1
        //= type=implementation
    }
}";
        let classifications = match JavaClassifier.classify(source) {
            Classification::Classified(c) => c,
            Classification::Unclassifiable { .. } => panic!("fixture must classify cleanly"),
        };
        let line_count = classifications.len() as u64;

        // Two method bodies + the class body ⇒ three scope opens from the
        // pristine classification.
        let pristine_opens = count_scope_opens(&classifications);
        assert!(
            pristine_opens >= 3,
            "expected the class body plus two method bodies, got {pristine_opens} scope opens"
        );

        // The scope tree is built from the CST-derived event stream, not from
        // `classifications`, so it recovers the real scopes (class body + two
        // method bodies) and — unlike the old set-based matcher — cannot be
        // collapsed by an annotation override that clobbers a `ScopeClose` on
        // the classification set. That hazard is eliminated by construction
        // (PR #227): `build_scope_tree` no longer reads the mutated set.
        let events = JavaClassifier.scope_events(source);
        let scopes = build_scope_tree(&events, line_count);
        assert!(
            scopes.len() >= 3,
            "expected class body + two method bodies, got {} scopes",
            scopes.len()
        );

        // Overriding the classification set (what `apply_annotation_override`
        // does) no longer feeds `build_scope_tree`, so the tree is unchanged —
        // demonstrating the reorder hazard is gone rather than merely avoided.
        let mut overridden = classifications.clone();
        stamp_annotation_range(&mut overridden, (6, 8));
        assert_eq!(
            build_scope_tree(&events, line_count).len(),
            scopes.len(),
            "the event-based tree is independent of classification-set overrides"
        );
    }

    #[test]
    fn preconditions_hold_for_in_bounds_coverage() {
        // 5 classified lines; coverage keys 1..=5 all map to valid indices.
        let coverage = coverage_with_keys(&[1, 3, 5]);
        assert!(classified_preconditions_hold(4, &coverage, 5));
    }

    #[test]
    fn coverage_key_past_eof_violates_precondition() {
        // Key 6 -> index 5, out of range for 5 classified lines. This is the
        // JaCoCo-nr-past-EOF / source-coverage-drift case that would otherwise
        // reach the verified fn with an input it never reasoned about.
        let coverage = coverage_with_keys(&[1, 6]);
        assert!(!classified_preconditions_hold(4, &coverage, 5));
    }

    #[test]
    fn zero_coverage_key_violates_precondition() {
        // Line numbers are 1-based; key 0 has no valid 0-based index.
        let coverage = coverage_with_keys(&[0, 1]);
        assert!(!classified_preconditions_hold(4, &coverage, 5));
    }

    #[test]
    fn end_line_at_u64_max_violates_precondition() {
        let coverage = coverage_with_keys(&[1]);
        assert!(!classified_preconditions_hold(u64::MAX, &coverage, 5));
    }

    #[test]
    fn empty_coverage_holds() {
        // No keys -> the forall is vacuously satisfied.
        let coverage = coverage_with_keys(&[]);
        assert!(classified_preconditions_hold(4, &coverage, 5));
    }

    // --- coverage_path_matches ---
    //
    // These exercise the single suffix rule against every shape the old
    // four-strategy `paths_match` handled, plus the boundary and same-name
    // cases. The duvet side is always an *absolute* path, since
    // the caller absolutizes before matching.

    #[test]
    fn exact_full_path_matches() {
        // Report names the whole path.
        assert!(coverage_path_matches(
            "/proj/src/main/java/com/example/Foo.java",
            "/proj/src/main/java/com/example/Foo.java"
        ));
    }

    #[test]
    fn package_relative_tail_matches() {
        // JaCoCo names the package-relative tail; it is a suffix of the real file.
        assert!(coverage_path_matches(
            "/proj/src/main/java/com/example/Foo.java",
            "com/example/Foo.java"
        ));
    }

    #[test]
    fn nested_duvet_coverage_is_longer_matches() {
        // duvet was run from inside the package (glob returned `Foo.java`), so its
        // real absolute path still ends with the report's longer package path.
        assert!(coverage_path_matches(
            "/proj/com/example/Foo.java",
            "com/example/Foo.java"
        ));
    }

    #[test]
    fn suffix_not_at_separator_boundary_is_rejected() {
        // `example/Foo.java` is a string-suffix of `...myexample/Foo.java` but not
        // at a `/` boundary — must NOT match.
        assert!(!coverage_path_matches(
            "/proj/src/main/java/com/myexample/Foo.java",
            "example/Foo.java"
        ));
    }

    #[test]
    fn filename_suffix_across_packages_is_rejected() {
        // A bare filename that is NOT the package-qualified tail must not match a
        // different package's file. `Foo.java` at a boundary DOES match (it's a
        // valid tail), but `otherFoo.java` does not.
        assert!(!coverage_path_matches(
            "/proj/src/main/java/com/example/Foo.java",
            "otherFoo.java"
        ));
    }

    #[test]
    fn different_package_same_filename_does_not_match() {
        // `org/other/Foo.java` is not a suffix of a file under `com/example/`.
        assert!(!coverage_path_matches(
            "/proj/src/main/java/com/example/Foo.java",
            "org/other/Foo.java"
        ));
    }

    #[test]
    fn longer_coverage_than_absolute_does_not_match() {
        // Report path longer than the whole absolute path cannot be a suffix.
        assert!(!coverage_path_matches(
            "com/example/Foo.java",
            "/proj/src/main/java/com/example/Foo.java"
        ));
    }

    // --- forward-walk fallback (degraded path) integration ---

    /// End-to-end coverage of the non-Java format path (the previously
    /// disclosed gap: "forward-walk fallback has no integration test, no non-Java
    /// format ships"). Drives the real dispatcher `build_file_execution_data` on a
    /// file whose extension has no tree-sitter classifier, then feeds the result
    /// to the verified `degraded_execution_status`.
    ///
    /// The extension `.xyzzy` is deliberately meaningless — the magic word from
    /// Colossal Cave Adventure, "nothing happens." It stands in for any language
    /// duvet has no classifier for, chosen over a real language (Rust, Python,
    /// Haskell, ...) precisely because none of those is safe: any of them could
    /// gain a classifier later and silently convert this from the degraded path
    /// to the classified path, rotting the test. `.xyzzy` will not.
    ///
    /// This exercises the routing decision unique to the fallback — `classifier_for_path`
    /// returns `None`, so the file must land on `FileExecutionData::Degraded`
    /// (not `Classified`, not `DefeatedClassification`) — and then the verified
    /// degraded verdict over the `DefaultClassifier` projection. (`executed_status_for`'s
    /// `Degraded` arm is a thin guard-and-delegate over `degraded_execution_status`,
    /// covered by that function's own unit tests.)
    #[tokio::test]
    async fn unknown_extension_routes_to_verified_degraded_path() {
        use duvet_coverage::types::{AnnotationSpan, CoverageStatus, ExecutionStatus};
        use std::io::Write;

        // File layout (1-based):
        //   1: code   (Hit)
        //   2: (blank) -> Whitespace, skippable by the forward walk
        //   3: code   (Miss)
        let content = "let a = compute();\n\nlet b = other();\n";

        // Write a real temp file: the default VFS reads from disk, and the
        // extension is what drives the routing under test.
        let mut path = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        path.push(format!(
            "duvet_fallback_{}_{}.xyzzy",
            std::process::id(),
            nanos
        ));
        std::fs::File::create(&path)
            .unwrap()
            .write_all(content.as_bytes())
            .unwrap();

        // Precondition of the fallback: no language classifier for this extension.
        assert!(
            classifier_for_path(&path).is_none(),
            ".xyzzy must have no classifier — that is what routes it to the degraded path"
        );

        // Annotations are irrelevant to the routing decision; an empty set means
        // no lines are stamped, isolating the DefaultClassifier projection.
        let annotations: AnnotationSet = Arc::new(std::collections::BTreeSet::new());

        // Coverage: line 1 hit (count 1), line 3 not hit (count 0) -> Hit / Miss.
        let mut lines = std::collections::BTreeMap::new();
        lines.insert(1u32, 1u64);
        lines.insert(3u32, 0u64);
        let file_coverage = FileCoverage {
            lines,
            branches: std::collections::BTreeMap::new(),
        };

        let data = classify_file(&path, &annotations)
            .await
            .expect("degraded path must not error");

        // The witness half: coverage stays separate from classification (the
        // per-file cache is witness-invariant); convert as a producer would.
        let coverage = file_coverage.to_coverage_report();

        let _ = std::fs::remove_file(&path);

        // 1. Routing: an unknown extension is neither refused nor classified — it
        //    is the verified degraded path.
        let degraded = match data {
            FileClassification::Degraded {
                classifications,
                file_length,
            } => (classifications, file_length),
            other => panic!("unknown extension must route to Degraded, got {other:?}"),
        };
        let (classifications, file_length) = degraded;

        // 2. The degraded data is the DefaultClassifier projection (blank ->
        //    Whitespace, code -> None); coverage lives with the witness.
        assert_eq!(file_length, 3);
        assert_eq!(classifications.len(), 3);
        assert!(
            classifications[0].is_none(),
            "line 1 is code -> None (unclassified)"
        );
        assert!(
            classifications[1]
                .as_ref()
                .unwrap()
                .contains(&LineProperty::Whitespace),
            "line 2 is blank -> Whitespace"
        );
        assert!(
            classifications[2].is_none(),
            "line 3 is code -> None (unclassified)"
        );
        assert_eq!(coverage.get(&1), Some(&CoverageStatus::Hit));
        assert_eq!(coverage.get(&3), Some(&CoverageStatus::Miss));

        // 3. The verified degraded verdict flows through. An annotation ending on
        //    line 1 resolves forward over the blank line 2 (skippable) to the
        //    nearest coverage-opinionated line 3 (Miss) -> NotExecuted.
        let not_executed = degraded_execution_status(
            &AnnotationSpan {
                start_line: 1,
                end_line: 1,
            },
            &classifications,
            &coverage,
            file_length,
        );
        assert_eq!(
            not_executed,
            ExecutionStatus::NotExecuted,
            "forward walk lands on line 3 (Miss)"
        );
    }
}
