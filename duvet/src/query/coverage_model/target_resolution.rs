// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Phase 1: Annotation Target Resolution (spec Section 2).
//!
//! Given an annotation span, determines the source construct it targets by walking
//! forward. This phase is purely structural — it does not consult coverage data.

use super::types::*;

/// Resolves the target of an annotation by forward walk (spec Section 2.3).
///
/// Returns `None` if the annotation is dangling (targets nothing) or reaches EOF.
/// Returns `Some(TargetLine { properties: None })` if the walk hits an unknown line.
pub fn annotation_target(
    annotation: &AnnotationSpan,
    classifications: &[Option<LineClass>],
    file_length: u64,
) -> Option<TargetLine> {
    let mut current = annotation.end_line + 1;

    while current <= file_length {
        // classifications is 0-indexed, lines are 1-indexed
        let idx = (current - 1) as usize;
        if idx >= classifications.len() {
            return None;
        }

        match &classifications[idx] {
            None => {
                // Unknown line — cannot resolve through it.
                return Some(TargetLine {
                    line_number: current,
                    properties: None,
                });
            }
            Some(props) => {
                if props.len() == 1 && props.contains(&LineProperty::Whitespace) {
                    current += 1;
                    continue;
                }

                if props.len() == 1 && props.contains(&LineProperty::Comment) {
                    current += 1;
                    continue;
                }

                if props.contains(&LineProperty::Annotation) {
                    // Stacked annotation — skip through it
                    current += 1;
                    continue;
                }

                if props.contains(&LineProperty::ScopeClose)
                    && !props.contains(&LineProperty::Statement)
                    && !props.contains(&LineProperty::Declaration)
                    && !props.contains(&LineProperty::ScopeOpen)
                {
                    // Closing brace with no substantive content — dangling
                    return None;
                }

                // Any other combination: this is the target
                return Some(TargetLine {
                    line_number: current,
                    properties: Some(props.clone()),
                });
            }
        }
    }

    // Reached end of file
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(props: &[LineProperty]) -> Option<LineClass> {
        Some(line_class(props))
    }

    #[test]
    fn annotation_before_method_sig() {
        let classifications = vec![
            s(&[LineProperty::Annotation]),  // line 1
            s(&[LineProperty::Annotation]),  // line 2
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),  // line 3
        ];
        let ann = AnnotationSpan { start_line: 1, end_line: 2 };
        let result = annotation_target(&ann, &classifications, 3);
        assert_eq!(result, Some(TargetLine {
            line_number: 3,
            properties: Some(line_class(&[LineProperty::Declaration, LineProperty::ScopeOpen])),
        }));
    }

    #[test]
    fn annotation_before_statement() {
        let classifications = vec![
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::Statement]),
        ];
        let ann = AnnotationSpan { start_line: 1, end_line: 1 };
        let result = annotation_target(&ann, &classifications, 2);
        assert_eq!(result, Some(TargetLine {
            line_number: 2,
            properties: Some(line_class(&[LineProperty::Statement])),
        }));
    }

    #[test]
    fn annotation_before_closing_brace() {
        let classifications = vec![
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::ScopeClose]),
        ];
        let ann = AnnotationSpan { start_line: 1, end_line: 1 };
        assert_eq!(annotation_target(&ann, &classifications, 2), None);
    }

    #[test]
    fn annotation_at_eof() {
        let classifications = vec![
            s(&[LineProperty::Annotation]),
        ];
        let ann = AnnotationSpan { start_line: 1, end_line: 1 };
        assert_eq!(annotation_target(&ann, &classifications, 1), None);
    }

    #[test]
    fn stacked_annotations_same_target() {
        let classifications = vec![
            s(&[LineProperty::Annotation]),  // line 1
            s(&[LineProperty::Annotation]),  // line 2
            s(&[LineProperty::Annotation]),  // line 3
            s(&[LineProperty::Annotation]),  // line 4
            s(&[LineProperty::Statement]),   // line 5
        ];
        let ann_a = AnnotationSpan { start_line: 1, end_line: 2 };
        let ann_b = AnnotationSpan { start_line: 3, end_line: 4 };
        let target_a = annotation_target(&ann_a, &classifications, 5);
        let target_b = annotation_target(&ann_b, &classifications, 5);
        assert_eq!(target_a, target_b);
        assert_eq!(target_a.unwrap().line_number, 5);
    }

    #[test]
    fn annotation_before_unknown_line() {
        let classifications = vec![
            s(&[LineProperty::Annotation]),
            None,  // unknown
            s(&[LineProperty::Statement]),
        ];
        let ann = AnnotationSpan { start_line: 1, end_line: 1 };
        let result = annotation_target(&ann, &classifications, 3);
        assert_eq!(result, Some(TargetLine {
            line_number: 2,
            properties: None,
        }));
    }

    #[test]
    fn skips_whitespace_and_comments() {
        let classifications = vec![
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::Whitespace]),
            s(&[LineProperty::Comment]),
            s(&[LineProperty::Declaration]),
        ];
        let ann = AnnotationSpan { start_line: 1, end_line: 1 };
        let result = annotation_target(&ann, &classifications, 4);
        assert_eq!(result.unwrap().line_number, 4);
    }

    #[test]
    fn annotation_before_declaration() {
        let classifications = vec![
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::Declaration]),
        ];
        let ann = AnnotationSpan { start_line: 1, end_line: 1 };
        let result = annotation_target(&ann, &classifications, 2);
        assert_eq!(result, Some(TargetLine {
            line_number: 2,
            properties: Some(line_class(&[LineProperty::Declaration])),
        }));
    }
}
