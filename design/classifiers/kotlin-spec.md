# Kotlin Line Classifier: Specification

**Version:** 1.0.0
**Date:** 2026-08-06

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT",
"SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this
document are to be interpreted as described in
[RFC 2119](https://www.rfc-editor.org/rfc/rfc2119).

This document is the normative node-kind → `LineProperty` mapping for the
Kotlin line classifier. It instantiates, for Kotlin, the language-generic
classifier contract of the
[coverage model specification](../query/coverage-model-spec.md):
§1.3 (classification function, `None` semantics, NonLinearControl and
mutual-exclusivity contracts), §1.5 (scope balance or escalate), and §8
(dispatch). Where this document is silent, the generic contract governs.
The evidence behind every rule is in
[kotlin-decisions.md](./kotlin-decisions.md),
whose decisions map one-to-one onto this document's rules.

## 1. Grammar {#grammar}

The classifier MUST parse Kotlin source with the `tree-sitter-kotlin-ng`
grammar.
Node-kind names in this document are that grammar's names.

## 2. Pre-pass and post-pass {#pre-and-post-pass}

Before the CST walk, the classifier MUST mark
blank lines as `Whitespace`
and duvet annotation lines (`//=` or `//#` after leading whitespace)
as `Annotation`.

After the CST walk, the classifier MUST apply the verified
mutual-exclusivity post-pass (`clean_classifications`)
to every classification it returns.

## 3. Parse errors {#parse-errors}

When the parse tree contains any `ERROR` or `MISSING` node,
the classifier MUST return `Unclassifiable`
and MUST report the line of every `ERROR` and `MISSING` node.
As defense in depth, a parser that yields no tree at all
is likewise treated as unclassifiable, reported at line 1;
tree-sitter does not produce this outcome for in-memory input,
so the path is untestable and carries no normative keyword.

## 4. Scope-bearing nodes {#scope-bearing-nodes}

The scope-bearing node kinds are
`block`, `class_body`, `enum_class_body`, `lambda_literal`,
and `when_expression`.
For each scope-bearing node, the classifier MUST mark
`ScopeOpen` on the line of its opening brace token
and `ScopeClose` on the line of its closing brace token.
The scope-event stream MUST contain one open and one close event
per scope-bearing node, keyed at the byte offsets of its brace tokens,
in source order.
`when_expression` and `lambda_literal` carry their brace tokens
as direct children rather than in a child `block`;
their events MUST be keyed on those tokens.

## 5. Declarations {#declarations}

### 5.1 Declarations with scope {#declarations-with-scope}

The node kinds
`class_declaration`, `object_declaration`, `companion_object`,
`function_declaration`, `secondary_constructor`,
`anonymous_initializer`, `getter`, and `setter`
MUST be marked with the declaration-with-scope treatment:
every line from the node's start to the line before its body
is marked `Declaration`,
and the body child supplies the scope events.
When the node has no brace-form body,
the treatment falls back as specified per kind below.

A function header line MUST NOT be marked `Statement`,
including when its parameters carry default-value expressions.

Class-like headers (including `data`, `sealed`, `enum`, and
`annotation` classes, interfaces, objects, and companion objects)
carry JaCoCo's synthesized-member and primary-constructor probes;
they MUST be marked `Declaration` and MUST NOT be marked `Statement`.

### 5.2 Declarations without scope {#declarations-without-scope}

The node kinds
`package_header`, `import`, `type_alias`, and `annotation`
MUST mark their span `Declaration`.
A `function_declaration` with neither a brace-form body
nor an expression body (an abstract or interface member)
MUST mark its span `Declaration` only.

### 5.3 Properties {#properties}

A `property_declaration` MUST mark its span `Declaration`.
When it carries an initializer or a `property_delegate` child,
it MUST additionally mark `Statement`
from its start line through the end line
of the initializer or delegate,
and MUST NOT mark `Statement` on any accessor line after it.

### 5.4 Enum entries {#enum-entries}

An `enum_entry` MUST mark its span
with both `Declaration` and `Statement`,
with or without constructor arguments.

## 6. Statements {#statements}

### 6.1 Expression-body functions {#expression-body-functions}

A `function_body` in `=` form (expression body)
MUST mark the expression's span `Statement`.
The declaration header keeps its `Declaration` marking;
a line shared by both is `{Declaration, Statement}`.

### 6.2 Statement-position expressions {#statement-position-expressions}

The node kinds
`call_expression`, `navigation_expression`, `assignment`,
`return_expression`, and `throw_expression`
MUST mark their span `Statement`.

### 6.3 Control headers {#control-headers}

The node kinds
`if_expression`, `when_expression`,
`for_statement`, and `while_statement`
MUST mark `Statement` on their start line only.
A `do_while_statement` MUST mark `Statement` on its end line
(the `} while (condition)` line) and MUST NOT mark its start line.

### 6.4 When arms {#when-arms}

A `when_entry` MUST mark `Statement` on its start line.

### 6.5 Try/catch/finally headers {#try-catch-finally}

`try_expression`, `catch_block`, and `finally_block`
MUST mark `Declaration` on their start lines;
their `block` children supply the scopes.

## 7. Non-linear control {#non-linear-control}

A `label` node (a label declaration such as `outer@` or `lit@`)
MUST mark `NonLinearControl` on its line.
A `labeled_expression` (`break@label`, `continue@label`)
MUST mark `NonLinearControl` on its line.
A `return_expression` whose first token is `return@`
(a labeled return) MUST be marked `Statement`,
not `NonLinearControl`.

## 8. Comments {#comments}

`line_comment` and `block_comment` nodes
MUST mark their span `Comment`,
except lines already marked `Annotation` by the pre-pass.

## 9. Unmodeled constructs {#unmodeled-constructs}

Any node kind this document does not name
contributes no property of its own;
its lines keep whatever enclosing nodes marked,
or remain `None` (unknown) per the generic contract's Decision 9.
Every arm of the classifier that marks a code or structural property
MUST record the marked node's start line as a code-start line
for the post-pass's `{code, Comment}` disambiguation.
