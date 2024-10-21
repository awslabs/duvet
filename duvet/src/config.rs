// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    manifest::{Requirement, Source},
    Result,
};
use duvet_core::{diagnostic::IntoDiagnostic, file::SourceFile, path::Path, vfs};
use serde::Deserialize;
use std::sync::Arc;

pub mod v1;

#[derive(Debug, Deserialize)]
#[serde(tag = "version", deny_unknown_fields)]
pub enum Schema {
    #[serde(rename = "1.0", alias = "1")]
    V1(v1::Schema),
}

#[derive(Clone, Debug)]
pub struct Config {
    schema: Arc<Schema>,
    file: SourceFile,
}

impl Config {
    pub fn load_sources(&self, sources: &mut Vec<Source>) {
        match &*self.schema {
            Schema::V1(v1) => v1.load_sources(sources, self.file.path()),
        }
    }

    pub fn load_requirements(&self, requirements: &mut Vec<Requirement>) {
        match &*self.schema {
            Schema::V1(v1) => v1.load_requirements(requirements, self.file.path()),
        }
    }
}

pub async fn load() -> Option<Result<Config>> {
    let args = crate::arguments::get().await;
    let path = args.config.clone().or_else(default_path)?;
    Some(load_from_path(path).await)
}

async fn load_from_path(path: Path) -> Result<Config> {
    let path = path.canonicalize().into_diagnostic()?;

    let file = vfs::read_string(path).await?;

    let schema = file.as_toml().await?;

    Ok(Config { schema, file })
}

fn default_path() -> Option<Path> {
    let dir = duvet_core::env::current_dir().ok()?;
    let path = dir.join("duvet.toml").canonicalize().ok()?;
    Some(path.into())
}
