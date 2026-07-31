// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use super::{
    checks::{
        classify_annotation_coverage,
        coverage::{
            classify_files, coverage_path_matches, executed_status, resolve_target_line,
            ClassificationMap, FileClassification, SourceIndex,
        },
        ClassifiedCoverage,
    },
    coverage::ExecutionStatus,
    producers::{produce, CoverageFormat, CoverageProducer, CoverageSource, RequestedPosition},
    requirements::RequirementMode,
    result::{
        AnnotationCoverage, CheckResult, CoverageResult, CoveredTestAnnotation, Duplicates,
        DuplicatesResult, ImplementationResult, MissingImplementationTest, NotExecutedAnnotation,
        QueryResult, QueryStatus, TestResult, UnwitnessedTestAnnotation,
    },
    witness::{VerifiedVerdicts, Witness},
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
                    sources.push(CoverageSource {
                        producer: format.producer(),
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

/// An annotation's role in one check's coverage fold.
enum CheckRole {
    /// The requirement side: the annotations the check reports on.
    Requirement,
    /// The covering pool: the annotations that tile a requirement's quote.
    Coverer,
    /// Pending work (`Todo`), a third bucket only the implementation
    /// check consumes.
    Pending,
}

/// One check's annotation partition: requirement role, covering pool,
/// and (for the implementation check) pending todos.
#[derive(Default)]
struct RolePartition {
    requirements: Vec<Arc<Annotation>>,
    coverers: Vec<Arc<Annotation>>,
    pending: Vec<Arc<Annotation>>,
}

/// Split the annotation set into each check's roles, applying the
/// spec-slice filter to the requirement role ONLY. This is the single
/// home of that rule; every coverage-fold check partitions through it.
///
/// `-s`/`-q` are *spec-slice filters*: they cut the words of the spec to
/// select which requirements are in scope to report on. They are applied
/// to the requirement role only — never to the covering pool. Coverers
/// come along transitively: `is_annotation_covered` pairs a coverer with
/// a requirement only when they share an exact `target` (checks/mod.rs),
/// so a coverer quoting an out-of-scope slice of the spec simply never
/// matches an in-scope requirement and falls away on its own — no error.
///
/// This is what keeps a filter honest: it narrows *what you look at*, but
/// can never turn a covered requirement into a miss (or a miss into a
/// pass). A requirement is covered when its coverers tile its full quote;
/// filtering the coverer pool by `-q` could drop one tile of that mosaic
/// and manufacture a false miss. So `in_scope` gates the requirement push
/// below and nothing else.
///
/// `role_of` names every annotation type's role in the calling check
/// (`None` excludes the type from the check entirely); exhaustive matches
/// at the call sites keep the exclusions explicit.
fn partition_check_roles(
    annotations: &AnnotationSet,
    mode: &RequirementMode,
    role_of: impl Fn(AnnotationType) -> Option<CheckRole>,
) -> RolePartition {
    let mut partition = RolePartition::default();
    for annotation in annotations.iter() {
        match role_of(annotation.anno) {
            Some(CheckRole::Requirement) => {
                // Requirement role: the one place the spec-slice filter
                // applies.
                if mode.in_scope(annotation) {
                    partition.requirements.push(annotation.clone());
                }
            }
            Some(CheckRole::Coverer) => partition.coverers.push(annotation.clone()),
            Some(CheckRole::Pending) => partition.pending.push(annotation.clone()),
            None => {}
        }
    }
    partition
}

async fn execute_implementation_check(
    project_data: &ProjectData,
    mode: &RequirementMode,
    verbose: bool,
) -> Result<CheckResult> {
    if verbose {
        progress!("Running implementation annotation coverage check...");
    }

    // Requirements are the spec's own annotations; implementations (plus
    // implications and exceptions) cover them; todos are pending. The
    // spec-slice filter applies to the requirement role only — see
    // `partition_check_roles` for the full rationale.
    let RolePartition {
        requirements: spec_annotations,
        coverers: implemented_annotations,
        pending: todo_annotations,
    } = partition_check_roles(&project_data.annotations, mode, |anno| match anno {
        AnnotationType::Spec => Some(CheckRole::Requirement),
        AnnotationType::Citation | AnnotationType::Implication | AnnotationType::Exception => {
            Some(CheckRole::Coverer)
        }
        AnnotationType::Todo => Some(CheckRole::Pending),
        AnnotationType::Test => None,
    });

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

    // The requirement role here is the implementations being tested (making
    // sure everything is implemented is the implementation check's job);
    // tests cover them. Spec requirements, todos (test-driven development?),
    // implications (fundamentally true or not testable), and exceptions
    // (you don't do it) need no test. The spec-slice filter applies to the
    // requirement role only — see `partition_check_roles` for why filtering
    // the test pool could manufacture a false "not tested".
    let RolePartition {
        requirements: implementation_annotations,
        coverers: test_annotations,
        ..
    } = partition_check_roles(&project_data.annotations, mode, |anno| match anno {
        AnnotationType::Citation => Some(CheckRole::Requirement),
        AnnotationType::Test => Some(CheckRole::Coverer),
        AnnotationType::Spec
        | AnnotationType::Todo
        | AnnotationType::Implication
        | AnnotationType::Exception => None,
    });

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
    /// Positions a prover producer elaborated but which root no
    /// obligation (spec §5.2, Decision 13): delivered as a fact by
    /// `produce`, consumed here only to refine the *report* for
    /// unwitnessed annotations — never the verdict.
    not_proof_testable: Vec<RequestedPosition>,
    classification: ClassificationMap,
    index: SourceIndex,
}

/// Produce every declared source's witnesses, in declaration order (which
/// makes the "first discharging witness" named in verdicts deterministic).
//= design/witness/spec.md#producer
//# Runtime producers MAY ignore the `annotations` argument
//# (their witnesses pre-exist in the artifact).
//# Prover producers use it to construct witnesses
//= design/witness/spec.md#two-pass-construction
//# Prover witnesses are constructed, not found:
//
// So when a prover producer is declared, test files are classified first
// and targets resolved via the verified target resolution — positions,
// not annotations, cross the boundary (§1.7's inertness requirement,
// cited on `RequestedPosition`).
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
                // Unresolvable target: the annotation surfaces through W6.
                //= design/witness/spec.md#claim-rules
                //# An annotation with no resolved target (e.g. a Structural
                //# annotation) binds no ByRootSpan witness —
                //# empty-target containment MUST NOT bind vacuously.
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
    let mut not_proof_testable = Vec::new();
    for source in sources {
        let produced = produce(source, &positions, coverage_path_matches, |file| {
            index.matches_any(file)
        })
        .await?;
        witnesses.extend(produced.witnesses);
        not_proof_testable.extend(produced.not_proof_testable);
    }

    Ok(WitnessLoad {
        witnesses,
        not_proof_testable,
        classification,
        index,
    })
}

/// Fold execution statuses with OR semantics: `Executed` wins outright;
/// among the rest `Unknown` is preferred (it carries a diagnostic line);
/// `NotExecuted` is the base case. Diagnostic detail only, never a
/// verdict: the quantifier properties of design/witness/spec.md §2 are
/// implemented by the verified layer (`duvet_coverage::witness`), not by
/// this fold — callers choose which statuses to feed it purely for
/// report headlines.
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
        not_proof_testable,
        mut classification,
        index,
    } = load;

    // Producer-delivered not-proof-testable positions, keyed for the
    // per-test membership check below.
    let not_proof_testable: HashSet<RequestedPosition> = not_proof_testable.into_iter().collect();

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
    // stream. Surfaced unconditionally because it signals either a mislabeled
    // file or a classifier gap — both need a human, and silence is the bug we
    // are fixing.
    //= design/query/coverage-model-spec.md#scopes
    //# When the stream is unbalanced,
    //# the coverage model MUST NOT score annotations against the collapsed scope tree;
    //# it MUST surface the file as a defeated classification and escalate
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

    // The `executed(X, w)` cell in DIAGNOSTIC form (`executed_status`, which
    // cites spec §1.4), with `Unknown` carrying line detail. Verdicts do not
    // flow through this closure — they are computed by the verified Phase 4
    // quantifier layer through the adapter below; this cell only feeds
    // report detail and the executed-tests-only mode filter.
    let cell = |annotation: &Arc<Annotation>, witness_index: usize| -> ExecutionStatus {
        let path = annotation.source.to_path_buf();
        executed_status(
            annotation,
            classification.get(&path),
            matched[witness_index].get(&path).map(|arc| arc.as_ref()),
        )
    };

    // A test annotation's resolved target, for the ByRootSpan claim arm:
    //= design/witness/spec.md#claim-rules
    //#     ByRootSpan(f, r)   → T's resolved target EXISTS and falls
    //#                           within r in file f
    // None when the file was never classified (it had no coverage and no
    // prover producer is configured), classification was defeated, or the
    // walk found no target - all of which bind no positional witness.
    let resolve = |annotation: &Arc<Annotation>| -> Option<RequestedPosition> {
        let path = annotation.source.to_path_buf();
        let line = resolve_target_line(annotation, classification.get(&path)?)?;
        Some(RequestedPosition {
            absolute_file: index.absolute_of(&path)?.to_string(),
            line,
        })
    };

    // The G1 adapter: witnesses and annotation scoring contexts translated
    // into the verified model's vocabulary (injective file ids, scoring
    // modes). Translation refuses ambiguous root-span path matches
    // (spec §1.5) rather than selecting. All verdicts below flow through it:
    //= design/witness/spec.md#engine-glue
    //# The engine MUST compute every pair,
    //# test, and global verdict (Properties
    //# [W1](#property-w1-same-witness-discharge)–[W4](#property-w4-monotonicity),
    //# [W6](#property-w6-unwitnessed-test-annotations)) by calling the
    //# verified layer's functions
    let adapter = VerifiedVerdicts::build(&witnesses, &matched, &classification, &index)?;

    // The requirement role here is the `Test` annotations being correlated;
    // implementations (plus implications and exceptions) cover them. The
    // spec-slice filter applies to the requirement role only — see
    // `partition_check_roles` for why filtering coverers can manufacture a
    // false miss.
    let RolePartition {
        requirements: test_annotations,
        coverers: implementation_annotations,
        ..
    } = partition_check_roles(&project_data.annotations, mode, |anno| match anno {
        AnnotationType::Test => Some(CheckRole::Requirement),
        AnnotationType::Citation | AnnotationType::Implication | AnnotationType::Exception => {
            Some(CheckRole::Coverer)
        }
        AnnotationType::Spec | AnnotationType::Todo => None,
    });

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
    //
    // Spec W6 quantifies over EVERY test annotation, these included: they
    // fail before reaching the unwitnessed check below, so the boundness
    // fact is computed here (same pure quantifier layer, reporting only —
    // the failure verdict is already decided by the missing implementation).
    let mut missing_implementation: Vec<MissingImplementationTest> = Vec::new();
    for test in &no_coverage {
        if coverage_check_executed_tests_only
            && matches!(
                fold_statuses((0..witnesses.len()).map(|wi| cell(test, wi))),
                ExecutionStatus::NotExecuted
            )
        {
            continue;
        }
        missing_implementation.push(MissingImplementationTest {
            test: test.clone(),
            // Verified W6 verdict through the adapter (reporting refinement
            // here — the failure is already decided by the missing
            // implementation).
            unwitnessed: adapter.is_unwitnessed(test),
        });
    }

    for test in complete_coverage.iter().chain(&incomplete_coverage) {
        //= design/witness/spec.md#claim-rules
        //= type=implementation
        //# A test annotation must find *its own* witness.
        let resolved_target = resolve(&test.target);
        // Verified W2/W6 verdict and the bound-witness detail, both through
        // the adapter (the same verified `binds` cell decides both).
        let test_is_unwitnessed = adapter.is_unwitnessed(&test.target);
        let bound = adapter.bound_witnesses(&test.target);

        if !test_is_unwitnessed {
            // The test is witnessed:
            //= design/witness/spec.md#property-w2-test-execution
            //# The implementation MUST prove that a test annotation is reported
            //# executed if and only if some delivered witness binds it:
            //
            // Evaluate each covering implementation against EVERY bound
            // witness — bound witnesses are never outvoted (decisions.md,
            // Decision 14):
            //= design/witness/spec.md#discharge
            //= type=implementation
            //# witnesses_for(T)  =  { w ∈ delivered : binds(T, w) }
            //#
            //# discharged(T, I)  ⟺  witnesses_for(T) ≠ ∅
            //#                       ∧  ∀w ∈ witnesses_for(T) : executed(I, w)
            let mut executed_implementations = Vec::new();
            let mut not_executed_implementations = Vec::new();

            for annotation in &test.covering_annotations {
                // Verified W1 verdict; the diagnostic closure only feeds the
                // per-witness `status` detail (`Unknown` line numbers).
                let verdict = adapter
                    .discharge_verdict(&test.target, annotation, &bound, |wi| cell(annotation, wi));
                if verdict.discharged {
                    executed_implementations.push(annotation.clone());
                } else {
                    // Headline status for the failing implementation:
                    // fold over the witnesses that did NOT execute it
                    // (preference order: `fold_statuses`).
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
                bound_witnesses: adapter.witness_refs(&bound),
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
            // Diagnostic detail: fold the test's own execution status
            // across ALL witnesses (preference order: `fold_statuses`).
            let diagnostic_status =
                fold_statuses((0..witnesses.len()).map(|wi| cell(&test.target, wi)));

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
                //= design/witness/spec.md#two-pass-construction
                //= type=implementation
                //# the report MUST identify the annotation as
                //# *not proof-testable* ("this position carries no dischargeable
                //# obligation; it can only be witnessed by an execution-style
                //# producer") — a report distinct from Property W6's
                //# "no witness from any configured producer."
                //
                // Reporting refinement only: the annotation is unwitnessed
                // either way (the verdict above is unchanged); this names the
                // producer-delivered reason when there is one.
                not_proof_testable: resolved_target
                    .as_ref()
                    .is_some_and(|target| not_proof_testable.contains(target)),
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
    //
    // Verified W3 verdict through the adapter. The union with discharged
    // implementations is subsumed by W1 ⟹ W3 (a discharged pair's bound
    // witness executed I), kept for report-shape stability.
    let executed_implementations = implementation_annotations
        .iter()
        .filter(|annotation| {
            executed_from_tests.contains(annotation) || adapter.ever_executed(annotation)
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
    // *itself*, so there is no requirement/coverer split (no
    // `partition_check_roles`) and no coverage mosaic to dismantle: the worst a
    // spec-slice filter can do here is not *show* you a duplicate that lies
    // outside the slice — it can never flip a verdict. So the filter is applied
    // uniformly, which is also what "only look at this slice" means for a
    // duplicate report.
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
