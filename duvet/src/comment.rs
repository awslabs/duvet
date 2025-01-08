// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    annotation::{AnnotationSet, AnnotationType},
    Error, Result,
};
use duvet_core::{error, file::SourceFile};
use std::sync::Arc;

pub mod parser;
pub mod tokenizer;

#[cfg(test)]
mod tests;

pub fn extract(
    file: &SourceFile,
    pattern: &Pattern,
    default_type: AnnotationType,
) -> (AnnotationSet, Vec<Error>) {
    let tokens = tokenizer::tokens(file, pattern);
    let mut parser = parser::parse(tokens, default_type);

    let annotations = (&mut parser).collect();
    let errors = parser.errors();

    (annotations, errors)
}

#[derive(Clone, Debug, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub struct Pattern {
    pub meta: Arc<str>,
    pub content: Arc<str>,
}

impl Default for Pattern {
    fn default() -> Self {
        Self {
            meta: "//=".into(),
            content: "//#".into(),
        }
    }
}

impl Pattern {
    pub fn from_arg(arg: &str) -> Result<Self> {
        let mut parts = arg.split(',').filter(|p| !p.is_empty());
        let meta = parts.next().expect("should have at least one pattern");
        if meta.is_empty() {
            return Err(error!("compliance pattern cannot be empty"));
        }

        let content = parts.next().unwrap();

        let meta = meta.into();
        let content = content.into();

        Ok(Self { meta, content })
    }
}
