// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    annotation::{Annotation, AnnotationSet, AnnotationType},
    query::{
        classify::{
            classifier_for_path, Classification, ClassifierFailure, ClassifierIssue,
            DefaultClassifier, LineClassifier,
        },
        coverage::{CoverageData, CoverageParser, FileCoverage},
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

/// Coverage model data for a file with a tree-sitter classifier.
/// Contains the classified line properties, scope tree, and coverage data
/// in the format expected by the verified `duvet-coverage` algorithms.
#[derive(Debug, Clone)]
pub struct ClassifiedFileData {
    pub classifications: Vec<Option<LineClass>>,
    pub scopes: Vec<Scope>,
    pub coverage: CoverageReportMap,
    pub file_length: u64,
}

/// Coverage model data for a file with **no** tree-sitter classifier: the
/// minimal universal classification (blank lines → `Whitespace`, everything else
/// → `None`, annotation lines stamped by the caller) plus coverage and length.
/// Fed to the verified degraded path, which needs no scope tree because it does
/// not propagate — it reads coverage directly on the resolved target line.
#[derive(Debug, Clone)]
pub struct DegradedFileData {
    pub classifications: Vec<Option<LineClass>>,
    pub coverage: CoverageReportMap,
    pub file_length: u64,
}

/// Per-file execution data. A file with a tree-sitter classifier uses the
/// verified two-phase model ([`FileExecutionData::Classified`]); a file without
/// one uses the verified degraded path ([`FileExecutionData::Degraded`]). Both
/// paths are verified in `duvet-coverage`; they differ only in fidelity
/// (scope-based governance vs. forward-nearest governance).
#[derive(Debug, Clone)]
pub enum FileExecutionData {
    /// File has a tree-sitter classifier — uses the two-phase coverage model
    /// (target resolution + execution propagation) from duvet-coverage.
    Classified(ClassifiedFileData),
    /// File has no tree-sitter classifier — uses the verified degraded path
    /// ([`duvet_coverage::degraded::degraded_execution_status`]): the same
    /// forward target walk, deciding status by reading coverage directly on the
    /// resolved line. Sound but lower-fidelity (no scope propagation).
    Degraded(DegradedFileData),
    /// **Defeated commitment** (Finding #3, spec §1.5). Either the classifier
    /// reported it could not parse the file, or the verified balance check found
    /// its `ScopeOpen`/`ScopeClose` stream unbalanced. Either way no trustworthy
    /// classification exists, so instead of scoring against `build_scope_tree`'s
    /// collapsed whole-file scope — a well-formed *wrong* tree — we route every
    /// annotation in the file to a located `Unknown`. `issues` is **non-empty**
    /// and names each problem (parse errors: one per `ERROR` node; imbalance: the
    /// witness site). The cause (the file is not this language vs. a classifier
    /// gap) is undecidable here, so we escalate rather than auto-substitute the
    /// coarse model. Non-blocking in `query`.
    DefeatedClassification { issues: Vec<ClassifierIssue> },
}

/// Map from file path to execution data.
pub type ExecutionDataMap = FxHashMap<PathBuf, FileExecutionData>;

/// Build execution data for all source files that have coverage. Each covered
/// file is routed to the verified two-phase model when a tree-sitter classifier
/// exists for its language ([`FileExecutionData::Classified`]), or to the
/// verified degraded path otherwise ([`FileExecutionData::Degraded`]). Both are
/// verified; neither is the old unverified forward-walk.
pub async fn build_execution_data(
    annotations: &AnnotationSet,
    coverage_data: &CoverageData,
    project_sources: &HashSet<SourceFile>,
) -> Result<ExecutionDataMap> {
    let generic = coverage_data.as_generic();

    // Resolve each duvet source file to its absolute path once. The absolute
    // path carries the file's full on-disk directory context, which is exactly
    // what lets a report's file path be matched regardless of where duvet was
    // run from: a report path (e.g. JaCoCo's package-relative `com/example/Foo.java`,
    // or LCOV's `SF:` path) is, when it refers to this file, a suffix of the
    // file's real absolute path. See `coverage_path_matches`.
    let mut duvet_sources: Vec<(&Path, String)> = Vec::new();
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
        duvet_sources.push((duvet_path, absolute.to_string_lossy().into_owned()));
    }

    // Match every duvet source against every report entry by the suffix rule,
    // collecting ALL matches in both directions so a genuine ambiguity is caught
    // rather than silently resolved to whichever entry iterated first (the old
    // `.find()` did the latter — verified-looking, but potentially wrong).
    //
    // A single duvet file matching TWO report entries, or a single report entry
    // matched by TWO duvet files (the multi-module collision: `moduleA/…/com/example/Foo.java`
    // and `moduleB/…/com/example/Foo.java` against a report that only says
    // `com/example/Foo.java`), are both unresolvable from the paths alone. We
    // refuse rather than guess.
    let mut matches_for_file: Vec<(&Path, &FileCoverage)> = Vec::new();
    let mut files_for_coverage: FxHashMap<&str, Vec<&Path>> = FxHashMap::default();

    for (duvet_path, absolute) in &duvet_sources {
        let mut matched: Vec<(&str, &FileCoverage)> = Vec::new();
        for (coverage_path, file_coverage) in &generic.files {
            if coverage_path_matches(absolute, coverage_path) {
                matched.push((coverage_path.as_str(), file_coverage));
                files_for_coverage
                    .entry(coverage_path.as_str())
                    .or_default()
                    .push(duvet_path);
            }
        }

        if matched.len() > 1 {
            // `generic.files` is a hash map, so match order is not stable; sort
            // the reported entries for a deterministic message.
            let mut entries = matched.iter().map(|(path, _)| *path).collect::<Vec<_>>();
            entries.sort_unstable();
            let entries = entries.join(", ");
            return Err(duvet_core::error!(
                "coverage is ambiguous for {}: its path matches multiple report \
                 entries ({}). duvet cannot tell which entry refers to this file.",
                duvet_path.display(),
                entries
            ));
        }

        if let Some((_, file_coverage)) = matched.first() {
            matches_for_file.push((*duvet_path, *file_coverage));
        }
    }

    // The mirror ambiguity: one report entry claimed by more than one source file.
    for (coverage_path, files) in &files_for_coverage {
        if files.len() > 1 {
            // `project_sources` is a `HashSet`, so iteration order is not stable;
            // sort the reported names for a deterministic message.
            let mut names = files
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

    let mut file_futures = Vec::new();
    for (duvet_path, file_coverage) in matches_for_file {
        // A covered file without a language classifier is no longer refused: it
        // is routed to the verified degraded path in `build_file_execution_data`
        // (forward-nearest governance over the minimal universal classification).
        // Both the classified and degraded paths are verified in duvet-coverage.
        let duvet_path = duvet_path.to_path_buf();
        let annotations = annotations.clone();
        let file_coverage = file_coverage.clone();

        let future = async move {
            let data = build_file_execution_data(&duvet_path, &annotations, &file_coverage).await?;
            Result::<_, crate::Error>::Ok((duvet_path.clone(), data))
        };

        file_futures.push(future);
    }

    let results = futures::future::try_join_all(file_futures).await?;
    Ok(results.into_iter().collect())
}

/// Build execution data for a single covered file. Uses the verified two-phase
/// model when a tree-sitter classifier exists for the language, otherwise the
/// verified degraded path over the minimal universal classification.
async fn build_file_execution_data(
    duvet_path: &Path,
    annotations: &AnnotationSet,
    file_coverage: &FileCoverage,
) -> Result<FileExecutionData> {
    let source_file = duvet_core::vfs::read_string(duvet_path).await?;
    let file_content = source_file.to_string();
    let line_count = file_content.lines().count() as u64;
    let coverage = file_coverage.to_coverage_report();

    if let Some(classifier) = classifier_for_path(duvet_path) {
        // Classified path: tree-sitter classification + verified two-phase model.
        let mut classifications = match classifier.classify(&file_content) {
            Classification::Classified(c) => c,
            // Defeated commitment (spec §1.5): the classifier could not parse the
            // file and reported located parse errors. Escalate rather than build
            // and score a scope tree from garbage — same response as an
            // unbalanced stream below, since the cause is equally undecidable.
            Classification::Unclassifiable { first, rest } => {
                let mut issues = Vec::with_capacity(rest.len() + 1);
                issues.push(first);
                issues.extend(rest);
                return Ok(FileExecutionData::DefeatedClassification { issues });
            }
        };

        // Build the scope tree from the *pristine* classifier output, before the
        // annotation override below. A duvet annotation trailing a structural
        // line (e.g. `//= spec.md#x` on a method's closing `}`) would otherwise
        // overwrite that line's ScopeClose with {Annotation}, unbalancing the
        // ScopeOpen/ScopeClose stream. build_scope_tree would then fall back to a
        // single whole-file scope and every annotation in the file would resolve
        // against it. The scope tree depends only on structure, not on which
        // lines carry annotations, so building it first is correct.
        // Discharge `build_scope_tree`'s (and `match_scope_pairs`') sole
        // precondition, `file_length < u64::MAX`, at this Verus/Rust boundary.
        // Verus checks it against the proof but it compiles away for this
        // unverified caller, so state it explicitly. It is physically
        // unfalsifiable — `line_count` is a line tally, and u64::MAX lines is
        // ~exabytes — so `debug_assert!` (checked in tests/CI, compiled out in
        // release) is the honest weight: it asserts the contract input where a
        // logic regression would surface, without a release panic path that can
        // never fire.
        debug_assert!(line_count < u64::MAX);

        // Spec §1.5 Scope Balance Contract: the selected classifier MUST emit a
        // balanced scope stream. We check the classifier's *ordered scope-event
        // stream* — every `{`/`}` in source order, with full multiplicity — not
        // the per-line `LineClass` set. The set holds at most one
        // `ScopeOpen`/`ScopeClose` per line and so drops a brace on a COMPOUND
        // line (`} finally {}`, `}}`), which made the verified balance check
        // (correctly, over its lossy input) report balanced code as unbalanced
        // and falsely escalate valid Java to `DefeatedClassification` (PR #227;
        // git bisect: a612679). The event stream is faithful, so a
        // brace-balanced file now passes the gate. On a genuine imbalance we
        // still refuse to score against `build_scope_tree`'s collapsed whole-file
        // scope (Finding #3) and escalate to a located `Unknown`.
        let scope_events = classifier.scope_events(&file_content);
        if let Some(witness_line) = scope_imbalance_site(&scope_events) {
            return Ok(FileExecutionData::DefeatedClassification {
                issues: vec![ClassifierIssue {
                    reason: ClassifierFailure::UnbalancedScopes,
                    line: witness_line,
                }],
            });
        }

        let scopes = build_scope_tree(&scope_events, line_count);

        apply_annotation_override(&mut classifications, annotations, duvet_path);

        // TRUSTED-BASE ASSUMPTION (unverified glue): the `scopes` stored here
        // satisfy `is_annotation_executed`'s scope-bound preconditions
        // (open_line >= 1, close_line < u64::MAX) *because* they came from
        // `build_scope_tree`, whose contract now states those bounds. Nothing at
        // the type level enforces that this field only ever holds a
        // `build_scope_tree` result — `scopes: Vec<Scope>` is a plain public
        // field. Today this is the sole producer, so the assumption holds by
        // construction. TODO(VerifiedScopeTree): replace `Vec<Scope>` with an
        // opaque newtype whose only constructor is `build_scope_tree` and whose
        // Verus type invariant carries the bounds, so the query-side consumer
        // discharges those `requires` from the type instead of this comment.
        Ok(FileExecutionData::Classified(ClassifiedFileData {
            classifications,
            scopes,
            coverage,
            file_length: line_count,
        }))
    } else {
        // Degraded path: no language classifier for this file. Build the minimal
        // universal classification (blank → Whitespace, else None), stamp
        // annotation lines, and let the verified `degraded_execution_status`
        // resolve the target and read coverage directly. No scope tree is built:
        // the degraded model does not propagate, so none is needed.
        let mut classifications = match DefaultClassifier.classify(&file_content) {
            Classification::Classified(c) => c,
            // DefaultClassifier is total (blank-line detection cannot fail), so
            // it never reports Unclassifiable. This arm is unreachable.
            Classification::Unclassifiable { .. } => {
                unreachable!("DefaultClassifier never returns Unclassifiable")
            }
        };
        apply_annotation_override(&mut classifications, annotations, duvet_path);
        Ok(FileExecutionData::Degraded(DegradedFileData {
            classifications,
            coverage,
            file_length: line_count,
        }))
    }
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
fn coverage_path_matches(absolute_duvet_path: &str, coverage_path: &str) -> bool {
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

/// Check if an annotation is executed according to coverage data.
///
/// Decide the [`ExecutionStatus`] of an annotation given the execution data for
/// its source file. A [`FileExecutionData::Classified`] entry is scored by the
/// verified two-phase model in `duvet_coverage`; a [`FileExecutionData::Degraded`]
/// entry (no tree-sitter classifier) is scored by the verified degraded path
/// [`duvet_coverage::degraded::degraded_execution_status`]. Both paths are
/// verified.
pub fn executed_status_for(
    annotation: &Arc<Annotation>,
    execution_data_map: &ExecutionDataMap,
) -> ExecutionStatus {
    if matches!(annotation.anno, AnnotationType::Spec | AnnotationType::Todo) {
        return ExecutionStatus::NotExecuted;
    }

    let file_path = annotation.source.to_path_buf();

    match execution_data_map.get(&file_path) {
        Some(FileExecutionData::Classified(data)) => {
            let (start_line, end_line) = annotation.line_range();
            let ann_span = AnnotationSpan {
                start_line,
                end_line,
            };

            // Enforce `is_annotation_executed`'s `requires` at this trust
            // boundary. Verus checks those clauses statically against the
            // *proof*, but they compile away in release builds, so nothing stops
            // ill-formed runtime inputs from reaching the verified algorithm and
            // making its guarantees vacuous. The inputs here are not proof-clean:
            // `file_length`/`classifications` come from the tree-sitter
            // classifier while `coverage` keys are JaCoCo `<line nr=...>` values
            // from a separately-produced XML, so source/coverage drift, a
            // trailing newline, or `nr` past EOF can violate the preconditions.
            // When that happens we cannot soundly trust the model's verdict, so
            // fall back to `Unknown` (the conservative status) instead of calling
            // the verified fn on inputs it never reasoned about.
            //
            // requires: annotation.end_line < u64::MAX
            // requires: every coverage key K has (K - 1) < classifications.len()
            let precondition_holds =
                classified_preconditions_hold(end_line, &data.coverage, data.classifications.len());

            if !precondition_holds {
                ExecutionStatus::Unknown {
                    line_number: start_line,
                }
            } else {
                is_annotation_executed(
                    &ann_span,
                    &data.classifications,
                    &data.scopes,
                    &data.coverage,
                    data.file_length,
                )
            }
        }
        Some(FileExecutionData::Degraded(data)) => {
            let (start_line, end_line) = annotation.line_range();
            // Trust boundary: `degraded_execution_status` requires
            // `end_line < u64::MAX` (it computes `end_line + 1` in the target
            // walk). Physically unfalsifiable for a real line number, but guard
            // rather than risk overflow in release, mirroring the classified
            // precondition guard above.
            if end_line == u64::MAX {
                ExecutionStatus::Unknown {
                    line_number: start_line,
                }
            } else {
                let ann_span = AnnotationSpan {
                    start_line,
                    end_line,
                };
                degraded_execution_status(
                    &ann_span,
                    &data.classifications,
                    &data.coverage,
                    data.file_length,
                )
            }
        }
        Some(FileExecutionData::DefeatedClassification { issues }) => {
            // Defeated commitment (Finding #3): no trustworthy classification
            // exists for this file (parse error or unbalanced scopes). Report a
            // located, non-blocking `Unknown` anchored to the first issue rather
            // than a verdict computed against a collapsed/garbage tree. `issues`
            // is non-empty by construction; fall back defensively to line 0.
            let line_number = issues.first().map(|i| i.line).unwrap_or(0);
            ExecutionStatus::Unknown { line_number }
        }
        None => ExecutionStatus::NotExecuted,
    }
}

/// Whether the classified inputs satisfy `is_annotation_executed`'s `requires`
/// clauses. Pure so it can be tested without constructing a full `Annotation`.
/// Mirrors, exactly, the two runtime-checkable preconditions:
///   - `annotation.end_line < u64::MAX`
///   - every coverage key `K` maps to a valid 0-based index: `1 <= K` and
///     `K - 1 < classifications_len`
///
/// (The scope invariants in the third/fourth `requires` are guaranteed by
/// `build_scope_tree`'s postcondition and need no runtime check here.)
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
    // four-strategy `paths_match` handled, plus the boundary and same-name cases
    // the reviewer asked for. The duvet side is always an *absolute* path, since
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

        let data = build_file_execution_data(&path, &annotations, &file_coverage)
            .await
            .expect("degraded path must not error");

        let _ = std::fs::remove_file(&path);

        // 1. Routing: an unknown extension is neither refused nor classified — it
        //    is the verified degraded path.
        let degraded = match data {
            FileExecutionData::Degraded(d) => d,
            other => panic!("unknown extension must route to Degraded, got {other:?}"),
        };

        // 2. The degraded data is the DefaultClassifier projection + the coverage
        //    report (blank -> Whitespace, code -> None).
        assert_eq!(degraded.file_length, 3);
        assert_eq!(degraded.classifications.len(), 3);
        assert!(
            degraded.classifications[0].is_none(),
            "line 1 is code -> None (unclassified)"
        );
        assert!(
            degraded.classifications[1]
                .as_ref()
                .unwrap()
                .contains(&LineProperty::Whitespace),
            "line 2 is blank -> Whitespace"
        );
        assert!(
            degraded.classifications[2].is_none(),
            "line 3 is code -> None (unclassified)"
        );
        assert_eq!(degraded.coverage.get(&1), Some(&CoverageStatus::Hit));
        assert_eq!(degraded.coverage.get(&3), Some(&CoverageStatus::Miss));

        // 3. The verified degraded verdict flows through. An annotation ending on
        //    line 1 resolves forward over the blank line 2 (skippable) to the
        //    nearest coverage-opinionated line 3 (Miss) -> NotExecuted.
        let not_executed = degraded_execution_status(
            &AnnotationSpan {
                start_line: 1,
                end_line: 1,
            },
            &degraded.classifications,
            &degraded.coverage,
            degraded.file_length,
        );
        assert_eq!(
            not_executed,
            ExecutionStatus::NotExecuted,
            "forward walk lands on line 3 (Miss)"
        );
    }
}
