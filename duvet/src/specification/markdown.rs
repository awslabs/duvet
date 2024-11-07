// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use super::{Format, Line, Section, Specification, Str};
use crate::Result;
use duvet_core::file::SourceFile;
use std::collections::{hash_map::Entry, HashMap};

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

    let mut spec_title = None;

    let mut sections = HashMap::new();

    for section in parser {
        let title = section.title.to_string().replace('\n', " ");

        // set the document title if it's a H1 and we haven't set it yet
        if spec_title.is_none() && section.level == 1 {
            spec_title = Some(title.clone());
        }

        let section = Section {
            title,
            id: section.id.to_string(),
            full_title: substr(&section.title, section.line),
            lines: section
                .lines
                .into_iter()
                .map(|(line, value)| {
                    if let Some(value) = value {
                        let value = substr(&value, line);
                        Line::Str(value)
                    } else {
                        Line::Break
                    }
                })
                .collect(),
        };

        insert_section(&mut sections, section);
    }

    Ok(Specification {
        title: spec_title,
        sections,
        format: Format::Markdown,
    })
}

/// Inserts the section into the document, appending a unique ID if needed
fn insert_section<'a>(sections: &mut HashMap<String, Section<'a>>, mut section: Section<'a>) {
    let mut counter = 0usize;

    loop {
        let key = if counter > 0 {
            format!("{}-{counter}", section.id)
        } else {
            section.id.clone()
        };

        if let Entry::Vacant(entry) = sections.entry(key) {
            if section.id != *entry.key() {
                section.id = entry.key().clone();
            }
            entry.insert(section);
            break;
        }

        counter += 1;
    }
}
