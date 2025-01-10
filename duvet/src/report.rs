// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    annotation::{Annotation, AnnotationLevel, AnnotationSet, AnnotationSetExt},
    project::Project,
    specification::Specification,
    target::Target,
    Result,
};
use clap::Parser;
use core::fmt;
use duvet_core::error;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::PathBuf,
    sync::Arc,
};

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
    lcov: Option<PathBuf>,

    #[clap(long)]
    json: Option<PathBuf>,

    #[clap(long)]
    html: Option<PathBuf>,

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

#[derive(Clone, Debug, PartialEq, PartialOrd, Eq, Ord)]
struct Reference<'a> {
    line: usize,
    start: usize,
    end: usize,
    annotation_id: usize,
    annotation: &'a Annotation,
    level: AnnotationLevel,
}

impl Reference<'_> {
    pub fn start(&self) -> usize {
        self.start
    }

    pub fn end(&self) -> usize {
        self.end
    }

    pub fn line(&self) -> usize {
        self.line
    }
}

#[derive(Debug)]
enum ReportError<'a> {
    QuoteMismatch { annotation: &'a Annotation },
    MissingSection { annotation: &'a Annotation },
}

impl fmt::Display for ReportError<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::QuoteMismatch { annotation } => write!(
                f,
                "{}#{} - quote not found in {:?}",
                annotation.source.display(),
                annotation.anno_line,
                annotation.target,
            ),
            Self::MissingSection { annotation } => write!(
                f,
                "{}#{} - section {:?} not found in {:?}",
                annotation.source.display(),
                annotation.anno_line,
                annotation.target_section().as_deref().unwrap_or("-"),
                annotation.target_path(),
            ),
        }
    }
}

impl Report {
    pub async fn exec(&self) -> Result {
        let project_sources = self.project.sources()?;
        let project_sources = Arc::new(project_sources);

        let annotations = crate::annotation::query(project_sources.clone()).await?;

        let targets = annotations.targets()?;

        let mut contents = HashMap::new();
        for target in targets.iter() {
            let spec_path = self.project.spec_path.as_ref();
            let file = target.path.load(spec_path).await?;

            contents.insert(target, file);
        }
        let spec_path = self.project.spec_path.clone();
        let specifications =
            crate::annotation::specifications(annotations.clone(), spec_path).await?;

        let reference_map = annotations.reference_map()?;

        let results: Vec<_> = reference_map
            .iter()
            .flat_map(|((target, section_id), annotations)| {
                let spec = specifications.get(target).expect("spec already checked");

                let mut results = vec![];

                if let Some(section_id) = section_id {
                    if let Some(section) = spec.section(section_id) {
                        let contents = section.view();

                        for (annotation_id, annotation) in annotations {
                            if annotation.quote.is_empty() {
                                // empty quotes don't count towards coverage but are still
                                // references
                                let text = section.full_title.clone();
                                results.push(Ok((
                                    target,
                                    Reference {
                                        line: text.line(),
                                        start: text.range().start,
                                        end: text.range().end,
                                        annotation,
                                        annotation_id: *annotation_id,
                                        level: annotation.level,
                                    },
                                )));
                                continue;
                            }

                            if let Some(range) = annotation.quote_range(&contents) {
                                for text in contents.ranges(range) {
                                    results.push(Ok((
                                        target,
                                        Reference {
                                            line: text.line(),
                                            start: text.range().start,
                                            end: text.range().end,
                                            annotation,
                                            annotation_id: *annotation_id,
                                            level: annotation.level,
                                        },
                                    )));
                                }
                            } else {
                                results
                                    .push(Err((target, ReportError::QuoteMismatch { annotation })));
                            }
                        }
                    } else {
                        for (_, annotation) in annotations {
                            results.push(Err((target, ReportError::MissingSection { annotation })));
                        }
                    }
                } else {
                    // TODO
                    eprintln!("TOTAL REFERENCE {:?}", annotations);
                }

                // TODO upgrade levels whenever they overlap

                results
            })
            .collect();

        let mut report = ReportResult {
            targets: Default::default(),
            annotations: &annotations,
            blob_link: self.blob_link.as_deref(),
            issue_link: self.issue_link.as_deref(),
        };
        let mut errors = BTreeSet::new();

        for result in results {
            let (target, result) = match result {
                Ok((target, entry)) => (target, Ok(entry)),
                Err((target, err)) => (target, Err(err)),
            };

            let entry = report
                .targets
                .entry(target)
                .or_insert_with(|| TargetReport {
                    target,
                    references: BTreeSet::new(),
                    specification: specifications.get(target).expect("content should exist"),
                    require_citations: self.require_citations(),
                    require_tests: self.require_tests(),
                    statuses: Default::default(),
                });

            match result {
                Ok(reference) => {
                    entry.references.insert(reference);
                }
                Err(err) => {
                    errors.insert(err.to_string());
                }
            }
        }

        if !errors.is_empty() {
            for error in &errors {
                eprintln!("{}", error);
            }

            return Err(error!(
                "source errors were found. no reports were generated"
            ));
        }

        report
            .targets
            .iter_mut()
            .for_each(|(_, target)| target.statuses.populate(&target.references));

        if let Some(dir) = &self.lcov {
            lcov::report(&report, dir)?;
        }

        if let Some(file) = &self.json {
            json::report(&report, file)?;
        }

        if let Some(dir) = &self.html {
            html::report(&report, dir)?;
        }

        // used for internal duvet CI checks
        if let Ok(file) = std::env::var("DUVET_INTERNAL_CI_JSON") {
            json::report(&report, std::path::Path::new(&file))?;
        }

        // used for internal duvet CI checks
        if let Ok(file) = std::env::var("DUVET_INTERNAL_CI_HTML") {
            html::report(&report, std::path::Path::new(&file))?;
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
    pub targets: BTreeMap<&'a Target, TargetReport<'a>>,
    pub annotations: &'a AnnotationSet,
    pub blob_link: Option<&'a str>,
    pub issue_link: Option<&'a str>,
}

#[derive(Debug)]
pub struct TargetReport<'a> {
    target: &'a Target,
    references: BTreeSet<Reference<'a>>,
    specification: &'a Specification,
    require_citations: bool,
    require_tests: bool,
    statuses: status::StatusMap,
}

impl TargetReport<'_> {
    #[allow(dead_code)]
    pub fn statistics(&self) -> Statistics {
        let mut stats = Statistics::default();

        for reference in &self.references {
            stats.record(reference);
        }

        stats
    }
}
