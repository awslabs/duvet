// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Conversion from the merge-friendly v2 report to the legacy v1 wire shape.

use super::{
    json_v1::{
        AnnotationV1, LineV1, RefStatusV1, ReportV1, RequirementStatusV1, SectionV1,
        SpecificationV1,
    },
    json_v2::{
        AnnotationLevel, AnnotationType, ByteRange, ReportV2, SourceRanges, SpecificationAnnotation,
    },
};
use crate::{
    specification::{Format, Line, Specification},
    target::TargetPath,
};
use duvet_core::file::SourceFile;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    ops::Range,
};

struct ParsedSpecification {
    identity: String,
    file: SourceFile,
    specification: Specification,
}

#[derive(Clone)]
struct ConvertedAnnotation {
    stable_id: String,
    source_id: String,
    ranges: Vec<ByteRange>,
    section: String,
    anno_type: String,
    level: String,
    annotation: AnnotationV1,
}

#[derive(Clone)]
struct RangeReference {
    annotation_id: usize,
    range: ByteRange,
    anno_type: String,
    level: String,
}

type RequirementIds = BTreeMap<String, Vec<String>>;

#[derive(Clone)]
struct CanonicalAnnotationMeta {
    anno_type: String,
    level: String,
}

struct CanonicalSpecification {
    title: Option<String>,
    format: String,
    requirements: BTreeSet<String>,
    sections: Vec<CanonicalSection>,
}

struct CanonicalSection {
    id: String,
    title: String,
    requirements: BTreeSet<String>,
    lines: Vec<CanonicalLine>,
}

struct CanonicalLine {
    text: String,
    overlays: Vec<(usize, usize, Vec<String>)>,
}

/// Convert a v2 report to v1. The optional issue link overrides all links in
/// the input. A warning is returned when a merged report has multiple links
/// and no override.
pub fn convert(
    report: &ReportV2,
    issue_link_override: Option<&str>,
) -> crate::Result<(ReportV1, Option<String>)> {
    validate_top_level(report)?;
    let parsed = parse_specifications(report)?;
    validate_sections(report, &parsed)?;

    let (mut annotations, requirement_ids) = build_annotations(report, &parsed)?;
    annotations.sort_by(|a, b| annotation_sort_key(a).cmp(&annotation_sort_key(b)));

    let stable_to_integer: HashMap<_, _> = annotations
        .iter()
        .enumerate()
        .map(|(id, annotation)| (annotation.stable_id.clone(), id))
        .collect();

    let ranges_by_source = build_range_references(&annotations);
    let statuses = build_statuses(report, &stable_to_integer, &parsed)?;
    let specifications = build_v1_specifications(
        &parsed,
        &ranges_by_source,
        &requirement_ids,
        &stable_to_integer,
    )?;

    let (issue_link, warning) = select_issue_link(report, issue_link_override);

    Ok((
        ReportV1 {
            blob_link: None,
            issue_link,
            specifications,
            annotations: annotations
                .into_iter()
                .map(|annotation| annotation.annotation)
                .collect(),
            statuses,
            refs: all_ref_statuses(),
        },
        warning,
    ))
}

fn validate_top_level(report: &ReportV2) -> crate::Result {
    if report.version != "2.0" {
        return Err(duvet_core::error!(
            "unsupported report version '{}', expected '2.0'",
            report.version
        ));
    }
    if !report.sources.extensions.is_empty() {
        return Err(duvet_core::error!(
            "cannot convert report with non-empty source extension buckets"
        ));
    }
    if !report.annotations.extensions.is_empty() {
        return Err(duvet_core::error!(
            "cannot convert report with non-empty annotation extension buckets"
        ));
    }
    Ok(())
}

fn parse_specifications(report: &ReportV2) -> crate::Result<BTreeMap<String, ParsedSpecification>> {
    let mut parsed = BTreeMap::new();
    let mut identities = BTreeSet::new();

    for (annotation_id, annotation) in &report.annotations.specification {
        let inline = report
            .sources
            .inline
            .get(&annotation.source.src)
            .ok_or_else(|| {
                duvet_core::error!(
                    "specification '{}' references missing inline source '{}'",
                    annotation_id,
                    annotation.source.src
                )
            })?;
        validate_source_ref(
            &inline.contents,
            annotation.source.start,
            annotation.source.end,
            &format!("specification '{annotation_id}'"),
        )?;
        if annotation.source.start != 0 || annotation.source.end != inline.contents.len() {
            return Err(duvet_core::error!(
                "specification '{}' extent {}..{} does not cover its full source 0..{}",
                annotation_id,
                annotation.source.start,
                annotation.source.end,
                inline.contents.len()
            ));
        }

        let format: Format = annotation.format.parse().map_err(|_| {
            duvet_core::error!(
                "specification '{}' has unsupported format '{}'",
                annotation_id,
                annotation.format
            )
        })?;
        let file = SourceFile::new(inline.file_name.clone(), inline.contents.clone())?;
        let specification = format.parse(&file)?;
        if specification.format.to_string() != annotation.format {
            return Err(duvet_core::error!(
                "specification '{}' reparsed as '{}' instead of recorded format '{}'",
                annotation_id,
                specification.format,
                annotation.format
            ));
        }
        if specification.title != annotation.title {
            return Err(duvet_core::error!(
                "specification '{}' title changed after reparsing: recorded {:?}, parsed {:?}",
                annotation_id,
                annotation.title,
                specification.title
            ));
        }

        let identity = specification_identity(annotation, &inline.file_name);
        if !identities.insert(identity.clone()) {
            return Err(duvet_core::error!(
                "multiple v2 specifications resolve to v1 identity '{}'",
                identity
            ));
        }
        if parsed
            .insert(
                annotation.source.src.clone(),
                ParsedSpecification {
                    identity,
                    file,
                    specification,
                },
            )
            .is_some()
        {
            return Err(duvet_core::error!(
                "multiple specification annotations reference inline source '{}'",
                annotation.source.src
            ));
        }
    }

    Ok(parsed)
}

fn validate_sections(
    report: &ReportV2,
    parsed: &BTreeMap<String, ParsedSpecification>,
) -> crate::Result {
    let mut recorded_by_source: BTreeMap<&str, Vec<(&str, &super::json_v2::SectionAnnotation)>> =
        BTreeMap::new();
    for (id, section) in &report.annotations.section {
        let spec = parsed.get(&section.source.src).ok_or_else(|| {
            duvet_core::error!(
                "section '{}' references inline source '{}' without a specification",
                id,
                section.source.src
            )
        })?;
        validate_source_ref(
            &spec.file,
            section.source.start,
            section.source.end,
            &format!("section '{id}'"),
        )?;
        recorded_by_source
            .entry(&section.source.src)
            .or_default()
            .push((id, section));
    }

    for (source_id, spec) in parsed {
        let recorded = recorded_by_source
            .get(source_id.as_str())
            .cloned()
            .unwrap_or_default();
        let parsed_sections = spec.specification.sorted_sections();
        if recorded.len() != parsed_sections.len() {
            return Err(duvet_core::error!(
                "parser drift for source '{}': v2 records {} sections but current parser produced {}",
                source_id,
                recorded.len(),
                parsed_sections.len()
            ));
        }

        for section in parsed_sections {
            let range = section_range(section);
            let matches: Vec<_> = recorded
                .iter()
                .filter(|(_, candidate)| {
                    candidate.short_name == section.id
                        && candidate.long_name.as_deref() == Some(section.title.as_str())
                        && candidate.source.start == range.start
                        && candidate.source.end == range.end
                })
                .collect();
            if matches.len() != 1 {
                return Err(duvet_core::error!(
                    "parser drift for source '{}', section '{}': expected exactly one recorded section matching title {:?} and extent {}..{}, found {}",
                    source_id,
                    section.id,
                    section.title,
                    range.start,
                    range.end,
                    matches.len()
                ));
            }
        }
    }

    Ok(())
}

fn build_annotations(
    report: &ReportV2,
    parsed: &BTreeMap<String, ParsedSpecification>,
) -> crate::Result<(Vec<ConvertedAnnotation>, RequirementIds)> {
    let mut annotations = Vec::new();
    let mut requirements: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for (stable_id, requirement) in &report.annotations.requirement {
        let spec = parsed.get(&requirement.origin.src).ok_or_else(|| {
            duvet_core::error!(
                "requirement '{}' references missing specification source '{}'",
                stable_id,
                requirement.origin.src
            )
        })?;
        validate_ranges(
            &spec.file,
            &requirement.origin,
            &format!("requirement '{stable_id}'"),
        )?;
        let section = infer_section(
            &spec.specification,
            &requirement.origin.ranges,
            &format!("requirement '{stable_id}'"),
        )?;
        let (source, blob_link) =
            resolve_linked_source(report, &requirement.source.src, stable_id)?;
        let comment = join_range_text(&spec.file, &requirement.origin.ranges);

        requirements
            .entry(requirement.origin.src.clone())
            .or_default()
            .push(stable_id.clone());
        annotations.push(ConvertedAnnotation {
            stable_id: stable_id.clone(),
            source_id: requirement.origin.src.clone(),
            ranges: sorted_ranges(&requirement.origin.ranges),
            section: section.id.clone(),
            anno_type: "SPEC".to_string(),
            level: level_name(requirement.level).to_string(),
            annotation: AnnotationV1 {
                source,
                target_path: spec.identity.clone(),
                target_section: Some(section.id.clone()),
                line: requirement.source.line.unwrap_or(0),
                anno_type: "SPEC".to_string(),
                level: level_name(requirement.level).to_string(),
                comment: nonempty(comment),
                blob_link,
                ..Default::default()
            },
        });
    }

    for (stable_id, cite) in &report.annotations.cite {
        let spec = parsed.get(&cite.target.src).ok_or_else(|| {
            duvet_core::error!(
                "cite '{}' references missing specification source '{}'",
                stable_id,
                cite.target.src
            )
        })?;
        validate_ranges(&spec.file, &cite.target, &format!("cite '{stable_id}'"))?;
        let section = infer_section(
            &spec.specification,
            &cite.target.ranges,
            &format!("cite '{stable_id}'"),
        )?;
        let (source, blob_link) = resolve_linked_source(report, &cite.source.src, stable_id)?;
        let anno_type = annotation_type_name(cite.anno_type).to_string();
        annotations.push(ConvertedAnnotation {
            stable_id: stable_id.clone(),
            source_id: cite.target.src.clone(),
            ranges: sorted_ranges(&cite.target.ranges),
            section: section.id.clone(),
            anno_type: anno_type.clone(),
            level: level_name(cite.level).to_string(),
            annotation: AnnotationV1 {
                source,
                target_path: spec.identity.clone(),
                target_section: Some(section.id.clone()),
                line: cite.source.line.unwrap_or(0),
                anno_type,
                level: level_name(cite.level).to_string(),
                comment: cite.comment.clone(),
                feature: cite.feature.clone(),
                tracking_issue: cite.tracking_issue.clone(),
                blob_link,
                tags: cite.tags.clone(),
            },
        });
    }

    for ids in requirements.values_mut() {
        ids.sort();
    }
    Ok((annotations, requirements))
}

fn annotation_sort_key(annotation: &ConvertedAnnotation) -> (&str, usize, &str, &str, &str, &str) {
    (
        &annotation.annotation.source,
        annotation.annotation.line,
        &annotation.annotation.target_path,
        &annotation.section,
        &annotation.anno_type,
        &annotation.stable_id,
    )
}

fn build_range_references(
    annotations: &[ConvertedAnnotation],
) -> BTreeMap<String, Vec<RangeReference>> {
    let mut by_source: BTreeMap<String, Vec<RangeReference>> = BTreeMap::new();
    for (annotation_id, annotation) in annotations.iter().enumerate() {
        for range in &annotation.ranges {
            by_source
                .entry(annotation.source_id.clone())
                .or_default()
                .push(RangeReference {
                    annotation_id,
                    range: range.clone(),
                    anno_type: annotation.anno_type.clone(),
                    level: annotation.level.clone(),
                });
        }
    }
    for references in by_source.values_mut() {
        references.sort_by_key(|reference| {
            (
                reference.range.start,
                reference.range.end,
                reference.annotation_id,
            )
        });
    }
    by_source
}

fn build_statuses(
    report: &ReportV2,
    stable_to_integer: &HashMap<String, usize>,
    parsed: &BTreeMap<String, ParsedSpecification>,
) -> crate::Result<BTreeMap<usize, RequirementStatusV1>> {
    let mut statuses = BTreeMap::new();

    for (requirement_id, requirement) in &report.annotations.requirement {
        let spec = parsed
            .get(&requirement.origin.src)
            .expect("requirements were validated while building annotations");
        let origins = normalize_ranges(&requirement.origin.ranges);
        let mut by_type: BTreeMap<&str, Vec<ByteRange>> = BTreeMap::new();
        let mut related = Vec::new();

        for (cite_id, ranges) in &requirement.coverage {
            let cite = report.annotations.cite.get(cite_id).ok_or_else(|| {
                duvet_core::error!(
                    "requirement '{}' coverage references missing cite '{}'",
                    requirement_id,
                    cite_id
                )
            })?;
            if cite.target.src != requirement.origin.src {
                return Err(duvet_core::error!(
                    "requirement '{}' coverage cite '{}' targets source '{}' instead of '{}'",
                    requirement_id,
                    cite_id,
                    cite.target.src,
                    requirement.origin.src
                ));
            }
            for range in ranges {
                validate_byte_range(
                    &spec.file,
                    range,
                    &format!("requirement '{requirement_id}' coverage for cite '{cite_id}'"),
                )?;
                if !range_is_contained(range, &origins)
                    || !range_is_contained(range, &normalize_ranges(&cite.target.ranges))
                {
                    return Err(duvet_core::error!(
                        "requirement '{}' coverage range {}..{} for cite '{}' is not contained in both the requirement and cite ranges",
                        requirement_id,
                        range.start,
                        range.end,
                        cite_id
                    ));
                }
            }
            by_type
                .entry(annotation_type_name(cite.anno_type))
                .or_default()
                .extend(ranges.iter().cloned());
            related.push(*stable_to_integer.get(cite_id).ok_or_else(|| {
                duvet_core::error!("cite '{}' has no converted annotation ID", cite_id)
            })?);
        }

        related.sort_unstable();
        related.dedup();
        let spec_count = range_len(&origins);
        let citation = normalized_len(by_type.get("CITATION"));
        let implication = normalized_len(by_type.get("IMPLICATION"));
        let test = normalized_len(by_type.get("TEST"));
        let exception = normalized_len(by_type.get("EXCEPTION"));
        let todo = normalized_len(by_type.get("TODO"));

        let mut completed = Vec::new();
        for kind in ["CITATION", "TEST", "IMPLICATION", "EXCEPTION"] {
            if let Some(ranges) = by_type.get(kind) {
                completed.extend(ranges.iter().cloned());
            }
        }
        let incomplete = difference_len(&origins, &normalize_ranges(&completed));
        let integer_id = *stable_to_integer.get(requirement_id).ok_or_else(|| {
            duvet_core::error!(
                "requirement '{}' has no converted annotation ID",
                requirement_id
            )
        })?;
        statuses.insert(
            integer_id,
            RequirementStatusV1 {
                spec: spec_count,
                incomplete,
                citation,
                implication,
                test,
                exception,
                todo,
                related,
            },
        );
    }

    Ok(statuses)
}

fn build_v1_specifications(
    parsed: &BTreeMap<String, ParsedSpecification>,
    ranges_by_source: &BTreeMap<String, Vec<RangeReference>>,
    requirement_ids: &BTreeMap<String, Vec<String>>,
    stable_to_integer: &HashMap<String, usize>,
) -> crate::Result<BTreeMap<String, SpecificationV1>> {
    let mut output = BTreeMap::new();

    for (source_id, parsed_spec) in parsed {
        let references = ranges_by_source
            .get(source_id)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let mut all_requirements = requirement_ids
            .get(source_id)
            .into_iter()
            .flatten()
            .map(|id| stable_to_integer[id])
            .collect::<Vec<_>>();
        all_requirements.sort_unstable();
        let mut sections = Vec::new();

        for section in parsed_spec.specification.sorted_sections() {
            let mut section_requirements = BTreeSet::new();
            let mut lines = Vec::new();
            for line in &section.lines {
                let Line::Str(line) = line else {
                    continue;
                };
                // Preserve the streaming v1 writer's behavior: it emits the
                // complete parser slice once for every source line it spans.
                for _ in line.line_range() {
                    lines.push(segment_line(
                        line.as_ref(),
                        line.range(),
                        references,
                        &mut section_requirements,
                    )?);
                }
            }
            sections.push(SectionV1 {
                id: section.id.clone(),
                title: section.title.clone(),
                lines,
                requirements: section_requirements.into_iter().collect(),
            });
        }

        output.insert(
            parsed_spec.identity.clone(),
            SpecificationV1 {
                title: parsed_spec.specification.title.clone(),
                format: parsed_spec.specification.format.to_string(),
                requirements: all_requirements,
                sections,
            },
        );
    }

    Ok(output)
}

fn segment_line(
    text: &str,
    line_range: Range<usize>,
    references: &[RangeReference],
    requirements: &mut BTreeSet<usize>,
) -> crate::Result<LineV1> {
    if text.is_empty() {
        return Ok(LineV1::Text(String::new()));
    }
    let overlapping: Vec<_> = references
        .iter()
        .filter(|reference| {
            reference.range.start < line_range.end && line_range.start < reference.range.end
        })
        .collect();
    if overlapping.is_empty() {
        return Ok(LineV1::Text(text.to_string()));
    }

    let mut start = line_range.start;
    let mut segments = Vec::new();
    while start < line_range.end {
        let mut end = line_range.end;
        let mut active = Vec::new();
        for reference in &overlapping {
            if reference.range.start <= start {
                if start < reference.range.end {
                    end = end.min(reference.range.end);
                    active.push(*reference);
                }
            } else {
                end = end.min(reference.range.start);
            }
        }
        if end <= start {
            return Err(duvet_core::error!(
                "could not advance while segmenting specification line at byte {}",
                start
            ));
        }
        active.sort_by_key(|reference| reference.annotation_id);
        active.dedup_by_key(|reference| reference.annotation_id);
        let annotation_ids: Vec<_> = active
            .iter()
            .map(|reference| {
                if reference.anno_type == "SPEC" {
                    requirements.insert(reference.annotation_id);
                }
                reference.annotation_id
            })
            .collect();
        let status_id = ref_status_id(&active);
        let local_start = start - line_range.start;
        let local_end = end - line_range.start;
        let segment_text = text.get(local_start..local_end).ok_or_else(|| {
            duvet_core::error!(
                "segment boundary {}..{} is not valid UTF-8 within line {}..{}",
                start,
                end,
                line_range.start,
                line_range.end
            )
        })?;
        segments.push((annotation_ids, status_id, segment_text.to_string()));
        start = end;
    }

    Ok(LineV1::Segments(segments))
}

fn all_ref_statuses() -> Vec<RefStatusV1> {
    // This order intentionally mirrors json.rs::RefStatus::id so converted
    // reports preserve the exact legacy wire layout, not just an internally
    // consistent table.
    let mut statuses = Vec::with_capacity(256);
    for level in ["AUTO", "MAY", "SHOULD", "MUST"] {
        for bits in 0..64 {
            statuses.push(RefStatusV1 {
                todo: bits & 1 != 0,
                exception: bits & 2 != 0,
                test: bits & 4 != 0,
                implication: bits & 8 != 0,
                citation: bits & 16 != 0,
                spec: bits & 32 != 0,
                level: level.to_string(),
            });
        }
    }
    statuses
}

fn ref_status_id(active: &[&RangeReference]) -> usize {
    let mut bits = 0;
    let mut level = 0;
    for reference in active {
        bits |= match reference.anno_type.as_str() {
            "TODO" => 1,
            "EXCEPTION" => 2,
            "TEST" => 4,
            "IMPLICATION" => 8,
            "CITATION" => 16,
            "SPEC" => 32,
            _ => 0,
        };
        level = level.max(level_index(&reference.level));
    }
    bits + level * 64
}

fn resolve_linked_source(
    report: &ReportV2,
    linked_id: &str,
    annotation_id: &str,
) -> crate::Result<(String, Option<String>)> {
    let linked = report.sources.linked.get(linked_id).ok_or_else(|| {
        duvet_core::error!(
            "annotation '{}' references missing linked source '{}'",
            annotation_id,
            linked_id
        )
    })?;
    let blob_link = match &linked.repository {
        Some(repository_id) => Some(
            report
                .repositories
                .get(repository_id)
                .ok_or_else(|| {
                    duvet_core::error!(
                        "linked source '{}' references missing repository '{}'",
                        linked_id,
                        repository_id
                    )
                })?
                .blob_link
                .clone(),
        ),
        None => None,
    };
    Ok((linked.file_name.clone(), blob_link))
}

fn infer_section<'a>(
    specification: &'a Specification,
    ranges: &[ByteRange],
    context: &str,
) -> crate::Result<&'a crate::specification::Section> {
    if ranges.is_empty() {
        return Err(duvet_core::error!(
            "{} has no target ranges, so its section cannot be inferred",
            context
        ));
    }
    let matches: Vec<_> = specification
        .sorted_sections()
        .into_iter()
        .filter(|section| {
            let extent = section_range(section);
            ranges
                .iter()
                .all(|range| extent.start <= range.start && range.end <= extent.end)
        })
        .collect();
    if matches.len() != 1 {
        return Err(duvet_core::error!(
            "{} is contained by {} sections; expected exactly one",
            context,
            matches.len()
        ));
    }
    Ok(matches[0])
}

fn section_range(section: &crate::specification::Section) -> Range<usize> {
    let start = section.full_title.range().start;
    let end = section
        .lines
        .iter()
        .filter_map(|line| match line {
            Line::Str(line) => Some(line.range().end),
            Line::Break => None,
        })
        .max()
        .unwrap_or(section.full_title.range().end);
    start..end
}

fn validate_ranges(file: &SourceFile, ranges: &SourceRanges, context: &str) -> crate::Result {
    for range in &ranges.ranges {
        validate_byte_range(file, range, context)?;
    }
    Ok(())
}

fn validate_byte_range(file: &SourceFile, range: &ByteRange, context: &str) -> crate::Result {
    if range.start > range.end || range.end > file.len() {
        return Err(duvet_core::error!(
            "{} has out-of-bounds range {}..{} for {}-byte source",
            context,
            range.start,
            range.end,
            file.len()
        ));
    }
    if file.get(range.start..range.end).is_none() {
        return Err(duvet_core::error!(
            "{} range {}..{} is not on UTF-8 boundaries",
            context,
            range.start,
            range.end
        ));
    }
    Ok(())
}

fn validate_source_ref(contents: &str, start: usize, end: usize, context: &str) -> crate::Result {
    if start > end || end > contents.len() {
        return Err(duvet_core::error!(
            "{} has out-of-bounds range {}..{} for {}-byte source",
            context,
            start,
            end,
            contents.len()
        ));
    }
    if contents.get(start..end).is_none() {
        return Err(duvet_core::error!(
            "{} range {}..{} is not on UTF-8 boundaries",
            context,
            start,
            end
        ));
    }
    Ok(())
}

fn specification_identity(annotation: &SpecificationAnnotation, file_name: &str) -> String {
    annotation
        .url
        .clone()
        .unwrap_or_else(|| file_name.to_string())
}

fn sorted_ranges(ranges: &[ByteRange]) -> Vec<ByteRange> {
    let mut ranges = ranges.to_vec();
    ranges.sort();
    ranges.dedup();
    ranges
}

fn normalize_ranges(ranges: &[ByteRange]) -> Vec<ByteRange> {
    let mut input = sorted_ranges(ranges);
    let mut output: Vec<ByteRange> = Vec::new();
    for range in input.drain(..) {
        if range.start == range.end {
            continue;
        }
        if let Some(last) = output.last_mut() {
            if range.start <= last.end {
                last.end = last.end.max(range.end);
                continue;
            }
        }
        output.push(range);
    }
    output
}

fn range_len(ranges: &[ByteRange]) -> usize {
    ranges.iter().map(|range| range.end - range.start).sum()
}

fn normalized_len(ranges: Option<&Vec<ByteRange>>) -> usize {
    ranges.map_or(0, |ranges| range_len(&normalize_ranges(ranges)))
}

fn difference_len(origin: &[ByteRange], covered: &[ByteRange]) -> usize {
    let mut total = 0;
    for origin in origin {
        let mut cursor = origin.start;
        for covered in covered {
            if covered.end <= cursor || covered.start >= origin.end {
                continue;
            }
            total += covered.start.min(origin.end).saturating_sub(cursor);
            cursor = cursor.max(covered.end.min(origin.end));
            if cursor >= origin.end {
                break;
            }
        }
        total += origin.end.saturating_sub(cursor);
    }
    total
}

fn range_is_contained(range: &ByteRange, containers: &[ByteRange]) -> bool {
    containers
        .iter()
        .any(|container| container.start <= range.start && range.end <= container.end)
}

fn join_range_text(file: &SourceFile, ranges: &[ByteRange]) -> String {
    sorted_ranges(ranges)
        .iter()
        .map(|range| &file[range.start..range.end])
        .collect::<Vec<_>>()
        .join("\n")
}

fn annotation_type_name(anno_type: AnnotationType) -> &'static str {
    match anno_type {
        AnnotationType::Citation => "CITATION",
        AnnotationType::Test => "TEST",
        AnnotationType::Implication => "IMPLICATION",
        AnnotationType::Exception => "EXCEPTION",
        AnnotationType::Todo => "TODO",
    }
}

fn level_name(level: AnnotationLevel) -> &'static str {
    match level {
        AnnotationLevel::Auto => "AUTO",
        AnnotationLevel::May => "MAY",
        AnnotationLevel::Should => "SHOULD",
        AnnotationLevel::Must => "MUST",
    }
}

fn level_index(level: &str) -> usize {
    match level {
        "MAY" => 1,
        "SHOULD" => 2,
        "MUST" => 3,
        _ => 0,
    }
}

fn nonempty(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
}

fn select_issue_link(
    report: &ReportV2,
    issue_link_override: Option<&str>,
) -> (Option<String>, Option<String>) {
    if let Some(issue_link) = issue_link_override {
        return (Some(issue_link.to_string()), None);
    }
    match report.issue_links.as_slice() {
        [] => (None, None),
        [issue_link] => (Some(issue_link.clone()), None),
        links => (
            None,
            Some(format!(
                "input contains {} issue links; v1 supports only one, so none was selected (use --issue-link to choose one)",
                links.len()
            )),
        ),
    }
}

/// Compare two v1 reports after removing ordering and integer-ID artifacts.
pub fn validate_semantics(
    direct: &ReportV1,
    converted: &ReportV1,
    compare_issue_link: bool,
) -> crate::Result {
    let direct = canonicalize(direct, compare_issue_link)?;
    let converted = canonicalize(converted, compare_issue_link)?;
    if direct == converted {
        return Ok(());
    }
    let path = first_difference(&direct, &converted, "$");
    Err(duvet_core::error!(
        "converted report does not match direct v1 report at {}",
        path
    ))
}

fn canonicalize(report: &ReportV1, compare_issue_link: bool) -> crate::Result<Value> {
    let spec_aliases = specification_aliases(report);
    let mut annotation_keys = Vec::with_capacity(report.annotations.len());
    let mut annotations = BTreeMap::new();
    let mut annotation_meta = BTreeMap::new();

    for annotation in &report.annotations {
        let target = canonical_target(&annotation.target_path);
        let section =
            canonical_section(&target, annotation.target_section.as_deref(), &spec_aliases);
        let key = semantic_annotation_key(annotation, &target, section.as_deref());
        let effective_blob = annotation
            .blob_link
            .as_ref()
            .or(report.blob_link.as_ref())
            .cloned();
        let mut tags = annotation.tags.clone();
        tags.sort();
        let comment = (annotation.anno_type != "SPEC")
            .then(|| annotation.comment.clone())
            .flatten();
        let value = json!({
            "source": annotation.source,
            "line": annotation.line,
            "type": annotation.anno_type,
            "level": annotation.level,
            "target": target,
            "section": section,
            "comment": comment,
            "feature": annotation.feature,
            "tracking_issue": annotation.tracking_issue,
            "blob_link": effective_blob,
            "tags": tags,
        });
        if annotations.insert(key.clone(), value).is_some() {
            return Err(duvet_core::error!(
                "v1 report contains duplicate semantic annotation key '{}'",
                key
            ));
        }
        annotation_meta.insert(
            key.clone(),
            CanonicalAnnotationMeta {
                anno_type: annotation.anno_type.clone(),
                level: annotation.level.clone(),
            },
        );
        annotation_keys.push(key);
    }

    let mut statuses = BTreeMap::new();
    for (id, status) in &report.statuses {
        let key = annotation_keys.get(*id).ok_or_else(|| {
            duvet_core::error!("v1 status references missing annotation ID {}", id)
        })?;
        let mut related = status
            .related
            .iter()
            .map(|id| {
                annotation_keys.get(*id).cloned().ok_or_else(|| {
                    duvet_core::error!("v1 status references missing related annotation ID {}", id)
                })
            })
            .collect::<crate::Result<Vec<_>>>()?;
        related.sort();
        statuses.insert(
            key.clone(),
            json!({
                "spec": status.spec,
                "incomplete": status.incomplete,
                "citation": status.citation,
                "implication": status.implication,
                "test": status.test,
                "exception": status.exception,
                "todo": status.todo,
                "related": related,
            }),
        );
    }

    let mut specification_accumulators: BTreeMap<String, CanonicalSpecification> = BTreeMap::new();
    for (identity, specification) in &report.specifications {
        let canonical_identity = canonical_target(identity);
        let requirements = canonical_ids(&specification.requirements, &annotation_keys)?;
        let mut incoming_sections = Vec::new();
        for section in &specification.sections {
            let section_id =
                canonical_section(&canonical_identity, Some(&section.id), &spec_aliases)
                    .unwrap_or_else(|| section.id.clone());
            let section_requirements = canonical_ids(&section.requirements, &annotation_keys)?;
            let mut lines = Vec::new();
            for (line_index, line) in section.lines.iter().enumerate() {
                lines.push(canonical_line_overlay(
                    line,
                    report,
                    &annotation_keys,
                    &annotation_meta,
                    &format!("spec[{canonical_identity}]/section[{section_id}]/line[{line_index}]"),
                )?);
            }
            incoming_sections.push(CanonicalSection {
                id: section_id,
                title: section.title.clone(),
                requirements: section_requirements.into_iter().collect(),
                lines,
            });
        }
        merge_canonical_specification(
            &canonical_identity,
            specification_accumulators
                .entry(canonical_identity.clone())
                .or_insert_with(|| CanonicalSpecification {
                    title: specification.title.clone(),
                    format: specification.format.clone(),
                    requirements: BTreeSet::new(),
                    sections: Vec::new(),
                }),
            requirements,
            incoming_sections,
            specification,
        )?;
    }

    let mut specifications = BTreeMap::new();
    for (identity, specification) in specification_accumulators {
        let sections = specification
            .sections
            .into_iter()
            .map(|section| {
                let lines = section
                    .lines
                    .into_iter()
                    .map(|line| render_canonical_line(line, &annotation_meta))
                    .collect::<crate::Result<Vec<_>>>()?;
                Ok(json!({
                    "id": section.id,
                    "title": section.title,
                    "requirements": section.requirements,
                    "lines": lines,
                }))
            })
            .collect::<crate::Result<Vec<_>>>()?;
        specifications.insert(
            format!("spec[{identity}]"),
            json!({
                "title": specification.title,
                "format": specification.format,
                "requirements": specification.requirements,
                "sections": sections,
            }),
        );
    }

    Ok(json!({
        "issue_link": compare_issue_link.then(|| report.issue_link.clone()).flatten(),
        "annotations": annotations,
        "statuses": statuses,
        "specifications": specifications,
    }))
}

fn specification_aliases(report: &ReportV1) -> BTreeMap<String, BTreeMap<String, String>> {
    report
        .specifications
        .iter()
        .map(|(identity, specification)| {
            let sections = specification
                .sections
                .iter()
                .map(|section| {
                    let stripped = section
                        .id
                        .trim_start_matches("section-")
                        .trim_start_matches("appendix-");
                    (stripped.to_string(), section.id.clone())
                })
                .collect();
            (canonical_target(identity), sections)
        })
        .collect()
}

fn canonical_target(target: &str) -> String {
    if target.contains("://") {
        TargetPath::canonical_url(target).into_owned()
    } else {
        target.to_string()
    }
}

fn canonical_section(
    target: &str,
    section: Option<&str>,
    aliases: &BTreeMap<String, BTreeMap<String, String>>,
) -> Option<String> {
    let section = section?;
    let stripped = section
        .trim_start_matches("section-")
        .trim_start_matches("appendix-");
    aliases
        .get(target)
        .and_then(|aliases| aliases.get(stripped))
        .cloned()
        .or_else(|| Some(section.to_string()))
}

fn semantic_annotation_key(
    annotation: &AnnotationV1,
    target: &str,
    section: Option<&str>,
) -> String {
    format!(
        "{}:{}:{}:{}:{}",
        annotation.source,
        annotation.line,
        annotation.anno_type,
        target,
        section.unwrap_or_default()
    )
}

fn canonical_ids(ids: &[usize], annotation_keys: &[String]) -> crate::Result<Vec<String>> {
    let mut output = ids
        .iter()
        .map(|id| {
            annotation_keys.get(*id).cloned().ok_or_else(|| {
                duvet_core::error!("v1 report references missing annotation ID {}", id)
            })
        })
        .collect::<crate::Result<Vec<_>>>()?;
    output.sort();
    output.dedup();
    Ok(output)
}

fn canonical_line_overlay(
    line: &LineV1,
    report: &ReportV1,
    annotation_keys: &[String],
    annotation_meta: &BTreeMap<String, CanonicalAnnotationMeta>,
    context: &str,
) -> crate::Result<CanonicalLine> {
    let mut text = String::new();
    let mut overlays = Vec::new();
    match line {
        LineV1::Text(value) => text.push_str(value),
        LineV1::Segments(segments) => {
            for (index, (ids, status_id, segment_text)) in segments.iter().enumerate() {
                let annotations = canonical_ids(ids, annotation_keys)?;
                let status = report.refs.get(*status_id).ok_or_else(|| {
                    duvet_core::error!(
                        "{} segment[{}] references missing ref status {}",
                        context,
                        index,
                        status_id
                    )
                })?;
                let expected = canonical_status_for_annotations(&annotations, annotation_meta)?;
                let actual = canonical_ref_status(status);
                if expected != actual {
                    return Err(duvet_core::error!(
                        "{} segment[{}] status does not match its annotations",
                        context,
                        index
                    ));
                }
                let start = text.len();
                let end = start + segment_text.len();
                if !annotations.is_empty() {
                    overlays.push((start, end, annotations));
                }
                text.push_str(segment_text);
            }
        }
    }
    Ok(CanonicalLine { text, overlays })
}

fn canonical_ref_status(status: &RefStatusV1) -> Value {
    json!({
        "spec": status.spec,
        "citation": status.citation,
        "implication": status.implication,
        "test": status.test,
        "exception": status.exception,
        "todo": status.todo,
        "level": if status.level.is_empty() { "AUTO" } else { &status.level },
    })
}

fn canonical_status_for_annotations(
    annotations: &[String],
    metadata: &BTreeMap<String, CanonicalAnnotationMeta>,
) -> crate::Result<Value> {
    let mut spec = false;
    let mut citation = false;
    let mut implication = false;
    let mut test = false;
    let mut exception = false;
    let mut todo = false;
    let mut level = 0;
    for annotation in annotations {
        let metadata = metadata.get(annotation).ok_or_else(|| {
            duvet_core::error!("missing canonical annotation metadata for '{}'", annotation)
        })?;
        match metadata.anno_type.as_str() {
            "SPEC" => spec = true,
            "CITATION" => citation = true,
            "IMPLICATION" => implication = true,
            "TEST" => test = true,
            "EXCEPTION" => exception = true,
            "TODO" => todo = true,
            other => {
                return Err(duvet_core::error!(
                    "unsupported v1 annotation type '{}'",
                    other
                ));
            }
        }
        level = level.max(level_index(&metadata.level));
    }
    let level = ["AUTO", "MAY", "SHOULD", "MUST"][level];
    Ok(json!({
        "spec": spec,
        "citation": citation,
        "implication": implication,
        "test": test,
        "exception": exception,
        "todo": todo,
        "level": level,
    }))
}

fn merge_canonical_specification(
    identity: &str,
    output: &mut CanonicalSpecification,
    requirements: Vec<String>,
    mut sections: Vec<CanonicalSection>,
    input: &SpecificationV1,
) -> crate::Result {
    if output.title != input.title || output.format != input.format {
        return Err(duvet_core::error!(
            "canonical specification '{}' has conflicting title or format",
            identity
        ));
    }
    output.requirements.extend(requirements);
    if output.sections.is_empty() {
        output.sections = sections;
        return Ok(());
    }
    if output.sections.len() != sections.len() {
        return Err(duvet_core::error!(
            "canonical specification '{}' has conflicting section counts",
            identity
        ));
    }
    for (index, (existing, incoming)) in output
        .sections
        .iter_mut()
        .zip(sections.iter_mut())
        .enumerate()
    {
        if existing.id != incoming.id
            || existing.title != incoming.title
            || existing.lines.len() != incoming.lines.len()
        {
            return Err(duvet_core::error!(
                "canonical specification '{}', section {} has conflicting structure",
                identity,
                index
            ));
        }
        existing.requirements.append(&mut incoming.requirements);
        for (line_index, (existing_line, incoming_line)) in existing
            .lines
            .iter_mut()
            .zip(incoming.lines.iter_mut())
            .enumerate()
        {
            if existing_line.text != incoming_line.text {
                return Err(duvet_core::error!(
                    "canonical specification '{}', section '{}', line {} has conflicting text",
                    identity,
                    existing.id,
                    line_index
                ));
            }
            existing_line.overlays.append(&mut incoming_line.overlays);
        }
    }
    Ok(())
}

fn render_canonical_line(
    line: CanonicalLine,
    metadata: &BTreeMap<String, CanonicalAnnotationMeta>,
) -> crate::Result<Value> {
    if line.text.is_empty() {
        return Ok(Value::Array(vec![json!({
            "annotations": Vec::<String>::new(),
            "status": canonical_status_for_annotations(&[], metadata)?,
            "text": "",
        })]));
    }
    let mut boundaries = BTreeSet::from([0, line.text.len()]);
    for (start, end, _) in &line.overlays {
        boundaries.insert(*start);
        boundaries.insert(*end);
    }
    let boundaries: Vec<_> = boundaries.into_iter().collect();
    let mut segments = Vec::new();
    for window in boundaries.windows(2) {
        let start = window[0];
        let end = window[1];
        let mut annotations = BTreeSet::new();
        for (overlay_start, overlay_end, overlay_annotations) in &line.overlays {
            if *overlay_start <= start && end <= *overlay_end {
                annotations.extend(overlay_annotations.iter().cloned());
            }
        }
        let annotations: Vec<_> = annotations.into_iter().collect();
        segments.push(json!({
            "status": canonical_status_for_annotations(&annotations, metadata)?,
            "annotations": annotations,
            "text": &line.text[start..end],
        }));
    }
    Ok(Value::Array(segments))
}

fn first_difference(left: &Value, right: &Value, path: &str) -> String {
    match (left, right) {
        (Value::Object(left), Value::Object(right)) => {
            let keys: BTreeSet<_> = left.keys().chain(right.keys()).collect();
            for key in keys {
                match (left.get(key), right.get(key)) {
                    (Some(left), Some(right)) if left != right => {
                        return first_difference(left, right, &format!("{path}/{key}"));
                    }
                    (Some(_), Some(_)) => {}
                    (left, right) => {
                        return format!(
                            "{path}/{key} (direct={}, converted={})",
                            left.map_or_else(|| "<missing>".to_string(), Value::to_string),
                            right.map_or_else(|| "<missing>".to_string(), Value::to_string)
                        );
                    }
                }
            }
            path.to_string()
        }
        (Value::Array(left), Value::Array(right)) => {
            for index in 0..left.len().max(right.len()) {
                match (left.get(index), right.get(index)) {
                    (Some(left), Some(right)) if left != right => {
                        return first_difference(left, right, &format!("{path}[{index}]"));
                    }
                    (Some(_), Some(_)) => {}
                    _ => return format!("{path}[{index}]"),
                }
            }
            path.to_string()
        }
        _ => format!("{path} (direct={left}, converted={right})"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::json_v2::{
        AnnotationsV2, CiteAnnotation, InlineSource, LinkedSource, RequirementAnnotation,
        SectionAnnotation, SourceLocation, SourceRef, SourcesV2,
    };

    #[test]
    fn ref_table_has_stable_legacy_order() {
        let refs = all_ref_statuses();
        assert_eq!(refs.len(), 256);
        assert!(refs[16].citation);
        assert!(refs[32].spec);
        assert_eq!(refs[240].level, "MUST");
        assert!(refs[240].spec);
        assert!(refs[240].citation);
        assert!(!refs[240].implication);
        assert!(!refs[240].test);
    }

    #[test]
    fn interval_difference_uses_union_semantics() {
        let origin = vec![ByteRange { start: 0, end: 10 }];
        let covered = normalize_ranges(&[
            ByteRange { start: 1, end: 4 },
            ByteRange { start: 3, end: 8 },
        ]);
        assert_eq!(difference_len(&origin, &covered), 3);
    }

    #[test]
    fn converts_disjoint_unicode_ranges_and_all_annotation_types() {
        let report = fixture();
        let (converted, warning) = convert(&report, None).unwrap();
        assert!(warning.is_none());
        assert_eq!(converted.annotations.len(), 6);
        let requirement = converted
            .annotations
            .iter()
            .position(|annotation| annotation.anno_type == "SPEC")
            .unwrap();
        assert_eq!(
            converted.annotations[requirement].comment.as_deref(),
            Some("alpha\ngamma")
        );
        let status = &converted.statuses[&requirement];
        assert_eq!(status.spec, 10);
        assert_eq!(status.incomplete, 5);
        assert_eq!(status.citation, 5);
        assert_eq!(status.test, 5);
        assert_eq!(status.implication, 5);
        assert_eq!(status.exception, 5);
        assert_eq!(status.todo, 5);
        assert_eq!(status.related.len(), 5);
        assert_eq!(converted.refs.len(), 256);
    }

    #[test]
    fn converted_markdown_snapshot() {
        let (mut converted, _) = convert(&fixture(), None).unwrap();
        // The stable 256-entry table has a dedicated ordering test; omitting
        // it keeps this focused snapshot readable.
        converted.refs.clear();
        insta::assert_json_snapshot!(converted);
    }

    #[test]
    fn empty_quote_does_not_add_a_section_requirement() {
        let mut report = fixture();
        let inline = &report.sources.inline["src-1"];
        let file = SourceFile::new(inline.file_name.clone(), inline.contents.clone()).unwrap();
        let specification = Format::Markdown.parse(&file).unwrap();
        let title_range = specification.section("section").unwrap().full_title.range();
        let requirement = report.annotations.requirement.get_mut("req-1").unwrap();
        requirement.origin.ranges = vec![ByteRange {
            start: title_range.start,
            end: title_range.end,
        }];
        requirement.coverage.clear();

        let (converted, _) = convert(&report, None).unwrap();
        let requirement_id = converted
            .annotations
            .iter()
            .position(|annotation| annotation.anno_type == "SPEC")
            .unwrap();
        let specification = &converted.specifications["spec.md"];
        assert_eq!(specification.requirements, [requirement_id]);
        assert!(specification
            .sections
            .iter()
            .all(|section| !section.requirements.contains(&requirement_id)));
    }

    #[test]
    fn converts_ietf_page_breaks_and_validates_url_and_section_aliases() {
        let contents = concat!(
            "1. Overview\n",
            "\n",
            "alpha part\n",
            "\u{C}\n",
            "RFC 9999  Example\n",
            "\n",
            "beta part\n",
        )
        .to_string();
        let alpha = contents.find("alpha part").unwrap();
        let beta = contents.find("beta part").unwrap();
        let ranges = vec![
            ByteRange {
                start: alpha,
                end: alpha + "alpha part".len(),
            },
            ByteRange {
                start: beta,
                end: beta + "beta part".len(),
            },
        ];
        let mut report = base_fixture(
            contents,
            "rfc9999.txt",
            Format::Ietf,
            Some("https://www.rfc-editor.org/rfc/rfc9999.txt"),
        );
        report.annotations.requirement.insert(
            "req-ietf".to_string(),
            RequirementAnnotation {
                source: SourceLocation {
                    src: "lnk-requirement".to_string(),
                    line: Some(7),
                },
                origin: SourceRanges {
                    src: "src-1".to_string(),
                    ranges: ranges.clone(),
                },
                level: AnnotationLevel::Must,
                coverage: BTreeMap::from([("cite-ietf".to_string(), ranges.clone())]),
            },
        );
        report.annotations.cite.insert(
            "cite-ietf".to_string(),
            CiteAnnotation {
                source: SourceLocation {
                    src: "lnk-code".to_string(),
                    line: Some(1),
                },
                target: SourceRanges {
                    src: "src-1".to_string(),
                    ranges,
                },
                anno_type: AnnotationType::Citation,
                level: AnnotationLevel::Must,
                comment: None,
                feature: None,
                tracking_issue: None,
                tags: Vec::new(),
            },
        );

        let (converted, _) = convert(&report, None).unwrap();
        let canonical_url = "https://www.rfc-editor.org/rfc/rfc9999.txt";
        let specification = &converted.specifications[canonical_url];
        let section = specification
            .sections
            .iter()
            .find(|section| section.id == "section-1")
            .unwrap();
        let text = section
            .lines
            .iter()
            .map(|line| match line {
                LineV1::Text(text) => text.clone(),
                LineV1::Segments(segments) => {
                    segments.iter().map(|(_, _, text)| text.as_str()).collect()
                }
            })
            .collect::<Vec<String>>()
            .join("\n");
        assert!(text.contains("alpha part"));
        assert!(text.contains("beta part"));
        assert!(!text.contains("RFC 9999"));

        let mut authored_v1 = converted.clone();
        let specification = authored_v1.specifications.remove(canonical_url).unwrap();
        authored_v1.specifications.insert(
            "https://www.rfc-editor.org/rfc/rfc9999.html".to_string(),
            specification,
        );
        for annotation in &mut authored_v1.annotations {
            annotation.target_path = "https://www.rfc-editor.org/rfc/rfc9999.html".to_string();
            annotation.target_section = Some("1".to_string());
        }
        validate_semantics(&authored_v1, &converted, true).unwrap();
    }

    #[test]
    fn rejects_dangling_coverage_reference() {
        let mut report = fixture();
        report
            .annotations
            .requirement
            .get_mut("req-1")
            .unwrap()
            .coverage
            .insert(
                "cite-missing".to_string(),
                vec![ByteRange { start: 20, end: 25 }],
            );
        let error = convert(&report, None).unwrap_err().to_string();
        assert!(error.contains("missing cite"), "{error}");
    }

    #[test]
    fn rejects_invalid_unicode_boundary() {
        let mut report = fixture();
        let beta = report.sources.inline["src-1"].contents.find('β').unwrap();
        report
            .annotations
            .requirement
            .get_mut("req-1")
            .unwrap()
            .origin
            .ranges = vec![ByteRange {
            start: beta + 1,
            end: beta + 2,
        }];
        let error = convert(&report, None).unwrap_err().to_string();
        assert!(error.contains("UTF-8"), "{error}");
    }

    #[test]
    fn rejects_out_of_bounds_range() {
        let mut report = fixture();
        report
            .annotations
            .cite
            .get_mut("cite-citation")
            .unwrap()
            .target
            .ranges = vec![ByteRange {
            start: 0,
            end: usize::MAX,
        }];
        let error = convert(&report, None).unwrap_err().to_string();
        assert!(error.contains("out-of-bounds"), "{error}");
    }

    #[test]
    fn rejects_parser_drift_and_extensions() {
        let mut drift = fixture();
        drift
            .annotations
            .section
            .values_mut()
            .next()
            .unwrap()
            .short_name = "changed".to_string();
        let error = convert(&drift, None).unwrap_err().to_string();
        assert!(error.contains("parser drift"), "{error}");

        let mut extension = fixture();
        extension
            .sources
            .extensions
            .insert("https://example.com/custom".to_string(), json!({}));
        let error = convert(&extension, None).unwrap_err().to_string();
        assert!(error.contains("extension"), "{error}");
    }

    #[test]
    fn multiple_issue_links_warn_unless_overridden() {
        let mut report = fixture();
        report.issue_links = vec!["https://a.example".into(), "https://b.example".into()];
        let (converted, warning) = convert(&report, None).unwrap();
        assert_eq!(converted.issue_link, None);
        assert!(warning.unwrap().contains("2 issue links"));

        let (converted, warning) = convert(&report, Some("https://override.example")).unwrap();
        assert_eq!(
            converted.issue_link.as_deref(),
            Some("https://override.example")
        );
        assert!(warning.is_none());
    }

    fn fixture() -> ReportV2 {
        let contents = "# Spec\n\n## Section\n\nalpha β gamma\n".to_string();
        let alpha = contents.find("alpha").unwrap();
        let gamma = contents.find("gamma").unwrap();
        let mut report = base_fixture(contents, "spec.md", Format::Markdown, None);

        let cite_types = [
            ("cite-citation", AnnotationType::Citation),
            ("cite-test", AnnotationType::Test),
            ("cite-implication", AnnotationType::Implication),
            ("cite-exception", AnnotationType::Exception),
            ("cite-todo", AnnotationType::Todo),
        ];
        let mut cites = BTreeMap::new();
        let mut coverage = BTreeMap::new();
        for (index, (id, anno_type)) in cite_types.into_iter().enumerate() {
            let range = ByteRange {
                start: alpha,
                end: alpha + 5,
            };
            coverage.insert(id.to_string(), vec![range.clone()]);
            cites.insert(
                id.to_string(),
                CiteAnnotation {
                    source: SourceLocation {
                        src: "lnk-code".to_string(),
                        line: Some(index + 1),
                    },
                    target: SourceRanges {
                        src: "src-1".to_string(),
                        ranges: vec![range],
                    },
                    anno_type,
                    level: AnnotationLevel::Must,
                    comment: None,
                    feature: None,
                    tracking_issue: None,
                    tags: Vec::new(),
                },
            );
        }

        report.annotations.requirement.insert(
            "req-1".to_string(),
            RequirementAnnotation {
                source: SourceLocation {
                    src: "lnk-requirement".to_string(),
                    line: Some(7),
                },
                origin: SourceRanges {
                    src: "src-1".to_string(),
                    ranges: vec![
                        ByteRange {
                            start: alpha,
                            end: alpha + 5,
                        },
                        ByteRange {
                            start: gamma,
                            end: gamma + 5,
                        },
                    ],
                },
                level: AnnotationLevel::Must,
                coverage,
            },
        );
        report.annotations.cite = cites;
        report
    }

    fn base_fixture(
        contents: String,
        file_name: &str,
        format: Format,
        url: Option<&str>,
    ) -> ReportV2 {
        let file = SourceFile::new(file_name, contents.clone()).unwrap();
        let specification = format.parse(&file).unwrap();
        let mut sections = BTreeMap::new();
        for (index, section) in specification.sorted_sections().into_iter().enumerate() {
            let range = section_range(section);
            sections.insert(
                format!("sec-{index}"),
                SectionAnnotation {
                    source: SourceRef {
                        src: "src-1".to_string(),
                        start: range.start,
                        end: range.end,
                    },
                    short_name: section.id.clone(),
                    long_name: Some(section.title.clone()),
                },
            );
        }

        ReportV2 {
            version: "2.0".to_string(),
            issue_links: Vec::new(),
            repositories: BTreeMap::new(),
            sources: SourcesV2 {
                inline: BTreeMap::from([(
                    "src-1".to_string(),
                    InlineSource {
                        file_name: file_name.to_string(),
                        contents: contents.clone(),
                    },
                )]),
                linked: BTreeMap::from([
                    (
                        "lnk-requirement".to_string(),
                        LinkedSource {
                            file_name: "requirements.toml".to_string(),
                            repository: None,
                        },
                    ),
                    (
                        "lnk-code".to_string(),
                        LinkedSource {
                            file_name: "src/lib.rs".to_string(),
                            repository: None,
                        },
                    ),
                ]),
                extensions: BTreeMap::new(),
            },
            annotations: AnnotationsV2 {
                specification: BTreeMap::from([(
                    "spc-1".to_string(),
                    SpecificationAnnotation {
                        source: SourceRef {
                            src: "src-1".to_string(),
                            start: 0,
                            end: contents.len(),
                        },
                        title: specification.title.clone(),
                        format: specification.format.to_string(),
                        url: url.map(str::to_string),
                    },
                )]),
                section: sections,
                requirement: BTreeMap::new(),
                cite: BTreeMap::new(),
                extensions: BTreeMap::new(),
            },
        }
    }
}
