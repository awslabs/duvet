// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    annotation::{Annotation, AnnotationLevel, AnnotationSet, AnnotationType},
    comment,
    specification::Format,
    Result,
};
use anyhow::anyhow;
use duvet_core::path::Path;
use serde::Deserialize;
use std::{collections::BTreeSet, sync::Arc};

#[derive(Clone, Debug, PartialEq, PartialOrd, Ord, Eq, Hash)]
pub enum SourceFile {
    Text(comment::Pattern, Path),
    Spec(Path),
}

impl SourceFile {
    pub async fn annotations(&self) -> (AnnotationSet, Vec<duvet_core::diagnostic::Error>) {
        match self {
            Self::Text(pattern, file) => match duvet_core::vfs::read_string(file).await {
                Ok(text) => comment::extract(&text, pattern, Default::default()),
                Err(err) => (Default::default(), vec![err]),
            },
            Self::Spec(file) => match Specs::load(file).await {
                Ok(specs) => {
                    let mut annotations = AnnotationSet::default();
                    let mut errors = vec![];

                    let annos =
                        None.into_iter()
                            .chain(specs.specs.iter().map(|anno| {
                                anno.clone().into_annotation(file.clone(), &specs.target)
                            }))
                            .chain(specs.exceptions.iter().map(|anno| {
                                anno.clone().into_annotation(file.clone(), &specs.target)
                            }))
                            .chain(specs.todos.iter().map(|anno| {
                                anno.clone().into_annotation(file.clone(), &specs.target)
                            }));

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
                Err(err) => (Default::default(), vec![err]),
            },
        }
    }
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

impl Specs {
    async fn load(path: &Path) -> Result<Arc<Self>> {
        let file = duvet_core::vfs::read_string(path).await?;
        let specs = file.as_toml().await?;
        Ok(specs)
    }
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Spec {
    target: Option<String>,
    level: Option<String>,
    format: Option<String>,
    quote: String,
}

impl Spec {
    fn into_annotation(self, source: Path, default_target: &Option<String>) -> Result<Annotation> {
        Ok(Annotation {
            anno_line: 0,
            anno_column: 0,
            anno: AnnotationType::Spec,
            target: self
                .target
                .or_else(|| default_target.as_ref().cloned())
                .ok_or_else(|| anyhow!("missing target"))?,
            quote: normalize_quote(&self.quote),
            comment: self.quote.to_string(),
            manifest_dir: source.clone(),
            feature: Default::default(),
            tags: Default::default(),
            tracking_issue: Default::default(),
            source,
            level: if let Some(level) = self.level {
                level.parse()?
            } else {
                AnnotationLevel::Auto
            },
            format: if let Some(format) = self.format {
                format.parse()?
            } else {
                Format::Auto
            },
        })
    }
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Exception {
    target: Option<String>,
    quote: String,
    reason: String,
}

impl Exception {
    fn into_annotation(self, source: Path, default_target: &Option<String>) -> Result<Annotation> {
        Ok(Annotation {
            anno_line: 0,
            anno_column: 0,
            anno: AnnotationType::Exception,
            target: self
                .target
                .or_else(|| default_target.as_ref().cloned())
                .ok_or_else(|| anyhow!("missing target"))?,
            quote: normalize_quote(&self.quote),
            comment: self.reason,
            manifest_dir: source.clone(),
            feature: Default::default(),
            tags: Default::default(),
            tracking_issue: Default::default(),
            source,
            level: AnnotationLevel::Auto,
            format: Format::Auto,
        })
    }
}

#[derive(Clone, Deserialize)]
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
    fn into_annotation(self, source: Path, default_target: &Option<String>) -> Result<Annotation> {
        Ok(Annotation {
            anno_line: 0,
            anno_column: 0,
            anno: AnnotationType::Todo,
            target: self
                .target
                .or_else(|| default_target.as_ref().cloned())
                .ok_or_else(|| anyhow!("missing target"))?,
            quote: normalize_quote(&self.quote),
            comment: self.reason.unwrap_or_default(),
            manifest_dir: source.clone(),
            source,
            tags: self.tags,
            feature: self.feature.unwrap_or_default(),
            tracking_issue: self.tracking_issue.unwrap_or_default(),
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
