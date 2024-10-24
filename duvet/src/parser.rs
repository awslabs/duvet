// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    annotation::{Annotation, AnnotationLevel, AnnotationType},
    specification::Format,
    Error,
};
use anyhow::anyhow;
use core::convert::TryInto;

pub struct Parser<'a>(pub &'a [u8]);

#[derive(Debug, Default)]
pub struct ParsedAnnotation<'a> {
    pub target: &'a str,
    pub quote: &'a str,
    pub anno: AnnotationType,
    pub comment: &'a str,
    pub source: &'a str,
    pub anno_line: u32,
    pub anno_column: u32,
    pub item_line: u32,
    pub item_column: u32,
    pub path: &'a str,
    pub manifest_dir: &'a str,
    pub feature: &'a str,
    pub tracking_issue: &'a str,
    pub level: AnnotationLevel,
    pub format: Format,
}

const U32_SIZE: usize = core::mem::size_of::<u32>();

macro_rules! read_u32 {
    ($buf:ident) => {{
        let (len, buf) = $buf.split_at(U32_SIZE);
        let len = u32::from_le_bytes(len.try_into()?) as usize;
        (len, buf)
    }};
}

impl<'a> ParsedAnnotation<'a> {
    fn parse(data: &'a [u8]) -> Result<(Self, &'a [u8]), Error> {
        let mut parsed = Self::default();
        let (len_prefix, data) = read_u32!(data);
        let (chunk, remaining) = data.split_at(len_prefix);
        let (version, mut chunk) = read_u32!(chunk);

        if version != 0 {
            return Err(anyhow!(format!("Invalid version {:?}", version)));
        }

        while !chunk.is_empty() {
            let (name, peek) = chunk.split_at(U32_SIZE);
            let (len, peek) = read_u32!(peek);
            let (value, peek) = peek.split_at(len);

            macro_rules! to_u32 {
                () => {
                    u32::from_le_bytes(value.try_into()?)
                };
            }

            macro_rules! to_str {
                () => {
                    core::str::from_utf8(value)?
                };
            }

            match name {
                b"spec" => parsed.target = to_str!(),
                b"quot" => parsed.quote = to_str!(),
                b"anno" => parsed.anno = to_str!().parse()?,
                b"comm" => parsed.comment = to_str!(),
                b"file" => parsed.source = to_str!(),
                b"ilin" => parsed.item_line = to_u32!(),
                b"icol" => parsed.item_column = to_u32!(),
                b"alin" => parsed.anno_line = to_u32!(),
                b"acol" => parsed.anno_column = to_u32!(),
                b"path" => parsed.path = to_str!(),
                b"mand" => parsed.manifest_dir = to_str!(),
                b"slvl" => parsed.level = to_str!().parse()?,
                b"sfmt" => parsed.format = to_str!().parse()?,
                other => {
                    if cfg!(debug_assertions) {
                        panic!("unhandled annotation field {:?}", other)
                    }
                }
            }

            chunk = peek;
        }

        Ok((parsed, remaining))
    }
}

impl<'a> From<ParsedAnnotation<'a>> for Annotation {
    fn from(a: ParsedAnnotation<'a>) -> Self {
        Annotation {
            target: a.target.to_string(),
            quote: a.quote.to_string(),
            anno: a.anno,
            comment: a.comment.to_string(),
            source: a.source.into(),
            path: a.path.to_string(),
            anno_line: a.anno_line,
            anno_column: a.anno_column,
            item_line: a.item_line,
            item_column: a.item_column,
            manifest_dir: a.manifest_dir.into(),
            level: a.level,
            format: a.format,
            feature: a.feature.to_string(),
            tags: Default::default(),
            tracking_issue: a.tracking_issue.to_string(),
        }
    }
}

impl Iterator for Parser<'_> {
    type Item = Result<Annotation, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        let data = self.0;
        if data.is_empty() {
            return None;
        }

        match ParsedAnnotation::parse(data) {
            Ok((annotation, data)) => {
                self.0 = data;
                Some(Ok(annotation.into()))
            }
            Err(err) => Some(Err(err)),
        }
    }
}
