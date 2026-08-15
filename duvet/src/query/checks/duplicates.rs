// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! The duplicates check.
//!
//! Normative semantics: design/duplicates/spec.md. Rationale:
//! design/duplicates/decisions.md. Every rule here is a predicate over one of
//! the two partitions the spec defines — claim classes (annotations sharing a
//! requirement) and target classes (annotations sharing a resolved position).
//! One check evaluates both axes, mirroring the one `[duplicates]` config
//! table.

use crate::{
    annotation::{Annotation, AnnotationType},
    config::{DuplicatesPolicy, FormSet},
    query::{
        checks::{coverage::stamp_annotation_range, is_annotation_covered},
        classify::{classifier_for_path, Classification, DefaultClassifier, LineClassifier},
        engine::ProjectData,
        requirements::RequirementMode,
        result::AnnotationCoverage,
    },
    text::whitespace,
    Error, Result,
};
use duvet_core::path::Path;
use duvet_coverage::{target_resolution::annotation_target, types::AnnotationSpan};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    sync::Arc,
};

/// A resolved target: the source position an annotation resolves to under the
/// coverage model's target resolution, identified as (file, line).
///
//= design/duplicates/spec.md#targets
//# The **resolved target** of an annotation is the source position
//# its annotation block resolves to under the coverage model's target
//# resolution (the classified or the degraded path), identified as
//# the pair (source file, resolved line).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ResolvedTarget {
    pub file: Path,
    pub line: u64,
}

/// The claim of an annotation: its target (spec + section) plus its
/// whitespace-normalized quote.
///
//= design/duplicates/spec.md#claims
//# Two annotations **share a claim** if and only if their target
//# sections are identical and their normalized quotes are identical.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClaimKey {
    pub target: String,
    pub quote: String,
}

impl ClaimKey {
    /// The claim of an annotation, or `None` for a section-level reference.
    ///
    //= design/duplicates/spec.md#claims
    //# An annotation whose normalized quote is empty is a section-level
    //# reference, not a claim: it participates in no claim class and no
    //# rule in [§2](#duplicates-check) applies to it.
    pub fn of(annotation: &Annotation) -> Option<Self> {
        let quote = whitespace::normalize(&annotation.quote);
        if quote.is_empty() {
            return None;
        }
        Some(Self {
            target: annotation.target.clone(),
            quote,
        })
    }
}

/// A duplicate set: the members of one claim class restricted to one form,
/// with the cap that governs it.
#[derive(Debug, PartialEq)]
pub struct DuplicateSet {
    pub form: AnnotationType,
    pub cap: u32,
    pub members: Vec<Arc<Annotation>>,
}

/// A same-claim-same-target stack (spec §2.1).
#[derive(Debug, PartialEq)]
pub struct StackedClaim {
    pub target: ResolvedTarget,
    pub members: Vec<Arc<Annotation>>,
}

/// A claim class whose mixed claim forms are admitted by no member of the
/// claim-form family (spec §2.4).
#[derive(Debug, PartialEq)]
pub struct ExclusivityViolation {
    pub forms: FormSet,
    pub members: Vec<Arc<Annotation>>,
}

/// The duplicates check's analysis (spec §2).
#[derive(Debug, Default)]
pub struct DuplicatesAnalysis {
    /// §2.1 violations: same claim, same resolved target — any forms.
    pub stacked: Vec<StackedClaim>,
    /// §2.2 violations: duplicate sets over their cap.
    pub over_cap: Vec<DuplicateSet>,
    /// §2.2 surfaced multiplicity: duplicate sets of size > 1 within cap.
    pub allowed_sets: Vec<DuplicateSet>,
    /// §2.3 violations: claims fully covered by same-form claims outside
    /// their class.
    pub subsumed: Vec<AnnotationCoverage>,
    /// §2.4 violations: free forms sharing a claim.
    pub exclusivity: Vec<ExclusivityViolation>,
    /// §2.5: partial overlap, reported and never failed.
    pub some_overlap: Vec<AnnotationCoverage>,
    /// Annotations with no duplicate relationship at all (verbose display).
    pub unique: Vec<(AnnotationType, Vec<Arc<Annotation>>)>,
    /// The target axis (spec §3): fan-in listing and its bounds. One check
    /// evaluates both axes, mirroring the one `[duplicates]` config table.
    pub targets: DuplicateTargetsAnalysis,
}

impl DuplicatesAnalysis {
    pub fn passes(&self) -> bool {
        //= design/duplicates/spec.md#duplicates-verdict
        //# The duplicates check MUST fail if and only if at least one rule of
        //# [§2.1](#same-claim-same-target)–[§2.4](#exclusivity) or of the
        //# target axis ([§3.2](#fan-in-bounds)–[§3.3](#type-combinations))
        //# fires.
        //
        //= design/duplicates/spec.md#partial-overlap
        //# Verdict: a partial overlap MUST NOT fail the duplicates check,
        //# matching the historical check.
        self.stacked.is_empty()
            && self.over_cap.is_empty()
            && self.subsumed.is_empty()
            && self.exclusivity.is_empty()
            && self.targets.passes()
    }
}

/// One entry of the fan-in listing: a target class of size ≥ 2.
#[derive(Debug)]
pub struct TargetClass {
    pub target: ResolvedTarget,
    pub members: Vec<Arc<Annotation>>,
    /// Per-form member counts.
    pub forms: BTreeMap<AnnotationType, usize>,
    /// Distinct (spec, section) targets claimed by members.
    pub sections: usize,
}

impl TargetClass {
    pub fn count(&self) -> usize {
        self.members.len()
    }

    pub fn form_set(&self) -> FormSet {
        self.forms.keys().copied().collect()
    }
}

/// The target-axis analysis of the duplicates check (spec §3).
#[derive(Debug, Default)]
pub struct DuplicateTargetsAnalysis {
    /// §3.1: every target class of size ≥ 2, sorted by count descending.
    pub listing: Vec<TargetClass>,
    /// Indices into `listing` violating the `count` bound (§3.2).
    pub count_violations: Vec<usize>,
    /// Indices into `listing` violating the `sections` bound (§3.2).
    pub sections_violations: Vec<usize>,
    /// Indices into `listing` violating the form-combination rule (§3.3).
    pub type_violations: Vec<usize>,
}

impl DuplicateTargetsAnalysis {
    pub fn passes(&self) -> bool {
        self.count_violations.is_empty()
            && self.sections_violations.is_empty()
            && self.type_violations.is_empty()
    }
}

/// Resolve the target of every annotation, statically: classification only,
/// no coverage data. Files with a language classifier use its classification;
/// files without one use the minimal universal classification (the degraded
/// path's input). Both feed the same verified forward walk.
///
/// `declaration_sources` is the set of TOML requirement-artifact paths
/// (extracted requirements, declared exceptions and todos — the project's
/// `SourceFile::Toml` entries). Annotations from those files are declarations
/// about requirements, not placements in source, and are excluded from
/// target resolution by provenance — never by file extension, which would
/// also exempt genuine comment annotations in scanned TOML sources.
///
//= design/duplicates/spec.md#targets
//# An annotation whose resolution yields no line (the forward walk
//# finds nothing below it to target) participates in no target
//# class.
pub async fn resolve_targets(
    annotations: &[Arc<Annotation>],
    declaration_sources: &HashSet<&Path>,
) -> Result<(HashMap<Arc<Annotation>, ResolvedTarget>, Vec<Error>)> {
    let mut by_file: BTreeMap<Path, Vec<Arc<Annotation>>> = BTreeMap::new();
    for annotation in annotations {
        //= design/duplicates/spec.md#targets
        //# An annotation declared in a requirement artifact (an extracted
        //# requirement, or an exception or todo declared in TOML) is a
        //# statement about a requirement, not a placement in source: it MUST
        //# NOT participate in any target class.
        if declaration_sources.contains(&annotation.source) {
            continue;
        }
        by_file
            .entry(annotation.source.clone())
            .or_default()
            .push(annotation.clone());
    }

    let mut resolved = HashMap::new();
    let mut errors: Vec<Error> = Vec::new();

    for (file, file_annotations) in by_file {
        let source = duvet_core::vfs::read_string(&file).await?;
        let content = source.to_string();
        let line_count = content.lines().count() as u64;

        // Classified when a language classifier exists and parses the file;
        // otherwise the minimal universal classification.
        let classification = match classifier_for_path(&file) {
            Some(classifier) => classifier.classify(&content),
            None => DefaultClassifier.classify(&content),
        };
        let mut classifications = match classification {
            Classification::Classified(c) => c,
            //= design/duplicates/spec.md#targets
            //# When any file bearing annotations defeats
            //# classification, the duplicates check MUST fail the run, reporting
            //# each located classifier issue, and render no verdict.
            Classification::Unclassifiable { first, rest } => {
                let lines: Vec<String> = core::iter::once(&first)
                    .chain(rest.iter())
                    .map(|issue| issue.line.to_string())
                    .collect();
                errors.push(duvet_core::error!(
                    "{}: the selected classifier could not produce a trustworthy \
                     classification (issue(s) at line(s) {}). The file's annotation \
                     targets are unknowable, so the duplicates check renders no \
                     verdict. Fix the file, or report the classifier gap.",
                    file.display(),
                    lines.join(", ")
                ));
                continue;
            }
        };

        // Duvet's parsed annotation ranges are authoritative over the
        // classifier's heuristic prefix detection — same override the
        // coverage model applies before resolution.
        for annotation in &file_annotations {
            stamp_annotation_range(&mut classifications, annotation.line_range());
        }

        for annotation in file_annotations {
            let (start_line, end_line) = annotation.line_range();
            // `end_line` is a line tally; u64::MAX lines is unreachable.
            debug_assert!(end_line < u64::MAX);
            let span = AnnotationSpan {
                start_line,
                end_line,
            };
            if let Some(target) = annotation_target(&span, &classifications, line_count) {
                resolved.insert(
                    annotation,
                    ResolvedTarget {
                        file: file.clone(),
                        line: target.line_number,
                    },
                );
            }
        }
    }

    Ok((resolved, errors))
}

/// Group annotations into claim classes (spec §1.2). Section-level references
/// (empty normalized quote) participate in no class.
fn claim_classes(annotations: &[Arc<Annotation>]) -> BTreeMap<ClaimKey, Vec<Arc<Annotation>>> {
    let mut classes: BTreeMap<ClaimKey, Vec<Arc<Annotation>>> = BTreeMap::new();
    for annotation in annotations {
        if let Some(key) = ClaimKey::of(annotation) {
            classes.entry(key).or_default().push(annotation.clone());
        }
    }
    classes
}

/// Run the duplicates check (spec §2) over the in-scope annotations.
///
/// Returns the analysis plus every quote-location error collected along the
/// way; the caller aggregates and fails the run if any were found, after all
/// were gathered.
pub async fn analyze_duplicates(
    project_data: &ProjectData,
    mode: &RequirementMode,
) -> Result<(DuplicatesAnalysis, Vec<Error>)> {
    let annotations: Vec<Arc<Annotation>> = project_data
        .annotations
        .iter()
        .filter(|annotation| mode.in_scope(annotation))
        .cloned()
        .collect();

    let policy = &project_data.duplicates_policy;
    let classes = claim_classes(&annotations);

    // Provenance, not extension: the project's `SourceFile::Toml` entries are
    // the requirement artifacts whose annotations never join target classes.
    let declaration_sources: HashSet<&Path> = project_data
        .project_sources
        .iter()
        .filter_map(|source| match source {
            crate::source::SourceFile::Toml(path) => Some(path),
            crate::source::SourceFile::Text { .. } => None,
        })
        .collect();
    let (targets, defeat_errors) = resolve_targets(&annotations, &declaration_sources).await?;

    let mut analysis = DuplicatesAnalysis {
        targets: analyze_target_axis(&annotations, &targets, policy),
        ..Default::default()
    };

    analyze_claim_axis(&classes, &targets, policy, &mut analysis);

    // §2.3 — subsumption: a claim fully covered by same-form claims outside
    // its own class. Reuses the engine's coverage relation with the coverer
    // pool restricted to same-form annotations whose claim differs, so a
    // claim-class twin never counts as a coverer (twins are the cap's
    // business, §2.2). Partial overlap surfaces here too (§2.5).
    //
    // Seed with the defeat errors so every problem surfaces in one pass;
    // any gathered error fails the run (execute_duplicates).
    let mut errors: Vec<Error> = defeat_errors;
    let mut unique_by_form: BTreeMap<AnnotationType, Vec<Arc<Annotation>>> = BTreeMap::new();

    let mut by_form: BTreeMap<AnnotationType, Vec<Arc<Annotation>>> = BTreeMap::new();
    for annotation in &annotations {
        by_form
            .entry(annotation.anno)
            .or_default()
            .push(annotation.clone());
    }

    for (form, form_annotations) in &by_form {
        // Precompute each annotation's claim key once. `ClaimKey::of` is a
        // pure normalization, so hoisting it out of the pair loop is
        // behavior-preserving; recomputing it per (annotation, other) pair
        // was O(n²) normalizations per form.
        let keyed: Vec<(&Arc<Annotation>, Option<ClaimKey>)> = form_annotations
            .iter()
            .map(|annotation| (annotation, ClaimKey::of(annotation)))
            .collect();

        // Index the form's annotations by target section. Coverage never
        // crosses sections — `is_annotation_covered` admits coverers only
        // from the target's own section (its second filter) — so a
        // cross-section pool member can contribute neither coverage nor a
        // coverer-level error, and restricting each pool to its section
        // group is behavior-preserving. This turns pool construction from
        // O(n²) Arc clones per form into O(Σ n_s²) over sections, the shape
        // the matching work already had. Iteration order (and therefore
        // report order) is unchanged: the target loop below still walks
        // `keyed` in the form's original order.
        type SectionGroup<'a> = Vec<(&'a Arc<Annotation>, &'a Option<ClaimKey>)>;
        let mut by_section: BTreeMap<&str, SectionGroup> = BTreeMap::new();
        for (annotation, key) in &keyed {
            by_section
                .entry(annotation.target.as_str())
                .or_default()
                .push((annotation, key));
        }

        for (annotation, annotation_key) in &keyed {
            let Some(key) = annotation_key else {
                continue;
            };
            let pool: Vec<Arc<Annotation>> = by_section[annotation.target.as_str()]
                .iter()
                .filter(|(_, other_key)| other_key.as_ref() != Some(key))
                .map(|(other, _)| Arc::clone(other))
                .collect();

            let (coverage, coverage_errors) =
                is_annotation_covered(annotation, &project_data.specifications, &pool).await;
            errors.extend(coverage_errors);

            let Some(coverage) = coverage else {
                continue;
            };

            if coverage.fully_covered {
                //= design/duplicates/spec.md#subsumption
                //# An annotation whose claim is fully covered
                //# ([§1.3](#claim-coverage)) by the claims of same-form annotations
                //# outside its own claim class MUST fail the duplicates check,
                //# regardless of any configured cap.
                analysis.subsumed.push(coverage);
            } else if !coverage.covering_annotations.is_empty() {
                //= design/duplicates/spec.md#partial-overlap
                //# Content: the check MUST identify every partial-overlap pair in
                //# its analysis.
                analysis.some_overlap.push(coverage);
            } else {
                unique_by_form
                    .entry(*form)
                    .or_default()
                    .push(Arc::clone(annotation));
            }
        }
    }

    analysis.unique = unique_by_form.into_iter().collect();

    Ok((analysis, errors))
}

/// The pure claim-axis rules over precomputed claim classes and target
/// resolution: §2.1 stacking, §2.2 caps, §2.4 the claim-form family.
/// §2.3 (subsumption) needs the engine's coverage relation and stays in
/// `analyze_duplicates`.
///
/// Pure — no I/O — so its properties are machine-checked below (see the
/// `properties` test module): section-level references never enter
/// `classes` (`ClaimKey::of` is the gate), so every §2 population this
/// function fills is invariant under adding or removing them.
fn analyze_claim_axis(
    classes: &BTreeMap<ClaimKey, Vec<Arc<Annotation>>>,
    targets: &HashMap<Arc<Annotation>, ResolvedTarget>,
    policy: &DuplicatesPolicy,
    analysis: &mut DuplicatesAnalysis,
) {
    for members in classes.values() {
        // §2.1 — same claim, same resolved target: always fails, every form
        // pair, cap-independent.
        //
        //= design/duplicates/spec.md#same-claim-same-target
        //# Two annotations that share a claim and share a resolved target
        //# MUST fail the duplicates check, regardless of their claim forms
        //# and regardless of any configured cap.
        let mut by_target: BTreeMap<&ResolvedTarget, Vec<Arc<Annotation>>> = BTreeMap::new();
        for member in members {
            if let Some(target) = targets.get(member) {
                by_target.entry(target).or_default().push(member.clone());
            }
        }
        for (target, stack) in by_target {
            if stack.len() > 1 {
                // Renderer contract (result.rs): a stacked class has >= 2
                // members — it split_first()s unconditionally.
                debug_assert!(stack.len() >= 2);
                analysis.stacked.push(StackedClaim {
                    target: target.clone(),
                    members: stack,
                });
            }
        }

        // §2.2 — multiplicity caps per duplicate set.
        //
        //= design/duplicates/spec.md#caps
        //# A duplicate set whose size exceeds its form's cap MUST fail the
        //# duplicates check.
        let mut by_form: BTreeMap<AnnotationType, Vec<Arc<Annotation>>> = BTreeMap::new();
        for member in members {
            by_form.entry(member.anno).or_default().push(member.clone());
        }
        for (form, set_members) in &by_form {
            let cap = policy.claims.cap(*form);
            let set = DuplicateSet {
                form: *form,
                cap,
                members: set_members.clone(),
            };
            if set_members.len() as u64 > cap as u64 {
                // Renderer contract (result.rs): an over-cap set is
                // non-empty (len > cap >= 1) — it split_first()s
                // unconditionally.
                debug_assert!(!set.members.is_empty());
                analysis.over_cap.push(set);
            } else if set_members.len() > 1 {
                //= design/duplicates/spec.md#caps
                //# A duplicate set within its cap MUST pass this rule
                //# ([Decision 5](decisions.md#decision-5)), and when its size exceeds
                //# one it MUST be reported with its size and every member's location,
                //# so multiplicity is always surfaced, never silent.
                analysis.allowed_sets.push(set);
            }
        }

        // §2.4 — the claim-form family over the class's non-spec members.
        //
        //= design/duplicates/spec.md#exclusivity
        //# A claim class whose non-`spec` members bear two or more distinct
        //# claim forms MUST fail the duplicates check unless the set of
        //# non-`spec` claim forms is admitted by the configured claim-form
        //# family ([§4.2](#schema-claims)).
        let non_spec: Vec<Arc<Annotation>> = members
            .iter()
            .filter(|member| member.anno != AnnotationType::Spec)
            .cloned()
            .collect();
        //= design/duplicates/spec.md#exclusivity
        //# A claim class whose non-`spec`
        //# members bear a single claim form MUST NOT fail this rule: copies
        //# of one form are the caps' business ([§2.2](#caps)).
        let forms: FormSet = non_spec.iter().map(|member| member.anno).collect();
        if forms.len() >= 2 && !policy.claims.admits(&forms) {
            // Renderer contract (result.rs): >= 2 distinct forms implies
            // >= 2 members — it split_first()s unconditionally.
            debug_assert!(non_spec.len() >= 2);
            analysis.exclusivity.push(ExclusivityViolation {
                forms,
                members: non_spec,
            });
        }
    }
}

/// The target axis (spec §3) over the in-scope annotations and their
/// precomputed resolution.
fn analyze_target_axis(
    annotations: &[Arc<Annotation>],
    targets: &HashMap<Arc<Annotation>, ResolvedTarget>,
    policy: &DuplicatesPolicy,
) -> DuplicateTargetsAnalysis {
    let mut by_target: BTreeMap<ResolvedTarget, Vec<Arc<Annotation>>> = BTreeMap::new();
    for annotation in annotations {
        if let Some(target) = targets.get(annotation) {
            by_target
                .entry(target.clone())
                .or_default()
                .push(annotation.clone());
        }
    }

    //= design/duplicates/spec.md#fan-in-listing
    //# The duplicates check's analysis MUST include the fan-in listing:
    //# it MUST contain a target if and only if two or more annotations
    //# resolve to it.
    let mut listing: Vec<TargetClass> = by_target
        .into_iter()
        .filter(|(_, members)| members.len() >= 2)
        .map(|(target, members)| {
            let mut forms: BTreeMap<AnnotationType, usize> = BTreeMap::new();
            let mut sections: BTreeSet<&str> = BTreeSet::new();
            for member in &members {
                *forms.entry(member.anno).or_default() += 1;
                sections.insert(member.target.as_str());
            }
            TargetClass {
                target,
                sections: sections.len(),
                forms,
                members,
            }
        })
        .collect();

    //= design/duplicates/spec.md#fan-in-listing
    //# Each listed target MUST report its exact annotation count, its
    //# per-form breakdown, and its distinct-section count, and the
    //# listing MUST be sorted by annotation count descending.
    listing.sort_by(|a, b| {
        b.count()
            .cmp(&a.count())
            .then_with(|| a.target.cmp(&b.target))
    });

    let mut analysis = DuplicateTargetsAnalysis {
        listing,
        ..Default::default()
    };

    for (index, class) in analysis.listing.iter().enumerate() {
        //= design/duplicates/spec.md#fan-in-bounds
        //# When a bound is configured, a target class exceeding it MUST fail
        //# the duplicates check.
        if let Some(count) = policy.targets.count {
            if class.count() as u64 > count {
                analysis.count_violations.push(index);
            }
        }
        if let Some(sections) = policy.targets.sections {
            if class.sections as u64 > sections {
                analysis.sections_violations.push(index);
            }
        }

        //= design/duplicates/spec.md#type-combinations
        //# A target class bearing two or more distinct claim forms MUST fail
        //# the duplicates check unless its form set is a subset of some
        //# member of the family.
        let form_set = class.form_set();
        //= design/duplicates/spec.md#type-combinations
        //# A target class bearing a single claim form
        //# MUST NOT fail this rule.
        if form_set.len() >= 2 && !policy.targets.admits(&form_set) {
            analysis.type_violations.push(index);
        }
    }

    analysis
}

#[cfg(test)]
mod properties {
    use super::*;
    use crate::{annotation::AnnotationLevel, config::ClaimsPolicy};
    use bolero::check;

    /// A synthetic annotation with the semantic fields the claim axis reads
    /// (form, target, quote) and a unique identity (`anno_line`).
    fn annotation(id: usize, form: AnnotationType, target: &str, quote: &str) -> Arc<Annotation> {
        use duvet_core::file::SourceFile as CoreSourceFile;
        let contents = "//= spec#s\ncode();\n";
        let source = CoreSourceFile::new("test/synthetic.rs", contents).unwrap();
        let text = source.substr_range(0..10).unwrap();
        let original_target = source.substr_range(4..10).unwrap();
        let original_quote = source.substr_range(11..18).unwrap();
        Arc::new(Annotation {
            source: source.path().clone(),
            anno_line: id,
            original_target,
            original_text: text,
            original_quote,
            anno: form,
            target: target.to_string(),
            quote: quote.to_string(),
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

    const FORMS: [AnnotationType; 6] = [
        AnnotationType::Spec,
        AnnotationType::Test,
        AnnotationType::Citation,
        AnnotationType::Exception,
        AnnotationType::Todo,
        AnnotationType::Implication,
    ];
    // Includes the empty and the whitespace-only quote: both normalize to
    // empty, so both are section-level references under §1.2.
    const QUOTES: [&str; 5] = ["", "   ", "aaa", "bbb", "aaa bbb"];

    /// Section-level references are inert on the claim axis: adding or
    /// removing annotations whose normalized quote is empty changes no
    /// claim class and no §2.1/§2.2/§2.4 population. `ClaimKey::of` is the
    /// gate; this checks it end-to-end through the pure claim axis, with
    /// the references' target resolutions present in both runs — so the
    /// axis provably-by-test never consults a non-member's resolution.
    #[test]
    fn section_level_references_are_inert_on_the_claim_axis() {
        //= design/duplicates/spec.md#claims
        //= type=test
        //# An annotation whose normalized quote is empty is a section-level
        //# reference, not a claim: it participates in no claim class and no
        //# rule in [§2](#duplicates-check) applies to it.
        check!()
            .with_type::<(u8, Vec<(u8, u8, u8, u8, bool)>)>()
            .for_each(|(caps, items)| {
                let policy = DuplicatesPolicy {
                    claims: ClaimsPolicy {
                        test: u32::from(caps % 3) + 1,
                        implementation: u32::from((caps >> 2) % 3) + 1,
                        types: None,
                    },
                    ..Default::default()
                };

                let mut all: Vec<Arc<Annotation>> = Vec::new();
                let mut targets: HashMap<Arc<Annotation>, ResolvedTarget> = HashMap::new();
                for (id, (form, section, quote, line, resolved)) in items.iter().enumerate() {
                    let ann = annotation(
                        id,
                        FORMS[usize::from(form % 6)],
                        &format!("spec.md#section-{}", section % 3),
                        QUOTES[usize::from(quote % 5)],
                    );
                    if *resolved {
                        targets.insert(
                            ann.clone(),
                            ResolvedTarget {
                                file: duvet_core::path::Path::from("src/lib.rs"),
                                line: u64::from(line % 4),
                            },
                        );
                    }
                    all.push(ann);
                }
                // Partition by the SPEC's definition (normalized-quote
                // emptiness), never by `ClaimKey::of` — deriving the
                // partition from the gate under test would make the whole
                // property circular and vacuously green.
                let kept: Vec<Arc<Annotation>> = all
                    .iter()
                    .filter(|ann| !whitespace::normalize(&ann.quote).is_empty())
                    .cloned()
                    .collect();

                // The gate: claim classes are identical with and without the
                // section-level references.
                let classes_all = claim_classes(&all);
                let classes_kept = claim_classes(&kept);
                assert_eq!(classes_all, classes_kept);

                // End-to-end through the pure axis, same resolution map for
                // both runs: identical §2 populations.
                let mut with_refs = DuplicatesAnalysis::default();
                let mut without_refs = DuplicatesAnalysis::default();
                analyze_claim_axis(&classes_all, &targets, &policy, &mut with_refs);
                analyze_claim_axis(&classes_kept, &targets, &policy, &mut without_refs);
                assert_eq!(with_refs.stacked, without_refs.stacked);
                assert_eq!(with_refs.over_cap, without_refs.over_cap);
                assert_eq!(with_refs.allowed_sets, without_refs.allowed_sets);
                assert_eq!(with_refs.exclusivity, without_refs.exclusivity);

                // And no population member is a section-level reference.
                let members = with_refs
                    .stacked
                    .iter()
                    .map(|stack| &stack.members)
                    .chain(with_refs.over_cap.iter().map(|set| &set.members))
                    .chain(with_refs.allowed_sets.iter().map(|set| &set.members))
                    .chain(
                        with_refs
                            .exclusivity
                            .iter()
                            .map(|violation| &violation.members),
                    )
                    .flatten();
                for member in members {
                    assert!(!whitespace::normalize(&member.quote).is_empty());
                }
            });
    }

    /// §1.2's definition of claim sharing, checked at its quantifier: two
    /// annotations share a claim (equal `ClaimKey`s) exactly when their
    /// target sections are equal and their whitespace-normalized quotes are
    /// equal — across all forms, and independent of everything else.
    #[test]
    fn claim_sharing_is_equality_of_section_and_normalized_quote() {
        //= design/duplicates/spec.md#claims
        //= type=test
        //# Two annotations **share a claim** if and only if their target
        //# sections are identical and their normalized quotes are identical.
        check!().with_type::<(u8, u8, u8, u8, u8, u8)>().for_each(
            |(form_a, section_a, quote_a, form_b, section_b, quote_b)| {
                let a = annotation(
                    0,
                    FORMS[usize::from(form_a % 6)],
                    &format!("spec.md#section-{}", section_a % 3),
                    QUOTES[usize::from(quote_a % 5)],
                );
                let b = annotation(
                    1,
                    FORMS[usize::from(form_b % 6)],
                    &format!("spec.md#section-{}", section_b % 3),
                    QUOTES[usize::from(quote_b % 5)],
                );
                let (key_a, key_b) = (ClaimKey::of(&a), ClaimKey::of(&b));
                let share = match (&key_a, &key_b) {
                    (Some(key_a), Some(key_b)) => key_a == key_b,
                    // A section-level reference shares a claim with nothing.
                    _ => false,
                };
                let definition = a.target == b.target
                    && whitespace::normalize(&a.quote) == whitespace::normalize(&b.quote)
                    // §1.2: an empty normalized quote is not a claim at all.
                    && !whitespace::normalize(&a.quote).is_empty();
                assert_eq!(share, definition);
            },
        );
    }

    /// §1.4: an annotation whose resolution yields no line joins no target
    /// class — it appears in no fan-in listing entry and can violate no
    /// target-axis rule, whatever the bounds.
    #[test]
    fn unresolved_annotations_join_no_target_class() {
        //= design/duplicates/spec.md#targets
        //= type=test
        //# An annotation whose resolution yields no line (the forward walk
        //# finds nothing below it to target) participates in no target
        //# class.
        let resolved_a = annotation(0, AnnotationType::Test, "spec.md#section-1", "aaa");
        let resolved_b = annotation(1, AnnotationType::Citation, "spec.md#section-2", "bbb");
        let unresolved = annotation(2, AnnotationType::Citation, "spec.md#section-1", "ccc");

        let mut targets = HashMap::new();
        let shared = ResolvedTarget {
            file: duvet_core::path::Path::from("src/lib.rs"),
            line: 7,
        };
        targets.insert(resolved_a.clone(), shared.clone());
        targets.insert(resolved_b.clone(), shared);
        // `unresolved` is deliberately absent: the forward walk found no line.

        // The tightest possible bounds, so membership would be a violation.
        let policy = DuplicatesPolicy {
            targets: crate::config::TargetsPolicy {
                count: Some(1),
                sections: Some(1),
                types: None,
            },
            ..Default::default()
        };
        let all = vec![resolved_a, resolved_b, unresolved.clone()];
        let analysis = analyze_target_axis(&all, &targets, &policy);

        for class in &analysis.listing {
            assert!(!class.members.contains(&unresolved));
        }
        // The resolved pair still forms its class and trips the bounds —
        // the exemption is the unresolved annotation's, not the tree's.
        assert_eq!(analysis.listing.len(), 1);
        assert_eq!(analysis.listing[0].count(), 2);
        assert!(!analysis.count_violations.is_empty());
    }

    /// The §2.3 side of the same gate: a whitespace-only quote normalizes to
    /// empty, so `ClaimKey::of` yields no claim and the subsumption loop's
    /// target-side skip (`let Some(key) = … else continue`) applies.
    #[test]
    fn whitespace_only_quote_is_a_section_level_reference() {
        let ann = annotation(0, AnnotationType::Citation, "spec.md#section-1", " \t ");
        assert_eq!(ClaimKey::of(&ann), None);
        assert!(whitespace::normalize(&ann.quote).is_empty());
    }
}
