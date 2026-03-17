// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Phase 3: Annotation Execution Check (spec Section 4).
//!
//! Composes Phase 1 (target resolution) and Phase 2 (execution propagation)
//! to determine whether an annotation is executed.

use super::execution_propagation::execution_set;
use super::target_resolution::annotation_target;
use super::types::*;

/// Determines whether an annotation is executed (spec Section 4.3).
///
/// Composes target resolution (Phase 1) and execution propagation (Phase 2).
pub fn is_annotation_executed(
    annotation: &AnnotationSpan,
    classifications: &[Option<LineClass>],
    scopes: &[Scope],
    coverage: &CoverageReport,
    file_length: u64,
) -> ExecutionStatus {
    // Phase 1: What does this annotation target?
    let target = annotation_target(annotation, classifications, file_length);

    match target {
        None => {
            // Annotation targets nothing (dangling or EOF)
            ExecutionStatus::Structural
        }
        Some(target_line) => {
            match &target_line.properties {
                None => {
                    // Target is an unknown line
                    ExecutionStatus::Unknown
                }
                Some(props) => {
                    if props.contains(&LineProperty::NonLinearControl) {
                        return ExecutionStatus::Unknown;
                    }

                    // Phase 2: Is the target in the execution set?
                    let exec_set = execution_set(classifications, scopes, coverage);

                    if exec_set.contains(&target_line.line_number) {
                        return ExecutionStatus::Executed;
                    }

                    // Target is not in execution set.
                    // Distinguish "not executed" from "structurally non-executable."
                    if props.contains(&LineProperty::Statement) {
                        return ExecutionStatus::NotExecuted;
                    }

                    if props.contains(&LineProperty::Declaration)
                        && !props.contains(&LineProperty::Statement)
                    {
                        // Check if there are any executable statements in the same scope
                        let scope = find_scope_containing(target_line.line_number, scopes);
                        let has_any_statements = scope.map_or(false, |s| {
                            (s.open_line..=s.close_line).any(|line| {
                                let idx = (line - 1) as usize;
                                if idx < classifications.len() {
                                    if let Some(p) = &classifications[idx] {
                                        return p.contains(&LineProperty::Statement);
                                    }
                                }
                                false
                            })
                        });

                        if !has_any_statements {
                            return ExecutionStatus::Structural;
                        }
                    }

                    ExecutionStatus::NotExecuted
                }
            }
        }
    }
}

/// Finds the innermost scope containing the given line.
fn find_scope_containing(line: u64, scopes: &[Scope]) -> Option<&Scope> {
    scopes
        .iter()
        .filter(|s| line >= s.open_line && line <= s.close_line)
        .min_by_key(|s| s.close_line - s.open_line)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        // Spec example 6.1
        let classifications = vec![
            s(&[LineProperty::Annotation]),                            // 1
            s(&[LineProperty::Annotation]),                            // 2
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),  // 3
            s(&[LineProperty::Declaration]),                           // 4
            s(&[LineProperty::Statement]),                             // 5: Hit
            s(&[LineProperty::ScopeClose]),                            // 6
        ];
        let scopes = vec![Scope { open_line: 3, close_line: 6, parent: None, children: vec![] }];
        let coverage = cov_hit(&[5]);
        let ann = AnnotationSpan { start_line: 1, end_line: 2 };
        assert_eq!(
            is_annotation_executed(&ann, &classifications, &scopes, &coverage, 6),
            ExecutionStatus::Executed
        );
    }

    #[test]
    fn example_6_2_interface() {
        // Spec example 6.2
        let classifications = vec![
            s(&[LineProperty::Annotation]),                            // 1
            s(&[LineProperty::Annotation]),                            // 2
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),  // 3
            s(&[LineProperty::Declaration]),                           // 4
            s(&[LineProperty::Declaration]),                           // 5
            s(&[LineProperty::Declaration]),                           // 6
            s(&[LineProperty::ScopeClose]),                            // 7
        ];
        let scopes = vec![Scope { open_line: 3, close_line: 7, parent: None, children: vec![] }];
        let coverage = CoverageReport::new();
        let ann = AnnotationSpan { start_line: 1, end_line: 2 };
        assert_eq!(
            is_annotation_executed(&ann, &classifications, &scopes, &coverage, 7),
            ExecutionStatus::Structural
        );
    }

    #[test]
    fn example_6_4_var_decl_no_init() {
        // Spec example 6.4
        let classifications = vec![
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),  // 1
            s(&[LineProperty::Annotation]),                            // 2
            s(&[LineProperty::Annotation]),                            // 3
            s(&[LineProperty::Declaration]),                           // 4: int result;
            s(&[LineProperty::Statement]),                             // 5: Hit
            s(&[LineProperty::ScopeClose]),                            // 6
        ];
        let scopes = vec![Scope { open_line: 1, close_line: 6, parent: None, children: vec![] }];
        let coverage = cov_hit(&[5]);
        let ann = AnnotationSpan { start_line: 2, end_line: 3 };
        assert_eq!(
            is_annotation_executed(&ann, &classifications, &scopes, &coverage, 6),
            ExecutionStatus::Executed
        );
    }

    #[test]
    fn example_6_5_stacked() {
        // Spec example 6.5
        let classifications = vec![
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),  // 1
            s(&[LineProperty::Annotation]),                            // 2
            s(&[LineProperty::Annotation]),                            // 3
            s(&[LineProperty::Annotation]),                            // 4
            s(&[LineProperty::Annotation]),                            // 5
            s(&[LineProperty::Statement]),                             // 6: Hit
            s(&[LineProperty::ScopeClose]),                            // 7
        ];
        let scopes = vec![Scope { open_line: 1, close_line: 7, parent: None, children: vec![] }];
        let coverage = cov_hit(&[6]);
        let ann_a = AnnotationSpan { start_line: 2, end_line: 3 };
        let ann_b = AnnotationSpan { start_line: 4, end_line: 5 };
        assert_eq!(
            is_annotation_executed(&ann_a, &classifications, &scopes, &coverage, 7),
            ExecutionStatus::Executed
        );
        assert_eq!(
            is_annotation_executed(&ann_b, &classifications, &scopes, &coverage, 7),
            ExecutionStatus::Executed
        );
    }

    #[test]
    fn example_6_6_goto() {
        // Spec example 6.6
        let classifications = vec![
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),           // 1
            s(&[LineProperty::Annotation]),                                     // 2
            s(&[LineProperty::Annotation]),                                     // 3
            s(&[LineProperty::Declaration]),                                    // 4
            s(&[LineProperty::NonLinearControl, LineProperty::Statement]),      // 5: Hit
            s(&[LineProperty::Statement]),                                      // 6: Miss
            s(&[LineProperty::NonLinearControl]),                               // 7
            s(&[LineProperty::Statement]),                                      // 8: Hit
            s(&[LineProperty::ScopeClose]),                                     // 9
        ];
        let scopes = vec![Scope { open_line: 1, close_line: 9, parent: None, children: vec![] }];
        let mut coverage = cov_hit(&[5, 8]);
        coverage.insert(6, CoverageStatus::Miss);
        let ann = AnnotationSpan { start_line: 2, end_line: 3 };
        assert_eq!(
            is_annotation_executed(&ann, &classifications, &scopes, &coverage, 9),
            ExecutionStatus::NotExecuted
        );
    }

    #[test]
    fn example_6_7_unknown_blocks_target() {
        // Spec example 6.7
        let classifications = vec![
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),  // 1
            s(&[LineProperty::Annotation]),                            // 2
            s(&[LineProperty::Annotation]),                            // 3
            None,                                                      // 4: unknown
            s(&[LineProperty::Statement]),                             // 5: Hit
            s(&[LineProperty::ScopeClose]),                            // 6
        ];
        let scopes = vec![Scope { open_line: 1, close_line: 6, parent: None, children: vec![] }];
        let coverage = cov_hit(&[5]);
        let ann = AnnotationSpan { start_line: 2, end_line: 3 };
        assert_eq!(
            is_annotation_executed(&ann, &classifications, &scopes, &coverage, 6),
            ExecutionStatus::Unknown
        );
    }

    #[test]
    fn example_6_3_cross_method() {
        // Spec example 6.3
        let classifications = vec![
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),  // 1
            s(&[LineProperty::Annotation]),                            // 2
            s(&[LineProperty::Annotation]),                            // 3
            s(&[LineProperty::Statement]),                             // 4: Hit
            s(&[LineProperty::ScopeClose]),                            // 5
            s(&[LineProperty::Whitespace]),                            // 6
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),  // 7
            s(&[LineProperty::Statement]),                             // 8: Hit
            s(&[LineProperty::ScopeClose]),                            // 9
        ];
        let scopes = vec![
            Scope { open_line: 1, close_line: 5, parent: None, children: vec![] },
            Scope { open_line: 7, close_line: 9, parent: None, children: vec![] },
        ];
        let coverage = cov_hit(&[4, 8]);
        let ann = AnnotationSpan { start_line: 2, end_line: 3 };
        assert_eq!(
            is_annotation_executed(&ann, &classifications, &scopes, &coverage, 9),
            ExecutionStatus::Executed
        );
    }

    #[test]
    fn dangling_annotation_is_structural() {
        let classifications = vec![
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::ScopeClose]),
        ];
        let scopes = vec![];
        let coverage = CoverageReport::new();
        let ann = AnnotationSpan { start_line: 1, end_line: 1 };
        assert_eq!(
            is_annotation_executed(&ann, &classifications, &scopes, &coverage, 2),
            ExecutionStatus::Structural
        );
    }

    #[test]
    fn not_executed_statement() {
        let classifications = vec![
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::Statement]),  // Miss
            s(&[LineProperty::ScopeClose]),
        ];
        let scopes = vec![Scope { open_line: 1, close_line: 4, parent: None, children: vec![] }];
        let coverage = cov_miss(&[3]);
        let ann = AnnotationSpan { start_line: 2, end_line: 2 };
        assert_eq!(
            is_annotation_executed(&ann, &classifications, &scopes, &coverage, 4),
            ExecutionStatus::NotExecuted
        );
    }
}
