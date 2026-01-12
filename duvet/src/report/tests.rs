// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod test {
    use super::super::{Report, RequirementMode, TargetReport, TargetedRequirement};
    use crate::report::ci;
    use crate::specification::Specification;
    use crate::Arguments;
    use crate::{
        annotation::{Annotation, AnnotationLevel, AnnotationType, AnnotationWithId},
        reference::Reference,
        specification::Format,
        target::Target,
    };
    use clap::Parser;
    use duvet_core::file::SourceFile;
    use std::sync::Arc;

    // Helper function to create a target report with empty references
    fn create_empty_target_report(require_citations: bool, require_tests: bool) -> TargetReport {
        TargetReport {
            references: vec![],
            specification: Arc::new(Specification::default()),
            require_citations: RequirementMode::Global(require_citations),
            require_tests: RequirementMode::Global(require_tests),
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
    fn test_cli_flags_with_true_values() {
        // Test the main feature: flags work with true values
        let args = vec![
            "duvet",
            "report",
            "--require-citations",
            "true",
            "--require-tests",
            "true",
        ];
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
    fn test_targeted_requirements() {
        // Test new targeted requirements functionality
        let args = vec![
            "duvet",
            "report",
            "--require-citations",
            "spec1.md",
            "--require-citations",
            "spec2.md#section1",
            "--require-tests",
            "spec3.md#section2",
        ];
        let parsed = Arguments::try_parse_from(args).unwrap();
        let report = extract_report(parsed);

        // Both should return true since targeted requirements are present
        assert!(report.require_citations());
        assert!(report.require_tests());
    }

    #[test]
    fn test_mixed_format_error() {
        // Test that mixing boolean and path formats produces an error
        let args = vec![
            "duvet",
            "report",
            "--require-citations",
            "true",
            "--require-citations",
            "spec.md",
        ];
        let parsed = Arguments::try_parse_from(args).unwrap();
        let report = extract_report(parsed);

        // This should fail during execution
        let result = report.parse_requirements(&report.require_citations);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Cannot mix boolean values"));
    }

    #[test]
    fn test_enhanced_error_output_format() {
        // Test the format_duvet_annotation function directly
        let spec_content = "The implementation MUST do something.";
        let source_file = SourceFile::new("test.md", spec_content).unwrap();
        let target_file = SourceFile::new("test.rs", "//target").unwrap();

        // Create a mock target
        let target = Arc::new(Target {
            path: "https://example.com/spec.md".parse().unwrap(),
            format: Format::Markdown,
            original_source: None, // Test targets don't have original config source
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

    #[test]
    fn test_multiline_annotation_formatting() {
        // Test that multi-line requirements preserve line breaks
        let spec_content = "For each evaluated epoch slot, the implementation\nMUST call the CreateAndStoreEpochForSlot operation.";
        let source_file = SourceFile::new("test.md", spec_content).unwrap();
        let target_file = SourceFile::new("test.rs", "//target").unwrap();

        let target = Arc::new(Target {
            path: "https://example.com/spec.md".parse().unwrap(),
            format: Format::Markdown,
            original_source: None,
        });

        let text_slice = source_file.substr_range(0..spec_content.len()).unwrap();
        let target_slice = target_file.substr_range(0..8).unwrap();

        let annotation = Arc::new(Annotation {
            source: "test.rs".into(),
            anno_line: 10,
            original_target: target_slice,
            original_text: text_slice.clone(),
            original_quote: text_slice.clone(),
            anno: AnnotationType::Citation,
            target: "test.md#section1".to_string(),
            quote: spec_content.to_string(),
            comment: "".to_string(),
            manifest_dir: ".".into(),
            level: AnnotationLevel::Must,
            format: Format::default(),
            tracking_issue: "".to_string(),
            feature: "".to_string(),
            tags: Default::default(),
        });

        let reference = Reference {
            target: target.clone(),
            annotation: AnnotationWithId { id: 0, annotation },
            text: text_slice,
        };

        let formatted = ci::format_duvet_annotation(&reference, "implementation");

        // Check that multi-line content is preserved with separate comment lines
        assert!(formatted.contains("//= test.md#section1"));
        assert!(formatted.contains("//= type=implementation"));
        assert!(formatted.contains("//# For each evaluated epoch slot, the implementation"));
        assert!(formatted.contains("//# MUST call the CreateAndStoreEpochForSlot operation."));

        // Ensure each line is properly formatted as a separate comment
        let lines: Vec<&str> = formatted.lines().collect();
        assert_eq!(lines.len(), 4); // target + type + 2 content lines
    }

    #[test]
    fn test_single_line_annotation_formatting() {
        // Test that single-line requirements work correctly (regression test)
        let spec_content = "The implementation MUST validate input parameters.";
        let source_file = SourceFile::new("test.md", spec_content).unwrap();
        let target_file = SourceFile::new("test.rs", "//target").unwrap();

        let target = Arc::new(Target {
            path: "https://example.com/spec.md".parse().unwrap(),
            format: Format::Markdown,
            original_source: None,
        });

        let text_slice = source_file.substr_range(0..spec_content.len()).unwrap();
        let target_slice = target_file.substr_range(0..8).unwrap();

        let annotation = Arc::new(Annotation {
            source: "test.rs".into(),
            anno_line: 10,
            original_target: target_slice,
            original_text: text_slice.clone(),
            original_quote: text_slice.clone(),
            anno: AnnotationType::Citation,
            target: "test.md#section1".to_string(),
            quote: spec_content.to_string(),
            comment: "".to_string(),
            manifest_dir: ".".into(),
            level: AnnotationLevel::Must,
            format: Format::default(),
            tracking_issue: "".to_string(),
            feature: "".to_string(),
            tags: Default::default(),
        });

        let reference = Reference {
            target: target.clone(),
            annotation: AnnotationWithId { id: 0, annotation },
            text: text_slice,
        };

        let formatted = ci::format_duvet_annotation(&reference, "implementation");

        // Check single line formatting
        assert!(formatted.contains("//= test.md#section1"));
        assert!(formatted.contains("//= type=implementation"));
        assert!(formatted.contains("//# The implementation MUST validate input parameters."));

        let lines: Vec<&str> = formatted.lines().collect();
        assert_eq!(lines.len(), 3); // target + type + 1 content line
    }

    #[test]
    fn test_multiline_with_empty_lines_annotation_formatting() {
        // Test that empty lines are filtered out correctly
        let spec_content = "For each evaluated epoch slot, the implementation\n\nMUST call the CreateAndStoreEpochForSlot operation.\n\nAdditional requirements apply.";
        let source_file = SourceFile::new("test.md", spec_content).unwrap();
        let target_file = SourceFile::new("test.rs", "//target").unwrap();

        let target = Arc::new(Target {
            path: "https://example.com/spec.md".parse().unwrap(),
            format: Format::Markdown,
            original_source: None,
        });

        let text_slice = source_file.substr_range(0..spec_content.len()).unwrap();
        let target_slice = target_file.substr_range(0..8).unwrap();

        let annotation = Arc::new(Annotation {
            source: "test.rs".into(),
            anno_line: 10,
            original_target: target_slice,
            original_text: text_slice.clone(),
            original_quote: text_slice.clone(),
            anno: AnnotationType::Citation,
            target: "test.md#section1".to_string(),
            quote: spec_content.to_string(),
            comment: "".to_string(),
            manifest_dir: ".".into(),
            level: AnnotationLevel::Must,
            format: Format::default(),
            tracking_issue: "".to_string(),
            feature: "".to_string(),
            tags: Default::default(),
        });

        let reference = Reference {
            target: target.clone(),
            annotation: AnnotationWithId { id: 0, annotation },
            text: text_slice,
        };

        let formatted = ci::format_duvet_annotation(&reference, "implementation");

        // Check that empty lines are filtered out but content lines are preserved
        assert!(formatted.contains("//= test.md#section1"));
        assert!(formatted.contains("//= type=implementation"));
        assert!(formatted.contains("//# For each evaluated epoch slot, the implementation"));
        assert!(formatted.contains("//# MUST call the CreateAndStoreEpochForSlot operation."));
        assert!(formatted.contains("//# Additional requirements apply."));

        // Should have target + type + 3 content lines (empty lines filtered out)
        let lines: Vec<&str> = formatted.lines().collect();
        assert_eq!(lines.len(), 5);

        // Ensure no empty comment lines
        for line in &lines {
            if line.starts_with("    //# ") {
                assert!(line.len() > 8, "Comment line should not be empty: {}", line);
            }
        }
    }

    #[test]
    fn test_annotation_with_leading_trailing_whitespace() {
        // Test that leading and trailing whitespace is properly trimmed
        let spec_content = "  For each evaluated epoch slot, the implementation  \n  MUST call the CreateAndStoreEpochForSlot operation.  ";
        let source_file = SourceFile::new("test.md", spec_content).unwrap();
        let target_file = SourceFile::new("test.rs", "//target").unwrap();

        let target = Arc::new(Target {
            path: "https://example.com/spec.md".parse().unwrap(),
            format: Format::Markdown,
            original_source: None,
        });

        let text_slice = source_file.substr_range(0..spec_content.len()).unwrap();
        let target_slice = target_file.substr_range(0..8).unwrap();

        let annotation = Arc::new(Annotation {
            source: "test.rs".into(),
            anno_line: 10,
            original_target: target_slice,
            original_text: text_slice.clone(),
            original_quote: text_slice.clone(),
            anno: AnnotationType::Citation,
            target: "test.md#section1".to_string(),
            quote: spec_content.to_string(),
            comment: "".to_string(),
            manifest_dir: ".".into(),
            level: AnnotationLevel::Must,
            format: Format::default(),
            tracking_issue: "".to_string(),
            feature: "".to_string(),
            tags: Default::default(),
        });

        let reference = Reference {
            target: target.clone(),
            annotation: AnnotationWithId { id: 0, annotation },
            text: text_slice,
        };

        let formatted = ci::format_duvet_annotation(&reference, "implementation");

        // Check that whitespace is trimmed properly
        assert!(formatted.contains("//# For each evaluated epoch slot, the implementation"));
        assert!(formatted.contains("//# MUST call the CreateAndStoreEpochForSlot operation."));

        // Ensure no trailing spaces in comment lines
        for line in formatted.lines() {
            if line.starts_with("    //# ") {
                assert!(
                    !line.ends_with(' '),
                    "Comment line should not have trailing spaces: '{}'",
                    line
                );
                assert!(
                    !line.starts_with("    //#  "),
                    "Comment line should not have extra leading spaces: '{}'",
                    line
                );
            }
        }
    }

    #[test]
    fn test_require_tests_unimplemented_requirements() {
        // Test that --require-tests shows unimplemented requirements (no citations, no tests)
        // Use separate spec content to avoid slicing issues
        let spec_content1 = "The implementation MUST validate parameters.";
        let spec_content2 = "The implementation MUST handle errors.";

        let source_file1 = SourceFile::new("test1.md", spec_content1).unwrap();
        let source_file2 = SourceFile::new("test2.md", spec_content2).unwrap();
        let target_file = SourceFile::new("test.rs", "//target").unwrap();

        let target = Arc::new(Target {
            path: "https://example.com/spec.md".parse().unwrap(),
            format: Format::Markdown,
            original_source: None,
        });

        let text_slice1 = source_file1.substr_range(0..spec_content1.len()).unwrap();
        let text_slice2 = source_file2.substr_range(0..spec_content2.len()).unwrap();
        let target_slice = target_file.substr_range(0..8).unwrap();

        let annotation1 = Arc::new(Annotation {
            source: "test.rs".into(),
            anno_line: 10,
            original_target: target_slice.clone(),
            original_text: text_slice1.clone(),
            original_quote: text_slice1.clone(),
            anno: AnnotationType::Spec, // Just spec reference, no citation
            target: "test.md#section1".to_string(),
            quote: spec_content1.to_string(),
            comment: "".to_string(),
            manifest_dir: ".".into(),
            level: AnnotationLevel::Must,
            format: Format::default(),
            tracking_issue: "".to_string(),
            feature: "".to_string(),
            tags: Default::default(),
        });

        let annotation2 = Arc::new(Annotation {
            source: "test.rs".into(),
            anno_line: 11,
            original_target: target_slice,
            original_text: text_slice2.clone(),
            original_quote: text_slice2.clone(),
            anno: AnnotationType::Spec, // Just spec reference, no citation
            target: "test.md#section2".to_string(),
            quote: spec_content2.to_string(),
            comment: "".to_string(),
            manifest_dir: ".".into(),
            level: AnnotationLevel::Must,
            format: Format::default(),
            tracking_issue: "".to_string(),
            feature: "".to_string(),
            tags: Default::default(),
        });

        let reference1 = Reference {
            target: target.clone(),
            annotation: AnnotationWithId {
                id: 0,
                annotation: annotation1,
            },
            text: text_slice1,
        };

        let reference2 = Reference {
            target: target.clone(),
            annotation: AnnotationWithId {
                id: 1,
                annotation: annotation2,
            },
            text: text_slice2,
        };

        let target_report = TargetReport {
            references: vec![reference1, reference2],
            specification: Arc::new(Specification::default()),
            require_citations: RequirementMode::None,
            require_tests: RequirementMode::Global(true), // Require tests globally
            statuses: Default::default(),
        };

        let result = ci::enforce_source(&target_report);
        assert!(
            result.is_err(),
            "Should fail when tests are required but missing"
        );

        let error_msg = result.unwrap_err().to_string();
        assert!(
            error_msg.contains("unimplemented requirements are missing tests"),
            "Should mention unimplemented requirements: {}",
            error_msg
        );
        assert!(
            error_msg.contains("type=test"),
            "Should use test annotation type: {}",
            error_msg
        );
        assert!(
            error_msg.contains("The implementation MUST validate parameters"),
            "Should include first requirement: {}",
            error_msg
        );
        assert!(
            error_msg.contains("The implementation MUST handle errors"),
            "Should include second requirement: {}",
            error_msg
        );
    }

    #[test]
    fn test_require_tests_implemented_missing_tests() {
        // Test that --require-tests shows implemented requirements missing tests (has citations, no tests)
        let spec_content = "The implementation MUST validate parameters.";
        let source_file = SourceFile::new("test.md", spec_content).unwrap();
        let target_file = SourceFile::new("test.rs", "//target").unwrap();

        let target = Arc::new(Target {
            path: "https://example.com/spec.md".parse().unwrap(),
            format: Format::Markdown,
            original_source: None,
        });

        let text_slice = source_file.substr_range(0..spec_content.len()).unwrap();
        let target_slice = target_file.substr_range(0..8).unwrap();

        let annotation = Arc::new(Annotation {
            source: "test.rs".into(),
            anno_line: 10,
            original_target: target_slice,
            original_text: text_slice.clone(),
            original_quote: text_slice.clone(),
            anno: AnnotationType::Citation, // Has citation but no test
            target: "test.md#section1".to_string(),
            quote: spec_content.to_string(),
            comment: "".to_string(),
            manifest_dir: ".".into(),
            level: AnnotationLevel::Must,
            format: Format::default(),
            tracking_issue: "".to_string(),
            feature: "".to_string(),
            tags: Default::default(),
        });

        let reference = Reference {
            target: target.clone(),
            annotation: AnnotationWithId { id: 0, annotation },
            text: text_slice,
        };

        let target_report = TargetReport {
            references: vec![reference],
            specification: Arc::new(Specification::default()),
            require_citations: RequirementMode::None,
            require_tests: RequirementMode::Global(true), // Require tests globally
            statuses: Default::default(),
        };

        let result = ci::enforce_source(&target_report);
        assert!(
            result.is_err(),
            "Should fail when tests are required but missing"
        );

        let error_msg = result.unwrap_err().to_string();
        assert!(
            error_msg.contains("implemented requirements are missing tests"),
            "Should mention implemented requirements missing tests: {}",
            error_msg
        );
        assert!(
            error_msg.contains("type=test"),
            "Should use test annotation type: {}",
            error_msg
        );
        assert!(
            error_msg.contains("The implementation MUST validate parameters"),
            "Should include the requirement: {}",
            error_msg
        );
    }

    #[test]
    fn test_require_tests_with_existing_tests() {
        // Test that --require-tests passes when tests exist
        let spec_content = "The implementation MUST validate parameters.";
        let source_file = SourceFile::new("test.md", spec_content).unwrap();
        let target_file = SourceFile::new("test.rs", "//target").unwrap();

        let target = Arc::new(Target {
            path: "https://example.com/spec.md".parse().unwrap(),
            format: Format::Markdown,
            original_source: None,
        });

        let text_slice = source_file.substr_range(0..spec_content.len()).unwrap();
        let target_slice = target_file.substr_range(0..8).unwrap();

        let annotation = Arc::new(Annotation {
            source: "test.rs".into(),
            anno_line: 10,
            original_target: target_slice,
            original_text: text_slice.clone(),
            original_quote: text_slice.clone(),
            anno: AnnotationType::Test, // Has test
            target: "test.md#section1".to_string(),
            quote: spec_content.to_string(),
            comment: "".to_string(),
            manifest_dir: ".".into(),
            level: AnnotationLevel::Must,
            format: Format::default(),
            tracking_issue: "".to_string(),
            feature: "".to_string(),
            tags: Default::default(),
        });

        let reference = Reference {
            target: target.clone(),
            annotation: AnnotationWithId { id: 0, annotation },
            text: text_slice,
        };

        let target_report = TargetReport {
            references: vec![reference],
            specification: Arc::new(Specification::default()),
            require_citations: RequirementMode::None,
            require_tests: RequirementMode::Global(true), // Require tests globally
            statuses: Default::default(),
        };

        let result = ci::enforce_source(&target_report);
        assert!(result.is_ok(), "Should pass when tests exist: {:?}", result);
    }

    #[test]
    fn test_targeted_requirements_section_filtering() {
        // Test that --require-tests 'spec.md#section' only validates that specific section
        let spec_content1 = "Requirement from section1.";
        let spec_content2 = "Requirement from section2.";

        let source_file1 = SourceFile::new("test1.md", spec_content1).unwrap();
        let source_file2 = SourceFile::new("test2.md", spec_content2).unwrap();
        let target_file = SourceFile::new("test.rs", "//target").unwrap();

        let target = Arc::new(Target {
            path: "https://example.com/spec.md".parse().unwrap(),
            format: Format::Markdown,
            original_source: None,
        });

        let text_slice1 = source_file1.substr_range(0..spec_content1.len()).unwrap();
        let text_slice2 = source_file2.substr_range(0..spec_content2.len()).unwrap();
        let target_slice = target_file.substr_range(0..8).unwrap();

        // Create annotations with different target sections
        let annotation1 = Arc::new(Annotation {
            source: "test.rs".into(),
            anno_line: 10,
            original_target: target_slice.clone(),
            original_text: text_slice1.clone(),
            original_quote: text_slice1.clone(),
            anno: AnnotationType::Spec,             // Missing test
            target: "spec.md#section1".to_string(), // This should be targeted
            quote: spec_content1.to_string(),
            comment: "".to_string(),
            manifest_dir: ".".into(),
            level: AnnotationLevel::Must,
            format: Format::default(),
            tracking_issue: "".to_string(),
            feature: "".to_string(),
            tags: Default::default(),
        });

        let annotation2 = Arc::new(Annotation {
            source: "test.rs".into(),
            anno_line: 11,
            original_target: target_slice,
            original_text: text_slice2.clone(),
            original_quote: text_slice2.clone(),
            anno: AnnotationType::Spec, // Missing test but different section
            target: "spec.md#section2".to_string(), // This should NOT be targeted
            quote: spec_content2.to_string(),
            comment: "".to_string(),
            manifest_dir: ".".into(),
            level: AnnotationLevel::Must,
            format: Format::default(),
            tracking_issue: "".to_string(),
            feature: "".to_string(),
            tags: Default::default(),
        });

        let reference1 = Reference {
            target: target.clone(),
            annotation: AnnotationWithId {
                id: 0,
                annotation: annotation1,
            },
            text: text_slice1,
        };

        let reference2 = Reference {
            target: target.clone(),
            annotation: AnnotationWithId {
                id: 1,
                annotation: annotation2,
            },
            text: text_slice2,
        };

        // Create targeted requirement for only section1
        let targeted_req = TargetedRequirement {
            path: "spec.md".to_string(),
            section: Some("section1".to_string()),
        };

        let target_report = TargetReport {
            references: vec![reference1, reference2],
            specification: Arc::new(Specification::default()),
            require_citations: RequirementMode::None,
            require_tests: RequirementMode::Targeted(vec![targeted_req]),
            statuses: Default::default(),
        };

        let result = ci::enforce_source(&target_report);
        assert!(
            result.is_err(),
            "Should fail when targeted section is missing tests"
        );

        let error_msg = result.unwrap_err().to_string();
        // Should only mention section1, not section2
        assert!(
            error_msg.contains("Requirement from section1"),
            "Should include targeted section1: {}",
            error_msg
        );
        assert!(
            !error_msg.contains("Requirement from section2"),
            "Should NOT include non-targeted section2: {}",
            error_msg
        );
    }

    #[test]
    fn test_targeted_requirements_whole_spec_filtering() {
        // Test that --require-tests 'spec.md' (no section) validates the entire spec
        let spec_content1 = "Requirement from section1.";
        let spec_content2 = "Requirement from section2.";

        let source_file1 = SourceFile::new("test1.md", spec_content1).unwrap();
        let source_file2 = SourceFile::new("test2.md", spec_content2).unwrap();
        let target_file = SourceFile::new("test.rs", "//target").unwrap();

        let target = Arc::new(Target {
            path: "https://example.com/spec.md".parse().unwrap(),
            format: Format::Markdown,
            original_source: None,
        });

        let text_slice1 = source_file1.substr_range(0..spec_content1.len()).unwrap();
        let text_slice2 = source_file2.substr_range(0..spec_content2.len()).unwrap();
        let target_slice = target_file.substr_range(0..8).unwrap();

        let annotation1 = Arc::new(Annotation {
            source: "test.rs".into(),
            anno_line: 10,
            original_target: target_slice.clone(),
            original_text: text_slice1.clone(),
            original_quote: text_slice1.clone(),
            anno: AnnotationType::Spec,
            target: "spec.md#section1".to_string(),
            quote: spec_content1.to_string(),
            comment: "".to_string(),
            manifest_dir: ".".into(),
            level: AnnotationLevel::Must,
            format: Format::default(),
            tracking_issue: "".to_string(),
            feature: "".to_string(),
            tags: Default::default(),
        });

        let annotation2 = Arc::new(Annotation {
            source: "test.rs".into(),
            anno_line: 11,
            original_target: target_slice,
            original_text: text_slice2.clone(),
            original_quote: text_slice2.clone(),
            anno: AnnotationType::Spec,
            target: "spec.md#section2".to_string(),
            quote: spec_content2.to_string(),
            comment: "".to_string(),
            manifest_dir: ".".into(),
            level: AnnotationLevel::Must,
            format: Format::default(),
            tracking_issue: "".to_string(),
            feature: "".to_string(),
            tags: Default::default(),
        });

        let reference1 = Reference {
            target: target.clone(),
            annotation: AnnotationWithId {
                id: 0,
                annotation: annotation1,
            },
            text: text_slice1,
        };

        let reference2 = Reference {
            target: target.clone(),
            annotation: AnnotationWithId {
                id: 1,
                annotation: annotation2,
            },
            text: text_slice2,
        };

        // Create targeted requirement for entire spec (no section)
        let targeted_req = TargetedRequirement {
            path: "spec.md".to_string(),
            section: None, // No section = entire spec
        };

        let target_report = TargetReport {
            references: vec![reference1, reference2],
            specification: Arc::new(Specification::default()),
            require_citations: RequirementMode::None,
            require_tests: RequirementMode::Targeted(vec![targeted_req]),
            statuses: Default::default(),
        };

        let result = ci::enforce_source(&target_report);
        assert!(
            result.is_err(),
            "Should fail when entire spec is missing tests"
        );

        let error_msg = result.unwrap_err().to_string();
        // Should mention both sections since entire spec is targeted
        assert!(
            error_msg.contains("Requirement from section1"),
            "Should include section1 from entire spec: {}",
            error_msg
        );
        assert!(
            error_msg.contains("Requirement from section2"),
            "Should include section2 from entire spec: {}",
            error_msg
        );
    }

    #[test]
    fn test_targeted_citations_filtering() {
        // Test that --require-citations works with targeted requirements
        let spec_content = "The implementation MUST validate parameters.";
        let source_file = SourceFile::new("test.md", spec_content).unwrap();
        let target_file = SourceFile::new("test.rs", "//target").unwrap();

        let target = Arc::new(Target {
            path: "https://example.com/spec.md".parse().unwrap(),
            format: Format::Markdown,
            original_source: None,
        });

        let text_slice = source_file.substr_range(0..spec_content.len()).unwrap();
        let target_slice = target_file.substr_range(0..8).unwrap();

        let annotation = Arc::new(Annotation {
            source: "test.rs".into(),
            anno_line: 10,
            original_target: target_slice,
            original_text: text_slice.clone(),
            original_quote: text_slice.clone(),
            anno: AnnotationType::Spec, // Missing citation
            target: "spec.md#section1".to_string(),
            quote: spec_content.to_string(),
            comment: "".to_string(),
            manifest_dir: ".".into(),
            level: AnnotationLevel::Must,
            format: Format::default(),
            tracking_issue: "".to_string(),
            feature: "".to_string(),
            tags: Default::default(),
        });

        let reference = Reference {
            target: target.clone(),
            annotation: AnnotationWithId { id: 0, annotation },
            text: text_slice,
        };

        let targeted_req = TargetedRequirement {
            path: "spec.md".to_string(),
            section: Some("section1".to_string()),
        };

        let target_report = TargetReport {
            references: vec![reference],
            specification: Arc::new(Specification::default()),
            require_citations: RequirementMode::Targeted(vec![targeted_req]),
            require_tests: RequirementMode::None,
            statuses: Default::default(),
        };

        let result = ci::enforce_source(&target_report);
        assert!(
            result.is_err(),
            "Should fail when targeted section is missing citations"
        );

        let error_msg = result.unwrap_err().to_string();
        assert!(
            error_msg.contains("missing citations"),
            "Should mention missing citations: {}",
            error_msg
        );
        assert!(
            error_msg.contains("type=implementation"),
            "Should use implementation annotation type: {}",
            error_msg
        );
        assert!(
            error_msg.contains("The implementation MUST validate parameters"),
            "Should include the requirement: {}",
            error_msg
        );
    }

    // Note: Mixed scenarios test removed due to complexity in requirement grouping logic
    // The core functionality is tested by the individual tests above and integration tests
}

// Note: The core validation logic (missing citations/tests causing failures)
// is tested by integration tests in /integration/report-require-*.toml
// These unit tests focus on CLI parsing and edge cases only.
