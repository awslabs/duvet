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
        QueryResult, QueryStatus, TestResult, UnwitnessableAnnotation, UnwitnessableKind,
        UnwitnessedTestAnnotation,
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

/// THE filter deciding which annotations get positions requested from
/// prover producers — nothing else ever crosses that boundary.
//= design/witness/spec.md#witnessable-annotations
//= type=implementation
//# Prover producers MUST construct witnesses for `type=test`
//# annotations only
//# (decisions.md, [Decision 6](decisions.md#decision-6);
//# self-discharging implication annotations are a deferred separate
//# feature).
fn witnessable(anno: AnnotationType) -> bool {
    matches!(anno, AnnotationType::Test)
}

/// Produce every declared source's witnesses, in declaration order (which
/// makes the "first discharging witness" named in verdicts deterministic).
//
// So when a prover producer is declared, test files are classified first
// and targets resolved via the verified target resolution — positions,
// not annotations, cross the boundary (§1.7's inertness requirement,
// cited on `RequestedPosition`).
//= design/witness/spec.md#producer
//# Runtime producers MAY ignore the `annotations` argument
//# (their witnesses pre-exist in the artifact).
//# Prover producers use it to construct witnesses
//= design/witness/spec.md#two-pass-construction
//# Prover witnesses are constructed, not found:
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
            .filter(|a| witnessable(a.anno))
            .map(|a| a.source.to_path_buf())
            .collect();
        classification.extend(classify_files(&project_data.annotations, test_files).await?);
        for annotation in project_data
            .annotations
            .iter()
            .filter(|a| witnessable(a.anno))
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

/// The default non-target pattern for Rust sources: attribute lines.
/// Config posture matches per-source comment styles — a `[[source]]`
/// entry's `non-target-pattern` overrides it.
const RUST_NON_TARGET_PATTERN: &str = r"^\s*#\[";

/// Per-source static-placement configuration for the unwitnessable
/// classifier: the ordinary-comment leader derived from the source's
/// configured comment style (the meta pattern minus its trailing `=`:
/// `//=` → `//`, `#=` → `#`, `*=` → `*`), the style's own meta/content
/// prefixes (annotation markup is NOT ordinary prose — a target on an
/// unstamped annotation line is a parse artifact, not a placement
/// verdict), and the non-target pattern (configured, or the
/// per-language default).
struct SourcePlacement {
    comment_leader: Arc<str>,
    style: crate::comment::Pattern,
    non_target: Option<regex::Regex>,
}

/// Build the per-file placement configuration from the project's
/// `[[source]]` entries. Deterministic under multiple matches: entries
/// are visited in sorted order and the first wins.
fn source_placements(
    project_sources: &HashSet<SourceFile>,
) -> Result<HashMap<PathBuf, SourcePlacement>> {
    let mut compiled: HashMap<Arc<str>, regex::Regex> = HashMap::new();
    let mut compile = |pattern: &Arc<str>| -> Result<regex::Regex> {
        if let Some(re) = compiled.get(pattern) {
            return Ok(re.clone());
        }
        let re = regex::Regex::new(pattern)
            .map_err(|err| duvet_core::error!("invalid non-target-pattern {pattern:?}: {err}"))?;
        compiled.insert(pattern.clone(), re.clone());
        Ok(re)
    };

    let mut sorted: Vec<&SourceFile> = project_sources.iter().collect();
    sorted.sort();
    let mut placements = HashMap::new();
    for source in sorted {
        let SourceFile::Text {
            pattern,
            path,
            non_target_pattern,
            ..
        } = source
        else {
            continue;
        };
        if placements.contains_key(&path.to_path_buf()) {
            continue;
        }
        let leader: Arc<str> = pattern
            .meta
            .strip_suffix('=')
            .unwrap_or(&pattern.meta)
            .into();
        let non_target = match non_target_pattern {
            Some(pattern) => Some(compile(pattern)?),
            None => default_non_target(path.as_ref())
                .map(|p| compile(&Arc::from(p)))
                .transpose()?,
        };
        placements.insert(
            path.to_path_buf(),
            SourcePlacement {
                comment_leader: leader,
                style: pattern.clone(),
                non_target,
            },
        );
    }
    Ok(placements)
}

/// The per-language default non-target pattern for files whose
/// `[[source]]` entry configures none.
fn default_non_target(path: &std::path::Path) -> Option<&'static str> {
    match path.extension().and_then(|e| e.to_str()) {
        Some("rs") => Some(RUST_NON_TARGET_PATTERN),
        _ => None,
    }
}

/// The static unwitnessable test (spec §3): does this annotation's
/// resolved target line fail to be code? Placement enforcement for the
/// witness spec's §1.1 placement rule — this classifier is that MUST's
/// enforcing site: a displaced annotation resolves onto the intervening
/// non-code line, and the verdict below names it instead of letting it
/// hide in the run-dependent unwitnessed report.
//= design/witness/spec.md#annotations
//= type=implementation
//# so an annotation
//# (stacked or not) MUST be the last comment block above the code it
//# targets
//= design/witness/spec.md#verdict-output
//= type=implementation
//# Unwitnessable is a placement verdict, not an evidence verdict: it
//# is computed from the source text alone and never consults any
//# witness.
fn classify_unwitnessable(
    annotation: &Arc<Annotation>,
    classification: Option<&FileClassification>,
    placement: Option<&SourcePlacement>,
) -> Option<UnwitnessableAnnotation> {
    let target_line = resolve_target_line(annotation, classification?)?;
    let idx = usize::try_from(target_line).ok()?.checked_sub(1)?;
    let text = annotation
        .original_text
        .file()
        .lines_slices()
        .nth(idx)?
        .to_string();

    // Fallback for sources outside any `[[source]]` entry (deprecated
    // CLI patterns): the default comment style's leader plus the
    // per-language default pattern.
    let default_placement;
    let placement = match placement {
        Some(p) => p,
        None => {
            default_placement = SourcePlacement {
                comment_leader: "//".into(),
                style: crate::comment::Pattern::default(),
                non_target: default_non_target(annotation.source.as_ref()).map(|p| {
                    regex::Regex::new(p).expect("default non-target pattern must compile")
                }),
            };
            &default_placement
        }
    };

    let trimmed = text.trim_start();
    // Annotation markup on the target line is a parse artifact (e.g. a
    // quote-less annotation whose marker lines were not all stamped),
    // not displaced prose: leave the verdict to the run-dependent
    // reports rather than misclassify markup as an ordinary comment.
    if trimmed.starts_with(&*placement.style.meta) || trimmed.starts_with(&*placement.style.content)
    {
        return None;
    }
    let kind = if text.trim().is_empty() {
        UnwitnessableKind::Blank
    } else if !placement.comment_leader.is_empty()
        && trimmed.starts_with(&*placement.comment_leader)
    {
        UnwitnessableKind::Comment
    } else if placement
        .non_target
        .as_ref()
        .is_some_and(|re| re.is_match(&text))
    {
        UnwitnessableKind::NonTargetPattern
    } else {
        return None;
    };

    Some(UnwitnessableAnnotation {
        annotation: annotation.clone(),
        target_line,
        target_text: text,
        kind,
    })
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
    //# (see [Classifier Selection and Dispatch](#dispatch)).
    //= design/query/coverage-model-spec.md#trust-taxonomy
    //= type=implementation
    //# duvet MUST NOT silently substitute the coarse model or score against
    //# the collapsed scope tree; it MUST escalate, reporting each located issue.
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

    // A test annotation's resolved target, for the ByRootSpan claim arm
    // (spec §1.5, quoted at the verified `binds`: the target must EXIST
    // and fall within the root span).
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
    // (spec §1.5) rather than selecting. All verdicts below flow through it
    // (glue obligation G2 — cited at the top of `query/witness.rs`):
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
    let mut unwitnessable: Vec<UnwitnessableAnnotation> = Vec::new();

    // Static-placement inputs for the unwitnessable classifier (spec §3):
    // the per-source comment leader and non-target pattern, plus
    // classifications for annotation-bearing files no witness touched
    // (the witness-matched cache above only covers files a producer
    // delivered maps for). This supplement feeds ONLY the unwitnessable
    // classifier — scoring, binding, and every witness verdict read the
    // original map, unchanged.
    let placements = source_placements(&project_data.project_sources)?;
    let placement_supplement: ClassificationMap = {
        let annotation_paths: HashSet<PathBuf> = test_annotations
            .iter()
            .chain(&implementation_annotations)
            .map(|a| a.source.to_path_buf())
            .filter(|p| !classification.contains_key(p))
            .collect();
        classify_files(&project_data.annotations, annotation_paths).await?
    };
    let placement_classification = |path: &PathBuf| -> Option<&FileClassification> {
        classification
            .get(path)
            .or_else(|| placement_supplement.get(path))
    };

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
            // Evaluate each covering implementation against EVERY bound
            // witness — bound witnesses are never outvoted (decisions.md,
            // Decision 14). Spec §1.6 discharge; the citation lives on the
            // verified `discharged` spec fn in `duvet-coverage`:
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
            // W6's spec text (prose and formula) is owned by the verified
            // `is_unwitnessed` in duvet-coverage — the engine-side copy was
            // removed with the other cross-crate W2/W3/W6 annotations (they
            // structurally cannot be witnessed by a proof-only run; they
            // return with the LCOV mixed-coverage producer).
            //
            // The static split (spec §3): a test whose resolved target line
            // is not code is reported unwitnessable, never unwitnessed —
            // the defect is placement (spec §1.1's placement MUST), not a
            // missing producer, and no run can change it. Checked before
            // the executed-tests-only skip: like Unknown targets, placement
            // errors must surface regardless of which test you're running.
            let path = test.target.source.to_path_buf();
            if let Some(entry) = classify_unwitnessable(
                &test.target,
                placement_classification(&path),
                placements.get(&path),
            ) {
                unwitnessable.push(entry);
                continue;
            }

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
                //# In a mixed run such an annotation binds runtime witnesses
                //# normally.
                //
                // Reporting refinement only: the annotation is unwitnessed
                // either way (the verdict above is unchanged), and the
                // producer's fact is consulted ONLY in this unwitnessed
                // branch — a not-proof-testable position that bound a
                // runtime witness never reaches it, so binding stays
                // normal in mixed runs by construction. This names the
                // producer-delivered reason when there is one.
                not_proof_testable: resolved_target
                    .as_ref()
                    .is_some_and(|target| not_proof_testable.contains(target)),
            });
        }
    }

    // Citation-typed coverers only: implication and exception annotations
    // carry no evidence obligation, so their targets carry no placement
    // obligation either.
    //= design/witness/spec.md#verdict-output
    //= type=implementation
    //# For every implementation annotation whose resolved target line is
    //# unwitnessable, the output MUST report the same finding:
    for annotation in &implementation_annotations {
        if !matches!(annotation.anno, AnnotationType::Citation) {
            continue;
        }
        let path = annotation.source.to_path_buf();
        if let Some(entry) = classify_unwitnessable(
            annotation,
            placement_classification(&path),
            placements.get(&path),
        ) {
            unwitnessable.push(entry);
        }
    }
    // Deterministic report order: by file, then annotation position.
    unwitnessable.sort_by(|a, b| {
        (a.annotation.source.as_ref(), a.annotation.anno_line)
            .cmp(&(b.annotation.source.as_ref(), b.annotation.anno_line))
    });

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

    let status = if failed.is_empty()
        && missing_implementation.is_empty()
        && unwitnessed.is_empty()
        && unwitnessable.is_empty()
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
        unwitnessable,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Executes the actual engine filter (`witnessable`) — the single
    /// predicate deciding which annotations get positions requested from
    /// prover producers — over every annotation type.
    //= design/witness/spec.md#witnessable-annotations
    //= type=test
    //# Prover producers MUST construct witnesses for `type=test`
    //# annotations only
    //# (decisions.md, [Decision 6](decisions.md#decision-6);
    //# self-discharging implication annotations are a deferred separate
    //# feature).
    #[test]
    fn only_test_annotations_are_witnessable() {
        assert!(witnessable(AnnotationType::Test));
        assert!(!witnessable(AnnotationType::Citation));
        assert!(!witnessable(AnnotationType::Spec));
        assert!(!witnessable(AnnotationType::Exception));
        assert!(!witnessable(AnnotationType::Todo));
        // Deferred separate feature — deliberately NOT witnessable today:
        assert!(!witnessable(AnnotationType::Implication));
    }

    mod unwitnessable {
        use super::super::*;
        use crate::{annotation::AnnotationLevel, query::result::UnwitnessableKind};
        use duvet_core::file::SourceFile as CoreSourceFile;
        use duvet_coverage::types::{line_class, LineProperty};

        /// Build an annotation over `contents`, spanning 1-based lines
        /// `start_line..=end_line`, plus the file's degraded
        /// classification exactly as `classify_file` produces it for a
        /// no-classifier file: blank lines `Whitespace`, annotation
        /// lines stamped `Annotation`, everything else unclassified.
        fn fixture(
            path: &str,
            contents: &str,
            start_line: usize,
            end_line: usize,
            anno: AnnotationType,
        ) -> (Arc<Annotation>, FileClassification) {
            let source = CoreSourceFile::new(path, contents).unwrap();
            let line_starts: Vec<usize> = std::iter::once(0)
                .chain(contents.match_indices('\n').map(|(i, _)| i + 1))
                .collect();
            let start = line_starts[start_line - 1];
            let end = line_starts.get(end_line).copied().unwrap_or(contents.len());
            let text = source.substr_range(start..end).unwrap();
            let target = source.substr_range(start..start).unwrap();
            let annotation = Arc::new(Annotation {
                source: source.path().clone(),
                anno_line: start_line,
                original_target: target.clone(),
                original_text: text,
                original_quote: target,
                anno,
                target: "spec.md#s".to_string(),
                quote: String::new(),
                comment: String::new(),
                manifest_dir: source.path().clone(),
                level: AnnotationLevel::Auto,
                format: crate::specification::Format::Auto,
                tracking_issue: String::new(),
                feature: String::new(),
                tags: Default::default(),
                blob_link: None,
            });
            let classifications = contents
                .lines()
                .enumerate()
                .map(|(i, line)| {
                    if (start_line..=end_line).contains(&(i + 1)) {
                        Some(line_class(&[LineProperty::Annotation]))
                    } else if line.trim().is_empty() {
                        Some(line_class(&[LineProperty::Whitespace]))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            let file_length = contents.lines().count() as u64;
            (
                annotation,
                FileClassification::Degraded {
                    classifications,
                    file_length,
                },
            )
        }

        fn rust_placement() -> SourcePlacement {
            SourcePlacement {
                comment_leader: "//".into(),
                style: crate::comment::Pattern::default(),
                non_target: Some(regex::Regex::new(RUST_NON_TARGET_PATTERN).unwrap()),
            }
        }

        /// The classifier is static: no witness, no coverage map, no
        /// producer appears in its inputs — source text and per-source
        /// configuration only.
        #[test]
        //= design/witness/spec.md#verdict-output
        //= type=test
        //# Unwitnessable is a placement verdict, not an evidence verdict: it
        //# is computed from the source text alone and never consults any
        //# witness.
        fn attribute_displaced_target_is_unwitnessable() {
            let contents = "//= spec.md#s\n//= type=test\n//# quoted text\n#[test]\nfn t() {}\n";
            let (annotation, classification) =
                fixture("displaced.rs", contents, 1, 3, AnnotationType::Test);
            let entry =
                classify_unwitnessable(&annotation, Some(&classification), Some(&rust_placement()))
                    .expect("attribute target must classify as unwitnessable");
            assert_eq!(entry.kind, UnwitnessableKind::NonTargetPattern);
            assert_eq!(entry.target_line, 4);
            assert_eq!(entry.target_text, "#[test]");
        }

        #[test]
        fn prose_displaced_target_is_unwitnessable() {
            let contents =
                "//= spec.md#s\n//# quoted text\n/// interleaved doc prose\nfn imp() {}\n";
            let (annotation, classification) =
                fixture("displaced.rs", contents, 1, 2, AnnotationType::Citation);
            let entry =
                classify_unwitnessable(&annotation, Some(&classification), Some(&rust_placement()))
                    .expect("comment target must classify as unwitnessable");
            assert_eq!(entry.kind, UnwitnessableKind::Comment);
            assert_eq!(entry.target_line, 3);
            assert_eq!(entry.target_text, "/// interleaved doc prose");
        }

        /// Defensive arm: real degraded classification skips blank lines
        /// (they can never be resolved targets), but an unclassified
        /// blank-looking line still classifies statically.
        #[test]
        fn blank_unclassified_target_is_unwitnessable() {
            let contents = "//= spec.md#s\n//# quoted text\n   \nfn code() {}\n";
            let (annotation, mut classification) =
                fixture("displaced.rs", contents, 1, 2, AnnotationType::Test);
            // Force the blank line unclassified so it becomes the target.
            if let FileClassification::Degraded {
                classifications, ..
            } = &mut classification
            {
                classifications[2] = None;
            }
            let entry =
                classify_unwitnessable(&annotation, Some(&classification), Some(&rust_placement()))
                    .expect("blank target must classify as unwitnessable");
            assert_eq!(entry.kind, UnwitnessableKind::Blank);
        }

        #[test]
        fn code_target_is_not_unwitnessable() {
            let contents = "//= spec.md#s\n//= type=test\n//# quoted text\nfn t() {}\n";
            let (annotation, classification) =
                fixture("well_placed.rs", contents, 1, 3, AnnotationType::Test);
            assert!(classify_unwitnessable(
                &annotation,
                Some(&classification),
                Some(&rust_placement()),
            )
            .is_none());
        }

        /// Annotation MARKUP on the target line is excluded: a
        /// quote-less annotation's unstamped `//=`/`//#` lines are a
        /// parse artifact, not displaced prose — the verdict stays with
        /// the run-dependent reports (e.g. §5.2 not-proof-testable),
        /// which pin their own wording.
        #[test]
        fn unstamped_annotation_markup_target_is_not_unwitnessable() {
            let contents = "//= spec.md#s\n//= type=test\n//= spec.md#other\nfn t() {}\n";
            // Only lines 1-2 stamped: line 3 is unstamped markup and
            // becomes the resolved target.
            let (annotation, classification) =
                fixture("markup.rs", contents, 1, 2, AnnotationType::Test);
            assert!(classify_unwitnessable(
                &annotation,
                Some(&classification),
                Some(&rust_placement()),
            )
            .is_none());
        }

        /// The non-target pattern is per-source configuration, exactly
        /// like comment styles: a configured pattern replaces the
        /// per-language default.
        #[test]
        fn configured_non_target_pattern_overrides_default() {
            let contents = "#= spec.md#s\n#% quoted text\n@decorator\ndef f(): pass\n";
            let (annotation, classification) =
                fixture("displaced.xyzzy", contents, 1, 2, AnnotationType::Test);
            let placement = SourcePlacement {
                comment_leader: "#".into(),
                style: crate::comment::Pattern {
                    meta: "#=".into(),
                    content: "#%".into(),
                },
                non_target: Some(regex::Regex::new(r"^\s*@").unwrap()),
            };
            let entry =
                classify_unwitnessable(&annotation, Some(&classification), Some(&placement))
                    .expect("configured pattern must match the decorator target");
            assert_eq!(entry.kind, UnwitnessableKind::NonTargetPattern);
            assert_eq!(entry.target_text, "@decorator");
        }
    }
}
