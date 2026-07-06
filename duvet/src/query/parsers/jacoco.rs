// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use quick_xml::{
    events::{BytesStart, Event},
    Reader,
};
use rustc_hash::FxHashMap;
use std::{
    collections::BTreeMap,
    io::{BufRead, Cursor},
    path::Path,
};

use super::super::coverage::{
    CoverageData, CoverageError, CoverageParser, FileCoverage, GenericCoverageData,
};
use crate::Result;

/// JaCoCo XML coverage parser
pub struct JacocoParser;

impl CoverageParser for JacocoParser {
    async fn parse(&self, file_path: &Path) -> Result<CoverageData> {
        // Use duvet's VFS system for consistent async file reading
        let source_file = duvet_core::vfs::read_string(file_path).await?;
        let file_contents = source_file.to_string();

        // Run CPU-intensive XML parsing in a thread pool to avoid blocking the async runtime
        let coverage_data = tokio::task::spawn_blocking(move || {
            let cursor = Cursor::new(file_contents);
            parse_jacoco_xml_report(cursor)
        })
        .await
        .map_err(|e| duvet_core::error!("Task join error: {}", e))??;

        Ok(CoverageData::Generic(coverage_data))
    }
}

/// Parse JaCoCo XML report from a buffered reader
pub fn parse_jacoco_xml_report<T: BufRead>(
    xml_reader: T,
) -> Result<GenericCoverageData, CoverageError> {
    let mut parser = Reader::from_reader(xml_reader);
    let config = parser.config_mut();
    config.expand_empty_elements = true;
    config.trim_text(false);

    let mut coverage_data = GenericCoverageData::new();
    let mut buf = Vec::new();

    loop {
        match parser.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) if e.local_name().into_inner() == b"package" => {
                let package = get_xml_attribute(&parser, e, "name")?;
                let package_results = parse_jacoco_report_package(&mut parser, &mut buf, &package)?;

                // Merge package results into coverage data
                for (file_path, file_coverage) in package_results {
                    coverage_data.files.insert(file_path, file_coverage);
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(CoverageError::Xml(e)),
            _ => {}
        }
        buf.clear();
    }

    Ok(coverage_data)
}

fn parse_jacoco_report_package<T: BufRead>(
    parser: &mut Reader<T>,
    buf: &mut Vec<u8>,
    package: &str,
) -> Result<Vec<(String, FileCoverage)>, CoverageError> {
    let mut results_map: FxHashMap<String, FileCoverage> = FxHashMap::default();

    loop {
        match parser.read_event_into(buf) {
            Ok(Event::Start(ref e)) => {
                match e.local_name().into_inner() {
                    b"class" => {
                        let fq_class = get_xml_attribute(parser, e, "name")?;
                        // Class name: "Person$Age"
                        let class = fq_class.split('/').next_back().ok_or_else(|| {
                            CoverageError::InvalidData("Failed to parse class name".to_string())
                        })?;
                        // Class name "Person"
                        let top_class = class.split('$').next().ok_or_else(|| {
                            CoverageError::InvalidData("Failed to parse top class name".to_string())
                        })?;
                        // Fully qualified class name: "org/example/Person$Age"
                        // Generally, we will use the filename if its present,
                        // but if it isn't, fallback to the top level class name
                        let file = get_xml_attribute(parser, e, "sourcefilename")
                            .unwrap_or_else(|_| format!("{top_class}.java"));

                        // Process all <method /> and <counter /> for this class
                        let functions = parse_jacoco_report_class(parser, buf, class)?;

                        match results_map.get_mut(&file) {
                            Some(file_coverage) => {
                                file_coverage.functions.extend(functions);
                            }
                            None => {
                                results_map.insert(
                                    file.clone(),
                                    FileCoverage {
                                        functions,
                                        lines: BTreeMap::new(),
                                        branches: BTreeMap::new(),
                                    },
                                );
                            }
                        }
                    }
                    b"sourcefile" => {
                        let file = get_xml_attribute(parser, e, "name")?;
                        let source_file_data = parse_jacoco_report_sourcefile(parser, buf)?;

                        match results_map.get_mut(&file) {
                            Some(file_coverage) => {
                                file_coverage.lines = source_file_data.lines;
                                file_coverage.branches = source_file_data.branches;
                            }
                            None => {
                                results_map.insert(
                                    file.clone(),
                                    FileCoverage {
                                        functions: FxHashMap::default(),
                                        lines: source_file_data.lines,
                                        branches: source_file_data.branches,
                                    },
                                );
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::End(ref e)) if e.local_name().into_inner() == b"package" => break,
            Err(e) => return Err(CoverageError::Xml(e)),
            _ => {}
        }
        buf.clear();
    }

    // Change all keys from the class name to the file name and turn the result into a Vec.
    // If package is the empty string, we have to trim the leading '/' in order to obtain a
    // relative path.
    Ok(results_map
        .into_iter()
        .map(|(class, result)| {
            (
                format!("{package}/{class}")
                    .trim_start_matches('/')
                    .to_string(),
                result,
            )
        })
        .collect())
}

fn parse_jacoco_report_class<T: BufRead>(
    parser: &mut Reader<T>,
    buf: &mut Vec<u8>,
    class_name: &str,
) -> Result<FxHashMap<String, String>, CoverageError> {
    let mut functions: FxHashMap<String, String> = FxHashMap::default();

    loop {
        match parser.read_event_into(buf) {
            Ok(Event::Start(ref e)) if e.local_name().into_inner() == b"method" => {
                let name = get_xml_attribute(parser, e, "name")?;
                let full_name = format!("{class_name}#{name}");

                let start_line = get_xml_attribute(parser, e, "line")?
                    .parse::<u32>()
                    .map_err(|_| CoverageError::InvalidData("Invalid line number".to_string()))?;
                let function_info = parse_jacoco_report_method(parser, buf, start_line)?;
                functions.insert(full_name, function_info);
            }
            Ok(Event::End(ref e)) if e.local_name().into_inner() == b"class" => break,
            Err(e) => return Err(CoverageError::Xml(e)),
            _ => {}
        }
        buf.clear();
    }

    Ok(functions)
}

fn parse_jacoco_report_method<T: BufRead>(
    parser: &mut Reader<T>,
    buf: &mut Vec<u8>,
    start: u32,
) -> Result<String, CoverageError> {
    loop {
        match parser.read_event_into(buf) {
            Ok(Event::End(ref e)) if e.local_name().into_inner() == b"method" => break,
            Err(e) => return Err(CoverageError::Xml(e)),
            _ => {}
        }
        buf.clear();
    }

    Ok(format!("method_at_line_{start}"))
}

struct JacocoSourceFileData {
    lines: BTreeMap<u32, u64>,
    branches: BTreeMap<u32, Vec<bool>>,
}

fn parse_jacoco_report_sourcefile<T: BufRead>(
    parser: &mut Reader<T>,
    buf: &mut Vec<u8>,
) -> Result<JacocoSourceFileData, CoverageError> {
    let mut lines: BTreeMap<u32, u64> = BTreeMap::new();
    let mut branches: BTreeMap<u32, Vec<bool>> = BTreeMap::new();

    loop {
        match parser.read_event_into(buf) {
            Ok(Event::Start(ref e)) if e.local_name().into_inner() == b"line" => {
                let (mut ci, mut mi, mut cb, mut mb, mut nr) = (None, None, None, None, None);
                for a in e.attributes() {
                    let a = a.map_err(|e| CoverageError::Xml(e.into()))?;
                    match a.key.into_inner() {
                        b"ci" => {
                            ci = Some(String::from_utf8_lossy(&a.value).parse::<u64>().map_err(
                                |_| CoverageError::InvalidData("Invalid ci value".to_string()),
                            )?)
                        }
                        b"mi" => {
                            mi = Some(String::from_utf8_lossy(&a.value).parse::<u64>().map_err(
                                |_| CoverageError::InvalidData("Invalid mi value".to_string()),
                            )?)
                        }
                        b"cb" => {
                            cb = Some(String::from_utf8_lossy(&a.value).parse::<u64>().map_err(
                                |_| CoverageError::InvalidData("Invalid cb value".to_string()),
                            )?)
                        }
                        b"mb" => {
                            mb = Some(String::from_utf8_lossy(&a.value).parse::<u64>().map_err(
                                |_| CoverageError::InvalidData("Invalid mb value".to_string()),
                            )?)
                        }
                        b"nr" => {
                            nr = Some(String::from_utf8_lossy(&a.value).parse::<u32>().map_err(
                                |_| CoverageError::InvalidData("Invalid nr value".to_string()),
                            )?)
                        }
                        _ => {}
                    }
                }

                let ci = ci.ok_or_else(|| {
                    CoverageError::InvalidData("Missing ci attribute".to_string())
                })?;
                let mi = mi.ok_or_else(|| {
                    CoverageError::InvalidData("Missing mi attribute".to_string())
                })?;
                let cb = cb.ok_or_else(|| {
                    CoverageError::InvalidData("Missing cb attribute".to_string())
                })?;
                let mb = mb.ok_or_else(|| {
                    CoverageError::InvalidData("Missing mb attribute".to_string())
                })?;
                let nr = nr.ok_or_else(|| {
                    CoverageError::InvalidData("Missing nr attribute".to_string())
                })?;

                if mb > 0 || cb > 0 {
                    // This line is a branch.
                    let mut v = vec![true; cb as usize];
                    v.extend(vec![false; mb as usize]);
                    branches.insert(nr, v);
                } else if ci > 0 || mi > 0 {
                    // This line is a statement with executable bytecode.
                    // Per JaCoCo's report.dtd, `ci`/`mi` count covered/missed
                    // *instructions*; a line is executed when at least one of its
                    // instructions ran. JaCoCo does not report execution counts,
                    // so we collapse to 0 or 1.
                    let hit = if ci > 0 { 1 } else { 0 };
                    lines.insert(nr, hit);
                }
                // else: ci == 0 && mi == 0 -> the line has no bytecode
                // instructions at all (blank line, comment, or a pure
                // declaration). It is not executable code, so we leave it
                // *absent* from the map rather than recording it as a Miss.
                // The verified model distinguishes "absent/unknown" from "Miss",
                // and that distinction drives Structural vs. NotExecuted; marking
                // a non-executable line as Miss would manufacture a false
                // "not executed" verdict for an annotation resolving to it.
            }
            Ok(Event::End(ref e)) if e.local_name().into_inner() == b"sourcefile" => {
                break;
            }
            Err(e) => return Err(CoverageError::Xml(e)),
            _ => {}
        }
        buf.clear();
    }

    Ok(JacocoSourceFileData { lines, branches })
}

fn get_xml_attribute<R: BufRead>(
    reader: &Reader<R>,
    event: &BytesStart<'_>,
    name: &str,
) -> Result<String, CoverageError> {
    for a in event.attributes() {
        let a = a.map_err(|e| CoverageError::InvalidData(format!("Attribute error: {e}")))?;
        if a.key.into_inner() == name.as_bytes() {
            return Ok(a
                .decode_and_unescape_value(reader.decoder())
                .map_err(CoverageError::Xml)?
                .into_owned());
        }
    }
    Err(CoverageError::InvalidData(format!(
        "Attribute {name} not found"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_parse_basic_jacoco_xml() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<!DOCTYPE report PUBLIC "-//JACOCO//DTD Report 1.1//EN" "report.dtd">
<report name="test">
    <package name="com/example">
        <class name="com/example/Hello" sourcefilename="Hello.java">
            <method name="&lt;init&gt;" desc="()V" line="1">
                <counter type="INSTRUCTION" missed="0" covered="3"/>
                <counter type="LINE" missed="0" covered="1"/>
                <counter type="COMPLEXITY" missed="0" covered="1"/>
                <counter type="METHOD" missed="0" covered="1"/>
            </method>
        </class>
        <sourcefile name="Hello.java">
            <line nr="1" mi="0" ci="3" mb="0" cb="0"/>
            <line nr="4" mi="0" ci="2" mb="0" cb="0"/>
        </sourcefile>
    </package>
</report>"#;

        let cursor = Cursor::new(xml);
        let result = parse_jacoco_xml_report(cursor).unwrap();

        assert_eq!(result.files.len(), 1);
        let file_coverage = result.files.get("com/example/Hello.java").unwrap();

        // Check lines
        assert_eq!(file_coverage.lines.get(&1), Some(&1));
        assert_eq!(file_coverage.lines.get(&4), Some(&1));

        // Check functions
        assert_eq!(file_coverage.functions.len(), 1);
        let function_info = file_coverage.functions.get("Hello#<init>").unwrap();
        assert_eq!(function_info, "method_at_line_1");
    }

    /// idx35: `ci`/`mi` decide statement status per JaCoCo's report.dtd
    /// (covered/missed *instructions*). The three cases:
    ///   - ci>0            -> executed        -> hit=1
    ///   - ci=0 && mi>0    -> uncovered code  -> hit=0 (Miss)
    ///   - ci=0 && mi=0    -> no bytecode      -> line absent (not executable)
    #[test]
    fn statement_status_from_ci_and_mi() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<report name="test">
    <package name="com/example">
        <sourcefile name="Foo.java">
            <line nr="10" mi="0" ci="4" mb="0" cb="0"/>
            <line nr="11" mi="3" ci="0" mb="0" cb="0"/>
            <line nr="12" mi="0" ci="0" mb="0" cb="0"/>
        </sourcefile>
    </package>
</report>"#;

        let result = parse_jacoco_xml_report(Cursor::new(xml)).unwrap();
        let file_coverage = result.files.get("com/example/Foo.java").unwrap();

        // ci>0 -> executed statement.
        assert_eq!(file_coverage.lines.get(&10), Some(&1));
        // ci=0, mi>0 -> uncovered statement (Miss).
        assert_eq!(file_coverage.lines.get(&11), Some(&0));
        // ci=0, mi=0 -> no bytecode: absent from the map, NOT recorded as Miss.
        assert_eq!(file_coverage.lines.get(&12), None);
        assert!(!file_coverage.branches.contains_key(&12));
    }

    /// idx35: a non-executable line (ci=0, mi=0) must not surface as a `Miss`
    /// in the verified-model report — it should be absent entirely.
    #[test]
    fn non_executable_line_is_absent_from_coverage_report() {
        use duvet_coverage::types::CoverageStatus;

        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<report name="test">
    <package name="com/example">
        <sourcefile name="Foo.java">
            <line nr="10" mi="0" ci="4" mb="0" cb="0"/>
            <line nr="12" mi="0" ci="0" mb="0" cb="0"/>
        </sourcefile>
    </package>
</report>"#;

        let result = parse_jacoco_xml_report(Cursor::new(xml)).unwrap();
        let report = result
            .files
            .get("com/example/Foo.java")
            .unwrap()
            .to_coverage_report();

        assert_eq!(report.get(&10), Some(&CoverageStatus::Hit));
        assert_eq!(report.get(&12), None);
    }

    /// idx36: branch lines (mb>0 || cb>0) build the `branches` map, and
    /// `to_coverage_report` collapses them with a branch-OR: any taken branch
    /// makes the line a Hit, otherwise Miss.
    #[test]
    fn branch_lines_parse_and_collapse_with_or() {
        use duvet_coverage::types::CoverageStatus;

        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<report name="test">
    <package name="com/example">
        <sourcefile name="Branchy.java">
            <line nr="5" mi="0" ci="2" mb="1" cb="1"/>
            <line nr="6" mi="0" ci="2" mb="2" cb="0"/>
        </sourcefile>
    </package>
</report>"#;

        let result = parse_jacoco_xml_report(Cursor::new(xml)).unwrap();
        let file_coverage = result.files.get("com/example/Branchy.java").unwrap();

        // Branch lines land in `branches`, not `lines`.
        assert!(file_coverage.lines.is_empty());
        // cb=1 -> one taken; mb=1 -> one not taken.
        assert_eq!(file_coverage.branches.get(&5), Some(&vec![true, false]));
        // cb=0, mb=2 -> both not taken.
        assert_eq!(file_coverage.branches.get(&6), Some(&vec![false, false]));

        let report = file_coverage.to_coverage_report();
        // At least one branch taken -> Hit.
        assert_eq!(report.get(&5), Some(&CoverageStatus::Hit));
        // No branch taken -> Miss.
        assert_eq!(report.get(&6), Some(&CoverageStatus::Miss));
    }

    /// idx25 (belt-and-suspenders): `to_coverage_report` is Hit-priority, so a
    /// `Miss` never overwrites a `Hit` for the same line even if the two source
    /// maps were to overlap.
    #[test]
    fn coverage_report_merge_is_hit_priority() {
        use crate::query::coverage::FileCoverage;
        use duvet_coverage::types::CoverageStatus;

        // Construct an (artificial) overlap the parser wouldn't currently emit:
        // line 7 is a missed statement AND a taken branch.
        let mut lines = BTreeMap::new();
        lines.insert(7u32, 0u64); // Miss
        let mut branches = BTreeMap::new();
        branches.insert(7u32, vec![true]); // Hit

        let fc = FileCoverage {
            lines,
            branches,
            functions: FxHashMap::default(),
        };

        // Regardless of insertion order, Hit wins.
        assert_eq!(fc.to_coverage_report().get(&7), Some(&CoverageStatus::Hit));
    }
}
