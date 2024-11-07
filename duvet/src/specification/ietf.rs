// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use super::{Format, Line, Section, Specification, Str};
use crate::Result;
use duvet_core::file::SourceFile;

pub mod break_filter;
pub mod parser;
pub mod tokenizer;

#[cfg(test)]
mod tests;

pub fn parse(contents: &SourceFile) -> Result<Specification> {
    let tokens = tokenizer::tokens(contents);
    let tokens = break_filter::break_filter(tokens);
    let parser = parser::parse(tokens);

    let pos = |substr: &str| substr.as_ptr() as usize - contents.as_ptr() as usize;
    let substr = |substr: &str, line: usize| {
        let pos = pos(substr);
        let value = &contents[pos..pos + substr.len()];
        Str { value, pos, line }
    };

    let sections = parser
        .map(|section| {
            let id = section.id.to_string();

            let section = Section {
                title: section.title.to_string(),
                id: id.clone(),
                full_title: substr(&section.title, section.line),
                lines: section
                    .lines
                    .into_iter()
                    .map(|(line, value)| {
                        let value = substr(&value, line);
                        Line::Str(value)
                    })
                    .collect(),
            };

            (id, section)
        })
        .collect();

    Ok(Specification {
        title: None,
        sections,
        format: Format::Ietf,
    })
}
