// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::io::{BufRead, Cursor};
use std::path::Path;
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use rustc_hash::FxHashMap;

use super::super::coverage::{
    CoverageData, CoverageParser, CoverageError, FileCoverage, GenericCoverageData,
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
                        let class = fq_class
                            .split('/')
                            .next_back()
                            .ok_or_else(|| CoverageError::InvalidData("Failed to parse class name".to_string()))?;
                        // Class name "Person"
                        let top_class = class
                            .split('$')
                            .next()
                            .ok_or_else(|| CoverageError::InvalidData("Failed to parse top class name".to_string()))?;
                        // Fully qualified class name: "org/example/Person$Age"
                        // Generally, we will use the filename if its present,
                        // but if it isn't, fallback to the top level class name
                        let file = get_xml_attribute(parser, e, "sourcefilename")
                            .unwrap_or_else(|_| format!("{}.java", top_class));

                        // Process all <method /> and <counter /> for this class
                        let functions = parse_jacoco_report_class(parser, buf, class)?;

                        match results_map.get_mut(&file) {
                            Some(file_coverage) => {
                                file_coverage.functions.extend(functions);
                            }
                            None => {
                                results_map.insert(file.clone(), FileCoverage {
                                    functions,
                                    lines: BTreeMap::new(),
                                    branches: BTreeMap::new(),
                                });
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
                                results_map.insert(file.clone(), FileCoverage {
                                    functions: FxHashMap::default(),
                                    lines: source_file_data.lines,
                                    branches: source_file_data.branches,
                                });
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
                format!("{}/{}", package, class)
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
                let full_name = format!("{}#{}", class_name, name);

                let start_line = get_xml_attribute(parser, e, "line")?.parse::<u32>()
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

    Ok(format!("method_at_line_{}", start))
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
                let (mut ci, mut cb, mut mb, mut nr) = (None, None, None, None);
                for a in e.attributes() {
                    let a = a.map_err(|e| CoverageError::Xml(e.into()))?;
                    match a.key.into_inner() {
                        b"ci" => ci = Some(String::from_utf8_lossy(&a.value).parse::<u64>()
                            .map_err(|_| CoverageError::InvalidData("Invalid ci value".to_string()))?),
                        b"cb" => cb = Some(String::from_utf8_lossy(&a.value).parse::<u64>()
                            .map_err(|_| CoverageError::InvalidData("Invalid cb value".to_string()))?),
                        b"mb" => mb = Some(String::from_utf8_lossy(&a.value).parse::<u64>()
                            .map_err(|_| CoverageError::InvalidData("Invalid mb value".to_string()))?),
                        b"nr" => nr = Some(String::from_utf8_lossy(&a.value).parse::<u32>()
                            .map_err(|_| CoverageError::InvalidData("Invalid nr value".to_string()))?),
                        _ => {}
                    }
                }

                let ci = ci.ok_or_else(|| CoverageError::InvalidData("Missing ci attribute".to_string()))?;
                let cb = cb.ok_or_else(|| CoverageError::InvalidData("Missing cb attribute".to_string()))?;
                let mb = mb.ok_or_else(|| CoverageError::InvalidData("Missing mb attribute".to_string()))?;
                let nr = nr.ok_or_else(|| CoverageError::InvalidData("Missing nr attribute".to_string()))?;

                if mb > 0 || cb > 0 {
                    // This line is a branch.
                    let mut v = vec![true; cb as usize];
                    v.extend(vec![false; mb as usize]);
                    branches.insert(nr, v);
                } else {
                    // This line is a statement.
                    // JaCoCo does not feature execution counts, so we set the
                    // count to 0 or 1.
                    let hit = if ci > 0 { 1 } else { 0 };
                    lines.insert(nr, hit);
                }
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
        let a = a.map_err(|e| CoverageError::InvalidData(format!("Attribute error: {}", e)))?;
        if a.key.into_inner() == name.as_bytes() {
            return Ok(a.decode_and_unescape_value(reader.decoder())
                .map_err(CoverageError::Xml)?
                .into_owned());
        }
    }
    Err(CoverageError::InvalidData(format!("Attribute {} not found", name)))
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
}
