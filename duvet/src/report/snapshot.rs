// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use super::{Reference, ReportResult, TargetReport};
use crate::{
    annotation::{AnnotationLevel, AnnotationType},
    specification::Line,
    target::Target,
    Result,
};
use core::fmt;
use duvet_core::{
    console::style,
    diagnostic::{Context as _, IntoDiagnostic},
    error,
    file::Slice,
    path::Path,
};
use std::{
    collections::HashMap,
    fs::File,
    io::{BufWriter, Write},
};

pub fn report(report: &ReportResult, file: &Path) -> Result {
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut file = BufWriter::new(File::create(file)?);

    report_writer(report, &mut file)
}

pub fn report_ci(report: &ReportResult, file: &Path) -> Result {
    let actual = match std::fs::read_to_string(file) {
        Ok(actual) => actual,
        Err(_err) => {
            return Err(error!(
                "Could not read report snapshot. This is required to enforce CI checks."
            ))
            .context(file.clone())
            .map_err(Into::into);
        }
    };

    let mut expected = vec![];
    report_writer(report, &mut expected)?;
    let expected = String::from_utf8(expected).into_diagnostic()?;

    // the two values match so we're ok
    if actual == expected {
        return Ok(());
    }

    eprintln!();
    eprintln!(
        "{}",
        style(format_args!("Differences detected in {file}:"))
            .red()
            .bold()
    );
    eprintln!();

    duvet_core::diff::dump(std::io::stderr(), &actual, &expected)?;

    Err(error!(
        "Report snapshot does not match with CI mode enabled."
    ))
    .context(file.clone())
    .map_err(Into::into)
}

pub fn report_writer<Output: Write>(report: &ReportResult, output: &mut Output) -> Result {
    for (idx, (source, report)) in report.targets.iter().enumerate() {
        if idx > 0 {
            writeln!(output)?;
        }
        report_target(source, report, output)?;
    }

    Ok(())
}

pub fn report_target<Output: Write>(
    target: &Target,
    report: &TargetReport,
    output: &mut Output,
) -> Result {
    let mut references: HashMap<_, Vec<&Reference>> = HashMap::new();
    for reference in &report.references {
        for line in reference.text.line_range() {
            references.entry(line).or_default().push(reference);
        }
    }

    let mut has_emitted_title = false;
    let mut has_emitted_prev_section = false;

    for section in report.specification.sorted_sections() {
        let mut has_emitted_section = false;
        for line in &section.lines {
            if let Line::Str(line) = line {
                for lineno in line.line_range() {
                    let Some(refs) = references.get(&lineno).map(|v| v.as_slice()) else {
                        continue;
                    };

                    if !core::mem::replace(&mut has_emitted_title, true) {
                        if let Some(title) = report.specification.title.as_ref() {
                            writeln!(output, "SPECIFICATION: [{title}]({})", target.path)?;
                        } else {
                            writeln!(output, "SPECIFICATION: {}", target.path)?;
                        }
                    }

                    if !core::mem::replace(&mut has_emitted_section, true) {
                        if core::mem::replace(&mut has_emitted_prev_section, true) {
                            writeln!(output)?;
                        }

                        writeln!(output, "  SECTION: [{}](#{})", section.title, section.id)?;
                    }

                    report_references(line, refs, output)?;
                }
            }
        }
    }

    Ok(())
}

fn report_references<Output: Write>(
    line: &Slice,
    refs: &[&Reference],
    output: &mut Output,
) -> Result {
    if line.is_empty() {
        return Ok(());
    }

    let line_range = line.range();
    let line_pos = line_range.start;
    let mut start = line_pos;
    let end = line_range.end;

    struct Buffer<'a, Output> {
        line: &'a Slice,
        output: Output,
        status: RefStatus,
        start: usize,
        end: usize,
    }

    impl<Output> Buffer<'_, Output>
    where
        Output: Write,
    {
        fn push(&mut self, start: usize, end: usize, status: RefStatus) -> Result {
            if self.status == status {
                self.end = self.end.max(end);
                return Ok(());
            }

            let preferred_start = self.flush()?;

            self.start = preferred_start.unwrap_or(start);
            self.end = end;
            self.status = status;

            Ok(())
        }

        fn flush(&mut self) -> Result<Option<usize>> {
            let start = core::mem::take(&mut self.start);
            let end = core::mem::take(&mut self.end);
            let status = core::mem::take(&mut self.status);

            let len = end - start;
            if len == 0 {
                return Ok(None);
            }

            let text = &self.line[start..end].trim_end();
            if text.is_empty() {
                // if we're at the beginning of the line then we want to buffer the whitespace for the next status
                if start == 0 {
                    return Ok(Some(start));
                }

                return Ok(None);
            }

            // only include text if it has a status
            if !status.is_empty() {
                writeln!(self.output, "    TEXT[{status}]: {text}")?;
            }

            Ok(None)
        }
    }

    let mut buffer = Buffer {
        line,
        output,
        status: RefStatus::default(),
        start: 0,
        end: 0,
    };

    while start < end {
        let mut min_end = end;
        let current_refs = refs.iter().filter(|r| {
            if r.start() <= start {
                if start < r.end() {
                    min_end = min_end.min(r.end());
                    true
                } else {
                    false
                }
            } else {
                min_end = min_end.min(r.start());
                false
            }
        });

        let mut status = RefStatus::default();

        // build a list of the referenced annotations
        for r in current_refs {
            status.on_anno(r);
        }

        buffer.push(start - line_pos, min_end - line_pos, status)?;
        start = min_end;
    }

    buffer.flush()?;

    Ok(())
}

#[derive(Default)]
struct Comma(bool);

impl Comma {
    fn comma(&mut self) -> &'static str {
        if core::mem::replace(&mut self.0, true) {
            ","
        } else {
            ""
        }
    }
}

#[derive(Clone, Copy, Default, PartialEq, Debug)]
#[cfg_attr(test, derive(bolero::TypeGenerator))]
struct RefStatus {
    implementation: bool,
    implication: bool,
    test: bool,
    exception: bool,
    todo: bool,
    level: AnnotationLevel,
}

impl fmt::Display for RefStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut comma = Comma::default();

        if self.level != AnnotationLevel::Auto {
            write!(f, "{}!{}", comma.comma(), self.level)?;
        }

        macro_rules! status {
            ($id:ident) => {
                if self.$id {
                    write!(f, "{}{}", comma.comma(), stringify!($id))?;
                }
            };
        }

        status!(implementation);
        status!(implication);
        status!(test);
        status!(exception);
        status!(todo);

        Ok(())
    }
}

impl RefStatus {
    fn is_empty(&self) -> bool {
        Self::default().eq(self)
    }

    fn on_anno(&mut self, r: &Reference) {
        self.level = self.level.max(r.annotation.level);
        match r.annotation.anno {
            AnnotationType::Spec => {
                // implied by the level
            }
            AnnotationType::Citation => self.implementation = true,
            AnnotationType::Implication => self.implication = true,
            AnnotationType::Test => self.test = true,
            AnnotationType::Exception => self.exception = true,
            AnnotationType::Todo => self.todo = true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bolero::check;

    #[test]
    fn status_test() {
        check!().with_type::<RefStatus>().for_each(|status| {
            let should_be_empty = status.is_empty();
            let output = status.to_string();
            assert!(
                output.is_empty() == should_be_empty,
                "should_be_empty: {should_be_empty}, output: {output:?}"
            );
        });
    }
}
