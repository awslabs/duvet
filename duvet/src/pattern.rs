// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    annotation::{Annotation, AnnotationSet, AnnotationType},
    sourcemap::{LinesIter, Str},
    Error,
};
use anyhow::anyhow;
use duvet_core::{path::Path, Result};
use std::sync::Arc;

#[cfg(test)]
mod tests;

enum ParserState<'a> {
    Search,
    CapturingMeta(Capture<'a>),
    CapturingContent(Capture<'a>),
}

impl<'a> ParserState<'a> {
    fn on_line(
        &mut self,
        path: &Path,
        default_type: AnnotationType,
        annotations: &mut AnnotationSet,
        pattern: &Pattern,
        line: &'a str,
        line_no: usize,
    ) -> Result<(), Error> {
        let content = line.trim_start();

        match core::mem::replace(self, ParserState::Search) {
            ParserState::Search => {
                let content = if let Some(content) = pattern.try_meta(content) {
                    content
                } else {
                    return Ok(());
                };

                if content.is_empty() {
                    return Ok(());
                }

                let indent = line.len() - content.len();
                let mut capture = Capture::new(line_no, indent, default_type);
                capture.push_meta(content)?;

                *self = ParserState::CapturingMeta(capture);
            }
            ParserState::CapturingMeta(mut capture) => {
                if let Some(meta) = pattern.try_meta(content) {
                    capture.push_meta(meta)?;
                    *self = ParserState::CapturingMeta(capture);
                } else if let Some(content) = pattern.try_content(content) {
                    capture.push_content(content);
                    *self = ParserState::CapturingContent(capture);
                } else {
                    annotations.insert(capture.done(line_no, path)?);
                }
            }
            ParserState::CapturingContent(mut capture) => {
                if pattern.try_meta(content).is_some() {
                    return Err(anyhow!("cannot set metadata while parsing content"));
                } else if let Some(content) = pattern.try_content(content) {
                    capture.push_content(content);
                    *self = ParserState::CapturingContent(capture);
                } else {
                    annotations.insert(capture.done(line_no, path)?);
                }
            }
        }

        Ok(())
    }
}

#[derive(Debug)]
struct Capture<'a> {
    contents: String,
    annotation: ParsedAnnotation<'a>,
}

impl<'a> Capture<'a> {
    fn new(line: usize, column: usize, default_type: AnnotationType) -> Self {
        Self {
            contents: String::new(),
            annotation: ParsedAnnotation {
                anno_line: line as _,
                anno_column: column as _,
                item_line: line as _,
                item_column: column as _,
                anno: default_type,
                ..Default::default()
            },
        }
    }

    fn push_meta(&mut self, value: &'a str) -> Result<(), Error> {
        let mut parts = value.trim_start().splitn(2, '=');

        let key = parts.next().unwrap();
        let value = parts.next();

        match (key, value) {
            ("source", Some(value)) => self.annotation.target = value,
            ("level", Some(value)) => self.annotation.level = value.parse()?,
            ("format", Some(value)) => self.annotation.format = value.parse()?,
            ("type", Some(value)) => self.annotation.anno = value.parse()?,
            ("reason", Some(value)) if self.annotation.anno == AnnotationType::Exception => {
                self.annotation.comment = value
            }
            ("feature", Some(value)) if self.annotation.anno == AnnotationType::Todo => {
                self.annotation.feature = value
            }
            ("tracking-issue", Some(value)) if self.annotation.anno == AnnotationType::Todo => {
                self.annotation.tracking_issue = value
            }
            (key, Some(_)) => return Err(anyhow!(format!("invalid metadata field {}", key))),
            (value, None) if self.annotation.target.is_empty() => self.annotation.target = value,
            (_, None) => return Err(anyhow!("annotation source already specified")),
        }

        Ok(())
    }

    fn push_content(&mut self, value: &'a str) {
        let value = value.trim();
        if !value.is_empty() {
            self.contents.push_str(value);
            self.contents.push(' ');
        }
    }

    fn done(self, item_line: usize, path: &Path) -> Result<Annotation, Error> {
        let mut annotation = Annotation {
            item_line: item_line as _,
            item_column: 0,
            source: path.clone(),
            quote: self.contents,
            manifest_dir: duvet_core::env::current_dir()?,
            ..self.annotation.into()
        };

        while annotation.quote.ends_with(' ') {
            annotation.quote.pop();
        }

        if annotation.target.is_empty() {
            return Err(anyhow!("missing source information"));
        }

        Ok(annotation)
    }
}
