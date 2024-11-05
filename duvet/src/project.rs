// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::{comment, source::SourceFile, Error};
use clap::Parser;
use duvet_core::path::Path;
use std::collections::HashSet;

#[derive(Debug, PartialEq, PartialOrd, Eq, Ord, Hash, Parser)]
pub struct Project {
    /// Package to run tests for
    #[clap(long, short = 'p')]
    package: Option<String>,

    /// Space or comma separated list of features to activate
    #[clap(long)]
    features: Vec<String>,

    /// Build all packages in the workspace
    #[clap(long)]
    workspace: bool,

    /// Exclude packages from the test
    #[clap(long = "exclude")]
    excludes: Vec<String>,

    /// Activate all available features
    #[clap(long = "all-features")]
    all_features: bool,

    /// Do not activate the `default` feature
    #[clap(long = "no-default-features")]
    no_default_features: bool,

    /// Disables running cargo commands
    #[clap(long = "no-cargo")]
    no_cargo: bool,

    /// TRIPLE
    #[clap(long)]
    target: Option<String>,

    /// Directory for all generated artifacts
    #[clap(long = "target-dir", default_value = "target/compliance")]
    target_dir: String,

    /// Path to Cargo.toml
    #[clap(long = "manifest-path")]
    manifest_path: Option<String>,

    /// Glob patterns for additional source files
    #[clap(long = "source-pattern")]
    source_patterns: Vec<String>,

    /// Glob patterns for spec files
    #[clap(long = "spec-pattern")]
    spec_patterns: Vec<String>,

    /// Path to store the collection of spec files
    ///
    /// The collection of spec files are stored in a folder called `specs`. The
    /// `specs` folder is stored in the current directory by default. Use this
    /// argument to override the default location.
    #[clap(long = "spec-path")]
    pub spec_path: Option<Path>,
}

// TODO
/*
impl Project {
    pub async fn sources(&self) -> Result<HashSet<SourceFile>, Error> {
        use core::pin::Pin;
        use duvet_core::{dir::walk, env, glob::Glob, path::Path};
        use futures::{Stream, StreamExt};

        let mut streams = vec![];

        for pattern in &self.source_patterns {
            streams.push(Self::source_file(pattern)?);
        }

        for pattern in &self.spec_patterns {
            streams.push(Self::spec_file(pattern)?);
        }

        let mut sources = HashSet::new();
        // TODO fix query concurrency
        /*
        let mut entries = futures::stream::select_all(streams);
        while let Some(entry) = entries.next().await {
            sources.insert(entry);
        }
        */
        for mut stream in streams {
            while let Some(entry) = stream.next().await {
                sources.insert(entry);
            }
        }

        Ok(sources)
    }

    fn source_file(pattern: &str) -> Result<Pin<Box<dyn Stream<Item = SourceFile>>>, Error> {
        let (comment, file_pattern) = if let Some(pattern) = pattern.strip_prefix('(') {
            let mut parts = pattern.splitn(2, ')');
            let pattern = parts.next().expect("invalid pattern");
            let file_pattern = parts.next().expect("invalid pattern");

            let pattern = comment::Pattern::from_arg(pattern)?;

            (pattern, file_pattern)
        } else {
            (comment::Pattern::default(), pattern)
        };

        let glob = Glob::try_from(file_pattern)?;
        // TODO add support for .gitignore to `walk`
        let ignore = Glob::try_from_iter([".git", "node_modules", "target"])?;
        let walk = walk::glob(env::current_dir()?, glob, ignore);
        let walk = walk.map(move |entry| SourceFile::Text(comment.clone(), entry));
        let walk = Box::pin(walk);

        Ok(walk)
    }

    fn spec_file(pattern: &str) -> Result<Pin<Box<dyn Stream<Item = SourceFile>>>, Error> {
        let glob = Glob::try_from(pattern)?;
        // TODO add support for .gitignore to `walk`
        let ignore = Glob::try_from_iter([".git", "node_modules", "target"])?;
        let walk = walk::glob(env::current_dir()?, glob, ignore);
        let walk = walk.map(|entry| SourceFile::Toml(entry));
        let walk = Box::pin(walk);
        Ok(walk)
    }
}
*/

impl Project {
    pub async fn sources(&self) -> Result<HashSet<SourceFile>, Error> {
        let mut sources = HashSet::new();

        for pattern in &self.source_patterns {
            self.source_file(pattern, &mut sources)?;
        }

        for pattern in &self.spec_patterns {
            self.toml_file(pattern, &mut sources)?;
        }

        Ok(sources)
    }

    fn source_file(&self, pattern: &str, files: &mut HashSet<SourceFile>) -> Result<(), Error> {
        let (compliance_pattern, file_pattern) = if let Some(pattern) = pattern.strip_prefix('(') {
            let mut parts = pattern.splitn(2, ')');
            let pattern = parts.next().expect("invalid pattern");
            let file_pattern = parts.next().expect("invalid pattern");

            let pattern = comment::Pattern::from_arg(pattern)?;

            (pattern, file_pattern)
        } else {
            (comment::Pattern::default(), pattern)
        };

        for entry in glob::glob(file_pattern)? {
            files.insert(SourceFile::Text(compliance_pattern.clone(), entry?.into()));
        }

        Ok(())
    }

    fn toml_file(&self, pattern: &str, files: &mut HashSet<SourceFile>) -> Result<(), Error> {
        for entry in glob::glob(pattern)? {
            files.insert(SourceFile::Toml(entry?.into()));
        }

        Ok(())
    }
}
