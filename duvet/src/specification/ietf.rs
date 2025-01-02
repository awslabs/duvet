// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use super::{Format, Line, Section, Specification};
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

    let sections = parser
        .map(|section| {
            let id = section.id.to_string();

            let section = Section {
                title: section.title.to_string(),
                id: id.clone(),
                full_title: section.title,
                lines: section.lines.into_iter().map(Line::Str).collect(),
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
