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
    },
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

                let coverage_check_executed_tests_only = if matches!(check_type, CheckType::ExecutedCoverage) {
                    true
                } else {
                    false
                };

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
    let overall_status = if results.iter().all(|r| match r {
        CheckResult::Implementation(impl_result) => impl_result.status == QueryStatus::Pass,
        CheckResult::Tests(test_result) => test_result.status == QueryStatus::Pass,
        CheckResult::Coverage(cov_result) => cov_result.status == QueryStatus::Pass,
        CheckResult::Duplicates(dup_result) => dup_result.status == QueryStatus::Pass,
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
    let ClassifiedCoverage {
        complete_coverage: fully_tested,
        incomplete_coverage: incomplete_tests,
        no_coverage: not_tested,
        ..
    } = classify_annotation_coverage(
        project_data,
        &implementation_annotations,
        &test_annotations,
        &Vec::new(),
    ).await?;

    let status = if incomplete_tests.len() == 0
        && not_tested.len() == 0 {
            QueryStatus::Pass
        } else {
            QueryStatus::Fail
        };

    Ok(CheckResult::Tests(TestResult {
        status: status,
        in_scope_requirements: implementation_annotations,
        fully_tested,
        incomplete_tests,
        not_tested,
        verbose,
    }))
}

async fn execute_coverage_check(
    project_data: &ProjectData,
    mode: &RequirementMode,
    coverage_data: &Vec<crate::query::coverage::CoverageData>,
    coverage_check_executed_tests_only: bool,
    verbose: bool,
) -> Result<CheckResult> {

    if verbose {
        progress!("Running test execution correlation check...");
    }

    // Build source line maps in parallel for better performance
    let map_futures: Vec<_> = coverage_data.iter().map(|cover| {
        build_source_line_map(&project_data.annotations, cover, &project_data.project_sources)
    }).collect();

    let source_line_maps = futures::future::try_join_all(map_futures).await?;

    let mut test_annotations: Vec<_> = Vec::new();
    let mut implementation_annotations: Vec<_> = Vec::new();

    for annotation in project_data
        .annotations
        .iter()
        // Spec and Todo are not executable, remove them
        .filter(|annotation| !matches!(annotation.anno, AnnotationType::Spec | AnnotationType::Todo))
        .filter(|annotation| mode.in_scope(annotation))
    {
        match &annotation.anno {
            AnnotationType::Test => test_annotations.push(annotation.clone()),
            AnnotationType::Citation | AnnotationType::Implication | AnnotationType::Exception => 
                implementation_annotations.push(annotation.clone()),
            // Shouldn't happen due to filter, but good to be explicit
            _ => unreachable!()

        }
    }

    // 4. Classify each annotation
    let ClassifiedCoverage {
        complete_coverage,
        incomplete_coverage,
        no_coverage: _no_coverage,
        ..
    } = classify_annotation_coverage(
        project_data,
        &test_annotations,
        &implementation_annotations,
        &Vec::new(),
    ).await?;

    let mut successful: Vec<CoveredTestAnnotation> = Vec::new();
    let mut failed: Vec<CoveredTestAnnotation> = Vec::new();

    for test in complete_coverage.iter().chain(&incomplete_coverage) {
        let mut test_executed = false;
        for source_line_map in &source_line_maps {
            if is_annotation_executed(&test.target, &source_line_map) {
                test_executed = true;
                let (
                    executed_implementations,
                    not_executed_implementations,
                ): (Vec<_>, Vec<_>) = test
                    .covering_annotations
                    .iter()
                    .cloned()
                    .partition(|annotation| is_annotation_executed(&annotation, &source_line_map));

                let result = CoveredTestAnnotation {
                    test: test.target.clone(),
                    test_executed: true,
                    executed_implementations,
                    not_executed_implementations,
                };
                if result.not_executed_implementations.is_empty() {
                    successful.push(result);
                } else {
                    failed.push(result);
                }
            }
        }
        if !test_executed && !coverage_check_executed_tests_only {
            let result = CoveredTestAnnotation {
                test: test.target.clone(),
                test_executed: false,
                executed_implementations: Vec::new(),
                // What is the right value here?
                // Some of these annotations might be executed in some of the coverage.
                // Does communicate anything meaningful?
                not_executed_implementations: Vec::new(),
            };
            failed.push(result);
        }
        // What happens if the test is executed in multiple runs?
    }

    let executed_tests: AnnotationSet = successful
        .iter()
        .chain(&failed)
        .filter(|result| result.test_executed)
        .map(|result| result.test.clone())
        .collect::<BTreeSet<_>>()
        .into();
    let executed_from_tests: BTreeSet<_> = successful
        .iter()
        .chain(&failed)
        .flat_map(|result| &result.executed_implementations)
        .collect::<BTreeSet<_>>()
        .into();

    let executed_implementations = implementation_annotations
        .iter()
        .filter(|annotation| {
            if executed_from_tests.contains(annotation) {
                true
            } else {
                source_line_maps
                    .iter()
                    .any(|source_line_map| is_annotation_executed(annotation, &source_line_map))
            }
        })
        .cloned()
        .collect::<BTreeSet<_>>()
        .into();

    let status = if 0 == failed.len() {
        QueryStatus::Pass
    } else {
        QueryStatus::Fail
    };

    // Put the output here
    Ok(CheckResult::Coverage(CoverageResult {
        status,
        executed_tests,
        executed_implementations,
        successful,
        failed,
        verbose: verbose,
    }))
}

async fn execute_duplicates(
    project_data: &ProjectData,
    mode: &RequirementMode,
    verbose: bool,
) -> Result<CheckResult> {

    let annotations_by_type: HashMap<AnnotationType, Vec<Arc<Annotation>>> = project_data
        .annotations
        .iter()
        .filter(|annotation| mode.in_scope(annotation))
        .fold(HashMap::new(), |mut acc: HashMap<AnnotationType, Vec<Arc<Annotation>>>, annotation| {
            acc.entry(annotation.anno).or_default().push(annotation.clone());
            acc
        });

    // Create futures for concurrent classification by annotation type
    let classification_futures: Vec<_> = annotations_by_type.iter().map(|(annotation_type, annotations)| {
        let annotation_type = *annotation_type;
        let annotations = annotations.clone();
        async move {
            let classified_coverage = classify_annotation_coverage(
                project_data,
                &annotations,
                &annotations,
                &Vec::new(),
            ).await?;
            Ok::<_, crate::Error>((annotation_type, classified_coverage))
        }
    }).collect();

    let classification_results = futures::future::try_join_all(classification_futures).await?;
    let classified_annotations_by_type: HashMap<AnnotationType, ClassifiedCoverage> = classification_results.into_iter().collect();
    
    let mut duplicates_by_type: HashMap<AnnotationType, Duplicates>  = classified_annotations_by_type
        .into_iter()
        .map(|(annotation_type, classified)| 
            (annotation_type, convert_to_duplicates(classified))
        )
        .collect();

    let has_duplicates = duplicates_by_type
        .iter()
        .any(|(_type, by_type)| !by_type.duplicates.is_empty());

    let status = if has_duplicates {
        QueryStatus::Fail
    } else {
        QueryStatus::Pass
    };

    Ok(CheckResult::Duplicates(DuplicatesResult{
        status,
        spec: duplicates_by_type.remove(&AnnotationType::Spec).unwrap_or_else(empty_duplicates),
        implementation: duplicates_by_type.remove(&AnnotationType::Citation).unwrap_or_else(empty_duplicates),
        test: duplicates_by_type.remove(&AnnotationType::Test).unwrap_or_else(empty_duplicates),
        exception: duplicates_by_type.remove(&AnnotationType::Exception).unwrap_or_else(empty_duplicates),
        todo: duplicates_by_type.remove(&AnnotationType::Todo).unwrap_or_else(empty_duplicates),
        implication: duplicates_by_type.remove(&AnnotationType::Implication).unwrap_or_else(empty_duplicates),
        verbose,
    }))

}

fn convert_to_duplicates(classified: ClassifiedCoverage) -> Duplicates {
    // This assumes that you used classify_annotation_coverage
    // where annotations == maybe_primary_covering_annotations
    // This means that mixed_coverage == [] && secondary_coverage == []

    let duplicates = deduplicate_annotation_coverage(classified.complete_coverage);
    Duplicates {
        duplicates,
        some_overlap: classified.incomplete_coverage,
        unique: classified.no_coverage,
    }
}

fn empty_duplicates() -> Duplicates {
    Duplicates {
        duplicates: Vec::new(),
        some_overlap: Vec::new(),
        unique: Vec::new(),
    }
}

fn deduplicate_annotation_coverage(coverage_list: Vec<AnnotationCoverage>) -> Vec<AnnotationCoverage> {
    let mut seen_annotations = HashSet::new();
    let mut result = Vec::new();
    
    for coverage in coverage_list {
        if !seen_annotations.contains(&coverage.target) {
            // This target hasn't been seen yet, so keep this coverage
            seen_annotations.insert(coverage.target.clone());
            
            // Add all covering annotations to the seen set too
            for annotation in &coverage.covering_annotations {
                seen_annotations.insert(annotation.clone());
            }
            
            result.push(coverage);
        }
        // else: target already seen, skip this duplicate coverage
    }
    
    result
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
