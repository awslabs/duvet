// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! JSON v2 format for duvet reports.
//!
//! This module provides a roundtrip-friendly JSON format with entity-typed
//! deterministic IDs, enabling multi-package report merging.

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
pub struct Repository {
    pub blob_link: String,
}

// ── Sources ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct SourcesV2 {
    #[serde(rename = "https://awslabs.github.io/duvet/v2/sources.json#inline")]
    pub inline: BTreeMap<String, InlineSource>,
    #[serde(rename = "https://awslabs.github.io/duvet/v2/sources.json#linked")]
    pub linked: BTreeMap<String, LinkedSource>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct InlineSource {
    pub file_name: String,
    pub contents: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct LinkedSource {
    pub file_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
}

// ── Annotations ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct AnnotationsV2 {
    #[serde(rename = "https://awslabs.github.io/duvet/v2/annotations.json#specification")]
    pub specification: BTreeMap<String, SpecificationAnnotation>,
    #[serde(rename = "https://awslabs.github.io/duvet/v2/annotations.json#section")]
    pub section: BTreeMap<String, SectionAnnotation>,
    #[serde(rename = "https://awslabs.github.io/duvet/v2/annotations.json#requirement")]
    pub requirement: BTreeMap<String, RequirementAnnotation>,
    #[serde(rename = "https://awslabs.github.io/duvet/v2/annotations.json#impl")]
    pub r#impl: BTreeMap<String, ImplAnnotation>,
}

// ── Shared types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct SourceRef {
    pub source: String,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct SourceLocation {
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct ByteRange {
    pub start: usize,
    pub end: usize,
}

// ── Annotation types ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct SpecificationAnnotation {
    pub source: SourceRef,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub format: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct SectionAnnotation {
    pub source: SourceRef,
    pub short_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub long_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct RequirementAnnotation {
    pub source: SourceRef,
    pub level: AnnotationLevel,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub coverage: BTreeMap<String, Vec<ByteRange>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct ImplAnnotation {
    pub source: SourceLocation,
    pub target_source: String,
    pub target_ranges: Vec<ByteRange>,
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

/// Annotation type for impl annotations (v2 format).
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

impl From<crate::annotation::AnnotationType> for AnnotationType {
    fn from(anno_type: crate::annotation::AnnotationType) -> Self {
        match anno_type {
            crate::annotation::AnnotationType::Citation => Self::Citation,
            crate::annotation::AnnotationType::Test => Self::Test,
            crate::annotation::AnnotationType::Implication => Self::Implication,
            crate::annotation::AnnotationType::Exception => Self::Exception,
            crate::annotation::AnnotationType::Todo => Self::Todo,
            crate::annotation::AnnotationType::Spec => {
                panic!("Spec annotations should not be converted to ImplAnnotation type")
            }
        }
    }
}

// ── Report construction ──────────────────────────────────────────────────────

impl ReportV2 {
    /// Build a v2 report from the internal report result.
    pub fn from_report_result(report: &ReportResult) -> Self {
        // Step 1: Build repository map from unique blob_links
        let mut repo_map: BTreeMap<String, Repository> = BTreeMap::new();
        let mut blob_link_to_repo_id: HashMap<String, String> = HashMap::new();

        for annotation in report.annotations.iter() {
            if let Some(bl) = &annotation.blob_link {
                let bl_str = bl.to_string();
                blob_link_to_repo_id
                    .entry(bl_str.clone())
                    .or_insert_with(|| {
                        let id = ids::repo_id(&bl_str);
                        repo_map.insert(id.clone(), Repository { blob_link: bl_str });
                        id
                    });
            }
        }
        // Also check the global blob_link
        if let Some(bl) = report.blob_link {
            let bl_str = bl.to_string();
            blob_link_to_repo_id
                .entry(bl_str.clone())
                .or_insert_with(|| {
                    let id = ids::repo_id(&bl_str);
                    repo_map.insert(id.clone(), Repository { blob_link: bl_str });
                    id
                });
        }

        // Step 2: Build inline source map from spec file contents
        let mut inline_sources: BTreeMap<String, InlineSource> = BTreeMap::new();
        // Maps target path string → src-ID for lookup during annotation building
        let mut target_to_src_id: HashMap<String, String> = HashMap::new();

        for (target, target_report) in report.targets.iter() {
            // Get the spec file contents from any section's backing SourceFile
            let source_file = target_report
                .specification
                .sorted_sections()
                .into_iter()
                .map(|s| s.full_title.file().clone())
                .next();

            if let Some(file) = source_file {
                let contents: &str = &file;
                let id = ids::src_id(contents.as_bytes());

                if !inline_sources.contains_key(&id) {
                    let file_name = file
                        .path()
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| target.path.to_string());

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
        }

        // Step 3: Build linked source map from annotations
        let mut linked_sources: BTreeMap<String, LinkedSource> = BTreeMap::new();
        // Maps (file_name, repo_id_or_empty) → lnk-ID
        let mut source_to_lnk_id: HashMap<(String, String), String> = HashMap::new();

        for annotation in report.annotations.iter() {
            let file_name = annotation.source.to_string_lossy().to_string();
            let repo_id = annotation
                .blob_link
                .as_ref()
                .and_then(|bl| blob_link_to_repo_id.get(bl.as_ref()))
                .or_else(|| report.blob_link.and_then(|bl| blob_link_to_repo_id.get(bl)))
                .cloned()
                .unwrap_or_default();

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

        let issue_links = report
            .issue_link
            .map(|s| vec![s.to_string()])
            .unwrap_or_default();

        // Step 4: Build specification and section annotations
        let mut spec_annotations: BTreeMap<String, SpecificationAnnotation> = BTreeMap::new();
        let mut section_annotations: BTreeMap<String, SectionAnnotation> = BTreeMap::new();

        for (target, target_report) in report.targets.iter() {
            let Some(src_id) = target_to_src_id.get(&target.path.to_string()) else {
                continue;
            };

            // Get file length from the backing SourceFile
            let file_len = target_report
                .specification
                .sorted_sections()
                .first()
                .map(|s| {
                    let file: &str = s.full_title.file();
                    file.len()
                })
                .unwrap_or(0);

            // Specification annotation: full byte range of the file
            let spc_anno_id = ids::spc_id(src_id, 0, file_len);
            spec_annotations.insert(
                spc_anno_id,
                SpecificationAnnotation {
                    source: SourceRef {
                        source: src_id.clone(),
                        start: 0,
                        end: file_len,
                    },
                    title: target_report.specification.title.clone(),
                    format: target_report.specification.format.to_string(),
                },
            );

            // Section annotations
            for section in target_report.specification.sorted_sections() {
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

                let sec_id = ids::spc_id(src_id, start, end);
                section_annotations.insert(
                    sec_id,
                    SectionAnnotation {
                        source: SourceRef {
                            source: src_id.clone(),
                            start,
                            end,
                        },
                        short_name: section.id.clone(),
                        long_name: Some(section.title.clone()),
                    },
                );
            }
        }

        // Step 5: Build impl annotations (non-Spec references)
        // Group references by cite-ID to collect all byte ranges per impl annotation
        let mut impl_annotations: BTreeMap<String, ImplAnnotation> = BTreeMap::new();
        // Also collect all non-Spec reference ranges indexed by target, for coverage computation
        // Key: target path string, Value: vec of (cite_id, start, end)
        let mut coverage_refs: HashMap<String, Vec<(String, usize, usize)>> = HashMap::new();

        for (target, target_report) in report.targets.iter() {
            let target_path_str = target.path.to_string();
            let Some(src_id) = target_to_src_id.get(&target_path_str) else {
                continue;
            };

            for reference in &target_report.references {
                if reference.annotation.anno == crate::annotation::AnnotationType::Spec {
                    continue;
                }

                let file_name = reference.annotation.source.to_string_lossy().to_string();
                let repo_id = reference
                    .annotation
                    .blob_link
                    .as_ref()
                    .and_then(|bl| blob_link_to_repo_id.get(bl.as_ref()))
                    .or_else(|| report.blob_link.and_then(|bl| blob_link_to_repo_id.get(bl)))
                    .cloned()
                    .unwrap_or_default();

                let lnk_key = (file_name.clone(), repo_id.clone());
                let lnk_id = source_to_lnk_id.get(&lnk_key).cloned().unwrap_or_default();

                let cite_id = ids::cite_id(&lnk_id, reference.annotation.anno_line, src_id);

                // Add byte range to existing impl annotation or create new one
                let range = ByteRange {
                    start: reference.start(),
                    end: reference.end(),
                };

                impl_annotations
                    .entry(cite_id.clone())
                    .and_modify(|a| a.target_ranges.push(range.clone()))
                    .or_insert_with(|| ImplAnnotation {
                        source: SourceLocation {
                            source: lnk_id.clone(),
                            line: if reference.annotation.anno_line > 0 {
                                Some(reference.annotation.anno_line)
                            } else {
                                None
                            },
                        },
                        target_source: src_id.clone(),
                        target_ranges: vec![range.clone()],
                        anno_type: reference.annotation.anno.into(),
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

                // Track for coverage computation
                coverage_refs
                    .entry(target_path_str.clone())
                    .or_default()
                    .push((cite_id, reference.start(), reference.end()));
            }
        }

        // Step 6: Build requirement annotations (Spec references with coverage)
        let mut req_annotations: BTreeMap<String, RequirementAnnotation> = BTreeMap::new();

        for (target, target_report) in report.targets.iter() {
            let target_path_str = target.path.to_string();
            let Some(src_id) = target_to_src_id.get(&target_path_str) else {
                continue;
            };

            let target_coverage = coverage_refs.get(&target_path_str);

            for reference in &target_report.references {
                if reference.annotation.anno != crate::annotation::AnnotationType::Spec {
                    continue;
                }

                let req_start = reference.start();
                let req_end = reference.end();
                let req_id = ids::spc_id(src_id, req_start, req_end);

                // Build coverage map: for each non-Spec reference that overlaps
                // this requirement's byte range, compute clamped intersection
                let mut coverage: BTreeMap<String, Vec<ByteRange>> = BTreeMap::new();

                if let Some(refs) = target_coverage {
                    for (cite_id, ref_start, ref_end) in refs {
                        let clamped_start = (*ref_start).max(req_start);
                        let clamped_end = (*ref_end).min(req_end);
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

                req_annotations
                    .entry(req_id)
                    .or_insert_with(|| RequirementAnnotation {
                        source: SourceRef {
                            source: src_id.clone(),
                            start: req_start,
                            end: req_end,
                        },
                        level: reference.annotation.level.into(),
                        coverage,
                    });
            }
        }

        ReportV2 {
            version: "2.0".to_string(),
            issue_links,
            repositories: repo_map,
            sources: SourcesV2 {
                inline: inline_sources,
                linked: linked_sources,
            },
            annotations: AnnotationsV2 {
                specification: spec_annotations,
                section: section_annotations,
                requirement: req_annotations,
                r#impl: impl_annotations,
            },
        }
    }
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
    use std::{fs::File, io::BufWriter};

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let file = File::create(path)
        .map_err(|e| duvet_core::error!("failed to create file '{}': {}", path.display(), e))?;
    let writer = BufWriter::new(file);
    write_report_v2_to_writer(report, writer)
}

pub fn write_report_v2_to_writer<W: std::io::Write>(report: &ReportV2, writer: W) -> crate::Result {
    serde_json::to_writer_pretty(writer, report)
        .map_err(|e| duvet_core::error!("failed to serialize report: {}", e))?;
    Ok(())
}

#[allow(dead_code)]
pub fn read_report_v2(path: &std::path::Path) -> crate::Result<ReportV2> {
    use std::{fs::File, io::BufReader};

    let file = File::open(path)
        .map_err(|e| duvet_core::error!("failed to open file '{}': {}", path.display(), e))?;
    let reader = BufReader::new(file);
    read_report_v2_from_reader(reader)
        .map_err(|e| duvet_core::error!("failed to read report from '{}': {}", path.display(), e))
}

#[allow(dead_code)]
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
            sources: SourcesV2 {
                inline: BTreeMap::new(),
                linked: BTreeMap::new(),
            },
            annotations: AnnotationsV2 {
                specification: BTreeMap::new(),
                section: BTreeMap::new(),
                requirement: BTreeMap::new(),
                r#impl: BTreeMap::new(),
            },
        }
    }

    #[test]
    fn empty_report_roundtrip() {
        let report = empty_report();
        let json = serde_json::to_string(&report).unwrap();
        let deserialized: ReportV2 = serde_json::from_str(&json).unwrap();
        assert_eq!(report, deserialized);
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
                    source: "src-def456".to_string(),
                    start: 0,
                    end: 18,
                },
                title: Some("Spec".to_string()),
                format: "markdown".to_string(),
            },
        );
        report.annotations.section.insert(
            "spc-002".to_string(),
            SectionAnnotation {
                source: SourceRef {
                    source: "src-def456".to_string(),
                    start: 0,
                    end: 18,
                },
                short_name: "spec".to_string(),
                long_name: Some("Spec".to_string()),
            },
        );
        report.annotations.requirement.insert(
            "spc-003".to_string(),
            RequirementAnnotation {
                source: SourceRef {
                    source: "src-def456".to_string(),
                    start: 7,
                    end: 18,
                },
                level: AnnotationLevel::Must,
                coverage: BTreeMap::from([(
                    "cite-aaa".to_string(),
                    vec![ByteRange { start: 7, end: 18 }],
                )]),
            },
        );
        report.annotations.r#impl.insert(
            "cite-aaa".to_string(),
            ImplAnnotation {
                source: SourceLocation {
                    source: "lnk-789abc".to_string(),
                    line: Some(42),
                },
                target_source: "src-def456".to_string(),
                target_ranges: vec![ByteRange { start: 7, end: 18 }],
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
                "https://awslabs.github.io/duvet/v2/annotations.json#impl": {}
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
        assert!(json.contains("annotations.json#impl"));
    }

    /// Coverage map keys in requirement annotations should reference valid
    /// cite- prefixed IDs that could exist in the impl annotation map.
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
                        format!("spc-{val}"),
                        RequirementAnnotation {
                            source: SourceRef {
                                source: "src-x".to_string(),
                                start: 0,
                                end: i + 1,
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
