// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

pub fn normalize(value: &str) -> String {
    normalize_mapped::<()>(value).0
}

pub fn normalize_mapped<O: OffsetMap>(value: &str) -> (String, O) {
    let offset_map = O::with_capacity(value.len());
    let out = String::with_capacity(value.len());

    let mut mapper = Mapper {
        offset_map,
        out,
        buffer: None,
        last_end: 0,
    };

    for (idx, c) in value.char_indices() {
        mapper.on_char(idx, c);
    }

    let (out, offset_map) = mapper.finish();

    (out, offset_map)
}

pub trait OffsetMap {
    fn with_capacity(len: usize) -> Self;
    fn push(&mut self, idx: usize);
}

impl OffsetMap for () {
    #[inline]
    fn with_capacity(_len: usize) -> Self {}

    #[inline]
    fn push(&mut self, _idx: usize) {}
}

impl OffsetMap for Vec<usize> {
    #[inline]
    fn with_capacity(len: usize) -> Self {
        Vec::with_capacity(len + 1)
    }

    #[inline]
    fn push(&mut self, idx: usize) {
        self.push(idx);
    }
}

struct Mapper<O: OffsetMap> {
    out: String,
    offset_map: O,
    buffer: Option<Buffer>,
    last_end: usize,
}

impl<O: OffsetMap> Mapper<O> {
    #[inline]
    fn on_char(&mut self, idx: usize, c: char) {
        if c.is_alphanumeric() {
            self.flush();
            self.push(idx, c);
            return;
        }

        if c.is_whitespace() {
            if self.buffer.is_none() && !self.out.is_empty() {
                self.buffer = Some(Buffer {
                    start: idx,
                    is_ws: true,
                    c,
                });
            }
            return;
        }

        // punctuation
        if let Some(buffer) = self.buffer.as_ref() {
            if !buffer.is_ws {
                self.flush();
            }
        }

        self.buffer = Some(Buffer {
            start: idx,
            is_ws: false,
            c,
        });
    }

    #[inline]
    fn flush(&mut self) {
        if let Some(buffer) = self.buffer.take() {
            self.push(buffer.start, buffer.c);
        }
    }

    #[inline]
    fn push(&mut self, idx: usize, c: char) {
        self.out.push(c);
        let len = c.len_utf8();
        for _ in 0..len {
            self.offset_map.push(idx);
        }
        self.last_end = idx + len;
    }

    #[inline]
    fn finish(mut self) -> (String, O) {
        if let Some(buffer) = self.buffer.take() {
            if !buffer.is_ws {
                self.push(buffer.start, buffer.c);
            }
        }
        self.offset_map.push(self.last_end);
        (self.out, self.offset_map)
    }
}

struct Buffer {
    start: usize,
    is_ws: bool,
    c: char,
}
