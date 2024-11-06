// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use core::{
    fmt,
    ops::{Deref, Range},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Str<'a> {
    pub value: &'a str,
    pub pos: usize,
    pub line: usize,
}

impl fmt::Display for Str<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.value.fmt(f)
    }
}

impl Str<'_> {
    pub fn indentation(&self) -> usize {
        let trimmed_line = self.trim_start();
        self.len() - trimmed_line.len()
    }

    pub fn slice(&self, bounds: Range<usize>) -> Self {
        let pos = self.pos + bounds.start;
        let value = &self.value[bounds];
        Self {
            value,
            pos,
            line: self.line,
        }
    }

    pub fn substr(&self, other: &str) -> Option<Self> {
        let s_start = self.value.as_ptr() as usize;
        let o_start = other.as_ptr() as usize;

        let start = o_start.checked_sub(s_start)?;
        let end = start.checked_add(other.len())?;
        let range = start..end;

        if self.range().end < end {
            return None;
        }

        Some(self.slice(range))
    }

    pub fn range(&self) -> Range<usize> {
        let pos = self.pos;
        pos..(pos + self.value.len())
    }

    pub fn trim(&self) -> Self {
        let value = self.value.trim_start();
        let pos = self.pos + (self.len() - value.len());
        let value = value.trim_end();
        Self {
            value,
            pos,
            line: self.line,
        }
    }

    pub fn trim_end_matches(&self, pat: char) -> Self {
        let value = self.value.trim_end_matches(pat);
        Self {
            value,
            pos: self.pos,
            line: self.line,
        }
    }
}

impl Deref for Str<'_> {
    type Target = str;

    fn deref(&self) -> &str {
        self.value
    }
}

impl<'a> From<Str<'a>> for &'a str {
    fn from(s: Str<'a>) -> Self {
        s.value
    }
}
