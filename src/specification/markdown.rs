// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use super::{Section, Specification, Str};
use crate::{sourcemap::LinesIter, Error};
use core::{iter::Peekable, ops::Range};

#[cfg(test)]
mod tests;

pub fn parse(contents: &str) -> Result<Specification, Error> {
    let mut parser = Parser::default();

    for token in Lex::new(contents) {
        parser.on_token(token)?;
    }

    let spec = parser.done()?;

    Ok(spec)
}

#[derive(Clone, Copy, Debug)]
enum Token<'a> {
    Header {
        line: Str<'a>,
        name: Str<'a>,
        fragment: Option<Str<'a>>,
        level: u8,
    },
    Line(Str<'a>),
    Break,
}

struct Lex<'a> {
    contents: &'a str,
    lines: Peekable<LinesIter<'a>>,
    cmark: Peekable<pulldown_cmark::OffsetIter<'a, 'a>>,
    next_line: Option<Str<'a>>,
    next_token: Option<Token<'a>>,
}

impl<'a> Lex<'a> {
    pub fn new(contents: &'a str) -> Self {
        Self {
            contents,
            lines: LinesIter::new(contents).peekable(),
            cmark: pulldown_cmark::Parser::new(contents)
                .into_offset_iter()
                .peekable(),
            next_line: None,
            next_token: None,
        }
    }

    fn slice(&self, line: usize, range: Range<usize>) -> Str<'a> {
        let pos = range.start;
        Str {
            value: &self.contents[range],
            pos,
            line,
        }
    }
}

impl<'a> Iterator for Lex<'a> {
    type Item = Token<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        use pulldown_cmark::{Event::*, HeadingLevel::*, Tag::*};

        let mut header_buffer = None;
        let mut text_buffer: Option<(usize, Range<usize>)> = None;

        loop {
            if let Some(token) = self.next_token.take() {
                return Some(token);
            }

            let line = if let Some(line) = self.next_line.take() {
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
                    Start(Heading(_, _, _)) => {
                        header_buffer = Some((line.line, line.range()));
                    }
                    // we're done parsing the header
                    End(Heading(level, fragment, _classes)) => {
                        // consume any lines captured by the header
                        while self
                            .lines
                            .next_if(|line| line.pos < event_range.end)
                            .is_some()
                        {}

                        // convert the header buffer into a Str
                        let line = if let Some((line_num, mut buf)) = header_buffer {
                            let r = line.range();
                            buf.start = r.start.min(buf.start);
                            buf.end = r.end.max(buf.end);
                            self.slice(line_num, buf)
                        } else {
                            line
                        };

                        // convert the fragment to a Str
                        let fragment = fragment.and_then(|f| line.substr(f));

                        // convert the text buffer range to a Str
                        let name = if let Some((line_num, mut buf)) = text_buffer {
                            buf.end = line.range().end.max(buf.end);
                            self.slice(line_num, buf)
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

                        return Some(Token::Header {
                            line,
                            level,
                            fragment,
                            name,
                        });
                    }
                    // insert a token break before returning the line
                    Start(Item) => {
                        self.next_line = Some(line);
                        return Some(Token::Break);
                    }
                    // insert a token break after returning the item line
                    End(Item) => {
                        self.next_token = Some(Token::Break);
                    }
                    // buffer the text if we're parsing a header
                    Text(t) => {
                        if header_buffer.is_some() {
                            if let Some(t) = line.substr(&t) {
                                let r = t.range();
                                if let Some((_line, buf)) = &mut text_buffer {
                                    buf.start = r.start.min(buf.start);
                                    buf.end = r.end.max(buf.end);
                                } else {
                                    text_buffer = Some((t.line, r));
                                }
                            }
                        }
                    }
                    _ => {
                        continue;
                    }
                }
            }

            // if we're not buffering anything for the header then return
            if header_buffer.is_none() {
                return Some(Token::Line(line));
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct Parser<'a> {
    spec: Specification<'a>,
    state: ParserState<'a>,
}

impl<'a> Parser<'a> {
    fn on_token(&mut self, token: Token<'a>) -> Result<(), Error> {
        match token {
            Token::Header {
                fragment,
                name,
                line,
                level,
            } => {
                if let Some((section, level)) =
                    self.state.new_section(fragment, name, line.trim(), level)
                {
                    self.push_section(section, level);
                }
            }
            Token::Line(line) => {
                self.state.push_line(line);
            }
            Token::Break => {
                self.state.push_break();
            }
        }

        Ok(())
    }

    fn done(mut self) -> Result<Specification<'a>, Error> {
        if let ParserState::Section { section, level } =
            core::mem::replace(&mut self.state, ParserState::Init)
        {
            self.push_section(section, level);
        }

        Ok(self.spec)
    }

    fn push_section(&mut self, section: Section<'a>, level: u8) {
        // set the document title if it's a H1 and we haven't set it yet
        if self.spec.title.is_none() && level == 1 {
            self.spec.title = Some(section.title.clone());
        }

        let name = section.id.clone();
        self.spec.sections.insert(name, section);
    }
}

#[derive(Debug)]
pub enum ParserState<'a> {
    Init,
    Section { section: Section<'a>, level: u8 },
}

impl<'a> Default for ParserState<'a> {
    fn default() -> Self {
        Self::Init
    }
}

impl<'a> ParserState<'a> {
    fn new_section(
        &mut self,
        id: Option<Str<'a>>,
        title: Str<'a>,
        full_title: Str<'a>,
        level: u8,
    ) -> Option<(Section<'a>, u8)> {
        let mut formatted_title = String::with_capacity(title.len());
        for line in crate::sourcemap::LinesIter::new(title.value) {
            if !formatted_title.is_empty() {
                formatted_title.push(' ');
            }
            formatted_title.push_str(&line);
        }

        let id = id
            .map(|i| i.to_string())
            .unwrap_or_else(|| slug::slugify(&*title));

        let prev = core::mem::replace(
            self,
            Self::Section {
                level,
                section: Section {
                    id,
                    title: formatted_title,
                    full_title,
                    lines: vec![],
                },
            },
        );

        if let Self::Section { section, level } = prev {
            Some((section, level))
        } else {
            None
        }
    }

    fn push_line(&mut self, line: Str<'a>) {
        match self {
            Self::Init => {}
            Self::Section { section, .. } => {
                // filter out any beginning empty lines
                if section.lines.is_empty() && line.trim().is_empty() {
                    return;
                }
                section.lines.push(line.into());
            }
        }
    }

    fn push_break(&mut self) {
        match self {
            Self::Init => {}
            Self::Section { section, .. } => {
                if let Some(super::Line::Str(s)) = section.lines.last() {
                    // only push a break if we have a non-empty line before it
                    if !s.trim().is_empty() {
                        section.lines.push(super::Line::Break);
                    }
                }
            }
        }
    }
}
