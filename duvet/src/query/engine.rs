// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use super::{
    requirements::RequirementMode,
    checks::{
        coverage::{
            CoverageFormat,
            parse_coverage_data,
            build_execution_data,
            is_annotation_executed,
            ExecutionDataMap,
        },
        classify_annotation_coverage,
        ClassifiedCoverage,
    },
    result::{
        QueryResult,
        CheckResult,
        ImplementationResult,
        TestResult,
        CoverageResult,
        CoveredTestAnnotation,
        QueryStatus,
        DuplicatesResult,
        Duplicates,
        AnnotationCoverage,
        NotExecutedAnnotation,
    },
    coverage::AnnotationExecutionStatus,
    CheckType,
};
use crate::{
    annotation::{self, AnnotationSet, AnnotationType, Annotation},
    project::Project,
    reference::{self},
    source::SourceFile,
    Result,
};

use duvet_core::{progress, diagnostic::IntoDiagnostic};
use glob::glob;
use std::{collections::{HashMap, HashSet, BTreeSet}, sync::Arc};

pub async fn execute_checks(
    checks: &[(CheckType, &RequirementMode)],
    coverage_reports: Option<&Vec<String>>,
    coverage_format: Option<&CoverageFormat>,
    verbose: bool,
) -> Result<QueryResult> {
    // Load project data
    let project_data = load_project_data(verbose).await?;
    
    // Execute each check type
    let mut results = Vec::new();
    
    for (check_type, mode) in checks {
        match check_type {
            CheckType::Implementation => {
                let result = execute_implementation_check(&project_data, mode, verbose).await?;
                results.push(result);
            }
            CheckType::Test => {
                let result = execute_test_check(&project_data, mode, verbose).await?;
                results.push(result);
            }
            CheckType::Duplicates => {
                let result = execute_duplicates(&project_data, mode, verbose).await?;
                results.push(result);
            }
            CheckType::Coverage | CheckType::ExecutedCoverage => {

                // Determine coverage report path
                let report_globs = coverage_reports
                    .ok_or_else(|| duvet_core::error!(
                        "Coverage report path is required. Use --coverage-report"
                    ))?;
                let report_paths = expand_coverage_globs(report_globs)?;

                // Determine coverage format
                let format = coverage_format
                    .ok_or_else(|| duvet_core::error!(
                        "Coverage format is required. Use --coverage-format"
                    ))?;

                let coverage_check_executed_tests_only = matches!(check_type, CheckType::ExecutedCoverage);

                // Parse coverage data in parallel using async
                let parse_futures: Vec<_> = report_paths.iter().map(|path| {
                    parse_coverage_data(path, format)
                }).collect();

                let coverage_data = futures::future::try_join_all(parse_futures).await?;

                let result = execute_coverage_check(
                    &project_data,
                    mode,
                    &coverage_data,
                    coverage_check_executed_tests_only,
                    verbose
                ).await?;

                results.push(result);
            }
        };
    }
    
    // Calculate overall status
    let overall_status = if results.iter().all(|r| *r.status() == QueryStatus::Pass) {
        QueryStatus::Pass
    } else {
        QueryStatus::Fail
    };
    
    // Create and return QueryResult
    Ok(QueryResult {
        overall_status,
        checks: results,
    })
}

#[derive(Debug)]
pub struct ProjectData {
    pub specifications: Arc<std::collections::HashMap<Arc<crate::target::Target>, Arc<crate::specification::Specification>>>,
    pub project_sources: Arc<HashSet<SourceFile>>,
    pub annotations: AnnotationSet,
}

async fn load_project_data(verbose: bool) -> Result<ProjectData> {
    let project = Project::new();
    
    let config = project.config().await?;
    let config = config.as_ref();

    if let Some(config) = config {
        let progress = progress!("Extracting requirements");
        let count = config.load_specifications().await?;
        if count > 0 && verbose {
            progress!(progress, "Extracted requirements from {count} specifications");
        }
    }

    let progress = progress!("Scanning sources");
    let project_sources = project.sources().await?;
    let project_sources = Arc::new(project_sources);
    progress!(progress, "Scanned {} sources", project_sources.len());


    let progress = progress!("Parsing annotations");
    let annotations = annotation::query(project_sources.clone()).await?;
    progress!(progress, "Parsed {} annotations", annotations.len());

    let progress = progress!("Loading specifications");
    let download_path = project.download_path().await?;
    let specifications = annotation::specifications(annotations.clone(), download_path.clone()).await?;
    progress!(progress, "Loaded {} specifications", specifications.len());

    let progress = progress!("Mapping sections");
    let reference_map = annotation::reference_map(annotations.clone()).await?;
    progress!(progress, "Mapped {} sections", reference_map.len());

    let progress = progress!("Matching references");
    let references = reference::query(reference_map.clone(), specifications.clone()).await?;
    progress!(progress, "Matched {} references", references.len());

    Ok(ProjectData {
        specifications,
        project_sources,
        annotations,
    })
}

async fn execute_implementation_check(
    project_data: &ProjectData,
    mode: &RequirementMode,
    verbose: bool,
) -> Result<CheckResult> {
    if verbose {
        progress!("Running implementation annotation coverage check...");
    }

    let (
        spec_annotations,
        implemented_annotations,
        todo_annotations,
    ) = project_data
        .annotations
        .iter()
        .filter(|annotation| mode.in_scope(annotation))
        .filter(|annotation| !matches!(annotation.anno, AnnotationType::Test))
        .fold(
            (Vec::new(), Vec::new(), Vec::new()),
            |(mut specs, mut impls, mut todos), annotation| {

                match &annotation.anno {
                    AnnotationType::Spec => {
                        specs.push(annotation.clone());
                    }
                    AnnotationType::Citation | AnnotationType::Implication | AnnotationType::Exception => {
                        impls.push(annotation.clone());
                    }
                    AnnotationType::Todo => {
                        todos.push(annotation.clone());
                    }
                    // Shouldn't happen due to filter, but good to be explicit
                    _ => unreachable!()
                }

                (specs, impls, todos)
            }
        );

    // 4. Classify each spec annotation
    let ClassifiedCoverage {
        complete_coverage: fully_implemented,
        mixed_coverage: mixed_implementation,
        incomplete_coverage: incomplete_implementation,
        secondary_coverage: todo,
        no_coverage: not_implemented,
    } = classify_annotation_coverage(
        project_data,
        &spec_annotations,
        &implemented_annotations,
        &todo_annotations,
    ).await?;
    
    let status = if mixed_implementation.len() == 0
        && incomplete_implementation.len() == 0
        && todo.len() == 0
        && not_implemented.len() == 0 {
            QueryStatus::Pass
        } else {
            QueryStatus::Fail
        };

    Ok(CheckResult::Implementation(ImplementationResult {
        status: status,
        in_scope_requirements: spec_annotations,
        fully_implemented: fully_implemented,
        mixed_implementation: mixed_implementation,
        incomplete_implementation: incomplete_implementation,
        todo: todo,
        not_implemented: not_implemented,
        verbose: verbose,
    }))
}
