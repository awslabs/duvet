// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use super::{
    checks::{
        classify_annotation_coverage,
        coverage::{
            build_execution_data, executed_status_for, parse_coverage_data, CoverageFormat,
            ExecutionDataMap,
        },
        ClassifiedCoverage,
    },
    coverage::ExecutionStatus,
    requirements::RequirementMode,
    result::{
        AnnotationCoverage, CheckResult, CoverageResult, CoveredTestAnnotation, Duplicates,
        DuplicatesResult, ImplementationResult, NotExecutedAnnotation, QueryResult, QueryStatus,
        TestResult,
    },
    CheckType,
};
use crate::{
    annotation::{self, Annotation, AnnotationSet, AnnotationType},
    project::Project,
    reference::{self},
    source::SourceFile,
    Result,
};

use duvet_core::{diagnostic::IntoDiagnostic, progress};
use glob::glob;
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    sync::Arc,
};

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
                let report_globs = coverage_reports.ok_or_else(|| {
                    duvet_core::error!("Coverage report path is required. Use --coverage-report")
                })?;
                let report_paths = expand_coverage_globs(report_globs)?;

                // Determine coverage format
                let format = coverage_format.ok_or_else(|| {
                    duvet_core::error!("Coverage format is required. Use --coverage-format")
                })?;

                let coverage_check_executed_tests_only =
                    matches!(check_type, CheckType::ExecutedCoverage);

                // Parse coverage data in parallel using async
                let parse_futures: Vec<_> = report_paths
                    .iter()
                    .map(|path| parse_coverage_data(path, format))
                    .collect();

                let coverage_data = futures::future::try_join_all(parse_futures).await?;

                let result = execute_coverage_check(
                    &project_data,
                    mode,
                    &coverage_data,
                    coverage_check_executed_tests_only,
                    verbose,
                )
                .await?;

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
    pub specifications: Arc<
        std::collections::HashMap<
            Arc<crate::target::Target>,
            Arc<crate::specification::Specification>,
        >,
    >,
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
            progress!(
                progress,
                "Extracted requirements from {count} specifications"
            );
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
    let specifications =
        annotation::specifications(annotations.clone(), download_path.clone()).await?;
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

    // `-s`/`-q` are *spec-slice filters*: they cut the words of the spec to
    // select which requirements are in scope to report on. They are applied to
    // the requirement annotations (`Spec`) ONLY — never to the covering pool.
    // Coverers come along transitively: `is_annotation_covered` pairs a coverer
    // with a requirement only when they share an exact `target` (checks/mod.rs),
    // so a coverer quoting an out-of-scope slice of the spec simply never matches
    // an in-scope requirement and falls away on its own — no error.
    //
    // This is what keeps a filter honest: it narrows *what you look at*, but can
    // never turn a covered requirement into a miss (or a miss into a pass). A
    // requirement is covered when its coverers tile its full quote; filtering the
    // coverer pool by `-q` could drop one tile of that mosaic and manufacture a
    // false miss. So `in_scope` gates the `Spec` push below and nothing else.
    let (spec_annotations, implemented_annotations, todo_annotations) = project_data
        .annotations
        .iter()
        .filter(|annotation| !matches!(annotation.anno, AnnotationType::Test))
        .fold(
            (Vec::new(), Vec::new(), Vec::new()),
            |(mut specs, mut impls, mut todos), annotation| {
                match &annotation.anno {
                    AnnotationType::Spec => {
                        // Requirement role: apply the spec-slice filter here.
                        if mode.in_scope(annotation) {
                            specs.push(annotation.clone());
                        }
                    }
                    AnnotationType::Citation
                    | AnnotationType::Implication
                    | AnnotationType::Exception => {
                        // Coverer: never filtered — the full pool tiles the quote.
                        impls.push(annotation.clone());
                    }
                    AnnotationType::Todo => {
                        todos.push(annotation.clone());
                    }
                    // Shouldn't happen due to filter, but good to be explicit
                    _ => unreachable!(),
                }

                (specs, impls, todos)
            },
        );

    // 4. Classify each spec annotation
    let ClassifiedCoverage {
        complete_coverage: fully_implemented,
        mixed_coverage: mixed_implementation,
        incomplete_coverage: incomplete_implementation,
        pending_coverage: todo,
        no_coverage: not_implemented,
    } = classify_annotation_coverage(
        project_data,
        &spec_annotations,
        &implemented_annotations,
        &todo_annotations,
    )
    .await?;

    let status = if mixed_implementation.is_empty()
        && incomplete_implementation.is_empty()
        && todo.is_empty()
        && not_implemented.is_empty()
    {
        QueryStatus::Pass
    } else {
        QueryStatus::Fail
    };

    Ok(CheckResult::Implementation(ImplementationResult {
        status,
        in_scope_requirements: spec_annotations,
        fully_implemented,
        mixed_implementation,
        incomplete_implementation,
        todo,
        not_implemented,
        verbose,
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

    let (implementation_annotations, test_annotations) = project_data
        .annotations
        .iter()
        // 1. Gather annotations that need testing
        // We are interested in testing things that are implemented
        // Making sure you have implemented everything is a job for the implementation check.
        .filter(|annotation| {
            !matches!(
                annotation.anno,
                // A requirement. i.e. something that needs to be implemented
                AnnotationType::Spec
            // Not yet been implemented. Test driven development?
            | AnnotationType::Todo
            // Fundamentally true or not testable. No test required.
            | AnnotationType::Implication
            // You don't do it. not test required.
            | AnnotationType::Exception
            )
        })
        // 2. Organize the annotations into implementations (things needing tests) and tests.
        // The `-s`/`-q` spec-slice filter applies to the requirement role only —
        // here the implementations being tested — never to the covering `Test`
        // pool. See `execute_implementation_check` for the full rationale: a test
        // may tile a requirement's quote in several pieces, so filtering the test
        // pool by `-q` could drop one tile and manufacture a false "not tested".
        .fold(
            (Vec::new(), Vec::new()),
            |(mut impls, mut tests), annotation| {
                match &annotation.anno {
                    // An implementation, it needs a test. Requirement role here:
                    // apply the spec-slice filter.
                    AnnotationType::Citation => {
                        if mode.in_scope(annotation) {
                            impls.push(annotation.clone());
                        }
                    }
                    // A test! Coverer: never filtered.
                    AnnotationType::Test => {
                        tests.push(annotation.clone());
                    }
                    // Shouldn't happen due to filter, but good to be explicit
                    _ => unreachable!(),
                }

                (impls, tests)
            },
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
    )
    .await?;

    let status = if incomplete_tests.is_empty() && not_tested.is_empty() {
        QueryStatus::Pass
    } else {
        QueryStatus::Fail
    };

    Ok(CheckResult::Tests(TestResult {
        status,
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
    coverage_data: &[crate::query::coverage::CoverageData],
    coverage_check_executed_tests_only: bool,
    verbose: bool,
) -> Result<CheckResult> {
    if verbose {
        progress!("Running test execution correlation check...");
    }

    // Build execution data for each coverage report in parallel.
    // Each report produces an ExecutionDataMap (one entry per source file with coverage).
    let build_futures: Vec<_> = coverage_data
        .iter()
        .map(|cover| {
            build_execution_data(
                &project_data.annotations,
                cover,
                &project_data.project_sources,
            )
        })
        .collect();

    let execution_data_maps: Vec<ExecutionDataMap> =
        futures::future::try_join_all(build_futures).await?;
    let report_count = execution_data_maps.len();

    // Loud, non-verbose: files whose selected classifier could not produce a
    // trustworthy classification — a parse error, or an unbalanced scope stream
    // (spec §1.5). We refuse to score against a collapsed/garbage
    // tree; their annotations are reported `Unknown`. Surfaced unconditionally
    // because it signals either a mislabeled file or a classifier gap — both need
    // a human, and silence is the bug we are fixing. `query` stays non-blocking
    // (annotations are `Unknown`, the run continues); the hard-error gate belongs
    // to `report` once it consumes coverage. We report *that* and *where*, never
    // a *cause* (mislabeled vs. classifier gap is undecidable here).
    //= design/query/coverage-model-spec.md#scopes
    //= type=implementation
    //# When the stream is unbalanced,
    //# the coverage model MUST NOT score annotations against the collapsed scope tree;
    //# it MUST surface the file as a defeated classification and escalate
    //# (see [Classifier Selection and Dispatch](#dispatch)).
    //= design/query/coverage-model-spec.md#trust-taxonomy
    //= type=implementation
    //# duvet MUST NOT silently substitute the coarse model or score against
    //# the collapsed scope tree; it MUST escalate, reporting each located issue.
    {
        use crate::query::classify::{ClassifierFailure, ClassifierIssue};
        let defeated = collect_defeated_issues(&execution_data_maps);
        for (path, issues) in &defeated {
            let (parse, unbalanced): (Vec<&ClassifierIssue>, Vec<&ClassifierIssue>) = issues
                .iter()
                .partition(|i| matches!(i.reason, ClassifierFailure::ParseError));
            let mut detail = Vec::new();
            if !parse.is_empty() {
                let lines: Vec<String> = parse.iter().map(|i| i.line.to_string()).collect();
                detail.push(format!("parse error(s) at line(s) {}", lines.join(", ")));
            }
            if !unbalanced.is_empty() {
                let lines: Vec<String> = unbalanced.iter().map(|i| i.line.to_string()).collect();
                detail.push(format!(
                    "unbalanced scope stream near line(s) {}",
                    lines.join(", ")
                ));
            }
            progress!(
                "Coverage model: {} — the selected classifier could not produce a \
                 trustworthy classification ({}). The file may not be this \
                 language, or the classifier has a gap. Its annotations are \
                 reported Unknown; report the file or the classifier gap.",
                path.display(),
                detail.join("; ")
            );
        }
    }

    if verbose {
        // Tell the user which coverage path each covered file uses: the
        // language-aware two-phase model (classifier present) or the verified
        // degraded path (no classifier). Both are verified; the degraded path is
        // lower-fidelity (forward-nearest governance). Aggregate across reports.
        let mut classified_files: BTreeSet<&std::path::Path> = BTreeSet::new();
        let mut degraded_files: BTreeSet<&std::path::Path> = BTreeSet::new();
        for map in &execution_data_maps {
            for (path, data) in map {
                match data {
                    crate::query::checks::coverage::FileExecutionData::Classified(_) => {
                        classified_files.insert(path.as_path());
                    }
                    crate::query::checks::coverage::FileExecutionData::Degraded(_) => {
                        degraded_files.insert(path.as_path());
                    }
                    crate::query::checks::coverage::FileExecutionData::DefeatedClassification {
                        ..
                    } => {
                        // Reported unconditionally above (loud, not verbose-gated).
                    }
                }
            }
        }
        progress!(
            "Coverage model: {} file(s) language-aware (verified), {} file(s) degraded — no classifier (verified)",
            classified_files.len(),
            degraded_files.len()
        );
        for path in &degraded_files {
            progress!("  degraded (no classifier, verified): {}", path.display());
        }
    }

    let mut test_annotations: Vec<_> = Vec::new();
    let mut implementation_annotations: Vec<_> = Vec::new();

    // The spec-slice filter (`-s`/`-q`) applies to the requirement role only —
    // here the `Test` annotations being correlated — never to the covering
    // implementation pool. See `execute_implementation_check` for why filtering
    // coverers can manufacture a false miss.
    for annotation in project_data.annotations.iter().filter(|annotation| {
        !matches!(annotation.anno, AnnotationType::Spec | AnnotationType::Todo)
    }) {
        match &annotation.anno {
            // Requirement role: apply the spec-slice filter here.
            AnnotationType::Test => {
                if mode.in_scope(annotation) {
                    test_annotations.push(annotation.clone())
                }
            }
            // Coverer: never filtered.
            AnnotationType::Citation | AnnotationType::Implication | AnnotationType::Exception => {
                implementation_annotations.push(annotation.clone())
            }
            _ => unreachable!(),
        }
    }

    let ClassifiedCoverage {
        complete_coverage,
        incomplete_coverage,
        no_coverage,
        ..
    } = classify_annotation_coverage(
        project_data,
        &test_annotations,
        &implementation_annotations,
        &Vec::new(),
    )
    .await?;

    let mut successful: Vec<CoveredTestAnnotation> = Vec::new();
    let mut failed: Vec<CoveredTestAnnotation> = Vec::new();

    // Tests whose covered spec text has no correlated implementation annotation
    // anywhere. Per design §2.4 the coverage check must surface these — the test
    // points at behavior nobody implements — rather than silently dropping them
    // (which reported ✓ PASS with zero correlations and hid the gap until the
    // `duvet report` CI gate). In executed-coverage mode a NotExecuted such test
    // is skipped, consistent with that mode ignoring tests that did not run.
    let mut missing_implementation: Vec<Arc<Annotation>> = Vec::new();
    for test in &no_coverage {
        if coverage_check_executed_tests_only
            && matches!(
                fold_execution_status(test, &execution_data_maps),
                ExecutionStatus::NotExecuted
            )
        {
            continue;
        }
        missing_implementation.push(test.clone());
    }

    for test in complete_coverage.iter().chain(&incomplete_coverage) {
        // Witnesses for this test: the reports that show the test itself
        // executed (design §5.2). A pair (test, implementation) is discharged
        // only by a single witness that also shows the implementation
        // executed — a test executed in one report and an implementation
        // executed only in a different report do not correlate, because no
        // single measurement demonstrates that the test exercised the
        // implementation (Decision 13). Folding each side independently
        // across ALL reports asked (∃r: test(r)) ∧ (∃r: impl(r)) when the
        // check means ∃r: test(r) ∧ impl(r), passing exactly the vacuous
        // cross-report case the check exists to catch.
        let witnesses: Vec<&ExecutionDataMap> = execution_data_maps
            .iter()
            .filter(|exec_data| {
                matches!(
                    executed_status_for(&test.target, exec_data),
                    ExecutionStatus::Executed
                )
            })
            .collect();

        if !witnesses.is_empty() {
            // The test executed in at least one report. Evaluate each
            // covering implementation across the witnesses only, with OR
            // semantics within that set: a pair discharged by any one
            // witness must not be failed by another report that missed it
            // (Decision 6's motivating case, preserved by Decision 13).
            let mut executed_implementations = Vec::new();
            let mut not_executed_implementations = Vec::new();

            for annotation in &test.covering_annotations {
                let status = fold_execution_status(annotation, witnesses.iter().copied());
                if matches!(status, ExecutionStatus::Executed) {
                    executed_implementations.push(annotation.clone());
                } else {
                    not_executed_implementations.push(NotExecutedAnnotation {
                        annotation: annotation.clone(),
                        status,
                    });
                }
            }

            let result = CoveredTestAnnotation {
                test: test.target.clone(),
                test_execution_status: ExecutionStatus::Executed,
                executed_implementations,
                not_executed_implementations,
            };
            if result.not_executed_implementations.is_empty() {
                successful.push(result);
            } else {
                failed.push(result);
            }
        } else {
            // No witnesses: fold the test's own status across all reports
            // for the diagnostic (Unknown is preferred over Structural /
            // NotExecuted because it carries line information).
            let test_executed = fold_execution_status(&test.target, &execution_data_maps);

            // Unknown tests are NOT skipped in executed-coverage mode: they
            // represent annotation placement errors that must be fixed
            // regardless of which test you're working on. Only NotExecuted
            // tests are skipped.
            if coverage_check_executed_tests_only
                && matches!(test_executed, ExecutionStatus::NotExecuted)
            {
                continue;
            }

            let result = CoveredTestAnnotation {
                test: test.target.clone(),
                test_execution_status: test_executed,
                executed_implementations: Vec::new(),
                // When the test itself wasn't executed, its implementation
                // correlations are meaningless — we can't know which
                // implementations would have been reached.
                not_executed_implementations: Vec::new(),
            };
            failed.push(result);
        }
    }

    let executed_tests: AnnotationSet = successful
        .iter()
        .chain(&failed)
        .filter(|result| matches!(result.test_execution_status, ExecutionStatus::Executed))
        .map(|result| result.test.clone())
        .collect::<BTreeSet<_>>()
        .into();
    let executed_from_tests: BTreeSet<_> = successful
        .iter()
        .chain(&failed)
        .flat_map(|result| &result.executed_implementations)
        .collect::<BTreeSet<_>>();

    let executed_implementations = implementation_annotations
        .iter()
        .filter(|annotation| {
            if executed_from_tests.contains(annotation) {
                true
            } else {
                execution_data_maps.iter().any(|exec_data| {
                    matches!(
                        executed_status_for(annotation, exec_data),
                        ExecutionStatus::Executed
                    )
                })
            }
        })
        .cloned()
        .collect::<BTreeSet<_>>()
        .into();

    let status = if failed.is_empty() && missing_implementation.is_empty() {
        QueryStatus::Pass
    } else {
        QueryStatus::Fail
    };

    Ok(CheckResult::Coverage(CoverageResult {
        status,
        report_count,
        executed_tests,
        executed_implementations,
        successful,
        failed,
        missing_implementation,
        verbose,
    }))
}

async fn execute_duplicates(
    project_data: &ProjectData,
    mode: &RequirementMode,
    verbose: bool,
) -> Result<CheckResult> {
    // Unlike the coverage-fold checks, duplicates classifies each type against
    // *itself*, so there is no requirement/coverer split and no coverage mosaic
    // to dismantle: the worst a spec-slice filter can do here is not *show* you a
    // duplicate that lies outside the slice — it can never flip a verdict. So the
    // filter is applied uniformly, which is also what "only look at this slice"
    // means for a duplicate report.
    let annotations_by_type: HashMap<AnnotationType, Vec<Arc<Annotation>>> = project_data
        .annotations
        .iter()
        .filter(|annotation| mode.in_scope(annotation))
        .fold(
            HashMap::new(),
            |mut acc: HashMap<AnnotationType, Vec<Arc<Annotation>>>, annotation| {
                acc.entry(annotation.anno)
                    .or_default()
                    .push(annotation.clone());
                acc
            },
        );

    // Create futures for concurrent classification by annotation type
    let classification_futures: Vec<_> = annotations_by_type
        .iter()
        .map(|(annotation_type, annotations)| {
            let annotation_type = *annotation_type;
            let annotations = annotations.clone();
            async move {
                let classified_coverage = classify_annotation_coverage(
                    project_data,
                    &annotations,
                    &annotations,
                    &Vec::new(),
                )
                .await?;
                Ok::<_, crate::Error>((annotation_type, classified_coverage))
            }
        })
        .collect();

    let classification_results = futures::future::try_join_all(classification_futures).await?;
    let classified_annotations_by_type: HashMap<AnnotationType, ClassifiedCoverage> =
        classification_results.into_iter().collect();

    let mut duplicates_by_type: HashMap<AnnotationType, Duplicates> =
        classified_annotations_by_type
            .into_iter()
            .map(|(annotation_type, classified)| {
                (annotation_type, convert_to_duplicates(classified))
            })
            .collect();

    let has_duplicates = duplicates_by_type
        .iter()
        .any(|(_type, by_type)| !by_type.duplicates.is_empty());

    let status = if has_duplicates {
        QueryStatus::Fail
    } else {
        QueryStatus::Pass
    };

    // Build categories in a stable order
    let category_order: &[(&str, AnnotationType)] = &[
        ("Spec", AnnotationType::Spec),
        ("Implementation", AnnotationType::Citation),
        ("Test", AnnotationType::Test),
        ("Exception", AnnotationType::Exception),
        ("Todo", AnnotationType::Todo),
        ("Implication", AnnotationType::Implication),
    ];
    let categories: Vec<(&'static str, Duplicates)> = category_order
        .iter()
        .map(|(name, anno_type)| {
            (
                *name,
                duplicates_by_type
                    .remove(anno_type)
                    .unwrap_or_else(empty_duplicates),
            )
        })
        .collect();

    Ok(CheckResult::Duplicates(DuplicatesResult {
        status,
        categories,
        verbose,
    }))
}

fn convert_to_duplicates(classified: ClassifiedCoverage) -> Duplicates {
    // This assumes that you used classify_annotation_coverage
    // where annotations == maybe_satisfied_covering_annotations
    // This means that mixed_coverage == [] && pending_coverage == []

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

/// Fold an annotation's execution status across the given coverage reports
/// with OR semantics: if any report shows it `Executed`, the result is
/// `Executed` (design §5.2). Among the remaining statuses, `Unknown` is
/// preferred over `Structural`/`NotExecuted` because it carries diagnostic
/// line information; `NotExecuted` is the base case when there are no reports.
///
/// The caller chooses the quantifier scope by choosing the reports (design
/// §5.2, Decision 13): fold over ALL reports for per-annotation questions
/// ("was this ever executed?"), or over a single test's witnesses for pair
/// discharge ("did the implementation run in a report where the test ran?").
fn fold_execution_status<'a>(
    annotation: &Arc<Annotation>,
    execution_data_maps: impl IntoIterator<Item = &'a ExecutionDataMap>,
) -> ExecutionStatus {
    let mut folded = ExecutionStatus::NotExecuted;
    for exec_data in execution_data_maps {
        let status = executed_status_for(annotation, exec_data);
        match status {
            // Executed wins outright — no later report can override it.
            ExecutionStatus::Executed => return ExecutionStatus::Executed,
            // Prefer Unknown over any previously-seen non-executed status.
            ExecutionStatus::Unknown { .. } => folded = status,
            // Structural / NotExecuted: only take it if we have nothing better.
            _ => {
                if matches!(folded, ExecutionStatus::NotExecuted) {
                    folded = status;
                }
            }
        }
    }
    folded
}

/// Aggregate every located issue from files whose classification was defeated,
/// across all reports — the escalation's input. Extracted so a unit test can
/// pin that each located issue reaches the escalation surface (none dropped,
/// none merged away); the loud per-file report in `execute_coverage_check`
/// iterates exactly this.
fn collect_defeated_issues(
    execution_data_maps: &[ExecutionDataMap],
) -> std::collections::BTreeMap<std::path::PathBuf, Vec<crate::query::classify::ClassifierIssue>> {
    let mut defeated: std::collections::BTreeMap<
        std::path::PathBuf,
        Vec<crate::query::classify::ClassifierIssue>,
    > = std::collections::BTreeMap::new();
    for map in execution_data_maps {
        for (path, data) in map {
            if let crate::query::checks::coverage::FileExecutionData::DefeatedClassification {
                issues,
            } = data
            {
                defeated.entry(path.clone()).or_default().extend(issues);
            }
        }
    }
    defeated
}

fn deduplicate_annotation_coverage(
    coverage_list: Vec<AnnotationCoverage>,
) -> Vec<AnnotationCoverage> {
    let mut seen_annotations = HashSet::new();
    let mut result = Vec::new();

    for coverage in coverage_list {
        if !seen_annotations.contains(&coverage.target) {
            // This target hasn't been seen yet, so keep this coverage.
            //
            // Only the *target* is marked seen — not its covering annotations.
            // A covering annotation can independently be the target of another
            // duplicate relationship (e.g. two identical annotations both cover
            // a third with a partial quote, but are also exact duplicates of
            // each other). Marking coverers seen dropped that second
            // relationship, hiding real duplicate pairs from the report.
            seen_annotations.insert(coverage.target.clone());

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
        let mut matched = false;
        for entry in glob(pattern).into_diagnostic()? {
            let path = entry.into_diagnostic()?;
            expanded_paths.push(path.to_string_lossy().to_string());
            matched = true;
        }

        // Each pattern must individually match at least one file. A pattern
        // matching zero files (typo'd path, wrong working directory) would
        // otherwise be silently dropped: with no other patterns the coverage
        // check passes vacuously ("reports checked: 0"), and alongside
        // matching patterns the dead pattern vanishes while the check may
        // still pass — the user believes reports were checked that never
        // existed. A union-level check would miss the second shape, so the
        // error is per-pattern.
        if !matched {
            return Err(duvet_core::error!(
                "coverage report pattern '{pattern}' matched no files"
            ));
        }
    }

    Ok(expanded_paths)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::{
        checks::coverage::FileExecutionData,
        classify::{ClassifierFailure, ClassifierIssue},
    };
    use std::sync::Arc;

    /// Minimal annotation in `path`: `//= spec#s` on line 1, `code();` on
    /// line 2. Single-line string literal on purpose — a multiline fixture
    /// with `//=` at line start would be ingested as a real annotation now
    /// that this file is a scanned source (see `.duvet/config.toml`).
    fn annotation(path: &str) -> Arc<Annotation> {
        use crate::annotation::AnnotationLevel;
        use duvet_core::file::SourceFile as CoreSourceFile;
        let contents = "//= spec#s\ncode();\n";
        let source = CoreSourceFile::new(path, contents).unwrap();
        let text = source.substr_range(0..10).unwrap();
        let target = source.substr_range(4..10).unwrap();
        let quote = source.substr_range(11..18).unwrap();
        Arc::new(Annotation {
            source: source.path().clone(),
            anno_line: 1,
            original_target: target,
            original_text: text,
            original_quote: quote,
            anno: AnnotationType::Citation,
            target: "spec#s".to_string(),
            quote: String::new(),
            comment: String::new(),
            manifest_dir: source.path().clone(),
            level: AnnotationLevel::Auto,
            format: crate::specification::Format::Auto,
            tracking_issue: String::new(),
            feature: String::new(),
            tags: Default::default(),
            blob_link: None,
        })
    }

    /// A file whose classification was defeated (here: an unbalanced scope
    /// stream) is never scored: its annotations resolve to a located
    /// `Unknown` anchored at the issue — not an `Executed`/`NotExecuted`
    /// verdict computed against the collapsed whole-file scope tree, and not
    /// a silent hand-off to the coarse degraded model.
    #[test]
    fn defeated_classification_is_surfaced_not_scored() {
        let ann = annotation("a/broken.rs");
        let mut map = ExecutionDataMap::default();
        map.insert(
            std::path::PathBuf::from("a/broken.rs"),
            FileExecutionData::DefeatedClassification {
                issues: vec![ClassifierIssue {
                    reason: ClassifierFailure::UnbalancedScopes,
                    line: 7,
                }],
            },
        );
        //= design/query/coverage-model-spec.md#scopes
        //= type=test
        //# When the stream is unbalanced,
        //# the coverage model MUST NOT score annotations against the collapsed scope tree;
        //# it MUST surface the file as a defeated classification and escalate
        //# (see [Classifier Selection and Dispatch](#dispatch)).
        //= design/query/coverage-model-spec.md#trust-taxonomy
        //= type=test
        //# duvet MUST NOT silently substitute the coarse model or score against
        //# the collapsed scope tree;
        assert!(matches!(
            executed_status_for(&ann, &map),
            ExecutionStatus::Unknown { line_number: 7 }
        ));
    }

    /// Escalation carries every located issue: the aggregation feeding the
    /// loud per-file report (the escalation surface) preserves each located
    /// issue from every defeated file across all reports — none dropped,
    /// none merged away. The printing itself is presentation glue; what is
    /// pinned here is that each located issue reaches it.
    #[test]
    fn escalation_reports_each_located_issue() {
        let unbalanced = |line| ClassifierIssue {
            reason: ClassifierFailure::UnbalancedScopes,
            line,
        };
        let parse = |line| ClassifierIssue {
            reason: ClassifierFailure::ParseError,
            line,
        };
        let mut map_a = ExecutionDataMap::default();
        map_a.insert(
            std::path::PathBuf::from("a/broken.rs"),
            FileExecutionData::DefeatedClassification {
                issues: vec![unbalanced(7), parse(2)],
            },
        );
        let mut map_b = ExecutionDataMap::default();
        map_b.insert(
            std::path::PathBuf::from("b/also_broken.rs"),
            FileExecutionData::DefeatedClassification {
                issues: vec![unbalanced(11)],
            },
        );
        let defeated = collect_defeated_issues(&[map_a, map_b]);
        //= design/query/coverage-model-spec.md#trust-taxonomy
        //= type=test
        //# it MUST escalate, reporting each located issue.
        assert_eq!(defeated.len(), 2);
        assert_eq!(
            defeated[std::path::Path::new("a/broken.rs")]
                .iter()
                .map(|i| i.line)
                .collect::<Vec<_>>(),
            [7, 2]
        );
        assert_eq!(
            defeated[std::path::Path::new("b/also_broken.rs")]
                .iter()
                .map(|i| i.line)
                .collect::<Vec<_>>(),
            [11]
        );
    }

    /// Creates a scratch directory containing one coverage report file and
    /// returns (dir, report_path). Std-only; no tempfile dependency.
    fn scratch_report_dir(tag: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("duvet-expand-globs-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let report = dir.join("lcov.info");
        std::fs::write(&report, "SF:src/lib.rs\nDA:1,1\nend_of_record\n").unwrap();
        (dir, report)
    }

    /// A single `--coverage-report` pattern matching zero files MUST be a
    /// hard error naming the pattern verbatim — not a vacuous pass with
    /// zero reports checked.
    #[test]
    fn zero_match_coverage_pattern_is_hard_error() {
        let (dir, _) = scratch_report_dir("all-zero");
        let pattern = dir.join("nonexistent/*.info").display().to_string();

        let err = expand_coverage_globs(std::slice::from_ref(&pattern)).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains(&format!("'{pattern}' matched no files")),
            "error must name the dead pattern verbatim, got: {msg}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Each pattern must individually match at least one file: a dead
    /// pattern alongside matching ones MUST still error — a non-empty
    /// union does not excuse it.
    #[test]
    fn partial_zero_match_coverage_pattern_is_hard_error() {
        let (dir, report) = scratch_report_dir("partial-zero");
        let live = report.display().to_string();
        let dead = dir.join("typo/*.info").display().to_string();

        let err = expand_coverage_globs(&[live, dead.clone()]).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains(&format!("'{dead}' matched no files")),
            "error must name the dead pattern, got: {msg}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Patterns that each match at least one file expand successfully —
    /// the guard rejects only dead patterns.
    #[test]
    fn matching_coverage_patterns_expand() {
        let (dir, report) = scratch_report_dir("happy");
        let literal = report.display().to_string();
        let globbed = dir.join("*.info").display().to_string();

        let paths = expand_coverage_globs(&[literal.clone(), globbed]).unwrap();
        assert_eq!(paths, [literal.clone(), literal]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
