// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::{comment, config, source::SourceFile, Result};
use clap::Parser;
use duvet_core::{diagnostic::IntoDiagnostic, path::Path};
use glob::glob;
use std::{collections::HashSet, sync::Arc};

#[derive(Debug, PartialEq, PartialOrd, Eq, Ord, Hash, Parser)]
pub struct Project {
    #[clap(flatten)]
    deprecated: Deprecated,

    #[clap(long)]
    config_path: Option<Path>,
}

impl Project {
    pub fn new() -> Self {
        Self {
            deprecated: Deprecated::default(),
            config_path: None,
        }
    }

    pub async fn download_path(&self) -> Result<Path> {
        if let Some(config) = self.config().await? {
            return Ok(config.download_path.clone());
        }

        if let Some(download_path) = self.deprecated.spec_path.as_ref() {
            // the previous behavior always appended `specs` so we need to preserve that
            return Ok(download_path.join("specs"));
        }

        Ok("specs".into())
    }

    pub async fn config(&self) -> Result<Option<Arc<config::Config>>> {
        let (path, root) = if let Some(path) = self.config_path.as_ref() {
            let root = duvet_core::env::current_dir()?;
            (path.clone(), root)
        } else if let Some((path, root)) = config::default_path_and_root().await {
            (path, root)
        } else {
            return Ok(None);
        };

        let config = config::load(path, root).await?;
        Ok(Some(config))
    }

    pub async fn sources(&self) -> Result<HashSet<SourceFile>> {
        let mut sources = HashSet::new();

        for pattern in &self.deprecated.source_patterns {
            self.source_file(pattern, &mut sources)?;
        }

        for pattern in &self.deprecated.spec_patterns {
            self.toml_file(pattern, &mut sources)?;
        }

        if let Some(config) = self.config().await? {
            for source in &config.sources {
                // TODO switch from `glob` to `duvet_core::glob`
                let _ = &source.root;
                for entry in glob(&source.pattern).into_diagnostic()? {
                    sources.insert(SourceFile::Text {
                        pattern: source.comment_style.clone(),
                        default_type: source.default_type,
                        path: entry.into_diagnostic()?.into(),
                    });
                }
            }

            for requirement in &config.requirements {
                // TODO switch from `glob` to `duvet_core::glob`
                let _ = &requirement.root;
                for entry in glob(&requirement.pattern).into_diagnostic()? {
                    sources.insert(SourceFile::Toml(entry.into_diagnostic()?.into()));
                }
            }
        }

        Ok(sources)
    }

    fn source_file(&self, pattern: &str, files: &mut HashSet<SourceFile>) -> Result {
        let (compliance_pattern, file_pattern) = if let Some(pattern) = pattern.strip_prefix('(') {
            let mut parts = pattern.splitn(2, ')');
            let pattern = parts.next().expect("invalid pattern");
            let file_pattern = parts.next().expect("invalid pattern");

            let pattern = comment::Pattern::from_arg(pattern)?;

            (pattern, file_pattern)
        } else {
            (comment::Pattern::default(), pattern)
        };

        for entry in glob(file_pattern).into_diagnostic()? {
            files.insert(SourceFile::Text {
                pattern: compliance_pattern.clone(),
                default_type: Default::default(),
                path: entry.into_diagnostic()?.into(),
            });
        }

        Ok(())
    }

    fn toml_file(&self, pattern: &str, files: &mut HashSet<SourceFile>) -> Result {
        for entry in glob(pattern).into_diagnostic()? {
            files.insert(SourceFile::Toml(entry.into_diagnostic()?.into()));
        }

        Ok(())
    }
}

// Set of options that are preserved for backwards compatibility but either
// don't do anything or are undocumented
#[derive(Debug, PartialEq, PartialOrd, Eq, Ord, Hash, Parser, Default)]
pub struct Deprecated {
    #[clap(long, short = 'p', hide = true)]
    package: Option<String>,

    #[clap(long, hide = true)]
    features: Vec<String>,

    #[clap(long, hide = true)]
    workspace: bool,

    #[clap(long = "exclude", hide = true)]
    excludes: Vec<String>,

    #[clap(long = "all-features", hide = true)]
    all_features: bool,

    #[clap(long = "no-default-features", hide = true)]
    no_default_features: bool,

    #[clap(long = "no-cargo", hide = true)]
    no_cargo: bool,

    #[clap(long, hide = true)]
    target: Option<String>,

    #[clap(long = "target-dir", default_value = "target/compliance", hide = true)]
    target_dir: String,

    #[clap(long = "manifest-path", hide = true)]
    manifest_path: Option<String>,

    #[clap(long = "source-pattern", hide = true)]
    source_patterns: Vec<String>,

    #[clap(long = "spec-pattern", hide = true)]
    spec_patterns: Vec<String>,

    #[clap(long = "spec-path", hide = true)]
    spec_path: Option<Path>,

    #[clap(long = "require-tests", hide = true)]
    require_tests: Option<String>,

    #[clap(long = "require-citations", hide = true)]
    require_citations: Option<String>,
}
