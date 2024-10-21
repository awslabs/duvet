// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::{pattern::Pattern, source::SourceFile, Error};
use clap::Parser;
use glob::glob;
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
    pub spec_path: Option<String>,
}

impl Project {
    pub fn sources(&self) -> Result<HashSet<SourceFile>, Error> {
        let mut sources = HashSet::new();

        for pattern in &self.source_patterns {
            self.source_file(pattern, &mut sources)?;
        }

        for pattern in &self.spec_patterns {
            self.spec_file(pattern, &mut sources)?;
        }

        Ok(sources)
    }

    fn source_file<'a>(
        &self,
        pattern: &'a str,
        files: &mut HashSet<SourceFile<'a>>,
    ) -> Result<(), Error> {
        let (compliance_pattern, file_pattern) = if let Some(pattern) = pattern.strip_prefix('(') {
            let mut parts = pattern.splitn(2, ')');
            let pattern = parts.next().expect("invalid pattern");
            let file_pattern = parts.next().expect("invalid pattern");

            let pattern = Pattern::from_arg(pattern)?;

            (pattern, file_pattern)
        } else {
            (Pattern::default(), pattern)
        };

        for entry in glob(file_pattern)? {
            files.insert(SourceFile::Text(compliance_pattern, entry?));
        }

        Ok(())
    }

    fn spec_file<'a>(
        &self,
        pattern: &'a str,
        files: &mut HashSet<SourceFile<'a>>,
    ) -> Result<(), Error> {
        for entry in glob(pattern)? {
            files.insert(SourceFile::Spec(entry?));
        }

        Ok(())
    }
}