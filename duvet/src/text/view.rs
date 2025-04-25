// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use core::ops::{Deref, Range};
use duvet_core::file::{Slice, SourceFile};

pub fn view<'a, C>(contents: C) -> Option<View>
where
    C: IntoIterator<Item = &'a Slice>,
{
    View::new(contents.into_iter())
}

#[derive(Debug)]
pub struct View {
    value: String,
    byte_map: Vec<usize>,
    file: SourceFile,
}

impl View {
    pub fn new<'a, C>(contents: C) -> Option<Self>
    where
        C: Iterator<Item = &'a Slice>,
    {
        let mut value = String::new();
        let mut byte_map = vec![];
        let mut file = None;
        let mut pushed_chunk = false;

        for chunk in contents {
            if file.is_none() {
                file = Some(chunk.file().clone());
            }

            let trimmed = chunk.trim();
            if trimmed.is_empty() {
                continue;
            }

            // check if we already pushed a chunk. if so we need to add some whitespace
            if core::mem::replace(&mut pushed_chunk, true) {
                value.push(' ');
                byte_map.push(usize::MAX);
            }

            value.push_str(trimmed);
            let range = chunk.file().substr(trimmed).unwrap().range();
            byte_map.extend(range.clone());
        }

        debug_assert_eq!(value.len(), byte_map.len());
        let file = file?;

        Some(Self {
            value,
            byte_map,
            file,
        })
    }

    pub fn ranges(&self, src: Range<usize>) -> StrRangeIter {
        StrRangeIter {
            byte_map: &self.byte_map,
            file: &self.file,
            start: src.start,
            end: src.end,
        }
    }
}

impl Deref for View {
    type Target = str;

    fn deref(&self) -> &str {
        &self.value
    }
}

pub struct StrRangeIter<'a> {
    byte_map: &'a [usize],
    file: &'a SourceFile,
    start: usize,
    end: usize,
}

impl Iterator for StrRangeIter<'_> {
    type Item = Slice;

    fn next(&mut self) -> Option<Self::Item> {
        if self.start == self.end {
            return None;
        }

        let mut start_target = self.byte_map[self.start];
        while start_target == usize::MAX {
            self.start += 1;
            if self.start == self.end {
                return None;
            }
            start_target = self.byte_map[self.start];
        }

        let mut range = start_target..start_target;
        self.start += 1;

        for i in self.start..self.end {
            let target = self.byte_map[i];

            if target == usize::MAX {
                break;
            }

            if range.end <= target {
                range.end = target + 1;
                self.start += 1;
            } else {
                break;
            }
        }

        let slice = self.file.substr_range(range).expect("missing range");

        Some(slice)
    }
}
