// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use super::super::build_references;
    use crate::{
        annotation::{Annotation, AnnotationLevel, AnnotationType, AnnotationWithId},
        specification::{Format, Section, Specification},
        target::Target,
    };
    use duvet_core::file::SourceFile;
    use std::{collections::HashMap, sync::Arc};

    fn create_test_source_file(content: &str) -> SourceFile {
        SourceFile::new("test.md", content).unwrap()
    }

    fn create_test_annotation(
        source_file: &SourceFile,
        quote: &str,
        target: &str,
    ) -> Arc<Annotation> {
        let target_slice = source_file.substr_range(0..8.min(source_file.len())).unwrap(); // Mock target slice
        // For the text slice, use the minimum of quote length and source file length
        let text_len = quote.len().min(source_file.len());
        let text_slice = source_file.substr_range(0..text_len).unwrap();

        Arc::new(Annotation {
            source: "test.rs".into(),
            anno_line: 1,
            original_target: target_slice,
            original_text: text_slice.clone(),
            original_quote: text_slice,
            anno: AnnotationType::Citation,
            target: target.to_string(),
            quote: quote.to_string(),
            comment: "".to_string(),
            manifest_dir: ".".into(),
            level: AnnotationLevel::Must,
            format: Format::default(),
            tracking_issue: "".to_string(),
            feature: "".to_string(),
            tags: Default::default(),
        })
    }

    fn create_test_specification(content: &str, section_id: &str) -> Arc<Specification> {
        let source_file = create_test_source_file(content);
        let title_slice = source_file.substr_range(0..10.min(source_file.len())).unwrap(); // Mock title

        let section = Section {
            id: section_id.to_string(),
            title: "Test Section".to_string(),
            full_title: title_slice,
            lines: vec![crate::specification::Line::Str(
                source_file.substr_range(0..content.len()).unwrap(),
            )],
        };

        let mut sections = HashMap::new();
        sections.insert(section_id.to_string(), section);

        Arc::new(Specification {
            title: Some("Test Spec".to_string()),
            sections,
            format: Format::Markdown,
        })
    }

    #[tokio::test]
    async fn test_single_line_reference_consolidation() {
        // Test that single-line requirements work as before (regression test)
        let content = "The implementation MUST validate input parameters.";
        let source_file = create_test_source_file(content);
        let annotation = create_test_annotation(&source_file, content, "test.md#section1");
        let target = Arc::new(Target {
            path: "test.md".parse().unwrap(),
            format: Format::Markdown,
            original_source: None,
        });
        let spec = create_test_specification(content, "section1");

        let annotations = Arc::from([AnnotationWithId {
            id: 0,
            annotation: annotation.clone(),
        }]);

        let (references, errors) = build_references(
            target.clone(),
            spec,
            Some(Arc::from("section1")),
            annotations,
        )
        .await;

        assert!(errors.is_empty(), "Should have no errors");
        assert_eq!(references.len(), 1, "Should have exactly one reference");

        let reference = &references[0];
        assert_eq!(
            reference.text.as_ref(),
            content,
            "Text should match the original content"
        );
    }

    #[tokio::test]
    async fn test_multiline_reference_consolidation() {
        // Test that multi-line requirements are properly consolidated
        let content = "For each evaluated epoch slot, the implementation\nMUST call the CreateAndStoreEpochForSlot operation.";
        let quote = content; // Full multi-line quote
        let source_file = create_test_source_file(content);
        let annotation = create_test_annotation(&source_file, quote, "test.md#section1");
        let target = Arc::new(Target {
            path: "test.md".parse().unwrap(),
            format: Format::Markdown,
            original_source: None,
        });
        let spec = create_test_specification(content, "section1");

        let annotations = Arc::from([AnnotationWithId {
            id: 0,
            annotation: annotation.clone(),
        }]);

        let (references, errors) = build_references(
            target.clone(),
            spec,
            Some(Arc::from("section1")),
            annotations,
        )
        .await;

        assert!(errors.is_empty(), "Should have no errors");
        assert_eq!(references.len(), 1, "Should have exactly one reference");

        let reference = &references[0];
        let reference_text = reference.text.as_ref();

        // The consolidated reference should contain the complete multi-line requirement
        assert!(
            reference_text.contains("For each evaluated epoch slot, the implementation"),
            "Should contain first line: {}",
            reference_text
        );
        assert!(
            reference_text.contains("MUST call the CreateAndStoreEpochForSlot operation."),
            "Should contain second line: {}",
            reference_text
        );

        // Check that line breaks are preserved
        assert!(
            reference_text.contains('\n'),
            "Should preserve line breaks: {}",
            reference_text
        );
    }

    #[tokio::test]
    async fn test_empty_quote_handling() {
        // Test that empty quotes are handled correctly (title reference)
        let content = "Test section content";
        let source_file = create_test_source_file(content);
        let annotation = create_test_annotation(&source_file, "", "test.md#section1");
        let target = Arc::new(Target {
            path: "test.md".parse().unwrap(),
            format: Format::Markdown,
            original_source: None,
        });
        let spec = create_test_specification(content, "section1");

        let annotations = Arc::from([AnnotationWithId {
            id: 0,
            annotation: annotation.clone(),
        }]);

        let (references, errors) = build_references(
            target.clone(),
            spec,
            Some(Arc::from("section1")),
            annotations,
        )
        .await;

        assert!(errors.is_empty(), "Should have no errors");
        assert_eq!(references.len(), 1, "Should have exactly one reference");

        // For empty quotes, should use the section title
        let reference = &references[0];
        assert_eq!(
            reference.text.range(),
            0..10, // Should match the title slice range we created
            "Empty quote should reference the section title"
        );
    }

    #[tokio::test]
    async fn test_missing_section_error() {
        // Test that missing sections produce appropriate errors
        let content = "Test content";
        let source_file = create_test_source_file(content);
        let annotation = create_test_annotation(&source_file, content, "test.md#nonexistent");
        let target = Arc::new(Target {
            path: "test.md".parse().unwrap(),
            format: Format::Markdown,
            original_source: None,
        });
        let spec = create_test_specification(content, "section1"); // Different section

        let annotations = Arc::from([AnnotationWithId {
            id: 0,
            annotation: annotation.clone(),
        }]);

        let (references, errors) = build_references(
            target.clone(),
            spec,
            Some(Arc::from("nonexistent")), // Non-existent section
            annotations,
        )
        .await;

        assert_eq!(references.len(), 0, "Should have no references");
        assert_eq!(errors.len(), 1, "Should have exactly one error");

        let error = &errors[0];
        assert!(
            error.to_string().contains("missing section"),
            "Error should mention missing section: {}",
            error
        );
    }

    #[tokio::test]
    async fn test_quote_not_found_error() {
        // Test that quotes not found in sections produce appropriate errors
        let content = "Actual section content";
        let source_file = create_test_source_file(content);
        let annotation = create_test_annotation(
            &source_file,
            "Non-existent quote text",
            "test.md#section1",
        );
        let target = Arc::new(Target {
            path: "test.md".parse().unwrap(),
            format: Format::Markdown,
            original_source: None,
        });
        let spec = create_test_specification(content, "section1");

        let annotations = Arc::from([AnnotationWithId {
            id: 0,
            annotation: annotation.clone(),
        }]);

        let (references, errors) = build_references(
            target.clone(),
            spec,
            Some(Arc::from("section1")),
            annotations,
        )
        .await;

        assert_eq!(references.len(), 0, "Should have no references");
        assert_eq!(errors.len(), 1, "Should have exactly one error");

        let error = &errors[0];
        assert!(
            error.to_string().contains("could not find text"),
            "Error should mention text not found: {}",
            error
        );
    }

    #[tokio::test]
    async fn test_no_section_id_handling() {
        // Test that missing section ID is handled correctly
        let content = "Test content";
        let source_file = create_test_source_file(content);
        let annotation = create_test_annotation(&source_file, content, "test.md");
        let target = Arc::new(Target {
            path: "test.md".parse().unwrap(),
            format: Format::Markdown,
            original_source: None,
        });
        let spec = create_test_specification(content, "section1");

        let annotations = Arc::from([AnnotationWithId {
            id: 0,
            annotation: annotation.clone(),
        }]);

        let (references, errors) =
            build_references(target.clone(), spec, None, annotations).await;

        assert_eq!(references.len(), 0, "Should have no references");
        assert_eq!(errors.len(), 0, "Should have no errors");
        // This case should return early without processing
    }

    #[tokio::test]
    async fn test_multiple_annotations_same_section() {
        // Test that multiple annotations for the same section work correctly
        let content = "First requirement.\nSecond requirement.";
        let source_file = create_test_source_file(content);
        let annotation1 = create_test_annotation(&source_file, "First requirement.", "test.md#section1");
        let annotation2 = create_test_annotation(&source_file, "Second requirement.", "test.md#section1");
        let target = Arc::new(Target {
            path: "test.md".parse().unwrap(),
            format: Format::Markdown,
            original_source: None,
        });
        let spec = create_test_specification(content, "section1");

        let annotations = Arc::from([
            AnnotationWithId {
                id: 0,
                annotation: annotation1.clone(),
            },
            AnnotationWithId {
                id: 1,
                annotation: annotation2.clone(),
            },
        ]);

        let (references, errors) = build_references(
            target.clone(),
            spec,
            Some(Arc::from("section1")),
            annotations,
        )
        .await;

        assert!(errors.is_empty(), "Should have no errors");
        assert_eq!(references.len(), 2, "Should have exactly two references");

        // Both references should exist and be properly formed
        let ref1_text = references[0].text.as_ref();
        let ref2_text = references[1].text.as_ref();

        assert!(
            ref1_text.contains("First requirement.") || ref2_text.contains("First requirement."),
            "Should contain first requirement"
        );
        assert!(
            ref1_text.contains("Second requirement.") || ref2_text.contains("Second requirement."),
            "Should contain second requirement"
        );
    }
}
