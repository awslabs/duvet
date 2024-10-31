// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use super::tokenizer::Token;
use core::fmt;
use duvet_core::file::Slice;

pub fn parse<T: IntoIterator<Item = Token>>(tokens: T) -> Parser<T::IntoIter> {
    Parser {
        section: None,
        tokens: tokens.into_iter(),
    }
}

pub struct Parser<T> {
    section: Option<Section>,
    tokens: T,
}

pub struct Section {
    pub id: Id,
    pub title: Slice,
    pub line: usize,
    pub lines: Vec<(usize, Slice)>,
}

pub enum Id {
    Section(Slice),
    Appendix(Slice),
    NamedSection(Slice),
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Id::Section(id) => write!(f, "section-{id}"),
            Id::Appendix(id) => write!(f, "appendix-{id}"),
            // TODO slugify
            Id::NamedSection(title) => write!(f, "named-{title}"),
        }
    }
}

impl Section {
    fn push(&mut self, line: usize, value: Slice) {
        // don't push an empty first line
        if self.lines.is_empty() && value.trim().is_empty() {
            return;
        }

        self.lines.push((line, value));
    }
}

impl<T: Iterator<Item = Token>> Parser<T> {
    fn on_token(&mut self, token: Token) -> Option<Section> {
        match token {
            Token::Section { id, title, line } => {
                let prev = self.flush();

                self.section = Some(Section {
                    id: Id::Section(id),
                    title,
                    line,
                    lines: vec![],
                });

                prev
            }
            Token::Appendix { id, title, line } => {
                let prev = self.flush();

                self.section = Some(Section {
                    id: Id::Appendix(id),
                    title,
                    line,
                    lines: vec![],
                });

                prev
            }
            Token::NamedSection { title, line } => {
                let prev = self.flush();

                self.section = Some(Section {
                    id: Id::NamedSection(title.clone()),
                    title,
                    line,
                    lines: vec![],
                });

                prev
            }
            Token::Break { line, value, ty: _ } => {
                if let Some(section) = self.section.as_mut() {
                    // just get the line offset
                    let trimmed = &value[0..0];
                    let value = value.file().substr(trimmed).unwrap();
                    section.push(line, value);
                }

                None
            }
            Token::Content { line, value } => {
                if let Some(section) = self.section.as_mut() {
                    section.push(line, value);
                }

                None
            }
            Token::Header { value: _, line: _ } => {
                // ignore headers
                None
            }
        }
    }

    fn flush(&mut self) -> Option<Section> {
        let mut section = core::mem::take(&mut self.section)?;

        // trim any trailing lines
        loop {
            let Some((_lineno, line)) = section.lines.last() else {
                break;
            };

            if !line.trim().is_empty() {
                break;
            }

            section.lines.pop();
        }

        Some(section)
    }
}

impl<T: Iterator<Item = Token>> Iterator for Parser<T> {
    type Item = Section;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let Some(token) = self.tokens.next() else {
                return self.flush();
            };
            if let Some(section) = self.on_token(token) {
                return Some(section);
            }
        }
    }
}
