// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::{sourcemap::Str, Error};
use anyhow::anyhow;
use core::{
    cmp::Ordering,
    fmt,
    ops::{Deref, Range},
    str::FromStr,
};
use std::collections::HashMap;

pub mod ietf;
pub mod markdown;

#[derive(Default)]
pub struct Specification<'a> {
    pub title: Option<String>,
    pub sections: HashMap<String, Section<'a>>,
    pub format: Format,
}

impl fmt::Debug for Specification<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Specification")
            .field("title", &self.title)
            .field("sections", &self.sorted_sections())
            .field("format", &self.format)
            .finish()
    }
}

impl<'a> Specification<'a> {
    pub fn sorted_sections(&self) -> Vec<&Section<'a>> {
        let mut sections: Vec<_> = self.sections.values().collect();

        // rely on the section ordering
        sections.sort();

        sections
    }

    pub fn section(&self, id: &str) -> Option<&Section<'a>> {
        self.sections.get(id).or_else(|| {
            // special case ietf references
            if !matches!(self.format, Format::Ietf) {
                return None;
            }

            // allow references to drop the section or appendix prefixes
            let id = id
                .trim_start_matches("section-")
                .trim_start_matches("appendix-");

            for prefix in ["section-", "appendix-"] {
                if let Some(section) = self.sections.get(&format!("{}{}", prefix, id)) {
                    return Some(section);
                }
            }

            None
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Ord, Eq, Hash)]
pub enum Format {
    Auto,
    Ietf,
    Markdown,
}

impl Default for Format {
    fn default() -> Self {
        Self::Auto
    }
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let v = match self {
            Self::Auto => "auto",
            Self::Ietf => "ietf",
            Self::Markdown => "markdown",
        };
        write!(f, "{}", v)
    }
}

impl Format {
    pub fn parse(self, contents: &str) -> Result<Specification, Error> {
        let spec = match self {
            Self::Auto => {
                // Markdown MAY start with a header (#),
                // but it also MAY start with a license/copyright.
                // In which case it is probably start something like
                // [//]: "Copyright Foo"
                if contents.trim().starts_with('#') || contents.trim().starts_with("[//]:") {
                    markdown::parse(contents)
                } else {
                    ietf::parse(contents)
                }
            }
            Self::Ietf => ietf::parse(contents),
            Self::Markdown => markdown::parse(contents),
        }?;

        if cfg!(debug_assertions) {
            for section in spec.sections.values() {
                for content in &section.lines {
                    if let Line::Str(content) = content {
                        assert_eq!(
                            content.value,
                            &contents[content.range()],
                            "ranges are incorrect expected {:?}, actual {:?}",
                            {
                                let start = (content.value.as_ptr() as usize)
                                    - (contents.as_ptr() as usize);
                                start..(start + content.value.len())
                            },
                            content.range(),
                        );
                    }
                }
            }
        }

        Ok(spec)
    }
}

impl FromStr for Format {
    type Err = Error;

    fn from_str(v: &str) -> Result<Self, Self::Err> {
        match v {
            "AUTO" | "auto" => Ok(Self::Auto),
            "IETF" | "ietf" => Ok(Self::Ietf),
            "MARKDOWN" | "markdown" | "md" => Ok(Self::Markdown),
            _ => Err(anyhow!(format!("Invalid spec type {:?}", v))),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Line<'a> {
    Str(Str<'a>),
    Break,
}

impl Line<'_> {
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Str(s) => s.is_empty(),
            Self::Break => true,
        }
    }
}

impl<'a> From<Str<'a>> for Line<'a> {
    fn from(s: Str<'a>) -> Self {
        Self::Str(s)
    }
}

#[derive(Clone, Debug, Eq)]
pub struct Section<'a> {
    pub id: String,
    pub title: String,
    pub full_title: Str<'a>,
    pub lines: Vec<Line<'a>>,
}

impl Ord for Section<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        macro_rules! cmp {
            ($($tt:tt)*) => {
                match self.$($tt)*.cmp(&other.$($tt)*) {
                    Ordering::Equal => {},
                    other => return other,
                }
            }
        }

        // compare the full title position first to order by appearance
        cmp!(full_title.pos);
        cmp!(full_title.value);

        cmp!(id);
        cmp!(title);

        cmp!(lines);

        Ordering::Equal
    }
}

impl core::hash::Hash for Section<'_> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
        self.title.hash(state);
        self.full_title.hash(state);
        self.lines.hash(state);
    }
}

impl PartialOrd for Section<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Section<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Section<'_> {
    pub fn contents(&self) -> StrView {
        StrView::new(&self.lines)
    }
}

#[derive(Debug)]
pub struct StrView {
    pub value: String,
    pub byte_map: Vec<usize>,
    pub line_map: Vec<usize>,
}

impl StrView {
    pub fn new(contents: &[Line]) -> Self {
        let mut value = String::new();
        let mut byte_map = vec![];
        let mut line_map = vec![];

        for chunk in contents {
            if let Line::Str(chunk) = chunk {
                let chunk = chunk.trim();
                if !chunk.is_empty() {
                    value.push_str(chunk.deref());
                    value.push(' ');
                    let mut range = chunk.range();
                    range.end += 1; // account for new line
                    line_map.extend(range.clone().map(|_| chunk.line));
                    byte_map.extend(range);
                }
            }
        }

        debug_assert_eq!(value.len(), byte_map.len());
        debug_assert_eq!(value.len(), line_map.len());

        Self {
            value,
            byte_map,
            line_map,
        }
    }

    pub fn ranges(&self, src: Range<usize>) -> StrRangeIter {
        StrRangeIter {
            byte_map: &self.byte_map,
            line_map: &self.line_map,
            start: src.start,
            end: src.end,
        }
    }
}

impl Deref for StrView {
    type Target = str;

    fn deref(&self) -> &str {
        &self.value
    }
}

pub struct StrRangeIter<'a> {
    byte_map: &'a [usize],
    line_map: &'a [usize],
    start: usize,
    end: usize,
}

impl Iterator for StrRangeIter<'_> {
    type Item = (usize, Range<usize>);

    fn next(&mut self) -> Option<Self::Item> {
        if self.start == self.end {
            return None;
        }

        let start_target = self.byte_map[self.start];
        let line = self.line_map[self.start];
        let mut range = start_target..start_target;
        self.start += 1;

        for i in self.start..self.end {
            let target_line = self.line_map[i];
            let target = self.byte_map[i];

            if line != target_line {
                break;
            }

            if range.end <= target {
                range.end = target + 1;
                self.start += 1;
            } else {
                break;
            }
        }

        Some((line, range))
    }
}
