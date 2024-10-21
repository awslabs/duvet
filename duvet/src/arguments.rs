// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    extract,
    manifest::{Requirement, Source},
    report,
};
use clap::Parser;
use duvet_core::{env, path::Path, query, Result};
use std::sync::Arc;

#[derive(Debug, Parser)]
pub struct Arguments {
    #[clap(short, long, global = true)]
    pub config: Option<Path>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Parser)]
#[allow(clippy::large_enum_variant)]
pub enum Command {
    Extract(extract::Extract),
    Report(report::Report),
}

impl Arguments {
    pub async fn exec(&self) -> Result<()> {
        match &self.command {
            Command::Extract(args) => args.exec().await,
            Command::Report(args) => args.exec().await,
        }
    }

    pub fn load_sources(&self, sources: &mut Vec<Source>) {
        match &self.command {
            Command::Extract(_) => (),
            Command::Report(args) => args.load_sources(sources),
        }
    }

    pub fn load_requirements(&self, requirements: &mut Vec<Requirement>) {
        match &self.command {
            Command::Extract(_) => (),
            Command::Report(args) => args.load_requirements(requirements),
        }
    }
}

#[query]
pub async fn get() -> Arc<Arguments> {
    let args = env::args();
    Arc::new(Arguments::parse_from(args.iter()))
}
