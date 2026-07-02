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
use duvet_core::{path::Path, progress};
use std::{collections::BTreeMap, sync::Arc};

pub(crate) mod ci;
mod html;
mod json;
pub(crate) mod json_v2;
pub(crate) mod json_v2_merge;
mod lcov;
mod snapshot;
mod stats;
pub(crate) mod status;

use stats::Statistics;

#[derive(Debug, Parser)]
pub struct Report {
    #[clap(flatten)]
    project: Project,

    #[clap(long)]
    lcov: Option<Path>,

    #[clap(long)]
    json: Option<Path>,

    #[clap(long)]
    json_v2: Option<Path>,

    #[clap(long)]
    html: Option<Path>,

    #[clap(long)]
    require_citations: Option<Option<bool>>,

    #[clap(long)]
    require_tests: Option<Option<bool>>,

    #[clap(long)]
    ci: Option<Option<bool>>,

    #[clap(long)]
    blob_link: Option<String>,

    #[clap(long)]
    issue_link: Option<String>,
}

/// Runs the full coverage-analysis pipeline: extract requirements, scan
/// sources, parse annotations, load specifications, and match references,
/// returning the sorted [`ReportResult`].
///
/// This is the shared core behind `duvet report` and the library API (see
/// [`crate::api`]). It reads and writes exclusively through the
/// [`duvet_core::vfs`] seam, so it runs unchanged against the real filesystem
/// or an in-memory one.
pub async fn analyze(
    project: &Project,
    require_citations: bool,
    require_tests: bool,
    blob_link: Option<String>,
    issue_link: Option<String>,
) -> Result<ReportResult> {
    let config = project.config().await?;
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
    let project_sources = project.sources().await?;
    let project_sources = Arc::new(project_sources);
    progress!(progress, "Scanned {} sources", project_sources.len());

    let progress = progress!("Parsing annotations");
    let annotations = crate::annotation::query(project_sources.clone()).await?;
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
    let blob_link = blob_link
        .or_else(|| config.and_then(|config| config.report.html.blob_link.as_deref().map(String::from)));
    let issue_link = issue_link
        .or_else(|| config.and_then(|config| config.report.html.issue_link.as_deref().map(String::from)));
    let mut targets: BTreeMap<Arc<Target>, TargetReport> = Default::default();
    let references = reference::query(reference_map.clone(), specifications.clone()).await?;
    progress!(progress, "Matched {} references", references.len());

    let progress = progress!("Sorting references");
    for reference in references.iter() {
        targets
            .entry(reference.target.clone())
            .or_insert_with(|| {
                let specification = specifications.get(&reference.target).unwrap().clone();
                TargetReport {
                    references: vec![],
                    specification,
                    require_citations,
                    require_tests,
                    statuses: Default::default(),
                }
            })
            .references
            .push(reference.clone());
    }

    targets.iter_mut().for_each(|(_, target)| {
        target.references.sort();
        target.statuses.populate(&target.references)
    });

    progress!(progress, "Sorted {} references", references.len());

    Ok(ReportResult {
        targets,
        annotations,
        blob_link,
        issue_link,
        download_path,
    })
}

impl Report {
    pub async fn exec(&self) -> Result {
        let config = self.project.config().await?;
        let config = config.as_ref();

        let analysis = analyze(
            &self.project,
            self.require_citations(),
            self.require_tests(),
            self.blob_link.clone(),
            self.issue_link.clone(),
        )
        .await?;

        let report = analysis;

        type ReportFn = fn(&ReportResult, &Path) -> crate::Result<()>;

        let internal_json = std::env::var("DUVET_INTERNAL_CI_JSON").ok().map(Path::from);
        let internal_json_v2 = std::env::var("DUVET_INTERNAL_CI_JSON_V2")
            .ok()
            .map(Path::from);
        let internal_html = std::env::var("DUVET_INTERNAL_CI_HTML").ok().map(Path::from);
        let internal_snapshot = std::env::var("DUVET_INTERNAL_CI_SNAPSHOT")
            .ok()
            .map(Path::from);
        let snapshot_path = config.and_then(|config| config.report.snapshot.path());

        let ci = match self.ci {
            // use the new CI environment variable if the snapshot reports are configured
            None if snapshot_path.is_some() => {
                std::env::var("CI").is_ok_and(|value| !["false", "0"].contains(&value.as_str()))
            }
            None => false,
            Some(None) => true,
            Some(Some(value)) => value,
        };

        let reports: &[(Option<&_>, ReportFn)] = &[
            (self.json.as_ref(), json::report),
            (self.json_v2.as_ref(), json_v2::report),
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
            (internal_json_v2.as_ref(), json_v2::report),
            (internal_html.as_ref(), html::report),
            // the duvet CI uses its own snapshotting mechanism
            (internal_snapshot.as_ref(), snapshot::report),
        ];

        for (path, report_fn) in reports {
            if let Some(path) = path {
                let is_snapshot_ci =
                    std::ptr::fn_addr_eq(snapshot::report_ci as ReportFn, *report_fn);
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

        // only use the old CI check if the config hasn't set up snapshots
        if ci && snapshot_path.is_none() {
            ci::report(&report)?;
        }

        Ok(())
    }

    fn require_citations(&self) -> bool {
        match self.require_citations {
            None => true,
            Some(None) => true,
            Some(Some(value)) => value,
        }
    }

    fn require_tests(&self) -> bool {
        match self.require_tests {
            None => true,
            Some(None) => true,
            Some(Some(value)) => value,
        }
    }
}

#[derive(Debug)]
pub struct ReportResult {
    pub targets: BTreeMap<Arc<Target>, TargetReport>,
    pub annotations: AnnotationSet,
    pub blob_link: Option<String>,
    pub issue_link: Option<String>,
    pub download_path: Path,
}

#[derive(Debug)]
pub struct TargetReport {
    pub(crate) references: Vec<Reference>,
    pub(crate) specification: Arc<Specification>,
    pub(crate) require_citations: bool,
    pub(crate) require_tests: bool,
    pub(crate) statuses: status::StatusMap,
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
