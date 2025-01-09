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

mod ci;
mod html;
mod json;
mod lcov;
mod stats;
mod status;

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
    html: Option<Path>,

    #[clap(long)]
    require_citations: Option<Option<bool>>,

    #[clap(long)]
    require_tests: Option<Option<bool>>,

    #[clap(long)]
    ci: bool,

    #[clap(long)]
    blob_link: Option<String>,

    #[clap(long)]
    issue_link: Option<String>,
}

impl Report {
    pub async fn exec(&self) -> Result {
        let progress = progress!("Scanning sources");
        let project_sources = self.project.sources().await?;
        let project_sources = Arc::new(project_sources);
        progress!(progress, "Scanned {} sources", project_sources.len());

        let progress = progress!("Extracing annotations");
        let annotations = crate::annotation::query(project_sources.clone()).await?;
        progress!(progress, "Extracted {} annotations", annotations.len());

        let progress = progress!("Loading specifications");
        let spec_path = self.project.spec_path.clone();
        let specifications = annotation::specifications(annotations.clone(), spec_path).await?;
        progress!(progress, "Loaded {} specifications", specifications.len());

        let progress = progress!("Mapping sections");
        let reference_map = annotation::reference_map(annotations.clone()).await?;
        progress!(progress, "Mapped {} sections", reference_map.len());

        let progress = progress!("Matching references");
        let mut report = ReportResult {
            targets: Default::default(),
            annotations,
            blob_link: self.blob_link.as_deref(),
            issue_link: self.issue_link.as_deref(),
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
                        require_citations: self.require_citations(),
                        require_tests: self.require_tests(),
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

        let reports: &[(&Option<_>, ReportFn)] = &[
            (&self.json, json::report),
            (&self.html, html::report),
            (&self.lcov, lcov::report),
            (
                &std::env::var("DUVET_INTERNAL_CI_JSON").ok().map(Path::from),
                json::report,
            ),
            (
                &std::env::var("DUVET_INTERNAL_CI_HTML").ok().map(Path::from),
                html::report,
            ),
        ];

        for (path, report_fn) in reports {
            if let Some(path) = path {
                let progress = progress!("Writing {path}");
                report_fn(&report, path)?;
                progress!(progress, "Wrote {path}");
            }
        }

        if self.ci {
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
pub struct ReportResult<'a> {
    pub targets: BTreeMap<Arc<Target>, TargetReport>,
    pub annotations: AnnotationSet,
    pub blob_link: Option<&'a str>,
    pub issue_link: Option<&'a str>,
}

#[derive(Debug)]
pub struct TargetReport {
    references: Vec<Reference>,
    specification: Arc<Specification>,
    require_citations: bool,
    require_tests: bool,
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
