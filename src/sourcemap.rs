// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use core::{
    fmt,
    ops::{Deref, Range},
};

#[derive(Clone, Copy, Debug)]
pub struct LinesIter<'a> {
    content: &'a str,
    line: usize,
    offset: usize,
}

impl<'a> LinesIter<'a> {
    pub fn new(content: &'a str) -> Self {
        Self {
            content,
            offset: 0,
            line: 1,
        }
    }
}

impl<'a> Iterator for LinesIter<'a> {
    type Item = Str<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let content = &self.content[self.offset..];

        if content.is_empty() {
            return None;
        }

        let pos = self.offset;

        let rel_offset = if let Some(next_newline) = content.find('\n') {
            self.offset += next_newline + 1; // trim \n
            next_newline
        } else {
            // consume the remaining characters
            let len = content.len();
            self.offset += len;
            len
        };

        let value = Str {
            value: content[..rel_offset].trim_end_matches('\r'),
            pos,
            line: self.line,
        };

        self.line += 1;
        Some(value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Str<'a> {
    pub value: &'a str,
    pub pos: usize,
    pub line: usize,
}

impl<'a> fmt::Display for Str<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.value.fmt(f)
    }
}

impl<'a> Str<'a> {
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

impl<'a> Deref for Str<'a> {
    type Target = str;

    fn deref(&self) -> &str {
        self.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use insta::assert_debug_snapshot;

    #[test]
    fn lines_iter_with_trailing_newline() {
        assert_debug_snapshot!(LinesIter::new("line 1\nline 2\n").collect::<Vec<_>>());
    }

    #[test]
    fn lines_iter_without_trailing_newline() {
        assert_debug_snapshot!(LinesIter::new("line 1\nline 2").collect::<Vec<_>>());
    }

    #[test]
    fn lines_iter_cr_newline() {
        assert_debug_snapshot!(LinesIter::new("line 1\r\nline 2\r\n").collect::<Vec<_>>());
    }
}
