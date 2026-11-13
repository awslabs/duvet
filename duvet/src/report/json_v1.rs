// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Serde representation of the legacy JSON report format.
//!
//! The normal report pipeline continues to use the streaming writer in
//! `json.rs`. These types are used when converting an already-materialized v2
//! report and when comparing two v1 reports semantically.

use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::File,
    io::{BufReader, BufWriter},
    path::Path,
};

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct ReportV1 {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blob_link: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issue_link: Option<String>,
    pub specifications: BTreeMap<String, SpecificationV1>,
    pub annotations: Vec<AnnotationV1>,
    pub statuses: BTreeMap<usize, RequirementStatusV1>,
    pub refs: Vec<RefStatusV1>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct SpecificationV1 {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub format: String,
    #[serde(default)]
    pub requirements: Vec<usize>,
    pub sections: Vec<SectionV1>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct SectionV1 {
    pub id: String,
    pub title: String,
    pub lines: Vec<LineV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requirements: Vec<usize>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(untagged)]
pub enum LineV1 {
    Text(String),
    Segments(Vec<SegmentV1>),
}

pub type SegmentV1 = (Vec<usize>, usize, String);

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AnnotationV1 {
    pub source: String,
    pub target_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_section: Option<String>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub line: usize,
    #[serde(
        default = "default_annotation_type",
        rename = "type",
        skip_serializing_if = "is_citation"
    )]
    pub anno_type: String,
    #[serde(default = "default_level", skip_serializing_if = "is_auto")]
    pub level: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feature: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tracking_issue: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blob_link: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

impl Default for AnnotationV1 {
    fn default() -> Self {
        Self {
            source: String::new(),
            target_path: String::new(),
            target_section: None,
            line: 0,
            anno_type: default_annotation_type(),
            level: default_level(),
            comment: None,
            feature: None,
            tracking_issue: None,
            blob_link: None,
            tags: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct RequirementStatusV1 {
    #[serde(default, skip_serializing_if = "is_zero")]
    pub spec: usize,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub incomplete: usize,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub citation: usize,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub implication: usize,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub test: usize,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub exception: usize,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub todo: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related: Vec<usize>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RefStatusV1 {
    #[serde(default, skip_serializing_if = "is_false")]
    pub spec: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub citation: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub implication: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub test: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub exception: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub todo: bool,
    #[serde(default = "default_level", skip_serializing_if = "is_auto")]
    pub level: String,
}

impl Default for RefStatusV1 {
    fn default() -> Self {
        Self {
            spec: false,
            citation: false,
            implication: false,
            test: false,
            exception: false,
            todo: false,
            level: default_level(),
        }
    }
}

pub fn read(path: &Path) -> crate::Result<ReportV1> {
    let file = File::open(path)
        .map_err(|e| duvet_core::error!("failed to open file '{}': {}", path.display(), e))?;
    serde_json::from_reader(BufReader::new(file))
        .map_err(|e| duvet_core::error!("failed to parse v1 report '{}': {}", path.display(), e))
}

pub fn write(report: &ReportV1, path: &Path) -> crate::Result {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = File::create(path)
        .map_err(|e| duvet_core::error!("failed to create file '{}': {}", path.display(), e))?;
    serde_json::to_writer_pretty(BufWriter::new(file), report)
        .map_err(|e| duvet_core::error!("failed to serialize v1 report: {}", e))?;
    Ok(())
}

fn default_annotation_type() -> String {
    "CITATION".to_string()
}

fn default_level() -> String {
    "AUTO".to_string()
}

fn is_zero(value: &usize) -> bool {
    *value == 0
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn is_citation(value: &String) -> bool {
    value == "CITATION"
}

fn is_auto(value: &String) -> bool {
    value == "AUTO"
}
