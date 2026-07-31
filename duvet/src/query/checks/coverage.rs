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
    witness::ScoringMode,
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
/// witness-invariant input to [`executed_status`]. Classification depends
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

/// One file's classification flattened to the inputs the verified scoring
/// layer consumes, plus the [`ScoringMode`] routing decision (G3).
///
/// This is THE routing point for the Classified/Degraded/Defeated arms:
/// every consumer — the diagnostic scorer ([`executed_status`]), target
/// resolution ([`resolve_target_line`]), and the verified adapter
/// (`VerifiedVerdicts`) — derives its arm from this view instead of
/// re-matching [`FileClassification`], so the trust-boundary routing
/// cannot drift between them.
#[derive(Clone, Copy, Debug)]
pub struct ScoringView<'a> {
    pub mode: ScoringMode,
    pub classifications: &'a [Option<LineClass>],
    /// Empty for every mode but `Classified` (the degraded path scores
    /// without a scope tree).
    pub scopes: &'a [Scope],
    pub file_length: u64,
    /// `Unscorable` (defeated) only: the first classifier issue's line,
    /// for `Unknown` diagnostics.
    pub defeat_line: Option<u64>,
}

impl ScoringView<'static> {
    /// The view of a file that cannot be scored at all: no
    /// classification exists, or the annotation's context is otherwise
    /// refused at the trust boundary. Binds nothing, executes nothing.
    pub const UNSCORABLE: Self = ScoringView {
        mode: ScoringMode::Unscorable,
        classifications: &[],
        scopes: &[],
        file_length: 0,
        defeat_line: None,
    };
}

impl ScoringView<'_> {
    /// Whether the verified scorers' runtime-checkable preconditions hold
    /// for an annotation ending at `end_line`, scored against `coverage`,
    /// under this view's mode. Mirrors, exactly, the two
    /// runtime-checkable preconditions:
    ///   - `annotation.end_line < u64::MAX` ([`span_in_model`])
    ///   - for classified files, every coverage key maps to a valid
    ///     0-based index ([`Self::coverage_in_bounds`])
    ///
    /// (The scope-bounds invariants in the verified fn's third/fourth
    /// `requires` are guaranteed by `build_scope_tree`'s postcondition
    /// and need no runtime check here.
    ///
    /// Property 2's `scopes_match_classifications` hypothesis is *not* a
    /// `requires` of `is_annotation_executed` and is likewise not checked
    /// here — but not because of `build_scope_tree`: that postcondition
    /// governs the scope *tree*, while propagation reads the per-line
    /// classification *set*, and their silent disagreement was a real bug
    /// (a `} // comment` line lost `ScopeClose` to the mutual-exclusivity
    /// post-pass, letting backward propagation cross the brace). It is
    /// discharged upstream by construction: the verified post-pass
    /// (`classify_postpass::clean_classifications`) proves
    /// `ScopeOpen`/`ScopeClose` are never stripped, and the classifier
    /// property test proves boundary lines carry them in the first
    /// place.)
    pub fn preconditions_hold(&self, end_line: u64, coverage: &CoverageReportMap) -> bool {
        span_in_model(end_line) && self.coverage_in_bounds(coverage)
    }

    /// The coverage-keys half of the preconditions, shared with the
    /// verified adapter's per-witness-map drop (which has no annotation
    /// span in scope): for a classified file, every coverage key `K`
    /// must map to a valid 0-based index (`1 <= K` and
    /// `K - 1 < classifications.len()`) — the verified two-phase
    /// scorer's `requires`. Degraded files carry no such requirement
    /// (direct observation), and an unscorable file's map is inert
    /// (nothing in it is ever scored), so both keep their maps.
    pub fn coverage_in_bounds(&self, coverage: &CoverageReportMap) -> bool {
        match self.mode {
            ScoringMode::Classified => {
                let len = self.classifications.len();
                coverage.keys().all(|&k| k >= 1 && (k as usize - 1) < len)
            }
            ScoringMode::Degraded | ScoringMode::Unscorable => true,
        }
    }
}

/// The span half of the verified scorers' preconditions: they require
/// `end_line < u64::MAX` (an annotation whose range never resolved).
/// Trust boundary: ill-formed spans are refused before the verified
/// fns, never fed to them.
pub fn span_in_model(end_line: u64) -> bool {
    end_line < u64::MAX
}

impl FileClassification {
    /// Flatten this classification to the [`ScoringView`] the scoring
    /// paths consume: `Classified` and `Degraded` expose their verified
    /// inputs; `Defeated` routes to [`ScoringMode::Unscorable`] with the
    /// defeat's diagnostic line.
    pub fn scoring_view(&self) -> ScoringView<'_> {
        match self {
            FileClassification::Classified {
                classifications,
                scopes,
                file_length,
            } => ScoringView {
                mode: ScoringMode::Classified,
                classifications,
                scopes,
                file_length: *file_length,
                defeat_line: None,
            },
            FileClassification::Degraded {
                classifications,
                file_length,
            } => ScoringView {
                mode: ScoringMode::Degraded,
                classifications,
                scopes: &[],
                file_length: *file_length,
                defeat_line: None,
            },
            FileClassification::Defeated { issues } => ScoringView {
                defeat_line: issues.first().map(|i| i.line),
                ..ScoringView::UNSCORABLE
            },
        }
    }
}

/// Classify a set of files once, in parallel — [`classify_file`] per file.
/// Classification carries no coverage: that half is per-witness and is
/// joined back in by [`executed_status`].
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

/// Classify a single source file (coverage-independent). The routing:
/// a tree-sitter classifier → [`FileClassification::Classified`] (verified
/// two-phase model inputs); no classifier → [`FileClassification::Degraded`];
/// parse error or unbalanced scopes → [`FileClassification::Defeated`].
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

        // The scope tree below is built from the *pristine* CST event
        // stream, before `apply_annotation_override`: an annotation
        // trailing a structural line (e.g. `//= spec.md#x` on a closing
        // `}`) would otherwise replace that line's ScopeClose and
        // unbalance the stream, collapsing the tree to a single
        // whole-file scope. Structure does not depend on which lines
        // carry annotations, so pristine-first is correct. The
        // debug_asserts here and below discharge `build_scope_tree`'s
        // preconditions (`file_length < u64::MAX`; ordered, bounded
        // events) at this Verus/Rust boundary — physically unfalsifiable
        // by construction, so debug-weight: a tripwire in tests/CI with
        // no release panic path.
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
    /// Exact project-path lookup for [`Self::absolute_of`].
    by_path: FxHashMap<PathBuf, usize>,
    /// Suffix-rule pre-filter: [`suffix_key`] of the absolute path →
    /// entry indices. Sound because [`coverage_path_matches`] implies
    /// equal suffix keys (see `suffix_key`), so a bucket lookup never
    /// drops a true match — in particular every ambiguity the full
    /// scan would refuse is still seen and refused.
    by_suffix: FxHashMap<String, Vec<usize>>,
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
            entries.push((
                duvet_path.to_path_buf(),
                absolute.to_string_lossy().into_owned(),
            ));
        }
        Ok(Self::index(entries))
    }

    /// Build the lookup maps over the entry list. Every constructor
    /// funnels through here so the maps can never drift from the list.
    fn index(entries: Vec<(PathBuf, String)>) -> Self {
        let mut by_path = FxHashMap::default();
        let mut by_suffix: FxHashMap<String, Vec<usize>> = FxHashMap::default();
        for (i, (path, absolute)) in entries.iter().enumerate() {
            by_path.entry(path.clone()).or_insert(i);
            by_suffix
                .entry(suffix_key(absolute).to_string())
                .or_default()
                .push(i);
        }
        Self {
            entries,
            by_path,
            by_suffix,
        }
    }

    /// The absolute path of a project source, if it is one.
    pub fn absolute_of(&self, path: &Path) -> Option<&str> {
        self.by_path.get(path).map(|&i| self.entries[i].1.as_str())
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
        Self::index(entries)
    }

    /// Every entry whose absolute path suffix-matches `coverage_path`,
    /// in entry (build) order. Exactly the entries a full
    /// [`coverage_path_matches`] scan would keep — the suffix-key
    /// bucket only skips entries the matcher must reject — so callers'
    /// ambiguity refusals (>1 candidate) are preserved verbatim.
    pub fn matching_entries<'a>(
        &'a self,
        coverage_path: &'a str,
    ) -> impl Iterator<Item = &'a (PathBuf, String)> + 'a {
        self.matching_indices(coverage_path)
            .map(|i| &self.entries[i])
    }

    /// Indices of [`Self::matching_entries`], ascending (bucket vectors
    /// are filled in entry order).
    fn matching_indices<'a>(&'a self, coverage_path: &'a str) -> impl Iterator<Item = usize> + 'a {
        self.by_suffix
            .get(suffix_key(coverage_path))
            .into_iter()
            .flatten()
            .copied()
            .filter(move |&i| coverage_path_matches(&self.entries[i].1, coverage_path))
    }

    /// Whether a producer-recorded path refers to any project source
    /// (the `project` predicate for prover-producer closures).
    pub fn matches_any(&self, coverage_path: &str) -> bool {
        self.matching_entries(coverage_path).next().is_some()
    }

    /// Match one witness's per-file maps to project sources by the suffix
    /// rule, refusing both ambiguity directions (one source matching two
    /// entries; one entry claimed by two sources) rather than guessing —
    /// the same refusals `build_execution_data` applies per report.
    /// Returned maps are `Arc` clones of the witness's own — shared, not
    /// copied.
    pub fn match_witness_files(
        &self,
        files: &std::collections::BTreeMap<
            String,
            std::sync::Arc<duvet_coverage::types::CoverageReport>,
        >,
    ) -> Result<FxHashMap<PathBuf, std::sync::Arc<duvet_coverage::types::CoverageReport>>> {
        let mut matched: FxHashMap<PathBuf, std::sync::Arc<duvet_coverage::types::CoverageReport>> =
            FxHashMap::default();
        let mut files_for_coverage: FxHashMap<&str, Vec<&Path>> = FxHashMap::default();
        // Hits per entry, indexed like `entries`. Bucket lookups visit
        // exactly the (entry, coverage_path) pairs the full scan would
        // match (suffix_key invariant), and both iteration orders —
        // entries ascending within a bucket, `files` in BTreeMap order —
        // reproduce the full scan's hit lists verbatim.
        let mut hits_per_entry: Vec<Vec<&str>> = vec![Vec::new(); self.entries.len()];

        for (coverage_path, report) in files {
            for i in self.matching_indices(coverage_path) {
                let (duvet_path, _) = &self.entries[i];
                hits_per_entry[i].push(coverage_path.as_str());
                files_for_coverage
                    .entry(coverage_path.as_str())
                    .or_default()
                    .push(duvet_path);
                matched.insert(duvet_path.clone(), std::sync::Arc::clone(report));
            }
        }

        for (i, mut hits) in hits_per_entry.into_iter().enumerate() {
            if hits.len() > 1 {
                hits.sort_unstable();
                let entries = hits.join(", ");
                return Err(duvet_core::error!(
                    "coverage is ambiguous for {}: its path matches multiple report \
                     entries ({}). duvet cannot tell which entry refers to this file.",
                    self.entries[i].0.display(),
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
/// `executed(X, w)` cell of spec §1.4. Routing follows the annotation's
/// [`FileClassification`] arm, with the classification supplied from the
/// per-file cache and the coverage from the witness.
//= design/witness/spec.md#executed
//# This is exactly the existing verified Phases 1–3
//# (`is_annotation_executed`, or the degraded path),
//# applied to one witness's coverage maps.
//
// `coverage` is `None` when the witness does not touch the annotation's
// file — diagnostic status `NotExecuted`, exactly as a report that does
// not name the file:
//= design/witness/spec.md#executed
//# If w's `files` contains no map for X's file at all,
//# `executed(X, w)` is false.
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
    // The witness covers the file but nothing classified it. The engine
    // classifies every witness-matched file, so a missing entry means a
    // caller bug; conservative `Unknown` rather than a panic in release.
    let Some(view) = classification.map(FileClassification::scoring_view) else {
        return ExecutionStatus::Unknown {
            line_number: start_line,
        };
    };
    // Trust boundary: ill-formed inputs fall back to `Unknown` rather
    // than reaching the verified fns — see
    // `ScoringView::preconditions_hold` for the predicate.
    if !view.preconditions_hold(end_line, coverage) {
        return ExecutionStatus::Unknown {
            line_number: start_line,
        };
    }
    let ann_span = AnnotationSpan {
        start_line,
        end_line,
    };
    match view.mode {
        ScoringMode::Classified => is_annotation_executed(
            &ann_span,
            view.classifications,
            view.scopes,
            coverage,
            view.file_length,
        ),
        ScoringMode::Degraded => {
            degraded_execution_status(&ann_span, view.classifications, coverage, view.file_length)
        }
        // Defeated commitment (spec §1.5): no trustworthy classification.
        ScoringMode::Unscorable => ExecutionStatus::Unknown {
            line_number: view.defeat_line.unwrap_or(0),
        },
    }
}

/// Resolve an annotation's target line via the verified target resolution.
/// This is resolution only — no scoring — and is what the `ByRootSpan`
/// claim rule consumes.
//= design/witness/spec.md#annotations
//# each resolved to the source lines it governs by the coverage
//# model's target resolution
//# ([coverage-model-spec §2](../query/coverage-model-spec.md#annotation-target-resolution),
//# including the degraded path).
//= design/witness/spec.md#claim-rules
//#     ByRootSpan(f, r)   → T's resolved target EXISTS and falls
//#                           within r in file f
//
// `None` when the file's classification is defeated, the annotation's
// range is degenerate, or the walk finds no target: an unresolvable
// target binds no positional witness (the annotation surfaces via W6).
pub fn resolve_target_line(
    annotation: &Arc<Annotation>,
    classification: &FileClassification,
) -> Option<u64> {
    let (start_line, end_line) = annotation.line_range();
    // Trust boundary: `annotation_target` requires `end_line < u64::MAX`.
    if !span_in_model(end_line) {
        return None;
    }
    let view = classification.scoring_view();
    if matches!(view.mode, ScoringMode::Unscorable) {
        return None;
    }
    let ann_span = AnnotationSpan {
        start_line,
        end_line,
    };
    duvet_coverage::target_resolution::annotation_target(
        &ann_span,
        view.classifications,
        view.file_length,
    )
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
/// duvet path, ending at a `/` boundary?** This is deterministic and
/// direction-free, and covers every shape reports produce:
///
///   - The report names the whole path, or a package-relative tail of it
///     (exact match; shorter report path).
///   - The report's path is longer than duvet's (duvet was run from inside
///     the package so its glob returned a short path): absolutizing
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

/// The final path component of `path` under the same separator
/// normalization [`coverage_path_matches`] applies (both `/` and `\`
/// split components).
///
/// Invariant (the bucket pre-filter's license, pinned by
/// `suffix_key_agrees_with_coverage_path_matches`):
/// `coverage_path_matches(abs, cov)` implies
/// `suffix_key(abs) == suffix_key(cov)`. Proof shape: a match makes
/// the normalized `cov` a suffix of the normalized `abs` beginning at
/// a separator boundary, so the text after the last separator is the
/// same string on both sides. Hence bucketing candidate paths by
/// suffix key never hides a true match — including the matches an
/// ambiguity refusal needs to see.
pub(crate) fn suffix_key(path: &str) -> &str {
    path.rsplit(['/', '\\'])
        .next()
        .expect("rsplit yields at least one segment")
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::{
        classify::{java::JavaClassifier, Classification, LineClassifier},
        coverage::FileCoverage,
    };
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
    /// not from the per-line classification set, so an annotation override
    /// that clobbers a `ScopeClose` on that set cannot unbalance the tree or
    /// collapse the file to a single whole-file scope — the hazard is
    /// eliminated by construction (regression pinned by PR #227).
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
        // method bodies) and cannot be collapsed by an annotation override
        // that clobbers a `ScopeClose` on the classification set — that
        // hazard is eliminated by construction (PR #227):
        // `build_scope_tree` does not read the mutated set.
        let events = JavaClassifier.scope_events(source);
        let scopes = build_scope_tree(&events, line_count);
        assert!(
            scopes.len() >= 3,
            "expected class body + two method bodies, got {} scopes",
            scopes.len()
        );

        // Overriding the classification set (what `apply_annotation_override`
        // does) does not feed `build_scope_tree`, so the tree is unchanged —
        // demonstrating the reorder hazard is gone rather than merely avoided.
        let mut overridden = classifications.clone();
        stamp_annotation_range(&mut overridden, (6, 8));
        assert_eq!(
            build_scope_tree(&events, line_count).len(),
            scopes.len(),
            "the event-based tree is independent of classification-set overrides"
        );
    }

    /// The guard predicate as [`executed_status`]'s Classified arm applies
    /// it: a view over `len` classified lines, probed with the shared
    /// [`ScoringView::preconditions_hold`]. Every guard test below routes
    /// through this — the same predicate the verified adapter's bounds-drop
    /// consults — so the two trust-boundary responses cannot drift.
    fn classified_preconditions(end_line: u64, coverage: &CoverageReportMap, len: usize) -> bool {
        let classifications = vec![None; len];
        let view = ScoringView {
            mode: ScoringMode::Classified,
            classifications: &classifications,
            scopes: &[],
            file_length: len as u64,
            defeat_line: None,
        };
        view.preconditions_hold(end_line, coverage)
    }

    #[test]
    fn preconditions_hold_for_in_bounds_coverage() {
        // 5 classified lines; coverage keys 1..=5 all map to valid indices.
        let coverage = coverage_with_keys(&[1, 3, 5]);
        assert!(classified_preconditions(4, &coverage, 5));
    }

    #[test]
    fn coverage_key_past_eof_violates_precondition() {
        // Key 6 -> index 5, out of range for 5 classified lines. This is the
        // JaCoCo-nr-past-EOF / source-coverage-drift case that would otherwise
        // reach the verified fn with an input it never reasoned about.
        let coverage = coverage_with_keys(&[1, 6]);
        assert!(!classified_preconditions(4, &coverage, 5));
    }

    #[test]
    fn zero_coverage_key_violates_precondition() {
        // Line numbers are 1-based; key 0 has no valid 0-based index.
        let coverage = coverage_with_keys(&[0, 1]);
        assert!(!classified_preconditions(4, &coverage, 5));
    }

    #[test]
    fn end_line_at_u64_max_violates_precondition() {
        let coverage = coverage_with_keys(&[1]);
        assert!(!classified_preconditions(u64::MAX, &coverage, 5));
    }

    #[test]
    fn empty_coverage_holds() {
        // No keys -> the forall is vacuously satisfied.
        let coverage = coverage_with_keys(&[]);
        assert!(classified_preconditions(4, &coverage, 5));
    }

    /// The guard behavior of every arm, pinned through the shared
    /// predicate: only classified views impose the coverage-keys bound
    /// (degraded is direct observation; an unscorable file's map is
    /// inert), while the span guard applies to every mode.
    #[test]
    fn coverage_bounds_guard_is_classified_only() {
        let out_of_bounds = coverage_with_keys(&[1, 99]);
        let classifications = vec![None, None];
        let classified = ScoringView {
            mode: ScoringMode::Classified,
            classifications: &classifications,
            scopes: &[],
            file_length: 2,
            defeat_line: None,
        };
        let degraded = ScoringView {
            mode: ScoringMode::Degraded,
            ..classified
        };
        assert!(!classified.coverage_in_bounds(&out_of_bounds));
        assert!(degraded.coverage_in_bounds(&out_of_bounds));
        assert!(ScoringView::UNSCORABLE.coverage_in_bounds(&out_of_bounds));
        // The span guard applies regardless of mode.
        assert!(!degraded.preconditions_hold(u64::MAX, &out_of_bounds));
        assert!(degraded.preconditions_hold(3, &out_of_bounds));
    }

    /// The Classified/Degraded/Defeated routing decision, pinned at the
    /// single accessor every consumer derives it from.
    #[test]
    fn scoring_view_routes_each_arm_to_its_mode() {
        use crate::query::classify::{ClassifierFailure, ClassifierIssue};
        let classified = FileClassification::Classified {
            classifications: vec![None; 3],
            scopes: vec![],
            file_length: 3,
        };
        let degraded = FileClassification::Degraded {
            classifications: vec![None; 3],
            file_length: 3,
        };
        let defeated = FileClassification::Defeated {
            issues: vec![ClassifierIssue {
                reason: ClassifierFailure::UnbalancedScopes,
                line: 7,
            }],
        };
        assert!(matches!(
            classified.scoring_view().mode,
            ScoringMode::Classified
        ));
        assert!(matches!(
            degraded.scoring_view().mode,
            ScoringMode::Degraded
        ));
        let view = defeated.scoring_view();
        assert!(matches!(view.mode, ScoringMode::Unscorable));
        assert_eq!(
            view.defeat_line,
            Some(7),
            "the defeat's diagnostic line rides on the view"
        );
        assert!(
            degraded.scoring_view().scopes.is_empty(),
            "degraded scoring has no scope tree"
        );
    }

    // --- coverage_path_matches ---
    //
    // These exercise the single suffix rule against every real-world path
    // shape (exact, package-relative tail, report-longer), plus the
    // boundary and same-name cases. The duvet side is always an *absolute*
    // path, since the caller absolutizes before matching.

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

    // --- suffix_key bucket pre-filter equivalence ---

    /// Adversarial path shapes for the bucket-invariant cross-product:
    /// same filenames under different roots, filename-only entries,
    /// non-boundary near-misses, backslash separators, mixed
    /// separators, trailing separators, empty string.
    fn adversarial_paths() -> Vec<&'static str> {
        vec![
            "/proj/src/main/java/com/example/Foo.java",
            "/other/src/main/java/com/example/Foo.java",
            "/proj/com/example/Foo.java",
            "com/example/Foo.java",
            "example/Foo.java",
            "Foo.java",
            "myexample/Foo.java",
            "/proj/src/main/java/com/myexample/Foo.java",
            "otherFoo.java",
            "com\\example\\Foo.java",
            "C:\\proj\\src\\com\\example\\Foo.java",
            "com/example\\Foo.java",
            "src/lib.rs",
            "duvet-coverage/src/lib.rs",
            "/abs/duvet-coverage/src/lib.rs",
            "lib.rs",
            "b.rs",
            "src/b.rs",
            "other/src/b.rs",
            "/proj/other/src/b.rs",
            "trailing/",
            "",
        ]
    }

    /// The license for every suffix-key bucket in the codebase
    /// (SourceIndex::matching_entries, the producer's graph-file
    /// buckets): a match implies equal suffix keys, so bucketing by
    /// suffix key never hides a match — nor an ambiguity. Checked as a
    /// full cross-product over the adversarial shapes, both argument
    /// orders.
    #[test]
    fn suffix_key_agrees_with_coverage_path_matches() {
        let paths = adversarial_paths();
        for a in &paths {
            for b in &paths {
                if coverage_path_matches(a, b) {
                    assert_eq!(
                        suffix_key(a),
                        suffix_key(b),
                        "match with unequal suffix keys: ({a:?}, {b:?}) — the \
                         bucket pre-filter would hide this match"
                    );
                }
            }
        }
    }

    /// Bucket-backed SourceIndex lookups are extensionally equal to the
    /// full linear scan they replaced, over the adversarial
    /// cross-product: same candidate sets (so the same ambiguity
    /// refusals), same matches_any, same absolute_of.
    #[test]
    fn source_index_bucket_lookups_match_full_scan() {
        let paths = adversarial_paths();
        let entries: Vec<(PathBuf, String)> = paths
            .iter()
            .enumerate()
            .map(|(i, p)| (PathBuf::from(format!("rel{i}")), p.to_string()))
            .collect();
        let index = SourceIndex::from_entries(entries.clone());
        for probe in &paths {
            let full_scan: Vec<&str> = entries
                .iter()
                .filter(|(_, abs)| coverage_path_matches(abs, probe))
                .map(|(_, abs)| abs.as_str())
                .collect();
            let bucketed: Vec<&str> = index
                .matching_entries(probe)
                .map(|(_, abs)| abs.as_str())
                .collect();
            assert_eq!(
                bucketed, full_scan,
                "candidate set diverged for probe {probe:?}"
            );
            assert_eq!(index.matches_any(probe), !full_scan.is_empty());
        }
        for (path, abs) in &entries {
            assert_eq!(index.absolute_of(path), Some(abs.as_str()));
        }
        assert_eq!(index.absolute_of(Path::new("not-an-entry")), None);
    }

    /// The loop-inversion equivalence evidence for
    /// `match_witness_files`: both refusal directions still fire with
    /// the same messages, and the happy path returns the same map.
    #[test]
    fn match_witness_files_refuses_source_matching_two_report_entries() {
        use duvet_coverage::types::{CoverageReport, CoverageStatus};
        let index = SourceIndex::from_entries(vec![(
            PathBuf::from("src/Foo.java"),
            "/proj/src/Foo.java".to_string(),
        )]);
        let mut files = std::collections::BTreeMap::<String, std::sync::Arc<CoverageReport>>::new();
        files.insert(
            "src/Foo.java".into(),
            std::sync::Arc::new([(1u64, CoverageStatus::Hit)].into_iter().collect()),
        );
        files.insert(
            "proj/src/Foo.java".into(),
            std::sync::Arc::new([(1u64, CoverageStatus::Hit)].into_iter().collect()),
        );
        let err = index.match_witness_files(&files).unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("coverage is ambiguous for"), "{msg}");
        assert!(
            msg.contains("proj/src/Foo.java, src/Foo.java"),
            "hits must be listed sorted: {msg}"
        );
    }

    #[test]
    fn match_witness_files_refuses_report_entry_matching_two_sources() {
        use duvet_coverage::types::{CoverageReport, CoverageStatus};
        let index = SourceIndex::from_entries(vec![
            (
                PathBuf::from("a/Foo.java"),
                "/a/com/example/Foo.java".to_string(),
            ),
            (
                PathBuf::from("b/Foo.java"),
                "/b/com/example/Foo.java".to_string(),
            ),
        ]);
        let mut files = std::collections::BTreeMap::<String, std::sync::Arc<CoverageReport>>::new();
        files.insert(
            "com/example/Foo.java".into(),
            std::sync::Arc::new([(1u64, CoverageStatus::Hit)].into_iter().collect()),
        );
        let err = index.match_witness_files(&files).unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("coverage report entry"), "{msg}");
        assert!(msg.contains("a/Foo.java, b/Foo.java"), "{msg}");
    }

    #[test]
    fn match_witness_files_unambiguous_maps_each_source_to_its_entry() {
        use duvet_coverage::types::{CoverageReport, CoverageStatus};
        let index = SourceIndex::from_entries(vec![
            (
                PathBuf::from("a/Foo.java"),
                "/proj/a/com/x/Foo.java".to_string(),
            ),
            (PathBuf::from("b/Bar.java"), "/proj/b/Bar.java".to_string()),
        ]);
        let mut files = std::collections::BTreeMap::<String, std::sync::Arc<CoverageReport>>::new();
        files.insert(
            "com/x/Foo.java".into(),
            std::sync::Arc::new([(1u64, CoverageStatus::Hit)].into_iter().collect()),
        );
        files.insert(
            "unrelated/Other.java".into(),
            std::sync::Arc::new([(2u64, CoverageStatus::Hit)].into_iter().collect()),
        );
        let matched = index.match_witness_files(&files).unwrap();
        assert_eq!(matched.len(), 1);
        assert_eq!(
            matched[&PathBuf::from("a/Foo.java")],
            files["com/x/Foo.java"]
        );
    }

    /// Perf harness (run explicitly: `cargo test --release -p duvet
    /// bench_source_index -- --ignored --nocapture`): bucket lookups vs
    /// the full linear scan they replaced. The scan closure below IS
    /// the old implementation shape, so this both measures the win and
    /// re-checks agreement on every probe.
    #[test]
    #[ignore = "perf harness, run explicitly with --ignored --nocapture"]
    fn bench_source_index_lookups() {
        let n = 500usize;
        let entries: Vec<(PathBuf, String)> = (0..n)
            .map(|i| {
                (
                    PathBuf::from(format!("mod{i}/src/file{i}.rs")),
                    format!("/proj/mod{i}/src/file{i}.rs"),
                )
            })
            .collect();
        let index = SourceIndex::from_entries(entries.clone());
        // Probe mix: hits (recorded tails), misses (foreign files),
        // same-filename near-misses.
        let probes: Vec<String> = (0..n)
            .flat_map(|i| {
                [
                    format!("src/file{i}.rs"),
                    format!("other{i}/nope.rs"),
                    format!("wrong{i}/src/file{i}.rs"),
                ]
            })
            .collect();
        let full_scan = |probe: &str| -> bool {
            entries
                .iter()
                .any(|(_, abs)| coverage_path_matches(abs, probe))
        };
        for p in &probes {
            assert_eq!(index.matches_any(p), full_scan(p), "probe {p}");
        }
        let iters = 10u32;
        let t0 = std::time::Instant::now();
        for _ in 0..iters {
            for p in &probes {
                std::hint::black_box(full_scan(p));
            }
        }
        let scan = t0.elapsed();
        let t1 = std::time::Instant::now();
        for _ in 0..iters {
            for p in &probes {
                std::hint::black_box(index.matches_any(p));
            }
        }
        let bucketed = t1.elapsed();
        println!(
            "bench_source_index_lookups: entries={} probes={} iters={} \
             full-scan={:?} bucketed={:?}",
            n,
            probes.len(),
            iters,
            scan,
            bucketed
        );
    }

    // --- forward-walk fallback (degraded path) integration ---

    /// End-to-end coverage of the non-Java format path (the previously
    /// disclosed gap: "forward-walk fallback has no integration test, no non-Java
    /// format ships"). Drives the real classification routing [`classify_file`] on a
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
    /// returns `None`, so the file must land on `FileClassification::Degraded`
    /// (not `Classified`, not `Defeated`) — and then the verified
    /// degraded verdict over the `DefaultClassifier` projection. (`executed_status`'s
    /// `Degraded` arm is a thin guard-and-delegate over `degraded_execution_status`,
    /// covered by that function's own unit tests in `duvet-coverage/src/degraded.rs`.)
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

    /// Ground truth for degraded target resolution over the two shapes the
    /// dogfood run exercises in Rust sources (no Rust classifier -> degraded
    /// path), pinned end-to-end through the REAL pipeline: comment parser ->
    /// `classify_file` (annotation-override stamping) -> `resolve_target_line`.
    ///
    /// Shape 1 — stacked annotations (`proofs.rs` mod tests shape): two
    /// back-to-back `//=` blocks above one `#[test]` fn. Both annotations'
    /// lines are stamped `{Annotation}` (skippable), so BOTH resolve past the
    /// stack to the first non-annotation, non-blank line below it. Stacking
    /// works; the walk does NOT stop on a later annotation's comment lines.
    ///
    /// Shape 2 — doc comments between the annotation and the fn header
    /// (`witness.rs` proof-fn shape): `///` lines have no classifier in
    /// degraded mode (-> None = unclassified), are NOT skippable, and become
    /// the resolved target. The annotation therefore resolves to the doc
    /// comment, not the fn header below it — so a prover producer sees an
    /// Unelaborated position and constructs no witness (plain W6, not
    /// not-proof-testable). This is the classified-vs-degraded divergence:
    /// the Java classifier marks comments skippable; the degraded projection
    /// cannot. Placement rule that follows: in degraded files, a test
    /// annotation must be the LAST comment block before the code it targets.
    #[tokio::test]
    async fn degraded_resolution_stacked_annotations_and_doc_comments() {
        use crate::comment;

        // 1-based layout mirroring the real shapes:
        //  1  //= spec.md#a
        //  2  //= type=test
        //  3  //# quote a
        //  4  //= spec.md#b
        //  5  //= type=test
        //  6  //# quote b
        //  7  #[test]                  <- shape-1 target (unclassified)
        //  8  fn t() {}
        //  9  (blank)
        // 10  //= spec.md#c
        // 11  //= type=test
        // 12  //# quote c
        // 13  /// doc comment          <- shape-2 target (unclassified!)
        // 14  /// more doc
        // 15  pub fn real_target() {}
        let content = "\
//= spec.md#a
//= type=test
//# quote a
//= spec.md#b
//= type=test
//# quote b
#[test]
fn t() {}

//= spec.md#c
//= type=test
//# quote c
/// doc comment
/// more doc
pub fn real_target() {}
";
        let mut path = std::env::temp_dir();
        path.push(format!(
            "duvet_degraded_shapes_{}_{}.rs",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&path, content).unwrap();

        // Rust has no classifier: these files take the degraded path. If a
        // Rust classifier ever lands, this test's premise changes — fail
        // loudly here rather than silently testing the wrong path.
        assert!(
            classifier_for_path(&path).is_none(),
            ".rs must have no classifier for the degraded premise to hold"
        );

        // Real comment parser produces the annotation set (line_range is the
        // parser's, not hand-built).
        let source = duvet_core::file::SourceFile::new(path.clone(), content).unwrap();
        let (annotations, errors) = comment::extract(
            &source,
            &comment::Pattern::default(),
            crate::annotation::AnnotationType::Citation,
            None,
        );
        assert!(errors.is_empty(), "parser errors: {errors:?}");
        assert_eq!(annotations.len(), 3, "three annotations parsed");

        let classification = classify_file(&path, &annotations).await.unwrap();
        let _ = std::fs::remove_file(&path);

        let mut targets: Vec<(String, Option<u64>)> = annotations
            .iter()
            .map(|a| (a.target.clone(), resolve_target_line(a, &classification)))
            .collect();
        targets.sort();

        // Shape 1: both stacked annotations resolve THROUGH the stack to the
        // `#[test]` attribute line (7) — never to the sibling annotation's
        // comment lines (4-6).
        assert_eq!(
            targets[0],
            ("spec.md#a".to_string(), Some(7)),
            "first stacked annotation skips the second's lines and lands on line 7"
        );
        assert_eq!(
            targets[1],
            ("spec.md#b".to_string(), Some(7)),
            "second stacked annotation lands on line 7"
        );

        // Shape 2: the doc comment line (13) is unclassified in degraded mode
        // and becomes the target — NOT the fn header (15).
        assert_eq!(
            targets[2],
            ("spec.md#c".to_string(), Some(13)),
            "doc comments are not skippable in degraded mode: the annotation \
             resolves to line 13 (the doc comment), not 15 (the fn header)"
        );
    }
}
