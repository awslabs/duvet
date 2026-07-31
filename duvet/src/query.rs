// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::Result;
use clap::Parser;
use duvet_core::progress;

pub mod classify;
pub mod coverage;
pub mod parsers;
pub mod result;

mod checks;
mod engine;
pub mod producers;
mod requirements;
pub mod witness;

use producers::CoverageFormat;
use requirements::RequirementMode;

#[derive(Debug, Parser)]
pub struct Query {
    /// Types of checks to run (comma-separated)
    #[clap(short = 'c', long, value_delimiter = ',')]
    pub check: Option<Vec<CheckType>>,

    /// Specific sections to validate (comma-separated)
    #[clap(short = 's', long, value_delimiter = ',')]
    pub section: Option<Vec<String>>,

    /// Filter by quoted requirement text (case-insensitive substring match, repeatable)
    #[clap(short = 'q', long)]
    pub quote: Option<Vec<String>>,

    /// Coverage report path(s), supports globs (with --coverage-format, the
    /// one-source shorthand for coverage checks)
    #[clap(short = 'r', long)]
    pub coverage_report: Option<Vec<String>>,

    /// Coverage format (applies to every --coverage-report path)
    #[clap(short = 'f', long)]
    pub coverage_format: Option<CoverageFormat>,

    /// Coverage source as PRODUCER=PATH_OR_GLOB (repeatable; Decision 10:
    /// each source pairs a producer with its artifacts, N sources per
    /// invocation). Producers: `jacoco-xml` (report files), `verus-sst`
    /// (Verus `--log vir-sst` log directories). Combines with the
    /// -r/-f shorthand.
    #[clap(long, value_parser = parse_coverage_source)]
    pub coverage_source: Option<Vec<crate::query::producers::CoverageSource>>,

    /// Enable verbose output
    #[clap(short = 'v', long)]
    pub verbose: bool,
}

#[derive(Clone, Debug, PartialEq, clap::ValueEnum)]
// NOTE: The `#[value(help = "...")]` strings below contain example duvet
// annotations like `//= https://www.rfc-editor.org/rfc/rfc2324...` and the
// matching `//#` quote lines, on purpose — they show users what real
// annotations look like.
//
// Duvet's own annotation parser does not distinguish source comments from
// string literal contents, so if this file were scanned, these examples
// would be recorded as real citations. The repo's `.duvet/config.toml`
// therefore scans only `duvet-coverage/**` and deliberately excludes
// `duvet/**`. See https://github.com/awslabs/duvet/issues/226 for the
// parser fix that would make `duvet/**` scannable.
pub enum CheckType {
    #[value(alias = "implementations")]
    #[value(
        help = "Verifies that requirements from specifications have corresponding implementation annotations in source code.

The check PASSES when:
- Annotations accurately quote the specification requirements
- All requirement from specifications are covered by one of the following annotations
  - `implementation`
  - `implication`
  - `exception`

The check FAILS when:
- Specification requirements have no corresponding annotations  
- Requirements are annotated as `todo`
- Annotations don't fully cover the requirement text

Example implementation annotation:
    //= https://www.rfc-editor.org/rfc/rfc2324#section-2.1.1
    //# A coffee pot server MUST accept both the BREW and POST method
    //# equivalently.
    pub fn handle_brew_request() { ... }"
    )]
    Implementation,
    #[value(alias = "tests")]
    #[value(
        help = "Verifies that all implementation annotations have a corresponding test.

The check PASSES when:
- Annotations accurately quote the specification requirements
- All implementation annotations are covered by at least one test annotation

The check FAILS when:
- An implementation annotation does not have a corresponding test annotation
- Test annotations don't fully cover the implementation text

Note: This check only operates on existing annotations.
It does not verify that all requirements have an implementation.
It is level agnostic, specification text with out a level still needs a test.
`implication` and `exception` annotations do not require tests.
If a test does exist, it is not a failure.
Also, tests without an implementation are fine.
Finally, a single test annotation can point to multiple implementation annotations.

Example test annotation:
    pub fn test_handle_brew_request() {
        //= https://www.rfc-editor.org/rfc/rfc2324#section-2.1.1
        //= type=test
        //# A coffee pot server MUST accept both the BREW and POST method
        //# equivalently.
    }"
    )]
    Test,
    #[value(
        help = "Uses code coverage to verify that all test annotations are executed
and that each test annotation executes its corresponding implementation annotation(s).

The check PASSES when:
- Annotations accurately quote the specification requirements
- All test annotations are executed and, for each test annotation, the corresponding implementation annotation is executed.

The check FAILS when:
- Any test annotations are not executed
- Any corresponding implementation annotations are not executed.
- Any corresponding implementation annotations do not exist.

Executed:
An annotation is executed when the code construct it targets was executed
according to the coverage report. The target is found by walking forward from
the annotation, skipping whitespace, comments, and stacked annotations
(so execution is transitive across a stack of annotations). Non-executable
target lines (declarations, method signatures) count as executed when the
code they introduce ran.

Annotations that target purely structural constructs (e.g. an interface with
no executable code) are reported as structural and FAIL this check —
execution cannot verify them. Use `type=implication` for such targets.

When no line classifier exists for the file's language, a degraded model
reads the target's coverage directly from the report instead.

Note: This check, like test, only operates on existing annotations.

Example test annotation:
    pub fn test_handle_brew_request() {
        //= https://www.rfc-editor.org/rfc/rfc2324#section-2.1.1
        //= type=test
        //# A coffee pot server MUST accept both the BREW and POST method
        //# equivalently.
    }"
    )]
    Coverage,
    #[value(
        help = "The same as `coverage` except it only operates on executed test annotations.
This is helpful for quick on-off checking of a single test.
"
    )]
    ExecutedCoverage,
    Duplicates,
}

/// Parse a `--coverage-source` value: `PRODUCER=PATH_OR_GLOB`.
fn parse_coverage_source(s: &str) -> Result<crate::query::producers::CoverageSource, String> {
    let Some((producer, path)) = s.split_once('=') else {
        return Err(format!(
            "expected PRODUCER=PATH_OR_GLOB (e.g. jacoco-xml=target/site/jacoco.xml \
             or verus-sst=target/verus-logs), got '{s}'"
        ));
    };
    let Some(producer) = crate::query::producers::CoverageProducer::parse(producer) else {
        return Err(format!(
            "unknown coverage producer '{producer}' (expected jacoco-xml or verus-sst)"
        ));
    };
    if path.is_empty() {
        return Err("coverage source path must not be empty".to_string());
    }
    Ok(crate::query::producers::CoverageSource {
        producer,
        globs: vec![path.to_string()],
    })
}

impl Query {
    pub async fn exec(&self) -> Result {
        let progress = progress!("Starting duvet in query mode...");

        let sections = self.section.clone().unwrap_or_default();

        let quotes = self.quote.clone().unwrap_or_default();

        // Convert sections and quotes to RequirementMode
        let requirement_mode = RequirementMode::from_options(&sections, &quotes);

        let result = match &self.check {
            Some(check_types) if !check_types.is_empty() => {
                let checks: Vec<(CheckType, &RequirementMode)> = check_types
                    .iter()
                    .map(|check_type| (check_type.clone(), &requirement_mode))
                    .collect();

                // Execute checks
                engine::execute_checks(
                    &checks,
                    self.coverage_report.as_ref(),
                    self.coverage_format.as_ref(),
                    self.coverage_source.as_deref().unwrap_or(&[]),
                    self.verbose,
                )
                .await
            }
            _ => {
                // No check types specified — show help
                use clap::CommandFactory;
                let mut cmd = crate::Arguments::command();
                cmd.find_subcommand_mut("query")
                    .expect("query subcommand")
                    .print_help()
                    .expect("print help");
                println!();
                return Ok(());
            }
        }?;

        progress!(progress, "{}", result);

        // Exit with appropriate code
        if result.overall_status == result::QueryStatus::Pass {
            Ok(())
        } else {
            std::process::exit(1);
        }
    }
}
