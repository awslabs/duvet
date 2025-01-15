// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::{annotation::AnnotationSet, comment, Error};
use duvet_core::path::Path;

pub mod toml;

#[derive(Clone, Debug, PartialEq, PartialOrd, Ord, Eq, Hash)]
pub enum SourceFile {
    Text(comment::Pattern, Path),
    Toml(Path),
}

impl SourceFile {
    pub async fn annotations(&self) -> (AnnotationSet, Vec<Error>) {
        match self {
            Self::Text(pattern, file) => match duvet_core::vfs::read_string(file).await {
                Ok(text) => comment::extract(&text, pattern, Default::default()),
                Err(err) => (Default::default(), vec![err]),
            },
            Self::Toml(file) => toml::load(file).await,
        }
    }
}
