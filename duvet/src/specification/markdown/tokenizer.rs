// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use core::{iter::Peekable, ops::Range};
use duvet_core::file::{Slice, SourceFile};
use pulldown_cmark::Event;

#[derive(Clone, Debug)]
pub enum Token {
    Section {
        id: Option<Slice>,
        title: Slice,
        level: u8,
        #[allow(dead_code)]
        line: usize,
    },
    Break {
        #[allow(dead_code)]
        line: usize,
    },
    Content {
        value: Slice,
        #[allow(dead_code)]
        line: usize,
    },
}

pub fn tokens(contents: &SourceFile) -> impl Iterator<Item = Token> + '_ {
    let lines = contents.lines_slices().enumerate().map(|(idx, line)| {
        // lines start with 1
        (idx + 1, line)
    });
    let options = pulldown_cmark::Options::ENABLE_HEADING_ATTRIBUTES;
    let cmark = pulldown_cmark::Parser::new_ext(contents, options).into_offset_iter();
    Tokenizer::new(contents, lines, cmark)
}

struct Tokenizer<'a, L, E>
where
    L: Iterator<Item = (usize, Slice)>,
    E: Iterator<Item = (Event<'a>, Range<usize>)>,
{
    contents: &'a SourceFile,
    lines: Peekable<L>,
    cmark: Peekable<E>,
    next_line: Option<(usize, Slice)>,
    next_token: Option<Token>,
}

impl<'a, L, E> Tokenizer<'a, L, E>
where
    L: Iterator<Item = (usize, Slice)>,
    E: Iterator<Item = (Event<'a>, Range<usize>)>,
{
    fn new(contents: &'a SourceFile, lines: L, cmark: E) -> Self {
        Self {
            contents,
            lines: lines.peekable(),
            cmark: cmark.peekable(),
            next_line: None,
            next_token: None,
        }
    }
}

impl<'a, L, E> Iterator for Tokenizer<'a, L, E>
where
    L: Iterator<Item = (usize, Slice)>,
    E: Iterator<Item = (Event<'a>, Range<usize>)>,
{
    type Item = Token;

    fn next(&mut self) -> Option<Self::Item> {
        use pulldown_cmark::{Event::*, HeadingLevel::*, Tag, TagEnd};

        let mut header_buffer: Option<(usize, Range<usize>, Option<Slice>)> = None;
        let mut text_buffer: Option<Range<usize>> = None;

        loop {
            if let Some(token) = self.next_token.take() {
                return Some(token);
            }

            let (lineno, line) = if let Some(line) = self.next_line.take() {
                line
            } else {
                self.lines.next()?
            };

            while let Some((event, event_range)) = self.cmark.next_if(|(_, range)| {
                let line_range = line.range();
                line_range.contains(&range.start) || range.start < line_range.start
            }) {
                match event {
                    // start buffering the header contents
                    Start(Tag::Heading { id, .. }) => {
                        // convert the fragment to a Slice
                        let fragment = id.and_then(|f| self.contents.substr(f.as_ref()));
                        header_buffer = Some((lineno, line.range(), fragment));
                    }
                    // we're done parsing the header
                    End(TagEnd::Heading(level)) => {
                        // consume any lines captured by the header
                        while self
                            .lines
                            .next_if(|(_lineno, line)| line.range().start < event_range.end)
                            .is_some()
                        {}

                        let id = header_buffer
                            .as_ref()
                            .and_then(|(_, _, fragment)| fragment.clone());

                        // convert the header buffer into a Slice
                        let (lineno, line) = if let Some((lineno, mut buf, _)) = header_buffer {
                            let r = line.range();
                            buf.start = r.start.min(buf.start);
                            buf.end = r.end.max(buf.end);
                            let line = self.contents.substr_range(buf).expect("invalid range");
                            (lineno, line)
                        } else {
                            (lineno, line)
                        };

                        // convert the text buffer range to a Slice
                        let title = if let Some(title_range) = text_buffer {
                            self.contents.substr_range(title_range).expect("invalid range")
                        } else {
                            line
                        };

                        let level = match level {
                            H1 => 1,
                            H2 => 2,
                            H3 => 3,
                            H4 => 4,
                            H5 => 5,
                            H6 => 6,
                        };

                        return Some(Token::Section {
                            line: lineno,
                            level,
                            id,
                            title,
                        });
                    }
                    // insert a token break before returning the line
                    Start(Tag::Item) => {
                        self.next_line = Some((lineno, line));
                        return Some(Token::Break { line: lineno });
                    }
                    // insert a token break after returning the item line
                    End(TagEnd::Item) => {
                        self.next_token = Some(Token::Break { line: lineno });
                    }
                    // buffer the text if we're parsing a header
                    Text(t) if header_buffer.is_some() => {
                        if let Some(t) = self.contents.substr(&t) {
                            let r = t.range();
                            if let Some(buf) = &mut text_buffer {
                                buf.start = r.start.min(buf.start);
                                buf.end = r.end.max(buf.end);
                            } else {
                                text_buffer = Some(r);
                            }
                        }
                    }
                    _ => {
                        // If we are buffering a heading and we get a non-text
                        // event, we have to move the end of our heading buffer
                        // to the end of the event range to capture the content
                        // of the event within our heading.
                        if let Some(buf) = &mut text_buffer {
                            buf.end = event_range.end;
                        }
                        continue;
                    }
                }
            }

            // if we're not buffering anything for the header then return
            if header_buffer.is_none() {
                return Some(Token::Content {
                    line: lineno,
                    value: line,
                });
            }
        }
    }
}
