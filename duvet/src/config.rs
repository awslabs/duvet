// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::{extract::Extraction, Result};
use duvet_core::{path::Path, vfs};
use std::sync::Arc;

pub mod schema;

#[derive(Clone, Debug)]
pub struct Config {
    pub sources: Vec<Source>,
    pub requirements: Vec<Requirement>,
    pub specifications: Vec<Specification>,
    pub report: Report,
    pub requirements_path: Path,
    pub download_path: Path,
}

impl Config {
    pub async fn load_specifications(&self) -> Result<usize> {
        let download_path = &self.download_path;
        let requirements_path = &self.requirements_path;

        for spec in &self.specifications {
            Extraction {
                download_path,
                base_path: Some(download_path),
                target: spec.target.clone(),
                out: requirements_path,
                extension: "toml",
                // don't log to reduce noise
                log: false,
            }
            .exec()
            .await?;
        }

        Ok(self.specifications.len())
    }
}

#[derive(Clone, Debug)]
pub struct Source {
    pub pattern: String,
    pub root: Path,
    pub comment_style: crate::comment::Pattern,
    pub default_type: crate::annotation::AnnotationType,
}

#[derive(Clone, Debug)]
pub struct Requirement {
    pub pattern: String,
    pub root: Path,
}

#[derive(Clone, Debug)]
pub struct Report {
    pub html: HtmlReport,
    pub json: JsonReport,
    pub snapshot: SnapshotReport,
}

#[derive(Clone, Debug)]
pub struct HtmlReport {
    pub enabled: bool,
    pub path: Path,
    pub blob_link: Option<Arc<str>>,
    pub issue_link: Option<Arc<str>>,
}

impl HtmlReport {
    pub fn path(&self) -> Option<&Path> {
        Some(&self.path).filter(|_| self.enabled)
    }
}

#[derive(Clone, Debug)]
pub struct JsonReport {
    pub enabled: bool,
    pub path: Path,
}

impl JsonReport {
    pub fn path(&self) -> Option<&Path> {
        Some(&self.path).filter(|_| self.enabled)
    }
}

#[derive(Clone, Debug)]
pub struct SnapshotReport {
    pub enabled: bool,
    pub path: Path,
}

impl SnapshotReport {
    pub fn path(&self) -> Option<&Path> {
        Some(&self.path).filter(|_| self.enabled)
    }
}

#[derive(Clone, Debug)]
pub struct Specification {
    pub target: Arc<crate::target::Target>,
}

pub async fn load(path: Path, root: Path) -> Result<Arc<Config>> {
    let file = vfs::read_string(path.clone()).await?;
    let schema: Arc<schema::Schema> = file.as_toml().await?;

    let mut sources = vec![];
    let mut requirements = vec![];
    let mut specifications = vec![];

    schema.load_sources(&mut sources, &root)?;
    schema.load_requirements(&mut requirements, &root)?;
    schema.load_specifications(&mut specifications, &root)?;

    let requirements_path = schema.requirements_path(&path, &root);
    let download_path = schema.download_path(&path, &root);
    let report = schema.report(&path, &root);

    Ok(Arc::new(Config {
        sources,
        requirements,
        specifications,
        requirements_path,
        download_path,
        report,
    }))
}

pub async fn default_path_and_root() -> Option<(Path, Path)> {
    let root = duvet_core::env::current_dir().ok()?;
    let path = root.join(".duvet").join("config.toml");

    // check to see if it exists
    let _ = vfs::read_metadata(&path).await.ok()?;

    Some((path, root))
}
