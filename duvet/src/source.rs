// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    annotation::{Annotation, AnnotationLevel, AnnotationSet, AnnotationType},
    comment,
    specification::Format,
};
use anyhow::anyhow;
use duvet_core::{diagnostic, path::Path, vfs, Result};
use serde::Deserialize;
use std::{collections::BTreeSet, sync::Arc};

#[derive(Clone, Debug, PartialEq, PartialOrd, Ord, Eq, Hash)]
pub enum SourceFile {
    Text(comment::Pattern, Path, AnnotationType),
    Spec(Path),
}

impl SourceFile {
    pub async fn annotations(&self) -> (AnnotationSet, Vec<diagnostic::Error>) {
        match self {
            Self::Text(pattern, file, default_type) => {
                read_source(file.clone(), pattern.clone(), *default_type).await
            }
            Self::Spec(file) => read_requirement(file.clone()).await,
        }
    }
}

async fn read_source(
    file: Path,
    pattern: comment::Pattern,
    default_type: AnnotationType,
) -> (AnnotationSet, Vec<diagnostic::Error>) {
    let source = match vfs::read_string(&file).await {
        Ok(specs) => specs,
        Err(err) => {
            return (Default::default(), vec![err]);
        }
    };

    comment::extract(&source, &pattern, default_type)
}

async fn read_requirement(file: Path) -> (AnnotationSet, Vec<diagnostic::Error>) {
    async fn read_file(file: &Path) -> Result<Arc<Specs>> {
        let text = vfs::read_string(file).await?;
        let specs = text.as_toml().await?;
        Ok(specs)
    }

    let specs = match read_file(&file).await {
        Ok(specs) => specs,
        Err(err) => {
            return (Default::default(), vec![err]);
        }
    };

    let mut annotations = AnnotationSet::default();
    let mut errors = vec![];

    let annos = None
        .into_iter()
        .chain(
            specs
                .specs
                .iter()
                .map(|anno| anno.as_annotation(file.clone(), &specs.target)),
        )
        .chain(
            specs
                .exceptions
                .iter()
                .map(|anno| anno.as_annotation(file.clone(), &specs.target)),
        )
        .chain(
            specs
                .todos
                .iter()
                .map(|anno| anno.as_annotation(file.clone(), &specs.target)),
        );

    for anno in annos {
        match anno {
            Ok(anno) => {
                annotations.insert(anno);
            }
            Err(err) => {
                errors.push(err);
            }
        }
    }

    (annotations, errors)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Specs {
    target: Option<String>,

    #[serde(alias = "spec", default)]
    specs: Vec<Spec>,

    #[serde(alias = "exception", default)]
    exceptions: Vec<Exception>,

    #[serde(alias = "TODO", alias = "todo", default)]
    todos: Vec<Todo>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Spec {
    target: Option<String>,
    level: Option<AnnotationLevel>,
    format: Option<Format>,
    quote: String,
}

impl Spec {
    fn as_annotation(&self, source: Path, default_target: &Option<String>) -> Result<Annotation> {
        Ok(Annotation {
            anno_line: 0,
            anno_column: 0,
            anno: AnnotationType::Spec,
            target: self
                .target
                .clone()
                .or_else(|| default_target.as_ref().cloned())
                .ok_or_else(|| anyhow!("missing target"))?,
            quote: normalize_quote(&self.quote),
            comment: self.quote.to_string(),
            feature: Default::default(),
            tags: Default::default(),
            tracking_issue: Default::default(),
            source,
            level: self.level.unwrap_or(AnnotationLevel::Auto),
            format: self.format.unwrap_or(Format::Auto),
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Exception {
    target: Option<String>,
    quote: String,
    reason: String,
}

impl Exception {
    fn as_annotation(&self, source: Path, default_target: &Option<String>) -> Result<Annotation> {
        Ok(Annotation {
            anno_line: 0,
            anno_column: 0,
            anno: AnnotationType::Exception,
            target: self
                .target
                .clone()
                .or_else(|| default_target.as_ref().cloned())
                .ok_or_else(|| anyhow!("missing target"))?,
            quote: normalize_quote(&self.quote),
            comment: self.reason.clone(),
            feature: Default::default(),
            tags: Default::default(),
            tracking_issue: Default::default(),
            source,
            level: AnnotationLevel::Auto,
            format: Format::Auto,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Todo {
    target: Option<String>,
    quote: String,
    feature: Option<String>,
    #[serde(alias = "tracking-issue")]
    tracking_issue: Option<String>,
    reason: Option<String>,
    #[serde(default)]
    tags: BTreeSet<String>,
}

impl Todo {
    fn as_annotation(&self, source: Path, default_target: &Option<String>) -> Result<Annotation> {
        Ok(Annotation {
            anno_line: 0,
            anno_column: 0,
            anno: AnnotationType::Todo,
            target: self
                .target
                .clone()
                .or_else(|| default_target.as_ref().cloned())
                .ok_or_else(|| anyhow!("missing target"))?,
            quote: normalize_quote(&self.quote),
            comment: self.reason.clone().unwrap_or_default(),
            source,
            tags: self.tags.clone(),
            feature: self.feature.clone().unwrap_or_default(),
            tracking_issue: self.tracking_issue.clone().unwrap_or_default(),
            level: AnnotationLevel::Auto,
            format: Format::Auto,
        })
    }
}

fn normalize_quote(s: &str) -> String {
    s.lines().fold(String::new(), |mut s, l| {
        let l = l.trim();
        if !l.is_empty() && !s.is_empty() {
            s.push(' ');
        }
        s.push_str(l);
        s
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quote_normalizing() {
        let sample = r"
        A
        B
        C
        ";
        assert_eq!(normalize_quote(sample), "A B C",);
    }

    #[test]
    fn test_quote_normalizing_with_empty_lines() {
        let sample = r"
            A:

            * B

            * C
              D
        ";
        assert_eq!(normalize_quote(sample), "A: * B * C D",);
    }
}
