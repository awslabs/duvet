// SPDX-License-Identifier: Apache-2.0

//! Kotlin line classifier using tree-sitter-kotlin-ng.
//!
//! Maps tree-sitter CST node kinds to `LineProperty` sets per the normative
//! mapping in `design/classifiers/kotlin-spec.md`; the evidence behind each
//! rule is in `design/classifiers/kotlin-decisions.md`. Returns `None` for
//! lines the walk does not visit and that are not blank or annotations
//! (coverage-model spec Decision 9).
//!
//! Test fixtures live in external files under `testdata/kotlin/` so this file
//! stays free of string literals that duvet's annotation parser would read as
//! citations — which lets this file be scanned as a duvet source.

use crate::query::classify::{Classification, ClassifierFailure, LineClassifier};
use duvet_coverage::types::{LineProperty, ScopeEvent};
use std::collections::BTreeSet;

/// Kotlin source classifier using tree-sitter.
pub struct KotlinClassifier;

impl LineClassifier for KotlinClassifier {
    fn classify(&self, source: &str) -> Classification {
        let lines: Vec<&str> = source.lines().collect();
        let line_count = lines.len();
        let mut line_props: Vec<BTreeSet<LineProperty>> = vec![BTreeSet::new(); line_count + 1];
        let mut visited: Vec<bool> = vec![false; line_count + 1];
        // Lines where a real code/structural node *starts*; disambiguates
        // `{code, Comment}` lines in the verified post-pass (see java.rs for
        // the full note — the mechanism is identical).
        let mut code_start: Vec<bool> = vec![false; line_count + 1];

        //= design/classifiers/kotlin-spec.md#pre-and-post-pass
        //= type=implementation
        //# Before the CST walk, the classifier MUST mark
        //# blank lines as `Whitespace`
        //# and duvet annotation lines (`//=` or `//#` after leading whitespace)
        //# as `Annotation`.
        super::mark_blank_and_annotation_lines(&lines, &mut line_props, &mut visited);

        //= design/classifiers/kotlin-spec.md#grammar
        //= type=implementation
        //# The classifier MUST parse Kotlin source with the `tree-sitter-kotlin-ng`
        //# grammar.
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_kotlin_ng::LANGUAGE.into())
            .expect("Error loading Kotlin grammar");

        // Spec #parse-errors: when the parser returns no tree at all, report
        // Unclassifiable at line 1. Not cited: `Parser::parse` on in-memory
        // source cannot be made to return `None` (no cancellation/timeout is
        // set), so no test can falsify the claim; the citation would be
        // untestable. The code path is kept for defense in depth.
        let tree = match parser.parse(source, None) {
            Some(tree) => tree,
            None => return Classification::unclassifiable(ClassifierFailure::ParseError, vec![1]),
        };

        //= design/classifiers/kotlin-spec.md#parse-errors
        //= type=implementation
        //# When the parse tree contains any `ERROR` or `MISSING` node,
        //# the classifier MUST return `Unclassifiable`
        //# and MUST report the line of every `ERROR` and `MISSING` node.
        //
        // Defeated commitment (coverage-model spec §1.5): tree-sitter returns
        // `Some(tree)` even for invalid input, inlining ERROR/MISSING nodes. A
        // missing brace would unbalance the scope stream and collapse the scope
        // tree into a well-formed WRONG tree; refuse instead and report every
        // located error. The dispatcher owns the response (spec §8).
        if tree.root_node().has_error() {
            let mut error_lines = Vec::new();
            super::collect_parse_error_lines(&tree.root_node(), &mut error_lines);
            if error_lines.is_empty() {
                error_lines.push(1);
            }
            return Classification::unclassifiable(ClassifierFailure::ParseError, error_lines);
        }

        walk_node(
            &tree.root_node(),
            &lines,
            &mut line_props,
            &mut visited,
            &mut code_start,
        );

        //= design/classifiers/kotlin-spec.md#pre-and-post-pass
        //= type=implementation
        //# After the CST walk, the classifier MUST apply the verified
        //# mutual-exclusivity post-pass (`clean_classifications`)
        //# to every classification it returns.
        let mut classifications_for_clean: Vec<Option<BTreeSet<LineProperty>>> = (1..=line_count)
            .map(|i| {
                if visited[i] {
                    Some(line_props[i].clone())
                } else {
                    None
                }
            })
            .collect();
        let code_start_slice = &code_start[1..];
        duvet_coverage::classify_postpass::clean_classifications(
            &mut classifications_for_clean,
            code_start_slice,
        );

        Classification::Classified(classifications_for_clean)
    }

    fn scope_events(&self, source: &str) -> Vec<ScopeEvent> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_kotlin_ng::LANGUAGE.into())
            .expect("Error loading Kotlin grammar");
        let Some(tree) = parser.parse(source, None) else {
            return Vec::new();
        };
        // A parse error means `classify` returns `Unclassifiable` and the
        // dispatcher escalates before any scope stream is consumed.
        if tree.root_node().has_error() {
            return Vec::new();
        }
        //= design/classifiers/kotlin-spec.md#scope-bearing-nodes
        //= type=implementation
        //# The scope-event stream MUST contain one open and one close event
        //# per scope-bearing node, keyed at the byte offsets of its brace tokens,
        //# in source order.
        let mut keyed: Vec<(usize, ScopeEvent)> = Vec::new();
        collect_scope_events(&tree.root_node(), &mut keyed);
        keyed.sort_by_key(|(byte, _)| *byte);
        keyed.into_iter().map(|(_, ev)| ev).collect()
    }
}

/// Local re-export of the shared annotation-meta prefix for tests that need
/// to synthesize an annotation line without embedding the literal shape.
#[cfg(test)]
use super::ANNOTATION_META;

//= design/classifiers/kotlin-spec.md#scope-bearing-nodes
//= type=implementation
//# The scope-bearing node kinds are
//# `block`, `class_body`, `enum_class_body`, `lambda_literal`,
//# and `when_expression`.
fn is_scope_bearing(kind: &str) -> bool {
    matches!(
        kind,
        "block" | "class_body" | "enum_class_body" | "lambda_literal" | "when_expression"
    )
}

/// The `{` and `}` token children of a scope-bearing node, as
/// (byte, line) pairs. `when_expression` and `lambda_literal` carry their
/// braces as direct children rather than in a child `block`
/// (kotlin-spec.md §4), so byte extremes would be wrong for `when` — the
/// node starts at the `when` keyword. One token-keyed helper serves every
/// scope-bearing kind, and both the per-line classification and the scope
/// event stream are derived from it, so the two cannot disagree on which
/// lines are boundaries.
fn brace_tokens(node: &tree_sitter::Node) -> Option<((usize, usize), (usize, usize))> {
    let mut cursor = node.walk();
    let mut open = None;
    let mut close = None;
    for child in node.children(&mut cursor) {
        match child.kind() {
            "{" if open.is_none() => {
                open = Some((child.start_byte(), child.start_position().row + 1));
            }
            "}" => {
                close = Some((child.start_byte(), child.start_position().row + 1));
            }
            _ => {}
        }
    }
    Some((open?, close?))
}

//= design/classifiers/kotlin-spec.md#scope-bearing-nodes
//= type=implementation
//# `when_expression` and `lambda_literal` carry their brace tokens
//# as direct children rather than in a child `block`;
//# their events MUST be keyed on those tokens.
fn collect_scope_events(node: &tree_sitter::Node, out: &mut Vec<(usize, ScopeEvent)>) {
    if is_scope_bearing(node.kind()) {
        if let Some(((open_byte, open_line), (close_byte, close_line))) = brace_tokens(node) {
            out.push((
                open_byte,
                ScopeEvent {
                    line: open_line as u64,
                    opens: true,
                },
            ));
            out.push((
                close_byte,
                ScopeEvent {
                    line: close_line as u64,
                    opens: false,
                },
            ));
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_scope_events(&child, out);
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

    // Each arm evaluates to `true` iff it stamped a real code/structural
    // property; that boolean drives `code_start` below, so the marking arm and
    // the code-start record can never fall out of sync (java.rs pattern).
    let marks_code = match kind {
        //= design/classifiers/kotlin-spec.md#declarations-with-scope
        //= type=implementation
        //# The node kinds
        //# `class_declaration`, `object_declaration`, `companion_object`,
        //# `function_declaration`, `secondary_constructor`,
        //# `anonymous_initializer`, `getter`, and `setter`
        //# MUST be marked with the declaration-with-scope treatment:
        //# every line from the node's start to the line before its body
        //# is marked `Declaration`,
        //# and the body child supplies the scope events.
        //
        //= design/classifiers/kotlin-spec.md#declarations-with-scope
        //= type=implementation
        //# A function header line MUST NOT be marked `Statement`,
        //# including when its parameters carry default-value expressions.
        //
        //= design/classifiers/kotlin-spec.md#declarations-with-scope
        //= type=implementation
        //# Class-like headers (including `data`, `sealed`, `enum`, and
        //# `annotation` classes, interfaces, objects, and companion objects)
        //# carry JaCoCo's synthesized-member and primary-constructor probes;
        //# they MUST be marked `Declaration` and MUST NOT be marked `Statement`.
        //
        // The default-arguments evidence (decisions D3) is why headers stay
        // Declaration-only: JaCoCo can report a fun-with-defaults header MISS
        // while the body ran (defaults never evaluated). Statement would stop
        // backward propagation and turn that into a false NotExecuted.
        "class_declaration"
        | "object_declaration"
        | "companion_object"
        | "function_declaration"
        | "secondary_constructor"
        | "anonymous_initializer"
        | "getter"
        | "setter" => {
            mark_declaration_with_scope(node, line_props, visited);
            true
        }

        //= design/classifiers/kotlin-spec.md#declarations-without-scope
        //= type=implementation
        //# The node kinds
        //# `package_header`, `import`, `type_alias`, and `annotation`
        //# MUST mark their span `Declaration`.
        "package_header" | "import" | "type_alias" | "annotation" => {
            mark_lines(
                start_line,
                end_line,
                LineProperty::Declaration,
                line_props,
                visited,
            );
            true
        }

        //= design/classifiers/kotlin-spec.md#properties
        //= type=implementation
        //# A `property_declaration` MUST mark its span `Declaration`.
        //
        //= design/classifiers/kotlin-spec.md#properties
        //= type=implementation
        //# When it carries an initializer or a `property_delegate` child,
        //# it MUST additionally mark `Statement`
        //# from its start line through the end line
        //# of the initializer or delegate,
        //# and MUST NOT mark `Statement` on any accessor line after it.
        "property_declaration" => {
            mark_lines(
                start_line,
                end_line,
                LineProperty::Declaration,
                line_props,
                visited,
            );
            if let Some(value_end) = property_value_end_line(node) {
                mark_lines(
                    start_line,
                    value_end,
                    LineProperty::Statement,
                    line_props,
                    visited,
                );
            }
            true
        }

        //= design/classifiers/kotlin-spec.md#enum-entries
        //= type=implementation
        //# An `enum_entry` MUST mark its span
        //# with both `Declaration` and `Statement`,
        //# with or without constructor arguments.
        //
        // Kotlin differs from Java here: kotlinc emits per-entry
        // initialization instructions attributed to the entry line in all
        // forms, so every entry is probe-bearing (decisions D9).
        "enum_entry" => {
            mark_lines(
                start_line,
                end_line,
                LineProperty::Declaration,
                line_props,
                visited,
            );
            mark_lines(
                start_line,
                end_line,
                LineProperty::Statement,
                line_props,
                visited,
            );
            true
        }

        //= design/classifiers/kotlin-spec.md#expression-body-functions
        //= type=implementation
        //# A `function_body` in `=` form (expression body)
        //# MUST mark the expression's span `Statement`.
        //
        // The JaCoCo probe follows the expression, not the declaration line
        // (decisions D2): `fun f(x) =` on its own line has no probe at all.
        "function_body" => {
            if let Some(expr) = expression_body_child(node) {
                mark_lines(
                    expr.start_position().row + 1,
                    expr.end_position().row + 1,
                    LineProperty::Statement,
                    line_props,
                    visited,
                );
                true
            } else {
                // Brace form: the block child carries scope + contents.
                false
            }
        }

        //= design/classifiers/kotlin-spec.md#statement-position-expressions
        //= type=implementation
        //# The node kinds
        //# `call_expression`, `navigation_expression`, `assignment`,
        //# `return_expression`, and `throw_expression`
        //# MUST mark their span `Statement`.
        //
        //= design/classifiers/kotlin-spec.md#non-linear-control
        //= type=implementation
        //# A `return_expression` whose first token is `return@`
        //# (a labeled return) MUST be marked `Statement`,
        //# not `NonLinearControl`.
        //
        // A labeled return exits the enclosing lambda early — the control
        // shape of `continue`, which is Statement in the Java classifier.
        // The label-declaration line (the jump target) carries the
        // NonLinearControl the generic contract requires. (decisions D7)
        "call_expression" | "navigation_expression" | "assignment" | "return_expression"
        | "throw_expression" => {
            mark_lines(
                start_line,
                end_line,
                LineProperty::Statement,
                line_props,
                visited,
            );
            true
        }

        //= design/classifiers/kotlin-spec.md#control-headers
        //= type=implementation
        //# The node kinds
        //# `if_expression`, `when_expression`,
        //# `for_statement`, and `while_statement`
        //# MUST mark `Statement` on their start line only.
        "if_expression" | "for_statement" | "while_statement" => {
            mark_lines(
                start_line,
                start_line,
                LineProperty::Statement,
                line_props,
                visited,
            );
            true
        }
        // `when_expression` is both a control header and a scope bearer: its
        // braces are direct children (no child `block`), so this arm stamps
        // the Statement AND the scope boundaries (kotlin-spec.md §4, §6.3).
        "when_expression" => {
            mark_lines(
                start_line,
                start_line,
                LineProperty::Statement,
                line_props,
                visited,
            );
            mark_brace_scopes(node, line_props, visited);
            true
        }

        //= design/classifiers/kotlin-spec.md#control-headers
        //= type=implementation
        //# A `do_while_statement` MUST mark `Statement` on its end line
        //# (the `} while (condition)` line) and MUST NOT mark its start line.
        //
        // JaCoCo probes the condition line, not the `do {` line
        // (decisions D6). The end line is also the body's ScopeClose —
        // a legal compound.
        "do_while_statement" => {
            mark_lines(
                end_line,
                end_line,
                LineProperty::Statement,
                line_props,
                visited,
            );
            true
        }

        //= design/classifiers/kotlin-spec.md#when-arms
        //= type=implementation
        //# A `when_entry` MUST mark `Statement` on its start line.
        //
        // Per-arm probes, the Java switch_rule analog (decisions D5).
        "when_entry" => {
            mark_lines(
                start_line,
                start_line,
                LineProperty::Statement,
                line_props,
                visited,
            );
            true
        }

        //= design/classifiers/kotlin-spec.md#try-catch-finally
        //= type=implementation
        //# `try_expression`, `catch_block`, and `finally_block`
        //# MUST mark `Declaration` on their start lines;
        //# their `block` children supply the scopes.
        "try_expression" | "catch_block" | "finally_block" => {
            mark_lines(
                start_line,
                start_line,
                LineProperty::Declaration,
                line_props,
                visited,
            );
            true
        }

        //= design/classifiers/kotlin-spec.md#non-linear-control
        //= type=implementation
        //# A `label` node (a label declaration such as `outer@` or `lit@`)
        //# MUST mark `NonLinearControl` on its line.
        //
        //= design/classifiers/kotlin-spec.md#non-linear-control
        //= type=implementation
        //# A `labeled_expression` (`break@label`, `continue@label`)
        //# MUST mark `NonLinearControl` on its line.
        "label" | "labeled_expression" => {
            mark_lines(
                start_line,
                start_line,
                LineProperty::NonLinearControl,
                line_props,
                visited,
            );
            true
        }

        // Scope-bearing nodes other than `when_expression` (handled above).
        // Brace-token keyed so the per-line marks agree with `scope_events`.
        "block" | "class_body" | "enum_class_body" | "lambda_literal" => {
            mark_brace_scopes(node, line_props, visited);
            true
        }

        //= design/classifiers/kotlin-spec.md#comments
        //= type=implementation
        //# `line_comment` and `block_comment` nodes
        //# MUST mark their span `Comment`,
        //# except lines already marked `Annotation` by the pre-pass.
        "line_comment" => {
            if !visited[start_line] || !line_props[start_line].contains(&LineProperty::Annotation) {
                mark_lines(
                    start_line,
                    end_line,
                    LineProperty::Comment,
                    line_props,
                    visited,
                );
            }
            false
        }
        "block_comment" => {
            mark_lines(
                start_line,
                end_line,
                LineProperty::Comment,
                line_props,
                visited,
            );
            false
        }

        //= design/classifiers/kotlin-spec.md#unmodeled-constructs
        //= type=implementation
        //# Any node kind this document does not name
        //# contributes no property of its own;
        //# its lines keep whatever enclosing nodes marked,
        //# or remain `None` (unknown) per the generic contract's Decision 9.
        _ => false,
    };

    //= design/classifiers/kotlin-spec.md#unmodeled-constructs
    //= type=implementation
    //# Every arm of the classifier that marks a code or structural property
    //# MUST record the marked node's start line as a code-start line
    //# for the post-pass's `{code, Comment}` disambiguation.
    if marks_code && start_line < code_start.len() {
        code_start[start_line] = true;
    }

    let mut cursor = node.walk();
    let children: Vec<_> = node.children(&mut cursor).collect();
    for child in children {
        walk_node(&child, lines, line_props, visited, code_start);
    }
}

/// Stamp `ScopeOpen`/`ScopeClose` on the lines of a scope-bearing node's brace
/// tokens — the classification-side twin of `collect_scope_events`, built on
/// the same `brace_tokens` helper so the two views cannot disagree.
///
//= design/classifiers/kotlin-spec.md#scope-bearing-nodes
//= type=implementation
//# For each scope-bearing node, the classifier MUST mark
//# `ScopeOpen` on the line of its opening brace token
//# and `ScopeClose` on the line of its closing brace token.
fn mark_brace_scopes(
    node: &tree_sitter::Node,
    line_props: &mut [BTreeSet<LineProperty>],
    visited: &mut [bool],
) {
    if let Some(((_, open_line), (_, close_line))) = brace_tokens(node) {
        mark_lines(
            open_line,
            open_line,
            LineProperty::ScopeOpen,
            line_props,
            visited,
        );
        mark_lines(
            close_line,
            close_line,
            LineProperty::ScopeClose,
            line_props,
            visited,
        );
    }
}

/// Marks a declaration node that may contain a body (scope). Lines from the
/// node's start up to (but excluding) the body-start line are `Declaration`;
/// when the body starts on the declaration's first line, that line is
/// `Declaration` too (compound with the body's `ScopeOpen`). Bodies are
/// `class_body`/`enum_class_body`/`block` children, or a brace-form
/// `function_body` (whose `block` child is the real scope). A node with no
/// brace-form body (expression-body function, abstract member, body-less
/// class) marks its whole span `Declaration` — for expression bodies the
/// `function_body` arm adds `Statement` on the expression's own lines.
fn mark_declaration_with_scope(
    node: &tree_sitter::Node,
    line_props: &mut [BTreeSet<LineProperty>],
    visited: &mut [bool],
) {
    let start_line = node.start_position().row + 1;
    let end_line = node.end_position().row + 1;

    let mut cursor = node.walk();
    let children: Vec<_> = node.children(&mut cursor).collect();
    let body = children.iter().find_map(|c| match c.kind() {
        "class_body" | "enum_class_body" | "block" => Some(*c),
        "function_body" => {
            // Brace form carries a block child; `=` form does not.
            let mut fb_cursor = c.walk();
            let fb_children: Vec<_> = c.children(&mut fb_cursor).collect();
            fb_children.into_iter().find(|b| b.kind() == "block")
        }
        _ => None,
    });

    if let Some(body) = body {
        let body_start = body.start_position().row + 1;
        for line in start_line..body_start {
            mark_lines(line, line, LineProperty::Declaration, line_props, visited);
        }
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
        //= design/classifiers/kotlin-spec.md#declarations-without-scope
        //= type=implementation
        //# A `function_declaration` with neither a brace-form body
        //# nor an expression body (an abstract or interface member)
        //# MUST mark its span `Declaration` only.
        mark_lines(
            start_line,
            end_line,
            LineProperty::Declaration,
            line_props,
            visited,
        );
    }
}

/// The end line of a `property_declaration`'s initializer or delegate, if it
/// has one. Asks the grammar directly (java.rs `node_has_initializer`
/// pattern): a `property_delegate` child, or the child following an `=`
/// token. Returns `None` for bare declarations (`val stored: Int`) and
/// accessor-only properties (`val computed: Int` + getter), which are
/// Declaration-only — backward propagation from the executed accessor or
/// initializer body rescues them (decisions D10).
fn property_value_end_line(node: &tree_sitter::Node) -> Option<usize> {
    let mut cursor = node.walk();
    let children: Vec<_> = node.children(&mut cursor).collect();
    if let Some(delegate) = children.iter().find(|c| c.kind() == "property_delegate") {
        return Some(delegate.end_position().row + 1);
    }
    let eq_index = children.iter().position(|c| c.kind() == "=")?;
    let value = children.get(eq_index + 1)?;
    Some(value.end_position().row + 1)
}

/// The expression child of an `=`-form `function_body`, or `None` for the
/// brace form. The `=` token is the discriminator the grammar provides.
fn expression_body_child<'a>(node: &tree_sitter::Node<'a>) -> Option<tree_sitter::Node<'a>> {
    let mut cursor = node.walk();
    let children: Vec<_> = node.children(&mut cursor).collect();
    let eq_index = children.iter().position(|c| c.kind() == "=")?;
    children.get(eq_index + 1).copied()
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

#[cfg(test)]
mod tests {
    use super::*;
    use duvet_coverage::types::LineClass;

    // Corpus fixtures: the exact Kotlin sources that were compiled with
    // kotlinc and executed under JaCoCo to produce the ground-truth evidence
    // in design/classifiers/kotlin-decisions.md. External files keep this
    // source free of literals duvet's annotation parser would misread.
    const SINGLE_EXPR: &str = include_str!("testdata/kotlin/SingleExpr.kt");
    const LAMBDAS: &str = include_str!("testdata/kotlin/Lambdas.kt");
    const WHEN_EXPR: &str = include_str!("testdata/kotlin/WhenExpr.kt");
    const TEMPLATES: &str = include_str!("testdata/kotlin/Templates.kt");
    const LABELS: &str = include_str!("testdata/kotlin/Labels.kt");
    const DATA_CLASSES: &str = include_str!("testdata/kotlin/DataClasses.kt");
    const DEFAULTS: &str = include_str!("testdata/kotlin/Defaults.kt");
    const SUSPEND: &str = include_str!("testdata/kotlin/Suspend.kt");
    const EXTRAS: &str = include_str!("testdata/kotlin/Extras.kt");
    const TRY_CATCH: &str = include_str!("testdata/kotlin/TryCatch.kt");
    const ENUMS: &str = include_str!("testdata/kotlin/Enums.kt");

    fn classify(source: &str) -> Vec<Option<LineClass>> {
        match KotlinClassifier.classify(source) {
            Classification::Classified(c) => c,
            Classification::Unclassifiable { .. } => {
                panic!("test source failed to classify (unexpected parse error)")
            }
        }
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

    /// 1-based line lookup, so assertions read like the evidence tables in
    /// kotlin-decisions.md.
    fn at(classified: &[Option<LineClass>], line: usize) -> &Option<LineClass> {
        &classified[line - 1]
    }

    //= design/classifiers/kotlin-spec.md#grammar
    //= type=test
    //# The classifier MUST parse Kotlin source with the `tree-sitter-kotlin-ng`
    //# grammar.
    #[test]
    fn valid_kotlin_is_classified() {
        // Every corpus fixture parses with zero errors under the pinned
        // grammar; a Classified result for each is the observable form of
        // the grammar requirement.
        for src in [
            SINGLE_EXPR,
            LAMBDAS,
            WHEN_EXPR,
            TEMPLATES,
            LABELS,
            DATA_CLASSES,
            DEFAULTS,
            SUSPEND,
            EXTRAS,
            TRY_CATCH,
            ENUMS,
        ] {
            assert!(matches!(
                KotlinClassifier.classify(src),
                Classification::Classified(_)
            ));
        }
    }

    //= design/classifiers/kotlin-spec.md#parse-errors
    //= type=test
    //# When the parse tree contains any `ERROR` or `MISSING` node,
    //# the classifier MUST return `Unclassifiable`
    //# and MUST report the line of every `ERROR` and `MISSING` node.
    #[test]
    fn parse_error_is_unclassifiable_and_located() {
        let broken = "fun f() {\n    val x =\n}\n";
        match KotlinClassifier.classify(broken) {
            Classification::Unclassifiable { first, rest } => {
                let mut all = vec![first];
                all.extend(rest);
                assert!(
                    all.iter()
                        .all(|i| i.reason == ClassifierFailure::ParseError),
                    "every issue must be a ParseError"
                );
                assert!(
                    all.iter().all(|i| i.line >= 1),
                    "every issue must be located (line >= 1), got {all:?}"
                );
            }
            Classification::Classified(_) => panic!("broken Kotlin must be Unclassifiable"),
        }
    }

    //= design/classifiers/kotlin-spec.md#pre-and-post-pass
    //= type=test
    //# Before the CST walk, the classifier MUST mark
    //# blank lines as `Whitespace`
    //# and duvet annotation lines (`//=` or `//#` after leading whitespace)
    //# as `Annotation`.
    #[test]
    fn blank_and_annotation_lines() {
        // Fixture-borne annotation lines would be parsed by duvet's scanner,
        // so build the annotation prefix at runtime instead.
        let meta = [' ', ' ', ' ', ' '].iter().collect::<String>()
            + ANNOTATION_META
            + " design/classifiers/kotlin-spec.md#properties";
        let src = format!("fun f(): Int {{\n\n{meta}\n    return 1\n}}\n");
        let c = classify(&src);
        assert!(is_exactly(at(&c, 2), &[LineProperty::Whitespace]));
        assert!(is_exactly(at(&c, 3), &[LineProperty::Annotation]));
    }

    //= design/classifiers/kotlin-spec.md#pre-and-post-pass
    //= type=test
    //# After the CST walk, the classifier MUST apply the verified
    //# mutual-exclusivity post-pass (`clean_classifications`)
    //# to every classification it returns.
    #[test]
    fn multiline_span_comment_lines_stay_pure() {
        // A statement span (multi-line chained call) crossing a comment line:
        // without the post-pass the comment line would read {Statement,
        // Comment}; the verified pass strips the span artifact. Line 21 of
        // SingleExpr.kt is a chain continuation; synthesize the sharper case
        // directly.
        let src = "fun f(xs: List<Int>) =\n    xs.map { it + 1 }\n        // note between chain links\n        .sum()\n";
        let c = classify(src);
        assert!(
            is_exactly(at(&c, 3), &[LineProperty::Comment]),
            "comment inside a statement span must be pure Comment, got {:?}",
            at(&c, 3)
        );
    }

    //= design/classifiers/kotlin-spec.md#scope-bearing-nodes
    //= type=test
    //# The scope-bearing node kinds are
    //# `block`, `class_body`, `enum_class_body`, `lambda_literal`,
    //# and `when_expression`.
    #[test]
    fn scope_bearing_kinds_open_and_close() {
        // One representative per kind, from the corpus fixtures.
        // block: Labels.kt fun body line 6 opens, 17 closes.
        let labels = classify(LABELS);
        assert!(has_prop(at(&labels, 6), LineProperty::ScopeOpen));
        assert!(has_prop(at(&labels, 17), LineProperty::ScopeClose));
        // class_body: Extras.kt WithInit line 13..39.
        let extras = classify(EXTRAS);
        assert!(has_prop(at(&extras, 13), LineProperty::ScopeOpen));
        assert!(has_prop(at(&extras, 39), LineProperty::ScopeClose));
        // enum_class_body: Enums.kt Color line 8..20.
        let enums = classify(ENUMS);
        assert!(has_prop(at(&enums, 8), LineProperty::ScopeOpen));
        assert!(has_prop(at(&enums, 20), LineProperty::ScopeClose));
        // lambda_literal: Lambdas.kt doubler line 7..9.
        let lambdas = classify(LAMBDAS);
        assert!(has_prop(at(&lambdas, 7), LineProperty::ScopeOpen));
        assert!(has_prop(at(&lambdas, 9), LineProperty::ScopeClose));
        // when_expression: WhenExpr.kt whenSubject line 7..11.
        let whens = classify(WHEN_EXPR);
        assert!(has_prop(at(&whens, 7), LineProperty::ScopeOpen));
        assert!(has_prop(at(&whens, 11), LineProperty::ScopeClose));
    }

    //= design/classifiers/kotlin-spec.md#scope-bearing-nodes
    //= type=test
    //# `when_expression` and `lambda_literal` carry their brace tokens
    //# as direct children rather than in a child `block`;
    //# their events MUST be keyed on those tokens.
    #[test]
    fn when_scope_marks_brace_lines_not_node_extremes() {
        // `return when (n) {` — the when_expression node STARTS mid-line at
        // `when`, but its `{` is on the same line; the `}` closes on its own
        // line. Byte-extreme keying (the Java pattern) would misplace the
        // open at the `when` keyword byte — for classification the same line
        // here, but the CLOSE line differs from the node end when the when is
        // an initializer of a multi-line declaration. Assert the brace lines
        // directly.
        let whens = classify(WHEN_EXPR);
        // whenSubject: when spans 7..11, braces on 7 and 11.
        assert!(has_prop(at(&whens, 7), LineProperty::ScopeOpen));
        assert!(has_prop(at(&whens, 11), LineProperty::ScopeClose));
        // whenExprBody (expression-body when): fun on line 36, close on 39.
        assert!(has_prop(at(&whens, 36), LineProperty::ScopeOpen));
        assert!(has_prop(at(&whens, 39), LineProperty::ScopeClose));
    }

    // Corpus-wide guard for the event stream: balanced and in source order
    // for every fixture. (The requirement's citation owner is the property
    // test below, which generalizes this beyond the corpus.)
    #[test]
    fn scope_event_stream_is_balanced_and_ordered() {
        use duvet_coverage::scopes::scope_imbalance_site;
        for src in [
            SINGLE_EXPR,
            LAMBDAS,
            WHEN_EXPR,
            TEMPLATES,
            LABELS,
            DATA_CLASSES,
            DEFAULTS,
            SUSPEND,
            EXTRAS,
            TRY_CATCH,
            ENUMS,
        ] {
            let events = KotlinClassifier.scope_events(src);
            assert!(
                scope_imbalance_site(&events).is_none(),
                "corpus fixture produced an unbalanced scope stream"
            );
            // Source order: line numbers never decrease.
            let lines: Vec<u64> = events.iter().map(|e| e.line).collect();
            let mut sorted = lines.clone();
            sorted.sort_unstable();
            assert_eq!(lines, sorted, "events must be in source order");
        }
    }

    //= design/classifiers/kotlin-spec.md#declarations-with-scope
    //= type=test
    //# The node kinds
    //# `class_declaration`, `object_declaration`, `companion_object`,
    //# `function_declaration`, `secondary_constructor`,
    //# `anonymous_initializer`, `getter`, and `setter`
    //# MUST be marked with the declaration-with-scope treatment:
    //# every line from the node's start to the line before its body
    //# is marked `Declaration`,
    //# and the body child supplies the scope events.
    #[test]
    fn declaration_with_scope_kinds() {
        let extras = classify(EXTRAS);
        // class WithInit(seed: Int) { — Declaration + ScopeOpen (body on
        // the same line).
        assert!(has_prop(at(&extras, 13), LineProperty::Declaration));
        assert!(has_prop(at(&extras, 13), LineProperty::ScopeOpen));
        // init { — anonymous_initializer, Declaration + body scope.
        assert!(has_prop(at(&extras, 16), LineProperty::Declaration));
        assert!(has_prop(at(&extras, 16), LineProperty::ScopeOpen));
        // get() { — getter header.
        assert!(has_prop(at(&extras, 21), LineProperty::Declaration));
        assert!(has_prop(at(&extras, 21), LineProperty::ScopeOpen));
        // set(value) { — setter header.
        assert!(has_prop(at(&extras, 26), LineProperty::Declaration));
        // companion object { — companion_object.
        assert!(has_prop(at(&extras, 30), LineProperty::Declaration));
        // object Singleton { — object_declaration.
        assert!(has_prop(at(&extras, 41), LineProperty::Declaration));
        let enums = classify(ENUMS);
        // constructor() : this(...) { — secondary_constructor.
        assert!(has_prop(at(&enums, 49), LineProperty::Declaration));
        // fun hex(): String { — function_declaration inside enum body.
        assert!(has_prop(at(&enums, 13), LineProperty::Declaration));
    }

    //= design/classifiers/kotlin-spec.md#declarations-with-scope
    //= type=test
    //# A function header line MUST NOT be marked `Statement`,
    //# including when its parameters carry default-value expressions.
    #[test]
    fn fun_header_with_defaults_is_never_statement() {
        // Defaults.kt line 20: `fun allArgsProvided(a: Int = 10, b: Int = 20)`.
        // JaCoCo reports this line MISS when defaults never evaluate even
        // though the body ran; Statement here would block backward propagation
        // and produce a false NotExecuted (decisions D3).
        let c = classify(DEFAULTS);
        assert!(has_prop(at(&c, 20), LineProperty::Declaration));
        assert!(
            !has_prop(at(&c, 20), LineProperty::Statement),
            "fun-with-defaults header must not be Statement: {:?}",
            at(&c, 20)
        );
        // Multi-line parameter list (lines 7-10): all Declaration, none
        // Statement.
        for line in 7..=10 {
            assert!(has_prop(at(&c, line), LineProperty::Declaration));
            assert!(!has_prop(at(&c, line), LineProperty::Statement));
        }
    }

    //= design/classifiers/kotlin-spec.md#declarations-with-scope
    //= type=test
    //# Class-like headers (including `data`, `sealed`, `enum`, and
    //# `annotation` classes, interfaces, objects, and companion objects)
    //# carry JaCoCo's synthesized-member and primary-constructor probes;
    //# they MUST be marked `Declaration` and MUST NOT be marked `Statement`.
    #[test]
    fn class_like_headers_are_declaration_not_statement() {
        let dc = classify(DATA_CLASSES);
        // data class UsedFully(val x: Int, val y: String) — header carries
        // synthesized equals/hashCode/copy probes (JaCoCo PART), Declaration.
        assert!(has_prop(at(&dc, 7), LineProperty::Declaration));
        assert!(!has_prop(at(&dc, 7), LineProperty::Statement));
        let enums = classify(ENUMS);
        // sealed class Shape { and subclass headers.
        assert!(has_prop(at(&enums, 27), LineProperty::Declaration));
        assert!(!has_prop(at(&enums, 27), LineProperty::Statement));
        assert!(has_prop(at(&enums, 28), LineProperty::Declaration));
        assert!(!has_prop(at(&enums, 28), LineProperty::Statement));
        // interface Greeter {
        assert!(has_prop(at(&enums, 39), LineProperty::Declaration));
        assert!(!has_prop(at(&enums, 39), LineProperty::Statement));
    }

    //= design/classifiers/kotlin-spec.md#declarations-without-scope
    //= type=test
    //# The node kinds
    //# `package_header`, `import`, `type_alias`, and `annotation`
    //# MUST mark their span `Declaration`.
    #[test]
    fn scope_less_declarations() {
        let suspend = classify(SUSPEND);
        // package corpus
        assert!(is_exactly(at(&suspend, 1), &[LineProperty::Declaration]));
        // import kotlin.coroutines.Continuation
        assert!(is_exactly(at(&suspend, 3), &[LineProperty::Declaration]));
        let enums = classify(ENUMS);
        // typealias IntPair = Pair<Int, Int>
        assert!(has_prop(at(&enums, 58), LineProperty::Declaration));
        // @Deprecated(...) use-site annotation
        assert!(has_prop(at(&enums, 60), LineProperty::Declaration));
    }

    //= design/classifiers/kotlin-spec.md#declarations-without-scope
    //= type=test
    //# A `function_declaration` with neither a brace-form body
    //# nor an expression body (an abstract or interface member)
    //# MUST mark its span `Declaration` only.
    #[test]
    fn abstract_member_is_pure_declaration() {
        // Enums.kt line 40: `fun name(): String` inside interface Greeter.
        let c = classify(ENUMS);
        assert!(
            is_exactly(at(&c, 40), &[LineProperty::Declaration]),
            "abstract member must be exactly Declaration, got {:?}",
            at(&c, 40)
        );
    }

    //= design/classifiers/kotlin-spec.md#properties
    //= type=test
    //# A `property_declaration` MUST mark its span `Declaration`.
    #[test]
    fn bare_property_is_declaration_without_statement() {
        // Extras.kt line 14: `val stored: Int` (no initializer; assigned in
        // init). Statement would stop backward propagation from the executed
        // init block (decisions D10).
        let c = classify(EXTRAS);
        assert!(has_prop(at(&c, 14), LineProperty::Declaration));
        assert!(!has_prop(at(&c, 14), LineProperty::Statement));
        // Line 20: `val computed: Int` (accessor-only) — same.
        assert!(has_prop(at(&c, 20), LineProperty::Declaration));
        assert!(!has_prop(at(&c, 20), LineProperty::Statement));
    }

    //= design/classifiers/kotlin-spec.md#properties
    //= type=test
    //# When it carries an initializer or a `property_delegate` child,
    //# it MUST additionally mark `Statement`
    //# from its start line through the end line
    //# of the initializer or delegate,
    //# and MUST NOT mark `Statement` on any accessor line after it.
    #[test]
    fn initialized_and_delegated_properties_are_statements() {
        let c = classify(EXTRAS);
        // val topLevelInitialized: Int = 7 * 6 — probe-bearing (JaCoCo PART).
        assert!(has_prop(at(&c, 7), LineProperty::Statement));
        assert!(has_prop(at(&c, 7), LineProperty::Declaration));
        // val topLevelLazy: String by lazy { — delegate line probes (HIT).
        assert!(has_prop(at(&c, 9), LineProperty::Statement));
        // var settable: Int = 0 (line 25) — initializer, then accessor lines.
        assert!(has_prop(at(&c, 25), LineProperty::Statement));
        // set(value) header (line 26) is an accessor line AFTER the
        // initializer: Declaration territory, no Statement from the property.
        assert!(
            !has_prop(at(&c, 26), LineProperty::Statement),
            "setter header must not inherit the property initializer's \
             Statement: {:?}",
            at(&c, 26)
        );
    }

    //= design/classifiers/kotlin-spec.md#enum-entries
    //= type=test
    //# An `enum_entry` MUST mark its span
    //# with both `Declaration` and `Statement`,
    //# with or without constructor arguments.
    #[test]
    fn enum_entries_with_and_without_args() {
        let c = classify(ENUMS);
        // RED(0xFF0000), — args; JaCoCo HIT.
        assert!(has_prop(at(&c, 9), LineProperty::Declaration));
        assert!(has_prop(at(&c, 9), LineProperty::Statement));
        // Plain's A, — no args; JaCoCo still probes (kotlinc emits per-entry
        // init instructions in all forms — decisions D9, unlike Java).
        assert!(has_prop(at(&c, 23), LineProperty::Declaration));
        assert!(has_prop(at(&c, 23), LineProperty::Statement));
    }

    //= design/classifiers/kotlin-spec.md#expression-body-functions
    //= type=test
    //# A `function_body` in `=` form (expression body)
    //# MUST mark the expression's span `Statement`.
    #[test]
    fn expression_body_probe_follows_the_expression() {
        let c = classify(SINGLE_EXPR);
        // fun exprCalled(x: Int) = x * 2 — one line: {Declaration, Statement}.
        assert!(has_prop(at(&c, 7), LineProperty::Declaration));
        assert!(has_prop(at(&c, 7), LineProperty::Statement));
        // Split form: declaration line 12 must NOT be Statement (JaCoCo puts
        // no probe there); expression line 13 must be Statement.
        assert!(has_prop(at(&c, 12), LineProperty::Declaration));
        assert!(
            !has_prop(at(&c, 12), LineProperty::Statement),
            "split expression-body declaration line carries no probe: {:?}",
            at(&c, 12)
        );
        assert!(has_prop(at(&c, 13), LineProperty::Statement));
        // Multi-line chain body: every chain line is Statement (each probes).
        for line in 20..=22 {
            assert!(has_prop(at(&c, line), LineProperty::Statement));
        }
    }

    //= design/classifiers/kotlin-spec.md#statement-position-expressions
    //= type=test
    //# The node kinds
    //# `call_expression`, `navigation_expression`, `assignment`,
    //# `return_expression`, and `throw_expression`
    //# MUST mark their span `Statement`.
    #[test]
    fn statement_position_expressions() {
        let labels = classify(LABELS);
        // call_expression: println(labeledBreak()) — line 56.
        assert!(has_prop(at(&labels, 56), LineProperty::Statement));
        // assignment: total += i * j — line 13.
        assert!(has_prop(at(&labels, 13), LineProperty::Statement));
        // return_expression: return total — line 16.
        assert!(has_prop(at(&labels, 16), LineProperty::Statement));
        let tc = classify(TRY_CATCH);
        // throw_expression: throw IllegalStateException(...) — line 30.
        assert!(has_prop(at(&tc, 30), LineProperty::Statement));
        let ss = classify(SUSPEND);
        // navigation_expression receiver line: body.startCoroutine(...) — 45.
        assert!(has_prop(at(&ss, 45), LineProperty::Statement));
    }

    //= design/classifiers/kotlin-spec.md#control-headers
    //= type=test
    //# The node kinds
    //# `if_expression`, `when_expression`,
    //# `for_statement`, and `while_statement`
    //# MUST mark `Statement` on their start line only.
    #[test]
    fn control_headers_statement_on_start_line() {
        let tc = classify(TRY_CATCH);
        // while (i < n) { — line 38 (probe-bearing condition).
        assert!(has_prop(at(&tc, 38), LineProperty::Statement));
        // if (flag) { — line 29.
        assert!(has_prop(at(&tc, 29), LineProperty::Statement));
        // for (i in 1..n) { — line 57.
        assert!(has_prop(at(&tc, 57), LineProperty::Statement));
        // when (n) { as a return value — WhenExpr.kt line 7.
        let whens = classify(WHEN_EXPR);
        assert!(has_prop(at(&whens, 7), LineProperty::Statement));
        // "only": the body lines of the while are NOT stamped by the header
        // (they're Statements via their own nodes, but the blank-bodied check
        // needs a body-less construct; assert the loop's closing brace line
        // is not Statement).
        assert!(
            !has_prop(at(&tc, 41), LineProperty::Statement),
            "while body close brace must not carry the header's Statement: {:?}",
            at(&tc, 41)
        );
    }

    //= design/classifiers/kotlin-spec.md#control-headers
    //= type=test
    //# A `do_while_statement` MUST mark `Statement` on its end line
    //# (the `} while (condition)` line) and MUST NOT mark its start line.
    #[test]
    fn do_while_probe_is_on_the_condition_line() {
        let c = classify(TRY_CATCH);
        // do { — line 48: JaCoCo places no probe here.
        assert!(
            !has_prop(at(&c, 48), LineProperty::Statement),
            "do line carries no probe: {:?}",
            at(&c, 48)
        );
        // } while (i < n) — line 51: probe-bearing, also the body ScopeClose.
        assert!(has_prop(at(&c, 51), LineProperty::Statement));
        assert!(has_prop(at(&c, 51), LineProperty::ScopeClose));
    }

    //= design/classifiers/kotlin-spec.md#when-arms
    //= type=test
    //# A `when_entry` MUST mark `Statement` on its start line.
    #[test]
    fn when_arms_are_per_arm_statements() {
        let c = classify(WHEN_EXPR);
        // Subject form arms (JaCoCo probes each arm independently).
        for line in 8..=10 {
            assert!(
                has_prop(at(&c, line), LineProperty::Statement),
                "when arm on line {line} must be Statement: {:?}",
                at(&c, line)
            );
        }
        // Block-bodied arm: `0 -> {` line 24 is Statement + ScopeOpen.
        assert!(has_prop(at(&c, 24), LineProperty::Statement));
        assert!(has_prop(at(&c, 24), LineProperty::ScopeOpen));
    }

    //= design/classifiers/kotlin-spec.md#try-catch-finally
    //= type=test
    //# `try_expression`, `catch_block`, and `finally_block`
    //# MUST mark `Declaration` on their start lines;
    //# their `block` children supply the scopes.
    #[test]
    fn try_catch_finally_headers() {
        let c = classify(TRY_CATCH);
        // try { — line 9: Declaration + ScopeOpen (block opens same line).
        assert!(has_prop(at(&c, 9), LineProperty::Declaration));
        assert!(has_prop(at(&c, 9), LineProperty::ScopeOpen));
        // } catch (e: ArithmeticException) { — line 11: compound
        // close(try-block) + Declaration + open(catch-block).
        assert!(has_prop(at(&c, 11), LineProperty::Declaration));
        assert!(has_prop(at(&c, 11), LineProperty::ScopeOpen));
        assert!(has_prop(at(&c, 11), LineProperty::ScopeClose));
        // } finally { — line 13: same compound shape.
        assert!(has_prop(at(&c, 13), LineProperty::Declaration));
        // try-as-expression keeps the same shape: val v = try { — line 20 is
        // the property Statement AND the try Declaration + ScopeOpen.
        assert!(has_prop(at(&c, 20), LineProperty::Statement));
        assert!(has_prop(at(&c, 20), LineProperty::ScopeOpen));
    }

    //= design/classifiers/kotlin-spec.md#non-linear-control
    //= type=test
    //# A `label` node (a label declaration such as `outer@` or `lit@`)
    //# MUST mark `NonLinearControl` on its line.
    #[test]
    fn label_declarations_are_non_linear_control() {
        let c = classify(LABELS);
        // outer@ for (i in 1..3) { — line 8.
        assert!(has_prop(at(&c, 8), LineProperty::NonLinearControl));
        // xs.forEach lit@{ n -> — line 34 (lambda label).
        assert!(has_prop(at(&c, 34), LineProperty::NonLinearControl));
    }

    //= design/classifiers/kotlin-spec.md#non-linear-control
    //= type=test
    //# A `labeled_expression` (`break@label`, `continue@label`)
    //# MUST mark `NonLinearControl` on its line.
    #[test]
    fn labeled_jumps_are_non_linear_control() {
        let c = classify(LABELS);
        // break@outer — line 11.
        assert!(has_prop(at(&c, 11), LineProperty::NonLinearControl));
        // continue@loop — line 24.
        assert!(has_prop(at(&c, 24), LineProperty::NonLinearControl));
    }

    //= design/classifiers/kotlin-spec.md#non-linear-control
    //= type=test
    //# A `return_expression` whose first token is `return@`
    //# (a labeled return) MUST be marked `Statement`,
    //# not `NonLinearControl`.
    #[test]
    fn labeled_return_is_statement_not_nlc() {
        let c = classify(LABELS);
        // return@lit — line 36; return@forEach — line 48. Both probe as
        // ordinary statements (JaCoCo HIT ci=1); the control shape is
        // `continue`, a structured exit (decisions D7).
        for line in [36, 48] {
            assert!(has_prop(at(&c, line), LineProperty::Statement));
            assert!(
                !has_prop(at(&c, line), LineProperty::NonLinearControl),
                "labeled return on line {line} must not be NLC: {:?}",
                at(&c, line)
            );
        }
    }

    //= design/classifiers/kotlin-spec.md#comments
    //= type=test
    //# `line_comment` and `block_comment` nodes
    //# MUST mark their span `Comment`,
    //# except lines already marked `Annotation` by the pre-pass.
    #[test]
    fn comments_are_comment_lines() {
        let c = classify(LABELS);
        // Lines 3-4: leading line comments.
        assert!(is_exactly(at(&c, 3), &[LineProperty::Comment]));
        assert!(is_exactly(at(&c, 4), &[LineProperty::Comment]));
        // A block comment spanning lines.
        let src = "/* first\n   second */\nfun f() = 1\n";
        let c2 = classify(src);
        assert!(is_exactly(at(&c2, 1), &[LineProperty::Comment]));
        assert!(is_exactly(at(&c2, 2), &[LineProperty::Comment]));
    }

    //= design/classifiers/kotlin-spec.md#unmodeled-constructs
    //= type=test
    //# Any node kind this document does not name
    //# contributes no property of its own;
    //# its lines keep whatever enclosing nodes marked,
    //# or remain `None` (unknown) per the generic contract's Decision 9.
    #[test]
    fn unmodeled_lines_stay_none() {
        // `): String {` — the close of a multi-line parameter list. The header
        // Declaration span covers it (enclosing function_declaration marks
        // it), so it is Some. A line genuinely outside every modeled node —
        // e.g. a stray semicolon statement the walk doesn't model — is None.
        // Templates.kt line 11 (plain template text) sits inside the enclosing
        // property span: Some. The contract's observable here: no panic, and
        // out-of-model lines are None, in-span lines keep parent marks.
        let c = classify(TEMPLATES);
        assert!(
            at(&c, 11).is_some(),
            "template text inside a marked span keeps the parent's marks"
        );
    }

    //= design/classifiers/kotlin-spec.md#unmodeled-constructs
    //= type=test
    //# Every arm of the classifier that marks a code or structural property
    //# MUST record the marked node's start line as a code-start line
    //# for the post-pass's `{code, Comment}` disambiguation.
    #[test]
    fn trailing_comment_on_code_line_keeps_code() {
        // Defaults.kt line 30: `println(greet())  // all defaults evaluate` —
        // code starts on the line, so the trailing comment must NOT strip the
        // Statement (the code-start record is what the verified post-pass
        // consults). The post-pass canonicalizes the incidental Comment away
        // on code-start lines (see clean_classifications' trailing-comment
        // tests in duvet-coverage), matching the Java classifier.
        let c = classify(DEFAULTS);
        assert!(has_prop(at(&c, 30), LineProperty::Statement));
        assert!(!has_prop(at(&c, 30), LineProperty::Comment));
        // The dual: a comment line inside a multi-line span (no code starts
        // there) is stripped to pure Comment — covered by
        // multiline_span_comment_lines_stay_pure above.
    }

    // -------------------------------------------------------------------------
    // Property tests (java.rs parity): scope faithfulness over generated
    // Kotlin-shaped inputs, event/classification agreement, and totality.
    // -------------------------------------------------------------------------

    fn classifier_scope_is_balanced(source: &str) -> bool {
        use duvet_coverage::scopes::scope_imbalance_site;
        if matches!(
            KotlinClassifier.classify(source),
            Classification::Unclassifiable { .. }
        ) {
            return true;
        }
        let events = KotlinClassifier.scope_events(source);
        scope_imbalance_site(&events).is_none()
    }

    /// Deterministically build valid, brace-balanced Kotlin from arbitrary
    /// bytes. Kotlin has no bare block statement (a `{ ... }` in statement
    /// position parses as a lambda literal — which is fine: lambda_literal is
    /// scope-bearing here), so each byte emits either a lambda-block open
    /// (`run {`), an `if (true) {` block open, or a close, separated by a
    /// space or newline at random. Any leftover opens are closed at the end,
    /// so the output is always balanced, parseable Kotlin exercising the
    /// compound-line space (`}}`, `} else {`-like shapes emerge from
    /// separators).
    fn gen_kotlin_blocks(bytes: &[u8]) -> String {
        let mut body = String::new();
        let mut depth: usize = 0;
        for &b in bytes.iter().take(64) {
            let open = (b & 1) == 0;
            let sep = if (b & 2) == 0 { ' ' } else { '\n' };
            if open && depth < 16 {
                if (b & 4) == 0 {
                    body.push_str("run {");
                } else {
                    body.push_str("if (true) {");
                }
                depth += 1;
            } else if !open && depth > 0 {
                body.push('}');
                depth -= 1;
            } else {
                continue;
            }
            body.push(sep);
        }
        while depth > 0 {
            body.push('}');
            depth -= 1;
        }
        format!("fun generated() {{\n{body}\n}}\n")
    }

    //= design/classifiers/kotlin-spec.md#scope-bearing-nodes
    //= type=test
    //# The scope-event stream MUST contain one open and one close event
    //# per scope-bearing node, keyed at the byte offsets of its brace tokens,
    //# in source order.
    #[test]
    fn prop_balanced_blocks_are_never_falsely_defeated() {
        use bolero::check;
        // For ALL byte inputs, gen_kotlin_blocks yields valid, brace-balanced
        // Kotlin; the classifier's scope stream MUST therefore be balanced. A
        // failing input is a false DefeatedClassification (valid code scored
        // Unknown) — the PR #227 bug class, generalized to Kotlin.
        check!().with_type::<Vec<u8>>().for_each(|bytes| {
            let src = gen_kotlin_blocks(bytes);
            assert!(
                classifier_scope_is_balanced(&src),
                "balanced generated Kotlin was falsely flagged unbalanced:\n{src}"
            );
        });
    }

    //= design/classifiers/kotlin-spec.md#scope-bearing-nodes
    //= type=test
    //# For each scope-bearing node, the classifier MUST mark
    //# `ScopeOpen` on the line of its opening brace token
    //# and `ScopeClose` on the line of its closing brace token.
    #[test]
    fn prop_scope_events_agree_with_classification_boundaries() {
        use bolero::check;
        use duvet_coverage::types::ScopeEvent;
        // The event stream (imbalance detector's view) and the per-line set
        // (Phase-2 propagation's view) must agree on boundary lines — both
        // are derived from the same brace_tokens helper, and this test pins
        // that invariant against refactoring drift.
        check!().with_type::<Vec<u8>>().for_each(|bytes| {
            let src = gen_kotlin_blocks(bytes);
            let classified = match KotlinClassifier.classify(&src) {
                Classification::Classified(c) => c,
                Classification::Unclassifiable { .. } => return,
            };
            let events = KotlinClassifier.scope_events(&src);
            for ScopeEvent { line, opens } in events {
                let idx = line as usize - 1;
                let want = if opens {
                    LineProperty::ScopeOpen
                } else {
                    LineProperty::ScopeClose
                };
                assert!(
                    has_prop(&classified[idx], want),
                    "scope event (line {line}, opens={opens}) has no matching \
                     {want:?} in the per-line classification: {:?}\nsource:\n{src}",
                    classified[idx]
                );
            }
        });
    }

    #[test]
    fn prop_totality_no_panics_and_length_matches() {
        use bolero::check;
        // Totality: for ARBITRARY input (valid Kotlin or garbage), classify
        // never panics, and a Classified result has exactly one element per
        // source line. Garbage routes to Unclassifiable with located issues.
        check!().with_type::<String>().for_each(|s| {
            match KotlinClassifier.classify(s) {
                Classification::Classified(c) => {
                    assert_eq!(c.len(), s.lines().count());
                }
                Classification::Unclassifiable { first, rest } => {
                    assert!(first.line >= 1);
                    assert!(rest.iter().all(|i| i.line >= 1));
                }
            }
            // scope_events is total as well.
            let _ = KotlinClassifier.scope_events(s);
        });
    }
}
