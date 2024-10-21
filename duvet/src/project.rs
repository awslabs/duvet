// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    comment::Pattern,
    manifest::{Requirement, Source},
};
use clap::Parser;
use duvet_core::glob::Glob;

#[derive(Debug, PartialEq, PartialOrd, Eq, Ord, Hash, Parser)]
struct Deprecated {
    #[clap(long, short = 'p', hide = true)]
    package: Option<String>,

    #[clap(long, hide = true)]
    features: Vec<String>,

    #[clap(long, hide = true)]
    workspace: bool,

    #[clap(long = "exclude", hide = true)]
    #[doc(hidden)]
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
}

#[derive(Debug, Parser)]
pub struct Project {
    /// Glob patterns for additional source files
    #[clap(long = "source-pattern")]
    source_patterns: Vec<SourcePattern>,

    /// Glob patterns for spec files
    #[clap(long = "spec-pattern")]
    spec_patterns: Vec<Glob>,

    /// Path to store the collection of spec files
    ///
    /// The collection of spec files are stored in a folder called `specs`. The
    /// `specs` folder is stored in the current directory by default. Use this
    /// argument to override the default location.
    #[clap(long = "spec-path")]
    pub spec_path: Option<String>,

    // Includes a list of deprecated options to avoid breakage
    #[clap(flatten)]
    #[doc(hidden)]
    deprecated: Deprecated,
}

impl Project {
    pub fn load_sources(&self, sources: &mut Vec<Source>) {
        for p in &self.source_patterns {
            sources.push(Source {
                pattern: p.glob.clone(),
                comment_style: p.pattern.clone(),
                root: duvet_core::env::current_dir().unwrap(),
                default_type: Default::default(),
            })
        }
    }

    pub fn load_requirements(&self, requirements: &mut Vec<Requirement>) {
        for req in &self.spec_patterns {
            requirements.push(Requirement {
                pattern: req.clone(),
                root: duvet_core::env::current_dir().unwrap(),
            })
        }
    }
}

#[derive(Clone, Debug)]
struct SourcePattern {
    pattern: Pattern,
    glob: Glob,
}

impl core::str::FromStr for SourcePattern {
    type Err = crate::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some(pattern) = s.strip_prefix('(') {
            let mut parts = pattern.splitn(2, ')');
            let pattern = parts.next().expect("invalid pattern");
            let file_pattern = parts.next().expect("invalid pattern");

            let pattern = Pattern::from_arg(pattern)?;
            let glob = file_pattern.parse()?;

            Ok(Self { pattern, glob })
        } else {
            let pattern = Pattern::default();
            let glob = s.parse()?;
            Ok(Self { pattern, glob })
        }
    }
}
