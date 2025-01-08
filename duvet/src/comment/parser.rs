// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use super::tokenizer::Token;
use crate::{
    annotation::{Annotation, AnnotationLevel, AnnotationType},
    specification::Format,
    Error,
};
use duvet_core::{
    ensure, error,
    file::{Slice, SourceFile},
    Result,
};

pub fn parse<T: IntoIterator<Item = Token>>(
    tokens: T,
    default_type: AnnotationType,
) -> Parser<T::IntoIter> {
    Parser {
        prev_line: 0,
        meta: Default::default(),
        contents: Default::default(),
        errors: Default::default(),
        tokens: tokens.into_iter(),
        default_type,
    }
}

#[derive(Debug, Default)]
struct Meta {
    first_meta: Option<Slice>,
    target: Option<Slice>,
    anno: Option<(AnnotationType, Slice)>,
    reason: Option<Slice>,
    line: usize,
    feature: Option<Slice>,
    tracking_issue: Option<Slice>,
    level: Option<(AnnotationLevel, Slice)>,
    format: Option<(Format, Slice)>,
}

impl Meta {
    fn set(&mut self, key: Option<Slice>, value: Slice) -> Result {
        let source_value = value.clone();

        let prev = match key.as_deref() {
            Some("source") => core::mem::replace(&mut self.target, Some(value)),
            Some("level") => {
                let level = value.parse()?;
                core::mem::replace(&mut self.level, Some((level, value))).map(|v| v.1)
            }
            Some("format") => {
                let format = value.parse()?;
                core::mem::replace(&mut self.format, Some((format, value))).map(|v| v.1)
            }
            Some("type") => {
                let ty = value.parse()?;
                core::mem::replace(&mut self.anno, Some((ty, value))).map(|v| v.1)
            }
            Some("reason") => core::mem::replace(&mut self.reason, Some(value)),
            Some("feature") => core::mem::replace(&mut self.feature, Some(value)),
            Some("tracking-issue") => core::mem::replace(&mut self.tracking_issue, Some(value)),
            Some(_) => {
                return Err(key
                    .unwrap()
                    .error(error!("invalid metadata field"), "defined here"));
            }
            None => core::mem::replace(&mut self.target, Some(value)),
        };

        if let Some(prev) = prev {
            let key = key.as_deref().unwrap_or("source");
            let err = error!("{key:?} already specified")
                .with_source_slice(source_value, "redefined here")
                .with_source_slice(prev, "already defined here");
            return Err(err);
        }

        Ok(())
    }

    fn build(self, contents: Vec<Slice>, default_type: AnnotationType) -> Result<Annotation> {
        let first_meta = self.first_meta.unwrap();
        let source = first_meta.file().clone();

        let Some(target) = self.target else {
            return Err(first_meta.error(
                error!("comment is missing source specification"),
                "defined here",
            ));
        };
        let original_target = target.clone();
        let target = target.to_string();

        let anno = self.anno.map_or(default_type, |v| v.0);

        for (allowed, field) in [
            (AnnotationType::Exception, self.reason.as_ref()),
            (AnnotationType::Todo, self.tracking_issue.as_ref()),
            (AnnotationType::Todo, self.feature.as_ref()),
        ] {
            if anno != allowed {
                if let Some(value) = field {
                    return Err(value.error(error!("invalid key for type {anno}"), "defined here"));
                }
            }
        }

        let mut original_text = first_meta.range();
        let mut original_quote = original_text.clone();

        let mut quote = String::new();
        for (idx, part) in contents.iter().enumerate() {
            if idx == 0 {
                original_quote = part.range();
            } else {
                quote.push(' ');
            }

            original_text.start = original_text.start.min(part.range().start);
            original_text.end = original_text.end.max(part.range().end);
            original_quote.end = original_quote.end.max(part.range().end);

            quote.push_str(part.trim());
        }

        let original_text = source.substr_range(original_text).unwrap();
        let original_quote = source.substr_range(original_quote).unwrap();

        let annotation = Annotation {
            source: source.path().clone(),
            anno_line: self.line,
            anno,
            original_text,
            original_target,
            original_quote,
            target,
            quote,
            manifest_dir: duvet_core::env::current_dir()?,
            comment: self.reason.map(|v| v.to_string()).unwrap_or_default(),
            level: self.level.map_or(AnnotationLevel::Auto, |v| v.0),
            format: self.format.map_or(Format::Auto, |v| v.0),
            tracking_issue: self
                .tracking_issue
                .map(|v| v.to_string())
                .unwrap_or_default(),
            feature: self.feature.map(|v| v.to_string()).unwrap_or_default(),
            tags: Default::default(),
        };

        Ok(annotation)
    }
}

pub struct Parser<T> {
    prev_line: usize,
    default_type: AnnotationType,
    meta: Meta,
    contents: Vec<Slice>,
    errors: Vec<Error>,
    tokens: T,
}

impl<T: Iterator<Item = Token>> Parser<T> {
    pub fn errors(self) -> Vec<Error> {
        self.errors
    }

    fn on_token(&mut self, token: Token) -> Option<Annotation> {
        let line_no = token.line_no();
        // if the line number isn't the next expected one then flush
        let prev = self.flush_if(line_no > self.prev_line + 1);
        self.prev_line = line_no;

        match token {
            Token::Meta {
                key,
                value,
                line: _,
            } => self.push_meta(Some(key.clone()), value.clone()),
            Token::UnnamedMeta { value, line: _ } => self.push_meta(None, value.clone()),
            Token::Content { value, line: _ } => {
                self.push_contents(value.clone());
                None
            }
        }
        .or(prev)
    }

    fn push_meta(
        &mut self,
        key: Option<Slice<SourceFile>>,
        value: Slice<SourceFile>,
    ) -> Option<Annotation> {
        let prev = self.flush_if(!self.contents.is_empty());

        if self.meta.first_meta.is_none() {
            self.meta.first_meta = Some(value.clone());
            self.meta.line = (self.prev_line + 1) as _;
        }

        if let Err(err) = self.meta.set(key, value) {
            self.errors.push(err);
        }

        prev
    }

    fn push_contents(&mut self, value: Slice) {
        let file = value.file();
        let value = value.trim();
        if !value.is_empty() {
            self.contents.push(file.substr(value).unwrap());
        }
    }

    fn flush_if(&mut self, cond: bool) -> Option<Annotation> {
        if cond {
            self.flush()
        } else {
            None
        }
    }

    fn flush(&mut self) -> Option<Annotation> {
        let meta = core::mem::take(&mut self.meta);
        let contents = core::mem::take(&mut self.contents);

        ensure!(meta.first_meta.is_some(), None);

        match meta.build(contents, self.default_type) {
            Ok(anno) => Some(anno),
            Err(err) => {
                self.errors.push(err);
                None
            }
        }
    }
}

impl<T: Iterator<Item = Token>> Iterator for Parser<T> {
    type Item = Annotation;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let Some(token) = self.tokens.next() else {
                return self.flush();
            };
            if let Some(annotation) = self.on_token(token) {
                return Some(annotation);
            }
        }
    }
}
