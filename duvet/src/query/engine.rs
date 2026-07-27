// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use super::{
    checks::{
        classify_annotation_coverage,
        coverage::{
            classify_files, coverage_path_matches, executed_status, resolve_target_line,
            ClassificationMap, CoverageFormat, FileClassification, SourceIndex,
        },
        ClassifiedCoverage,
    },
    coverage::ExecutionStatus,
    producers::{produce, CoverageProducer, CoverageSource, RequestedPosition},
    requirements::RequirementMode,
    result::{
        AnnotationCoverage, CheckResult, CoverageResult, CoveredTestAnnotation,
        Duplicates, DuplicatesResult, ImplementationResult,
        NotExecutedAnnotation, QueryResult, QueryStatus, TestResult, UnwitnessedTestAnnotation,
    },
    witness::{bound_witnesses, discharge_verdict, witness_refs, ResolvedTarget, Witness},
    CheckType,
};
use crate::{
    annotation::{self, Annotation, AnnotationSet, AnnotationType},
    project::Project,
    reference::{self},
    source::SourceFile,
    Result,
};

use duvet_core::progress;
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
};

pub async fn execute_checks(
    checks: &[(CheckType, &RequirementMode)],
    coverage_reports: Option<&Vec<String>>,
    coverage_format: Option<&CoverageFormat>,
    coverage_sources: &[CoverageSource],
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
                // Assemble the declared coverage sources (Decision 10: each
                // source pairs a producer with its artifacts, declared
                // repeatably). The legacy flags are the one-source
                // degenerate case and combine with --coverage-source.
                let mut sources: Vec<CoverageSource> = Vec::new();
                if let Some(report_globs) = coverage_reports {
                    let format = coverage_format.ok_or_else(|| {
                        duvet_core::error!("Coverage format is required. Use --coverage-format")
                    })?;
                    let producer = match format {
                        CoverageFormat::JacocoXml => CoverageProducer::JacocoXml,
                    };
                    sources.push(CoverageSource {
                        producer,
                        globs: report_globs.clone(),
                    });
                }
                sources.extend_from_slice(coverage_sources);
                if sources.is_empty() {
                    return Err(duvet_core::error!(
                        "Coverage source is required. Use --coverage-report with \
                         --coverage-format, or --coverage-source"
                    ));
                }

                let coverage_check_executed_tests_only =
                    matches!(check_type, CheckType::ExecutedCoverage);

                let load = load_witnesses(&sources, &project_data).await?;

                let result = execute_coverage_check(
                    &project_data,
                    mode,
                    load,
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

/// Everything witness production yields besides the witnesses:
/// the per-file classification cache (already seeded with test-annotation
/// files when a prover producer needed target resolution) and the source
/// index (absolute paths for suffix matching).
struct WitnessLoad {
    witnesses: Vec<Witness>,
    classification: ClassificationMap,
    index: SourceIndex,
}

/// Produce every declared source's witnesses (spec §1.7), in declaration
/// order (which makes the "first discharging witness" named in verdicts
/// deterministic).
///
/// Prover producers are annotation-driven (spec §5.2): they consume the
/// *resolved positions* of test annotations — positions, not annotations,
/// per §1.7's inertness requirement — so when one is declared, test files
/// are classified first and targets resolved via the verified target
/// resolution.
async fn load_witnesses(
    sources: &[CoverageSource],
    project_data: &ProjectData,
) -> Result<WitnessLoad> {
    let index = SourceIndex::build(&project_data.project_sources)?;
    let mut classification: ClassificationMap = Default::default();

    let needs_positions = sources
        .iter()
        .any(|s| matches!(s.producer, CoverageProducer::VerusSst));
    let mut positions: Vec<RequestedPosition> = Vec::new();
    if needs_positions {
        let test_files: HashSet<PathBuf> = project_data
            .annotations
            .iter()
            .filter(|a| matches!(a.anno, AnnotationType::Test))
            .map(|a| a.source.to_path_buf())
            .collect();
        classification.extend(classify_files(&project_data.annotations, test_files).await?);
        for annotation in project_data
            .annotations
            .iter()
            .filter(|a| matches!(a.anno, AnnotationType::Test))
        {
            let path = annotation.source.to_path_buf();
            let Some(file_classification) = classification.get(&path) else {
                continue;
            };
            let Some(line) = resolve_target_line(annotation, file_classification) else {
                // Unresolvable target: no positional witness can bind it
                // (spec §1.5); the annotation surfaces through W6.
                continue;
            };
            let Some(absolute) = index.absolute_of(&path) else {
                continue;
            };
            positions.push(RequestedPosition {
                absolute_file: absolute.to_string(),
                line,
            });
        }
    }

    let mut witnesses = Vec::new();
    for source in sources {
        witnesses.extend(
            produce(source, &positions, coverage_path_matches, |file| {
                index.matches_any(file)
            })
            .await?,
        );
    }

    Ok(WitnessLoad {
        witnesses,
        classification,
        index,
    })
}

/// Fold execution statuses with OR semantics: `Executed` wins outright;
/// among the rest `Unknown` is preferred (it carries a diagnostic line);
/// `NotExecuted` is the base case. The caller chooses the quantifier scope
/// by choosing the statuses: fold over ALL witnesses for global questions
/// (Property W3), or over one test's bound witnesses for pair discharge
/// (Property W1) — same-witness discharge is exactly this scoping.
fn fold_statuses(statuses: impl IntoIterator<Item = ExecutionStatus>) -> ExecutionStatus {
    let mut folded = ExecutionStatus::NotExecuted;
    for status in statuses {
        match status {
            ExecutionStatus::Executed => return ExecutionStatus::Executed,
            ExecutionStatus::Unknown { .. } => folded = status,
            _ => {
                if matches!(folded, ExecutionStatus::NotExecuted) {
                    folded = status;
                }
            }
        }
    }
    folded
}

async fn execute_coverage_check(
    project_data: &ProjectData,
    mode: &RequirementMode,
    load: WitnessLoad,
    coverage_check_executed_tests_only: bool,
    verbose: bool,
) -> Result<CheckResult> {
    if verbose {
        progress!("Running test execution correlation check...");
    }

    let WitnessLoad {
        witnesses,
        mut classification,
        index,
    } = load;

    // Match each witness's per-file maps to project sources (suffix rule,
    // both ambiguity refusals), then classify every matched file once.
    // Classification is witness-invariant, so this cache sits above the
    // witness loop: witness count can exceed report count by orders of
    // magnitude for prover producers.
    let matched: Vec<_> = witnesses
        .iter()
        .map(|w| index.match_witness_files(&w.files))
        .collect::<Result<Vec<_>, _>>()?;

    let unclassified: HashSet<PathBuf> = matched
        .iter()
        .flat_map(|m| m.keys().cloned())
        .filter(|p| !classification.contains_key(p))
        .collect();
    classification.extend(classify_files(&project_data.annotations, unclassified).await?);

    // Loud, non-verbose: files whose selected classifier could not produce a
    // trustworthy classification — a parse error, or an unbalanced scope
    // stream (spec §1.5). We refuse to score against a collapsed/garbage
    // tree; their annotations are reported `Unknown`. Surfaced unconditionally
    // because it signals either a mislabeled file or a classifier gap — both
    // need a human, and silence is the bug we are fixing.
    {
        use crate::query::classify::{ClassifierFailure, ClassifierIssue};
        let mut defeated: std::collections::BTreeMap<&std::path::Path, &Vec<ClassifierIssue>> =
            std::collections::BTreeMap::new();
        for (path, data) in &classification {
            if let FileClassification::Defeated { issues } = data {
                defeated.insert(path.as_path(), issues);
            }
        }
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
        // degraded path (no classifier). Both are verified; the degraded path
        // is lower-fidelity (forward-nearest governance).
        let mut classified_files: BTreeSet<&std::path::Path> = BTreeSet::new();
        let mut degraded_files: BTreeSet<&std::path::Path> = BTreeSet::new();
        for (path, data) in &classification {
            match data {
                FileClassification::Classified { .. } => {
                    classified_files.insert(path.as_path());
                }
                FileClassification::Degraded { .. } => {
                    degraded_files.insert(path.as_path());
                }
                FileClassification::Defeated { .. } => {
                    // Reported unconditionally above (loud, not verbose-gated).
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

    // The executed(X, w) cell (spec §1.4): the existing verified Phases 1-3
    // scored against one witness's maps. Everything above this line is
    // per-file and witness-invariant; everything below quantifies over
    // witnesses (the shape milestone 3 verifies as Phase 4).
    let cell = |annotation: &Arc<Annotation>, witness_index: usize| -> ExecutionStatus {
        let path = annotation.source.to_path_buf();
        executed_status(
            annotation,
            classification.get(&path),
            matched[witness_index].get(&path).copied(),
        )
    };

    // A test annotation's resolved target, for the ByRootSpan claim arm
    // (spec §1.5). None when the file was never classified (it had no
    // coverage and no prover producer is configured), classification was
    // defeated, or the walk found no target - all of which bind no
    // positional witness.
    let resolve = |annotation: &Arc<Annotation>| -> Option<ResolvedTarget> {
        let path = annotation.source.to_path_buf();
        let line = resolve_target_line(annotation, classification.get(&path)?)?;
        Some(ResolvedTarget {
            absolute_file: index.absolute_of(&path)?.to_string(),
            line,
        })
    };

    // Witnesses carried with their index so the cell matrix and the pure
    // quantifier layer (witness.rs) run over the same carrier.
    let indexed: Vec<(usize, &Witness)> = witnesses.iter().enumerate().collect();

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
    let mut unwitnessed: Vec<UnwitnessedTestAnnotation> = Vec::new();

    // Tests whose covered spec text has no correlated implementation
    // annotation anywhere (design §2.4). In executed-coverage mode a
    // NotExecuted such test is skipped, consistent with that mode ignoring
    // tests that did not run.
    let mut missing_implementation: Vec<Arc<Annotation>> = Vec::new();
    for test in &no_coverage {
        if coverage_check_executed_tests_only
            && matches!(
                fold_statuses((0..witnesses.len()).map(|wi| cell(test, wi))),
                ExecutionStatus::NotExecuted
            )
        {
            continue;
        }
        missing_implementation.push(test.clone());
    }

    for test in complete_coverage.iter().chain(&incomplete_coverage) {
        //= design/witness/spec.md#claim-rules
        //= type=implementation
        //# A test annotation must find *its own* witness.
        let resolved_target = resolve(&test.target);
        let bound = bound_witnesses(
            &indexed,
            |carrier: &(usize, &Witness)| carrier.1,
            |carrier: &(usize, &Witness)| cell(&test.target, carrier.0),
            resolved_target.as_ref(),
            coverage_path_matches,
        );

        if !bound.is_empty() {
            // The test is witnessed (Property W2). Evaluate each covering
            // implementation against EVERY bound witness: the verdict is
            // universal (Decision 14) — one bound witness that did not
            // execute the implementation fails the pair; bound witnesses
            // are never outvoted.
            //
            //= design/witness/spec.md#discharge
            //= type=implementation
            //# witnesses_for(T)  =  { w ∈ delivered : binds(T, w) }
            //#
            //# discharged(T, I)  ⟺  witnesses_for(T) ≠ ∅
            //#                       ∧  ∀w ∈ witnesses_for(T) : executed(I, w)
            let mut executed_implementations = Vec::new();
            let mut not_executed_implementations = Vec::new();

            for annotation in &test.covering_annotations {
                let verdict = discharge_verdict(
                    &bound,
                    |carrier: &(usize, &Witness)| carrier.1,
                    |carrier| cell(annotation, carrier.0),
                );
                if verdict.discharged {
                    executed_implementations.push(annotation.clone());
                } else {
                    // Headline status for the failing implementation:
                    // fold over the witnesses that did NOT execute it
                    // (Unknown preferred — it carries a line number).
                    let status = fold_statuses(
                        verdict
                            .per_witness
                            .iter()
                            .filter(|r| !r.executed)
                            .map(|r| r.status),
                    );
                    not_executed_implementations.push(NotExecutedAnnotation {
                        annotation: annotation.clone(),
                        status,
                        per_witness: verdict.per_witness,
                    });
                }
            }

            let result = CoveredTestAnnotation {
                test: test.target.clone(),
                test_execution_status: ExecutionStatus::Executed,
                bound_witnesses: witness_refs(&bound, |carrier| carrier.1),
                executed_implementations,
                not_executed_implementations,
            };
            if result.not_executed_implementations.is_empty() {
                successful.push(result);
            } else {
                failed.push(result);
            }
        } else {
            //= design/witness/spec.md#property-w6-unwitnessed-test-annotations
            //= type=implementation
            //# ¬∃ w ∈ witnesses : binds(T, w)   ⟹   T is reported unwitnessed
            //
            // Diagnostic detail: fold the test's own execution status across
            // ALL witnesses (Unknown is preferred over Structural /
            // NotExecuted because it carries line information).
            let diagnostic_status = fold_statuses((0..witnesses.len()).map(|wi| cell(&test.target, wi)));

            // Unknown tests are NOT skipped in executed-coverage mode: they
            // represent annotation placement errors that must be fixed
            // regardless of which test you're working on. Only NotExecuted
            // tests are skipped.
            if coverage_check_executed_tests_only
                && matches!(diagnostic_status, ExecutionStatus::NotExecuted)
            {
                continue;
            }

            unwitnessed.push(UnwitnessedTestAnnotation {
                test: test.target.clone(),
                diagnostic_status,
            });
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

    //= design/witness/spec.md#property-w3-global-execution
    //= type=implementation
    //# report_ever_executed(I, witnesses) = true
    //#     ⟺  ∃ w ∈ witnesses : executed(I, w)
    let executed_implementations = implementation_annotations
        .iter()
        .filter(|annotation| {
            if executed_from_tests.contains(annotation) {
                true
            } else {
                (0..witnesses.len()).any(|wi| {
                    matches!(cell(annotation, wi), ExecutionStatus::Executed)
                })
            }
        })
        .cloned()
        .collect::<BTreeSet<_>>()
        .into();

    let status = if failed.is_empty() && missing_implementation.is_empty() && unwitnessed.is_empty()
    {
        QueryStatus::Pass
    } else {
        QueryStatus::Fail
    };

    Ok(CheckResult::Coverage(CoverageResult {
        status,
        report_count: witnesses.len(),
        executed_tests,
        executed_implementations,
        successful,
        failed,
        missing_implementation,
        unwitnessed,
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

