// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Scope analysis (spec Section 1.5).
//!
//! Builds a scope tree from line classifications by matching ScopeOpen and ScopeClose
//! lines (balanced parentheses).

use super::types::*;

/// Builds a scope tree from classifications (spec Section 1.5).
///
/// Pairs ScopeOpen and ScopeClose lines into balanced scopes. Only `Some`
/// classifications contribute. Unknown (`None`) lines are ignored.
///
/// Returns a fallback single file-level scope on unbalanced braces.
pub fn build_scope_tree(classifications: &[Option<LineClass>], file_length: u64) -> Vec<Scope> {
    let mut stack: Vec<(u64, usize)> = Vec::new(); // (open_line, scope_index)
    let mut scopes: Vec<Scope> = Vec::new();

    for line_num in 1..=file_length {
        let idx = (line_num - 1) as usize;
        if idx >= classifications.len() {
            break;
        }

        if let Some(props) = &classifications[idx] {
            if props.contains(&LineProperty::ScopeOpen) {
                let scope_idx = scopes.len();
                let parent = stack.last().map(|&(_, idx)| idx);
                scopes.push(Scope {
                    open_line: line_num,
                    close_line: 0, // filled when we find the matching close
                    parent,
                    children: vec![],
                });
                if let Some(&(_, parent_idx)) = stack.last() {
                    scopes[parent_idx].children.push(scope_idx);
                }
                stack.push((line_num, scope_idx));
            }

            if props.contains(&LineProperty::ScopeClose) && !props.contains(&LineProperty::ScopeOpen) {
                if let Some((_, scope_idx)) = stack.pop() {
                    scopes[scope_idx].close_line = line_num;
                } else {
                    // Unbalanced: more closes than opens → fallback
                    return fallback_scope(file_length);
                }
            }
        }
    }

    if !stack.is_empty() {
        // Unbalanced: more opens than closes → fallback
        return fallback_scope(file_length);
    }

    // Add implicit file-level scope if there are lines not in any scope
    if scopes.is_empty() {
        return vec![Scope {
            open_line: 1,
            close_line: file_length,
            parent: None,
            children: vec![],
        }];
    }

    scopes
}

fn fallback_scope(file_length: u64) -> Vec<Scope> {
    vec![Scope {
        open_line: 1,
        close_line: file_length,
        parent: None,
        children: vec![],
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(props: &[LineProperty]) -> Option<LineClass> {
        Some(line_class(props))
    }

    #[test]
    fn simple_method_in_class() {
        let classifications = vec![
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),  // 1: class {
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),  // 2: method {
            s(&[LineProperty::Statement]),                             // 3
            s(&[LineProperty::ScopeClose]),                            // 4: }
            s(&[LineProperty::ScopeClose]),                            // 5: }
        ];
        let scopes = build_scope_tree(&classifications, 5);
        assert_eq!(scopes.len(), 2);
        assert_eq!(scopes[0].open_line, 1);
        assert_eq!(scopes[0].close_line, 5);
        assert_eq!(scopes[1].open_line, 2);
        assert_eq!(scopes[1].close_line, 4);
        assert_eq!(scopes[0].children, vec![1]);
        assert_eq!(scopes[1].parent, Some(0));
    }

    #[test]
    fn sibling_methods() {
        let classifications = vec![
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),  // 1: class {
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),  // 2: foo {
            s(&[LineProperty::ScopeClose]),                            // 3: }
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),  // 4: bar {
            s(&[LineProperty::ScopeClose]),                            // 5: }
            s(&[LineProperty::ScopeClose]),                            // 6: }
        ];
        let scopes = build_scope_tree(&classifications, 6);
        assert_eq!(scopes.len(), 3);
        assert_eq!(scopes[0].children, vec![1, 2]);
    }

    #[test]
    fn unbalanced_fallback() {
        let classifications = vec![
            s(&[LineProperty::ScopeOpen]),
            s(&[LineProperty::Statement]),
            // missing ScopeClose
        ];
        let scopes = build_scope_tree(&classifications, 2);
        assert_eq!(scopes.len(), 1);
        assert_eq!(scopes[0].open_line, 1);
        assert_eq!(scopes[0].close_line, 2);
    }

    #[test]
    fn empty_file() {
        let classifications: Vec<Option<LineClass>> = vec![];
        let scopes = build_scope_tree(&classifications, 0);
        // file_length 0 means no lines, but we still get a scope
        assert_eq!(scopes.len(), 1);
    }

    #[test]
    fn unknown_lines_ignored() {
        let classifications = vec![
            s(&[LineProperty::Declaration, LineProperty::ScopeOpen]),  // 1
            None,                                                      // 2: unknown
            s(&[LineProperty::Statement]),                             // 3
            s(&[LineProperty::ScopeClose]),                            // 4
        ];
        let scopes = build_scope_tree(&classifications, 4);
        assert_eq!(scopes.len(), 1);
        assert_eq!(scopes[0].open_line, 1);
        assert_eq!(scopes[0].close_line, 4);
    }

    #[test]
    fn four_level_nesting() {
        let classifications = vec![
            s(&[LineProperty::ScopeOpen]),   // 1
            s(&[LineProperty::ScopeOpen]),   // 2
            s(&[LineProperty::ScopeOpen]),   // 3
            s(&[LineProperty::ScopeOpen]),   // 4
            s(&[LineProperty::ScopeClose]),  // 5
            s(&[LineProperty::ScopeClose]),  // 6
            s(&[LineProperty::ScopeClose]),  // 7
            s(&[LineProperty::ScopeClose]),  // 8
        ];
        let scopes = build_scope_tree(&classifications, 8);
        assert_eq!(scopes.len(), 4);
        assert_eq!(scopes[3].open_line, 4);
        assert_eq!(scopes[3].close_line, 5);
    }
}
