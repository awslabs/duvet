// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use super::{Section, Specification, Str};
use crate::{sourcemap::LinesIter, Error};

#[cfg(test)]
mod tests;

pub fn parse(contents: &str) -> Result<Specification, Error> {
    let mut parser = Parser::default();

    for line in Lex::new(contents) {
        parser.on_token(line)?;
    }

    let spec = parser.done()?;

    Ok(spec)
}

#[derive(Clone, Copy, Debug)]
enum Token<'a> {
    Header(Str<'a>),
    Code(Str<'a>),
    Line(Str<'a>),
}

impl<'a> Token<'a> {
    pub fn line(&self) -> &Str<'a> {
        match self {
            Self::Header(line) => line,
            Self::Code(line) => line,
            Self::Line(line) => line,
        }
    }

    fn header_name(line: Str<'a>) -> (Str<'a>, usize) {
        let line = line.trim();
        let count = line.chars().take_while(|c| *c == '#').count();
        let name = line.slice(count..line.len()).trim();
        (name, count)
    }
}

struct Lex<'a> {
    iter: LinesIter<'a>,
}

impl<'a> Lex<'a> {
    pub fn new(contents: &'a str) -> Self {
        Self {
            iter: LinesIter::new(contents),
        }
    }
}

impl<'a> Iterator for Lex<'a> {
    type Item = Token<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let line = self.iter.next()?;

        let trimmed_line = line.trim();

        if trimmed_line.starts_with('#') {
            return Some(Token::Header(line));
        }

        if trimmed_line.starts_with("```") {
            return Some(Token::Code(line));
        }

        Some(Token::Line(line))
    }
}

#[derive(Debug, Default)]
pub struct Parser<'a> {
    spec: Specification<'a>,
    state: ParserState<'a>,
    in_code: bool,
}

impl<'a> Parser<'a> {
    fn on_token(&mut self, token: Token<'a>) -> Result<(), Error> {
        if self.in_code {
            if matches!(token, Token::Code(_)) {
                self.in_code = false;
            } else {
                self.state.push_line(*token.line());
                return Ok(());
            }
        }

        match token {
            Token::Header(line) => {
                let (name, count) = Token::header_name(line);
                // TODO handle count
                let _ = count;

                if let Some(section) = self.state.new_section(name) {
                    self.push_section(section);
                }
            }
            Token::Code(line) => {
                self.in_code = true;
                self.state.push_line(line)
            }
            Token::Line(line) => {
                self.state.push_line(line);
            }
        }

        Ok(())
    }

    fn done(mut self) -> Result<Specification<'a>, Error> {
        if let ParserState::Section(section) =
            core::mem::replace(&mut self.state, ParserState::Init)
        {
            self.push_section(section);
        }

        Ok(self.spec)
    }

    fn push_section(&mut self, section: Section<'a>) {
        let name = section.id;
        self.spec.sections.insert(name.into(), section);

        if self.spec.title.is_none() {
            self.spec.title = Some(name);
        }
    }
}

#[derive(Debug)]
pub enum ParserState<'a> {
    Init,
    Section(Section<'a>),
}

impl<'a> Default for ParserState<'a> {
    fn default() -> Self {
        Self::Init
    }
}

impl<'a> ParserState<'a> {
    fn new_section(&mut self, id: Str<'a>) -> Option<Section<'a>> {
        let prev = core::mem::replace(
            self,
            Self::Section(Section {
                id,
                title: id,
                full_title: id,
                lines: vec![],
            }),
        );

        if let Self::Section(section) = prev {
            Some(section)
        } else {
            None
        }
    }

    fn push_line(&mut self, line: Str<'a>) {
        match self {
            Self::Init => {}
            Self::Section(section) => {
                section.lines.push(line);
            }
        }
    }
}
