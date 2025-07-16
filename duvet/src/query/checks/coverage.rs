// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    annotation::{Annotation, AnnotationType, AnnotationSet},
    source::SourceFile,
    query::{
        coverage::{CoverageData, CoverageParser, LineMap, SourceLineMap, FileCoverage, LineInfo, ExecutionType},
        parsers::JacocoParser,
    },
    Result,
};
use rustc_hash::FxHashMap;
use std::{
    collections::{HashSet},
    path::Path,
    sync::Arc,
};

#[derive(Clone, Debug, clap::ValueEnum)]
pub enum CoverageFormat {
    JacocoXml,
    // Future: Lcov, Clover
}

pub fn build_source_line_map(
    annotations: &AnnotationSet,
    coverage_data: &CoverageData,
    project_sources: &HashSet<SourceFile>
) -> Result<SourceLineMap> {
    let mut source_line_map = FxHashMap::default();
    
    for source_file in project_sources {
        // Only process Text files, skip Toml files
        let duvet_path = match source_file {
            SourceFile::Text { path, .. } => path,
            SourceFile::Toml(_) => continue, // Skip TOML files
        };
        
        // Find matching coverage using paths_match
        let coverage_option = coverage_data.as_generic().files.iter()
            .find(|(coverage_path, _)| paths_match(duvet_path, coverage_path))
            .map(|(_, file_coverage)| file_coverage);
            
        if let Some(file_coverage) = coverage_option {
            let line_map = build_line_map_for_file(duvet_path, &annotations, file_coverage)?;
            source_line_map.insert(duvet_path.to_path_buf(), line_map); // Path as key!
        }
    }
    
    Ok(source_line_map)
}

fn build_line_map_for_file(
    duvet_path: &Path,
    annotations: &AnnotationSet,
    file_coverage: &FileCoverage
) -> Result<LineMap> {
    // 1. Read file to get line count and content
    let file_content = std::fs::read_to_string(duvet_path)?;
    let lines: Vec<&str> = file_content.lines().collect();
    let line_count = lines.len();
    
    // 2. Initialize all lines as Unknown
    let mut line_map: LineMap = (1..=line_count as u64)
        .map(|line_num| (line_num, LineInfo::Unknown))
        .collect();
    
    // 3. Update with coverage data
    update_coverage_lines(&mut line_map, file_coverage);
    
    // 4. Update with annotations for this specific file
    update_annotation_lines(&mut line_map, annotations, duvet_path);
    
    // 5. Update whitespace lines
    update_whitespace_lines(&mut line_map, &lines);
    
    Ok(line_map)
}

fn update_coverage_lines(
    line_map: &mut LineMap, 
    file_coverage: &FileCoverage
) {
    // 1. Process line coverage
    for (&line_num, &hit_count) in &file_coverage.lines {
        let line_info = if hit_count > 0 {
            LineInfo::Executed(ExecutionType::Line)
        } else {
            LineInfo::NotExecuted(ExecutionType::Line)
        };
        line_map.insert(line_num as u64, line_info);
    }
    
    // 2. Process branch coverage (overwrites line coverage if both exist)
    for (&line_num, branches) in &file_coverage.branches {
        let any_branch_taken = branches.iter().any(|&taken| taken);
        let line_info = if any_branch_taken {
            LineInfo::Executed(ExecutionType::Branch)
        } else {
            LineInfo::NotExecuted(ExecutionType::Branch)
        };
        line_map.insert(line_num as u64, line_info);
    }
    
    // 3. Process functions for method boundaries (if we decide to implement this)
    // TODO: Handle functions for MethodBoundary detection
}

fn update_annotation_lines(line_map: &mut LineMap, annotations: &AnnotationSet, duvet_path: &Path) {
    for annotation in annotations.iter() {
        if annotation.source == *duvet_path {
            let start_line = annotation.anno_line as u64;
            let text_lines = annotation.original_text.lines().count();
            let end_line = start_line + text_lines as u64 - 1;

            for line_num in start_line..=end_line {
                line_map.insert(line_num, LineInfo::Annotation(annotation.clone()));
            }
        }
    }
}

fn update_whitespace_lines(
    line_map: &mut LineMap,
    lines: &[&str]
) {
    for (index, line_content) in lines.iter().enumerate() {
        let line_num = (index + 1) as u64; // Convert 0-based index to 1-based line number
        
        // Only process lines that are still Unknown
        if let Some(LineInfo::Unknown) = line_map.get(&line_num) {
            // Check if line contains only whitespace
            if line_content.trim().is_empty() {
                line_map.insert(line_num, LineInfo::Whitespace);
            }
            // If not whitespace, leave as Unknown
        }
    }
}

/// Check if a Duvet source path matches a coverage report path
/// Handles various path format differences between Duvet and coverage tools
fn paths_match(duvet_path: &Path, coverage_path: &str) -> bool {

    // Explicitly use the Display trait
    let duvet_path_str = format!("{}", duvet_path.display());
    
    // Strategy 1: Direct exact match
    if duvet_path_str == coverage_path {
        return true;
    }
    
    // Strategy 2: Check if duvet path (usually longer/absolute) ends with coverage path (usually shorter/relative)
    if duvet_path_str.ends_with(coverage_path) {
        return true;
    }
    
    // Strategy 3: Handle Jacoco parser bug where coverage path has extra package prefix
    // e.g. coverage_path = "com/example/src/main/java/com/example/StackingTest.java"
    // should match duvet_path = "src/main/java/com/example/StackingTest.java"
    if let Some(src_index) = coverage_path.find("src/") {
        let trimmed_coverage_path = &coverage_path[src_index..];
        if duvet_path_str == trimmed_coverage_path {
            return true;
        }
    }
    
    // Strategy 4: Check reverse - if coverage path ends with duvet path
    if coverage_path.ends_with(&duvet_path_str) {
        return true;
    }
    
    false
}

/// Parse coverage data from file
pub fn parse_coverage_data(
    coverage_path: &String,
    format: &CoverageFormat,
) -> Result<CoverageData> {
    // Parse coverage data
    let parser: Box<dyn CoverageParser> = match format {
        CoverageFormat::JacocoXml => Box::new(JacocoParser),
    };

    parser.parse(Path::new(coverage_path))
}

/// Check if an annotation is executed according to coverage data
pub fn is_annotation_executed(
    annotation: &Arc<Annotation>,
    source_line_map: &SourceLineMap
) -> bool {
    // Spec and Todo are never executable
    if matches!(annotation.anno, AnnotationType::Spec | AnnotationType::Todo) {
        return true;
    }

    // 1. Find the right file
    let line_map = match source_line_map.get(&annotation.source.to_path_buf()) {
        Some(map) => map,
        None => return false, // File not in coverage data
    };
    
    // 2. Find annotation end line
    let start_line = annotation.anno_line as u64;
    let text_lines = annotation.original_text.lines().count();
    let end_line = start_line + text_lines as u64 - 1;
    
    // 3. Confirm this is the same annotation
    for line_num in start_line..=end_line {
        if let Some(LineInfo::Annotation(stored_annotation)) = line_map.get(&line_num) {
            if stored_annotation != annotation {
                // TODO, this should be an error, false is just confusing
                return false; // Different annotation at expected location
            }
        } else {
            // TODO, this should be an error, false is just confusing
            return false; // Expected annotation not found
        }
    }
    
    // 4. Proceed forward from end of annotation
    let mut current_line = end_line + 1;
    
    loop {
        match line_map.get(&current_line) {
            Some(LineInfo::Whitespace) => {
                // Skip whitespace and continue
                // whitespace is not executable.
                current_line += 1;
            }
            Some(LineInfo::Annotation(next_annotation)) => {
                // Recurse the next annotation
                // Execution is a transitive property
                // If an annotation is stacked on an executable annotation
                // the stacked annotation has the same executable value.
                return is_annotation_executed(next_annotation, source_line_map);
            }
            Some(LineInfo::Executed(_)) => {
                return true; // Found executed line
            }
            Some(LineInfo::NotExecuted(_)) | Some(LineInfo::Unknown) => {
                // If a line is not executed clearly the annotation is not executed.
                // If the line is unknown we don't know what to do with this line.
                // The line could be a comment, but we can't know this without parsing the language.
                // Is the line a comment is a complicated question since we have to deal with many languages.
                return false; // Not executed or unknown = false
            }
            None => {
                return false; // End of file reached
            }
        }
    }
}


// NOTE: Unit tests for this module would require extensive mocking of duvet's
// internal types (Reference, Annotation, Target, Specification, CoverageData) which have
// complex APIs and construction requirements. The function logic is tested
// through integration tests via the query command.
