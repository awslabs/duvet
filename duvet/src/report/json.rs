// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use super::{Reference, ReportResult, TargetReport};
use crate::{
    annotation::{AnnotationLevel, AnnotationType},
    specification::Line,
    Error, Result,
};
use duvet_core::{file::Slice, path::Path};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs::File,
    io::{BufWriter, Cursor, Write},
};

macro_rules! writer {
    ($writer:ident) => {
        macro_rules! w {
            ($arg: expr) => {
                write!($writer, "{}", $arg)?
            };
        }
    };
}

macro_rules! kv {
    ($comma:ident, $k:stmt, $v:stmt) => {{
        w!($comma.comma());
        $k
        w!(":");
        $v
    }};
}

macro_rules! su {
    ($v:expr) => {
        w!(format_args!(r#""{}""#, $v))
    };
}
macro_rules! s {
    ($v:expr) => {
        su!(v_jsonescape::escape($v.as_ref()))
    };
}

macro_rules! comma {
    () => {
        Comma::default()
    };
}

macro_rules! obj {
    (| $comma:ident | $s:stmt) => {{
        w!("{");
        let mut $comma = comma!();

        $s

        w!("}");
    }};
}

macro_rules! arr {
    (| $comma:ident | $s:stmt) => {{
        w!("[");
        let mut $comma = comma!();

        $s

        w!("]");
    }};
}

macro_rules! item {
    ($comma:ident, $v:stmt) => {{
        w!($comma.comma());
        $v
    }};
}

pub fn report(report: &ReportResult, file: &Path) -> Result {
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut file = BufWriter::new(File::create(file)?);

    report_writer(report, &mut file)
}

pub fn report_writer<Output: Write>(report: &ReportResult, output: &mut Output) -> Result {
    let mut specs = BTreeMap::new();
    for (source, report) in report.targets.iter() {
        let id = format!("{}", &source.path);
        let mut output = Cursor::new(vec![]);
        report_source(report, &mut output)?;
        let output = unsafe { String::from_utf8_unchecked(output.into_inner()) };
        specs.insert(id, output);
    }

    writer!(output);

    obj!(|obj| {
        if let Some(link) = report.blob_link {
            kv!(obj, s!("blob_link"), s!(link));
        }
        if let Some(link) = report.issue_link {
            kv!(obj, s!("issue_link"), s!(link));
        }

        kv!(
            obj,
            s!("specifications"),
            obj!(|obj| {
                for (id, spec) in &specs {
                    // don't escape `spec` since it's already been serialized to json
                    kv!(obj, s!(id), w!(spec));
                }
            })
        );

        kv!(
            obj,
            s!("annotations"),
            arr!(|arr| {
                for annotation in report.annotations.iter() {
                    item!(
                        arr,
                        obj!(|obj| {
                            kv!(obj, s!("source"), s!(annotation.source.to_string_lossy()));
                            kv!(obj, s!("target_path"), s!(annotation.resolve_target_path()));

                            if let Some(section) = annotation.target_section() {
                                kv!(obj, s!("target_section"), s!(section));
                            }

                            if annotation.anno_line > 0 {
                                kv!(obj, s!("line"), w!(annotation.anno_line));
                            }

                            if annotation.anno != AnnotationType::Citation {
                                kv!(obj, s!("type"), su!(annotation.anno));
                            }

                            if annotation.level != AnnotationLevel::Auto {
                                kv!(obj, s!("level"), su!(annotation.level));
                            }

                            if !annotation.comment.is_empty() {
                                kv!(obj, s!("comment"), s!(annotation.comment));
                            }

                            if !annotation.feature.is_empty() {
                                kv!(obj, s!("feature"), s!(annotation.feature));
                            }

                            if !annotation.tracking_issue.is_empty() {
                                kv!(obj, s!("tracking_issue"), s!(annotation.tracking_issue));
                            }

                            if !annotation.tags.is_empty() {
                                kv!(
                                    obj,
                                    s!("tags"),
                                    arr!(|arr| {
                                        for tag in &annotation.tags {
                                            item!(arr, s!(tag));
                                        }
                                    })
                                )
                            }
                        })
                    );
                }
            })
        );

        kv!(
            obj,
            s!("statuses"),
            obj!(|obj| {
                for target in report.targets.values() {
                    for (anno_id, status) in target.statuses.iter() {
                        kv!(
                            obj,
                            su!(anno_id),
                            obj!(|obj| {
                                macro_rules! status {
                                    ($field:ident) => {
                                        if status.$field > 0 {
                                            kv!(obj, su!(stringify!($field)), w!(status.$field));
                                        }
                                    };
                                }
                                status!(spec);
                                status!(incomplete);
                                status!(citation);
                                status!(implication);
                                status!(test);
                                status!(exception);
                                status!(todo);

                                if !status.related.is_empty() {
                                    kv!(
                                        obj,
                                        su!("related"),
                                        arr!(|arr| {
                                            for id in &status.related {
                                                item!(arr, w!(id));
                                            }
                                        })
                                    );
                                }
                            })
                        );
                    }
                }
            })
        );

        kv!(
            obj,
            s!("refs"),
            arr!(|arr| {
                RefStatus::for_each::<_, Error>(|s| {
                    item!(
                        arr,
                        obj!(|obj| {
                            macro_rules! status {
                                ($field:ident) => {
                                    if s.$field {
                                        kv!(obj, su!(stringify!($field)), w!("true"));
                                    }
                                };
                            }

                            status!(spec);
                            status!(citation);
                            status!(implication);
                            status!(test);
                            status!(exception);
                            status!(todo);

                            if s.level != AnnotationLevel::Auto {
                                kv!(obj, su!("level"), su!(s.level));
                            }
                        })
                    );

                    Ok(())
                })?
            })
        );
    });

    Ok(())
}

pub fn report_source<Output: Write>(report: &TargetReport, output: &mut Output) -> Result {
    writer!(output);

    let mut references: HashMap<_, Vec<&Reference>> = HashMap::new();
    let mut requirements = BTreeSet::new();
    for reference in &report.references {
        if reference.annotation.anno == AnnotationType::Spec {
            requirements.insert(reference.annotation.id);
        }
        for line in reference.text.line_range() {
            references.entry(line).or_default().push(reference);
        }
    }

    obj!(|obj| {
        if let Some(title) = &report.specification.title {
            kv!(obj, s!("title"), s!(title));
        }

        kv!(
            obj,
            s!("format"),
            s!(report.specification.format.to_string())
        );

        kv!(
            obj,
            s!("requirements"),
            arr!(|arr| {
                for requirement in requirements.iter() {
                    item!(arr, w!(requirement));
                }
                requirements.clear();
            })
        );

        kv!(
            obj,
            s!("sections"),
            arr!(|arr| {
                for section in report.specification.sorted_sections() {
                    item!(
                        arr,
                        obj!(|obj| {
                            kv!(obj, s!("id"), s!(section.id));
                            kv!(obj, s!("title"), s!(section.title));

                            kv!(
                                obj,
                                s!("lines"),
                                arr!(|arr| {
                                    for line in &section.lines {
                                        if let Line::Str(line) = line {
                                            for lineno in line.line_range() {
                                                item!(
                                                    arr,
                                                    if let Some(refs) = references.get(&lineno) {
                                                        report_references(
                                                            line,
                                                            refs,
                                                            &mut requirements,
                                                            output,
                                                        )?;
                                                    } else {
                                                        // the line has no annotations so just print it
                                                        s!(line);
                                                    }
                                                )
                                            }
                                        }
                                    }
                                })
                            );

                            if !requirements.is_empty() {
                                kv!(
                                    obj,
                                    s!("requirements"),
                                    arr!(|arr| {
                                        for requirement in requirements.iter() {
                                            item!(arr, w!(requirement));
                                        }
                                        requirements.clear();
                                    })
                                );
                            }
                        })
                    );
                }
            })
        );
    });

    Ok(())
}

fn report_references<Output: Write>(
    line: &Slice,
    refs: &[&Reference],
    requirements: &mut BTreeSet<usize>,
    output: &mut Output,
) -> Result {
    writer!(output);

    if line.is_empty() {
        s!("");
        return Ok(());
    }

    assert!(!refs.is_empty());
    arr!(|arr| {
        let line_range = line.range();
        let line_pos = line_range.start;
        let mut start = line_pos;
        let end = line_range.end;

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

            item!(
                arr,
                arr!(|arr| {
                    let mut status = RefStatus::default();

                    // build a list of the referenced annotations
                    item!(
                        arr,
                        arr!(|arr| {
                            for r in current_refs {
                                item!(arr, w!(r.annotation.id));
                                if r.annotation.anno == AnnotationType::Spec {
                                    requirements.insert(r.annotation.id);
                                }
                                status.on_anno(r);
                            }
                        })
                    );

                    // report on the status of this particular set of references
                    item!(arr, w!(status.id()));

                    // output the actual text
                    item!(arr, s!(line[(start - line_pos)..(min_end - line_pos)]));
                })
            );

            start = min_end;
        }
    });

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

#[derive(Clone, Copy, Default, Debug)]
struct RefStatus {
    spec: bool,
    citation: bool,
    implication: bool,
    test: bool,
    exception: bool,
    todo: bool,
    level: AnnotationLevel,
}

impl RefStatus {
    fn for_each<F: FnMut(Self) -> Result<(), E>, E>(mut f: F) -> Result<(), E> {
        for level in AnnotationLevel::LEVELS.iter().copied() {
            for spec in [false, true].iter().copied() {
                for citation in [false, true].iter().copied() {
                    for implication in [false, true].iter().copied() {
                        for test in [false, true].iter().copied() {
                            for exception in [false, true].iter().copied() {
                                for todo in [false, true].iter().copied() {
                                    let status = Self {
                                        spec,
                                        citation,
                                        implication,
                                        test,
                                        exception,
                                        todo,
                                        level,
                                    };
                                    f(status)?;
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub fn id(self) -> usize {
        let mut id = 0;
        let mut mask = 0x1;
        let mut count = 0;

        macro_rules! field {
            ($name:ident) => {
                if self.$name {
                    id |= mask;
                }
                mask <<= 1;
                count += 1;
            };
        }

        // Order is important
        field!(todo);
        field!(exception);
        field!(test);
        field!(implication);
        field!(citation);
        field!(spec);

        let _ = mask;

        let level = AnnotationLevel::LEVELS
            .iter()
            .copied()
            .position(|l| l == self.level)
            .unwrap();

        id += level * 2usize.pow(count);

        id
    }

    fn on_anno(&mut self, r: &Reference) {
        self.level = self.level.max(r.annotation.level);
        match r.annotation.anno {
            AnnotationType::Spec => self.spec = true,
            AnnotationType::Citation => self.citation = true,
            AnnotationType::Implication => self.implication = true,
            AnnotationType::Test => self.test = true,
            AnnotationType::Exception => self.exception = true,
            AnnotationType::Todo => self.todo = true,
        }
    }
}

impl From<RefStatus> for usize {
    fn from(s: RefStatus) -> Self {
        s.id()
    }
}

#[test]
fn status_id_test() {
    let mut count = 0;
    let _ = RefStatus::for_each::<_, ()>(|s| {
        dbg!((count, s));
        assert_eq!(s.id(), count);
        count += 1;
        Ok(())
    });
}
