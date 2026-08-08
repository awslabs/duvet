// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    annotation::AnnotationType,
    config,
    target::{Target, TargetPath},
};
use duvet_core::{diagnostic::IntoDiagnostic, path::Path, Result};
use serde::Deserialize;
use std::sync::Arc;

use super::TemplatedString;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct Schema {
    #[serde(default, rename = "source")]
    pub sources: Arc<[Source]>,

    #[serde(default, rename = "requirement")]
    pub requirements: Arc<[Requirement]>,

    #[serde(default)]
    pub report: Arc<Report>,

    #[serde(default, rename = "specification")]
    pub specifications: Arc<[Specification]>,

    /// Policy for the duplicates check — both coincidence axes
    /// (design/duplicates/spec.md §4).
    #[serde(default)]
    pub duplicates: Option<DuplicatesSchema>,

    #[serde(rename = "$schema")]
    _schema: Option<Arc<str>>,
}

impl Schema {
    pub fn load_sources(&self, sources: &mut Vec<config::Source>, root: &Path) -> Result {
        for source in self.sources.iter() {
            sources.push(config::Source {
                // TODO add context to error
                pattern: source.pattern.parse().into_diagnostic()?,
                comment_style: (&source.comment_style).into(),
                default_type: source.default_type.into(),
                root: root.clone(),
                blob_link: source.blob_link.as_ref().map(From::from),
            });
        }

        Ok(())
    }

    pub fn load_requirements(
        &self,
        requirements: &mut Vec<config::Requirement>,
        root: &Path,
    ) -> Result {
        // include several default paths
        for pattern in [
            ".duvet/requirements/**/*.toml",
            ".duvet/todos/**/*.toml",
            ".duvet/exceptions/**/*.toml",
        ] {
            requirements.push(config::Requirement {
                pattern: pattern.parse().into_diagnostic()?,
                root: root.clone(),
            })
        }

        for requirement in self.requirements.iter() {
            requirements.push(config::Requirement {
                // TODO add context to error
                pattern: requirement.pattern.parse().into_diagnostic()?,
                root: root.clone(),
            });
        }

        Ok(())
    }

    pub fn load_specifications(
        &self,
        specifications: &mut Vec<config::Specification>,
        _root: &Path,
    ) -> Result {
        for spec in self.specifications.iter() {
            let path = spec.source.parse::<TargetPath>()?;
            let format = spec
                .format
                .map(From::from)
                .unwrap_or_else(|| crate::specification::Format::Auto);

            let target = Target { path, format }.into();
            specifications.push(config::Specification { target });
        }

        Ok(())
    }

    pub fn download_path(&self, config: &Path, _root: &Path) -> Path {
        config.parent().unwrap().join("specifications").into()
    }

    pub fn requirements_path(&self, config: &Path, _root: &Path) -> Path {
        config.parent().unwrap().join("requirements").into()
    }

    pub fn report(&self, _config: &Path, _root: &Path) -> config::Report {
        (&*self.report).into()
    }

    /// Convert the raw `[duplicates]` section into the validated policy.
    ///
    /// //= design/duplicates/spec.md#policy-source
    /// //# Every policy value in this specification MUST be read from
    /// //# checked-in configuration.
    pub fn duplicates(&self) -> Result<config::DuplicatesPolicy> {
        let Some(schema) = &self.duplicates else {
            return Ok(config::DuplicatesPolicy::default());
        };

        let mut policy = config::DuplicatesPolicy::default();

        if let Some(claims) = &schema.claims {
            if let Some(test) = claims.test {
                policy.claims.test = positive_cap(test, "duplicates.claims.test")?;
            }
            if let Some(implementation) = claims.implementation {
                policy.claims.implementation =
                    positive_cap(implementation, "duplicates.claims.implementation")?;
            }
            if let Some(types) = &claims.types {
                policy.claims.types = Some(config::parse_form_family(types)?);
            }
        }

        if let Some(targets) = &schema.targets {
            if let Some(count) = targets.count {
                if count == 0 {
                    return Err(duvet_core::error!(
                        "duplicates.targets.count must be a positive integer"
                    ));
                }
                policy.targets.count = Some(count);
            }
            if let Some(sections) = targets.sections {
                if sections == 0 {
                    return Err(duvet_core::error!(
                        "duplicates.targets.sections must be a positive integer"
                    ));
                }
                policy.targets.sections = Some(sections);
            }
            if let Some(types) = &targets.types {
                policy.targets.types = Some(config::parse_form_family(types)?);
            }
        }

        Ok(policy)
    }
}

fn positive_cap(value: u32, key: &str) -> Result<u32> {
    if value == 0 {
        return Err(duvet_core::error!("{key} must be a positive integer"));
    }
    Ok(value)
}

/// The `[duplicates]` configuration section (design/duplicates/spec.md §4.2).
/// Two namespaces mirror the two coincidence axes; every setting lives in
/// exactly one namespace.
///
/// //= design/duplicates/spec.md#schema-shared
/// //# An unrecognized key under `[duplicates.claims]` or
/// //# `[duplicates.targets]` MUST be a configuration error, so a typo
/// //# never silently takes a default.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct DuplicatesSchema {
    #[serde(default)]
    pub claims: Option<DuplicatesClaims>,
    #[serde(default)]
    pub targets: Option<DuplicatesTargets>,
}

/// `[duplicates.claims]` — predicates over claim classes.
///
/// //= design/duplicates/spec.md#caps
/// //# Caps MUST be configurable only for the priced forms.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct DuplicatesClaims {
    /// Cap for `test` duplicate sets. Default 1.
    #[serde(default)]
    pub test: Option<u32>,
    /// Cap for `implementation` duplicate sets. Default 1. `citation` is
    /// accepted as an input alias and never emitted.
    #[serde(default, alias = "citation")]
    pub implementation: Option<u32>,
    /// The claim-form family: allowed coexisting form sets, `+`-joined
    /// (e.g. `["exception+test"]`). Unset = default family (no free form
    /// shares a claim).
    #[serde(default)]
    pub types: Option<Vec<String>>,
}

/// `[duplicates.targets]` — predicates over target classes. Bounds are
/// opt-in: unlimited when unset.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct DuplicatesTargets {
    /// Fan-in bound: maximum annotations per target.
    #[serde(default)]
    pub count: Option<u64>,
    /// Maximum distinct sections per target.
    #[serde(default)]
    pub sections: Option<u64>,
    /// The target-form family: allowed coexisting form sets, `+`-joined.
    /// Unset = default family (everything except `test`+`implementation`).
    #[serde(default)]
    pub types: Option<Vec<String>>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct Source {
    pub pattern: String,
    #[serde(default, rename = "comment-style")]
    pub comment_style: CommentStyle,
    #[serde(rename = "type", default)]
    pub default_type: DefaultType,
    #[serde(default, rename = "blob-link")]
    pub blob_link: Option<TemplatedString>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub enum DefaultType {
    #[default]
    #[serde(rename = "implementation")]
    Implementation,
    #[serde(rename = "spec")]
    Spec,
    #[serde(rename = "test")]
    Test,
    #[serde(rename = "exception")]
    Exception,
    #[serde(rename = "todo")]
    Todo,
    #[serde(rename = "implication")]
    Implication,
}

impl From<DefaultType> for AnnotationType {
    fn from(value: DefaultType) -> Self {
        match value {
            DefaultType::Implementation => Self::Citation,
            DefaultType::Spec => Self::Spec,
            DefaultType::Test => Self::Test,
            DefaultType::Todo => Self::Todo,
            DefaultType::Exception => Self::Exception,
            DefaultType::Implication => Self::Implication,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct CommentStyle {
    #[serde(default = "default_meta")]
    pub meta: Arc<str>,
    #[serde(default = "default_content")]
    pub content: Arc<str>,
}

fn default_meta() -> Arc<str> {
    Arc::from("//=")
}

fn default_content() -> Arc<str> {
    Arc::from("//#")
}

impl Default for CommentStyle {
    fn default() -> Self {
        Self {
            meta: default_meta(),
            content: default_content(),
        }
    }
}

impl From<&CommentStyle> for crate::comment::Pattern {
    fn from(value: &CommentStyle) -> Self {
        Self {
            meta: value.meta.clone(),
            content: value.content.clone(),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct Requirement {
    pub pattern: String,
}

#[derive(Clone, Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct Report {
    #[serde(default)]
    pub html: Option<HtmlReport>,
    #[serde(default)]
    pub json: Option<JsonReport>,
    #[serde(default)]
    pub snapshot: Option<SnapshotReport>,
}

impl From<&Report> for config::Report {
    fn from(value: &Report) -> Self {
        Self {
            html: value
                .html
                .as_ref()
                .map(From::from)
                .unwrap_or_else(|| (&HtmlReport::default()).into()),
            json: value
                .json
                .as_ref()
                .map(From::from)
                .unwrap_or_else(|| (&JsonReport::default()).into()),
            snapshot: value
                .snapshot
                .as_ref()
                .map(From::from)
                .unwrap_or_else(|| (&SnapshotReport::default()).into()),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct HtmlReport {
    #[serde(default = "HtmlReport::default_enabled")]
    pub enabled: bool,
    #[serde(default = "HtmlReport::default_path")]
    pub path: String,
    #[serde(default, rename = "blob-link")]
    pub blob_link: Option<TemplatedString>,
    #[serde(default, rename = "issue-link")]
    pub issue_link: Option<TemplatedString>,
}

impl Default for HtmlReport {
    fn default() -> Self {
        Self {
            enabled: Self::default_enabled(),
            path: Self::default_path(),
            blob_link: None,
            issue_link: None,
        }
    }
}

impl HtmlReport {
    fn default_enabled() -> bool {
        true
    }

    fn default_path() -> String {
        ".duvet/reports/report.html".into()
    }
}

impl From<&HtmlReport> for config::HtmlReport {
    fn from(value: &HtmlReport) -> Self {
        Self {
            enabled: value.enabled,
            path: value.path.as_str().into(),
            issue_link: value.issue_link.as_ref().map(From::from),
            blob_link: value.blob_link.as_ref().map(From::from),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct JsonReport {
    #[serde(default = "JsonReport::default_enabled")]
    pub enabled: bool,
    #[serde(default = "JsonReport::default_path")]
    pub path: String,
}

impl Default for JsonReport {
    fn default() -> Self {
        Self {
            enabled: Self::default_enabled(),
            path: Self::default_path(),
        }
    }
}

impl JsonReport {
    fn default_enabled() -> bool {
        false
    }

    fn default_path() -> String {
        ".duvet/reports/report.json".into()
    }
}

impl From<&JsonReport> for config::JsonReport {
    fn from(value: &JsonReport) -> Self {
        Self {
            enabled: value.enabled,
            path: value.path.as_str().into(),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct SnapshotReport {
    #[serde(default = "SnapshotReport::default_enabled")]
    pub enabled: bool,
    #[serde(default = "SnapshotReport::default_path")]
    pub path: String,
}

impl Default for SnapshotReport {
    fn default() -> Self {
        Self {
            enabled: Self::default_enabled(),
            path: Self::default_path(),
        }
    }
}

impl SnapshotReport {
    fn default_enabled() -> bool {
        false
    }

    fn default_path() -> String {
        ".duvet/snapshot.txt".into()
    }
}

impl From<&SnapshotReport> for config::SnapshotReport {
    fn from(value: &SnapshotReport) -> Self {
        Self {
            enabled: value.enabled,
            path: value.path.as_str().into(),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct Specification {
    #[serde(default)]
    pub source: String,
    pub format: Option<SpecificationFormat>,
}

#[derive(Copy, Clone, Debug, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub enum SpecificationFormat {
    #[serde(rename = "ietf", alias = "IETF")]
    Ietf,
    #[serde(rename = "markdown", alias = "md")]
    Markdown,
}

impl From<SpecificationFormat> for crate::specification::Format {
    fn from(value: SpecificationFormat) -> Self {
        match value {
            SpecificationFormat::Ietf => Self::Ietf,
            SpecificationFormat::Markdown => Self::Markdown,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_test() {
        let mut schema = schemars::schema_for!(Schema);

        schema.insert(
            "title".to_string(),
            serde_json::Value::String("Duvet Configuration".to_string()),
        );

        schema.insert(
            "$id".to_string(),
            serde_json::Value::String(
                "https://awslabs.github.io/duvet/config/v0.4.0.json".to_string(),
            ),
        );

        duvet_core::artifact::sync(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../config/v0.4.0.json"),
            serde_json::to_string_pretty(&schema).unwrap(),
        );

        schema.insert(
            "$id".to_string(),
            serde_json::Value::String(
                "https://awslabs.github.io/duvet/config/v0.4.json".to_string(),
            ),
        );
        duvet_core::artifact::sync(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../config/v0.4.json"),
            serde_json::to_string_pretty(&schema).unwrap(),
        );
    }
}
