// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Phase 1: Annotation Target Resolution (spec Section 2).

#[cfg(verus_keep_ghost)]
pub use crate::predicates::line_is_skippable;
use crate::types::*;
use vstd::prelude::*;

verus! {

// Spec twin of `annotation_target`. `annotation_target_walk` is a pure,
// recursive definition of the forward target walk, returning the target's
// line number (or `None` when there is no target). `annotation_target`
// (exec) is proven equivalent to it (see its `ensures`), so specifications
// can refer to the walk's result without calling exec code in a ghost
// context (e.g. Property 6 in `annotation_execution`). The target's identity
// is its line number; the `properties` cached on the exec `TargetLine` are an
// implementation optimization. The skip test is exactly `line_is_skippable`,
// matching the exec body.
pub open spec fn annotation_target_walk(
    classifications: &[Option<LineClass>],
    current: u64,
    file_length: u64,
) -> Option<u64>
    decreases file_length - current + 1,
{
    if current > file_length || current == 0 || (current as int - 1) >= classifications@.len() {
        None
    } else if line_is_skippable(classifications, current) {
        if current < file_length {
            annotation_target_walk(classifications, (current + 1) as u64, file_length)
        } else {
            None
        }
    } else {
        match classifications@[current as int - 1] {
            // Unclassified line: it is the target (its properties are unknown).
            None => Some(current),
            Some(props) => if props@.contains(LineProperty::ScopeClose)
                && !props@.contains(LineProperty::Statement)
                && !props@.contains(LineProperty::Declaration)
                && !props@.contains(LineProperty::ScopeOpen) {
                // Pure scope-close: no target.
                None
            } else {
                Some(current)
            },
        }
    }
}

pub open spec fn annotation_target_spec(
    annotation: &AnnotationSpan,
    classifications: &[Option<LineClass>],
    file_length: u64,
) -> Option<u64> {
    annotation_target_walk(classifications, (annotation.end_line + 1) as u64, file_length)
}

//= design/query/coverage-model-spec.md#property-10-annotation-target-bounds
//= type=implication
//# If `annotation_target(annotation, ...) = Some(target)`,
//# then `target.line_number > annotation.end_line`.
pub fn annotation_target(
    annotation: &AnnotationSpan,
    classifications: &[Option<LineClass>],
    file_length: u64,
) -> (result: Option<TargetLine>)
    requires
        annotation.end_line < u64::MAX,
    ensures
        // Property 10: a resolved target lies below the annotation.
        result.is_some() ==> result.unwrap().line_number > annotation.end_line,
        // Equivalence with the spec twin: same presence and same target line.
        result.is_some() <==> annotation_target_spec(annotation, classifications, file_length).is_some(),
        result.is_some() ==> result.unwrap().line_number
            == annotation_target_spec(annotation, classifications, file_length).unwrap(),
        // The cached properties are present iff the target line is classified.
        result.is_some() ==> (result.unwrap().properties.is_some()
            <==> classifications@[result.unwrap().line_number as int - 1].is_some()),
{
    let mut current: u64 = annotation.end_line + 1;

    while current <= file_length
        invariant
            current > annotation.end_line,
            current >= 1,
            annotation.end_line < u64::MAX,
            // The remaining walk from `current` equals the whole walk: every
            // line skipped so far was skippable, so it did not change the target.
            annotation_target_walk(classifications, current, file_length)
                == annotation_target_spec(annotation, classifications, file_length),
        decreases file_length - current + 1,
    {
        let idx: usize = ((current - 1) as usize);
        proof {
            // u64->usize cast is lossless on this platform (compile-time assert
            // in lib.rs that usize >= u64). Connects exec `classifications[idx]`
            // to spec `classifications@[current as int - 1]`.
            assume(idx as int == current as int - 1);
        }
        if idx >= classifications.len() {
            proof { assert(annotation_target_walk(classifications, current, file_length) == None::<u64>); }
            return None;
        }

        match &classifications[idx] {
            None => {
                proof { assert(annotation_target_walk(classifications, current, file_length) == Some(current)); }
                return Some(TargetLine { line_number: current, properties: None });
            }
            Some(props) => {
                proof { broadcast use crate::types::lemma_line_property_obeys_cmp_spec; }
                if (props.len() == 1 && props.contains(&LineProperty::Whitespace))
                    || (props.len() == 1 && props.contains(&LineProperty::Comment))
                    || props.contains(&LineProperty::Annotation)
                {
                    proof { assert(line_is_skippable(classifications, current)); }
                    if current == file_length {
                        proof { assert(annotation_target_walk(classifications, current, file_length) == None::<u64>); }
                        return None;
                    }
                    proof {
                        assert(annotation_target_walk(classifications, current, file_length)
                            == annotation_target_walk(classifications, (current + 1) as u64, file_length));
                    }
                    current = current + 1;
                    continue;
                }
                if props.contains(&LineProperty::ScopeClose)
                    && !props.contains(&LineProperty::Statement)
                    && !props.contains(&LineProperty::Declaration)
                    && !props.contains(&LineProperty::ScopeOpen)
                {
                    proof { assert(annotation_target_walk(classifications, current, file_length) == None::<u64>); }
                    return None;
                }
                proof { assert(annotation_target_walk(classifications, current, file_length) == Some(current)); }
                return Some(TargetLine { line_number: current, properties: Some(props.clone()) });
            }
        }
    }
    // Unreachable in practice (the loop only exits by returning), but the
    // walk agrees: past EOF there is no target.
    proof { assert(current > file_length); }
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
