// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Phase 1: Annotation Target Resolution (spec Section 2).

#[cfg(verus_keep_ghost)]
pub use crate::predicates::line_is_skippable;
use crate::types::*;
use vstd::prelude::*;

verus! {

//= design/query/coverage-model-spec.md#property-10-annotation-target-bounds
//= type=implication
//# If `annotation_target(annotation, ...) = Some(target)`,
//# then `target.line_number > annotation.end_line`.
pub(crate) fn annotation_target(
    annotation: &AnnotationSpan,
    classifications: &[Option<LineClass>],
    file_length: u64,
) -> (result: Option<TargetLine>)
    requires
        annotation.end_line < u64::MAX,
    ensures
        result.is_some() ==> result.unwrap().line_number > annotation.end_line,
        // Property 5 support: all lines between the annotation end and the
        // target (exclusive) are skippable. This enables the stacking proof:
        // if annotation A is above B with only skippable lines between them,
        // A's walk skips through to B's end, then continues identically to B's walk.
        result.is_some() ==> forall|l: u64|
            annotation.end_line < l && l < result.unwrap().line_number
            ==> line_is_skippable(classifications, l),
        // When result is None, all lines from end_line+1 to file_length are
        // either skippable or past the classifications array.
        result.is_none() ==> forall|l: u64|
            annotation.end_line < l && l <= file_length
            && (l as int - 1) >= 0 && (l as int - 1) < classifications@.len()
            ==> line_is_skippable(classifications, l)
                || (classifications@[l as int - 1].is_some()
                    && classifications@[l as int - 1].unwrap()@.contains(LineProperty::ScopeClose)
                    && !classifications@[l as int - 1].unwrap()@.contains(LineProperty::Statement)
                    && !classifications@[l as int - 1].unwrap()@.contains(LineProperty::Declaration)
                    && !classifications@[l as int - 1].unwrap()@.contains(LineProperty::ScopeOpen)),
{
    let mut current: u64 = annotation.end_line + 1;

    while current <= file_length
        invariant
            current > annotation.end_line,
            current >= 1,
            annotation.end_line < u64::MAX,
            // All lines from annotation.end_line+1 to current-1 were skippable
            forall|l: u64| annotation.end_line < l && l < current
                ==> line_is_skippable(classifications, l),
        decreases file_length - current + 1,
    {
        if current == 0 { break; }
        let idx: usize = ((current - 1) as usize);
        if idx >= classifications.len() {
            return None;
        }

        match &classifications[idx] {
            None => {
                return Some(TargetLine { line_number: current, properties: None });
            }
            Some(props) => {
                proof { broadcast use crate::types::lemma_line_property_obeys_cmp_spec; }
                if props.len() == 1 && props.contains(&LineProperty::Whitespace) {
                    proof { assert(line_is_skippable(classifications, current)); }
                    if current == file_length { break; }
                    current = current + 1;
                    continue;
                }
                if props.len() == 1 && props.contains(&LineProperty::Comment) {
                    proof { assert(line_is_skippable(classifications, current)); }
                    if current == file_length { break; }
                    current = current + 1;
                    continue;
                }
                if props.contains(&LineProperty::Annotation) {
                    proof { assert(line_is_skippable(classifications, current)); }
                    if current == file_length { break; }
                    current = current + 1;
                    continue;
                }
                if props.contains(&LineProperty::ScopeClose)
                    && !props.contains(&LineProperty::Statement)
                    && !props.contains(&LineProperty::Declaration)
                    && !props.contains(&LineProperty::ScopeOpen)
                {
                    return None;
                }
                return Some(TargetLine { line_number: current, properties: Some(props.clone()) });
            }
        }
    }
    None
}

} // verus!

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;
    fn s(props: &[LineProperty]) -> Option<LineClass> {
        Some(line_class(props))
    }

    #[test]
    fn annotation_before_method_sig() {
        let c = vec![
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),
        ];
        assert_eq!(
            annotation_target(
                &AnnotationSpan {
                    start_line: 1,
                    end_line: 2
                },
                &c,
                3
            )
            .unwrap()
            .line_number,
            3
        );
    }
    #[test]
    fn annotation_before_statement() {
        assert_eq!(
            annotation_target(
                &AnnotationSpan {
                    start_line: 1,
                    end_line: 1
                },
                &vec![
                    s(&[LineProperty::Annotation]),
                    s(&[LineProperty::Statement])
                ],
                2
            )
            .unwrap()
            .line_number,
            2
        );
    }
    #[test]
    fn annotation_before_closing_brace() {
        assert_eq!(
            annotation_target(
                &AnnotationSpan {
                    start_line: 1,
                    end_line: 1
                },
                &vec![
                    s(&[LineProperty::Annotation]),
                    s(&[LineProperty::ScopeClose])
                ],
                2
            ),
            None
        );
    }
    #[test]
    fn annotation_at_eof() {
        assert_eq!(
            annotation_target(
                &AnnotationSpan {
                    start_line: 1,
                    end_line: 1
                },
                &vec![s(&[LineProperty::Annotation])],
                1
            ),
            None
        );
    }
    #[test]
    fn stacked_annotations_same_target() {
        let c = vec![
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::Statement]),
        ];
        assert_eq!(
            annotation_target(
                &AnnotationSpan {
                    start_line: 1,
                    end_line: 2
                },
                &c,
                5
            ),
            annotation_target(
                &AnnotationSpan {
                    start_line: 3,
                    end_line: 4
                },
                &c,
                5
            )
        );
    }
    #[test]
    fn annotation_before_unknown_line() {
        assert_eq!(
            annotation_target(
                &AnnotationSpan {
                    start_line: 1,
                    end_line: 1
                },
                &vec![
                    s(&[LineProperty::Annotation]),
                    None,
                    s(&[LineProperty::Statement])
                ],
                3
            ),
            Some(TargetLine {
                line_number: 2,
                properties: None
            })
        );
    }
    #[test]
    fn skips_whitespace_and_comments() {
        assert_eq!(
            annotation_target(
                &AnnotationSpan {
                    start_line: 1,
                    end_line: 1
                },
                &vec![
                    s(&[LineProperty::Annotation]),
                    s(&[LineProperty::Whitespace]),
                    s(&[LineProperty::Comment]),
                    s(&[LineProperty::Declaration])
                ],
                4
            )
            .unwrap()
            .line_number,
            4
        );
    }
    #[test]
    fn annotation_before_declaration() {
        assert_eq!(
            annotation_target(
                &AnnotationSpan {
                    start_line: 1,
                    end_line: 1
                },
                &vec![
                    s(&[LineProperty::Annotation]),
                    s(&[LineProperty::Declaration])
                ],
                2
            )
            .unwrap()
            .line_number,
            2
        );
    }
}
