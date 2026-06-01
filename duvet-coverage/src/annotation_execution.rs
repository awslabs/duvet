// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Phase 3: Annotation Execution Check (spec Section 4).

use crate::{execution_propagation::execution_set, target_resolution::annotation_target, types::*};
// `annotation_target_spec` is a spec fn (ghost-only); it exists only when Verus
// is processing the crate, and is referenced solely from the ghost `ensures`.
#[cfg(verus_keep_ghost)]
use crate::target_resolution::annotation_target_spec;
use vstd::prelude::*;

verus! {

pub fn is_annotation_executed(
    annotation: &AnnotationSpan,
    classifications: &[Option<LineClass>],
    scopes: &[Scope],
    coverage: &CoverageReport,
    file_length: u64,
) -> (status: ExecutionStatus)
    requires
        annotation.end_line < u64::MAX,
        forall|line: u64| coverage@.contains_key(line) ==> (line as int - 1) >= 0 && (line as int - 1) < classifications@.len(),
        forall|i: int| 0 <= i < scopes@.len() ==> (#[trigger] scopes@[i]).close_line < u64::MAX,
        forall|i: int| 0 <= i < scopes@.len() ==> (#[trigger] scopes@[i]).open_line >= 1,
    ensures
        // Property 6 (Unknown Safety): Executed requires a classified target.
        // If the result is Executed, then the resolved target line exists and
        // is classified (not an unknown line).
        status == ExecutionStatus::Executed ==> {
            let line = annotation_target_spec(annotation, classifications, file_length);
            &&& line.is_some()
            &&& classifications@[line.unwrap() as int - 1].is_some()
        },
{
    let target = annotation_target(annotation, classifications, file_length);
    match target {
        None => ExecutionStatus::Structural,
        Some(target_line) => {
            match &target_line.properties {
                None => ExecutionStatus::Unknown { line_number: target_line.line_number },
                Some(props) => {
                    if props.contains(&LineProperty::NonLinearControl) { return ExecutionStatus::Unknown { line_number: target_line.line_number }; }
                    let exec_set = execution_set(classifications, scopes, coverage);
                    if exec_set.contains(&target_line.line_number) { return ExecutionStatus::Executed; }
                    if props.contains(&LineProperty::Statement) { return ExecutionStatus::NotExecuted; }
                    if props.contains(&LineProperty::Declaration) && !props.contains(&LineProperty::Statement) {
                        let scope = find_scope_containing(target_line.line_number, scopes);
                        if let Some(s) = scope {
                            let mut has_any_statements = false;
                            if s.open_line >= 1 && s.close_line < u64::MAX {
                                let mut line = s.open_line;
                                while line <= s.close_line
                                    invariant
                                        line >= s.open_line,
                                    decreases s.close_line - line + 1,
                                {
                                    if line >= 1 {
                                        let idx: usize = ((line - 1) as usize);
                                        if idx < classifications.len() {
                                            if let Some(p) = &classifications[idx] {
                                                if p.contains(&LineProperty::Statement) { has_any_statements = true; break; }
                                            }
                                        }
                                    }
                                    if line == s.close_line { break; }
                                    line = line + 1;
                                }
                            }
                            if !has_any_statements { return ExecutionStatus::Structural; }
                        }
                    }
                    ExecutionStatus::NotExecuted
                }
            }
        }
    }
}

// Spec twin of `find_scope_containing`: the index of the minimal-size scope
// containing `line`, with the earliest index winning ties; None if no scope
// contains `line`. `best`/`best_size` are the running accumulator.
pub open spec fn find_scope_containing_spec(
    line: u64,
    scopes: &[Scope],
    i: int,
    best: Option<int>,
    best_size: int,
) -> Option<int>
    decreases scopes@.len() - i,
{
    if i >= scopes@.len() {
        best
    } else if line >= scopes@[i].open_line && line <= scopes@[i].close_line
        && (scopes@[i].close_line - scopes@[i].open_line) < best_size {
        find_scope_containing_spec(
            line, scopes, i + 1, Some(i),
            scopes@[i].close_line - scopes@[i].open_line,
        )
    } else {
        find_scope_containing_spec(line, scopes, i + 1, best, best_size)
    }
}

fn find_scope_containing<'a>(line: u64, scopes: &'a [Scope]) -> (result: Option<&'a Scope>)
    ensures
        result.is_some() <==> find_scope_containing_spec(line, scopes, 0, None, u64::MAX as int).is_some(),
        result.is_some() ==> {
            let idx = find_scope_containing_spec(line, scopes, 0, None, u64::MAX as int).unwrap();
            &&& 0 <= idx < scopes@.len()
            &&& result.unwrap().open_line == scopes@[idx].open_line
            &&& result.unwrap().close_line == scopes@[idx].close_line
        },
{
    let mut best: Option<&Scope> = None;
    let mut best_size: u64 = u64::MAX;
    let ghost mut best_idx: Option<int> = None;
    let mut i: usize = 0;
    while i < scopes.len()
        invariant
            0 <= i <= scopes@.len(),
            find_scope_containing_spec(line, scopes, 0, None, u64::MAX as int)
                == find_scope_containing_spec(line, scopes, i as int, best_idx, best_size as int),
            best.is_some() <==> best_idx.is_some(),
            best.is_none() ==> best_size == u64::MAX,
            best.is_some() ==> {
                &&& 0 <= best_idx.unwrap() < scopes@.len()
                &&& best.unwrap().open_line == scopes@[best_idx.unwrap()].open_line
                &&& best.unwrap().close_line == scopes@[best_idx.unwrap()].close_line
                &&& best_size as int
                    == scopes@[best_idx.unwrap()].close_line - scopes@[best_idx.unwrap()].open_line
            },
        decreases scopes.len() - i,
    {
        let s = &scopes[i];
        if line >= s.open_line && line <= s.close_line {
            let size = s.close_line - s.open_line;
            if size < best_size {
                best = Some(s);
                best_size = size;
                proof { best_idx = Some(i as int); }
            }
        }
        i = i + 1;
    }
    best
}

} // verus!

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;
    fn s(props: &[LineProperty]) -> Option<LineClass> {
        Some(line_class(props))
    }
    fn cov_hit(lines: &[u64]) -> CoverageReport {
        lines.iter().map(|&l| (l, CoverageStatus::Hit)).collect()
    }
    fn cov_miss(lines: &[u64]) -> CoverageReport {
        lines.iter().map(|&l| (l, CoverageStatus::Miss)).collect()
    }

    #[test]
    fn example_6_1_method_signature() {
        let c = vec![
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),
            s(&[LineProperty::Declaration]),
            s(&[LineProperty::Statement]),
            s(&[LineProperty::ScopeClose]),
        ];
        assert_eq!(
            is_annotation_executed(
                &AnnotationSpan {
                    start_line: 1,
                    end_line: 2
                },
                &c,
                &[Scope {
                    open_line: 3,
                    close_line: 6,
                    parent: None,
                    children: vec![]
                }],
                &cov_hit(&[5]),
                6
            ),
            ExecutionStatus::Executed
        );
    }
    #[test]
    fn example_6_2_interface() {
        let c = vec![
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),
            s(&[LineProperty::Declaration]),
            s(&[LineProperty::Declaration]),
            s(&[LineProperty::Declaration]),
            s(&[LineProperty::ScopeClose]),
        ];
        assert_eq!(
            is_annotation_executed(
                &AnnotationSpan {
                    start_line: 1,
                    end_line: 2
                },
                &c,
                &[Scope {
                    open_line: 3,
                    close_line: 7,
                    parent: None,
                    children: vec![]
                }],
                &CoverageReport::new(),
                7
            ),
            ExecutionStatus::Structural
        );
    }
    #[test]
    fn example_6_3_cross_method() {
        let c = vec![
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::Statement]),
            s(&[LineProperty::ScopeClose]),
            s(&[LineProperty::Whitespace]),
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),
            s(&[LineProperty::Statement]),
            s(&[LineProperty::ScopeClose]),
        ];
        assert_eq!(
            is_annotation_executed(
                &AnnotationSpan {
                    start_line: 2,
                    end_line: 3
                },
                &c,
                &[
                    Scope {
                        open_line: 1,
                        close_line: 5,
                        parent: None,
                        children: vec![]
                    },
                    Scope {
                        open_line: 7,
                        close_line: 9,
                        parent: None,
                        children: vec![]
                    }
                ],
                &cov_hit(&[4, 8]),
                9
            ),
            ExecutionStatus::Executed
        );
    }
    #[test]
    fn example_6_4_var_decl_no_init() {
        let c = vec![
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::Declaration]),
            s(&[LineProperty::Statement]),
            s(&[LineProperty::ScopeClose]),
        ];
        assert_eq!(
            is_annotation_executed(
                &AnnotationSpan {
                    start_line: 2,
                    end_line: 3
                },
                &c,
                &[Scope {
                    open_line: 1,
                    close_line: 6,
                    parent: None,
                    children: vec![]
                }],
                &cov_hit(&[5]),
                6
            ),
            ExecutionStatus::Executed
        );
    }
    #[test]
    fn example_6_5_stacked() {
        let c = vec![
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::Statement]),
            s(&[LineProperty::ScopeClose]),
        ];
        let sc = &[Scope {
            open_line: 1,
            close_line: 7,
            parent: None,
            children: vec![],
        }];
        let cov = cov_hit(&[6]);
        assert_eq!(
            is_annotation_executed(
                &AnnotationSpan {
                    start_line: 2,
                    end_line: 3
                },
                &c,
                sc,
                &cov,
                7
            ),
            ExecutionStatus::Executed
        );
        assert_eq!(
            is_annotation_executed(
                &AnnotationSpan {
                    start_line: 4,
                    end_line: 5
                },
                &c,
                sc,
                &cov,
                7
            ),
            ExecutionStatus::Executed
        );
    }
    #[test]
    fn example_6_6_goto() {
        let c = vec![
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::Declaration]),
            s(&[LineProperty::NonLinearControl, LineProperty::Statement]),
            s(&[LineProperty::Statement]),
            s(&[LineProperty::NonLinearControl]),
            s(&[LineProperty::Statement]),
            s(&[LineProperty::ScopeClose]),
        ];
        let mut cov = cov_hit(&[5, 8]);
        cov.insert(6, CoverageStatus::Miss);
        assert_eq!(
            is_annotation_executed(
                &AnnotationSpan {
                    start_line: 2,
                    end_line: 3
                },
                &c,
                &[Scope {
                    open_line: 1,
                    close_line: 9,
                    parent: None,
                    children: vec![]
                }],
                &cov,
                9
            ),
            ExecutionStatus::NotExecuted
        );
    }
    #[test]
    fn example_6_7_unknown_blocks_target() {
        let c = vec![
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::Annotation]),
            None,
            s(&[LineProperty::Statement]),
            s(&[LineProperty::ScopeClose]),
        ];
        // Target resolution lands on line 4 (the unknown line); Unknown carries that line number.
        assert_eq!(
            is_annotation_executed(
                &AnnotationSpan {
                    start_line: 2,
                    end_line: 3
                },
                &c,
                &[Scope {
                    open_line: 1,
                    close_line: 6,
                    parent: None,
                    children: vec![]
                }],
                &cov_hit(&[5]),
                6
            ),
            ExecutionStatus::Unknown { line_number: 4 }
        );
    }
    #[test]
    fn dangling_annotation_is_structural() {
        assert_eq!(
            is_annotation_executed(
                &AnnotationSpan {
                    start_line: 1,
                    end_line: 1
                },
                &vec![
                    s(&[LineProperty::Annotation]),
                    s(&[LineProperty::ScopeClose])
                ],
                &[],
                &CoverageReport::new(),
                2
            ),
            ExecutionStatus::Structural
        );
    }
    #[test]
    fn not_executed_statement() {
        let c = vec![
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::Statement]),
            s(&[LineProperty::ScopeClose]),
        ];
        assert_eq!(
            is_annotation_executed(
                &AnnotationSpan {
                    start_line: 2,
                    end_line: 2
                },
                &c,
                &[Scope {
                    open_line: 1,
                    close_line: 4,
                    parent: None,
                    children: vec![]
                }],
                &cov_miss(&[3]),
                4
            ),
            ExecutionStatus::NotExecuted
        );
    }
}
