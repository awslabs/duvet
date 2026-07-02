// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::{comment, config, source::SourceFile, Result};
use clap::Parser;
use duvet_core::{diagnostic::IntoDiagnostic, glob::Glob, path::Path};
use futures::StreamExt;
use std::{collections::HashSet, sync::Arc};

/// A [`Glob`] that never matches, used when a walk needs no ignore filter.
fn empty_glob() -> Glob {
    Glob::try_from_iter(core::iter::empty::<&str>()).expect("empty glob set is always valid")
}

#[derive(Debug, Default, PartialEq, PartialOrd, Eq, Ord, Hash, Parser)]
pub struct Project {
    #[clap(flatten)]
    deprecated: Deprecated,

    #[clap(long)]
    config_path: Option<Path>,
}

impl Project {
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

        let cwd = duvet_core::env::current_dir()?;

        for pattern in &self.deprecated.source_patterns {
            self.source_file(pattern, &cwd, &mut sources).await?;
        }

        for pattern in &self.deprecated.spec_patterns {
            self.toml_file(pattern, &cwd, &mut sources).await?;
        }

        if let Some(config) = self.config().await? {
            for source in &config.sources {
                let paths = glob_files(&source.root, &source.pattern).await?;
                for path in paths {
                    sources.insert(SourceFile::Text {
                        pattern: source.comment_style.clone(),
                        default_type: source.default_type,
                        path,
                        blob_link: source.blob_link.clone(),
                    });
                }
            }

            for requirement in &config.requirements {
                let paths = glob_files(&requirement.root, &requirement.pattern).await?;
                for path in paths {
                    sources.insert(SourceFile::Toml(path));
                }
            }
        }

        Ok(sources)
    }

    async fn source_file(
        &self,
        pattern: &str,
        root: &Path,
        files: &mut HashSet<SourceFile>,
    ) -> Result {
        let (compliance_pattern, file_pattern) = if let Some(pattern) = pattern.strip_prefix('(') {
            let mut parts = pattern.splitn(2, ')');
            let pattern = parts.next().expect("invalid pattern");
            let file_pattern = parts.next().expect("invalid pattern");

            let pattern = comment::Pattern::from_arg(pattern)?;

            (pattern, file_pattern)
        } else {
            (comment::Pattern::default(), pattern)
        };

        for path in glob_files(root, file_pattern).await? {
            files.insert(SourceFile::Text {
                pattern: compliance_pattern.clone(),
                default_type: Default::default(),
                path,
                blob_link: None,
            });
        }

        Ok(())
    }

    async fn toml_file(&self, pattern: &str, root: &Path, files: &mut HashSet<SourceFile>) -> Result {
        for path in glob_files(root, pattern).await? {
            files.insert(SourceFile::Toml(path));
        }

        Ok(())
    }
}

/// Walks `root` through the [`duvet_core::vfs`] seam, returning every file whose
/// path matches `pattern`.
///
/// `pattern` is resolved relative to `root` and matched with the glob
/// *anchored* at `root` — mirroring the historical `glob` crate behavior, where
/// patterns were resolved against (and anchored at) the current directory.
/// This is important because [`duvet_core::glob::Glob`] otherwise implicitly
/// prepends `**/` to relative patterns, which would match files anywhere in the
/// tree rather than at the configured location.
///
/// Returned paths are relative to `root`. Downstream consumers embed
/// `SourceFile` paths verbatim (e.g. the `source` field of report
/// annotations), so keeping them relative preserves report output.
async fn glob_files(root: &Path, pattern: &str) -> Result<Vec<Path>> {
    // Anchor the pattern at the absolute root so it matches the walked
    // (absolute) paths exactly. A leading path separator keeps `Glob` from
    // prepending `**/`.
    let anchored = root.join(pattern);
    let anchored = anchored
        .to_str()
        .ok_or_else(|| duvet_core::error!("glob pattern is not valid UTF-8: {pattern:?}"))?;
    let include: Glob = anchored.parse().into_diagnostic()?;

    let stream = duvet_core::dir::walk::glob(root.clone(), include, empty_glob());
    futures::pin_mut!(stream);
    let mut paths = vec![];
    while let Some(path) = stream.next().await {
        let relative = path
            .strip_prefix(root)
            .map(Path::from)
            .unwrap_or_else(|_| path.clone());
        paths.push(relative);
    }
    Ok(paths)
}

// Set of options that are preserved for backwards compatibility but either
// don't do anything or are undocumented
#[derive(Debug, Default, PartialEq, PartialOrd, Eq, Ord, Hash, Parser)]
struct Deprecated {
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
}
