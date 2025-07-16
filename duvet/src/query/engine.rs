// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use super::{
    requirements::RequirementMode,
    checks::{
        coverage::{
            CoverageFormat,
            parse_coverage_data,
            build_source_line_map,
            is_annotation_executed,
        },
        is_annotation_covered,
    },
    result::{
        QueryResult,
        CheckResult,
        ImplementationResult,
        TestResult,
        CoverageResult,
        CoveredTestAnnotation,
        QueryStatus,
    },
    CheckType,
};
use crate::{
    annotation::{self, AnnotationSet, AnnotationType},
    project::Project,
    reference::{self},
    source::SourceFile,
    Result,
};
use duvet_core::{progress, diagnostic::IntoDiagnostic};
use glob::glob;
use std::{collections::HashSet, sync::Arc};

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

                let coverage_check_executed_tests_only = if matches!(check_type, CheckType::ExecutedCoverage) {
                    true
                } else {
                    false
                };

                for report in report_paths {
                    // Parse coverage data
                    let coverage_data = parse_coverage_data(&report, format)?;
                    let result = execute_coverage_check(
                        &project_data,
                        mode,
                        &coverage_data,
                        coverage_check_executed_tests_only,
                        verbose
                    ).await?;

                    results.push(result);
                }
            }
        };
    }
    
    // Calculate overall status
    let overall_status = if results.iter().all(|r| match r {
        CheckResult::Implementation(impl_result) => impl_result.status == QueryStatus::Pass,
        CheckResult::Tests(test_result) => test_result.status == QueryStatus::Pass,
        CheckResult::Coverage(cov_result) => cov_result.status == QueryStatus::Pass,
    }) {
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
    // Currently references are not used.
    // pub references: Arc<[crate::reference::Reference]>,
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
        // references,
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
        // 1. get the sections in scope
        .filter(|annotation| !matches!(annotation.anno, AnnotationType::Test))
        .filter(|annotation| mode.in_scope(annotation))
        // 2. organize the annotations into spec implemented and todo
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
    let (
        fully_implemented,
        mixed_implementation,
        incomplete_implementation,
        todo,
        not_implemented,
    ) = spec_annotations
        .iter()
        .try_fold(
            (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new()),
            |(mut full, mut mixed, mut incomplete, mut todo, mut not), annotation| -> Result<_> {
                let implemented_coverage = is_annotation_covered(annotation, &project_data.specifications, &implemented_annotations)?;
                let todo_coverage = is_annotation_covered(annotation, &project_data.specifications, &todo_annotations)?;

                let fully_covered = implemented_coverage.fully_covered;
                let implemented_len = implemented_coverage.covering_annotations.len();
                let todo_len = todo_coverage.covering_annotations.len();

                match (fully_covered, implemented_len, todo_len) {
                    // Fully implemented
                    (true, _, t) if 0 == t => full.push(implemented_coverage),
                    // Mixed implementations and todo. duplicates?
                    (true, _, t) if 0 < t => mixed.push(implemented_coverage.merge(todo_coverage)),
                    // Implementation missing something
                    (false, i, t) if 0 < i && 0 == t => incomplete.push(implemented_coverage),
                    // Mixed implementations and todo. duplicates?
                    (false, i, t) if 0 < i && 0 < t => mixed.push(implemented_coverage.merge(todo_coverage)),
                    // Only Todo
                    (false, i, t) if 0 == i && 0 < t => todo.push(todo_coverage),
                    // Zero coverage
                    _ => not.push(annotation.clone()),
                }

                Ok((full, mixed, incomplete, todo, not))
            }
        )?;
    
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

async fn execute_test_check(
    project_data: &ProjectData,
    mode: &RequirementMode,
    verbose: bool,
) -> Result<CheckResult> {

    if verbose {
        progress!("Running test annotation coverage check...");
    }

    let (
        implementation_annotations,
        test_annotations,
    ) = project_data
        .annotations
        .iter()
        // 1. Gather annotations that need testing
        // We are interested in testing things that are implemented
        // Making sure you have implemented everything is a job for the implementation check.
        .filter(|annotation| !matches!(annotation.anno,
            // A requirement. i.e. something that needs to be implemented
            AnnotationType::Spec
            // Not yet been implemented. Test driven development?
            | AnnotationType::Todo
            // Fundamentally true or not testable. No test required.
            | AnnotationType::Implication
            // You don't do it. not test required.
            | AnnotationType::Exception
        ))
        // 2. Only annotations that are in scope
        .filter(|annotation| mode.in_scope(annotation))
        // 3. Organize the annotations into implementations (things needing tests) and tests
        .fold(
            (Vec::new(), Vec::new()),
            |(mut impls, mut tests), annotation| {

                match &annotation.anno {
                    // An implementation, it needs a test
                    AnnotationType::Citation  => {
                        impls.push(annotation.clone());
                    }
                    // A test!
                    AnnotationType::Test => {
                        tests.push(annotation.clone());
                    }
                    // Shouldn't happen due to filter, but good to be explicit
                    _ => unreachable!()
                }

                (impls, tests)
            }
        );

    // 4. Classify each annotation
    let (
        fully_tested,
        incomplete_tests,
        not_tested,
    ) = implementation_annotations
        .iter()
        .try_fold(
            (Vec::new(), Vec::new(), Vec::new()),
            |(mut full, mut incomplete, mut not), annotation| -> Result<_> {
                let test_coverage = is_annotation_covered(annotation, &project_data.specifications, &test_annotations)?;

                let fully_covered = test_coverage.fully_covered;
                let tested_len = test_coverage.covering_annotations.len();

                match (fully_covered, tested_len) {
                    // Fully tested
                    (true, _) => full.push(test_coverage),
                    // Tests are missing some coverage
                    (false, t) if 0 < t => incomplete.push(test_coverage),
                    // Zero coverage
                    _ => not.push(annotation.clone()),
                }

                Ok((full, incomplete, not))
            }
        )?;

    let status = if incomplete_tests.len() == 0
        && not_tested.len() == 0 {
            QueryStatus::Pass
        } else {
            QueryStatus::Fail
        };

    Ok(CheckResult::Tests(TestResult {
        status: status,
        in_scope_requirements: implementation_annotations,
        fully_tested: fully_tested,
        incomplete_tests: incomplete_tests,
        not_tested: not_tested,
        verbose: verbose,
    }))
}

async fn execute_coverage_check(
    project_data: &ProjectData,
    mode: &RequirementMode,
    coverage_data: &crate::query::coverage::CoverageData,
    coverage_check_executed_tests_only: bool,
    verbose: bool,
) -> Result<CheckResult> {

    if verbose {
        progress!("Running test execution correlation check...");
    }

    let source_line_map = build_source_line_map(&project_data.annotations, coverage_data, &project_data.project_sources)?;

    // 1. For every annotation, use source_line_map and is_annotation_executed to find all executed annotations
    // 2. Split executed annotations into tests and implementations (type ==> CITATION | IMPLEMENTATION | IMPLICATION | EXCEPTION)
    let (
        executed_test_annotations,
        executed_implementation_annotations,
        not_executed_test_annotations,
        not_executed_implementation_annotations,
    ) = project_data
        .annotations
        .iter()
        // Spec and Todo are not executable, remove them
        .filter(|annotation| !matches!(annotation.anno, AnnotationType::Spec | AnnotationType::Todo))
        .filter(|annotation| mode.in_scope(annotation))
        .fold(
            (Vec::new(), Vec::new(), Vec::new(), Vec::new()),
            |(mut executed_tests, mut executed_impls, mut not_executed_tests, mut not_executed_impls), annotation| {
                let is_executed = is_annotation_executed(annotation, &source_line_map);
                
                match (is_executed, &annotation.anno) {
                    (true, AnnotationType::Test) => {
                        executed_tests.push(annotation.clone());
                    }
                    (true, AnnotationType::Citation | AnnotationType::Implication | AnnotationType::Exception) => {
                        executed_impls.push(annotation.clone());
                    }
                    (false, AnnotationType::Test) => {
                        not_executed_tests.push(annotation.clone());
                    }
                    (false, AnnotationType::Citation | AnnotationType::Implication | AnnotationType::Exception) => {
                        not_executed_impls.push(annotation.clone());
                    }
                    // Shouldn't happen due to filter, but good to be explicit
                    _ => unreachable!()
                }
                
                (executed_tests, executed_impls, not_executed_tests, not_executed_impls)
            },
        );

    // 3. For every test annotation, check how it is covered by executed implementations annotations
    let mut successful_annotations = Vec::new();
    let mut failed_annotations = Vec::new();
    executed_test_annotations
        .iter()
        .try_for_each(|annotation| {
            let executed_coverage = is_annotation_covered(annotation, &project_data.specifications, &executed_implementation_annotations)?;
            let not_executed_coverage = is_annotation_covered(annotation, &project_data.specifications, &not_executed_implementation_annotations)?;

            match executed_coverage.fully_covered {
                true => {
                    // TODO? If !not_executed_coverage.covering_annotations.is_empty() is this ok?
                    // In this case there are duplicate implementation annotations,
                    // some have been executed and some have not.
                    successful_annotations.push(CoveredTestAnnotation{
                        test: annotation.clone(),
                        test_executed: true,
                        executed_implementations: executed_coverage.covering_annotations,
                        not_executed_implementations: not_executed_coverage.covering_annotations,
                    });
                }
                false => {
                    failed_annotations.push(CoveredTestAnnotation{
                        test: annotation.clone(),
                        test_executed: true,
                        executed_implementations: executed_coverage.covering_annotations,
                        not_executed_implementations: not_executed_coverage.covering_annotations,
                    });
                }
            }

            <Result>::Ok(())
        }
    )?;

    if !coverage_check_executed_tests_only {
        not_executed_test_annotations
            .iter()
            .try_for_each(|annotation| {
                let executed_coverage = is_annotation_covered(annotation, &project_data.specifications, &executed_implementation_annotations)?;
                let not_executed_coverage = is_annotation_covered(annotation, &project_data.specifications, &not_executed_implementation_annotations)?;

                failed_annotations
                    .push(CoveredTestAnnotation{
                        test: annotation.clone(),
                        test_executed: false,
                        executed_implementations: executed_coverage.covering_annotations,
                        not_executed_implementations: not_executed_coverage.covering_annotations,
                    });
                <Result>::Ok(())
            })?;

    }

    let status = if 0 == failed_annotations.len() {
        QueryStatus::Pass
    } else {
        QueryStatus::Fail
    };

    // Put the output here
    Ok(CheckResult::Coverage(CoverageResult {
        status: status,
        executed_tests: executed_test_annotations,
        executed_implementations: executed_implementation_annotations,
        successful: successful_annotations,
        failed: failed_annotations,
        verbose: verbose,
    }))
}

fn expand_coverage_globs(reports: &[String]) -> Result<Vec<String>> {
    let mut expanded_paths = Vec::new();
    
    for pattern in reports {
        // TODO, same as project.js:
        // switch from `glob` to `duvet_core::glob` once the implementation
        // is compatible with the expected behavior.
        // Using glob here so that the pattern matching is predictable and the same as the current process.
        for entry in glob(pattern).into_diagnostic()? {
            let path = entry.into_diagnostic()?;
            expanded_paths.push(path.to_string_lossy().to_string());
        }
    }
    
    Ok(expanded_paths)
}
