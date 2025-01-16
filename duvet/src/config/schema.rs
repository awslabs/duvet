// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use std::{str::FromStr, sync::Arc};

use crate::{config, Result};
use duvet_core::{error, path::Path};
use serde::{de, Deserialize};

pub mod v0_4_0;

pub static DEFAULT: &str = "https://awslabs.github.io/duvet/config/v0.4.0.json";

#[derive(Debug, Deserialize)]
#[serde(tag = "$schema", deny_unknown_fields)]
pub enum Schema {
    #[serde(
        rename = "https://awslabs.github.io/duvet/config/v0.4.0.json",
        alias = "https://awslabs.github.io/duvet/config/v0.4.0.json#",
        alias = "https://awslabs.github.io/duvet/config/v0.4.json",
        alias = "https://awslabs.github.io/duvet/config/v0.4.json#"
    )]
    V1_0_0(v0_4_0::Schema),
}

impl Schema {
    pub fn load_sources(&self, sources: &mut Vec<config::Source>, root: &Path) -> Result {
        match self {
            Schema::V1_0_0(schema) => schema.load_sources(sources, root),
        }
    }

    pub fn load_requirements(
        &self,
        requirements: &mut Vec<config::Requirement>,
        root: &Path,
    ) -> Result {
        match self {
            Schema::V1_0_0(schema) => schema.load_requirements(requirements, root),
        }
    }

    pub fn load_specifications(
        &self,
        specifications: &mut Vec<config::Specification>,
        root: &Path,
    ) -> Result {
        match self {
            Schema::V1_0_0(schema) => schema.load_specifications(specifications, root),
        }
    }

    pub fn download_path(&self, config: &Path, root: &Path) -> Path {
        match self {
            Schema::V1_0_0(schema) => schema.download_path(config, root),
        }
    }

    pub fn requirements_path(&self, config: &Path, root: &Path) -> Path {
        match self {
            Schema::V1_0_0(schema) => schema.requirements_path(config, root),
        }
    }

    pub fn report(&self, config: &Path, root: &Path) -> config::Report {
        match self {
            Schema::V1_0_0(schema) => schema.report(config, root),
        }
    }
}

#[derive(Clone, Debug)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct TemplatedString(Arc<str>);

impl FromStr for TemplatedString {
    type Err = crate::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let mut out = String::new();

        for (idx, part) in s.split("${{").enumerate() {
            if idx == 0 {
                out.push_str(part);
                continue;
            }

            let close = "}}";
            let (expr, rest) = part
                .split_once(close)
                .ok_or_else(|| error!("expected {close:?}"))?;

            let mut value = None;

            for choice in expr.split("||") {
                let choice = choice.trim();
                if let Some(choice) = choice.strip_prefix('\'') {
                    let choice = choice.trim_end_matches('\'');
                    value = Some(choice.to_string());
                    break;
                } else if let Ok(v) = std::env::var(choice) {
                    value = Some(v);
                    break;
                }
            }

            let Some(value) = value else {
                return Err(error!("failed to evaluate expression: {expr:?}"));
            };

            out.push_str(&value);
            out.push_str(rest);
        }

        Ok(Self(out.into()))
    }
}

impl From<&TemplatedString> for Arc<str> {
    fn from(value: &TemplatedString) -> Self {
        value.0.clone()
    }
}

impl<'de> serde::de::Deserialize<'de> for TemplatedString {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use core::fmt;

        struct Visitor;

        impl de::Visitor<'_> for Visitor {
            type Value = TemplatedString;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("string")
            }

            fn visit_str<E>(self, tmpl: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                tmpl.parse().map_err(de::Error::custom)
            }
        }

        deserializer.deserialize_str(Visitor)
    }
}
