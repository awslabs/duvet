// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Multi-report merge for v2 JSON reports.
//!
//! # Strategy
//!
//! v2 entity IDs are deterministic FNV-1a 64-bit hashes of structurally
//! significant fields (see `crate::ids` for the per-entity hash schema).
//! Equal IDs across inputs therefore imply that the *hashed* fields are
//! equal — modulo the ~1-in-2^32 birthday-bound collision probability the
//! `ids.rs` module documents.
//!
//! Given that, merging N reports collapses to a per-bucket [`BTreeMap`]
//! union keyed by ID. For each ID seen in more than one input we then need
//! to reconcile *un-hashed* fields: those are not implied by ID equality,
//! so two inputs can disagree.
//!
//! # Field classification
//!
//! Each entity field falls into one of three categories:
//!
//! 1. **Hash-input** — included in the FNV-1a key. ID equality implies field
//!    equality, so any mismatch is a hash collision (or a producer bug). We
//!    treat these as hard errors — silently dropping one side could merge
//!    structurally distinct entities. See `ids.rs`: "a merge tool should
//!    detect and handle collisions".
//!
//! 2. **Structural conflict** — outside the hash, but we still require
//!    agreement (e.g. spec `title`, requirement `level`, cite `anno_type`).
//!    A mismatch indicates the producers disagree on the entity's meaning;
//!    refusing to merge protects downstream consumers from silently averaged
//!    semantics.
//!
//! 3. **Soft drift** — outside the hash and tolerated. Cite metadata
//!    (`comment`, `feature`, `tracking_issue`, `tags`) often differs across
//!    branches/checkouts and isn't load-bearing for traceability. When two
//!    inputs disagree we reconcile deterministically: lexicographically
//!    smaller wins for the `Option<String>` fields, and `tags` is the
//!    sorted union of both sets. The deterministic tie-break keeps the
//!    binary merge commutative and associative, so an N-way fold is
//!    order-independent. Drift is logged as a warning so it stays
//!    observable.
//!
//! Coverage on requirements is a fourth, special case: it's a *set* keyed
//! by cite ID, so we merge by union and post-process with sort+dedup so the
//! merged report is canonical regardless of input order.

use crate::report::json_v2::{
    AnnotationsV2, CiteAnnotation, ReportV2, Repository, RequirementAnnotation, SectionAnnotation,
    SourcesV2, SpecificationAnnotation,
};
use std::collections::BTreeMap;

/// Merge `reports` into a single `ReportV2`.
///
/// Returns an error when:
/// * `reports` is empty.
/// * Any input report has `version != "2.0"`.
/// * Two inputs disagree on a structurally meaningful field for the same
///   entity ID (e.g. spec annotation title, requirement level, cite
///   `anno_type`/`level`/`target`, or extension key value).
/// * A hash-input invariant fails — i.e. two inputs share an ID but differ
///   on a field that goes into that ID's hash. This is either a hash
///   collision or a producer bug; either way we refuse to silently merge.
pub fn merge_reports(reports: Vec<ReportV2>) -> crate::Result<ReportV2> {
    if reports.is_empty() {
        return Err(duvet_core::error!(
            "merge requires at least one input report"
        ));
    }

    // Linked sources without a `repository` collapse to a `lnk-` ID hashed
    // from `file_name` alone. Within a single report that's fine — paths
    // are unique by construction. The hazard is across reports: if two
    // unrelated projects each have a no-repo linked source for the same
    // path (e.g. `src/lib.rs`), they share a `lnk-` ID and merging them
    // would silently consolidate annotations from disjoint codebases.
    //
    // We can't tell same-project from cross-project at merge time, so we
    // refuse the merge whenever the same no-repo `lnk-` ID appears in two
    // or more inputs. Single-input occurrences are safe and remain allowed.
    {
        let mut no_repo_seen: BTreeMap<&str, usize> = BTreeMap::new();
        for (i, report) in reports.iter().enumerate() {
            for (lnk_id, src) in &report.sources.linked {
                if src.repository.is_some() {
                    continue;
                }
                if let Some(&prior) = no_repo_seen.get(lnk_id.as_str()) {
                    return Err(duvet_core::error!(
                        "linked source '{}' (file '{}') without a repository appears in inputs #{} and #{}; \
                         merging would silently consolidate it across reports — \
                         add a blob-link to disambiguate (see [[source]] blob-link in your duvet config)",
                        lnk_id,
                        src.file_name,
                        prior,
                        i,
                    ));
                }
                no_repo_seen.insert(lnk_id, i);
            }
        }
    }

    // Use the first input as the accumulator and fold the rest in. Because
    // every per-bucket merge operation is a BTreeMap union over content-
    // hashed IDs, the choice of seed doesn't affect the final result —
    // the merge is order-independent (see `property_commutativity_*`).
    let mut iter = reports.into_iter().enumerate();
    let (_, mut out) = iter.next().expect("non-empty");
    if out.version != "2.0" {
        return Err(duvet_core::error!(
            "input #0: unsupported report version '{}', expected '2.0'",
            out.version
        ));
    }

    for (idx, next) in iter {
        merge_one(&mut out, next, idx)?;
    }

    // After all inputs have been folded in, normalize coverage range lists
    // so the output is stable regardless of input order or duplicates.
    finalize_coverage(&mut out.annotations.requirement);

    Ok(out)
}

fn merge_one(out: &mut ReportV2, incoming: ReportV2, idx: usize) -> crate::Result {
    if incoming.version != "2.0" {
        return Err(duvet_core::error!(
            "input #{}: unsupported report version '{}', expected '2.0'",
            idx,
            incoming.version
        ));
    }

    // Each top-level bucket is independent: ordering between them doesn't
    // matter, and within a bucket the merge is keyed by deterministic ID.
    merge_issue_links(&mut out.issue_links, incoming.issue_links);
    merge_repositories(&mut out.repositories, incoming.repositories, idx)?;
    merge_sources(&mut out.sources, incoming.sources, idx)?;
    merge_annotations(&mut out.annotations, incoming.annotations, idx)?;

    Ok(())
}

/// `issue_links` is a sorted, deduped union. The schema is a free-form
/// `Vec<String>` with no ID, so we can't use a `BTreeMap`; sort+dedup gives
/// a canonical order that is independent of fold order, which is what keeps
/// the binary merge commutative (and therefore the N-way fold associative).
fn merge_issue_links(out: &mut Vec<String>, incoming: Vec<String>) {
    out.extend(incoming);
    out.sort();
    out.dedup();
}

fn merge_repositories(
    out: &mut BTreeMap<String, Repository>,
    incoming: BTreeMap<String, Repository>,
    idx: usize,
) -> crate::Result {
    for (id, repo) in incoming {
        match out.get(&id) {
            Some(existing) => {
                // `blob_link` is the sole hash input for `repo-` IDs, so
                // ID equality must imply blob_link equality. If it doesn't,
                // we've hit a hash collision (or a producer bug) and have
                // no safe way to pick between the two.
                if existing.blob_link != repo.blob_link {
                    return Err(duvet_core::error!(
                        "internal invariant violation: input #{} has repository '{}' with blob_link '{}' conflicting with previously seen '{}'; likely hash collision or producer bug -- this is rare; please file a bug with both input reports attached.",
                        idx,
                        id,
                        repo.blob_link,
                        existing.blob_link,
                    ));
                }
            }
            None => {
                out.insert(id, repo);
            }
        }
    }
    Ok(())
}

fn merge_sources(out: &mut SourcesV2, incoming: SourcesV2, idx: usize) -> crate::Result {
    // Inline sources: `src-` IDs hash the contents only, so contents
    // equality is implied by ID equality. `file_name` is *not* hashed —
    // two reports can legitimately disagree on the path they observed for
    // the same bytes (e.g. one stores `specs/rfc.txt`, another stores an
    // absolute path), and we surface that as a hard error rather than
    // silently picking a side.
    for (id, src) in incoming.inline {
        match out.inline.get(&id) {
            Some(existing) => {
                if existing.contents != src.contents {
                    return Err(duvet_core::error!(
                        "internal invariant violation: input #{} has inline source '{}' with contents conflicting with previously seen contents; likely hash collision or producer bug -- this is rare; please file a bug with both input reports attached.",
                        idx,
                        id,
                    ));
                }
                if existing.file_name != src.file_name {
                    return Err(duvet_core::error!(
                        "input #{} has inline source '{}' with file_name '{}' conflicting with previously seen '{}'",
                        idx,
                        id,
                        src.file_name,
                        existing.file_name,
                    ));
                }
            }
            None => {
                out.inline.insert(id, src);
            }
        }
    }

    // Linked sources: `lnk-` IDs hash both `file_name` and `repository_id`.
    // The struct itself only carries those two fields, so ID equality
    // implies struct equality. Any disagreement is a hash collision.
    for (id, src) in incoming.linked {
        match out.linked.get(&id) {
            Some(existing) => {
                if existing != &src {
                    return Err(duvet_core::error!(
                        "internal invariant violation: input #{} has linked source '{}' that differs from previously seen entry under the same id; likely hash collision or producer bug -- this is rare; please file a bug with both input reports attached.",
                        idx,
                        id,
                    ));
                }
            }
            None => {
                out.linked.insert(id, src);
            }
        }
    }

    // Unknown URL-keyed source buckets are passed through verbatim under
    // strict equality — see `merge_extensions`.
    merge_extensions("sources", &mut out.extensions, incoming.extensions, idx)?;

    Ok(())
}

fn merge_annotations(
    out: &mut AnnotationsV2,
    incoming: AnnotationsV2,
    idx: usize,
) -> crate::Result {
    // Each annotation kind has its own conflict policy; see the per-kind
    // helpers below for the precise rules.
    for (id, anno) in incoming.specification {
        merge_specification(&mut out.specification, id, anno, idx)?;
    }
    for (id, anno) in incoming.section {
        merge_section(&mut out.section, id, anno, idx)?;
    }
    for (id, anno) in incoming.requirement {
        merge_requirement(&mut out.requirement, id, anno, idx)?;
    }
    for (id, anno) in incoming.cite {
        merge_cite(&mut out.cite, id, anno, idx)?;
    }

    merge_extensions("annotations", &mut out.extensions, incoming.extensions, idx)?;

    Ok(())
}

/// `spc-` IDs hash `(source_id, start, end)`, so `source` equality is
/// implied by ID equality. `title` and `format` aren't hashed — they
/// reflect parser metadata that can legitimately differ (and that we treat
/// as semantically load-bearing, hence hard errors on mismatch).
fn merge_specification(
    out: &mut BTreeMap<String, SpecificationAnnotation>,
    id: String,
    anno: SpecificationAnnotation,
    idx: usize,
) -> crate::Result {
    match out.get(&id) {
        Some(existing) => {
            if existing.title != anno.title {
                return Err(duvet_core::error!(
                    "input #{} has specification annotation '{}' with title {:?} conflicting with previously seen {:?}",
                    idx, id, anno.title, existing.title
                ));
            }
            if existing.format != anno.format {
                return Err(duvet_core::error!(
                    "input #{} has specification annotation '{}' with format '{}' conflicting with previously seen '{}'",
                    idx, id, anno.format, existing.format
                ));
            }
            if existing.source != anno.source {
                return Err(duvet_core::error!(
                    "internal invariant violation: input #{} has specification '{}' with source differing from previously seen; likely hash collision or producer bug -- this is rare; please file a bug with both input reports attached.",
                    idx, id
                ));
            }
        }
        None => {
            out.insert(id, anno);
        }
    }
    Ok(())
}

/// `sec-` IDs hash `(source_id, start, end)` like `spc-`. `short_name` and
/// `long_name` are extracted by the parser and not hashed; mismatches are
/// hard errors so consumers don't see ambiguous section labels.
fn merge_section(
    out: &mut BTreeMap<String, SectionAnnotation>,
    id: String,
    anno: SectionAnnotation,
    idx: usize,
) -> crate::Result {
    match out.get(&id) {
        Some(existing) => {
            if existing.short_name != anno.short_name {
                return Err(duvet_core::error!(
                    "input #{} has section annotation '{}' with short_name '{}' conflicting with previously seen '{}'",
                    idx, id, anno.short_name, existing.short_name
                ));
            }
            if existing.long_name != anno.long_name {
                return Err(duvet_core::error!(
                    "input #{} has section annotation '{}' with long_name {:?} conflicting with previously seen {:?}",
                    idx,
                    id,
                    anno.long_name,
                    existing.long_name,
                ));
            }
            if existing.source != anno.source {
                return Err(duvet_core::error!(
                    "internal invariant violation: input #{} has section '{}' with source differing from previously seen; likely hash collision or producer bug -- this is rare; please file a bug with both input reports attached.",
                    idx, id
                ));
            }
        }
        None => {
            out.insert(id, anno);
        }
    }
    Ok(())
}

/// Requirements are special: `req-` IDs hash origin, ranges, source, *and*
/// line, so `source` and `origin` equality follow from ID equality. `level`
/// is an out-of-hash structural field we require agreement on. `coverage`
/// is set-valued and unioned across inputs — different reports may witness
/// disjoint cite IDs for the same requirement, and a merge that dropped
/// either set would lose traceability.
fn merge_requirement(
    out: &mut BTreeMap<String, RequirementAnnotation>,
    id: String,
    anno: RequirementAnnotation,
    idx: usize,
) -> crate::Result {
    match out.get_mut(&id) {
        Some(existing) => {
            if existing.level != anno.level {
                return Err(duvet_core::error!(
                    "input #{} has requirement annotation '{}' with level {:?} conflicting with previously seen {:?}",
                    idx,
                    id,
                    anno.level,
                    existing.level,
                ));
            }
            if existing.source != anno.source {
                return Err(duvet_core::error!(
                    "internal invariant violation: input #{} has requirement '{}' with source differing from previously seen; likely hash collision or producer bug -- this is rare; please file a bug with both input reports attached.",
                    idx, id
                ));
            }
            if existing.origin != anno.origin {
                return Err(duvet_core::error!(
                    "internal invariant violation: input #{} has requirement '{}' with origin differing from previously seen; likely hash collision or producer bug -- this is rare; please file a bug with both input reports attached.",
                    idx, id
                ));
            }

            // Coverage is the merge of two cite-keyed range sets. We extend
            // here and normalize (sort+dedup) once at the end, in
            // `finalize_coverage`, to keep the per-merge cost linear.
            for (cite_id, ranges) in anno.coverage {
                existing.coverage.entry(cite_id).or_default().extend(ranges);
            }
        }
        None => {
            out.insert(id, anno);
        }
    }
    Ok(())
}

/// `cite-` IDs hash `(source_id, line, target_source_id)`, so `source` and
/// `target.src` are guaranteed by ID equality. `target.ranges`,
/// `anno_type`, and `level` are out-of-hash structural fields — mismatches
/// here would change the meaning of the citation, so we hard-error.
///
/// The "soft" cite metadata (`comment`, `feature`, `tracking_issue`,
/// `tags`) is not load-bearing for traceability and routinely drifts
/// across branches. On disagreement we reconcile deterministically: the
/// lexicographically smaller value wins for `Option<String>` fields, and
/// `tags` becomes the sorted union of both sets. See [`merge_soft_drift`].
fn merge_cite(
    out: &mut BTreeMap<String, CiteAnnotation>,
    id: String,
    anno: CiteAnnotation,
    idx: usize,
) -> crate::Result {
    match out.get_mut(&id) {
        Some(existing) => {
            if existing.anno_type != anno.anno_type {
                return Err(duvet_core::error!(
                    "input #{} has cite annotation '{}' with type {:?} conflicting with previously seen {:?}",
                    idx,
                    id,
                    anno.anno_type,
                    existing.anno_type,
                ));
            }
            if existing.level != anno.level {
                return Err(duvet_core::error!(
                    "input #{} has cite annotation '{}' with level {:?} conflicting with previously seen {:?}",
                    idx,
                    id,
                    anno.level,
                    existing.level,
                ));
            }
            if existing.target != anno.target {
                return Err(duvet_core::error!(
                    "input #{} has cite annotation '{}' with target conflicting with previously seen",
                    idx, id
                ));
            }
            if existing.source != anno.source {
                return Err(duvet_core::error!(
                    "internal invariant violation: input #{} has cite '{}' with source differing from previously seen; likely hash collision or producer bug -- this is rare; please file a bug with both input reports attached.",
                    idx, id
                ));
            }

            merge_soft_drift(&id, existing, anno, idx);
        }
        None => {
            out.insert(id, anno);
        }
    }
    Ok(())
}

/// Reconcile the soft cite metadata fields (`comment`, `feature`,
/// `tracking_issue`, `tags`) deterministically so the binary merge is
/// commutative and associative. For `Option<String>` fields, `Some` beats
/// `None`, and when both sides are `Some` the lexicographically smaller
/// value wins. `tags` becomes the sorted union of both sets.
///
/// Any field that actually differed before reconciliation is reported on
/// stderr so the drift remains observable.
fn merge_soft_drift(id: &str, existing: &mut CiteAnnotation, incoming: CiteAnnotation, idx: usize) {
    let mut drifted = Vec::new();
    if existing.comment != incoming.comment {
        drifted.push("comment");
    }
    if existing.feature != incoming.feature {
        drifted.push("feature");
    }
    if existing.tracking_issue != incoming.tracking_issue {
        drifted.push("tracking_issue");
    }
    if existing.tags != incoming.tags {
        drifted.push("tags");
    }

    existing.comment = pick_lex_min(existing.comment.take(), incoming.comment);
    existing.feature = pick_lex_min(existing.feature.take(), incoming.feature);
    existing.tracking_issue = pick_lex_min(existing.tracking_issue.take(), incoming.tracking_issue);

    // Sorted union: extend then sort+dedup. The sort gives a canonical
    // order independent of which side contributed which tag; dedup keeps
    // the result idempotent under self-merge.
    existing.tags.extend(incoming.tags);
    existing.tags.sort();
    existing.tags.dedup();

    if !drifted.is_empty() {
        eprintln!(
            "warning: input #{} has cite annotation '{}' with metadata drift from previously seen ({}); reconciling deterministically (lex-min for strings, sorted union for tags)",
            idx,
            id,
            drifted.join(", ")
        );
    }
}

/// Combine two `Option<String>` values from the two sides of a binary
/// merge into a single deterministic value. `Some` beats `None`; when both
/// sides are `Some`, the lexicographically smaller string wins.
fn pick_lex_min(a: Option<String>, b: Option<String>) -> Option<String> {
    match (a, b) {
        (None, None) => None,
        (Some(x), None) | (None, Some(x)) => Some(x),
        (Some(x), Some(y)) => Some(if x <= y { x } else { y }),
    }
}

/// Forward-compatibility passthrough for unknown URL-keyed buckets in
/// `sources` / `annotations`. We can't introspect the value semantics, so
/// we require strict equality on conflict; producers that want softer
/// merge behavior should add a typed bucket and a per-kind handler above.
fn merge_extensions(
    bucket: &str,
    out: &mut BTreeMap<String, serde_json::Value>,
    incoming: BTreeMap<String, serde_json::Value>,
    idx: usize,
) -> crate::Result {
    for (key, value) in incoming {
        match out.get(&key) {
            Some(existing) if existing != &value => {
                return Err(duvet_core::error!(
                    "input #{} has extension '{}' under {} conflicting with previously seen value",
                    idx,
                    key,
                    bucket,
                ));
            }
            Some(_) => {}
            None => {
                out.insert(key, value);
            }
        }
    }
    Ok(())
}

/// Sort + dedup every coverage `Vec<ByteRange>` after all inputs have been
/// merged in. Keeps the merged output canonical regardless of input order.
fn finalize_coverage(requirements: &mut BTreeMap<String, RequirementAnnotation>) {
    for req in requirements.values_mut() {
        for ranges in req.coverage.values_mut() {
            ranges.sort();
            ranges.dedup();
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::json_v2::{
        AnnotationLevel, AnnotationType, ByteRange, InlineSource, LinkedSource, SourceLocation,
        SourceRanges, SourceRef,
    };

    fn empty() -> ReportV2 {
        ReportV2 {
            version: "2.0".to_string(),
            issue_links: Vec::new(),
            repositories: BTreeMap::new(),
            sources: SourcesV2::default(),
            annotations: AnnotationsV2::default(),
        }
    }

    fn cite(
        line: usize,
        anno_type: AnnotationType,
        target_ranges: Vec<ByteRange>,
    ) -> CiteAnnotation {
        CiteAnnotation {
            source: SourceLocation {
                src: "lnk-x".to_string(),
                line: Some(line),
            },
            target: SourceRanges {
                src: "src-spec".to_string(),
                ranges: target_ranges,
            },
            anno_type,
            level: AnnotationLevel::Auto,
            comment: None,
            feature: None,
            tracking_issue: None,
            tags: Vec::new(),
        }
    }

    fn req(level: AnnotationLevel, ranges: Vec<ByteRange>) -> RequirementAnnotation {
        RequirementAnnotation {
            source: SourceLocation {
                src: "lnk-x".to_string(),
                line: Some(1),
            },
            origin: SourceRanges {
                src: "src-spec".to_string(),
                ranges,
            },
            level,
            coverage: BTreeMap::new(),
        }
    }

    #[test]
    fn merge_empty_inputs_errors() {
        let result = merge_reports(Vec::new());
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("at least one"), "msg = {msg}");
    }

    #[test]
    fn merge_single_input_is_identity() {
        let r = empty();
        let merged = merge_reports(vec![r.clone()]).unwrap();
        assert_eq!(merged, r);
    }

    #[test]
    fn merge_two_disjoint() {
        let mut a = empty();
        a.repositories.insert(
            "repo-a".to_string(),
            Repository {
                blob_link: "blob_a".to_string(),
            },
        );
        let mut b = empty();
        b.repositories.insert(
            "repo-b".to_string(),
            Repository {
                blob_link: "blob_b".to_string(),
            },
        );

        let merged = merge_reports(vec![a, b]).unwrap();
        assert_eq!(merged.repositories.len(), 2);
        assert!(merged.repositories.contains_key("repo-a"));
        assert!(merged.repositories.contains_key("repo-b"));
    }

    #[test]
    fn merge_dedups_shared_inline_source() {
        let mut a = empty();
        a.sources.inline.insert(
            "src-1".to_string(),
            InlineSource {
                file_name: "spec.md".to_string(),
                contents: "X".to_string(),
            },
        );
        let b = a.clone();
        let merged = merge_reports(vec![a, b]).unwrap();
        assert_eq!(merged.sources.inline.len(), 1);
    }

    #[test]
    fn merge_unions_requirement_coverage() {
        let mut a = empty();
        let mut req_a = req(AnnotationLevel::Must, vec![ByteRange { start: 0, end: 10 }]);
        req_a
            .coverage
            .insert("cite-aa".to_string(), vec![ByteRange { start: 0, end: 5 }]);
        a.annotations.requirement.insert("req-1".to_string(), req_a);

        let mut b = empty();
        let mut req_b = req(AnnotationLevel::Must, vec![ByteRange { start: 0, end: 10 }]);
        req_b
            .coverage
            .insert("cite-bb".to_string(), vec![ByteRange { start: 5, end: 10 }]);
        b.annotations.requirement.insert("req-1".to_string(), req_b);

        let merged = merge_reports(vec![a, b]).unwrap();
        let req = merged.annotations.requirement.get("req-1").unwrap();
        assert_eq!(req.coverage.len(), 2);
        assert!(req.coverage.contains_key("cite-aa"));
        assert!(req.coverage.contains_key("cite-bb"));
    }

    #[test]
    fn merge_dedups_overlapping_coverage_ranges() {
        let mut a = empty();
        let mut req_a = req(AnnotationLevel::Must, vec![ByteRange { start: 0, end: 10 }]);
        req_a.coverage.insert(
            "cite-aa".to_string(),
            vec![
                ByteRange { start: 0, end: 5 },
                ByteRange { start: 7, end: 10 },
            ],
        );
        a.annotations.requirement.insert("req-1".to_string(), req_a);

        let mut b = empty();
        let mut req_b = req(AnnotationLevel::Must, vec![ByteRange { start: 0, end: 10 }]);
        req_b.coverage.insert(
            "cite-aa".to_string(),
            vec![
                ByteRange { start: 0, end: 5 },
                ByteRange { start: 5, end: 8 },
            ],
        );
        b.annotations.requirement.insert("req-1".to_string(), req_b);

        let merged = merge_reports(vec![a, b]).unwrap();
        let ranges = &merged.annotations.requirement["req-1"].coverage["cite-aa"];
        assert_eq!(
            ranges,
            &vec![
                ByteRange { start: 0, end: 5 },
                ByteRange { start: 5, end: 8 },
                ByteRange { start: 7, end: 10 },
            ]
        );
    }

    #[test]
    fn merge_concats_issue_links_dedup() {
        let mut a = empty();
        a.issue_links = vec!["x".to_string(), "y".to_string()];
        let mut b = empty();
        b.issue_links = vec!["y".to_string(), "z".to_string()];

        let merged = merge_reports(vec![a, b]).unwrap();
        assert_eq!(
            merged.issue_links,
            vec!["x".to_string(), "y".to_string(), "z".to_string()]
        );
    }

    #[test]
    fn merge_error_names_offending_input_index_for_non_adjacent_conflict() {
        // 3-way fold where #0 and #2 conflict on a spec title, with #1
        // contributing nothing on `spc-1`. The error should name the
        // incoming input (#2), not just print "A vs B" without a pointer.
        let mut a = empty();
        a.annotations.specification.insert(
            "spc-1".to_string(),
            SpecificationAnnotation {
                source: SourceRef {
                    src: "src-x".to_string(),
                    start: 0,
                    end: 1,
                },
                title: Some("A".to_string()),
                format: "markdown".to_string(),
            },
        );
        let b = empty();
        let mut c = empty();
        c.annotations.specification.insert(
            "spc-1".to_string(),
            SpecificationAnnotation {
                source: SourceRef {
                    src: "src-x".to_string(),
                    start: 0,
                    end: 1,
                },
                title: Some("C".to_string()),
                format: "markdown".to_string(),
            },
        );

        let err = merge_reports(vec![a, b, c]).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("input #2"), "msg = {msg}");
        assert!(msg.contains("spc-1"), "msg = {msg}");
        assert!(msg.contains("title"), "msg = {msg}");
    }

    #[test]
    fn merge_errors_on_spec_title_mismatch() {
        let mut a = empty();
        a.annotations.specification.insert(
            "spc-1".to_string(),
            SpecificationAnnotation {
                source: SourceRef {
                    src: "src-x".to_string(),
                    start: 0,
                    end: 1,
                },
                title: Some("A".to_string()),
                format: "markdown".to_string(),
            },
        );
        let mut b = empty();
        b.annotations.specification.insert(
            "spc-1".to_string(),
            SpecificationAnnotation {
                source: SourceRef {
                    src: "src-x".to_string(),
                    start: 0,
                    end: 1,
                },
                title: Some("B".to_string()),
                format: "markdown".to_string(),
            },
        );
        let err = merge_reports(vec![a, b]).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("spc-1"), "msg = {msg}");
        assert!(msg.contains("title"), "msg = {msg}");
    }

    #[test]
    fn merge_errors_on_section_long_name_mismatch() {
        let mut a = empty();
        a.annotations.section.insert(
            "sec-1".to_string(),
            SectionAnnotation {
                source: SourceRef {
                    src: "src-x".to_string(),
                    start: 0,
                    end: 1,
                },
                short_name: "s".to_string(),
                long_name: Some("A".to_string()),
            },
        );
        let mut b = empty();
        b.annotations.section.insert(
            "sec-1".to_string(),
            SectionAnnotation {
                source: SourceRef {
                    src: "src-x".to_string(),
                    start: 0,
                    end: 1,
                },
                short_name: "s".to_string(),
                long_name: Some("B".to_string()),
            },
        );
        let err = merge_reports(vec![a, b]).unwrap_err();
        assert!(format!("{err}").contains("long_name"));
    }

    #[test]
    fn merge_errors_on_inline_source_file_name_mismatch() {
        let mut a = empty();
        a.sources.inline.insert(
            "src-1".to_string(),
            InlineSource {
                file_name: "a.md".to_string(),
                contents: "X".to_string(),
            },
        );
        let mut b = empty();
        b.sources.inline.insert(
            "src-1".to_string(),
            InlineSource {
                file_name: "b.md".to_string(),
                contents: "X".to_string(),
            },
        );
        let err = merge_reports(vec![a, b]).unwrap_err();
        assert!(format!("{err}").contains("file_name"));
    }

    #[test]
    fn merge_errors_on_requirement_level_mismatch() {
        let mut a = empty();
        a.annotations
            .requirement
            .insert("req-1".to_string(), req(AnnotationLevel::Must, vec![]));
        let mut b = empty();
        b.annotations
            .requirement
            .insert("req-1".to_string(), req(AnnotationLevel::Should, vec![]));
        let err = merge_reports(vec![a, b]).unwrap_err();
        assert!(format!("{err}").contains("level"));
    }

    #[test]
    fn merge_errors_on_cite_anno_type_mismatch() {
        let mut a = empty();
        a.annotations.cite.insert(
            "cite-1".to_string(),
            cite(1, AnnotationType::Citation, vec![]),
        );
        let mut b = empty();
        b.annotations
            .cite
            .insert("cite-1".to_string(), cite(1, AnnotationType::Test, vec![]));
        let err = merge_reports(vec![a, b]).unwrap_err();
        assert!(format!("{err}").contains("type"));
    }

    #[test]
    fn merge_errors_on_cite_target_mismatch() {
        let mut a = empty();
        a.annotations.cite.insert(
            "cite-1".to_string(),
            cite(
                1,
                AnnotationType::Citation,
                vec![ByteRange { start: 0, end: 5 }],
            ),
        );
        let mut b = empty();
        b.annotations.cite.insert(
            "cite-1".to_string(),
            cite(
                1,
                AnnotationType::Citation,
                vec![ByteRange { start: 0, end: 6 }],
            ),
        );
        let err = merge_reports(vec![a, b]).unwrap_err();
        assert!(format!("{err}").contains("target"));
    }

    #[test]
    fn merge_errors_on_cite_level_mismatch() {
        let mut c1 = cite(1, AnnotationType::Citation, vec![]);
        c1.level = AnnotationLevel::Must;
        let mut c2 = cite(1, AnnotationType::Citation, vec![]);
        c2.level = AnnotationLevel::Should;

        let mut a = empty();
        a.annotations.cite.insert("cite-1".to_string(), c1);
        let mut b = empty();
        b.annotations.cite.insert("cite-1".to_string(), c2);
        let err = merge_reports(vec![a, b]).unwrap_err();
        assert!(format!("{err}").contains("level"));
    }

    #[test]
    fn merge_cite_comment_drift_picks_lexicographically_smaller() {
        // The drift winner is whichever string sorts first, regardless of
        // input order — that's what makes the merge commutative.
        let mut c1 = cite(1, AnnotationType::Citation, vec![]);
        c1.comment = Some("first".to_string());
        let mut c2 = cite(1, AnnotationType::Citation, vec![]);
        c2.comment = Some("second".to_string());

        let mut a = empty();
        a.annotations.cite.insert("cite-1".to_string(), c1);
        let mut b = empty();
        b.annotations.cite.insert("cite-1".to_string(), c2);

        let forward = merge_reports(vec![a.clone(), b.clone()]).unwrap();
        let reverse = merge_reports(vec![b, a]).unwrap();
        // "first" < "second", so it wins regardless of fold order.
        assert_eq!(
            forward.annotations.cite["cite-1"].comment.as_deref(),
            Some("first")
        );
        assert_eq!(forward, reverse);
    }

    #[test]
    fn merge_cite_some_beats_none_in_either_direction() {
        // `Some` should always win over `None` so a sparsely-populated
        // input doesn't erase metadata recorded by another.
        let mut c1 = cite(1, AnnotationType::Citation, vec![]);
        c1.feature = Some("f".to_string());
        let c2 = cite(1, AnnotationType::Citation, vec![]); // feature: None

        let mut a = empty();
        a.annotations.cite.insert("cite-1".to_string(), c1);
        let mut b = empty();
        b.annotations.cite.insert("cite-1".to_string(), c2);

        let forward = merge_reports(vec![a.clone(), b.clone()]).unwrap();
        let reverse = merge_reports(vec![b, a]).unwrap();
        assert_eq!(
            forward.annotations.cite["cite-1"].feature.as_deref(),
            Some("f")
        );
        assert_eq!(forward, reverse);
    }

    #[test]
    fn merge_cite_tags_are_sorted_union() {
        let mut c1 = cite(1, AnnotationType::Citation, vec![]);
        c1.tags = vec!["b".to_string(), "a".to_string()];
        let mut c2 = cite(1, AnnotationType::Citation, vec![]);
        c2.tags = vec!["c".to_string(), "a".to_string()];

        let mut a = empty();
        a.annotations.cite.insert("cite-1".to_string(), c1);
        let mut b = empty();
        b.annotations.cite.insert("cite-1".to_string(), c2);

        let forward = merge_reports(vec![a.clone(), b.clone()]).unwrap();
        let reverse = merge_reports(vec![b, a]).unwrap();
        assert_eq!(
            forward.annotations.cite["cite-1"].tags,
            vec!["a".to_string(), "b".to_string(), "c".to_string()]
        );
        assert_eq!(forward, reverse);
    }

    #[test]
    fn merge_errors_on_extension_conflict() {
        let mut a = empty();
        a.sources
            .extensions
            .insert("https://x/y".to_string(), serde_json::json!({"v": 1}));
        let mut b = empty();
        b.sources
            .extensions
            .insert("https://x/y".to_string(), serde_json::json!({"v": 2}));
        let err = merge_reports(vec![a, b]).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("https://x/y"), "msg = {msg}");
    }

    #[test]
    fn merge_passes_through_identical_extensions() {
        let mut a = empty();
        a.sources
            .extensions
            .insert("https://x/y".to_string(), serde_json::json!({"v": 1}));
        let b = a.clone();
        let merged = merge_reports(vec![a, b]).unwrap();
        assert_eq!(merged.sources.extensions.len(), 1);
    }

    #[test]
    fn merge_errors_on_unsupported_version() {
        let mut a = empty();
        a.version = "1.0".to_string();
        let err = merge_reports(vec![a]).unwrap_err();
        assert!(format!("{err}").contains("unsupported report version"));
    }

    #[test]
    fn merge_errors_on_unsupported_version_in_second_input() {
        let a = empty();
        let mut b = empty();
        b.version = "3.0".to_string();
        let err = merge_reports(vec![a, b]).unwrap_err();
        assert!(format!("{err}").contains("unsupported report version"));
    }

    #[test]
    fn merge_errors_on_repository_blob_link_collision() {
        // Same repo-id, different blob_link — i.e. simulated hash collision.
        let mut a = empty();
        a.repositories.insert(
            "repo-1".to_string(),
            Repository {
                blob_link: "blob_one".to_string(),
            },
        );
        let mut b = empty();
        b.repositories.insert(
            "repo-1".to_string(),
            Repository {
                blob_link: "blob_two".to_string(),
            },
        );
        let err = merge_reports(vec![a, b]).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("invariant"), "msg = {msg}");
        assert!(msg.contains("blob_link"), "msg = {msg}");
    }

    #[test]
    fn merge_errors_on_inline_source_contents_collision() {
        // Same src-id, different contents — simulated hash collision.
        let mut a = empty();
        a.sources.inline.insert(
            "src-1".to_string(),
            InlineSource {
                file_name: "spec.md".to_string(),
                contents: "X".to_string(),
            },
        );
        let mut b = empty();
        b.sources.inline.insert(
            "src-1".to_string(),
            InlineSource {
                file_name: "spec.md".to_string(),
                contents: "Y".to_string(),
            },
        );
        let err = merge_reports(vec![a, b]).unwrap_err();
        assert!(format!("{err}").contains("invariant"));
    }

    #[test]
    fn merge_errors_when_no_repo_linked_source_overlaps_inputs() {
        // Two inputs both carry a no-repo linked source under the same
        // lnk-id — the canonical cross-project consolidation hazard.
        let mut a = empty();
        a.sources.linked.insert(
            "lnk-x".to_string(),
            LinkedSource {
                file_name: "src/lib.rs".to_string(),
                repository: None,
            },
        );
        let mut b = empty();
        b.sources.linked.insert(
            "lnk-x".to_string(),
            LinkedSource {
                file_name: "src/lib.rs".to_string(),
                repository: None,
            },
        );
        let err = merge_reports(vec![a, b]).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("inputs #0 and #1"), "msg = {msg}");
        assert!(msg.contains("lnk-x"), "msg = {msg}");
        assert!(msg.contains("src/lib.rs"), "msg = {msg}");
        assert!(msg.contains("blob-link"), "msg = {msg}");
    }

    #[test]
    fn merge_errors_when_no_repo_linked_source_overlaps_third_input() {
        // The collision is across non-adjacent inputs (#0 and #2). The
        // pre-validation pass needs to catch it independent of fold order.
        let mut a = empty();
        a.sources.linked.insert(
            "lnk-x".to_string(),
            LinkedSource {
                file_name: "src/lib.rs".to_string(),
                repository: None,
            },
        );
        let b = empty();
        let mut c = empty();
        c.sources.linked.insert(
            "lnk-x".to_string(),
            LinkedSource {
                file_name: "src/lib.rs".to_string(),
                repository: None,
            },
        );
        let err = merge_reports(vec![a, b, c]).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("inputs #0 and #2"), "msg = {msg}");
    }

    #[test]
    fn merge_allows_disjoint_no_repo_linked_sources_three_way() {
        // 3-way happy path: each input has its own no-repo linked source
        // under a distinct lnk-id, so there's no cross-input collision.
        let mut a = empty();
        a.sources.linked.insert(
            "lnk-x".to_string(),
            LinkedSource {
                file_name: "package-a/src/lib.rs".to_string(),
                repository: None,
            },
        );
        let mut b = empty();
        b.sources.linked.insert(
            "lnk-y".to_string(),
            LinkedSource {
                file_name: "package-b/src/lib.rs".to_string(),
                repository: None,
            },
        );
        let mut c = empty();
        c.sources.linked.insert(
            "lnk-z".to_string(),
            LinkedSource {
                file_name: "package-c/src/lib.rs".to_string(),
                repository: None,
            },
        );

        let merged = merge_reports(vec![a, b, c]).unwrap();
        assert_eq!(merged.sources.linked.len(), 3);
        assert!(merged.sources.linked.contains_key("lnk-x"));
        assert!(merged.sources.linked.contains_key("lnk-y"));
        assert!(merged.sources.linked.contains_key("lnk-z"));
    }

    #[test]
    fn merge_allows_no_repo_linked_source_in_single_input() {
        // A no-repo linked source that appears in only one input is fine —
        // there's no cross-input collision possible.
        let mut a = empty();
        a.sources.linked.insert(
            "lnk-x".to_string(),
            LinkedSource {
                file_name: "src/lib.rs".to_string(),
                repository: None,
            },
        );
        let b = empty();
        let merged = merge_reports(vec![a, b]).unwrap();
        assert_eq!(merged.sources.linked.len(), 1);
        assert_eq!(merged.sources.linked["lnk-x"].repository, None);
    }

    // ── Property tests ──

    fn fixture_a() -> ReportV2 {
        let mut r = empty();
        r.repositories.insert(
            "repo-a".to_string(),
            Repository {
                blob_link: "blob_a".to_string(),
            },
        );
        r.sources.inline.insert(
            "src-1".to_string(),
            InlineSource {
                file_name: "spec.md".to_string(),
                contents: "X".to_string(),
            },
        );
        let mut req_x = req(AnnotationLevel::Must, vec![ByteRange { start: 0, end: 10 }]);
        req_x
            .coverage
            .insert("cite-aa".to_string(), vec![ByteRange { start: 0, end: 5 }]);
        r.annotations.requirement.insert("req-1".to_string(), req_x);
        r.annotations.cite.insert(
            "cite-aa".to_string(),
            cite(10, AnnotationType::Citation, vec![]),
        );
        r
    }

    fn fixture_b() -> ReportV2 {
        let mut r = empty();
        r.repositories.insert(
            "repo-b".to_string(),
            Repository {
                blob_link: "blob_b".to_string(),
            },
        );
        r.sources.inline.insert(
            "src-1".to_string(),
            InlineSource {
                file_name: "spec.md".to_string(),
                contents: "X".to_string(),
            },
        );
        let mut req_x = req(AnnotationLevel::Must, vec![ByteRange { start: 0, end: 10 }]);
        req_x
            .coverage
            .insert("cite-bb".to_string(), vec![ByteRange { start: 5, end: 10 }]);
        r.annotations.requirement.insert("req-1".to_string(), req_x);
        r.annotations.cite.insert(
            "cite-bb".to_string(),
            cite(20, AnnotationType::Citation, vec![]),
        );
        r
    }

    fn fixture_c() -> ReportV2 {
        let mut r = empty();
        r.repositories.insert(
            "repo-c".to_string(),
            Repository {
                blob_link: "blob_c".to_string(),
            },
        );
        r
    }

    #[test]
    fn property_commutativity_handcrafted() {
        let a = fixture_a();
        let b = fixture_b();
        let forward = merge_reports(vec![a.clone(), b.clone()]).unwrap();
        let reverse = merge_reports(vec![b, a]).unwrap();
        assert_eq!(forward, reverse);
    }

    #[test]
    fn property_associativity_handcrafted() {
        let a = fixture_a();
        let b = fixture_b();
        let c = fixture_c();
        let abc = merge_reports(vec![a.clone(), b.clone(), c.clone()]).unwrap();
        let ab = merge_reports(vec![a, b]).unwrap();
        let abc2 = merge_reports(vec![ab, c]).unwrap();
        assert_eq!(abc, abc2);
    }

    #[test]
    fn property_idempotency_handcrafted() {
        let a = fixture_a();
        let merged = merge_reports(vec![a.clone(), a.clone()]).unwrap();
        assert_eq!(merged, a);
    }

    #[test]
    fn property_serde_roundtrip_after_merge() {
        let merged = merge_reports(vec![fixture_a(), fixture_b()]).unwrap();
        let json = serde_json::to_string(&merged).unwrap();
        let back: ReportV2 = serde_json::from_str(&json).unwrap();
        assert_eq!(back, merged);
    }

    /// Three reports that share a single cite ID but disagree on every
    /// soft-drift field. Used to exercise the commutative reconciliation.
    fn drift_fixture(comment: &str, feature: &str, tracking: &str, tag: &str) -> ReportV2 {
        let mut r = empty();
        let mut c = cite(1, AnnotationType::Citation, vec![]);
        c.comment = Some(comment.to_string());
        c.feature = Some(feature.to_string());
        c.tracking_issue = Some(tracking.to_string());
        c.tags = vec![tag.to_string()];
        r.annotations.cite.insert("cite-shared".to_string(), c);
        r
    }

    #[test]
    fn property_commutativity_of_issue_links() {
        // `issue_links` has no entity ID, so the only thing keeping the merge
        // commutative is canonical ordering of the unioned list. With
        // overlapping-but-not-equal inputs, an order-preserving append
        // would produce different results for `[a, b]` vs `[b, a]`.
        let mut a = empty();
        a.issue_links = vec!["x".to_string(), "y".to_string()];
        let mut b = empty();
        b.issue_links = vec!["y".to_string(), "z".to_string()];

        let forward = merge_reports(vec![a.clone(), b.clone()]).unwrap();
        let reverse = merge_reports(vec![b, a]).unwrap();
        assert_eq!(forward, reverse);
    }

    #[test]
    fn property_commutativity_with_soft_drift() {
        let a = drift_fixture("alpha", "f-a", "T-1", "x");
        let b = drift_fixture("bravo", "f-b", "T-2", "y");
        let forward = merge_reports(vec![a.clone(), b.clone()]).unwrap();
        let reverse = merge_reports(vec![b, a]).unwrap();
        assert_eq!(forward, reverse);
    }

    #[test]
    fn property_n_way_order_independence_with_soft_drift() {
        // Three inputs, all sharing one cite ID with disagreeing soft-drift
        // values. All 6 permutations of the 3-way fold must produce the
        // same merged report.
        let a = drift_fixture("alpha", "f-a", "T-1", "x");
        let b = drift_fixture("bravo", "f-b", "T-2", "y");
        let c = drift_fixture("charlie", "f-c", "T-3", "z");

        let perms = [
            vec![a.clone(), b.clone(), c.clone()],
            vec![a.clone(), c.clone(), b.clone()],
            vec![b.clone(), a.clone(), c.clone()],
            vec![b.clone(), c.clone(), a.clone()],
            vec![c.clone(), a.clone(), b.clone()],
            vec![c.clone(), b.clone(), a.clone()],
        ];
        let baseline = merge_reports(perms[0].clone()).unwrap();
        for perm in &perms[1..] {
            let merged = merge_reports(perm.clone()).unwrap();
            assert_eq!(merged, baseline);
        }

        // Sanity-check the reconciled values: lex-min for Option strings,
        // sorted union for tags.
        let merged_cite = &baseline.annotations.cite["cite-shared"];
        assert_eq!(merged_cite.comment.as_deref(), Some("alpha"));
        assert_eq!(merged_cite.feature.as_deref(), Some("f-a"));
        assert_eq!(merged_cite.tracking_issue.as_deref(), Some("T-1"));
        assert_eq!(
            merged_cite.tags,
            vec!["x".to_string(), "y".to_string(), "z".to_string()]
        );
    }

    // ── Light snapshot-based smoke test ──
    //
    // Read an existing v2 report fixture (the same one used by
    // `real_report_snapshot_roundtrip` in `json_v2.rs`), self-merge it,
    // assert the result equals the input. This exercises the merge code
    // path against realistic data without needing fixture sets or a
    // generator.

    #[test]
    fn merge_real_snapshot_self_idempotent() {
        // Use the per-source blob-link fixture: every linked source has a
        // repository, so the no-repo cross-input collision check accepts a
        // self-merge. The markdown fixture would (correctly) be rejected,
        // since self-merge of a no-repo lnk-id is indistinguishable from a
        // cross-project consolidation.
        const SNAPSHOT: &str =
            include_str!("../../../integration/snapshots/report-source-blob-link_json_v2.snap");
        let json = SNAPSHOT
            .split_once("---\n")
            .and_then(|(_, rest)| rest.split_once("---\n"))
            .map(|(_, body)| body)
            .expect("snapshot should have two `---` front-matter delimiters");
        let report: ReportV2 = serde_json::from_str(json).expect("must deserialize");

        let merged = merge_reports(vec![report.clone(), report.clone()]).unwrap();
        assert_eq!(merged, report);
    }
}
