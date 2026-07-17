// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    annotation::Annotation,
    query::{engine::ProjectData, result::AnnotationCoverage},
    specification::Specification,
    target::Target,
    text::whitespace,
    Error, Result,
};
use std::{collections::HashMap, sync::Arc};

pub mod coverage;

/// Check if a target annotation is covered by a collection annotations
/// A target annotation is covered,
/// if every part of the target's quote is quoted in in the collection.
/// If the target quote is "MUST foo, MUST bar, MUST baz"
/// And the collection has "MUST foo,"; "MUST bar"; "MUST bar, MUST baz"; "MUST run"
/// then the target is said to be covered
///
/// # Error collection
///
/// This returns `(Option<AnnotationCoverage>, Vec<Error>)` rather than a plain
/// `Result` so that a single `duvet query` run can report *every* unmatchable
/// quote at once instead of aborting on the first. The caller aggregates the
/// `Vec<Error>` across all targets and, if any were collected, fails the run at
/// the end (`duvet query` still exits non-zero — the errors are real). The two
/// error kinds are treated differently:
///   - A failure to locate the *target itself* (missing spec/section, or the
///     target's own quote not found) means there is nothing to compute coverage
///     against, so coverage is `None` and the error is collected.
///   - A *covering* annotation whose quote can't be found is collected and that
///     one coverer is skipped; the remaining coverers are still scored, so the
///     target's `AnnotationCoverage` is still produced.
pub async fn is_annotation_covered(
    target_annotation: &Arc<Annotation>,
    specifications: &Arc<HashMap<Arc<Target>, Arc<Specification>>>,
    annotations: &[Arc<Annotation>],
) -> (Option<AnnotationCoverage>, Vec<Error>) {
    if target_annotation.quote.trim().is_empty() {
        return (
            Some(AnnotationCoverage {
                fully_covered: true,
                target: target_annotation.clone(),
                covering_annotations: Vec::new(),
                covered: Vec::new(),
            }),
            Vec::new(),
        );
    }

    // Get the target and find the specification. Any of these lookups failing is
    // a target-level error: collect it and produce no coverage for this target.
    let target = match target_annotation.target() {
        Ok(target) => target,
        Err(err) => return (None, vec![err]),
    };
    let Some(specification) = specifications.get(&target) else {
        return (
            None,
            vec![duvet_core::error!(
                "Specification not found for target: {}",
                target_annotation.target
            )],
        );
    };
    let Some(target_section) = target_annotation.target_section() else {
        return (
            None,
            vec![duvet_core::error!(
                "No section in annotation target: {}",
                target_annotation.target
            )],
        );
    };
    let Some(section) = specification.section(&target_section) else {
        return (
            None,
            vec![duvet_core::error!(
                "Section '{}' not found in specification: {}",
                target_section,
                target_annotation.target
            )],
        );
    };
    let section_contents = section.view();

    // Normalize the sections for our matching
    let normalize_section_contents = whitespace::normalize(&section_contents);
    let normalized_target_quote = whitespace::normalize(&target_annotation.quote);

    // Get the target range.
    // This will only match the first match.
    // If the specification section has duplicate quotes,
    // then this matching will be unexpected.
    // On the good side, duplicate requirements in a section are confusing,
    // and as long as the specification reads nicely
    // degenerate duplicates like `the` won't work well and are not encouraged.
    let target_start = match normalize_section_contents.find(&normalized_target_quote) {
        Some(start) => start,
        None => {
            return (
                None,
                vec![
                    duvet_core::error!("Exactly matchable quote not found in section")
                    .with_source_slice(target_annotation.original_text.clone(), "Quote")
                    .with_help("This is likely a multiline comment and the second line is missing `-` or other list operator.")
                ],
            );
        }
    };
    let target_end = target_start + normalized_target_quote.len();

    let mut covered = vec![false; normalized_target_quote.len()];
    let mut covering_annotations: Vec<Arc<Annotation>> = Vec::new();
    // Coverer-level errors: a covering quote we couldn't locate. Collected and
    // skipped so the remaining coverers still score this target.
    let mut errors: Vec<Error> = Vec::new();

    for annotation in annotations
        .iter()
        // An annotation can not be covered by itself
        .filter(|annotation| *annotation != target_annotation)
        // An annotation can only be covered by annotations in the same target (section)
        .filter(|annotation| target_annotation.target == annotation.target)
        // Trying to match empty annotations is silly
        .filter(|annotation| !annotation.quote.is_empty())
    {
        let normalized_quote = whitespace::normalize(&annotation.quote);

        // A covering quote can occur more than once in the section
        // (e.g. a short requirement phrase repeated across paragraphs). The bare
        // `find()` anchors to the *first* global occurrence, which may be nowhere
        // near the requirement this covering annotation is meant to cover — so the
        // overlap below would be computed against the wrong span and yield a wrong
        // covered/uncovered verdict. The target's range is the natural anchor, so
        // we pick the occurrence whose range best overlaps `[target_start,
        // target_end)` (ties broken by proximity, then by first occurrence).
        let annotation_start = match best_occurrence(
            &normalize_section_contents,
            &normalized_quote,
            target_start,
            target_end,
        ) {
            Some(start) => start,
            None => {
                // Collect and skip this coverer, don't abort the run.
                errors.push(
                    duvet_core::error!("Exactly matchable quote not found in section")
                    .with_source_slice(annotation.original_text.clone(), "Quote")
                    .with_help("This is likely a multiline comment and the second line is missing `-` or other list operator.")
                );
                continue;
            }
        };
        let annotation_end = annotation_start + normalized_quote.len();

        // Find overlap between the target range and this annotation range
        let overlap_start = std::cmp::max(target_start, annotation_start);
        let overlap_end = std::cmp::min(target_end, annotation_end);

        // If there's an overlap, mark those positions as covered
        if overlap_start < overlap_end {
            for pos in overlap_start..overlap_end {
                let index = pos - target_start;
                if index < covered.len() {
                    covered[index] = true;
                }
            }
            covering_annotations.push(annotation.clone());
        }
    }

    (
        Some(AnnotationCoverage {
            fully_covered: covered.iter().all(|&covered| covered),
            target: target_annotation.clone(),
            covering_annotations,
            covered,
        }),
        errors,
    )
}

/// Find the occurrence of `needle` in `haystack` whose range best matches the
/// target range `[target_start, target_end)`.
///
/// "Best" is: the occurrence with the largest overlap with the target range; on
/// a tie (including the common no-overlap case) the one whose start is closest
/// to `target_start`; on a further tie the earliest occurrence. Returns `None`
/// only when `needle` does not occur in `haystack` at all — matching the
/// contract of the bare `find()` this replaces.
fn best_occurrence(
    haystack: &str,
    needle: &str,
    target_start: usize,
    target_end: usize,
) -> Option<usize> {
    if needle.is_empty() {
        return haystack.find(needle);
    }

    let mut best: Option<(usize, usize, usize)> = None; // (overlap, distance, start)
    let mut search_from = 0;
    while let Some(rel) = haystack[search_from..].find(needle) {
        let start = search_from + rel;
        let end = start + needle.len();

        let overlap = end.min(target_end).saturating_sub(start.max(target_start));
        let distance = start.abs_diff(target_start);

        // Maximize overlap, then minimize distance, then earliest start.
        let candidate = (overlap, distance, start);
        let better = match best {
            None => true,
            Some((bo, bd, _)) => overlap > bo || (overlap == bo && distance < bd),
        };
        if better {
            best = Some(candidate);
        }

        // Advance past this whole occurrence. `start` is a char boundary and
        // `needle.len()` spans complete chars, so `end` is also a boundary —
        // slicing there is panic-safe (a `start + 1` advance could split a
        // multibyte char and panic). We therefore skip self-overlapping matches,
        // which for real requirement phrases do not meaningfully occur.
        search_from = end;
    }

    best.map(|(_, _, start)| start)
}

#[derive(Debug)]
pub struct ClassifiedCoverage {
    pub complete_coverage: Vec<AnnotationCoverage>,
    pub incomplete_coverage: Vec<AnnotationCoverage>,
    pub no_coverage: Vec<Arc<Annotation>>,
    pub mixed_coverage: Vec<AnnotationCoverage>,
    // "Pending" coverage: the requirement is acknowledged but not yet resolved.
    // Today this is exactly the `Todo` bucket; the name leaves room for other
    // kinds of pending (deferred, blocked, etc.) without another rename.
    pub pending_coverage: Vec<AnnotationCoverage>,
}

pub async fn classify_annotation_coverage(
    project_data: &ProjectData,
    annotations: &[Arc<Annotation>],
    // "Satisfied" coverers resolve a requirement one way or another —
    // `Citation` (implemented), `Implication` (already true), or `Exception`
    // (deliberately waived). "Pending" coverers (`Todo`) acknowledge it without
    // resolving it. The distinction is resolved-vs-outstanding, not a type check.
    maybe_satisfied_covering_annotations: &[Arc<Annotation>],
    maybe_pending_covering_annotations: &[Arc<Annotation>],
) -> Result<ClassifiedCoverage> {
    let mut complete_coverage: Vec<AnnotationCoverage> = Vec::new();
    let mut incomplete_coverage: Vec<AnnotationCoverage> = Vec::new();
    let mut no_coverage: Vec<Arc<Annotation>> = Vec::new();
    let mut mixed_coverage: Vec<AnnotationCoverage> = Vec::new();
    let mut pending_coverage: Vec<AnnotationCoverage> = Vec::new();

    // Wrap in Arc to share across futures without cloning the entire Vec per annotation
    let specifications = project_data.specifications.clone();
    let satisfied_annotations = Arc::new(maybe_satisfied_covering_annotations.to_vec());
    let pending_annotations = Arc::new(maybe_pending_covering_annotations.to_vec());

    // Create futures for concurrent coverage calculation
    let coverage_futures: Vec<_> = annotations
        .iter()
        .map(|annotation| {
            let annotation = annotation.clone();
            let specifications = specifications.clone();
            let satisfied_annotations = satisfied_annotations.clone();
            let pending_annotations = pending_annotations.clone();

            async move {
                // Neither call aborts; each returns its own collected
                // errors so a single run reports every unmatchable quote at once.
                let (satisfied_coverage, satisfied_errors) =
                    is_annotation_covered(&annotation, &specifications, &satisfied_annotations)
                        .await;
                let (pending_coverage, pending_errors) =
                    is_annotation_covered(&annotation, &specifications, &pending_annotations).await;

                // A *target-level* failure (coverage is `None`: the target's own
                // quote/section couldn't be located) is produced identically by
                // both calls, since they share the same target — so take it once.
                // *Coverer-level* errors (coverage is `Some`) come from the two
                // distinct covering pools, so keep both.
                let errors = if satisfied_coverage.is_none() {
                    satisfied_errors
                } else {
                    let mut errors = satisfied_errors;
                    errors.extend(pending_errors);
                    errors
                };
                (annotation, satisfied_coverage, pending_coverage, errors)
            }
        })
        .collect();

    // Execute all coverage calculations concurrently. Unlike `try_join_all`,
    // `join_all` does not short-circuit on the first error — every future runs to
    // completion so we can surface all problems together.
    let coverage_results = futures::future::join_all(coverage_futures).await;

    // Collect every per-annotation error across the whole run. If any were
    // found, the run still fails at the end (below) with the aggregate — the
    // errors are real, but the user gets to see all of them in one pass rather
    // than fixing one, re-running, and hitting the next.
    let mut errors: Vec<Error> = Vec::new();

    // Process results sequentially for classification
    for (annotation, satisfied_coverage, pending, annotation_errors) in coverage_results {
        errors.extend(annotation_errors);

        // If either side failed to locate the target itself, there is no coverage
        // to classify for this annotation; its error is already collected above.
        let (Some(satisfied_coverage), Some(pending)) = (satisfied_coverage, pending) else {
            continue;
        };

        let satisfied_len = satisfied_coverage.covering_annotations.len();
        let pending_len = pending.covering_annotations.len();

        match (satisfied_coverage.fully_covered, satisfied_len, pending_len) {
            // Complete satisfied coverage
            (true, _, 0) => complete_coverage.push(satisfied_coverage),
            // Complete satisfied but there is pending coverage. duplicates?
            (true, _, s) if 0 < s => mixed_coverage.push(satisfied_coverage.merge(pending)),
            // Satisfied is missing something
            (false, p, s) if 0 < p && 0 == s => incomplete_coverage.push(satisfied_coverage),
            // Mixed satisfied and pending. duplicates?
            (false, p, s) if 0 < p && 0 < s => {
                mixed_coverage.push(satisfied_coverage.merge(pending))
            }
            // Only pending
            (false, p, s) if 0 == p && 0 < s => pending_coverage.push(pending),
            // Zero coverage
            _ => no_coverage.push(annotation.clone()),
        }
    }

    // Any collected error fails the run, but only after all were
    // gathered — so the user sees every problem in one pass instead of fixing
    // one, re-running, and hitting the next. (Per-annotation duplicate
    // suppression happens above, where the satisfied/pending split is known.)
    if !errors.is_empty() {
        return Err(errors.into());
    }

    Ok(ClassifiedCoverage {
        complete_coverage,
        incomplete_coverage,
        no_coverage,
        mixed_coverage,
        pending_coverage,
    })
}

#[cfg(test)]
mod tests {
    use super::best_occurrence;

    #[test]
    fn absent_needle_returns_none() {
        assert_eq!(best_occurrence("the section text", "missing", 0, 3), None);
    }

    #[test]
    fn single_occurrence_returns_it() {
        let hay = "MUST encrypt the payload";
        let start = hay.find("payload").unwrap();
        assert_eq!(best_occurrence(hay, "payload", 0, 4), Some(start));
    }

    #[test]
    fn picks_occurrence_overlapping_target_not_first() {
        // "encrypt" appears twice; the target range sits on the SECOND one.
        let hay = "encrypt A. Later we encrypt B.";
        let first = hay.find("encrypt").unwrap();
        let second = hay[first + 1..].find("encrypt").unwrap() + first + 1;
        // Target range covers the second occurrence.
        let got = best_occurrence(hay, "encrypt", second, second + 7);
        assert_eq!(got, Some(second), "should anchor to the overlapping copy");
        // Sanity: the naive find() would have returned `first`.
        assert_ne!(got, Some(first));
    }

    #[test]
    fn no_overlap_picks_closest_occurrence() {
        // Three occurrences, none overlaps the target; pick the nearest start.
        let hay = "foo ... foo ... target ... foo";
        let occurrences: Vec<usize> = {
            let mut v = Vec::new();
            let mut from = 0;
            while let Some(rel) = hay[from..].find("foo") {
                let s = from + rel;
                v.push(s);
                from = s + 3;
            }
            v
        };
        let t = hay.find("target").unwrap();
        // Independently compute which occurrence is nearest to the target start.
        let expected = *occurrences
            .iter()
            .min_by_key(|&&s| s.abs_diff(t))
            .unwrap();
        let got = best_occurrence(hay, "foo", t, t + 6).unwrap();
        assert_eq!(got, expected, "nearest occurrence to the target wins");
    }

    #[test]
    fn multibyte_section_does_not_panic() {
        // Non-ASCII before/around the needle: advancing by needle.len() must land
        // on char boundaries.
        let hay = "café — MUST café — donné";
        let got = best_occurrence(hay, "café", 0, 4);
        assert!(got.is_some());
    }
}
