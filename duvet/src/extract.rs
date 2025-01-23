// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

#![allow(clippy::trivial_regex)]

use crate::{
    annotation::AnnotationLevel,
    project::Project,
    specification::{Format, Line, Section, Specification},
    target::{self, Target, TargetPath},
    text::whitespace,
    Result,
};
use clap::Parser;
use core::fmt;
use duvet_core::{diagnostic::IntoDiagnostic, error, path::Path, progress};
use lazy_static::lazy_static;
use regex::{Regex, RegexSet};
use std::{
    collections::{hash_map::Entry, HashMap},
    fs::OpenOptions,
    io::BufWriter,
    sync::Arc,
};

#[cfg(test)]
mod tests;

lazy_static! {
    static ref KEY_WORDS: Vec<(Regex, AnnotationLevel)> = {
        let matches = [
            ("MUST( NOT)?", AnnotationLevel::Must),
            ("SHALL( NOT)?", AnnotationLevel::Must),
            ("REQUIRED", AnnotationLevel::Must),
            ("SHOULD( NOT)?", AnnotationLevel::Should),
            ("(NOT )?RECOMMENDED", AnnotationLevel::Should),
            ("MAY", AnnotationLevel::May),
            ("OPTIONAL", AnnotationLevel::May),
        ];

        matches
            .iter()
            .cloned()
            .map(|(pat, l)| {
                let r = Regex::new(&format!("\\b{}\\b\"?", pat)).into_diagnostic()?;
                Ok((r, l))
            })
            .collect::<Result<_>>()
            .unwrap()
    };
    static ref KEY_WORDS_SET: RegexSet =
        RegexSet::new(KEY_WORDS.iter().map(|(r, _)| r.as_str())).unwrap();
}

#[derive(Debug, Parser)]
pub struct Extract {
    #[clap(short, long, default_value = "IETF")]
    format: Format,

    #[clap(short, long, default_value = "toml")]
    extension: String,

    #[clap(short, long, default_value = ".")]
    out: Path,

    #[clap(flatten)]
    project: Project,

    target_path: TargetPath,
}

impl Extract {
    pub async fn exec(&self) -> Result {
        let download_path = self.project.download_path().await?;

        let target = Arc::new(Target {
            format: self.format,
            path: self.target_path.clone(),
        });

        Extraction {
            download_path: &download_path,
            base_path: None,
            target,
            out: &self.out,
            extension: &self.extension,
            log: true,
        }
        .exec()
        .await
    }
}

pub struct Extraction<'a> {
    pub download_path: &'a Path,
    pub base_path: Option<&'a Path>,
    pub target: Arc<Target>,
    pub out: &'a Path,
    pub extension: &'a str,
    pub log: bool,
}

impl Extraction<'_> {
    pub async fn exec(self) -> Result {
        if self.out.extension().is_some() {
            // assume a path with an extension is a single file
            // TODO output to single file?
            return Err(error!(
                "single file extraction not supported, got {}",
                self.out.display()
            ));
        }

        let download_path = self.download_path;
        let local_path = self.target.path.local(download_path);

        // The specification may be stored alongside the extracted TOML.
        let out = match local_path.strip_prefix(self.out) {
            Ok(path) => self.out.join(path),
            Err(_e) => {
                let local_path = self
                    .base_path
                    .and_then(|base| local_path.strip_prefix(base).ok())
                    .unwrap_or(&local_path);
                self.out.join(local_path)
            }
        };

        let progress = if self.log {
            Some(progress!(
                "Extracting requirements from {}",
                self.target.path
            ))
        } else {
            None
        };

        let spec = target::to_specification(self.target.clone(), download_path.clone()).await?;
        let sections = extract_sections(&spec);

        // output to directory

        let mut total = 0;

        for (section, features) in sections.iter() {
            total += features.len();

            let mut out = out.to_path_buf();

            out.set_extension("");
            let _ = std::fs::create_dir_all(&out);
            out.push(format!("{}.{}", section.id, self.extension));

            let file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(out)?;
            let mut file = BufWriter::new(file);

            let target = &self.target.path;

            match self.extension {
                "rs" => write_rust(&mut file, target, section, features)?,
                "toml" => write_toml(&mut file, target, section, features)?,
                ext => return Err(error!("unsupported extraction type: {ext:?}")),
            }
        }

        if let Some(progress) = progress {
            progress!(
                progress,
                "Extracted {total} requirements across {} sections",
                sections.len()
            );
        }

        Ok(())
    }
}

fn extract_sections(spec: &Specification) -> Vec<(&Section, Vec<Feature>)> {
    spec.sorted_sections()
        .iter()
        .map(|section| extract_section(section))
        .filter(|(_section, features)| !features.is_empty())
        .collect()
}

fn extract_section(section: &Section) -> (&Section, Vec<Feature>) {
    // use a hashmap to deduplicate quotes
    let mut quotes = HashMap::<_, Feature>::new();
    let lines = &section.lines[..];

    for (lineno, line) in lines.iter().enumerate() {
        let line = if let Line::Str(l) = line {
            l
        } else {
            continue;
        };

        if !KEY_WORDS_SET.is_match(line) {
            continue;
        }

        for (key_word, level) in KEY_WORDS.iter() {
            for occurrence in key_word.find_iter(line) {
                // filter out any matches in quotes - these are definitions in the
                // document
                if occurrence.as_str().ends_with('"') {
                    continue;
                }

                let start = find_open(lines, lineno, occurrence.start());
                let end = find_close(lines, lineno, occurrence.end());
                let quote = quote_from_range(lines, start, end);

                let feature = Feature {
                    line_col: start,
                    level: *level,
                    quote,
                };

                // TODO split compound features by level

                let normalized = whitespace::normalize(&feature.quote.join("\n"));
                match quotes.entry(normalized) {
                    Entry::Occupied(mut entry) => {
                        let level = entry.get().level.max(*level);
                        entry.get_mut().level = level;
                    }
                    Entry::Vacant(entry) => {
                        entry.insert(feature);
                    }
                }
            }
        }
    }

    // dedup and sort features
    let mut features: Vec<_> = quotes.into_values().collect();

    features.sort();

    (section, features)
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Feature<'a> {
    line_col: (usize, usize),
    level: AnnotationLevel,
    quote: Vec<&'a str>,
}

impl fmt::Debug for Feature<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // don't show line_col, since it's just for sorting
        f.debug_struct("Feature")
            .field("level", &self.level)
            .field("quote", &self.quote)
            .finish()
    }
}

fn quote_from_range(lines: &[Line], start: (usize, usize), end: (usize, usize)) -> Vec<&str> {
    let mut quote = vec![];

    #[allow(clippy::needless_range_loop)]
    for i in start.0..=end.0 {
        // The requirement didn't end with a period so stop processing
        if i == lines.len() {
            break;
        }

        match &lines[i] {
            Line::Break => {
                continue;
            }
            Line::Str(line) => {
                let mut line = &line[..];

                if i == end.0 {
                    line = &line[..end.1];
                }

                if i == start.0 {
                    line = &line[start.1..];
                }

                line = line.trim();

                if !line.is_empty() {
                    quote.push(line);
                }
            }
        }
    }

    quote
}

fn find_open(lines: &[Line], lineno: usize, start: usize) -> (usize, usize) {
    let line = &lines[lineno];

    if let Line::Str(line) = line {
        if let Some(offset) = find_open_line(&line[..start]) {
            return (lineno, offset);
        }
    }

    let before = &lines[..lineno];

    if !before.is_empty() {
        return find_next_open(before);
    }

    (lineno, 0)
}

fn find_next_open(lines: &[Line]) -> (usize, usize) {
    let mut open = (lines.len() - 1, 0);

    for (lineno, line) in lines.iter().enumerate().rev() {
        let line = match line {
            Line::Str(l) => l,
            Line::Break => return open,
        };

        // if the line is empty we're at the beginning sentence
        if line.is_empty() {
            return open;
        }

        if let Some(end) = find_open_line(line) {
            return (lineno, end);
        }

        open = (lineno, 0);
    }

    open
}

fn find_open_line(line: &str) -> Option<usize> {
    let end = line.rfind('.')? + 1;

    match line[(end)..].chars().next() {
        Some(' ') | Some('\t') => Some(end),
        None => Some(end),
        _ => find_close_line(&line[..(end - 1)]),
    }
}

fn find_close(lines: &[Line], lineno: usize, end: usize) -> (usize, usize) {
    let line = &lines[lineno];

    if let Line::Str(line) = line {
        if let Some(offset) = find_close_line(&line[end..]) {
            return (lineno, end + offset);
        }
    }

    let after = &lines[lineno..];

    if !after.is_empty() {
        let (mut end_line, end_offset) = find_next_close(&after[1..]);
        end_line += lineno + 1;

        return (end_line, end_offset);
    }

    (lineno, end)
}

fn find_next_close(lines: &[Line]) -> (usize, usize) {
    let mut end = (0, 0);

    for (lineno, line) in lines.iter().enumerate() {
        let line = match line {
            Line::Str(l) => l,
            Line::Break => return (lineno, 0),
        };

        // if the line is empty we're finished with the sentence
        if line.is_empty() {
            return (lineno, 0);
        }

        if let Some(end) = find_close_line(line) {
            return (lineno, end);
        }

        end = (lineno, line.len());
    }

    end
}

fn find_close_line(line: &str) -> Option<usize> {
    let end = line.find('.')? + 1;
    let line = &line[end..];

    match line.chars().next() {
        Some(' ') => Some(end),
        Some('\t') => Some(end),
        None => Some(end),
        _ => {
            let end = end + 1 + find_close_line(&line[1..])?;
            Some(end)
        }
    }
}

fn write_rust<W: std::io::Write>(
    w: &mut W,
    target: &TargetPath,
    section: &Section,
    features: &[Feature],
) -> Result {
    writeln!(w, "//! {}#{}", target, section.id)?;
    writeln!(w, "//!")?;
    writeln!(w, "//! {}", section.full_title)?;
    writeln!(w, "//!")?;
    for line in &section.lines {
        if let Line::Str(line) = line {
            writeln!(w, "//! {}", line)?;
        }
    }
    writeln!(w)?;

    for feature in features {
        writeln!(w, "//= {}#{}", target, section.id)?;
        writeln!(w, "//= type=spec")?;
        writeln!(w, "//= level={}", feature.level)?;
        for line in feature.quote.iter() {
            writeln!(w, "//# {}", line)?;
        }
        writeln!(w)?;
    }

    Ok(())
}

fn write_toml<W: std::io::Write>(
    w: &mut W,
    target: &TargetPath,
    section: &Section,
    features: &[Feature],
) -> Result {
    writeln!(w, "target = \"{}#{}\"", target, section.id)?;
    writeln!(w)?;
    writeln!(w, "# {}", section.full_title)?;
    writeln!(w, "#")?;
    for line in &section.lines {
        if let Line::Str(line) = line {
            writeln!(w, "# {}", line)?;
        }
    }
    writeln!(w)?;

    for feature in features {
        writeln!(w, "[[spec]]")?;
        writeln!(w, "level = \"{}\"", feature.level)?;
        writeln!(w, "quote = '''")?;
        for line in feature.quote.iter() {
            writeln!(w, "{}", line)?;
        }
        writeln!(w, "'''")?;
        writeln!(w)?;
    }

    Ok(())
}
