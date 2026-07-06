// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Phase 3: Annotation Execution Check (spec Section 4).

use crate::{execution_propagation::execution_set, target_resolution::annotation_target, types::*};
// `annotation_target_spec` and `validly_in_exec_set` are spec fns (ghost-only);
// they exist only when Verus is processing the crate, referenced from `ensures`
// and the `execution_status_of` spec twin.
#[cfg(verus_keep_ghost)]
use crate::predicates::validly_in_exec_set;
#[cfg(verus_keep_ghost)]
use crate::target_resolution::annotation_target_spec;
use vstd::prelude::*;

verus! {

// Spec twin of is_annotation_executed's status computation: a pure function of
// the resolved target line and the (annotation-independent) shared inputs.
// is_annotation_executed is proven equal to this (see its `ensures`), so the
// status depends on the annotation only through `annotation_target_spec` — the
// basis for Property 5 (stacking transitivity).
pub open spec fn execution_status_of(
    target: Option<u64>,
    classifications: &[Option<LineClass>],
    scopes: &[Scope],
    coverage: &CoverageReport,
) -> ExecutionStatus {
    match target {
        None => ExecutionStatus::Structural,
        Some(line) => {
            if classifications@[line as int - 1].is_none() {
                ExecutionStatus::Unknown { line_number: line }
            } else {
                let props = classifications@[line as int - 1].unwrap();
                if props@.contains(LineProperty::NonLinearControl) {
                    ExecutionStatus::Unknown { line_number: line }
                } else if validly_in_exec_set(line, classifications, scopes, coverage) {
                    ExecutionStatus::Executed
                } else if props@.contains(LineProperty::Statement) {
                    ExecutionStatus::NotExecuted
                } else if props@.contains(LineProperty::Declaration) {
                    match find_scope_containing_spec(line, scopes, 0, None, u64::MAX as int) {
                        None => ExecutionStatus::NotExecuted,
                        Some(idx) => if scopes@[idx].open_line >= 1
                            && scopes@[idx].close_line < u64::MAX
                            && has_statement_in_range(
                                classifications, scopes@[idx].open_line, scopes@[idx].close_line) {
                            ExecutionStatus::NotExecuted
                        } else {
                            ExecutionStatus::Structural
                        },
                    }
                } else {
                    ExecutionStatus::NotExecuted
                }
            }
        }
    }
}

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
        // Equivalence with the status spec twin: the status is a pure function of
        // the resolved target line and the shared inputs (basis for Property 5).
        status == execution_status_of(
            annotation_target_spec(annotation, classifications, file_length),
            classifications, scopes, coverage),
        // Property 6 (Unknown Safety), bullet (a): Executed requires a classified
        // target. If the result is Executed, the resolved target line exists and
        // is classified (not an unknown line).
        status == ExecutionStatus::Executed ==> {
            let line = annotation_target_spec(annotation, classifications, file_length);
            &&& line.is_some()
            &&& classifications@[line.unwrap() as int - 1].is_some()
        },
        // Property 6, bullet (b): no unknown line lies on the propagation path.
        // The spec (design §property-6-unknown-safety) requires that every line
        // between the directly-hit line L and the target is classified `Some(_)`.
        // Rather than leave that implicit in the four-predicate chain
        // (Executed => target in exec_set => validly_in_exec_set => has_valid_path
        // => clear_path => every intervening line is_some), state it directly:
        // an Executed target is validly in the execution set, i.e. either it is
        // itself directly hit (L == target, no lines between) or there is a hit
        // line with a clear path to it — and `clear_path` forbids `None` on that
        // path. This is exactly `execution_set`'s Property-1 postcondition
        // instantiated at the target, so it discharges from the exec-set
        // membership that produced the `Executed` verdict.
        status == ExecutionStatus::Executed ==> {
            let line = annotation_target_spec(annotation, classifications, file_length);
            &&& line.is_some()
            &&& validly_in_exec_set(line.unwrap(), classifications, scopes, coverage)
        },
{
    let target = annotation_target(annotation, classifications, file_length);
    match target {
        None => ExecutionStatus::Structural,
        Some(target_line) => {
            match &target_line.properties {
                None => ExecutionStatus::Unknown { line_number: target_line.line_number },
                Some(props) => {
                    proof {
                        broadcast use crate::types::lemma_line_property_obeys_cmp_spec;
                        assert(annotation_target_spec(annotation, classifications, file_length)
                            == Some(target_line.line_number));
                        assert(classifications@[target_line.line_number as int - 1].is_some());
                        assert(props@ == classifications@[target_line.line_number as int - 1].unwrap()@);
                    }
                    if props.contains(&LineProperty::NonLinearControl) { return ExecutionStatus::Unknown { line_number: target_line.line_number }; }
                    let exec_set = execution_set(classifications, scopes, coverage);
                    if exec_set.contains(&target_line.line_number) { return ExecutionStatus::Executed; }
                    if props.contains(&LineProperty::Statement) { return ExecutionStatus::NotExecuted; }
                    if props.contains(&LineProperty::Declaration) && !props.contains(&LineProperty::Statement) {
                        let scope = find_scope_containing(target_line.line_number, scopes);
                        if let Some(s) = scope {
                            let has_any_statements = if s.open_line >= 1 && s.close_line < u64::MAX {
                                scope_contains_statement(classifications, s.open_line, s.close_line)
                            } else {
                                false
                            };
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

// Spec: some line in [lo, hi] is a classified line carrying `Statement`.
pub open spec fn has_statement_in_range(
    classifications: &[Option<LineClass>],
    lo: u64,
    hi: u64,
) -> bool {
    exists|l: u64| #![trigger classifications@[l as int - 1]] lo <= l <= hi && l >= 1
        && (l as int - 1) < classifications@.len()
        && classifications@[l as int - 1].is_some()
        && classifications@[l as int - 1].unwrap()@.contains(LineProperty::Statement)
}

// Whether the scope spanning [lo, hi] contains any executable statement.
fn scope_contains_statement(classifications: &[Option<LineClass>], lo: u64, hi: u64) -> (result: bool)
    requires
        hi < u64::MAX,
    ensures
        result <==> has_statement_in_range(classifications, lo, hi),
{
    let mut found = false;
    let mut line = lo;
    while line <= hi && !found
        invariant
            lo <= line,
            hi < u64::MAX,
            found <==> exists|l: u64| #![trigger classifications@[l as int - 1]]
                lo <= l < line && l <= hi && l >= 1 && (l as int - 1) < classifications@.len()
                && classifications@[l as int - 1].is_some()
                && classifications@[l as int - 1].unwrap()@.contains(LineProperty::Statement),
        decreases hi - line + 1,
    {
        proof { broadcast use crate::types::lemma_line_property_obeys_cmp_spec; }
        if line >= 1 {
            let idx: usize = ((line - 1) as usize);
            // Lossless u64->usize cast given `global size_of usize == 8` (lib.rs).
            proof { assert(idx as int == line as int - 1); }
            if idx < classifications.len() {
                if let Some(p) = &classifications[idx] {
                    if p.contains(&LineProperty::Statement) {
                        found = true;
                    }
                }
            }
        }
        line = line + 1;
    }
    found
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
