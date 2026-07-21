// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Java line classifier using tree-sitter-java.
//!
//! Maps tree-sitter CST node types to `LineProperty` sets per the plan's mapping table.
//! Returns `None` for lines the tree-sitter walk does not visit and that are not
//! blank or annotations (Decision 9).

use crate::query::classify::{Classification, ClassifierFailure, LineClassifier};
use duvet_coverage::types::{LineProperty, ScopeEvent};
use std::collections::BTreeSet;

/// Java source classifier using tree-sitter.
pub struct JavaClassifier;

impl LineClassifier for JavaClassifier {
    fn classify(&self, source: &str) -> Classification {
        let lines: Vec<&str> = source.lines().collect();
        let line_count = lines.len();
        // Track which properties apply to each line (1-indexed, index 0 unused)
        let mut line_props: Vec<BTreeSet<LineProperty>> = vec![BTreeSet::new(); line_count + 1];
        let mut visited: Vec<bool> = vec![false; line_count + 1];
        // Track lines on which a real code/structural node *starts*. The
        // post-pass below uses this to disambiguate the two ways a line ends up
        // as `{code, Comment}`: a real code line with a trailing comment
        // (`doX(); // note`, code starts here → keep the code) versus a comment
        // line a multi-line node merely spanned (no code starts here → the code
        // is a span artifact, strip it). See the post-pass for why only the
        // latter direction is load-bearing.
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
            // tree-sitter could not produce a tree at all. We cannot localize
            // within the file, so report a single file-level parse issue (line 1).
            None => return Classification::unclassifiable(ClassifierFailure::ParseError, vec![1]),
        };

        // Defeated commitment (spec §1.5): `parse` returns `Some(tree)` even for
        // syntactically invalid input, inlining `ERROR`/`MISSING` nodes for the
        // parts it could not model (a truncated file, a half-typed edit, or a
        // Java construct tree-sitter-java doesn't support). The walk below would
        // still emit `ScopeOpen`/`ScopeClose` for whichever subtrees parsed — but
        // a missing `{`/`}` unbalances that stream, and `build_scope_tree` would
        // then collapse to one whole-file scope (a well-formed WRONG tree: every
        // annotation resolves against the file-level scope). Rather than feed the
        // verified model that garbage, we refuse and surface every located parse
        // error. The dispatcher escalates (non-blocking `Unknown` in `query`);
        // whether the file simply isn't Java or the grammar has a gap is
        // undecidable here, so we report facts (where), never a cause (why).
        if tree.root_node().has_error() {
            let mut error_lines = Vec::new();
            collect_parse_error_lines(&tree.root_node(), &mut error_lines);
            // has_error() implies at least one ERROR/MISSING node; the fallback
            // is purely defensive so `unclassifiable`'s non-empty contract holds.
            if error_lines.is_empty() {
                error_lines.push(1);
            }
            return Classification::unclassifiable(ClassifierFailure::ParseError, error_lines);
        }

        // Walk the CST
        walk_node(
            &tree.root_node(),
            &lines,
            &mut line_props,
            &mut visited,
            &mut code_start,
        );

        // Post-processing: enforce the mutual-exclusivity contract (spec §1.3) —
        // no line may carry both a code property (Statement/Declaration/scope)
        // and a non-code one (Annotation/Comment/Whitespace). The two strips
        // below are NOT symmetric in importance:
        //
        //   * Stripping spurious code off a non-code line is LOAD-BEARING.
        //     A multi-line AST node (e.g. a fluent builder chain parsed as one
        //     `local_variable_declaration`) stamps Statement/Declaration on
        //     every line it spans, including an annotation or comment line in
        //     its interior — yielding e.g. `{Annotation, Statement}`. That
        //     `Statement` is a lie the verified model would act on: Phase 2
        //     backward propagation stops at any `Statement` line, so a
        //     contaminated annotation line between a covered line and its target
        //     blocks the path and turns a genuinely-Executed target into a false
        //     NotExecuted. (An `{Annotation, Statement}` line never arises
        //     honestly — Annotation requires `//=`/`//#` at line start, which
        //     leaves no room for a statement; it is always a span artifact.)
        //     So on Annotation/Whitespace lines, and on comment lines with no
        //     code node *starting* on them, drop the spurious code properties.
        //
        //   * Stripping Comment off a real code line is CANONICALIZATION, not
        //     correctness. `doX(); // note` is honestly `{Statement, Comment}`.
        //     The model only ever reads Comment behind a `len == 1` guard (a
        //     line counts as skippable only if it is *pure* `{Comment}`), so a
        //     Comment riding alongside a Statement is already ignored and the
        //     verdict is identical with or without this strip. We drop it anyway
        //     to keep the "exactly one authoritative class per line" invariant
        //     and stable snapshots.
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
                // Trailing comment on a real code line (`doX(); // note`):
                // canonicalize to pure code (see contract note above).
                props.remove(&LineProperty::Comment);
            }
        }

        // Build result: 0-indexed output, each element corresponds to line (i+1)
        Classification::Classified(
            (0..line_count)
                .map(|i| {
                    let line_num = i + 1;
                    if visited[line_num] {
                        Some(line_props[line_num].clone())
                    } else {
                        None // Unvisited, non-blank, non-annotation → unknown
                    }
                })
                .collect(),
        )
    }

    fn scope_events(&self, source: &str) -> Vec<ScopeEvent> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_java::LANGUAGE.into())
            .expect("Error loading Java grammar");
        let Some(tree) = parser.parse(source, None) else {
            return Vec::new();
        };
        // A parse error means `classify` returns `Unclassifiable` and the
        // dispatcher escalates before any scope stream is consumed; emit nothing.
        if tree.root_node().has_error() {
            return Vec::new();
        }
        // Collect every block delimiter keyed by source byte offset, then sort
        // into source order. Sorting is what makes a COMPOUND line faithful: the
        // close-try, open-finally, close-finally of `} finally {}` all sit on one
        // line but at distinct byte offsets, so they emerge as three ordered
        // events — the multiplicity the per-line `LineClass` set drops (PR #227).
        let mut keyed: Vec<(usize, ScopeEvent)> = Vec::new();
        collect_scope_events(&tree.root_node(), &mut keyed);
        keyed.sort_by_key(|(byte, _)| *byte);
        keyed.into_iter().map(|(_, ev)| ev).collect()
    }
}

/// Collect one `ScopeEvent` per brace of every scope-bearing block node, keyed
/// by source byte offset (open at the `{`, close at the `}`). The node kinds
/// mirror the `block`-family arm in `walk_node` that stamps
/// `ScopeOpen`/`ScopeClose`, so the event stream and the per-line set agree on
/// *which* lines are boundaries — they differ only in that the stream also keeps
/// multiplicity and order, which the set cannot.
fn collect_scope_events(node: &tree_sitter::Node, out: &mut Vec<(usize, ScopeEvent)>) {
    if matches!(
        node.kind(),
        "block"
            | "class_body"
            | "interface_body"
            | "enum_body"
            | "switch_block"
            | "constructor_body"
    ) {
        out.push((
            node.start_byte(),
            ScopeEvent {
                line: node.start_position().row as u64 + 1,
                opens: true,
            },
        ));
        // `end_byte` is exclusive (one past `}`); the `}` itself is the last
        // byte, and `end_position().row` is the row that `}` sits on.
        out.push((
            node.end_byte().saturating_sub(1),
            ScopeEvent {
                line: node.end_position().row as u64 + 1,
                opens: false,
            },
        ));
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_scope_events(&child, out);
    }
}

/// Collect the 1-based start line of every `ERROR`/`MISSING` node in the parse
/// tree. These locate the syntax errors for the defeated-commitment diagnostic
/// (spec §1.5). Reporting *all* of them — not just the first — lets the user see
/// the whole set in one `query` run (cf. idx37A: collect all before failing).
fn collect_parse_error_lines(node: &tree_sitter::Node, out: &mut Vec<u64>) {
    if node.is_error() || node.is_missing() {
        out.push(node.start_position().row as u64 + 1);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_parse_error_lines(&child, out);
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
    // property on this node's lines. That boolean — not a separate,
    // hand-maintained list of code kinds — drives `code_start` below, so the two
    // can never fall out of sync: the arm that marks a code property is the same
    // arm that reports that code starts here.
    let marks_code = match kind {
        // Declarations that open scopes. `record_declaration` (Java 14+) carries
        // a `class_body`, so it resolves through the same body-search as the
        // others. It is worth modeling — JaCoCo attributes real hit/miss to the
        // record header line (the generated accessors/ctor/equals live there),
        // so treating it as a Declaration lets us consume that verdict instead of
        // discarding it as an unclassified line.
        "class_declaration"
        | "interface_declaration"
        | "enum_declaration"
        | "record_declaration" => {
            mark_declaration_with_scope(node, lines, line_props, visited);
            true
        }
        // `compact_constructor_declaration` is a record's canonical constructor
        // (`Point { … }`); it has a `block` body and behaves like any other
        // constructor for coverage. `static_initializer` (`static { … }`) also
        // carries a `block`; marking the `static` keyword line as Declaration
        // keeps it transparent to the backward walk (and handles the `static`
        // and `{` landing on separate lines — the keyword line falls before the
        // body start and is stamped Declaration, while the block child supplies
        // the scope).
        "method_declaration"
        | "constructor_declaration"
        | "compact_constructor_declaration"
        | "static_initializer" => {
            mark_declaration_with_scope(node, lines, line_props, visited);
            true
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
            true
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
            true
        }
        // Arrow-form switch arm (`case A -> expr;` / `default -> { … }`).
        // JaCoCo emits a per-arm hit/miss on the arm's own line, so the arm is a
        // Statement (a coverage-bearing line with its own verdict). A block body
        // (`-> { … }`) still gets ScopeOpen/ScopeClose from its `block` child via
        // the recursion below; a bare expression arm is a one-liner with no scope.
        "switch_rule" => {
            mark_lines(
                start_line,
                start_line,
                LineProperty::Statement,
                line_props,
                visited,
            );
            true
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
            true
        }
        "catch_clause" => {
            mark_lines(
                start_line,
                start_line,
                LineProperty::Declaration,
                line_props,
                visited,
            );
            true
        }
        "finally_clause" => {
            mark_lines(
                start_line,
                start_line,
                LineProperty::Declaration,
                line_props,
                visited,
            );
            true
        }

        // Variable declarations
        "local_variable_declaration" | "field_declaration" => {
            // `node_has_initializer` already looks only at `variable_declarator`
            // children, so no separate presence check is needed.
            let has_init = node_has_initializer(node);
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
            true
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
            true
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

        // Import and package declarations
        "import_declaration" | "package_declaration" => {
            mark_lines(
                start_line,
                end_line,
                LineProperty::Declaration,
                line_props,
                visited,
            );
            true
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
            true
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
            true
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
            true
        }

        // Any construct the classifier does not model contributes no property;
        // the line stays `None`/whatever a parent stamped, which the verified
        // model treats conservatively (Unknown-safety). It is *not* code-start.
        _ => false,
    };

    // Feed `code_start` for the post-pass's `{code, Comment}` disambiguation
    // (see its note). Key it on where code *starts*, not on every spanned line:
    // a multi-line node marks all its lines with a code property, but only its
    // first line is genuinely code — a comment on a continuation line must stay
    // a comment, not be rescued as a trailing-comment code line.
    if marks_code && start_line < code_start.len() {
        code_start[start_line] = true;
    }

    // Recurse into children
    let mut cursor = node.walk();
    let children: Vec<_> = node.children(&mut cursor).collect();
    for child in children {
        walk_node(&child, lines, line_props, visited, code_start);
    }
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

/// Whether a `local_variable_declaration` / `field_declaration` node declares a
/// variable with an initializer (`int x = 5;`, executable → `Statement`) versus
/// a bare declaration (`int x;`, `Declaration` only). The distinction feeds the
/// verified model: Phase-2 backward propagation stops at `Statement` lines, so a
/// mislabel changes execution verdicts.
///
/// This asks the grammar directly instead of guessing. In
/// tree-sitter-java a `variable_declarator` is `name` (+ optional `dimensions`)
/// with the initializer carried in the optional `value` field (grammar 0.23.x
/// `node-types.json`: `value` is `array_initializer | expression`). So an
/// initializer is present iff some `variable_declarator` has a `value` child.
///
/// This replaces a prior allowlist-of-initializer-kinds plus a denylist-of-type-
/// kinds fallback. That denylist was fragile: any type node the grammar added or
/// renamed (a new primitive, an annotated type, etc.) fell through and was
/// misread as an initializer — a false `Statement`. Field-based detection has no
/// such failure mode; it can only miss if the grammar renames the `value` field,
/// which is a compile-surviving but test-visible change (see the initializer
/// tests below).
fn node_has_initializer(node: &tree_sitter::Node) -> bool {
    let mut cursor = node.walk();
    let children: Vec<_> = node.children(&mut cursor).collect();
    children
        .iter()
        .filter(|child| child.kind() == "variable_declarator")
        .any(|declarator| declarator.child_by_field_name("value").is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use duvet_coverage::types::LineClass;

    fn classify(source: &str) -> Vec<Option<LineClass>> {
        match JavaClassifier.classify(source) {
            Classification::Classified(c) => c,
            Classification::Unclassifiable { .. } => {
                panic!("test source failed to classify (unexpected parse error)")
            }
        }
    }

    #[test]
    fn valid_java_is_classified() {
        let ok = "public class Foo {\n    void m() {\n        doX();\n    }\n}";
        assert!(matches!(
            JavaClassifier.classify(ok),
            Classification::Classified(_)
        ));
    }

    #[test]
    fn parse_error_is_unclassifiable_and_located() {
        // Broken Java (incomplete expression): tree-sitter reports an error, so
        // the classifier refuses rather than emitting a half-built (and likely
        // unbalanced) classification. The result is a non-empty set of located
        // ParseError issues — never a silent all-`None` (Finding #3 / idx55).
        let broken = "public class Foo {\n    void m() {\n        int x = ;\n    }\n}";
        match JavaClassifier.classify(broken) {
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
            Classification::Classified(_) => panic!("broken Java must be Unclassifiable"),
        }
    }

    // -------------------------------------------------------------------------
    // Scope Faithfulness (no false defeat) — regression guard for PR #227.
    //
    // Property: for any parseable, brace-balanced Java source, the classifier's
    // ScopeOpen/ScopeClose stream MUST be balanced, i.e. the verified
    // `scope_imbalance_site` returns `None`. A balanced source the classifier
    // reports as unbalanced is a *false* defeated-classification: valid code
    // scored as `Unknown` (coverage.rs routes an imbalance to
    // `DefeatedClassification`). git bisect pinned the regression to a612679;
    // the root cause is representational — `LineClass = BTreeSet<LineProperty>`
    // cannot carry more than one `ScopeClose` (or `ScopeOpen`) per physical line,
    // so a compound line like `} finally {}` (close-try + open-finally +
    // close-finally) loses a close and the stream reads as unbalanced.
    //
    // NOTE ON PLUMBING: this helper reads the property through the classifier's
    // *current* scope representation. When the faithful ordered-transition
    // representation lands, only this helper changes; every assertion below is
    // stated in terms of the property and stays put.
    // -------------------------------------------------------------------------
    fn classifier_scope_is_balanced(source: &str) -> bool {
        use duvet_coverage::scopes::scope_imbalance_site;
        // A genuine parse error is a *different*, legitimate outcome; the
        // property is only about brace-balanced code the parser accepts.
        if matches!(
            JavaClassifier.classify(source),
            Classification::Unclassifiable { .. }
        ) {
            return true;
        }
        let events = JavaClassifier.scope_events(source);
        scope_imbalance_site(&events).is_none()
    }

    #[test]
    fn balanced_compound_brace_lines_are_not_falsely_defeated() {
        // Each snippet is valid, brace-balanced Java containing a COMPOUND line:
        // a single physical line carrying more than one scope transition. These
        // are exactly the inputs the per-line-set representation cannot encode
        // faithfully, and each was (or would be) falsely flagged unbalanced.
        let cases = [
            // The exact PR #227 regression: `} finally {}` on one line =
            // close(try-block) + open(finally-block) + close(finally-block).
            (
                "try/empty-finally",
                "public class C {\n    int m() {\n        int x;\n        try {\n            x = 1;\n        } finally {}\n        return x;\n    }\n}",
            ),
            // `} else {` = close(then-block) + open(else-block).
            (
                "if-else-same-line",
                "public class C {\n    int m(boolean b) {\n        int x;\n        if (b) {\n            x = 1;\n        } else {\n            x = 2;\n        }\n        return x;\n    }\n}",
            ),
            // `} catch (Exception e) {` = close(try-block) + open(catch-block).
            (
                "try-catch-same-line",
                "public class C {\n    void m() {\n        try {\n            work();\n        } catch (Exception e) {\n            handle();\n        }\n    }\n}",
            ),
            // Two closes on one line: `}}`.
            (
                "double-close",
                "public class C {\n    void m() {\n        if (true) {\n            work();\n        }}\n}",
            ),
        ];
        for (name, src) in cases {
            assert!(
                classifier_scope_is_balanced(src),
                "valid balanced Java `{name}` was falsely flagged as an unbalanced \
                 scope stream (false DefeatedClassification)"
            );
        }
    }

    /// Deterministically build valid, brace-balanced Java from arbitrary bytes:
    /// a method body containing a random nesting of *bare block statements*
    /// (`{ ... }`), a construct tree-sitter-java always accepts. Each byte emits
    /// one brace (open while depth is bounded, else close while depth > 0),
    /// separated by either a space or a newline — so sibling/nested braces land
    /// on the same physical line (`}}`, `}{`, `{}`) or separate lines at random.
    /// Any leftover open blocks are closed at the end, so the output is always
    /// balanced valid Java. This exercises the compound-line space that the
    /// per-line-set representation mishandles.
    fn gen_java_blocks(bytes: &[u8]) -> String {
        let mut body = String::new();
        let mut depth: usize = 0;
        for &b in bytes.iter().take(64) {
            let open = (b & 1) == 0;
            let sep = if (b & 2) == 0 { ' ' } else { '\n' };
            if open && depth < 16 {
                body.push('{');
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
        format!("public class C {{\n    void m() {{\n{body}\n}}\n}}")
    }

    #[test]
    fn prop_balanced_blocks_are_never_falsely_defeated() {
        use bolero::check;
        // For ALL byte inputs, gen_java_blocks yields valid, brace-balanced Java;
        // therefore the classifier's scope stream MUST be balanced. A failing
        // input is a source whose braces balance but whose *classified* stream
        // does not — the false-defeat bug, generalized beyond the fixed cases.
        check!().with_type::<Vec<u8>>().for_each(|bytes| {
            let src = gen_java_blocks(bytes);
            assert!(
                classifier_scope_is_balanced(&src),
                "balanced generated Java was falsely flagged unbalanced:\n{src}"
            );
        });
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

    // field-based initializer detection must recognize every initializer
    // shape (the `value` field is `array_initializer | expression`, so any
    // expression counts), and must NOT be fooled by declarations whose only
    // interesting children are type nodes. These pin the shapes the old
    // allowlist/denylist handled only incidentally.

    #[test]
    fn variable_with_ternary_init() {
        // A ternary initializer: not one of the old allowlisted kinds, previously
        // caught only by the type-denylist fallback.
        let source = "public class Foo {\n    void bar() {\n        int x = a ? 1 : 2;\n    }\n}";
        let result = classify(source);
        assert!(has_prop(&result[2], LineProperty::Statement));
        assert!(has_prop(&result[2], LineProperty::Declaration));
    }

    #[test]
    fn variable_with_cast_init() {
        // A cast initializer, likewise not in the old allowlist.
        let source = "public class Foo {\n    void bar() {\n        int x = (int) y;\n    }\n}";
        let result = classify(source);
        assert!(has_prop(&result[2], LineProperty::Statement));
        assert!(has_prop(&result[2], LineProperty::Declaration));
    }

    #[test]
    fn generic_field_without_init_is_not_statement() {
        // A parameterized-type field with NO initializer. Its `variable_declarator`
        // has only a `name` child; the type nodes live on the declaration, not the
        // declarator's `value` field. Must be Declaration only — never a false
        // Statement from a type node.
        let source = "public class Foo {\n    private Map<String, Integer> m;\n}";
        let result = classify(source);
        assert!(has_prop(&result[1], LineProperty::Declaration));
        assert!(!has_prop(&result[1], LineProperty::Statement));
    }

    #[test]
    fn multi_declarator_one_initialized_is_statement() {
        // `int a, b = 2;` — one declarator has a `value`, one does not. The line
        // executes, so it must be a Statement.
        let source = "public class Foo {\n    void bar() {\n        int a, b = 2;\n    }\n}";
        let result = classify(source);
        assert!(has_prop(&result[2], LineProperty::Statement));
        assert!(has_prop(&result[2], LineProperty::Declaration));
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

    // An `Annotation` line carries *only* `Annotation`, never a code property —
    // even when a multi-line AST node paints over it. `//=`/`//#` is detected at
    // the (trimmed) line start, so everything after is comment text; and the
    // mutual-exclusivity post-pass strips Statement/Declaration/ScopeOpen/
    // ScopeClose/NonLinearControl off any annotation line. This is what makes
    // `line_is_skippable`'s `contains(Annotation)` rule sound in the coverage
    // model (duvet-coverage predicates.rs): the walk can skip an annotation line
    // without ever stepping past a scope boundary, because a line like
    // `{Annotation, ScopeClose}` cannot leave this classifier.
    #[test]
    fn annotation_line_is_pure_even_across_multiline_span() {
        // A fluent builder chain whose `.build();` (with the closing `}` and
        // `;`) is interrupted by a duvet annotation. The chain is one CST node
        // spanning lines 3–5, so tree-sitter paints Statement/ScopeClose onto
        // the annotation line before the post-pass runs.
        let source = "public class Foo {\n  void bar() {\n    thing\n      //= spec.md#s\n      .build();\n  }\n}";
        let result = classify(source);
        // Line 4 (index 3) is the annotation line: it must be exactly Annotation,
        // with no Statement/ScopeClose leaking in from the surrounding chain.
        assert!(is_exactly(&result[3], &[LineProperty::Annotation]));
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

    /// syntactically invalid Java (here a method body truncated mid-edit, so the
    /// closing braces are missing) must NOT emit a lopsided ScopeOpen/ScopeClose
    /// stream. tree-sitter returns `Some(tree)` with inline ERROR/MISSING nodes;
    /// we detect `has_error()` and report `Unclassifiable` with located parse
    /// issues (Finding #3), so the dispatcher escalates loudly to a located
    /// `Unknown` instead of silently feeding the verified scope model an
    /// unbalanced stream it would collapse to one whole-file scope.
    #[test]
    fn invalid_java_is_unclassifiable_and_located() {
        // Two opening braces, no closes — a partial save.
        let source = "public class Foo {\n    void bar() {\n        doX();";
        match JavaClassifier.classify(source) {
            Classification::Unclassifiable { first, rest } => {
                let mut all = vec![first];
                all.extend(rest);
                assert!(
                    all.iter()
                        .all(|i| i.reason == ClassifierFailure::ParseError),
                    "truncated Java must report ParseError issues, got {all:?}"
                );
                assert!(
                    all.iter().all(|i| i.line >= 1),
                    "every issue must be located, got {all:?}"
                );
            }
            Classification::Classified(_) => {
                panic!("invalid Java must be Unclassifiable, not silently classified")
            }
        }
    }

    /// the parse-error guard must not fire on valid Java. A well-formed
    /// file with a normally-uncommon-but-legal construct (a static initializer
    /// block) still classifies as usual — the guard keys on `has_error()`, not on
    /// unfamiliarity.
    #[test]
    fn valid_java_is_not_treated_as_error() {
        let source = "public class Foo {\n    static { init(); }\n}";
        let result = classify(source);
        assert!(
            result.iter().any(|c| c.is_some()),
            "valid Java must still be classified, got all Unknown"
        );
    }

    // --- Modern Java constructs ---
    //
    // Each of these was verified against a real JaCoCo report generated from the
    // same construct: the classifier's role (which lines are Statement /
    // Declaration / scope boundaries) is chosen to line up with the lines JaCoCo
    // actually emits hit/miss for. Node kinds (`static_initializer`,
    // `record_declaration`, `compact_constructor_declaration`, `switch_rule`)
    // are the tree-sitter-java grammar's own names — leaning on the parser rather
    // than pattern-matching source text.

    /// A `static { … }` initializer runs at class-init and JaCoCo reports its
    /// body lines. The `static` keyword line is a Declaration (transparent to the
    /// backward walk); the inner block supplies the scope. Covers the reviewer's
    /// `static` / `{` split-line hazard: the keyword line is stamped even when the
    /// brace is on a later line.
    #[test]
    fn static_initializer_same_line_brace() {
        let source = "public class Foo {\n    static {\n        setup();\n    }\n}";
        let result = classify(source);
        // line 2: `static {` → Declaration + ScopeOpen (block opens here)
        assert!(has_prop(&result[1], LineProperty::Declaration));
        assert!(has_prop(&result[1], LineProperty::ScopeOpen));
        // line 3: body statement
        assert!(has_prop(&result[2], LineProperty::Statement));
        // line 4: `}` → ScopeClose
        assert!(has_prop(&result[3], LineProperty::ScopeClose));
    }

    /// The `static` keyword on its own line (brace on the next) must still be
    /// classified — otherwise an annotation above the block walks forward into an
    /// unclassified line. This is the exact split the reviewer flagged.
    #[test]
    fn static_initializer_split_line_brace() {
        let source = "public class Foo {\n    static\n    {\n        setup();\n    }\n}";
        let result = classify(source);
        // line 2: `static` → Declaration (not unknown)
        assert!(
            has_prop(&result[1], LineProperty::Declaration),
            "the `static` keyword line must be a Declaration, got: {:?}",
            result[1]
        );
        // line 3: `{` → ScopeOpen
        assert!(has_prop(&result[2], LineProperty::ScopeOpen));
        // line 5: `}` → ScopeClose
        assert!(has_prop(&result[4], LineProperty::ScopeClose));
    }

    /// A record header carries a real JaCoCo verdict (generated accessors / ctor
    /// / equals are attributed to it), so it must be a Declaration that opens the
    /// record body scope — not an unclassified line whose coverage we discard.
    #[test]
    fn record_declaration_header_and_body() {
        let source =
            "public class Foo {\n    record Point(int x, int y) {\n        int sum() {\n            return x + y;\n        }\n    }\n}";
        let result = classify(source);
        // line 2: `record Point(...) {` → Declaration + ScopeOpen
        assert!(has_prop(&result[1], LineProperty::Declaration));
        assert!(has_prop(&result[1], LineProperty::ScopeOpen));
        // line 3: `int sum() {` → Declaration + ScopeOpen (method inside record)
        assert!(has_prop(&result[2], LineProperty::Declaration));
        assert!(has_prop(&result[2], LineProperty::ScopeOpen));
        // line 4: `return x + y;` → Statement
        assert!(has_prop(&result[3], LineProperty::Statement));
    }

    /// A compact canonical constructor (`Point { … }`) is a real, coverage-bearing
    /// constructor. It must behave like any other constructor: Declaration header
    /// opening a scope, executable body inside.
    #[test]
    fn compact_constructor_declaration() {
        let source =
            "public class Foo {\n    record Point(int x) {\n        Point {\n            check(x);\n        }\n    }\n}";
        let result = classify(source);
        // line 3: `Point {` → Declaration + ScopeOpen
        assert!(has_prop(&result[2], LineProperty::Declaration));
        assert!(has_prop(&result[2], LineProperty::ScopeOpen));
        // line 4: `check(x);` → Statement
        assert!(has_prop(&result[3], LineProperty::Statement));
    }

    /// Arrow-form switch arms get a per-arm hit/miss from JaCoCo, so a bare
    /// expression arm is a Statement (a coverage-bearing line), and a block-bodied
    /// arm still opens a scope via its `block` child.
    #[test]
    fn switch_rule_arrow_arms() {
        let source = "public class Foo {\n    String c(int n) {\n        return switch (n) {\n            case 0 -> \"zero\";\n            default -> {\n                yield \"many\";\n            }\n        };\n    }\n}";
        let result = classify(source);
        // line 4: `case 0 -> "zero";` → Statement (its own JaCoCo verdict)
        assert!(
            has_prop(&result[3], LineProperty::Statement),
            "arrow arm must be a Statement, got: {:?}",
            result[3]
        );
        // line 5: `default -> {` → Statement + ScopeOpen (block body)
        assert!(has_prop(&result[4], LineProperty::Statement));
        assert!(has_prop(&result[4], LineProperty::ScopeOpen));
        // line 6: `yield "many";` → Statement
        assert!(has_prop(&result[5], LineProperty::Statement));
        // line 7: `}` → ScopeClose
        assert!(has_prop(&result[6], LineProperty::ScopeClose));
    }

    /// Sealed types need no special handling: tree-sitter parses
    /// `sealed interface … permits …` as an ordinary `interface_declaration`
    /// (the `sealed` modifier and `permits` clause are child tokens), so it
    /// already classifies as a Declaration that opens a scope. This locks that in
    /// so a future refactor doesn't assume it needs a bespoke arm.
    #[test]
    fn sealed_interface_classifies_as_interface() {
        let source =
            "public class Foo {\n    sealed interface Shape permits Circle {\n        double area();\n    }\n}";
        let result = classify(source);
        // line 2: `sealed interface Shape permits Circle {` → Declaration + ScopeOpen
        assert!(has_prop(&result[1], LineProperty::Declaration));
        assert!(has_prop(&result[1], LineProperty::ScopeOpen));
        // line 3: `double area();` → Declaration (abstract method)
        assert!(has_prop(&result[2], LineProperty::Declaration));
        // line 4: `}` → ScopeClose
        assert!(has_prop(&result[3], LineProperty::ScopeClose));
    }

    /// A block-bodied lambda (`() -> { … }`) opens a real scope via its `block`
    /// child — JaCoCo tracks the body lines independently of the creation site,
    /// so the scope boundary is needed for correct attribution.
    #[test]
    fn block_bodied_lambda_opens_scope() {
        let source =
            "public class Foo {\n    void bar() {\n        Runnable r = () -> {\n            work();\n        };\n    }\n}";
        let result = classify(source);
        // line 3: `Runnable r = () -> {` → Statement (field/local w/ init) + ScopeOpen
        assert!(has_prop(&result[2], LineProperty::ScopeOpen));
        // line 4: `work();` → Statement
        assert!(has_prop(&result[3], LineProperty::Statement));
        // line 5: `};` → ScopeClose
        assert!(has_prop(&result[4], LineProperty::ScopeClose));
    }

    /// An expression-bodied lambda (`x -> x * 2`, no braces) has no `block` child,
    /// so it opens no scope — JaCoCo folds it into the enclosing statement's line,
    /// which is already a Statement. Keeping scopes tied to braces avoids
    /// synthesizing a boundary we'd then have to balance. This test asserts the
    /// declaration line is a single-line Statement with no stray ScopeOpen.
    #[test]
    fn expression_bodied_lambda_opens_no_scope() {
        let source = "public class Foo {\n    java.util.function.Function<Integer, Integer> f = x -> x * 2;\n}";
        let result = classify(source);
        // line 2: field with lambda initializer → Statement, but NO ScopeOpen
        assert!(has_prop(&result[1], LineProperty::Statement));
        assert!(
            !has_prop(&result[1], LineProperty::ScopeOpen),
            "brace-less lambda must not open a scope, got: {:?}",
            result[1]
        );
        assert!(!has_prop(&result[1], LineProperty::ScopeClose));
    }
}
