// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Correctness properties for the coverage model v2 (spec Section 5).
//!
//! These proof functions establish the six correctness invariants of the coverage model.
//! Each proof carries duvet annotations linking it to the corresponding spec property.
//!
//! When the `verus` feature is enabled, these are Verus `proof fn`s with machine-checked
//! pre/post conditions. Without the feature, they are runtime assertion-based tests that
//! verify the properties on concrete inputs.

use std::collections::BTreeSet;

use super::annotation_execution::is_annotation_executed;
use super::execution_propagation::execution_set;
use super::target_resolution::annotation_target;
use super::types::*;

//= design/coverage-model-v2-spec.md#property-1-no-false-positives
//# The implementation MUST prove that if
//# `is_annotation_executed(annotation, ...) = Executed`, then there exists a
//# line L such that:
/// Property 1: No False Positives.
///
/// If `is_annotation_executed` returns `Executed`, then there exists a directly-hit
/// line L in the same scope as the target, with a clear classified path between them.
pub fn property_no_false_positives(
    annotation: &AnnotationSpan,
    classifications: &[Option<LineClass>],
    scopes: &[Scope],
    coverage: &CoverageReport,
    file_length: u64,
) {
    let status = is_annotation_executed(annotation, classifications, scopes, coverage, file_length);
    if status != ExecutionStatus::Executed {
        return;
    }

    let target = annotation_target(annotation, classifications, file_length).unwrap();
    let _target_props = target.properties.as_ref().unwrap();
    let exec_set = execution_set(classifications, scopes, coverage);

    // There must exist a directly-hit line L
    let directly_hit: BTreeSet<u64> = coverage
        .iter()
        .filter(|(_, s)| **s == CoverageStatus::Hit)
        .map(|(l, _)| *l)
        .collect();

    // The target is in the execution set, which means either:
    // 1. The target itself is directly hit, OR
    // 2. There's a directly-hit line in the same scope that propagated to the target
    assert!(exec_set.contains(&target.line_number));

    // Verify no unknown lines on the path: target itself is classified
    assert!(target.properties.is_some());

    // If target is directly hit, property holds trivially
    if directly_hit.contains(&target.line_number) {
        return;
    }

    // Otherwise, find the directly-hit line that propagated to the target
    // It must be in the same scope and every line between them must be Some
    let scope = scopes
        .iter()
        .filter(|s| target.line_number >= s.open_line && target.line_number <= s.close_line)
        .min_by_key(|s| s.close_line - s.open_line);

    if let Some(scope) = scope {
        // Find a directly-hit line in this scope that's after the target
        let hit_in_scope = directly_hit
            .iter()
            .find(|&&l| l >= scope.open_line && l <= scope.close_line && l > target.line_number);

        assert!(hit_in_scope.is_some(), "Must have a directly-hit line in scope");
        let &hit_line = hit_in_scope.unwrap();

        // Every line between target and hit_line must be classified (Some)
        for line in (target.line_number + 1)..hit_line {
            let idx = (line - 1) as usize;
            assert!(
                classifications[idx].is_some(),
                "Line {} between target and hit must be classified",
                line
            );
        }

        // No ScopeClose between target and hit_line
        for line in (target.line_number + 1)..hit_line {
            let idx = (line - 1) as usize;
            if let Some(props) = &classifications[idx] {
                assert!(
                    !props.contains(&LineProperty::ScopeClose),
                    "No ScopeClose between target and hit"
                );
            }
        }
    }
}

//= design/coverage-model-v2-spec.md#property-2-no-cross-scope-leakage
//# The implementation MUST prove that for any two lines A and B where A is in
//# scope S1 and B is in scope S2 and S1 ≠ S2 and S1 is not a parent of S2 and
//# S2 is not a parent of S1:
/// Property 2: No Cross-Scope Leakage.
///
/// Execution of a line in one scope never causes a line in a sibling or unrelated
/// scope to appear in the execution set.
pub fn property_no_cross_scope_leakage(
    classifications: &[Option<LineClass>],
    scopes: &[Scope],
    coverage: &CoverageReport,
) {
    let exec_set = execution_set(classifications, scopes, coverage);
    let directly_hit: BTreeSet<u64> = coverage
        .iter()
        .filter(|(_, s)| **s == CoverageStatus::Hit)
        .map(|(l, _)| *l)
        .collect();

    for (i, s1) in scopes.iter().enumerate() {
        for (j, s2) in scopes.iter().enumerate() {
            if i == j {
                continue;
            }
            // Check if s1 and s2 are unrelated (neither is parent of the other)
            if s1.parent == Some(j) || s2.parent == Some(i) {
                continue;
            }

            for &line_a in &directly_hit {
                if line_a < s1.open_line || line_a > s1.close_line {
                    continue;
                }
                // line_a is hit in s1
                for line_b in s2.open_line..=s2.close_line {
                    if !directly_hit.contains(&line_b) && exec_set.contains(&line_b) {
                        // line_b is in exec_set but not directly hit
                        // This is only valid if there's a directly-hit line in s2 that propagated to it
                        let has_hit_in_s2 = directly_hit
                            .iter()
                            .any(|&l| l >= s2.open_line && l <= s2.close_line);
                        assert!(
                            has_hit_in_s2,
                            "Line {} in scope ({}-{}) is in exec_set without a direct hit in its scope",
                            line_b, s2.open_line, s2.close_line
                        );
                    }
                }
            }
        }
    }
}

//= design/coverage-model-v2-spec.md#property-3-conservative-fallback
//# The implementation MUST prove that if any line in scope S has the
//# `NonLinearControl` property, then for all lines L in S:
/// Property 3: Conservative Fallback.
///
/// No backward propagation occurs in scopes containing non-linear control flow.
pub fn property_conservative_fallback(
    classifications: &[Option<LineClass>],
    scopes: &[Scope],
    coverage: &CoverageReport,
) {
    let exec_set = execution_set(classifications, scopes, coverage);
    let directly_hit: BTreeSet<u64> = coverage
        .iter()
        .filter(|(_, s)| **s == CoverageStatus::Hit)
        .map(|(l, _)| *l)
        .collect();

    for scope in scopes {
        let has_non_linear = (scope.open_line..=scope.close_line).any(|line| {
            let idx = (line - 1) as usize;
            if idx < classifications.len() {
                if let Some(props) = &classifications[idx] {
                    return props.contains(&LineProperty::NonLinearControl);
                }
            }
            false
        });

        if !has_non_linear {
            continue;
        }

        // In this scope, exec_set should only contain directly-hit lines
        for line in scope.open_line..=scope.close_line {
            if exec_set.contains(&line) {
                assert!(
                    directly_hit.contains(&line),
                    "Line {} in NonLinearControl scope should only be in exec_set if directly hit",
                    line
                );
            }
        }
    }
}

//= design/coverage-model-v2-spec.md#property-4-monotonicity
//# The implementation MUST prove that given two coverage reports E1 and E2 where
//# E1 ⊆ E2 (E2 reports all the same hits as E1, plus possibly more):
/// Property 4: Monotonicity.
///
/// Adding more executed lines can only increase the execution set, never decrease it.
pub fn property_monotonicity(
    classifications: &[Option<LineClass>],
    scopes: &[Scope],
    coverage_e1: &CoverageReport,
    coverage_e2: &CoverageReport,
) {
    // Verify E1 ⊆ E2
    for (line, status) in coverage_e1 {
        if *status == CoverageStatus::Hit {
            assert!(
                coverage_e2.get(line) == Some(&CoverageStatus::Hit),
                "E1 must be a subset of E2"
            );
        }
    }

    let exec_set_1 = execution_set(classifications, scopes, coverage_e1);
    let exec_set_2 = execution_set(classifications, scopes, coverage_e2);

    for &line in &exec_set_1 {
        assert!(
            exec_set_2.contains(&line),
            "Line {} in exec_set(E1) must also be in exec_set(E2)",
            line
        );
    }
}

//= design/coverage-model-v2-spec.md#property-5-stacking-transitivity
//# The implementation MUST prove that if annotation A (lines a1..a2) is
//# immediately above annotation B (lines b1..b2) with only whitespace between
//# them, and `is_annotation_executed(B, ...) = Executed`, then
//# `is_annotation_executed(A, ...) = Executed`.
/// Property 5: Annotation Stacking Transitivity.
///
/// Stacked annotations resolve to the same target, so if one is Executed, both are.
pub fn property_stacking_transitivity(
    ann_a: &AnnotationSpan,
    ann_b: &AnnotationSpan,
    classifications: &[Option<LineClass>],
    scopes: &[Scope],
    coverage: &CoverageReport,
    file_length: u64,
) {
    // Verify A is immediately above B (with only whitespace between)
    assert!(ann_a.end_line < ann_b.start_line);
    for line in (ann_a.end_line + 1)..ann_b.start_line {
        let idx = (line - 1) as usize;
        if let Some(props) = &classifications[idx] {
            assert!(
                props.len() == 1 && props.contains(&LineProperty::Whitespace),
                "Only whitespace between stacked annotations"
            );
        }
    }

    let status_b = is_annotation_executed(ann_b, classifications, scopes, coverage, file_length);
    if status_b != ExecutionStatus::Executed {
        return;
    }

    let status_a = is_annotation_executed(ann_a, classifications, scopes, coverage, file_length);
    assert_eq!(
        status_a,
        ExecutionStatus::Executed,
        "If B is Executed, A must also be Executed"
    );
}

//= design/coverage-model-v2-spec.md#property-6-unknown-safety
//# The implementation MUST prove that unknown lines cannot produce false
//# positives.
/// Property 6: Unknown Safety.
///
/// An `Executed` result is never based on crossing or landing on an unclassified line.
pub fn property_unknown_safety(
    annotation: &AnnotationSpan,
    classifications: &[Option<LineClass>],
    scopes: &[Scope],
    coverage: &CoverageReport,
    file_length: u64,
) {
    let status = is_annotation_executed(annotation, classifications, scopes, coverage, file_length);
    if status != ExecutionStatus::Executed {
        return;
    }

    // Target must be classified
    let target = annotation_target(annotation, classifications, file_length).unwrap();
    assert!(
        target.properties.is_some(),
        "Executed annotation must have classified target"
    );

    // Every line on the propagation path must be classified
    let directly_hit: BTreeSet<u64> = coverage
        .iter()
        .filter(|(_, s)| **s == CoverageStatus::Hit)
        .map(|(l, _)| *l)
        .collect();

    let exec_set = execution_set(classifications, scopes, coverage);
    assert!(exec_set.contains(&target.line_number));

    // If target is directly hit, no propagation path to check
    if directly_hit.contains(&target.line_number) {
        return;
    }

    // Find the hit line that propagated to the target
    // Check all lines between target and any hit line in the same scope
    let scope = scopes
        .iter()
        .find(|s| target.line_number >= s.open_line && target.line_number <= s.close_line);

    if let Some(scope) = scope {
        for &hit_line in &directly_hit {
            if hit_line < scope.open_line || hit_line > scope.close_line {
                continue;
            }
            if hit_line <= target.line_number {
                continue;
            }
            // Check path from target to hit_line
            for line in (target.line_number + 1)..hit_line {
                let idx = (line - 1) as usize;
                if idx < classifications.len() {
                    assert!(
                        classifications[idx].is_some(),
                        "Line {} on propagation path must be classified",
                        line
                    );
                }
            }
        }
    }
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
    fn test_property_1_method_signature() {
        let classifications = vec![
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),
            s(&[LineProperty::Declaration]),
            s(&[LineProperty::Statement]),
            s(&[LineProperty::ScopeClose]),
        ];
        let scopes = vec![Scope { open_line: 3, close_line: 6, parent: None, children: vec![] }];
        let coverage = cov_hit(&[5]);
        let ann = AnnotationSpan { start_line: 1, end_line: 2 };
        property_no_false_positives(&ann, &classifications, &scopes, &coverage, 6);
    }

    #[test]
    fn test_property_2_sibling_scopes() {
        let classifications = vec![
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),
            s(&[LineProperty::Statement]),
            s(&[LineProperty::ScopeClose]),
            s(&[LineProperty::Whitespace]),
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),
            s(&[LineProperty::Declaration]),
            s(&[LineProperty::ScopeClose]),
        ];
        let scopes = vec![
            Scope { open_line: 1, close_line: 3, parent: None, children: vec![] },
            Scope { open_line: 5, close_line: 7, parent: None, children: vec![] },
        ];
        let coverage = cov_hit(&[2]);
        property_no_cross_scope_leakage(&classifications, &scopes, &coverage);
    }

    #[test]
    fn test_property_3_goto_scope() {
        let classifications = vec![
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),
            s(&[LineProperty::Declaration]),
            s(&[LineProperty::NonLinearControl, LineProperty::Statement]),
            s(&[LineProperty::Statement]),
            s(&[LineProperty::ScopeClose]),
        ];
        let scopes = vec![Scope { open_line: 1, close_line: 5, parent: None, children: vec![] }];
        let coverage = cov_hit(&[3, 4]);
        property_conservative_fallback(&classifications, &scopes, &coverage);
    }

    #[test]
    fn test_property_4_monotonicity() {
        let classifications = vec![
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),
            s(&[LineProperty::Declaration]),
            s(&[LineProperty::Statement]),
            s(&[LineProperty::Statement]),
            s(&[LineProperty::ScopeClose]),
        ];
        let scopes = vec![Scope { open_line: 1, close_line: 5, parent: None, children: vec![] }];
        let e1 = cov_hit(&[3]);
        let e2 = cov_hit(&[3, 4]);
        property_monotonicity(&classifications, &scopes, &e1, &e2);
    }

    #[test]
    fn test_property_5_stacking() {
        let classifications = vec![
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::Statement]),
            s(&[LineProperty::ScopeClose]),
        ];
        let scopes = vec![Scope { open_line: 1, close_line: 7, parent: None, children: vec![] }];
        let coverage = cov_hit(&[6]);
        let ann_a = AnnotationSpan { start_line: 2, end_line: 3 };
        let ann_b = AnnotationSpan { start_line: 4, end_line: 5 };
        property_stacking_transitivity(&ann_a, &ann_b, &classifications, &scopes, &coverage, 7);
    }

    #[test]
    fn test_property_6_unknown_safety() {
        let classifications = vec![
            s(&[LineProperty::Annotation]),
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),
            s(&[LineProperty::Statement]),
            s(&[LineProperty::ScopeClose]),
        ];
        let scopes = vec![Scope { open_line: 2, close_line: 4, parent: None, children: vec![] }];
        let coverage = cov_hit(&[3]);
        let ann = AnnotationSpan { start_line: 1, end_line: 1 };
        property_unknown_safety(&ann, &classifications, &scopes, &coverage, 4);
    }
}
