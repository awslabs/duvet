// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    annotation::{Annotation, AnnotationSet, AnnotationType},
    query::{
        classify::{classifier_for_path, DefaultClassifier, LineClassifier},
        coverage::{CoverageData, CoverageParser, FileCoverage},
        parsers::JacocoParser,
    },
    source::SourceFile,
    Result,
};
use duvet_coverage::{
    annotation_execution::is_annotation_executed,
    degraded::degraded_execution_status,
    scopes::build_scope_tree,
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
        let mut classifications = classifier.classify(&file_content);

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
        let scopes = build_scope_tree(&classifications, line_count);

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
        let mut classifications = DefaultClassifier.classify(&file_content);
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
/// its source file, using the verified two-phase coverage model in
/// `duvet_coverage`. Files without a tree-sitter classifier are refused upstream
/// by [`build_execution_data`], so every entry reaching here is classified.
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
    use crate::query::classify::{java::JavaClassifier, LineClassifier};
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

    /// Regression: the scope tree must be built from the pristine classifier
    /// output, before the annotation override. An annotation trailing a method's
    /// closing brace would otherwise clobber the `ScopeClose`, unbalance the
    /// stream, and collapse the file to a single whole-file scope.
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
        let classifications = JavaClassifier.classify(source);
        let line_count = classifications.len() as u64;

        // Two method bodies + the class body ⇒ three scope opens from the
        // pristine classification.
        let pristine_opens = count_scope_opens(&classifications);
        assert!(
            pristine_opens >= 3,
            "expected the class body plus two method bodies, got {pristine_opens} scope opens"
        );

        // Correct order (what production now does): build the tree from the
        // pristine classification, before the override.
        let scopes = build_scope_tree(&classifications, line_count);
        assert!(
            scopes.len() >= 3,
            "scope tree collapsed even before the override: {} scopes",
            scopes.len()
        );

        // Simulate an annotation spanning lines 6-8, whose last line is the `}`.
        // Stamping it collapses the ScopeClose on line 8.
        let mut overridden = classifications.clone();
        stamp_annotation_range(&mut overridden, (6, 8));

        // Building the tree from the *overridden* classification (the old,
        // buggy order) collapses it — this asserts the hazard the reorder
        // avoids. Production builds `scopes` (above) from pristine data instead.
        let scopes_after_wrong_order = build_scope_tree(&overridden, line_count);
        assert!(
            scopes_after_wrong_order.len() < scopes.len(),
            "sanity: overriding the brace before build_scope_tree should collapse \
             the tree (the bug the reorder avoids); got {} vs {}",
            scopes_after_wrong_order.len(),
            scopes.len()
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
}
