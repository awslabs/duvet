// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Java line classifier using tree-sitter-java.
//!
//! Maps tree-sitter CST node types to `LineProperty` sets per the plan's mapping table.
//! Returns `None` for lines the tree-sitter walk does not visit and that are not
//! blank or annotations (Decision 9).

use crate::query::classify::LineClassifier;
use duvet_coverage::types::{LineClass, LineProperty};
use std::collections::BTreeSet;

/// Java source classifier using tree-sitter.
pub struct JavaClassifier;

impl LineClassifier for JavaClassifier {
    fn classify(&self, source: &str) -> Vec<Option<LineClass>> {
        let lines: Vec<&str> = source.lines().collect();
        let line_count = lines.len();
        // Track which properties apply to each line (1-indexed, index 0 unused)
        let mut line_props: Vec<BTreeSet<LineProperty>> = vec![BTreeSet::new(); line_count + 1];
        let mut visited: Vec<bool> = vec![false; line_count + 1];
        // Track lines on which a real code/structural node *starts*. Used by the
        // mutual-exclusivity post-pass to tell a code line carrying a trailing
        // comment (`doX(); // note`) apart from an annotation/comment line merely
        // contaminated by a multi-line node that spans it.
        let mut code_start: Vec<bool> = vec![false; line_count + 1];

        // Mark blank lines and annotation lines first
        for (i, line) in lines.iter().enumerate() {
            let line_num = i + 1;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                line_props[line_num].insert(LineProperty::Whitespace);
                visited[line_num] = true;
            } else if trimmed.starts_with("//=") || trimmed.starts_with("//#") {
                line_props[line_num].insert(LineProperty::Annotation);
                visited[line_num] = true;
            }
        }

        // Parse with tree-sitter
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_java::LANGUAGE.into())
            .expect("Error loading Java grammar");

        let tree = match parser.parse(source, None) {
            Some(tree) => tree,
            None => return (0..line_count).map(|_| None).collect(),
        };

        // Walk the CST
        walk_node(
            &tree.root_node(),
            &lines,
            &mut line_props,
            &mut visited,
            &mut code_start,
        );

        // Post-processing: enforce mutual exclusivity contract (spec §1.3).
        // A line must be classified as *either* code (Statement/Declaration/
        // scope) *or* Annotation/Comment/Whitespace, never both. The ambiguous
        // lines are those carrying both a code property and a comment-ish one;
        // we resolve them by which is authoritative for that line:
        //
        //   * Annotation / Whitespace lines, and comment lines with no code
        //     node *starting* on them, are genuine non-code lines that a
        //     multi-line AST node (e.g. a fluent builder chain) merely spanned.
        //     Strip the spurious code properties.
        //
        //   * A line where real code starts but that also carries a trailing
        //     `//` comment (`doX(); // note`) is a code line. Strip the
        //     incidental Comment instead, so the verified model still sees the
        //     executable statement.
        for line_num in 1..=line_count {
            let props = &mut line_props[line_num];
            let has_annotation = props.contains(&LineProperty::Annotation);
            let has_whitespace = props.contains(&LineProperty::Whitespace);
            let has_comment = props.contains(&LineProperty::Comment);

            if has_annotation || has_whitespace || (has_comment && !code_start[line_num]) {
                // Non-code line (possibly spanned by a multi-line node): strip
                // the spurious code properties. Annotation/Whitespace/Comment
                // are left intact — they are the authoritative classification.
                props.remove(&LineProperty::Statement);
                props.remove(&LineProperty::Declaration);
                props.remove(&LineProperty::ScopeOpen);
                props.remove(&LineProperty::ScopeClose);
                props.remove(&LineProperty::NonLinearControl);
            } else if has_comment {
                // Trailing comment on a real code line (`doX(); // note`) —
                // code wins; drop the incidental Comment so the verified model
                // still sees the executable statement.
                props.remove(&LineProperty::Comment);
            }
        }

        // Build result: 0-indexed output, each element corresponds to line (i+1)
        (0..line_count)
            .map(|i| {
                let line_num = i + 1;
                if visited[line_num] {
                    Some(line_props[line_num].clone())
                } else {
                    None // Unvisited, non-blank, non-annotation → unknown
                }
            })
            .collect()
    }
}

fn walk_node(
    node: &tree_sitter::Node,
    lines: &[&str],
    line_props: &mut [BTreeSet<LineProperty>],
    visited: &mut [bool],
    code_start: &mut [bool],
) {
    let kind = node.kind();
    let start_line = node.start_position().row + 1; // tree-sitter is 0-indexed
    let end_line = node.end_position().row + 1;

    // Record that a real code/structural construct begins on `start_line`.
    // The mutual-exclusivity post-pass uses this to distinguish a code line
    // with a trailing comment from a comment line spanned by a multi-line node.
    if is_code_kind(kind) && start_line < code_start.len() {
        code_start[start_line] = true;
    }

    match kind {
        // Declarations that open scopes
        "class_declaration" | "interface_declaration" | "enum_declaration" => {
            mark_declaration_with_scope(node, lines, line_props, visited);
        }
        "method_declaration" | "constructor_declaration" => {
            mark_declaration_with_scope(node, lines, line_props, visited);
        }

        // Statements
        "expression_statement"
        | "return_statement"
        | "throw_statement"
        | "assert_statement"
        | "break_statement"
        | "continue_statement"
        | "yield_statement" => {
            mark_lines(
                start_line,
                end_line,
                LineProperty::Statement,
                line_props,
                visited,
            );
        }

        // Control flow statements that open scopes
        "if_statement"
        | "for_statement"
        | "enhanced_for_statement"
        | "while_statement"
        | "do_statement"
        | "switch_expression" => {
            mark_lines(
                start_line,
                start_line,
                LineProperty::Statement,
                line_props,
                visited,
            );
            // The block child handles ScopeOpen/ScopeClose
        }
        "try_statement" => {
            // try itself is structural, block child handles scope
            mark_lines(
                start_line,
                start_line,
                LineProperty::Declaration,
                line_props,
                visited,
            );
        }
        "catch_clause" => {
            mark_lines(
                start_line,
                start_line,
                LineProperty::Declaration,
                line_props,
                visited,
            );
        }
        "finally_clause" => {
            mark_lines(
                start_line,
                start_line,
                LineProperty::Declaration,
                line_props,
                visited,
            );
        }

        // Variable declarations
        "local_variable_declaration" | "field_declaration" => {
            let has_init =
                has_child_kind(node, "variable_declarator") && node_has_initializer(node);
            mark_lines(
                start_line,
                end_line,
                LineProperty::Declaration,
                line_props,
                visited,
            );
            if has_init {
                mark_lines(
                    start_line,
                    end_line,
                    LineProperty::Statement,
                    line_props,
                    visited,
                );
            }
        }

        // Blocks → ScopeOpen on first line, ScopeClose on last line
        "block" | "class_body" | "interface_body" | "enum_body" | "switch_block"
        | "constructor_body" => {
            mark_lines(
                start_line,
                start_line,
                LineProperty::ScopeOpen,
                line_props,
                visited,
            );
            if end_line != start_line {
                mark_lines(
                    end_line,
                    end_line,
                    LineProperty::ScopeClose,
                    line_props,
                    visited,
                );
            } else {
                // Single-line block: both open and close on same line
                mark_lines(
                    start_line,
                    start_line,
                    LineProperty::ScopeClose,
                    line_props,
                    visited,
                );
            }
        }

        // Comments
        "line_comment" => {
            // Check if it's a duvet annotation (already handled above)
            if !visited[start_line] || !line_props[start_line].contains(&LineProperty::Annotation) {
                mark_lines(
                    start_line,
                    end_line,
                    LineProperty::Comment,
                    line_props,
                    visited,
                );
            }
        }
        "block_comment" => {
            mark_lines(
                start_line,
                end_line,
                LineProperty::Comment,
                line_props,
                visited,
            );
        }

        // Import and package declarations
        "import_declaration" | "package_declaration" => {
            mark_lines(
                start_line,
                end_line,
                LineProperty::Declaration,
                line_props,
                visited,
            );
        }

        // Annotations like @Override
        "marker_annotation" | "annotation" => {
            mark_lines(
                start_line,
                end_line,
                LineProperty::Declaration,
                line_props,
                visited,
            );
        }

        // Enum constants
        "enum_constant" => {
            mark_lines(
                start_line,
                end_line,
                LineProperty::Declaration,
                line_props,
                visited,
            );
            if has_child_kind(node, "argument_list") {
                mark_lines(
                    start_line,
                    end_line,
                    LineProperty::Statement,
                    line_props,
                    visited,
                );
            }
        }

        // Labels (non-linear control)
        "labeled_statement" => {
            mark_lines(
                start_line,
                start_line,
                LineProperty::NonLinearControl,
                line_props,
                visited,
            );
        }

        _ => {}
    }

    // Recurse into children
    let mut cursor = node.walk();
    let children: Vec<_> = node.children(&mut cursor).collect();
    for child in children {
        walk_node(&child, lines, line_props, visited, code_start);
    }
}

/// Node kinds that represent real code or structure (as opposed to comments,
/// whitespace, or non-marking syntax). A line on which one of these *starts*
/// is treated as a code line even if it also carries a trailing comment.
fn is_code_kind(kind: &str) -> bool {
    matches!(
        kind,
        "class_declaration"
            | "interface_declaration"
            | "enum_declaration"
            | "method_declaration"
            | "constructor_declaration"
            | "expression_statement"
            | "return_statement"
            | "throw_statement"
            | "assert_statement"
            | "break_statement"
            | "continue_statement"
            | "yield_statement"
            | "if_statement"
            | "for_statement"
            | "enhanced_for_statement"
            | "while_statement"
            | "do_statement"
            | "switch_expression"
            | "try_statement"
            | "catch_clause"
            | "finally_clause"
            | "local_variable_declaration"
            | "field_declaration"
            | "block"
            | "class_body"
            | "interface_body"
            | "enum_body"
            | "switch_block"
            | "constructor_body"
            | "import_declaration"
            | "package_declaration"
            | "marker_annotation"
            | "annotation"
            | "enum_constant"
            | "labeled_statement"
    )
}

/// Marks a declaration node that may contain a block (scope).
fn mark_declaration_with_scope(
    node: &tree_sitter::Node,
    _lines: &[&str],
    line_props: &mut [BTreeSet<LineProperty>],
    visited: &mut [bool],
) {
    let start_line = node.start_position().row + 1;
    let end_line = node.end_position().row + 1;

    // Find the body/block child to determine where the scope opens
    let mut cursor = node.walk();
    let children: Vec<_> = node.children(&mut cursor).collect();
    let body = children.iter().find(|c| {
        matches!(
            c.kind(),
            "class_body" | "interface_body" | "enum_body" | "block" | "constructor_body"
        )
    });

    if let Some(body) = body {
        let body_start = body.start_position().row + 1;
        // Lines before the body are pure declaration
        for line in start_line..body_start {
            mark_lines(line, line, LineProperty::Declaration, line_props, visited);
        }
        // The body start line has both Declaration and ScopeOpen if it's the same as decl
        if body_start == start_line {
            mark_lines(
                start_line,
                start_line,
                LineProperty::Declaration,
                line_props,
                visited,
            );
        }
    } else {
        // No body (e.g., abstract method) — pure declaration
        mark_lines(
            start_line,
            end_line,
            LineProperty::Declaration,
            line_props,
            visited,
        );
    }
}

fn mark_lines(
    start: usize,
    end: usize,
    prop: LineProperty,
    line_props: &mut [BTreeSet<LineProperty>],
    visited: &mut [bool],
) {
    for line in start..=end {
        if line < line_props.len() {
            line_props[line].insert(prop);
            visited[line] = true;
        }
    }
}

fn has_child_kind(node: &tree_sitter::Node, kind: &str) -> bool {
    let mut cursor = node.walk();
    let result = node.children(&mut cursor).any(|c| c.kind() == kind);
    result
}

fn node_has_initializer(node: &tree_sitter::Node) -> bool {
    let mut cursor = node.walk();
    let children: Vec<_> = node.children(&mut cursor).collect();
    for child in children {
        if child.kind() == "variable_declarator" {
            let mut inner = child.walk();
            let grandchildren: Vec<_> = child.children(&mut inner).collect();
            for grandchild in grandchildren {
                if grandchild.kind() == "="
                    || grandchild.kind() == "object_creation_expression"
                    || grandchild.kind() == "method_invocation"
                    || grandchild.kind() == "array_creation_expression"
                {
                    return true;
                }
                if grandchild.is_named()
                    && grandchild.kind() != "identifier"
                    && grandchild.kind() != "dimensions"
                    && grandchild.kind() != "type_identifier"
                    && grandchild.kind() != "integral_type"
                    && grandchild.kind() != "floating_point_type"
                    && grandchild.kind() != "boolean_type"
                    && grandchild.kind() != "void_type"
                    && grandchild.kind() != "generic_type"
                    && grandchild.kind() != "array_type"
                    && grandchild.kind() != "scoped_type_identifier"
                {
                    return true;
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classify(source: &str) -> Vec<Option<LineClass>> {
        JavaClassifier.classify(source)
    }

    fn has_prop(class: &Option<LineClass>, prop: LineProperty) -> bool {
        class.as_ref().is_some_and(|c| c.contains(&prop))
    }

    fn is_exactly(class: &Option<LineClass>, props: &[LineProperty]) -> bool {
        match class {
            Some(c) => {
                let expected: BTreeSet<LineProperty> = props.iter().copied().collect();
                *c == expected
            }
            None => false,
        }
    }

    #[test]
    fn method_signature() {
        let source = "public class Foo {\n    public void foo() {\n        doX();\n    }\n}";
        let result = classify(source);
        // line 1: class Foo { → Declaration, ScopeOpen
        assert!(has_prop(&result[0], LineProperty::Declaration));
        assert!(has_prop(&result[0], LineProperty::ScopeOpen));
        // line 2: public void foo() { → Declaration, ScopeOpen
        assert!(has_prop(&result[1], LineProperty::Declaration));
        assert!(has_prop(&result[1], LineProperty::ScopeOpen));
        // line 3: doX(); → Statement
        assert!(has_prop(&result[2], LineProperty::Statement));
        // line 4: } → ScopeClose
        assert!(has_prop(&result[3], LineProperty::ScopeClose));
        // line 5: } → ScopeClose
        assert!(has_prop(&result[4], LineProperty::ScopeClose));
    }

    #[test]
    fn interface_declaration() {
        let source = "public interface IKeyring {\n    void onEncrypt();\n}";
        let result = classify(source);
        assert!(has_prop(&result[0], LineProperty::Declaration));
        assert!(has_prop(&result[0], LineProperty::ScopeOpen));
        assert!(has_prop(&result[1], LineProperty::Declaration));
        assert!(has_prop(&result[2], LineProperty::ScopeClose));
    }

    #[test]
    fn variable_with_init() {
        let source = "public class Foo {\n    void bar() {\n        int x = 5;\n    }\n}";
        let result = classify(source);
        // line 3: int x = 5; → Statement, Declaration
        assert!(has_prop(&result[2], LineProperty::Statement));
        assert!(has_prop(&result[2], LineProperty::Declaration));
    }

    #[test]
    fn variable_without_init() {
        let source = "public class Foo {\n    void bar() {\n        int x;\n    }\n}";
        let result = classify(source);
        // line 3: int x; → Declaration (no Statement)
        assert!(has_prop(&result[2], LineProperty::Declaration));
        assert!(!has_prop(&result[2], LineProperty::Statement));
    }

    #[test]
    fn closing_brace() {
        let source = "public class Foo {\n}";
        let result = classify(source);
        assert!(has_prop(&result[1], LineProperty::ScopeClose));
    }

    #[test]
    fn line_comment() {
        let source = "public class Foo {\n    // hello\n}";
        let result = classify(source);
        assert!(has_prop(&result[1], LineProperty::Comment));
    }

    #[test]
    fn java_annotation() {
        let source = "public class Foo {\n    @Override\n    public void bar() {\n    }\n}";
        let result = classify(source);
        assert!(has_prop(&result[1], LineProperty::Declaration));
    }

    #[test]
    fn import_declaration() {
        let source = "import java.util.List;";
        let result = classify(source);
        assert!(has_prop(&result[0], LineProperty::Declaration));
    }

    #[test]
    fn blank_line() {
        let source = "public class Foo {\n\n}";
        let result = classify(source);
        assert!(is_exactly(&result[1], &[LineProperty::Whitespace]));
    }

    #[test]
    fn duvet_annotation() {
        let source = "public class Foo {\n    //= spec.md#section-1\n    //# MUST do X\n}";
        let result = classify(source);
        assert!(has_prop(&result[1], LineProperty::Annotation));
        assert!(has_prop(&result[2], LineProperty::Annotation));
    }

    #[test]
    fn return_statement() {
        let source = "public class Foo {\n    int bar() {\n        return 42;\n    }\n}";
        let result = classify(source);
        assert!(has_prop(&result[2], LineProperty::Statement));
    }

    #[test]
    fn throw_statement() {
        let source =
            "public class Foo {\n    void bar() {\n        throw new RuntimeException();\n    }\n}";
        let result = classify(source);
        assert!(has_prop(&result[2], LineProperty::Statement));
    }

    #[test]
    fn enum_constant_with_args() {
        let source = "public enum Cipher {\n    AES_128(128, 12, 16);\n}";
        let result = classify(source);
        // AES_128(128, 12, 16); → Declaration, Statement
        assert!(has_prop(&result[1], LineProperty::Declaration));
        assert!(has_prop(&result[1], LineProperty::Statement));
    }

    #[test]
    fn enum_constant_without_args() {
        let source = "public enum Color {\n    RED,\n    GREEN\n}";
        let result = classify(source);
        assert!(has_prop(&result[1], LineProperty::Declaration));
        assert!(!has_prop(&result[1], LineProperty::Statement));
    }

    #[test]
    fn package_declaration() {
        let source = "package com.example;";
        let result = classify(source);
        assert!(has_prop(&result[0], LineProperty::Declaration));
    }

    #[test]
    fn field_with_init() {
        let source = "public class Foo {\n    private static final int X = 5;\n}";
        let result = classify(source);
        assert!(has_prop(&result[1], LineProperty::Declaration));
        assert!(has_prop(&result[1], LineProperty::Statement));
    }

    #[test]
    fn balanced_scopes_all_constructs() {
        let source = r#"public class AllScopes {
    static { setup(); }
    void method() {
        if (a) { doA(); }
        if (b) { doB(); } else { doC(); }
        for (int i = 0; i < 10; i++) { loop(); }
        while (cond) { spin(); }
        try { risky(); } catch (Exception e) { handle(); } finally { cleanup(); }
        switch (x) { case 1: break; }
        Runnable r = () -> { lambda(); };
        new Thread() { public void run() { anon(); } };
    }
    enum Inner { A, B, C }
    interface Nested { void foo(); }
}"#;
        let result = classify(source);
        let mut opens = 0;
        let mut closes = 0;
        for props in result.iter().flatten() {
            if props.contains(&LineProperty::ScopeOpen) {
                opens += 1;
            }
            if props.contains(&LineProperty::ScopeClose) {
                closes += 1;
            }
        }
        assert_eq!(
            opens, closes,
            "ScopeOpen ({opens}) and ScopeClose ({closes}) count must match"
        );
    }

    #[test]
    fn balanced_scopes_try_catch_finally() {
        let source = r#"public class T {
    void foo() {
        try {
            a();
        } catch (IOException e) {
            b();
        } catch (RuntimeException e) {
            c();
        } finally {
            d();
        }
    }
}"#;
        let result = classify(source);
        let mut opens = 0;
        let mut closes = 0;
        for props in result.iter().flatten() {
            if props.contains(&LineProperty::ScopeOpen) {
                opens += 1;
            }
            if props.contains(&LineProperty::ScopeClose) {
                closes += 1;
            }
        }
        assert_eq!(
            opens, closes,
            "ScopeOpen ({opens}) and ScopeClose ({closes}) count must match"
        );
    }

    #[test]
    fn field_without_init() {
        let source = "public class Foo {\n    private int x;\n}";
        let result = classify(source);
        assert!(has_prop(&result[1], LineProperty::Declaration));
        assert!(!has_prop(&result[1], LineProperty::Statement));
    }

    /// Annotation lines inside a multi-line fluent builder chain must be
    /// pure {Annotation}, not contaminated with Statement/Declaration from
    /// the parent AST node (spec §1.3 mutual exclusivity contract).
    #[test]
    fn annotation_inside_builder_chain_not_contaminated() {
        let source = r#"public class Foo {
    void bar() {
        Object keyring = SomeBuilder.builder()
            //= spec.md#section-1
            //# MUST do X
            .keyNamespace("myNamespace")
            .build();
    }
}"#;
        let result = classify(source);
        // line 4: //= spec.md#section-1 → must be pure {Annotation}
        assert!(is_exactly(&result[3], &[LineProperty::Annotation]));
        // line 5: //# MUST do X → must be pure {Annotation}
        assert!(is_exactly(&result[4], &[LineProperty::Annotation]));
        // Must NOT have Statement or Declaration
        assert!(!has_prop(&result[3], LineProperty::Statement));
        assert!(!has_prop(&result[3], LineProperty::Declaration));
        assert!(!has_prop(&result[4], LineProperty::Statement));
        assert!(!has_prop(&result[4], LineProperty::Declaration));
    }

    /// A statement carrying a trailing `//` comment is a code line: the
    /// mutual-exclusivity post-pass must drop the incidental Comment, not the
    /// Statement. Otherwise the verified model sees a non-executable line where
    /// real, coverage-hit code lives and reports a false "not executed".
    #[test]
    fn trailing_comment_on_statement_keeps_statement() {
        let source = "public class Foo {\n    void bar() {\n        doX(); // x\n    }\n}";
        let result = classify(source);
        // line 3: `doX(); // x` — Statement must survive; Comment is incidental.
        assert!(
            has_prop(&result[2], LineProperty::Statement),
            "trailing comment stripped the Statement: {:?}",
            result[2]
        );
        assert!(!has_prop(&result[2], LineProperty::Comment));
    }

    /// A comment on its own line remains a pure Comment line (regression guard
    /// so the trailing-comment fix does not over-broaden and start treating
    /// stand-alone comments as code).
    #[test]
    fn standalone_comment_stays_comment() {
        let source = "public class Foo {\n    // just a note\n    void bar() {}\n}";
        let result = classify(source);
        assert!(is_exactly(&result[1], &[LineProperty::Comment]));
    }
}
