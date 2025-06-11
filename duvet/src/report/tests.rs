// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod test {
    use super::super::{Report, TargetReport};
    use crate::report::ci;
    use crate::specification::Specification;
    use crate::Arguments;
    use clap::Parser;
    use std::sync::Arc;

    // Helper function to create a target report with empty references
    fn create_empty_target_report(require_citations: bool, require_tests: bool) -> TargetReport {
        TargetReport {
            references: vec![],
            specification: Arc::new(Specification::default()),
            require_citations,
            require_tests,
            statuses: Default::default(),
        }
    }

    // Helper function to extract Report from Arguments
    fn extract_report(args: Arguments) -> Report {
        match args {
            Arguments::Report(report) => report,
            _ => panic!("Expected Report variant"),
        }
    }

    #[test]
    fn test_cli_flags_without_values() {
        // Test the main feature: flags work without explicit values
        let args = vec!["duvet", "report", "--require-citations", "--require-tests"];
        let parsed = Arguments::try_parse_from(args).unwrap();
        let report = extract_report(parsed);
        
        assert!(report.require_citations());
        assert!(report.require_tests());
    }

    #[test]
    fn test_cli_flags_with_false_values() {
        // Test explicit false values
        let args = vec!["duvet", "report", "--require-citations", "false", "--require-tests", "false"];
        let parsed = Arguments::try_parse_from(args).unwrap();
        let report = extract_report(parsed);
        
        assert!(!report.require_citations());
        assert!(!report.require_tests());
    }

    #[test]
    fn test_backward_compatibility_defaults() {
        // Test that defaults are false (opt-in behavior for new validation)
        let args = vec!["duvet", "report"];
        let parsed = Arguments::try_parse_from(args).unwrap();
        let report = extract_report(parsed);
        
        assert!(!report.require_citations(), "Should default to false (opt-in)");
        assert!(!report.require_tests(), "Should default to false (opt-in)");
    }

    #[test]
    fn test_empty_references_edge_case() {
        // Edge case: with no references, validation should always pass regardless of flags
        // This is correct behavior - you can't fail validation if there's nothing to validate
        let test_cases = [(false, false), (false, true), (true, false), (true, true)];

        for (require_citations, require_tests) in test_cases {
            let target_report = create_empty_target_report(require_citations, require_tests);
            let result = ci::enforce_source(&target_report);
            assert!(result.is_ok(), 
                "Should pass with no references (citations={}, tests={})", 
                require_citations, require_tests
            );
        }
    }
}

// Note: The core validation logic (missing citations/tests causing failures) 
// is tested by integration tests in /integration/report-require-*.toml
// These unit tests focus on CLI parsing and edge cases only.
