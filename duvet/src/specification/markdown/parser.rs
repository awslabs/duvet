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
    pub level: u8,
    pub id: Id,
    pub title: Slice,
    pub lines: Vec<Option<Slice>>,
}

pub enum Id {
    Fragment(Slice),
    Title(Slice),
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Id::Fragment(id) => write!(f, "{id}"),
            Id::Title(title) => write!(f, "{}", slug::slugify(title)),
        }
    }
}

impl Section {
    fn push(&mut self, value: Option<Slice>) {
        // don't push an empty first line
        if self.lines.is_empty() && is_empty(&value) {
            return;
        }

        self.lines.push(value);
    }
}

impl<T: Iterator<Item = Token>> Parser<T> {
    fn on_token(&mut self, token: Token) -> Option<Section> {
        match token {
            Token::Section {
                id,
                title,
                level,
                line: _,
            } => {
                let prev = self.flush();

                let id = id
                    .map(Id::Fragment)
                    .unwrap_or_else(|| Id::Title(title.clone()));

                self.section = Some(Section {
                    level,
                    id,
                    title,
                    lines: vec![],
                });

                prev
            }
            Token::Break { line: _ } => {
                if let Some(section) = self.section.as_mut() {
                    // just get the line offset
                    section.push(None);
                }

                None
            }
            Token::Content { value, line: _ } => {
                if let Some(section) = self.section.as_mut() {
                    section.push(Some(value));
                }

                None
            }
        }
    }

    fn flush(&mut self) -> Option<Section> {
        let mut section = core::mem::take(&mut self.section)?;

        // trim any trailing lines
        loop {
            let Some(line) = section.lines.last() else {
                break;
            };

            if !is_empty(line) {
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

fn is_empty(v: &Option<Slice>) -> bool {
    v.as_ref().is_none_or(|v| v.trim().is_empty())
}
