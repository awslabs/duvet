// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod test {
    use super::super::{Report, TargetReport};
    use crate::report::ci;
    use crate::specification::Specification;
    use crate::Arguments;
    use crate::{
        annotation::{Annotation, AnnotationLevel, AnnotationType, AnnotationWithId},
        reference::Reference,
        specification::Format,
        target::{Target, TargetPath},
    };
    use clap::Parser;
    use duvet_core::file::SourceFile;
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
        let args = vec![
            "duvet",
            "report",
            "--require-citations",
            "false",
            "--require-tests",
            "false",
        ];
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

        assert!(
            !report.require_citations(),
            "Should default to false (opt-in)"
        );
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
            assert!(
                result.is_ok(),
                "Should pass with no references (citations={}, tests={})",
                require_citations,
                require_tests
            );
        }
    }

    #[test]
    fn test_enhanced_error_output_format() {
        // Test the format_duvet_annotation function directly
        let spec_content = "The implementation MUST do something.";
        let source_file = SourceFile::new("test.md", spec_content).unwrap();
        let target_file = SourceFile::new("test.rs", "//target").unwrap();

        // Create a mock target
        let target = Arc::new(Target {
            path: TargetPath::Path("test.md".into()),
            format: Format::default(),
        });

        // Get a slice that exists in the file
        let text_slice = source_file.substr_range(0..spec_content.len()).unwrap();
        let target_slice = target_file.substr_range(0..8).unwrap();

        // Create mock annotation
        let annotation = Arc::new(Annotation {
            source: "test.rs".into(),
            anno_line: 10,
            original_target: target_slice,
            original_text: text_slice.clone(),
            original_quote: text_slice.clone(),
            anno: AnnotationType::Spec,
            target: "test.md#section1".to_string(),
            quote: "The implementation MUST do something.".to_string(),
            comment: "".to_string(),
            manifest_dir: ".".into(),
            level: AnnotationLevel::Must,
            format: Format::default(),
            tracking_issue: "".to_string(),
            feature: "".to_string(),
            tags: Default::default(),
        });

        // Create reference
        let reference = Reference {
            target: target.clone(),
            annotation: AnnotationWithId { id: 0, annotation },
            text: text_slice,
        };

        // Test the format function directly
        let formatted = ci::format_duvet_annotation(&reference, "implementation");

        // Check that the formatted annotation contains expected components
        assert!(formatted.contains("//= test.md#section1"));
        assert!(formatted.contains("//= type=implementation"));
        assert!(formatted.contains("//# The implementation MUST do something."));
    }
}

// Note: The core validation logic (missing citations/tests causing failures)
// is tested by integration tests in /integration/report-require-*.toml
// These unit tests focus on CLI parsing and edge cases only.
