// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! JSON v2 format for duvet reports.
//!
//! This module provides a roundtrip-friendly JSON format with entity-typed
//! deterministic IDs, enabling multi-package report merging.
//!
//! ## Schema-URL JSON keys
//!
//! `SourcesV2` and `AnnotationsV2` are serialized with full schema URLs as
//! keys (e.g. `"https://awslabs.github.io/duvet/v2/sources.json#inline"`)
//! rather than plain field names. This is an intentional design choice by
//! the format owners to make the document self-addressing: each nested
//! collection advertises the schema it conforms to, so the JSON can be
//! validated and navigated without out-of-band context.

use crate::{ids, report::ReportResult};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

// ── Top-level structure ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ReportV2 {
    pub version: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub issue_links: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub repositories: BTreeMap<String, Repository>,
    pub sources: SourcesV2,
    pub annotations: AnnotationsV2,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Repository {
    pub blob_link: String,
}

// ── Sources ──────────────────────────────────────────────────────────────────

/// Container for source entries keyed by schema URL. Each known bucket
/// (`#inline`, `#linked`) is optional, and unknown URL-keyed buckets are
/// preserved verbatim in `extensions` so consumers and merge tooling can
/// roundtrip new source types added by future schema revisions or
/// out-of-band extensions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct SourcesV2 {
    #[serde(
        rename = "https://awslabs.github.io/duvet/v2/sources.json#inline",
        default
    )]
    pub inline: BTreeMap<String, InlineSource>,
    #[serde(
        rename = "https://awslabs.github.io/duvet/v2/sources.json#linked",
        default
    )]
    pub linked: BTreeMap<String, LinkedSource>,
    #[serde(flatten)]
    pub extensions: BTreeMap<String, serde_json::Value>,
}

/// A specification source file whose full contents are embedded in the report.
///
/// `file_name` is a path relative to the project root
/// (e.g. `specs/rfc7541.txt`), matching the `LinkedSource.file_name`
/// contract.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct InlineSource {
    pub file_name: String,
    pub contents: String,
}

/// A code or authoring source file referenced by path (and optionally by the
/// repository it lives in). Contents are not embedded; consumers resolve
/// `file_name` against the referenced `repository.blob_link` or local
/// filesystem.
///
/// `file_name` is a path relative to the project root, matching the
/// `InlineSource.file_name` contract.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct LinkedSource {
    pub file_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
}

// ── Annotations ──────────────────────────────────────────────────────────────

/// Container for annotations keyed by schema URL. Each known bucket
/// (`#specification`, `#section`, `#requirement`, `#cite`) is optional, and
/// unknown URL-keyed buckets are preserved verbatim in `extensions` so
/// consumers and merge tooling can roundtrip new annotation types added by
/// future schema revisions or out-of-band extensions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct AnnotationsV2 {
    #[serde(
        rename = "https://awslabs.github.io/duvet/v2/annotations.json#specification",
        default
    )]
    pub specification: BTreeMap<String, SpecificationAnnotation>,
    #[serde(
        rename = "https://awslabs.github.io/duvet/v2/annotations.json#section",
        default
    )]
    pub section: BTreeMap<String, SectionAnnotation>,
    #[serde(
        rename = "https://awslabs.github.io/duvet/v2/annotations.json#requirement",
        default
    )]
    pub requirement: BTreeMap<String, RequirementAnnotation>,
    #[serde(
        rename = "https://awslabs.github.io/duvet/v2/annotations.json#cite",
        default
    )]
    pub cite: BTreeMap<String, CiteAnnotation>,
    #[serde(flatten)]
    pub extensions: BTreeMap<String, serde_json::Value>,
}

// ── Shared types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct SourceRef {
    pub src: String,
    pub start: usize,
    pub end: usize,
}

/// A reference to one or more (possibly disjoint) byte ranges within an
/// inline source file. Used wherever a matched quote can span multiple
/// non-contiguous regions of the spec (e.g., across IETF RFC page breaks).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct SourceRanges {
    pub src: String,
    pub ranges: Vec<ByteRange>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct SourceLocation {
    pub src: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ByteRange {
    pub start: usize,
    pub end: usize,
}

// ── Annotation types ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct SpecificationAnnotation {
    pub source: SourceRef,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub format: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct SectionAnnotation {
    pub source: SourceRef,
    pub short_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub long_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct RequirementAnnotation {
    /// Authoring site: where this requirement was declared (TOML file or
    /// inline `//= type=spec` comment).
    pub source: SourceLocation,
    /// Spec byte range(s) this requirement represents. May contain multiple
    /// disjoint ranges when the matched quote spans regions the spec parser
    /// normalized away (e.g., IETF RFC page breaks).
    pub origin: SourceRanges,
    pub level: AnnotationLevel,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub coverage: BTreeMap<String, Vec<ByteRange>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct CiteAnnotation {
    pub source: SourceLocation,
    /// Matched byte range(s) within the target specification file. May
    /// contain multiple disjoint ranges when the quote spans regions the
    /// spec parser normalized away (e.g., IETF RFC page breaks).
    pub target: SourceRanges,
    #[serde(rename = "type")]
    pub anno_type: AnnotationType,
    #[serde(default)]
    pub level: AnnotationLevel,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tracking_issue: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

/// Annotation type for cite annotations (v2 format).
///
/// Note: `Spec` is not included — spec-derived annotations are represented by
/// `SpecificationAnnotation`, `SectionAnnotation`, and `RequirementAnnotation`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub enum AnnotationType {
    #[serde(rename = "CITATION")]
    #[default]
    Citation,
    #[serde(rename = "TEST")]
    Test,
    #[serde(rename = "IMPLICATION")]
    Implication,
    #[serde(rename = "EXCEPTION")]
    Exception,
    #[serde(rename = "TODO")]
    Todo,
}

/// Annotation level for v2 format.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub enum AnnotationLevel {
    #[serde(rename = "AUTO")]
    #[default]
    Auto,
    #[serde(rename = "MAY")]
    May,
    #[serde(rename = "SHOULD")]
    Should,
    #[serde(rename = "MUST")]
    Must,
}

impl From<crate::annotation::AnnotationLevel> for AnnotationLevel {
    fn from(level: crate::annotation::AnnotationLevel) -> Self {
        match level {
            crate::annotation::AnnotationLevel::Auto => Self::Auto,
            crate::annotation::AnnotationLevel::May => Self::May,
            crate::annotation::AnnotationLevel::Should => Self::Should,
            crate::annotation::AnnotationLevel::Must => Self::Must,
        }
    }
}

impl TryFrom<crate::annotation::AnnotationType> for AnnotationType {
    type Error = ();

    /// Converts an internal annotation type to the v2 impl annotation type.
    ///
    /// Returns `Err(())` for `Spec`: spec-derived annotations are represented
    /// by `SpecificationAnnotation`, `SectionAnnotation`, and
    /// `RequirementAnnotation`, not by `CiteAnnotation`. Callers in Step 5
    /// use this to filter Spec references out of the impl pipeline.
    fn try_from(anno_type: crate::annotation::AnnotationType) -> Result<Self, Self::Error> {
        Ok(match anno_type {
            crate::annotation::AnnotationType::Citation => Self::Citation,
            crate::annotation::AnnotationType::Test => Self::Test,
            crate::annotation::AnnotationType::Implication => Self::Implication,
            crate::annotation::AnnotationType::Exception => Self::Exception,
            crate::annotation::AnnotationType::Todo => Self::Todo,
            crate::annotation::AnnotationType::Spec => return Err(()),
        })
    }
}

// ── Report construction ──────────────────────────────────────────────────────

// Grouping key for Step 6: (origin src-id, authoring lnk-id, anno_line).
// One key identifies a single logical requirement authoring site.
type ReqGroupKey = (String, String, usize);

struct ReqGroup {
    level: crate::annotation::AnnotationLevel,
    ranges: Vec<(usize, usize)>,
}

// Per-target index of (cite_id, start, end) built by Step 5 and consumed by
// Step 6 to compute coverage.
type CoverageRefs = HashMap<String, Vec<(String, usize, usize)>>;

impl ReportV2 {
    /// Build a v2 report from the internal report result.
    pub fn from_report_result(report: &ReportResult) -> Self {
        let (repo_map, blob_link_to_repo_id) = build_repositories(report);

        let (inline_sources, target_to_src_id) = build_inline_sources(report);

        let (linked_sources, source_to_lnk_id) =
            build_linked_sources(report, &blob_link_to_repo_id);

        let issue_links = report
            .issue_link
            .as_deref()
            .map(|s| vec![s.to_string()])
            .unwrap_or_default();

        let (spec_annotations, section_annotations) =
            build_spec_and_section_annotations(report, &target_to_src_id);

        let (cite_annotations, coverage_refs) = build_cite_annotations(
            report,
            &target_to_src_id,
            &blob_link_to_repo_id,
            &source_to_lnk_id,
        );

        let req_annotations = build_requirement_annotations(
            report,
            &target_to_src_id,
            &blob_link_to_repo_id,
            &source_to_lnk_id,
            &coverage_refs,
        );

        ReportV2 {
            version: "2.0".to_string(),
            issue_links,
            repositories: repo_map,
            sources: SourcesV2 {
                inline: inline_sources,
                linked: linked_sources,
                extensions: BTreeMap::new(),
            },
            annotations: AnnotationsV2 {
                specification: spec_annotations,
                section: section_annotations,
                requirement: req_annotations,
                cite: cite_annotations,
                extensions: BTreeMap::new(),
            },
        }
    }
}

// ── Step helpers ─────────────────────────────────────────────────────────────

/// Resolve the repo-id for an annotation, falling back to the report-level
/// blob_link when the annotation doesn't carry one. Returns an empty string
/// when neither is present.
fn resolve_repo_id(
    annotation_blob_link: Option<&str>,
    report: &ReportResult,
    blob_link_to_repo_id: &HashMap<String, String>,
) -> String {
    annotation_blob_link
        .and_then(|bl| blob_link_to_repo_id.get(bl))
        .or_else(|| {
            report
                .blob_link
                .as_deref()
                .and_then(|bl| blob_link_to_repo_id.get(bl))
        })
        .cloned()
        .unwrap_or_default()
}

/// Step 1: build the repository map from all unique blob_links seen in
/// annotations and at the report level.
fn build_repositories(
    report: &ReportResult,
) -> (BTreeMap<String, Repository>, HashMap<String, String>) {
    let mut repo_map: BTreeMap<String, Repository> = BTreeMap::new();
    let mut blob_link_to_repo_id: HashMap<String, String> = HashMap::new();

    let mut insert = |bl_str: String| {
        blob_link_to_repo_id
            .entry(bl_str.clone())
            .or_insert_with(|| {
                let id = ids::repo_id(&bl_str);
                repo_map.insert(id.clone(), Repository { blob_link: bl_str });
                id
            });
    };

    for annotation in report.annotations.iter() {
        if let Some(bl) = &annotation.blob_link {
            insert(bl.to_string());
        }
    }
    if let Some(bl) = report.blob_link.as_deref() {
        insert(bl.to_string());
    }

    (repo_map, blob_link_to_repo_id)
}

/// Step 2: build the inline source map from the spec file contents backing
/// each target. Returns the map plus a target-path → src-id index used by
/// later steps.
fn build_inline_sources(
    report: &ReportResult,
) -> (BTreeMap<String, InlineSource>, HashMap<String, String>) {
    let mut inline_sources: BTreeMap<String, InlineSource> = BTreeMap::new();
    let mut target_to_src_id: HashMap<String, String> = HashMap::new();

    for (target, target_report) in report.targets.iter() {
        let sections = target_report.specification.sorted_sections();
        let Some(first_section) = sections.first() else {
            continue;
        };
        let file = first_section.full_title.file().clone();

        let contents: &str = &file;
        let id = ids::src_id(contents.as_bytes());

        // Use Display, which strips the current-dir prefix, so the
        // path is relative to the project root — matching the
        // LinkedSource contract.
        let file_name = file.path().to_string();

        if let Some(existing) = inline_sources.get(&id) {
            debug_assert_eq!(
                existing.file_name, file_name,
                "src-id {id}: file_name drift"
            );
        } else {
            inline_sources.insert(
                id.clone(),
                InlineSource {
                    file_name,
                    contents: contents.to_string(),
                },
            );
        }

        target_to_src_id.insert(target.path.to_string(), id);
    }

    (inline_sources, target_to_src_id)
}

/// Step 3: build the linked source map from annotation source paths.
/// Returns the map plus a (file_name, repo_id) → lnk-id index used by
/// later steps.
fn build_linked_sources(
    report: &ReportResult,
    blob_link_to_repo_id: &HashMap<String, String>,
) -> (
    BTreeMap<String, LinkedSource>,
    HashMap<(String, String), String>,
) {
    let mut linked_sources: BTreeMap<String, LinkedSource> = BTreeMap::new();
    let mut source_to_lnk_id: HashMap<(String, String), String> = HashMap::new();

    for annotation in report.annotations.iter() {
        let file_name = annotation.source.to_string_lossy().to_string();
        let repo_id = resolve_repo_id(
            annotation.blob_link.as_deref(),
            report,
            blob_link_to_repo_id,
        );

        let key = (file_name.clone(), repo_id.clone());
        source_to_lnk_id.entry(key).or_insert_with(|| {
            let id = ids::lnk_id(&file_name, &repo_id);
            linked_sources.insert(
                id.clone(),
                LinkedSource {
                    file_name,
                    repository: if repo_id.is_empty() {
                        None
                    } else {
                        Some(repo_id)
                    },
                },
            );
            id
        });
    }

    (linked_sources, source_to_lnk_id)
}

/// Step 4: build specification and section annotations from each target's
/// parsed specification.
fn build_spec_and_section_annotations(
    report: &ReportResult,
    target_to_src_id: &HashMap<String, String>,
) -> (
    BTreeMap<String, SpecificationAnnotation>,
    BTreeMap<String, SectionAnnotation>,
) {
    let mut spec_annotations: BTreeMap<String, SpecificationAnnotation> = BTreeMap::new();
    let mut section_annotations: BTreeMap<String, SectionAnnotation> = BTreeMap::new();

    for (target, target_report) in report.targets.iter() {
        let Some(src_id) = target_to_src_id.get(&target.path.to_string()) else {
            continue;
        };

        let sections = target_report.specification.sorted_sections();

        let file_len = sections
            .first()
            .map(|s| {
                let file: &str = s.full_title.file();
                file.len()
            })
            .unwrap_or(0);

        let url = match &target.path {
            crate::target::TargetPath::Url(u) => {
                Some(crate::target::TargetPath::canonical_url(u.as_str()).into_owned())
            }
            crate::target::TargetPath::Path(_) => None,
        };

        let spc_anno_id = ids::spc_id(src_id, 0, file_len);
        let new_spec = SpecificationAnnotation {
            source: SourceRef {
                src: src_id.clone(),
                start: 0,
                end: file_len,
            },
            title: target_report.specification.title.clone(),
            format: target_report.specification.format.to_string(),
            url,
        };
        if let Some(existing) = spec_annotations.get(&spc_anno_id) {
            debug_assert_eq!(
                existing, &new_spec,
                "spc-id {spc_anno_id}: specification metadata drift"
            );
        } else {
            spec_annotations.insert(spc_anno_id, new_spec);
        }

        for section in &sections {
            let start = section.full_title.range().start;
            let end = section
                .lines
                .iter()
                .filter_map(|l| match l {
                    crate::specification::Line::Str(s) => Some(s.range().end),
                    crate::specification::Line::Break => None,
                })
                .max()
                .unwrap_or(section.full_title.range().end);

            let sec_id = ids::sec_id(src_id, start, end);
            let new_section = SectionAnnotation {
                source: SourceRef {
                    src: src_id.clone(),
                    start,
                    end,
                },
                short_name: section.id.clone(),
                long_name: Some(section.title.clone()),
            };
            if let Some(existing) = section_annotations.get(&sec_id) {
                debug_assert_eq!(
                    existing, &new_section,
                    "sec-id {sec_id}: section metadata drift"
                );
            } else {
                section_annotations.insert(sec_id, new_section);
            }
        }
    }

    (spec_annotations, section_annotations)
}

/// Step 5: build impl annotations from every non-Spec reference. Returns
/// the cite map plus a per-target index of (cite_id, start, end) used by
/// Step 6 to compute coverage.
fn build_cite_annotations(
    report: &ReportResult,
    target_to_src_id: &HashMap<String, String>,
    blob_link_to_repo_id: &HashMap<String, String>,
    source_to_lnk_id: &HashMap<(String, String), String>,
) -> (BTreeMap<String, CiteAnnotation>, CoverageRefs) {
    let mut cite_annotations: BTreeMap<String, CiteAnnotation> = BTreeMap::new();
    let mut coverage_refs: CoverageRefs = HashMap::new();

    for (target, target_report) in report.targets.iter() {
        let target_path_str = target.path.to_string();
        let Some(src_id) = target_to_src_id.get(&target_path_str) else {
            continue;
        };

        for reference in &target_report.references {
            // Skip Spec references; they're represented by RequirementAnnotation
            // in Step 6. TryFrom returning Err for Spec makes this filter
            // compiler-enforced so future edits can't accidentally let Spec
            // references reach the cite pipeline.
            let Ok(new_anno_type): Result<AnnotationType, _> = reference.annotation.anno.try_into()
            else {
                continue;
            };

            let file_name = reference.annotation.source.to_string_lossy().to_string();
            let repo_id = resolve_repo_id(
                reference.annotation.blob_link.as_deref(),
                report,
                blob_link_to_repo_id,
            );

            let lnk_key = (file_name, repo_id);
            let lnk_id = source_to_lnk_id
                .get(&lnk_key)
                .cloned()
                .expect("linked source must be registered in build_linked_sources");

            let cite_id = ids::cite_id(&lnk_id, reference.annotation.anno_line, src_id);

            let range = ByteRange {
                start: reference.start(),
                end: reference.end(),
            };

            // When multiple references collapse to the same cite-id
            // (same authoring lnk-id, anno_line, and target src-id), the
            // first reference's metadata wins. In debug builds, verify
            // that subsequent references agree — they should, because
            // they come from the same annotation comment.
            let new_level: AnnotationLevel = reference.annotation.level.into();
            cite_annotations
                .entry(cite_id.clone())
                .and_modify(|a| {
                    debug_assert_eq!(a.anno_type, new_anno_type, "cite_id {cite_id}");
                    debug_assert_eq!(a.level, new_level, "cite_id {cite_id}");
                    debug_assert_eq!(
                        a.comment.as_deref().unwrap_or(""),
                        reference.annotation.comment,
                        "cite_id {cite_id}"
                    );
                    debug_assert_eq!(
                        a.feature.as_deref().unwrap_or(""),
                        reference.annotation.feature,
                        "cite_id {cite_id}"
                    );
                    debug_assert_eq!(
                        a.tracking_issue.as_deref().unwrap_or(""),
                        reference.annotation.tracking_issue,
                        "cite_id {cite_id}"
                    );
                    debug_assert!(
                        a.tags.iter().eq(reference.annotation.tags.iter()),
                        "cite_id {cite_id}: tag mismatch"
                    );
                    a.target.ranges.push(range.clone());
                })
                .or_insert_with(|| CiteAnnotation {
                    source: SourceLocation {
                        src: lnk_id.clone(),
                        line: if reference.annotation.anno_line > 0 {
                            Some(reference.annotation.anno_line)
                        } else {
                            None
                        },
                    },
                    target: SourceRanges {
                        src: src_id.clone(),
                        ranges: vec![range.clone()],
                    },
                    anno_type: new_anno_type,
                    level: reference.annotation.level.into(),
                    comment: if reference.annotation.comment.is_empty() {
                        None
                    } else {
                        Some(reference.annotation.comment.clone())
                    },
                    feature: if reference.annotation.feature.is_empty() {
                        None
                    } else {
                        Some(reference.annotation.feature.clone())
                    },
                    tracking_issue: if reference.annotation.tracking_issue.is_empty() {
                        None
                    } else {
                        Some(reference.annotation.tracking_issue.clone())
                    },
                    tags: reference.annotation.tags.iter().cloned().collect(),
                });

            coverage_refs
                .entry(target_path_str.clone())
                .or_default()
                .push((cite_id, reference.start(), reference.end()));
        }
    }

    // Normalize cite target ranges: sort and dedup for deterministic output.
    for cite_anno in cite_annotations.values_mut() {
        cite_anno.target.ranges.sort();
        cite_anno.target.ranges.dedup();
    }

    (cite_annotations, coverage_refs)
}

/// Step 6: build requirement annotations from Spec references, grouping by
/// authoring site so a single logical requirement whose quote matched N
/// disjoint byte ranges (e.g., across an IETF RFC page break) becomes one
/// RequirementAnnotation with all N ranges.
fn build_requirement_annotations(
    report: &ReportResult,
    target_to_src_id: &HashMap<String, String>,
    blob_link_to_repo_id: &HashMap<String, String>,
    source_to_lnk_id: &HashMap<(String, String), String>,
    coverage_refs: &CoverageRefs,
) -> BTreeMap<String, RequirementAnnotation> {
    let mut req_annotations: BTreeMap<String, RequirementAnnotation> = BTreeMap::new();

    for (target, target_report) in report.targets.iter() {
        let target_path_str = target.path.to_string();
        let Some(src_id) = target_to_src_id.get(&target_path_str) else {
            continue;
        };

        let target_coverage = coverage_refs.get(&target_path_str);

        // Pass 1: group Spec references by authoring site.
        let mut groups: BTreeMap<ReqGroupKey, ReqGroup> = BTreeMap::new();
        for reference in &target_report.references {
            if reference.annotation.anno != crate::annotation::AnnotationType::Spec {
                continue;
            }

            let file_name = reference.annotation.source.to_string_lossy().to_string();
            let repo_id = resolve_repo_id(
                reference.annotation.blob_link.as_deref(),
                report,
                blob_link_to_repo_id,
            );

            let lnk_key = (file_name, repo_id);
            let lnk_id = source_to_lnk_id
                .get(&lnk_key)
                .cloned()
                .expect("linked source must be registered in build_linked_sources");

            let anno_line = reference.annotation.anno_line;
            let key = (src_id.clone(), lnk_id, anno_line);
            groups
                .entry(key)
                .or_insert_with(|| ReqGroup {
                    level: reference.annotation.level,
                    ranges: Vec::new(),
                })
                .ranges
                .push((reference.start(), reference.end()));
        }

        // Pass 2: emit one RequirementAnnotation per group.
        for ((group_src_id, lnk_id, anno_line), mut group) in groups {
            group.ranges.sort();
            group.ranges.dedup();

            let req_id = ids::req_id(&group_src_id, &group.ranges, &lnk_id, anno_line);

            // Build coverage: for each origin range, clamp every non-Spec
            // reference on this target against it. A single impl reference
            // may contribute under multiple origin ranges of the same
            // requirement.
            let mut coverage: BTreeMap<String, Vec<ByteRange>> = BTreeMap::new();
            if let Some(refs) = target_coverage {
                for (origin_start, origin_end) in &group.ranges {
                    for (cite_id, ref_start, ref_end) in refs {
                        let clamped_start = (*ref_start).max(*origin_start);
                        let clamped_end = (*ref_end).min(*origin_end);
                        if clamped_start < clamped_end {
                            coverage
                                .entry(cite_id.clone())
                                .or_default()
                                .push(ByteRange {
                                    start: clamped_start,
                                    end: clamped_end,
                                });
                        }
                    }
                }
                for ranges in coverage.values_mut() {
                    ranges.sort();
                    ranges.dedup();
                }
            }

            req_annotations.insert(
                req_id,
                RequirementAnnotation {
                    source: SourceLocation {
                        src: lnk_id,
                        line: if anno_line > 0 { Some(anno_line) } else { None },
                    },
                    origin: SourceRanges {
                        src: group_src_id,
                        ranges: group
                            .ranges
                            .into_iter()
                            .map(|(start, end)| ByteRange { start, end })
                            .collect(),
                    },
                    level: group.level.into(),
                    coverage,
                },
            );
        }
    }

    req_annotations
}

// ── CLI entry point ──────────────────────────────────────────────────────────

/// Generate a v2 JSON report from a ReportResult.
pub fn report(
    report: &crate::report::ReportResult,
    path: &duvet_core::path::Path,
) -> crate::Result {
    let report_v2 = ReportV2::from_report_result(report);
    write_report_v2(&report_v2, path.as_ref())
}

// ── JSON I/O ─────────────────────────────────────────────────────────────────

pub fn write_report_v2(report: &ReportV2, path: &std::path::Path) -> crate::Result {
    let mut out = vec![];
    write_report_v2_to_writer(report, &mut out)?;
    duvet_core::vfs::write(duvet_core::path::Path::from(path), out)
}

pub fn write_report_v2_to_writer<W: std::io::Write>(report: &ReportV2, writer: W) -> crate::Result {
    serde_json::to_writer_pretty(writer, report)
        .map_err(|e| duvet_core::error!("failed to serialize report: {}", e))?;
    Ok(())
}

pub fn read_report_v2(path: &std::path::Path) -> crate::Result<ReportV2> {
    let contents = duvet_core::vfs::read_sync(duvet_core::path::Path::from(path))
        .map_err(|e| duvet_core::error!("failed to open file '{}': {}", path.display(), e))?;
    read_report_v2_from_reader(contents.data())
        .map_err(|e| duvet_core::error!("failed to read report from '{}': {}", path.display(), e))
}

pub fn read_report_v2_from_reader<R: std::io::Read>(reader: R) -> crate::Result<ReportV2> {
    let report: ReportV2 = serde_json::from_reader(reader)
        .map_err(|e| duvet_core::error!("failed to parse JSON: {}", e))?;

    if report.version != "2.0" {
        return Err(duvet_core::error!(
            "unsupported report version '{}', expected '2.0'",
            report.version
        ));
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bolero::check;

    fn empty_report() -> ReportV2 {
        ReportV2 {
            version: "2.0".to_string(),
            issue_links: Vec::new(),
            repositories: BTreeMap::new(),
            sources: SourcesV2::default(),
            annotations: AnnotationsV2::default(),
        }
    }

    #[test]
    fn empty_report_roundtrip() {
        let report = empty_report();
        let json = serde_json::to_string(&report).unwrap();
        let deserialized: ReportV2 = serde_json::from_str(&json).unwrap();
        assert_eq!(report, deserialized);
    }

    /// Roundtrip a real report fixture produced by an integration test run.
    ///
    /// Unlike the handwritten fixtures in this module, this exercises the
    /// output shape of `ReportV2::from_report_result` as serialized by the
    /// production code path. If the serializer and the type definitions
    /// ever drift (e.g. a field is renamed on the struct but not the
    /// matching `#[serde(rename)]`), the fixture tests can still pass
    /// while real reports fail to deserialize — this test catches that.
    ///
    /// The fixture is an insta snapshot stored under
    /// `integration/snapshots/`, so it is regenerated automatically
    /// whenever the serialization format changes.
    #[test]
    fn real_report_snapshot_roundtrip() {
        // Strip the YAML front matter that insta prepends to snapshots
        // (delimited by `---` lines) to leave just the JSON body.
        const SNAPSHOT: &str =
            include_str!("../../../integration/snapshots/report-markdown_json_v2.snap");
        let json = SNAPSHOT
            .split_once("---\n")
            .and_then(|(_, rest)| rest.split_once("---\n"))
            .map(|(_, body)| body)
            .expect("snapshot should have two `---` front-matter delimiters");

        let report: ReportV2 = serde_json::from_str(json)
            .expect("real report snapshot must deserialize into ReportV2");

        // Sanity-check: every annotation bucket in this fixture is populated.
        assert!(!report.annotations.specification.is_empty());
        assert!(!report.annotations.section.is_empty());
        assert!(!report.annotations.requirement.is_empty());
        assert!(!report.annotations.cite.is_empty());
        assert!(!report.sources.inline.is_empty());
        assert!(!report.sources.linked.is_empty());

        // Idempotent roundtrip: re-serializing and re-parsing must yield
        // an equal value.
        let reserialized = serde_json::to_string(&report).unwrap();
        let redeserialized: ReportV2 = serde_json::from_str(&reserialized).unwrap();
        assert_eq!(report, redeserialized);
    }

    #[test]
    fn populated_report_roundtrip() {
        let mut report = empty_report();
        report.issue_links = vec!["https://github.com/org/repo/issues".to_string()];
        report.repositories.insert(
            "repo-abc123".to_string(),
            Repository {
                blob_link: "https://github.com/org/repo/blob/main".to_string(),
            },
        );
        report.sources.inline.insert(
            "src-def456".to_string(),
            InlineSource {
                file_name: "spec.md".to_string(),
                contents: "# Spec\nMUST do X".to_string(),
            },
        );
        report.sources.linked.insert(
            "lnk-789abc".to_string(),
            LinkedSource {
                file_name: "src/lib.rs".to_string(),
                repository: Some("repo-abc123".to_string()),
            },
        );
        report.annotations.specification.insert(
            "spc-001".to_string(),
            SpecificationAnnotation {
                source: SourceRef {
                    src: "src-def456".to_string(),
                    start: 0,
                    end: 18,
                },
                title: Some("Spec".to_string()),
                format: "markdown".to_string(),
                url: Some("https://www.rfc-editor.org/rfc/rfc9000.txt".to_string()),
            },
        );
        report.annotations.section.insert(
            "spc-002".to_string(),
            SectionAnnotation {
                source: SourceRef {
                    src: "src-def456".to_string(),
                    start: 0,
                    end: 18,
                },
                short_name: "spec".to_string(),
                long_name: Some("Spec".to_string()),
            },
        );
        report.annotations.requirement.insert(
            "req-003".to_string(),
            RequirementAnnotation {
                source: SourceLocation {
                    src: "lnk-789abc".to_string(),
                    line: Some(5),
                },
                origin: SourceRanges {
                    src: "src-def456".to_string(),
                    ranges: vec![ByteRange { start: 7, end: 18 }],
                },
                level: AnnotationLevel::Must,
                coverage: BTreeMap::from([(
                    "cite-aaa".to_string(),
                    vec![ByteRange { start: 7, end: 18 }],
                )]),
            },
        );
        report.annotations.cite.insert(
            "cite-aaa".to_string(),
            CiteAnnotation {
                source: SourceLocation {
                    src: "lnk-789abc".to_string(),
                    line: Some(42),
                },
                target: SourceRanges {
                    src: "src-def456".to_string(),
                    ranges: vec![ByteRange { start: 7, end: 18 }],
                },
                anno_type: AnnotationType::Citation,
                level: AnnotationLevel::Auto,
                comment: None,
                feature: None,
                tracking_issue: None,
                tags: Vec::new(),
            },
        );

        let json = serde_json::to_string_pretty(&report).unwrap();
        let deserialized: ReportV2 = serde_json::from_str(&json).unwrap();
        assert_eq!(report, deserialized);
    }

    #[test]
    fn spec_url_none_is_omitted_in_json() {
        let mut report = empty_report();
        report.annotations.specification.insert(
            "spc-001".to_string(),
            SpecificationAnnotation {
                source: SourceRef {
                    src: "src-x".to_string(),
                    start: 0,
                    end: 1,
                },
                title: Some("Spec".to_string()),
                format: "markdown".to_string(),
                url: None,
            },
        );
        let json = serde_json::to_string(&report).unwrap();
        assert!(
            !json.contains("\"url\""),
            "url key should be omitted when None: {json}"
        );
    }

    #[test]
    fn read_wrong_version_returns_error() {
        let json = r#"{
            "version": "1.0",
            "sources": {
                "https://awslabs.github.io/duvet/v2/sources.json#inline": {},
                "https://awslabs.github.io/duvet/v2/sources.json#linked": {}
            },
            "annotations": {
                "https://awslabs.github.io/duvet/v2/annotations.json#specification": {},
                "https://awslabs.github.io/duvet/v2/annotations.json#section": {},
                "https://awslabs.github.io/duvet/v2/annotations.json#requirement": {},
                "https://awslabs.github.io/duvet/v2/annotations.json#cite": {}
            }
        }"#;
        let result = read_report_v2_from_reader(json.as_bytes());
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("unsupported report version"));
    }

    #[test]
    fn read_invalid_json_returns_error() {
        let result = read_report_v2_from_reader("{ not valid }".as_bytes());
        assert!(result.is_err());
    }

    #[test]
    fn schema_url_keys_in_json() {
        let json = serde_json::to_string_pretty(&empty_report()).unwrap();
        assert!(json.contains("sources.json#inline"));
        assert!(json.contains("sources.json#linked"));
        assert!(json.contains("annotations.json#specification"));
        assert!(json.contains("annotations.json#section"));
        assert!(json.contains("annotations.json#requirement"));
        assert!(json.contains("annotations.json#cite"));
    }

    /// Coverage map keys in requirement annotations should reference valid
    /// cite- prefixed IDs that could exist in the cite annotation map.
    #[test]
    fn coverage_keys_have_cite_prefix() {
        check!()
            .with_type::<Vec<(String, String)>>()
            .for_each(|pairs| {
                let mut report = empty_report();
                for (i, (key, val)) in pairs.iter().enumerate() {
                    let cite_key = format!("cite-{key}");
                    let mut coverage = BTreeMap::new();
                    coverage.insert(
                        cite_key.clone(),
                        vec![ByteRange {
                            start: 0,
                            end: i + 1,
                        }],
                    );
                    report.annotations.requirement.insert(
                        format!("req-{val}"),
                        RequirementAnnotation {
                            source: SourceLocation {
                                src: "lnk-x".to_string(),
                                line: Some(i + 1),
                            },
                            origin: SourceRanges {
                                src: "src-x".to_string(),
                                ranges: vec![ByteRange {
                                    start: 0,
                                    end: i + 1,
                                }],
                            },
                            level: AnnotationLevel::Must,
                            coverage,
                        },
                    );
                }

                // Verify roundtrip
                let json = serde_json::to_string(&report).unwrap();
                let rt: ReportV2 = serde_json::from_str(&json).unwrap();
                assert_eq!(report, rt);
            });
    }

    /// Unknown URL-keyed buckets in `sources` and `annotations` must
    /// roundtrip unchanged. The v2 format is designed to be extended by
    /// future schema revisions and out-of-band tooling, so unknown keys
    /// must be preserved on read so downstream consumers can opt in to
    /// schemas this build doesn't recognize.
    #[test]
    fn unknown_extension_keys_roundtrip() {
        let json = r#"{
            "version": "2.0",
            "sources": {
                "https://awslabs.github.io/duvet/v2/sources.json#inline": {},
                "https://awslabs.github.io/duvet/v2/sources.json#linked": {},
                "https://example.com/duvet/ext/sources.json#virtual": {
                    "vsrc-1": { "label": "synthetic" }
                }
            },
            "annotations": {
                "https://awslabs.github.io/duvet/v2/annotations.json#specification": {},
                "https://awslabs.github.io/duvet/v2/annotations.json#section": {},
                "https://awslabs.github.io/duvet/v2/annotations.json#requirement": {},
                "https://awslabs.github.io/duvet/v2/annotations.json#cite": {},
                "https://example.com/duvet/ext/annotations.json#review": {
                    "rev-1": { "reviewer": "alice" }
                }
            }
        }"#;

        let report = read_report_v2_from_reader(json.as_bytes()).unwrap();
        assert_eq!(report.sources.extensions.len(), 1);
        assert_eq!(report.annotations.extensions.len(), 1);

        let reserialized = serde_json::to_string(&report).unwrap();
        let rt = read_report_v2_from_reader(reserialized.as_bytes()).unwrap();
        assert_eq!(report, rt);
    }

    /// Known buckets must be optional. A document that omits some or all
    /// of the named bucket keys should still parse — only `version`,
    /// `sources`, and `annotations` are structurally required.
    #[test]
    fn missing_known_buckets_parse() {
        let json = r#"{
            "version": "2.0",
            "sources": {},
            "annotations": {}
        }"#;

        let report = read_report_v2_from_reader(json.as_bytes()).unwrap();
        assert!(report.sources.inline.is_empty());
        assert!(report.sources.linked.is_empty());
        assert!(report.annotations.specification.is_empty());
        assert!(report.annotations.section.is_empty());
        assert!(report.annotations.requirement.is_empty());
        assert!(report.annotations.cite.is_empty());
    }

    #[test]
    fn schema_test() {
        let mut schema = schemars::schema_for!(ReportV2);

        schema.insert(
            "title".to_string(),
            serde_json::Value::String("Duvet Report V2".to_string()),
        );

        schema.insert(
            "$id".to_string(),
            serde_json::Value::String("https://awslabs.github.io/duvet/v2/report.json".to_string()),
        );

        duvet_core::artifact::sync(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../schemas/report-v2.json"),
            serde_json::to_string_pretty(&schema).unwrap(),
        );
    }
}
