// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    annotation::{self, AnnotationSet},
    project::Project,
    reference::{self, Reference},
    specification::Specification,
    target::Target,
    Result,
};
use clap::Parser;
use duvet_core::{error, path::Path, progress};
use std::{collections::BTreeMap, sync::Arc};

mod ci;
mod html;
mod json;
mod lcov;
mod snapshot;
mod stats;
mod status;
#[cfg(test)]
mod tests;

use stats::Statistics;

#[derive(Debug, Clone)]
enum RequirementMode {
    None,                               // No requirements specified
    Global(bool),                       // All values are boolean (true/false)
    Targeted(Vec<TargetedRequirement>), // All values are spec paths
}

#[derive(Debug, Clone)]
struct TargetedRequirement {
    path: String,
    section: Option<String>, // Some("section") for "path#section", None for "path"
}

#[derive(Debug, Parser)]
pub struct Report {
    #[clap(flatten)]
    project: Project,

    #[clap(long)]
    lcov: Option<Path>,

    #[clap(long)]
    json: Option<Path>,

    #[clap(long)]
    html: Option<Path>,

    #[clap(long, action = clap::ArgAction::Append)]
    require_citations: Vec<String>,

    #[clap(long, action = clap::ArgAction::Append)]
    require_tests: Vec<String>,

    #[clap(long, action = clap::ArgAction::SetTrue)]
    ci: bool,

    #[clap(long)]
    blob_link: Option<String>,

    #[clap(long)]
    issue_link: Option<String>,
}

impl Report {
    pub async fn exec(&self) -> Result {
        // Validate requirement formats early to catch mixing errors
        let citations_mode = self.parse_requirements(&self.require_citations)?;
        let tests_mode = self.parse_requirements(&self.require_tests)?;

        let config = self.project.config().await?;
        let config = config.as_ref();

        if let Some(config) = config {
            let progress = progress!("Extracting requirements");
            let count = config.load_specifications().await?;
            if count > 0 {
                progress!(
                    progress,
                    "Extracted requirements from {count} specifications"
                );
            } else {
                progress!(
                    progress,
                    "Extracted no requirements - config does not include any specifications"
                )
            }
        }

        let progress = progress!("Scanning sources");
        let project_sources = self.project.sources().await?;
        let project_sources = Arc::new(project_sources);
        progress!(progress, "Scanned {} sources", project_sources.len());

        let progress = progress!("Parsing annotations");
        let annotations = crate::annotation::query(project_sources.clone()).await?;
        progress!(progress, "Parsed {} annotations", annotations.len());

        let progress = progress!("Loading specifications");
        let download_path = self.project.download_path().await?;
        let specifications =
            annotation::specifications(annotations.clone(), download_path.clone()).await?;
        progress!(progress, "Loaded {} specifications", specifications.len());

        let progress = progress!("Mapping sections");
        let reference_map = annotation::reference_map(annotations.clone()).await?;
        progress!(progress, "Mapped {} sections", reference_map.len());

        let progress = progress!("Matching references");
        let blob_link = self
            .blob_link
            .as_deref()
            .or_else(|| config.and_then(|config| config.report.html.blob_link.as_deref()));
        let issue_link = self
            .issue_link
            .as_deref()
            .or_else(|| config.and_then(|config| config.report.html.issue_link.as_deref()));
        let mut report = ReportResult {
            targets: Default::default(),
            annotations,
            blob_link,
            issue_link,
            download_path: &download_path,
        };
        let references = reference::query(reference_map.clone(), specifications.clone()).await?;
        progress!(progress, "Matched {} references", references.len());

        let progress = progress!("Sorting references");

        for reference in references.iter() {
            report
                .targets
                .entry(reference.target.clone())
                .or_insert_with(|| {
                    let specification = specifications.get(&reference.target).unwrap().clone();
                    TargetReport {
                        references: vec![],
                        specification,
                        require_citations: citations_mode.clone(),
                        require_tests: tests_mode.clone(),
                        statuses: Default::default(),
                    }
                })
                .references
                .push(reference.clone());
        }

        report.targets.iter_mut().for_each(|(_, target)| {
            target.references.sort();
            target.statuses.populate(&target.references)
        });

        progress!(progress, "Sorted {} references", references.len());

        type ReportFn = fn(&ReportResult, &Path) -> crate::Result<()>;

        let internal_json = std::env::var("DUVET_INTERNAL_CI_JSON").ok().map(Path::from);
        let internal_html = std::env::var("DUVET_INTERNAL_CI_HTML").ok().map(Path::from);
        let internal_snapshot = std::env::var("DUVET_INTERNAL_CI_SNAPSHOT")
            .ok()
            .map(Path::from);
        let snapshot_path = config.and_then(|config| config.report.snapshot.path());

        let ci = if !self.ci && snapshot_path.is_some() {
            // If --ci is not explicitly set but snapshots are configured, check CI env var
            std::env::var("CI").is_ok_and(|value| !["false", "0"].contains(&value.as_str()))
        } else {
            // Otherwise use the flag value
            self.ci
        };

        let reports: &[(Option<&_>, ReportFn)] = &[
            (self.json.as_ref(), json::report),
            (self.html.as_ref(), html::report),
            (self.lcov.as_ref(), lcov::report),
            (
                config.and_then(|config| config.report.json.path()),
                json::report,
            ),
            (
                config.and_then(|config| config.report.html.path()),
                html::report,
            ),
            (
                snapshot_path,
                if ci {
                    snapshot::report_ci
                } else {
                    snapshot::report
                },
            ),
            (internal_json.as_ref(), json::report),
            (internal_html.as_ref(), html::report),
            // the duvet CI uses its own snapshotting mechanism
            (internal_snapshot.as_ref(), snapshot::report),
        ];

        for (path, report_fn) in reports {
            if let Some(path) = path {
                let is_snapshot_ci = snapshot::report_ci as ReportFn == *report_fn;
                let progress = if is_snapshot_ci {
                    progress!("Checking {path}")
                } else {
                    progress!("Writing {path}")
                };
                report_fn(&report, path)?;
                if is_snapshot_ci {
                    progress!(progress, "Checked {path}");
                } else {
                    progress!(progress, "Wrote {path}");
                }
            }
        }

        // Run citation validation checks if required, regardless of snapshot configuration
        if self.require_citations() || self.require_tests() || (ci && snapshot_path.is_none()) {
            ci::report(&report)?;
        }

        Ok(())
    }

    fn require_citations(&self) -> bool {
        match self.parse_requirements(&self.require_citations) {
            Ok(RequirementMode::Global(value)) => value,
            Ok(RequirementMode::Targeted(_)) => true, // If targeted requirements exist, enable validation
            _ => false,
        }
    }

    fn require_tests(&self) -> bool {
        match self.parse_requirements(&self.require_tests) {
            Ok(RequirementMode::Global(value)) => value,
            Ok(RequirementMode::Targeted(_)) => true, // If targeted requirements exist, enable validation
            _ => false,
        }
    }

    fn parse_requirements(&self, values: &[String]) -> Result<RequirementMode> {
        if values.is_empty() {
            return Ok(RequirementMode::None);
        }

        let mut has_boolean = false;
        let mut has_paths = false;

        for value in values {
            match value.as_str() {
                "true" | "false" => has_boolean = true,
                _ => has_paths = true,
            }
        }

        if has_boolean && has_paths {
            return Err(error!(
                "Cannot mix boolean values (true/false) with spec paths in requirement flags. \
                 Use either --require-citations true/false OR --require-citations path/to/spec.md"
            ));
        }

        if has_boolean {
            // All should be boolean, validate and return the effective value
            let mut results = Vec::new();
            for value in values {
                match value.parse::<bool>() {
                    Ok(b) => results.push(b),
                    Err(_) => {
                        return Err(error!(
                            "Invalid boolean value '{}', expected 'true' or 'false'",
                            value
                        ))
                    }
                }
            }
            let effective = results.into_iter().any(|b| b); // true if any value is true
            Ok(RequirementMode::Global(effective))
        } else {
            // All should be spec paths
            let mut targeted = Vec::new();
            for value in values {
                targeted.push(self.parse_targeted_requirement(value)?);
            }
            Ok(RequirementMode::Targeted(targeted))
        }
    }

    fn parse_targeted_requirement(&self, value: &str) -> Result<TargetedRequirement> {
        if let Some((path, section)) = value.split_once('#') {
            Ok(TargetedRequirement {
                path: path.to_string(),
                section: Some(section.to_string()),
            })
        } else {
            Ok(TargetedRequirement {
                path: value.to_string(),
                section: None,
            })
        }
    }
}

#[derive(Debug)]
pub struct ReportResult<'a> {
    pub targets: BTreeMap<Arc<Target>, TargetReport>,
    pub annotations: AnnotationSet,
    pub blob_link: Option<&'a str>,
    pub issue_link: Option<&'a str>,
    pub download_path: &'a Path,
}

#[derive(Debug)]
pub struct TargetReport {
    references: Vec<Reference>,
    specification: Arc<Specification>,
    require_citations: RequirementMode,
    require_tests: RequirementMode,
    statuses: status::StatusMap,
}

impl TargetReport {
    #[allow(dead_code)]
    pub fn statistics(&self) -> Statistics {
        let mut stats = Statistics::default();

        for reference in &self.references {
            stats.record(reference);
        }

        stats
    }
}
