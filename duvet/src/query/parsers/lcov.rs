// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! LCOV tracefile (`.info`) coverage parser.
//!
//! Normative behavior is specified in `design/lcov-parser/spec.md`; design
//! rationale in `design/lcov-parser/decisions.md`. In brief:
//!
//! - Only `DA:<line>,<count>[,<checksum>]` records are consumed. `TN`, `FN`,
//!   `FNDA`, `FNF`, `FNH`, `BRDA`, `BRF`, `BRH`, `LF`, `LH`, and any record
//!   type this parser does not recognize are ignored (spec §2): `DA` is the
//!   only record carrying the per-line counts the coverage model consumes,
//!   and unknown types are skipped for forward compatibility with newer
//!   producers. Recognition is exact and case-sensitive (spec §1); a
//!   *near-miss* of a structural keyword (`DA1,2`, `end_of_record `) is a
//!   hard error, not an unknown type — ignoring it would silently corrupt
//!   coverage or block structure (spec §2, decisions.md Decision 9).
//! - A line absent from the report is *no opinion* — never a Miss
//!   (coverage-model-spec §1.4). Only lines with a `DA` record appear in the
//!   output; `count > 0` becomes Hit and `count == 0` becomes Miss downstream
//!   in [`FileCoverage::to_coverage_report`].
//! - Duplicate `DA` records for the same (file, line) — within one `SF` block
//!   or across blocks — merge by saturating summation (spec §6).
//! - No branch data is produced (`BRDA` is out of scope).
//!
//! This module is the *trusted lexer glue* of the parser split (spec §8): it
//! turns text into per-file [`DaRecord`] sequences and hands them to the
//! Verus-verified aggregation core
//! [`duvet_coverage::lcov::aggregate_da_records`], which carries the
//! machine-checked aggregation properties (spec §7).

use rustc_hash::FxHashMap;
use std::{collections::BTreeMap, io::BufRead, path::Path};

use super::super::coverage::{
    parse_report_blocking, CoverageData, CoverageError, CoverageParser, FileCoverage,
    GenericCoverageData,
};
use crate::Result;
use duvet_coverage::lcov::{aggregate_da_records, DaRecord};

/// LCOV tracefile coverage parser.
pub struct LcovParser;

impl CoverageParser for LcovParser {
    async fn parse(&self, file_path: &Path) -> Result<CoverageData> {
        parse_report_blocking(file_path, parse_lcov_report).await
    }
}

/// Parse an LCOV tracefile from a buffered reader.
pub fn parse_lcov_report<T: BufRead>(reader: T) -> Result<GenericCoverageData, CoverageError> {
    // Lexed DA records per file key. Records for the same file accumulate
    // across SF blocks; the verified core then sums per line, so block
    // structure provably cannot affect the result (spec §7, Property 4).
    let mut records_by_file: FxHashMap<String, Vec<DaRecord>> = FxHashMap::default();
    // The open source-file block: its file key and the records lexed so far,
    // buffered locally and flushed into `records_by_file` when the block
    // closes (explicitly or at end-of-input).
    let mut open_block: Option<(String, Vec<DaRecord>)> = None;

    //= design/lcov-parser/spec.md#report-structure
    //= type=implementation
    //# The parser MUST accept both LF and CRLF line endings.
    // Discharged by the stdlib: `BufRead::lines` recognizes both `\n` and
    // `\r\n` as terminators and strips them from the yielded lines.
    for (idx, line) in reader.lines().enumerate() {
        let line_no = idx + 1;
        let line = line?;

        //= design/lcov-parser/spec.md#report-structure
        //= type=implementation
        //# The parser MUST ignore blank lines.
        if line.trim().is_empty() {
            continue;
        }

        if let Some(path) = line.strip_prefix("SF:") {
            //= design/lcov-parser/spec.md#report-structure
            //= type=implementation
            //# The parser MUST reject an `SF:` record that appears while
            //# a source-file block is already open.
            if open_block.is_some() {
                return Err(CoverageError::InvalidData(format!(
                    "line {line_no}: SF record inside an open source-file block \
                     (missing end_of_record?)"
                )));
            }

            //= design/lcov-parser/spec.md#path-handling
            //= type=implementation
            //# The parser MUST use the `SF:` payload verbatim as the
            //# file key, except that a leading `./` MUST be stripped.
            let path = path.strip_prefix("./").unwrap_or(path);

            //= design/lcov-parser/spec.md#path-handling
            //= type=implementation
            //# The parser MUST reject an `SF:` record whose payload is
            //# empty after stripping.
            if path.is_empty() {
                return Err(CoverageError::InvalidData(format!(
                    "line {line_no}: SF record with an empty path"
                )));
            }

            // Register the file even if the block carries no DA records: the
            // report names it, so it exists (with no per-line opinions).
            records_by_file.entry(path.to_string()).or_default();
            open_block = Some((path.to_string(), Vec::new()));
        } else if let Some(payload) = line.strip_prefix("DA:") {
            //= design/lcov-parser/spec.md#record-consumption
            //= type=implementation
            //# The parser MUST consume `DA` records.
            //= design/lcov-parser/spec.md#report-structure
            //= type=implementation
            //# The parser MUST reject a `DA` record that appears outside
            //# a source-file block.
            let Some((_, records)) = &mut open_block else {
                return Err(CoverageError::InvalidData(format!(
                    "line {line_no}: DA record outside a source-file block"
                )));
            };

            let record = parse_da_payload(payload).map_err(|reason| {
                CoverageError::InvalidData(format!(
                    "line {line_no}: malformed DA record 'DA:{payload}': {reason}"
                ))
            })?;

            records.push(record);
        } else if line == "end_of_record" {
            //= design/lcov-parser/spec.md#report-structure
            //= type=implementation
            //# The parser MUST reject an `end_of_record` record that
            //# appears outside a source-file block.
            let Some((file, records)) = open_block.take() else {
                return Err(CoverageError::InvalidData(format!(
                    "line {line_no}: end_of_record outside a source-file block"
                )));
            };
            // Flush the block: the entry exists since SF handling registered
            // it, but `entry().or_default()` keeps this structurally total.
            records_by_file.entry(file).or_default().extend(records);
        } else if line.starts_with("DA") {
            //= design/lcov-parser/spec.md#record-consumption
            //= type=implementation
            //# The parser MUST reject a record that begins with `DA`
            //# but is not a `DA` record.
            // Recognition is exact (spec §1): the `DA:` branch above did not
            // match, so this is a near-miss (`DA1,2`, `DA 4,1`) — producer
            // error or corruption, not a future record type. Ignoring it
            // would silently drop coverage (decisions.md, Decision 9).
            return Err(CoverageError::InvalidData(format!(
                "line {line_no}: malformed record {line:?}: a record starting \
                 with 'DA' must be 'DA:<line>,<count>[,<checksum>]'"
            )));
        } else if line.starts_with("end_of_record") {
            //= design/lcov-parser/spec.md#record-consumption
            //= type=implementation
            //# The parser MUST reject a record that begins with
            //# `end_of_record` but is not an `end_of_record` record.
            // The exact-match branch above did not match, so this line has
            // trailing content (e.g. `end_of_record `). Ignoring it would
            // leave the block open and fold the next block's DA records into
            // the wrong file (decisions.md, Decision 9).
            return Err(CoverageError::InvalidData(format!(
                "line {line_no}: malformed record {line:?}: a record starting \
                 with 'end_of_record' must be exactly 'end_of_record'"
            )));
        } else {
            //= design/lcov-parser/spec.md#record-consumption
            //= type=implementation
            //# The parser MUST ignore records of the following types:
            //# `TN`, `FN`, `FNDA`, `FNF`, `FNH`, `BRDA`, `BRF`, `BRH`, `LF`, `LH`.
            //= design/lcov-parser/spec.md#record-consumption
            //= type=implementation
            //# The parser MUST ignore records of types it does not recognize.
            continue;
        }
    }

    //= design/lcov-parser/spec.md#report-structure
    //= type=implementation
    //# The parser MUST accept end-of-input while a source-file
    //# block is open, treating it as an implicit `end_of_record`.
    if let Some((file, records)) = open_block.take() {
        records_by_file.entry(file).or_default().extend(records);
    }

    let mut coverage_data = GenericCoverageData::new();
    for (file, records) in records_by_file {
        // The verified core sums per line with saturation and returns
        // strictly sorted, unique keys (spec §7, Properties 1-3).
        //= design/lcov-parser/spec.md#aggregation
        //= type=implementation
        //# A line with no `DA` record MUST be absent from the parsed
        //# coverage for its file.
        let aggregated = aggregate_da_records(&records);

        // Property 3 (ordered uniqueness): the pairs are strictly sorted with
        // unique keys, so this collect is a structurally trivial conversion.
        let lines: BTreeMap<u64, u64> = aggregated.into_iter().collect();

        //= design/lcov-parser/spec.md#aggregation
        //= type=implementation
        //# The parsed coverage MUST contain no branch data.
        coverage_data.files.insert(
            file,
            FileCoverage {
                lines,
                branches: BTreeMap::new(),
            },
        );
    }

    Ok(coverage_data)
}

/// Parse the payload of a `DA` record: `<line>,<count>[,<checksum>]`.
///
/// Returns the reason string on malformed input; the caller attaches file
/// position context.
fn parse_da_payload(payload: &str) -> std::result::Result<DaRecord, String> {
    let mut fields = payload.split(',');

    let line_field = fields.next().expect("split yields at least one field");
    let Some(count_field) = fields.next() else {
        //= design/lcov-parser/spec.md#da-record-syntax
        //= type=implementation
        //# The parser MUST reject a `DA` record whose payload does
        //# not conform to this syntax.
        return Err("missing count field".to_string());
    };
    //= design/lcov-parser/spec.md#da-record-syntax
    //= type=implementation
    //# The parser MUST accept and ignore the optional third
    //# checksum field.
    let _checksum = fields.next();
    if fields.next().is_some() {
        return Err("too many fields".to_string());
    }

    //= design/lcov-parser/spec.md#da-record-syntax
    //= type=implementation
    //# The `<line>` and `<count>` fields MUST consist solely of
    //# ASCII digits `0`-`9`.
    // Stricter than the `str::parse` calls below, which would also accept a
    // leading `+`. Emptiness is caught here too (`all` is true for "").
    if line_field.is_empty() || !line_field.bytes().all(|b| b.is_ascii_digit()) {
        return Err(format!("invalid line number '{line_field}'"));
    }
    if count_field.is_empty() || !count_field.bytes().all(|b| b.is_ascii_digit()) {
        return Err(format!("invalid count '{count_field}'"));
    }

    //= design/lcov-parser/spec.md#da-record-syntax
    //= type=implementation
    //# The parser MUST parse `<line>` as a decimal integer
    //# greater than or equal to 1
    //# and representable in an unsigned 64-bit integer.
    let line: u64 = line_field
        .parse()
        .map_err(|_| format!("invalid line number '{line_field}'"))?;
    if line == 0 {
        return Err("line number must be >= 1".to_string());
    }

    //= design/lcov-parser/spec.md#da-record-syntax
    //= type=implementation
    //# The parser MUST parse `<count>` as a decimal integer
    //# representable in an unsigned 64-bit integer.
    let count: u64 = count_field
        .parse()
        .map_err(|_| format!("invalid count '{count_field}'"))?;

    Ok(DaRecord { line, count })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn parse(input: &str) -> GenericCoverageData {
        parse_lcov_report(Cursor::new(input)).unwrap()
    }

    fn parse_err(input: &str) -> String {
        match parse_lcov_report(Cursor::new(input)) {
            Ok(_) => panic!("expected parse error"),
            Err(e) => e.to_string(),
        }
    }

    #[test]
    //= design/lcov-parser/spec.md#record-consumption
    //= type=test
    //# The parser MUST consume `DA` records.
    //= design/lcov-parser/spec.md#aggregation
    //= type=test
    //# A line with no `DA` record MUST be absent from the parsed
    //# coverage for its file.
    fn basic_da_records() {
        let data = parse("SF:src/lib.rs\nDA:1,5\nDA:3,0\nend_of_record\n");
        let fc = data.files.get("src/lib.rs").unwrap();
        assert_eq!(fc.lines.get(&1), Some(&5));
        assert_eq!(fc.lines.get(&3), Some(&0));
        // Absent line: no opinion, not Miss.
        assert_eq!(fc.lines.get(&2), None);
        assert!(fc.branches.is_empty());
    }

    /// count > 0 -> Hit, count == 0 -> Miss, absent -> absent, through the
    /// shared verified-model conversion. (The absent-line requirement itself
    /// is owned by `basic_da_records`; this exercises the downstream report.)
    #[test]
    fn hit_miss_absent_through_coverage_report() {
        use duvet_coverage::types::CoverageStatus;

        let data = parse("SF:src/lib.rs\nDA:1,5\nDA:3,0\nend_of_record\n");
        let report = data.files.get("src/lib.rs").unwrap().to_coverage_report();
        assert_eq!(report.get(&1), Some(&CoverageStatus::Hit));
        assert_eq!(report.get(&3), Some(&CoverageStatus::Miss));
        assert_eq!(report.get(&2), None);
    }

    /// Duplicate DA records for the same line in one block sum their counts.
    /// (The summing requirement is owned by `duplicate_file_across_blocks_sums`,
    /// which covers the full quote including "every source-file block".)
    #[test]
    fn duplicate_lines_within_block_sum() {
        let data = parse("SF:a.rs\nDA:7,2\nDA:7,3\nend_of_record\n");
        assert_eq!(data.files.get("a.rs").unwrap().lines.get(&7), Some(&5));
    }

    /// The same file appearing in two SF blocks merges by summing —
    /// concatenated tracefiles (one block per test binary) behave like
    /// `lcov -a` merged output.
    #[test]
    //= design/lcov-parser/spec.md#aggregation
    //= type=test
    //# The count recorded for a line MUST be the sum of the
    //# counts of every `DA` record for that line in every
    //# source-file block naming that file.
    fn duplicate_file_across_blocks_sums() {
        let data = parse(
            "SF:a.rs\nDA:7,2\nDA:9,0\nend_of_record\nSF:a.rs\nDA:7,3\nDA:9,0\nend_of_record\n",
        );
        let fc = data.files.get("a.rs").unwrap();
        assert_eq!(fc.lines.get(&7), Some(&5));
        // 0 + 0 stays 0: still a Miss, not promoted or dropped.
        assert_eq!(fc.lines.get(&9), Some(&0));
        assert_eq!(data.files.len(), 1);
    }

    /// Summing saturates at u64::MAX instead of overflowing.
    #[test]
    //= design/lcov-parser/spec.md#aggregation
    //= type=test
    //# The sum MUST saturate at the maximum unsigned 64-bit
    //# value instead of overflowing.
    fn count_sum_saturates() {
        let input = format!("SF:a.rs\nDA:1,{}\nDA:1,2\nend_of_record\n", u64::MAX - 1);
        let data = parse(&input);
        assert_eq!(
            data.files.get("a.rs").unwrap().lines.get(&1),
            Some(&u64::MAX)
        );
    }

    /// The optional third checksum field is accepted and ignored.
    #[test]
    //= design/lcov-parser/spec.md#da-record-syntax
    //= type=test
    //# The parser MUST accept and ignore the optional third
    //# checksum field.
    fn da_checksum_field_ignored() {
        let data = parse("SF:a.rs\nDA:4,1,abc123\nend_of_record\n");
        assert_eq!(data.files.get("a.rs").unwrap().lines.get(&4), Some(&1));
    }

    /// A leading `./` on the SF path is stripped; nothing else is normalized.
    #[test]
    //= design/lcov-parser/spec.md#path-handling
    //= type=test
    //# The parser MUST use the `SF:` payload verbatim as the
    //# file key, except that a leading `./` MUST be stripped.
    fn sf_leading_dot_slash_stripped() {
        let data = parse("SF:./src/lib.rs\nDA:1,1\nend_of_record\n");
        assert!(data.files.contains_key("src/lib.rs"));
        // Absolute paths stay verbatim.
        let data = parse("SF:/abs/path/src/lib.rs\nDA:1,1\nend_of_record\n");
        assert!(data.files.contains_key("/abs/path/src/lib.rs"));
    }

    /// Non-DA record types are ignored, including unknown ones. Recognition
    /// is exact and case-sensitive (spec §1), so a lowercase `da:`, a
    /// whitespace-indented `DA:`, and a geninfo `--comment` line (`#...`)
    /// all fall into the unrecognized bucket — ignored, because none of them
    /// *begins with* the `DA` or `end_of_record` keywords (decisions.md,
    /// Decision 9).
    #[test]
    //= design/lcov-parser/spec.md#record-consumption
    //= type=test
    //# The parser MUST ignore records of the following types:
    //# `TN`, `FN`, `FNDA`, `FNF`, `FNH`, `BRDA`, `BRF`, `BRH`, `LF`, `LH`.
    //= design/lcov-parser/spec.md#record-consumption
    //= type=test
    //# The parser MUST ignore records of types it does not recognize.
    //= design/lcov-parser/spec.md#aggregation
    //= type=test
    //# The parsed coverage MUST contain no branch data.
    fn non_da_records_ignored() {
        let data = parse(
            "#comment line from geninfo --comment\n\
             TN:my_test\nSF:a.rs\nVER:some-future-record\nFN:3,foo\nFNDA:1,foo\nFNF:1\nFNH:1\n\
             BRDA:3,0,0,1\nBRF:1\nBRH:1\nda:3,9\n  DA:3,9\nDA:3,1\nLF:1\nLH:1\nend_of_record\n",
        );
        let fc = data.files.get("a.rs").unwrap();
        assert_eq!(fc.lines.len(), 1);
        // 1, not 10: the case-variant and indented records did not count.
        assert_eq!(fc.lines.get(&3), Some(&1));
        assert!(fc.branches.is_empty());
    }

    /// CRLF line endings and blank lines are tolerated.
    #[test]
    //= design/lcov-parser/spec.md#report-structure
    //= type=test
    //# The parser MUST ignore blank lines.
    //= design/lcov-parser/spec.md#report-structure
    //= type=test
    //# The parser MUST accept both LF and CRLF line endings.
    fn crlf_and_blank_lines() {
        let data = parse("SF:a.rs\r\n\r\nDA:1,1\r\n\nend_of_record\r\n");
        assert_eq!(data.files.get("a.rs").unwrap().lines.get(&1), Some(&1));
    }

    /// EOF with an open block is an implicit end_of_record.
    #[test]
    //= design/lcov-parser/spec.md#report-structure
    //= type=test
    //# The parser MUST accept end-of-input while a source-file
    //# block is open, treating it as an implicit `end_of_record`.
    fn eof_closes_open_block() {
        let data = parse("SF:a.rs\nDA:1,1");
        assert_eq!(data.files.get("a.rs").unwrap().lines.get(&1), Some(&1));
    }

    /// An SF block with no DA records still registers the file, with no
    /// per-line opinions.
    #[test]
    fn sf_block_without_da_registers_empty_file() {
        let data = parse("SF:a.rs\nend_of_record\n");
        let fc = data.files.get("a.rs").unwrap();
        assert!(fc.lines.is_empty());
        assert!(fc.branches.is_empty());
    }

    #[test]
    //= design/lcov-parser/spec.md#report-structure
    //= type=test
    //# The parser MUST reject a `DA` record that appears outside
    //# a source-file block.
    fn da_outside_block_errors() {
        let err = parse_err("DA:1,1\n");
        assert!(err.contains("outside a source-file block"), "{err}");
    }

    #[test]
    //= design/lcov-parser/spec.md#report-structure
    //= type=test
    //# The parser MUST reject an `SF:` record that appears while
    //# a source-file block is already open.
    fn nested_sf_errors() {
        let err = parse_err("SF:a.rs\nSF:b.rs\n");
        assert!(err.contains("inside an open source-file block"), "{err}");
    }

    #[test]
    //= design/lcov-parser/spec.md#report-structure
    //= type=test
    //# The parser MUST reject an `end_of_record` record that
    //# appears outside a source-file block.
    fn end_of_record_outside_block_errors() {
        let err = parse_err("end_of_record\n");
        assert!(err.contains("outside a source-file block"), "{err}");
    }

    #[test]
    //= design/lcov-parser/spec.md#path-handling
    //= type=test
    //# The parser MUST reject an `SF:` record whose payload is
    //# empty after stripping.
    fn empty_sf_path_errors() {
        assert!(parse_err("SF:\n").contains("empty path"));
        assert!(parse_err("SF:./\n").contains("empty path"));
    }

    /// A record starting with `DA` that is not an exact `DA:` record is a
    /// hard error, not an ignored unknown type: silently skipping it would
    /// drop coverage (decisions.md, Decision 9).
    #[test]
    //= design/lcov-parser/spec.md#record-consumption
    //= type=test
    //# The parser MUST reject a record that begins with `DA`
    //# but is not a `DA` record.
    fn near_miss_da_errors() {
        // Missing colon.
        let err = parse_err("SF:a.rs\nDA1,2\nend_of_record\n");
        assert!(err.contains("starting with 'DA'"), "{err}");
        // Space instead of colon.
        let err = parse_err("SF:a.rs\nDA 4,1\nend_of_record\n");
        assert!(err.contains("starting with 'DA'"), "{err}");
    }

    /// A record starting with `end_of_record` that is not exactly
    /// `end_of_record` is a hard error. Previously it was silently ignored,
    /// which left the block open: the DA records of the *next* block folded
    /// into the previous file and the eventual error surfaced at the next
    /// `SF:` with a misleading message (decisions.md, Decision 9).
    #[test]
    //= design/lcov-parser/spec.md#record-consumption
    //= type=test
    //# The parser MUST reject a record that begins with
    //# `end_of_record` but is not an `end_of_record` record.
    fn near_miss_end_of_record_errors() {
        // Trailing space — the silent-structural-corruption case. Assert the
        // near-miss diagnostic specifically: before this rule, this input
        // failed later, at `SF:b.rs`, with "inside an open source-file
        // block".
        let err = parse_err("SF:a.rs\nDA:1,1\nend_of_record \nSF:b.rs\nDA:2,1\nend_of_record\n");
        assert!(err.contains("exactly 'end_of_record'"), "{err}");
        assert!(err.contains("line 3"), "{err}");
        // Trailing non-whitespace content.
        let err = parse_err("SF:a.rs\nend_of_recordX\n");
        assert!(err.contains("exactly 'end_of_record'"), "{err}");
    }

    #[test]
    //= design/lcov-parser/spec.md#da-record-syntax
    //= type=test
    //# The parser MUST reject a `DA` record whose payload does
    //# not conform to this syntax.
    //= design/lcov-parser/spec.md#da-record-syntax
    //= type=test
    //# The parser MUST parse `<line>` as a decimal integer
    //# greater than or equal to 1
    //# and representable in an unsigned 64-bit integer.
    //= design/lcov-parser/spec.md#da-record-syntax
    //= type=test
    //# The parser MUST parse `<count>` as a decimal integer
    //# representable in an unsigned 64-bit integer.
    fn malformed_da_payloads_error() {
        // Missing count.
        assert!(parse_err("SF:a.rs\nDA:1\n").contains("missing count"));
        // Too many fields.
        assert!(parse_err("SF:a.rs\nDA:1,2,3,4\n").contains("too many fields"));
        // Non-numeric line / count.
        assert!(parse_err("SF:a.rs\nDA:x,1\n").contains("invalid line number"));
        assert!(parse_err("SF:a.rs\nDA:1,x\n").contains("invalid count"));
        // Negative values are non-numeric for unsigned parses.
        assert!(parse_err("SF:a.rs\nDA:-1,1\n").contains("invalid line number"));
        // Line 0 is producer error (LCOV is 1-based).
        assert!(parse_err("SF:a.rs\nDA:0,1\n").contains(">= 1"));
        // Line numbers must fit in u64 (duvet's line key width); 2^64 does not.
        assert!(parse_err("SF:a.rs\nDA:18446744073709551616,1\n").contains("invalid line number"));
        // A line number at the u32 boundary is representable now that the
        // whole pipeline keys lines as u64.
        let data = parse("SF:a.rs\nDA:4294967296,1\nend_of_record\n");
        assert_eq!(
            data.files.get("a.rs").unwrap().lines.get(&4294967296),
            Some(&1)
        );
        // Counts must fit in u64.
        assert!(parse_err("SF:a.rs\nDA:1,18446744073709551616\n").contains("invalid count"));
    }

    /// The field grammar is digits-only: a leading `+` (which `str::parse`
    /// would accept) does not conform; leading zeros do.
    #[test]
    //= design/lcov-parser/spec.md#da-record-syntax
    //= type=test
    //# The `<line>` and `<count>` fields MUST consist solely of
    //# ASCII digits `0`-`9`.
    fn da_fields_are_digits_only() {
        assert!(parse_err("SF:a.rs\nDA:+1,1\n").contains("invalid line number"));
        assert!(parse_err("SF:a.rs\nDA:1,+1\n").contains("invalid count"));
        // Leading zeros are digits: they conform and carry their value.
        let data = parse("SF:a.rs\nDA:007,1\nend_of_record\n");
        assert_eq!(data.files.get("a.rs").unwrap().lines.get(&7), Some(&1));
    }

    /// Empty input parses to an empty report.
    #[test]
    fn empty_input_is_empty_report() {
        let data = parse("");
        assert!(data.files.is_empty());
    }

    // -------------------------------------------------------------------------
    // Corpus pin tests (decisions.md Decision 7). Each corpus is a committed
    // real-producer tracefile pinned against its exact source
    // (duvet/tests/lcov-corpora/, regeneration recipes in its README). These
    // tests pin *which lines appear at all* — the absent-line behavior that
    // hand-written fixtures get wrong — so a toolchain behavior change
    // surfaces as a fixture diff, not silent semantic drift.
    //
    // Line numbers are derived from the committed source by content search, so
    // each assertion names the construct it pins.
    // -------------------------------------------------------------------------

    const MACRO_CORPUS_SRC: &str = include_str!("../../../tests/lcov-corpora/macro-corpus/lib.rs");
    const MACRO_CORPUS_INFO: &str =
        include_str!("../../../tests/lcov-corpora/macro-corpus/lcov.info");
    const TWO_MACROS_SRC: &str = include_str!("../../../tests/lcov-corpora/two-macros/lib.rs");
    const TWO_MACROS_INFO: &str = include_str!("../../../tests/lcov-corpora/two-macros/lcov.info");
    const VERUS_CORPUS_SRC: &str =
        include_str!("../../../tests/lcov-corpora/verus-corpus/vexample.rs");
    const VERUS_CORPUS_INFO: &str =
        include_str!("../../../tests/lcov-corpora/verus-corpus/lcov.info");

    /// 1-based line number of the `occurrence`-th line containing `needle`.
    fn line_of(src: &str, needle: &str, occurrence: usize) -> u64 {
        let mut seen = 0;
        for (idx, line) in src.lines().enumerate() {
            if line.contains(needle) {
                seen += 1;
                if seen == occurrence {
                    return (idx + 1) as u64;
                }
            }
        }
        panic!("needle {needle:?} occurrence {occurrence} not found in corpus source");
    }

    fn corpus_lines(info: &str, sf_key: &str) -> BTreeMap<u64, u64> {
        let data = parse(info);
        data.files
            .get(sf_key)
            .unwrap_or_else(|| panic!("corpus file {sf_key} missing from parsed report"))
            .lines
            .clone()
    }

    /// macro-corpus: rustc `-C instrument-coverage` line shape for macros,
    /// cfg-gating, and #[test] items.
    #[test]
    fn corpus_macro_lcov_line_shape() {
        let src = MACRO_CORPUS_SRC;
        let lines = corpus_lines(MACRO_CORPUS_INFO, "/tmp/macro-corpus/src/lib.rs");

        // Executed fn: signature line AND closing-brace line are present & Hit.
        let sig = line_of(src, "pub fn covered_vec", 1);
        assert_eq!(
            lines.get(&sig),
            Some(&1),
            "covered fn signature line is Hit"
        );
        // Body tail: `    v` then `}` — the closing brace line is present.
        let ret = line_of(src, "    v", 1);
        assert_eq!(
            lines.get(&(ret + 1)),
            Some(&1),
            "covered fn closing brace is Hit"
        );

        // Macro-interior argument lines are ABSENT (no DA record at all) —
        // absent means "no opinion", never Miss.
        for needle in ["        1,", "        2,", "        3,"] {
            let l = line_of(src, needle, 1);
            assert_eq!(
                lines.get(&l),
                None,
                "macro-interior line {l} ({needle:?}) is absent"
            );
        }

        // Compiled-but-uncalled fn: present with count 0 (a definite Miss).
        let sig = line_of(src, "pub fn uncovered_vec", 1);
        assert_eq!(
            lines.get(&sig),
            Some(&0),
            "uncovered fn signature line is Miss"
        );

        // cfg'd-out fn: totally absent — every line in its item range.
        let start = line_of(src, "pub fn cfg_gated_out", 1);
        for l in start..start + 6 {
            assert_eq!(lines.get(&l), None, "cfg'd-out line {l} is absent");
        }

        // Compiled-but-not-run #[test]: body lines present with count 0.
        let body = line_of(src, "assert_eq!(uncovered_vec()", 1);
        assert_eq!(
            lines.get(&body),
            Some(&0),
            "unrun #[test] body line is Miss"
        );
    }

    /// two-macros: identical multi-line call sites, different DA records —
    /// record presence depends on the macro *definition*, not the call site.
    #[test]
    fn corpus_two_macros_record_presence_depends_on_macro_definition() {
        let src = TWO_MACROS_SRC;
        let lines = corpus_lines(TWO_MACROS_INFO, "/tmp/two-macros/src/lib.rs");

        // `y * 3,` appears once in each call site. pick_first! discards its
        // second argument, so the line is ABSENT; add_both! uses it, so the
        // line is present and Hit.
        let in_pick = line_of(src, "        y * 3,", 1);
        let in_add = line_of(src, "        y * 3,", 2);
        assert_eq!(
            lines.get(&in_pick),
            None,
            "discarded macro argument line is absent (pick_first!)"
        );
        assert_eq!(
            lines.get(&in_add),
            Some(&1),
            "used macro argument line is Hit (add_both!)"
        );
    }

    /// verus-corpus: ghost code is erased before codegen — spec/proof fns and
    /// requires/ensures lines are entirely absent — but `proof { }` block
    /// lines INSIDE exec fns report HIT under today's Verus. That last shape
    /// is deliberately pinned: if a future Verus changes ghost erasure, this
    /// test fails and the change surfaces as a fixture decision instead of
    /// silent drift.
    #[test]
    fn corpus_verus_ghost_code_line_shape() {
        let src = VERUS_CORPUS_SRC;
        let lines = corpus_lines(VERUS_CORPUS_INFO, "/tmp/verus-corpus/vexample.rs");

        // spec fn / proof fn bodies: entirely absent (erased).
        for (needle, span) in [("spec fn abs_spec", 7u64), ("proof fn abs_nonneg", 7u64)] {
            let start = line_of(src, needle, 1);
            for l in start..start + span {
                assert_eq!(
                    lines.get(&l),
                    None,
                    "ghost item line {l} ({needle}) is absent"
                );
            }
        }

        // Exec fn: signature Hit (called twice -> count 2).
        let sig = line_of(src, "fn abs_exec", 1);
        assert_eq!(
            lines.get(&sig),
            Some(&2),
            "exec fn signature is Hit with call count"
        );

        // requires/ensures lines of the exec fn: absent.
        for l in sig + 1..sig + 6 {
            assert_eq!(lines.get(&l), None, "requires/ensures line {l} is absent");
        }

        // `proof { }` block lines inside the exec fn: present and HIT.
        let proof_open = line_of(src, "    proof {", 1);
        for l in proof_open..proof_open + 3 {
            assert_eq!(
                lines.get(&l),
                Some(&2),
                "proof-block line {l} inside exec fn reports HIT (pinned Verus behavior)"
            );
        }

        // Uncalled exec fn: present with count 0, including its proof block.
        let sig = line_of(src, "fn uncalled_exec", 1);
        assert_eq!(
            lines.get(&sig),
            Some(&0),
            "uncalled exec fn signature is Miss"
        );
        let proof_open = line_of(src, "    proof {", 2);
        for l in proof_open..proof_open + 3 {
            assert_eq!(
                lines.get(&l),
                Some(&0),
                "uncalled proof-block line {l} is Miss"
            );
        }
    }

    // -------------------------------------------------------------------------
    // Property-based round-trip (spec §8: the lexer is trusted glue, so its
    // contract is checked against an independent oracle rather than proved).
    // For ALL generated record multisets: render to LCOV text (with producer
    // variance — block splits, CRLF, checksums, `./` prefixes, noise records),
    // parse, and compare against an independently-computed reference: per-file,
    // per-line u128 sums saturated to u64. Exercises lexer + verified
    // aggregation end-to-end; the aggregation ensures are machine-checked, so
    // a failure here localizes to the lexer.
    // -------------------------------------------------------------------------
    #[test]
    fn prop_render_parse_round_trip() {
        use bolero::check;

        check!().with_type::<Vec<u8>>().for_each(|bytes| {
            let (text, expected) = gen_lcov(bytes);
            let parsed = parse_lcov_report(Cursor::new(text.clone()))
                .unwrap_or_else(|e| panic!("generated tracefile failed to parse: {e}\n{text}"));

            let mut expected_files: Vec<&String> = expected.keys().collect();
            expected_files.sort();
            let mut parsed_files: Vec<&String> = parsed.files.keys().collect();
            parsed_files.sort();
            assert_eq!(parsed_files, expected_files, "file sets differ\n{text}");

            for (file, want_lines) in &expected {
                let got = parsed.files.get(file).unwrap();
                assert_eq!(&got.lines, want_lines, "lines differ for {file}\n{text}");
                assert!(got.branches.is_empty(), "no branch data may be produced");
            }
        });
    }

    /// Deterministically derive (tracefile text, reference model) from bytes.
    /// The reference sums in u128 and saturates to u64 independently of the
    /// implementation under test.
    fn gen_lcov(bytes: &[u8]) -> (String, FxHashMap<String, BTreeMap<u64, u64>>) {
        const FILES: [&str; 2] = ["gen/a.rs", "gen/sub/b.rs"];
        let mut text = String::new();
        let mut sums: FxHashMap<String, BTreeMap<u64, u128>> = FxHashMap::default();

        let mut chunks = bytes.chunks_exact(4);
        let mut open = false;
        let mut current: usize = 0;

        for chunk in &mut chunks {
            let (b0, b1, b2, b3) = (chunk[0], chunk[1], chunk[2], chunk[3]);
            let eol = if b0 & 0x02 != 0 { "\r\n" } else { "\n" };

            if !open {
                current = (b0 & 0x01) as usize;
                // Sometimes render with a `./` prefix; the key never has it.
                let prefix = if b0 & 0x04 != 0 { "./" } else { "" };
                text.push_str(&format!("SF:{}{}{}", prefix, FILES[current], eol));
                sums.entry(FILES[current].to_string()).or_default();
                open = true;
            }

            // Noise records the parser must ignore.
            if b0 & 0x08 != 0 {
                text.push_str(&format!("FN:1,gen_fn{eol}BRDA:1,0,0,{b3}{eol}"));
            }
            if b0 & 0x10 != 0 {
                text.push_str(&format!("VER:future-record{eol}{eol}"));
            }

            let line = (b1 % 8) as u64 + 1;
            let count: u64 = match b2 % 4 {
                0 => 0,
                1 => b3 as u64,
                2 => u64::MAX - (b3 as u64 % 3),
                _ => 1,
            };
            if b0 & 0x20 != 0 {
                text.push_str(&format!("DA:{line},{count},checksum{eol}"));
            } else {
                text.push_str(&format!("DA:{line},{count}{eol}"));
            }
            *sums
                .get_mut(FILES[current])
                .unwrap()
                .entry(line)
                .or_insert(0) += count as u128;

            // Sometimes close the block; the same file may reopen later
            // (Property 4: block structure cannot matter).
            if b0 & 0x40 != 0 {
                text.push_str(&format!("end_of_record{eol}"));
                open = false;
            }
        }
        // A possibly-unterminated final block pins implicit EOF close.

        let expected = sums
            .into_iter()
            .map(|(file, lines)| {
                let lines = lines
                    .into_iter()
                    .map(|(l, sum)| (l, u64::try_from(sum).unwrap_or(u64::MAX)))
                    .collect();
                (file, lines)
            })
            .collect();
        (text, expected)
    }
}
