// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    annotation::{Annotation, AnnotationSet, AnnotationType},
    parser::ParsedAnnotation,
    sourcemap::{LinesIter, Str},
    Error,
};
use anyhow::anyhow;
use std::path::Path;

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub struct Pattern<'a> {
    meta: &'a str,
    content: &'a str,
}

impl<'a> Default for Pattern<'a> {
    fn default() -> Self {
        Self {
            meta: "//=",
            content: "//#",
        }
    }
}

impl<'a> Pattern<'a> {
    pub fn from_arg(arg: &'a str) -> Result<Self, Error> {
        let mut parts = arg.split(',').filter(|p| !p.is_empty());
        let meta = parts.next().expect("should have at least one pattern");
        if meta.is_empty() {
            return Err(anyhow!("compliance pattern cannot be empty"));
        }

        let content = parts.next().unwrap();

        Ok(Self { meta, content })
    }

    pub fn extract(
        &self,
        source: &str,
        path: &Path,
        annotations: &mut AnnotationSet,
    ) -> Result<(), Error> {
        let mut state = ParserState::Search;

        let mut last_line = 0;
        for Str { value, line, .. } in LinesIter::new(source) {
            state.on_line(path, annotations, self, value, line)?;
            last_line = line;
        }

        // make sure we finish off the state machine
        state.on_line(path, annotations, self, "", last_line)?;

        Ok(())
    }

    fn try_meta<'b>(&self, line: &'b str) -> Option<&'b str> {
        line.strip_prefix(self.meta)
    }

    fn try_content<'b>(&self, line: &'b str) -> Option<&'b str> {
        line.strip_prefix(self.content)
    }
}

enum ParserState<'a> {
    Search,
    CapturingMeta(Capture<'a>),
    CapturingContent(Capture<'a>),
}

impl<'a> ParserState<'a> {
    fn on_line(
        &mut self,
        path: &Path,
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
                let mut capture = Capture::new(line_no, indent);
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
                    return Err(anyhow!(
                        "cannot set metadata while parsing content".to_owned() + content
                    ));
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
    fn new(line: usize, column: usize) -> Self {
        Self {
            contents: String::new(),
            annotation: ParsedAnnotation {
                anno_line: line as _,
                anno_column: column as _,
                item_line: line as _,
                item_column: column as _,
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
            source: path.into(),
            quote: self.contents,
            manifest_dir: std::env::current_dir()?,
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
