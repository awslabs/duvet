// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    annotation::{AnnotationSet, AnnotationType},
    comment, Error,
};
use duvet_core::path::Path;
use std::sync::Arc;

pub mod toml;

#[derive(Clone, Debug, PartialEq, PartialOrd, Ord, Eq, Hash)]
pub enum SourceFile {
    Text {
        pattern: comment::Pattern,
        default_type: AnnotationType,
        path: Path,
        blob_link: Option<Arc<str>>,
        /// Per-source regex for statically unwitnessable target lines
        /// (config `non-target-pattern`); `None` defers to the
        /// per-language default.
        non_target_pattern: Option<Arc<str>>,
    },
    Toml(Path),
}

impl SourceFile {
    pub async fn annotations(&self) -> (AnnotationSet, Vec<Error>) {
        match self {
            Self::Text {
                pattern,
                default_type,
                path,
                blob_link,
                non_target_pattern: _,
            } => match duvet_core::vfs::read_string(path).await {
                Ok(text) => comment::extract(&text, pattern, *default_type, blob_link.clone()),
                Err(err) => (Default::default(), vec![err]),
            },
            Self::Toml(file) => toml::load(file).await,
        }
    }
}
