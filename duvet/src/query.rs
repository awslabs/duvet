// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use crate::Result;
use duvet_core::{progress};

// Keep existing coverage modules for now (they will be used by the new coverage check)
pub mod coverage;
pub mod result;
pub mod parsers;

// New query modules
mod requirements;
mod checks;
mod engine;

use requirements::RequirementMode;
use checks::coverage::CoverageFormat;

#[derive(Debug, Parser)]
pub struct Query {
    /// Types of checks to run
    #[clap(short = 'c', long, value_delimiter = ',')]
    pub check: Option<Vec<CheckType>>,
    
    /// Specific sections to validate (comma-separated)
    #[clap(short = 's', long, value_delimiter = ',')]
    pub section: Option<Vec<String>>,
    
    /// Coverage report path override
    #[clap(short = 'r', long)]
    pub coverage_report: Option<Vec<String>>,
    
    /// Coverage format override
    #[clap(short = 'f', long)]
    pub coverage_format: Option<CoverageFormat>, 
    
    /// Enable verbose output
    #[clap(short = 'v', long)]
    pub verbose: bool,
}

#[derive(Clone, Debug, PartialEq, clap::ValueEnum)]
pub enum CheckType {
    #[value(alias = "implementations")]
    #[value(help = "Verifies that requirements from specifications have corresponding implementation annotations in source code.

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
    pub fn handle_brew_request() { ... }")]
    Implementation,
    #[value(alias = "tests")]
    #[value(help = "Verifies that all implementation annotations have a corresponding test.

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
    }")]
    Test,
    #[value(help = "Uses code coverage to verify that all test annotations are executed
and that each test annotation executes it corresponding implementation annotation(s).

The check PASSES when:
- Annotations accurately quote the specification requirements
- All test annotations are executed and each test annotation, the corresponding implementation annotation is executed.

The check FAILS when:
- Any test annotations are not executed
- Any corresponding implementation annotations are not executed.
- Any corresponding implementation annotations do not exist.

Executed:
An annotation is said to be executed if it is followed by an executed line in code coverage.
If _only_ whitespace exists between the end of the annotation and the executed line,
then the annotation is still said to be executed.
Execution is also a transitive property,
so if an annotation is stacked onto of an executed annotation it is also executed.

Any line that is not an annotation, or appears in the coverage report as executable
is considered `unknown` and will break the chain of executable.
This includes comments and type definitions, like interfaces.

Note: This check, like test, only operates on existing annotations.

Example implementation annotation:
    pub fn handle_brew_request() {
        //= https://www.rfc-editor.org/rfc/rfc2324#section-2.1.1
        //= type=test
        //# A coffee pot server MUST accept both the BREW and POST method
        //# equivalently.
    }")]
    Coverage,
    #[value(help = "The same as `coverage` expect it only operates on executed test annotations.
This is helpful for quick on-off checking of a single test.
")]
    ExecutedCoverage,
    Duplicates,
}

impl Query {
    pub async fn exec(&self) -> Result {        
        let progress = progress!("Starting duvet in query mode...");

        let sections = self.section.as_ref()
            .map(|v| v.clone())
            .unwrap_or_else(|| vec![]);

        // Convert sections to RequirementMode
        let requirement_mode = if sections.is_empty() {
            RequirementMode::Global
        } else {
            RequirementMode::parse_requirements(&sections)
        };

        let result = match &self.check {
            Some(check_types) => {
                let checks: Vec<(CheckType, &RequirementMode)> = check_types
                    .iter()
                    .map(|check_type| (check_type.clone(), &requirement_mode))
                    .collect();

                // Execute checks
                engine::execute_checks(
                    &checks,
                    self.coverage_report.as_ref(),
                    self.coverage_format.as_ref(),
                    self.verbose,
                ).await
            },
            None => unreachable!("Nothing to check?")
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
