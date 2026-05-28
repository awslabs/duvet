// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    annotation::{Annotation, AnnotationSet, AnnotationType},
    query::{
        classify::classifier_for_path,
        coverage::{CoverageData, CoverageParser, ExecutionType, FileCoverage, LineInfo, LineMap},
        parsers::JacocoParser,
    },
    source::SourceFile,
    Result,
};
use duvet_coverage::{
    annotation_execution::is_annotation_executed,
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

/// Per-file execution data: either classified (enhanced two-phase model)
/// or unclassified (basic forward-walk fallback).
#[derive(Debug, Clone)]
pub enum FileExecutionData {
    /// File has a tree-sitter classifier — uses the two-phase coverage model
    /// (target resolution + execution propagation) from duvet-coverage.
    Classified(ClassifiedFileData),
    /// No classifier available — uses the basic forward-walk over a LineMap.
    Unclassified(LineMap),
}

/// Map from file path to execution data.
pub type ExecutionDataMap = FxHashMap<PathBuf, FileExecutionData>;

/// Build execution data for all source files that have coverage.
/// Each file gets either classified data (when a tree-sitter classifier exists)
/// or a LineMap (fallback). Never both.
pub async fn build_execution_data(
    annotations: &AnnotationSet,
    coverage_data: &CoverageData,
    project_sources: &HashSet<SourceFile>,
) -> Result<ExecutionDataMap> {
    let mut file_futures = Vec::new();

    for source_file in project_sources {
        let duvet_path = match source_file {
            SourceFile::Text { path, .. } => path,
            SourceFile::Toml(_) => continue,
        };

        let coverage_option = coverage_data
            .as_generic()
            .files
            .iter()
            .find(|(coverage_path, _)| paths_match(duvet_path, coverage_path))
            .map(|(_, file_coverage)| file_coverage);

        if let Some(file_coverage) = coverage_option {
            let duvet_path = duvet_path.clone();
            let annotations = annotations.clone();
            let file_coverage = file_coverage.clone();

            let future = async move {
                let data =
                    build_file_execution_data(&duvet_path, &annotations, &file_coverage).await?;
                Result::<_, crate::Error>::Ok((duvet_path.to_path_buf(), data))
            };

            file_futures.push(future);
        }
    }

    let results = futures::future::try_join_all(file_futures).await?;
    Ok(results.into_iter().collect())
}

/// Build execution data for a single file.
/// Uses the classified path if a tree-sitter classifier exists, otherwise falls back
/// to the basic LineMap. Only one path is built — no redundant work.
async fn build_file_execution_data(
    duvet_path: &Path,
    annotations: &AnnotationSet,
    file_coverage: &FileCoverage,
) -> Result<FileExecutionData> {
    if let Some(classifier) = classifier_for_path(duvet_path) {
        // Enhanced path: tree-sitter classification + verified two-phase model
        let source_file = duvet_core::vfs::read_string(duvet_path).await?;
        let file_content = source_file.to_string();
        let line_count = file_content.lines().count() as u64;

        let mut classifications = classifier.classify(&file_content);

        // Override annotation lines using duvet's authoritative parsed annotation data.
        // The classifier's heuristic prefix detection (e.g., `//=` for Java) serves as
        // a first pass; this override ensures correctness across all comment styles.
        for annotation in annotations.iter() {
            if annotation.source == *duvet_path {
                let (start_line, end_line) = annotation.line_range();
                for line_num in start_line..=end_line {
                    let idx = (line_num - 1) as usize;
                    if idx < classifications.len() {
                        classifications[idx] = Some(duvet_coverage::types::line_class(&[
                            LineProperty::Annotation,
                        ]));
                    }
                }
            }
        }

        let scopes = build_scope_tree(&classifications, line_count);
        let coverage = file_coverage.to_coverage_report();

        Ok(FileExecutionData::Classified(ClassifiedFileData {
            classifications,
            scopes,
            coverage,
            file_length: line_count,
        }))
    } else {
        // Fallback path: basic forward-walk over LineMap
        let line_map = build_line_map_for_file(duvet_path, annotations, file_coverage).await?;
        Ok(FileExecutionData::Unclassified(line_map))
    }
}

async fn build_line_map_for_file(
    duvet_path: &Path,
    annotations: &AnnotationSet,
    file_coverage: &FileCoverage,
) -> Result<LineMap> {
    let source_file = duvet_core::vfs::read_string(duvet_path).await?;
    let file_content = source_file.to_string();
    let lines: Vec<&str> = file_content.lines().collect();
    let line_count = lines.len();

    let mut line_map: LineMap = (1..=line_count as u64)
        .map(|line_num| (line_num, LineInfo::Unknown))
        .collect();

    update_coverage_lines(&mut line_map, file_coverage);
    update_annotation_lines(&mut line_map, annotations, duvet_path);
    update_whitespace_lines(&mut line_map, &lines);

    Ok(line_map)
}

fn update_coverage_lines(line_map: &mut LineMap, file_coverage: &FileCoverage) {
    for (&line_num, &hit_count) in &file_coverage.lines {
        let line_info = if hit_count > 0 {
            LineInfo::Executed(ExecutionType::Line)
        } else {
            LineInfo::NotExecuted(ExecutionType::Line)
        };
        line_map.insert(line_num as u64, line_info);
    }

    for (&line_num, branches) in &file_coverage.branches {
        let any_branch_taken = branches.iter().any(|&taken| taken);
        let line_info = if any_branch_taken {
            LineInfo::Executed(ExecutionType::Branch)
        } else {
            LineInfo::NotExecuted(ExecutionType::Branch)
        };
        line_map.insert(line_num as u64, line_info);
    }
}

fn update_annotation_lines(line_map: &mut LineMap, annotations: &AnnotationSet, duvet_path: &Path) {
    for annotation in annotations.iter() {
        if annotation.source == *duvet_path {
            let (start_line, end_line) = annotation.line_range();
            for line_num in start_line..=end_line {
                line_map.insert(line_num, LineInfo::Annotation(annotation.clone()));
            }
        }
    }
}

fn update_whitespace_lines(line_map: &mut LineMap, lines: &[&str]) {
    for (index, line_content) in lines.iter().enumerate() {
        let line_num = (index + 1) as u64;
        if let Some(LineInfo::Unknown) = line_map.get(&line_num) {
            if line_content.trim().is_empty() {
                line_map.insert(line_num, LineInfo::Whitespace);
            }
        }
    }
}

/// Check if a Duvet source path matches a coverage report path.
/// Handles various path format differences between Duvet and coverage tools.
fn paths_match(duvet_path: &Path, coverage_path: &str) -> bool {
    let duvet_path_str = format!("{}", duvet_path.display());

    // Strategy 1: Direct exact match
    if duvet_path_str == coverage_path {
        return true;
    }

    // Strategy 2: Duvet path ends with coverage path at a path separator boundary
    if duvet_path_str.ends_with(coverage_path) {
        let prefix_len = duvet_path_str.len() - coverage_path.len();
        if prefix_len == 0 || duvet_path_str.as_bytes()[prefix_len - 1] == b'/' {
            return true;
        }
    }

    // Strategy 3: Handle JaCoCo parser where coverage path has extra package prefix
    // e.g. coverage_path = "com/example/src/main/java/com/example/Foo.java"
    // should match duvet_path = "src/main/java/com/example/Foo.java"
    if let Some(src_index) = coverage_path.find("src/") {
        let trimmed_coverage_path = &coverage_path[src_index..];
        if duvet_path_str == trimmed_coverage_path {
            return true;
        }
    }

    // Strategy 4: Coverage path ends with duvet path at a path separator boundary
    if coverage_path.ends_with(&duvet_path_str) {
        let prefix_len = coverage_path.len() - duvet_path_str.len();
        if prefix_len == 0 || coverage_path.as_bytes()[prefix_len - 1] == b'/' {
            return true;
        }
    }

    false
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
/// Decide the [`ExecutionStatus`] of an annotation given the execution data
/// for its source file.
///
/// When classified data (tree-sitter) is available for the file, delegates to
/// the verified two-phase coverage model in `duvet_coverage`. Otherwise falls
/// back to [`executed_status_for_unclassified`], which performs a forward
/// walk over a `LineMap`. The unclassified fallback exists for languages
/// without a tree-sitter classifier; with the current set of supported
/// coverage formats (jacoco-xml only) it is not actually reachable from any
/// integration test, but it remains the contract for future formats.
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

            is_annotation_executed(
                &ann_span,
                &data.classifications,
                &data.scopes,
                &data.coverage,
                data.file_length,
            )
        }
        Some(FileExecutionData::Unclassified(line_map)) => {
            executed_status_for_unclassified(annotation, line_map)
        }
        None => ExecutionStatus::NotExecuted,
    }
}

/// Forward-walk execution detection for files without a tree-sitter
/// classifier. Walks forward from the annotation, skipping whitespace and
/// stacked annotations, until reaching a line that coverage data has an
/// opinion about.
fn executed_status_for_unclassified(
    annotation: &Arc<Annotation>,
    line_map: &LineMap,
) -> ExecutionStatus {
    let (start_line, end_line) = annotation.line_range();

    // Confirm this is the same annotation in the line map
    for line_num in start_line..=end_line {
        if let Some(LineInfo::Annotation(stored_annotation)) = line_map.get(&line_num) {
            if stored_annotation != annotation {
                return ExecutionStatus::Unknown {
                    line_number: line_num,
                };
            }
        } else {
            return ExecutionStatus::Unknown {
                line_number: line_num,
            };
        }
    }

    // Walk forward from end of annotation
    let mut current_line = end_line + 1;

    loop {
        match line_map.get(&current_line) {
            Some(LineInfo::Whitespace) => {
                current_line += 1;
            }
            Some(LineInfo::Annotation(next_annotation)) => {
                // Stacked annotation — execution is transitive
                return executed_status_for_unclassified(next_annotation, line_map);
            }
            Some(LineInfo::Executed(_)) => {
                return ExecutionStatus::Executed;
            }
            Some(LineInfo::NotExecuted(_)) => {
                return ExecutionStatus::NotExecuted;
            }
            Some(LineInfo::Unknown) => {
                return ExecutionStatus::Unknown {
                    line_number: current_line,
                };
            }
            None => {
                return ExecutionStatus::NotExecuted;
            }
        }
    }
}
