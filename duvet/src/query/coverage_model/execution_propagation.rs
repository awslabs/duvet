// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Phase 2: Execution Propagation (spec Section 3).
//!
//! Given the set of lines reported as executed by the coverage tool, computes the
//! full set of lines that can be considered "executed by association."

use std::collections::BTreeSet;

use super::types::*;

/// Computes the execution set by backward propagation from directly-hit lines
/// (spec Section 3.3).
///
/// For each scope, walks backward from each directly-executed line, propagating
/// execution to Declaration, Whitespace, Comment, Annotation, and ScopeOpen lines.
/// Stops at ScopeClose, Statement, unknown (None), or ScopeOpen (include then stop).
/// Skips entire scope if it contains NonLinearControl.
pub fn execution_set(
    classifications: &[Option<LineClass>],
    scopes: &[Scope],
    coverage: &CoverageReport,
) -> BTreeSet<u64> {
    let directly_executed: BTreeSet<u64> = coverage
        .iter()
        .filter(|(_, status)| **status == CoverageStatus::Hit)
        .map(|(line, _)| *line)
        .collect();

    let mut result = directly_executed.clone();

    for scope in scopes {
        // Check if scope contains NonLinearControl
        let has_non_linear = (scope.open_line..=scope.close_line).any(|line| {
            let idx = (line - 1) as usize;
            if idx < classifications.len() {
                if let Some(props) = &classifications[idx] {
                    return props.contains(&LineProperty::NonLinearControl);
                }
            }
            false
        });

        if has_non_linear {
            continue;
        }

        // For each directly executed line in this scope
        for &exec_line in &directly_executed {
            if exec_line < scope.open_line || exec_line > scope.close_line {
                continue;
            }

            // Walk backward from exec_line
            let mut current = exec_line.saturating_sub(1);

            while current >= scope.open_line {
                let idx = (current - 1) as usize;
                if idx >= classifications.len() {
                    break;
                }

                match &classifications[idx] {
                    None => {
                        // Unknown line — cannot propagate through it
                        break;
                    }
                    Some(props) => {
                        if props.contains(&LineProperty::ScopeClose) {
                            break;
                        }

                        if props.contains(&LineProperty::Statement) {
                            break;
                        }

                        // Propagate to this line
                        result.insert(current);

                        if props.contains(&LineProperty::ScopeOpen) {
                            // Include it but stop
                            break;
                        }

                        if current == 0 {
                            break;
                        }
                        current -= 1;
                    }
                }
            }
        }
    }

    result
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

    #[test]
    fn propagates_backward_through_declaration() {
        // Spec example 6.1: method signature + executed statement
        let classifications = vec![
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),  // line 1
            s(&[LineProperty::Declaration]),                           // line 2
            s(&[LineProperty::Statement]),                             // line 3: Hit
            s(&[LineProperty::ScopeClose]),                            // line 4
        ];
        let scopes = vec![Scope { open_line: 1, close_line: 4, parent: None, children: vec![] }];
        let coverage = cov_hit(&[3]);
        let result = execution_set(&classifications, &scopes, &coverage);
        assert!(result.contains(&1)); // ScopeOpen propagated
        assert!(result.contains(&2)); // Declaration propagated
        assert!(result.contains(&3)); // directly hit
        assert!(!result.contains(&4)); // ScopeClose not propagated
    }

    #[test]
    fn stops_at_scope_close() {
        let classifications = vec![
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),
            s(&[LineProperty::Statement]),  // Hit
            s(&[LineProperty::ScopeClose]),
            s(&[LineProperty::Whitespace]),
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),
            s(&[LineProperty::Statement]),  // Hit
            s(&[LineProperty::ScopeClose]),
        ];
        let scopes = vec![
            Scope { open_line: 1, close_line: 3, parent: None, children: vec![] },
            Scope { open_line: 5, close_line: 7, parent: None, children: vec![] },
        ];
        let coverage = cov_hit(&[6]);
        let result = execution_set(&classifications, &scopes, &coverage);
        assert!(result.contains(&5)); // propagated in scope 2
        assert!(result.contains(&6)); // directly hit
        assert!(!result.contains(&4)); // whitespace between scopes not propagated
        assert!(!result.contains(&1)); // scope 1 not affected
    }

    #[test]
    fn stops_at_statement() {
        let classifications = vec![
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),
            s(&[LineProperty::Statement]),  // line 2: another statement
            s(&[LineProperty::Declaration]),
            s(&[LineProperty::Statement]),  // line 4: Hit
            s(&[LineProperty::ScopeClose]),
        ];
        let scopes = vec![Scope { open_line: 1, close_line: 5, parent: None, children: vec![] }];
        let coverage = cov_hit(&[4]);
        let result = execution_set(&classifications, &scopes, &coverage);
        assert!(result.contains(&3)); // Declaration propagated
        assert!(result.contains(&4)); // directly hit
        assert!(!result.contains(&2)); // Statement blocks propagation
    }

    #[test]
    fn stops_at_unknown() {
        // Spec example 6.8
        let classifications = vec![
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::Statement, LineProperty::Declaration]),  // line 4: Hit
            None,  // line 5: unknown
            s(&[LineProperty::Statement]),  // line 6: Hit
            s(&[LineProperty::ScopeClose]),
        ];
        let scopes = vec![Scope { open_line: 1, close_line: 7, parent: None, children: vec![] }];
        let coverage = cov_hit(&[4, 6]);
        let result = execution_set(&classifications, &scopes, &coverage);
        // From line 6: backward walk hits None at line 5 → break
        assert!(!result.contains(&5));
        // From line 4: backward walk propagates through annotations
        assert!(result.contains(&3));
        assert!(result.contains(&2));
        assert!(result.contains(&1));
    }

    #[test]
    fn no_propagation_with_non_linear_control() {
        // Spec example 6.6
        let classifications = vec![
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::Declaration]),
            s(&[LineProperty::NonLinearControl, LineProperty::Statement]),  // Hit
            s(&[LineProperty::Statement]),
            s(&[LineProperty::NonLinearControl]),
            s(&[LineProperty::Statement]),  // Hit
            s(&[LineProperty::ScopeClose]),
        ];
        let scopes = vec![Scope { open_line: 1, close_line: 9, parent: None, children: vec![] }];
        let coverage = cov_hit(&[5, 8]);
        let result = execution_set(&classifications, &scopes, &coverage);
        // Only directly hit lines
        assert_eq!(result, BTreeSet::from([5, 8]));
    }

    #[test]
    fn scope_open_included_then_stop() {
        let classifications = vec![
            s(&[LineProperty::Whitespace]),  // line 1: outside scope
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),  // line 2
            s(&[LineProperty::Declaration]),  // line 3
            s(&[LineProperty::Statement]),   // line 4: Hit
            s(&[LineProperty::ScopeClose]),  // line 5
        ];
        let scopes = vec![Scope { open_line: 2, close_line: 5, parent: None, children: vec![] }];
        let coverage = cov_hit(&[4]);
        let result = execution_set(&classifications, &scopes, &coverage);
        assert!(result.contains(&2)); // ScopeOpen included
        assert!(result.contains(&3)); // Declaration propagated
        assert!(!result.contains(&1)); // Outside scope
    }
}
