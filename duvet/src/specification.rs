// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::{Error, Result};
use core::{cmp::Ordering, fmt, str::FromStr};
use duvet_core::{
    error,
    file::{Slice, SourceFile},
};
use std::collections::HashMap;

pub mod ietf;
pub mod markdown;

#[derive(Default)]
pub struct Specification {
    pub title: Option<String>,
    pub sections: HashMap<String, Section>,
    pub format: Format,
}

impl fmt::Debug for Specification {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Specification")
            .field("title", &self.title)
            .field("sections", &self.sorted_sections())
            .field("format", &self.format)
            .finish()
    }
}

impl Specification {
    pub fn sorted_sections(&self) -> Vec<&Section> {
        let mut sections: Vec<_> = self.sections.values().collect();

        // rely on the section ordering
        sections.sort();

        sections
    }

    pub fn section(&self, id: &str) -> Option<&Section> {
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
                if let Some(section) = self.sections.get(&format!("{prefix}{id}")) {
                    return Some(section);
                }
            }

            None
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd, Ord, Eq, Hash)]
pub enum Format {
    #[default]
    Auto,
    Ietf,
    Markdown,
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let v = match self {
            Self::Auto => "auto",
            Self::Ietf => "ietf",
            Self::Markdown => "markdown",
        };
        write!(f, "{v}")
    }
}

impl Format {
    pub fn parse(self, contents: &SourceFile) -> Result<Specification> {
        let spec = match self {
            Self::Auto => {
                if let Some(ext) = contents.path().extension() {
                    if ext == "md" || ext == "markdown" {
                        return markdown::parse(contents);
                    }
                }

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
            _ => Err(error!(format!(
                "Invalid spec type {:?}, expected one of {:?}",
                v,
                ["auto", "ietf", "markdown"]
            ))),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Line {
    Str(Slice),
    /// Used when extracting requirements to break content into separate groupings.
    Break,
}

impl From<Slice> for Line {
    fn from(s: Slice) -> Self {
        Self::Str(s)
    }
}

#[derive(Clone, Debug, Eq)]
pub struct Section {
    pub id: String,
    pub title: String,
    pub full_title: Slice,
    pub lines: Vec<Line>,
}

impl Section {
    pub fn view(&self) -> crate::text::view::View {
        crate::text::view(self.lines.iter().filter_map(|l| match l {
            Line::Str(slice) => Some(slice),
            Line::Break => None,
        }))
    }

    pub fn original_text(&self) -> Option<Slice> {
        let mut start = usize::MAX;
        let mut end = 0;
        let mut has_content = false;

        for line in self.lines.iter() {
            match line {
                Line::Str(v) => {
                    let r = v.range();
                    start = start.min(r.start);
                    end = end.max(r.end);
                    has_content = true;
                }
                Line::Break => {}
            }
        }

        if !has_content {
            return None;
        }

        Some(self.full_title.file().substr_range(start..end).unwrap())
    }
}

impl Ord for Section {
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
        cmp!(full_title.range().start);
        cmp!(full_title.as_ref());

        cmp!(id);
        cmp!(title);

        cmp!(lines);

        Ordering::Equal
    }
}

impl core::hash::Hash for Section {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
        self.title.hash(state);
        self.full_title.hash(state);
        self.lines.hash(state);
    }
}

impl PartialOrd for Section {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Section {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
