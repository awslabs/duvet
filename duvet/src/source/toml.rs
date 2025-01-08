// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    annotation::{Annotation, AnnotationLevel, AnnotationSet, AnnotationType},
    specification::Format,
    Error, Result,
};
use duvet_core::{
    error,
    file::{self, Slice},
    path::Path,
};
use serde::Deserialize;
use serde_spanned::Spanned;
use std::{collections::BTreeSet, sync::Arc};

pub async fn load(path: &Path) -> (AnnotationSet, Vec<Error>) {
    match Specs::from_path(path).await {
        Ok((specs, file)) => {
            let mut annotations = BTreeSet::default();
            let mut errors = vec![];

            let default_target = &specs.target;

            macro_rules! load {
                ($field:ident) => {
                    for anno in specs.$field.iter() {
                        let original_text = file.substr_range(anno.span()).unwrap();
                        match anno.clone().into_inner().into_annotation(
                            file.clone(),
                            original_text.clone(),
                            default_target,
                        ) {
                            Ok(anno) => {
                                annotations.insert(anno);
                            }
                            Err(err) => {
                                errors.push(err);
                            }
                        }
                    }
                };
            }

            load!(specs);
            load!(exceptions);
            load!(todos);

            (annotations, errors)
        }
        Err(err) => (Default::default(), vec![err]),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Specs {
    target: Option<Spanned<String>>,

    #[serde(alias = "spec", default)]
    specs: Vec<Spanned<Spec>>,

    #[serde(alias = "exception", default)]
    exceptions: Vec<Spanned<Exception>>,

    #[serde(alias = "TODO", alias = "todo", default)]
    todos: Vec<Spanned<Todo>>,
}

impl Specs {
    async fn from_path(path: &Path) -> Result<(Arc<Self>, file::SourceFile)> {
        let file = duvet_core::vfs::read_string(path).await?;
        let specs = file.as_toml().await?;
        Ok((specs, file))
    }
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Spec {
    target: Option<Spanned<String>>,
    level: Option<Spanned<String>>,
    format: Option<Spanned<String>>,
    quote: Spanned<String>,
}

impl Spec {
    fn into_annotation(
        self,
        source: file::SourceFile,
        original_text: Slice,
        default_target: &Option<Spanned<String>>,
    ) -> Result<Annotation> {
        let original_quote = source.substr_range(self.quote.span()).unwrap();

        let target = self
            .target
            .or_else(|| default_target.as_ref().cloned())
            .ok_or_else(|| error!("missing target"))?;
        let original_target = source.substr_range(target.span()).unwrap();
        let target = target.into_inner();

        let level = if let Some(value) = self.level {
            value.as_ref().parse().map_err(|err| {
                source
                    .substr_range(value.span())
                    .unwrap()
                    .error(err, "defined here")
            })?
        } else {
            AnnotationLevel::Auto
        };

        let format = if let Some(value) = self.format {
            value.as_ref().parse().map_err(|err| {
                source
                    .substr_range(value.span())
                    .unwrap()
                    .error(err, "defined here")
            })?
        } else {
            Format::Auto
        };

        let anno_line = original_text.line_range().start;

        Ok(Annotation {
            anno_line,
            anno: AnnotationType::Spec,
            original_text,
            original_quote,
            original_target,
            target,
            quote: normalize_quote(self.quote.as_ref()),
            comment: self.quote.into_inner(),
            manifest_dir: source.path().clone(),
            feature: Default::default(),
            tags: Default::default(),
            tracking_issue: Default::default(),
            source: source.path().clone(),
            level,
            format,
        })
    }
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Exception {
    target: Option<Spanned<String>>,
    quote: Spanned<String>,
    reason: Spanned<String>,
}

impl Exception {
    fn into_annotation(
        self,
        source: file::SourceFile,
        original_text: Slice,
        default_target: &Option<Spanned<String>>,
    ) -> Result<Annotation> {
        let original_quote = source.substr_range(self.quote.span()).unwrap();

        let target = self
            .target
            .or_else(|| default_target.as_ref().cloned())
            .ok_or_else(|| error!("missing target"))?;
        let original_target = source.substr_range(target.span()).unwrap();
        let target = target.into_inner();

        let anno_line = original_text.line_range().start;

        Ok(Annotation {
            anno_line,
            anno: AnnotationType::Exception,
            original_text,
            original_quote,
            original_target,
            target,
            quote: normalize_quote(self.quote.as_ref()),
            comment: self.reason.into_inner(),
            manifest_dir: source.path().clone(),
            feature: Default::default(),
            tags: Default::default(),
            tracking_issue: Default::default(),
            source: source.path().clone(),
            level: AnnotationLevel::Auto,
            format: Format::Auto,
        })
    }
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Todo {
    target: Option<Spanned<String>>,
    quote: Spanned<String>,
    feature: Option<Spanned<String>>,
    #[serde(alias = "tracking-issue")]
    tracking_issue: Option<Spanned<String>>,
    reason: Option<Spanned<String>>,
    #[serde(default)]
    tags: BTreeSet<String>,
}

impl Todo {
    fn into_annotation(
        self,
        source: file::SourceFile,
        original_text: Slice,
        default_target: &Option<Spanned<String>>,
    ) -> Result<Annotation> {
        let original_quote = source.substr_range(self.quote.span()).unwrap();

        let target = self
            .target
            .or_else(|| default_target.as_ref().cloned())
            .ok_or_else(|| error!("missing target"))?;
        let original_target = source.substr_range(target.span()).unwrap();
        let target = target.into_inner();

        let anno_line = original_text.line_range().start;

        Ok(Annotation {
            anno_line,
            anno: AnnotationType::Todo,
            original_text,
            original_quote,
            original_target,
            target,
            quote: normalize_quote(self.quote.as_ref()),
            comment: self.reason.map(|v| v.into_inner()).unwrap_or_default(),
            manifest_dir: source.path().clone(),
            source: source.path().clone(),
            tags: self.tags,
            feature: self.feature.map(|v| v.into_inner()).unwrap_or_default(),
            tracking_issue: self
                .tracking_issue
                .map(|v| v.into_inner())
                .unwrap_or_default(),
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
