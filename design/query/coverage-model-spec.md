# Duvet Coverage Model: Formal Specification

**Version:** 1.0.0
**Date:** 2026-03-19

## 1. Definitions {#definitions}

### 1.1 Source File {#source-file}

A source file is an ordered sequence of lines,
indexed from 1 to N.

```
File = Vec<Line>  where  Line = (line_number: u64, content: String)
```

### 1.2 Line Properties {#line-properties}

Each line in a source file has a set of properties.
A line may have multiple properties simultaneously
(e.g., a method signature that opens a scope).

```
LineProperty ::=
    | Statement           -- executable code (assignment, call, return, throw, etc.)
    | Declaration         -- structural definition (method sig, class decl,
                             field, import, package, abstract method, etc.)
    | ScopeOpen           -- opens a new lexical scope
    | ScopeClose          -- closes a lexical scope
    | Comment             -- non-annotation comment text
    | Annotation          -- a duvet annotation line
    | Whitespace          -- blank or whitespace-only
    | NonLinearControl    -- goto, label, or non-linear control flow
```

A line's classification is the set of all its properties:

```
LineClass = Set<LineProperty>
```

Examples of compound classifications:

- `public void foo() {` → {Declaration, ScopeOpen}
- `AES_GCM_128(128, 12, 16),` → {Declaration, Statement}
- `private static final X = new Y();` → {Declaration, Statement}
- `def foo():` (Python) → {Declaration, ScopeOpen}
- `int x;` → {Declaration}
- `x = 5;` → {Statement}
- `}` → {ScopeClose}
- `// comment` → {Comment}
- `//= spec.md#section` → {Annotation}
- (blank line) → {Whitespace}
- `goto label;` → {NonLinearControl, Statement}

### 1.3 Classification Function and Unknown Lines {#classification-function}

The classification function is the only language-specific component:

```
classify: (language: Language, source: File) → Vec<Option<LineClass>>
```

The return type is `Option<LineClass>`, not `LineClass`.
A value of `None` means the classifier could not determine
the line's properties —
the line is **unknown**.
A value of `Some(s)` means the line was classified
with property set `s`.

Unknown (`None`) is not a `LineProperty` —
it is the absence of classification.
This distinction is important:
a classified line has a known set of properties (possibly empty),
while an unknown line has no properties at all.
The type system prevents incoherent combinations
like `{Unknown, ScopeOpen}`
because `Unknown` does not exist in the `LineProperty` enum.

All subsequent operations are language-agnostic —
they operate on `Option<LineClass>` values,
not on source text.

**NonLinearControl contract:**
For the correctness of execution propagation (Property 3),
classifiers are expected to assign `NonLinearControl`
to both the source and target of any non-linear control flow.
For example,
both `goto label;` and `label:` should be classified
with `NonLinearControl`.
This ensures that any scope which can be entered via a jump
contains a `NonLinearControl` line,
disabling backward propagation in that scope.
If a language has non-linear control flow
where the target is not syntactically distinguishable
(e.g., BASIC's `GOTO <line-number>`),
the classifier should conservatively add `NonLinearControl`
to all lines in the file.
See [Property 3](#property-3-conservative-fallback)
for the soundness argument.

**Mutual exclusivity contract:** {#mutual-exclusivity-contract}
The properties `Annotation`, `Comment`, and `Whitespace`
are mutually exclusive with `Statement` and `Declaration`.
A line classified as `{Annotation}` MUST NOT also have
`Statement` or `Declaration` in its property set.
The same applies to `Comment` and `Whitespace`.

This invariant is necessary for the correctness
of Phase 1 (target resolution).
The forward walk skips lines whose properties are exactly
`{Whitespace}`, `{Comment}`, or contain `Annotation`.
If a line had `{Annotation, Statement}`,
the walk would skip it (because it contains `Annotation`),
but the line is actually executable code —
the `Statement` property would be silently ignored.
This would produce correct behavior by accident
(the annotation is skipped),
but the classification is semantically wrong
and would cause incorrect behavior in Phase 2
(backward propagation stops at `Statement`,
so a contaminated annotation line would block propagation).

This situation arises in practice
when a tree-sitter AST node spans multiple lines
(e.g., a fluent builder chain classified as a single
`local_variable_declaration`).
The classifier marks all lines in the node's span
with `Statement` and `Declaration`,
including lines that are actually annotations or comments.

Classifiers MUST apply a post-processing pass
after AST classification:
for any line that has `Annotation`, `Comment`, or `Whitespace`,
remove `Statement` and `Declaration` from its property set.
This ensures that annotation lines are always pure `{Annotation}`,
comment lines are always pure `{Comment}`,
and whitespace lines are always pure `{Whitespace}`.

When no classifier is available for a language,
all lines start as `None`
and are reclassified incrementally by coverage data,
annotation detection,
and whitespace detection.
Lines that remain `None` after all passes are unknown.

### 1.4 Coverage Data {#coverage-data}

Coverage data maps line numbers to execution status:

```
CoverageReport = Map<line_number, CoverageStatus>

CoverageStatus ::=
    | Hit       -- line was executed at least once
    | Miss      -- line is executable but was not executed
```

Lines not present in the coverage report
are not considered executable by the coverage tool.
This is distinct from `Miss`,
which means the tool considers the line executable
but it was not reached.

### 1.5 Scopes {#scopes}

A scope is a contiguous range of lines
delimited by `ScopeOpen` and `ScopeClose` properties.
Scopes nest.

```
Scope = {
    open_line:  u64,
    close_line: u64,
    parent:     Option<ScopeId>,
    children:   Vec<ScopeId>,
}
```

The scope tree is derived from the classification data
by matching `ScopeOpen` and `ScopeClose` lines
(balanced parentheses).
For indentation-based languages (Python),
the AST parser identifies scope boundaries from block nodes,
and the classification function assigns
`ScopeOpen`/`ScopeClose` to the first and last lines of each block.

Lines not contained in any scope are in the "file scope"
(implicit top-level scope spanning the entire file).

Unknown lines (`None`) do not contribute to scope construction.
If a line is unknown,
it cannot be a `ScopeOpen` or `ScopeClose`,
so it does not affect the scope tree.

### 1.6 Annotations {#annotations}

An annotation occupies one or more contiguous lines,
all classified with the `Annotation` property.

```
AnnotationSpan = {
    start_line: u64,
    end_line:   u64,
}
```

## 2. Phase 1: Annotation Target Resolution {#annotation-target-resolution}

### 2.1 Purpose {#annotation-target-resolution-purpose}

Given an annotation span,
determine the source construct it targets by walking forward.
This phase is purely structural —
it does not consult coverage data.
It answers:
"what did the developer intend to annotate?"

### 2.2 Definition {#annotation-target-resolution-definition}

The target line's properties are `Option<LineClass>`
to account for unknown lines.
When the forward walk lands on an unknown line,
the target is returned with `properties: None`
so that Phase 3 can report `ExecutionStatus::Unknown`
with the specific line number.

```
TargetLine = {
    line_number: u64,
    properties:  Option<LineClass>,
}

annotation_target(
    annotation: AnnotationSpan,
    classifications: Vec<Option<LineClass>>,
    file_length: u64,
) → Option<TargetLine>
```

### 2.3 Algorithm {#annotation-target-resolution-algorithm}

```
fn annotation_target(annotation, classifications, file_length):
    let current = annotation.end_line + 1

    while current <= file_length:
        let class = classifications[current]

        match class:
            None →
                // Unknown line — cannot resolve through it.
                // Return it as the target so Phase 3 can report Unknown
                // with the specific line number for diagnostics.
                return Some(TargetLine { line_number: current, properties: None })

            Some(props) →
                if props == {Whitespace}:
                    current += 1
                    continue

                if props == {Comment}:
                    current += 1
                    continue

                if props == {Annotation}:
                    // Stacked annotation — skip through it to find the
                    // shared target. Walk past all contiguous annotation lines.
                    current += 1
                    continue

                if props contains ScopeClose and not (Statement or Declaration or ScopeOpen):
                    // Reached a closing brace with no substantive content.
                    // The annotation is dangling — it targets nothing.
                    //
                    // The compound check is necessary because some lines have
                    // ScopeClose combined with other properties:
                    //   `} catch (Exception e) {` → {ScopeClose, ScopeOpen, Declaration}
                    //   `} while (condition);`    → {ScopeClose, Statement}
                    // These are real targets, not dangling braces.
                    // Only a bare `}` (ScopeClose alone) is dangling.
                    return None

                // Any other combination: this is the target.
                // Could be Statement, Declaration, ScopeOpen, NonLinearControl,
                // or any compound like {Declaration, ScopeOpen}.
                return Some(TargetLine { line_number: current, properties: Some(props) })

    // Reached end of file without finding a target.
    return None
```

**Note on Whitespace and Comment checks:**
The checks `props == {Whitespace}` and `props == {Comment}`
use equality, not containment.
This means a line with `{Whitespace, Comment}` would match
the `{Comment}` check (not the `{Whitespace}` check),
and a line with `{Whitespace, ScopeOpen}` would match neither —
it would fall through to the target case.
In practice,
the [mutual exclusivity contract](#mutual-exclusivity-contract)
ensures that `Whitespace` and `Comment` do not appear
in compound classifications with `Statement` or `Declaration`,
so the only realistic compound cases are
`{Whitespace}`, `{Comment}`, or `{Whitespace, Comment}`.
The `Annotation` check uses containment (`props contains Annotation`)
because annotations may appear on lines
that the AST parser also visits,
though the mutual exclusivity contract
ensures `Statement` and `Declaration` are stripped.

### 2.4 Properties {#annotation-target-resolution-properties}

- **Deterministic:**
  For a given annotation and classification,
  the target is always the same.
- **No coverage dependency:**
  Target resolution is independent of coverage data.
- **Stacking:**
  Stacked annotations all resolve to the same target
  (the walk skips through intermediate annotations).
- **Unknown stops the walk:**
  An unknown line becomes the target (with `properties: None`)
  rather than being skipped.
  This ensures unknown lines cannot be silently bypassed.

## 3. Phase 2: Execution Propagation {#execution-propagation}

### 3.1 Purpose {#execution-propagation-purpose}

Given the set of lines reported as executed by the coverage tool,
compute the full set of lines
that can be considered "executed by association."
This phase answers:
"given that line X was executed,
what other lines in the same scope were necessarily reached?"

### 3.2 Definition {#execution-propagation-definition}

```
execution_set(
    classifications: Vec<Option<LineClass>>,
    scopes: Vec<Scope>,
    coverage: CoverageReport,
) → Set<u64>
```

### 3.3 Algorithm {#execution-propagation-algorithm}

```
fn execution_set(classifications, scopes, coverage):
    let directly_executed = { line | coverage[line] == Hit }
    let result = directly_executed.clone()

    for each scope S in scopes:
        // Conservative fallback: if scope contains non-linear control flow,
        // do not propagate in this scope.
        if any line L in S where classifications[L] is Some(props) and props contains NonLinearControl:
            continue

        for each line L in directly_executed where L is within S:
            // Walk backward from L
            let current = L - 1

            while current >= S.open_line:
                let class = classifications[current]

                match class:
                    None →
                        // Unknown line — cannot propagate through it.
                        break

                    Some(props) →
                        if props contains ScopeClose:
                            // Hit a closing brace of a nested scope.
                            // Do not propagate into or through it.
                            break

                        if props contains Statement:
                            // Hit another statement. It has its own coverage status.
                            // Do not propagate past it.
                            break

                        // Line is Whitespace, Comment, Annotation, Declaration,
                        // ScopeOpen, or a compound containing only these.
                        // Propagate execution to this line.
                        result.add(current)

                        if props contains ScopeOpen:
                            // Reached the opening of this scope.
                            // Include it but do not propagate further.
                            break

                        current -= 1

    return result
```

### 3.4 Properties {#execution-propagation-properties}

- **Scope-bounded:**
  Propagation never crosses a `ScopeClose` boundary.
- **Statement-bounded:**
  Propagation stops at another `Statement`
  (which has its own coverage status from the coverage tool).
- **Unknown-bounded:**
  Propagation never crosses an unknown (`None`) line.
- **Conservative under non-linear control:**
  If a scope contains `goto` or labels,
  no propagation occurs within that scope.
  Propagation may still occur through a child scope
  that does not itself contain non-linear control.

## 4. Phase 3: Annotation Execution Check {#annotation-execution-check}

### 4.1 Purpose {#annotation-execution-check-purpose}

Compose Phase 1 and Phase 2
to determine whether an annotation is executed.

### 4.2 Definition {#annotation-execution-check-definition}

```
ExecutionStatus ::=
    | Executed       -- target line is in the execution set
    | NotExecuted    -- target line is reachable but not in the execution set
    | Structural     -- target is purely declarative with no executable code
                        in its scope; cannot be verified by execution
    | Unknown        -- cannot determine (unclassified line, non-linear control flow, etc.)

is_annotation_executed(
    annotation: AnnotationSpan,
    classifications: Vec<Option<LineClass>>,
    scopes: Vec<Scope>,
    coverage: CoverageReport,
) → ExecutionStatus
```

### 4.3 Algorithm {#annotation-execution-check-algorithm}

```
fn is_annotation_executed(annotation, classifications, scopes, coverage):
    // Phase 1: What does this annotation target?
    let target = annotation_target(annotation, classifications, file_length)

    match target:
        None →
            return Structural  // annotation targets nothing (dangling or EOF)

        Some(target_line) →
            match target_line.properties:
                None →
                    // Target is an unknown line. Cannot determine execution.
                    return Unknown

                Some(props) →
                    if props contains NonLinearControl:
                        return Unknown  // can't reason about non-linear control flow

                    // Phase 2: Is the target in the execution set?
                    let exec_set = execution_set(classifications, scopes, coverage)

                    if target_line.line_number ∈ exec_set:
                        return Executed

                    // Target is not in execution set.
                    // Distinguish "not executed" from "structurally non-executable."
                    if props contains Statement:
                        // Target is a statement that was not executed.
                        return NotExecuted

                    if props contains Declaration and not Statement:
                        // Target is purely declarative. Check if there are any
                        // executable statements in the same scope that could have
                        // propagated execution to it.
                        let scope = find_scope_containing(target_line.line_number, scopes)
                        let has_any_statements = any line in scope where
                            classifications[line] is Some(p) and p contains Statement
                        if not has_any_statements:
                            return Structural  // e.g., interface with no executable code
                        else:
                            return NotExecuted  // there are statements, they just weren't executed

                    return NotExecuted
```

## 5. Correctness Properties {#correctness-properties}

These properties MUST be proven with Verus.
Each property below defines a correctness invariant of the coverage model.
The Verus proof files MUST carry duvet annotations
linking each `proof fn` back to the corresponding property section
in this document.

### Property 1: No False Positives {#property-1-no-false-positives}

The implementation MUST prove that if
`is_annotation_executed(annotation, ...) = Executed`,
then there exists a line L such that:

- `coverage[L] == Hit`
  (L is directly reported as executed)
- L is in the same scope as the annotation's target
- Every line between L and the annotation's target (exclusive)
  is classified (`Some`)
  and has properties that are a subset of
  {Whitespace, Comment, Annotation, Declaration}
- No line between L and the annotation's target
  has the `ScopeClose` property
- No line between L and the annotation's target
  has the `ScopeOpen` property
- No line between L and the annotation's target
  is unknown (`None`)

Note that `ScopeOpen` is excluded from the between-lines subset even though a
`ScopeOpen` line can itself be propagated to. The backward walk (§3.3) *includes*
a `ScopeOpen` line and then stops — "reached the opening of this scope; include
it but do not propagate further." A `ScopeOpen` is a scope-entry boundary:
reaching a hit inside the block proves the opener ran, so it is included, but the
walk deliberately refuses to reason backward *past* the entry from a hit inside
the block. Anything above the opener belongs to the enclosing scope's own
propagation. Stopping is the conservative choice — it can only drop lines from the
result (a completeness cost), never add a false positive. Consequently a
`ScopeOpen` line may only ever be the *terminal* (topmost) line of a propagation
path, never one strictly between L and the target.

### Property 2: No Cross-Scope Leakage {#property-2-no-cross-scope-leakage}

The implementation MUST prove that
for any two lines A and B
where A is in scope S1 and B is in scope S2
and S1 ≠ S2
and S1 is not a parent of S2
and S2 is not a parent of S1:

- If `coverage[A] == Hit` and `coverage[B] != Hit`,
  then B ∉ execution_set

In other words:
execution of a line in one scope never causes a line
in a sibling or unrelated scope
to appear in the execution set.

### Property 3: Conservative Fallback {#property-3-conservative-fallback}

Backward propagation relies on a sequential execution model:
if there is no control flow redirection between two lines,
and one executed,
the other must have too.
The `NonLinearControl` property (goto, labels) breaks this model
because control may jump over lines without executing them.

The implementation MUST prove that
no backward propagation occurs WITHIN a scope
that contains a `NonLinearControl` line.
Formally:
if a line L is in the execution set via propagation
(not directly hit),
then the scope through which propagation occurred
does not contain any `NonLinearControl` line.

For nested scopes,
a line L may belong to multiple scopes
(a child and its ancestors).
If an ancestor scope S contains `NonLinearControl`
but a child scope S' does not,
propagation MAY occur through S'.
This is sound because:

1. The `NonLinearControl` line is in S but not in S'
   (it is outside S' boundaries or S' does not contain it).
2. The backward walk in S' only considers lines
   within S'.open_line to S'.close_line.
3. No `NonLinearControl` exists
   between the propagated line and the hit line within S',
   so sequential execution holds within S'.
4. A goto in the parent scope S
   cannot redirect control flow within the child scope S'
   without first exiting S'
   (which would cross a ScopeClose, stopping propagation).

The `has_valid_path` predicate encodes this precisely:
`!scope_has_non_linear_control(classifications, scopes, scope_idx)`
applies to the propagation scope,
not to all ancestor scopes.

**Classifier requirement:**
The soundness of this property depends on
the classification function marking BOTH the source (e.g., `goto`)
and the target (e.g., label) of non-linear control flow
with `NonLinearControl`.
This ensures that any scope which can be jumped into
contains a `NonLinearControl` line (the label),
disabling propagation in that scope.

If a language permits non-linear control flow
where the target cannot be distinguished from ordinary code
(e.g., BASIC's `GOTO <line-number>`
where the target line has no syntactic marker),
the classifier for that language should add `NonLinearControl`
to every line in the file.
This pushes the conservatism into the classifier,
preserving the algorithm's soundness.

### Property 4: Monotonicity {#property-4-monotonicity}

The implementation MUST prove that
given two coverage reports E1 and E2
where E1 ⊆ E2
(E2 reports all the same hits as E1, plus possibly more):

- execution_set(classifications, scopes, E1) ⊆
  execution_set(classifications, scopes, E2)

Adding more executed lines can only increase the execution set,
never decrease it.

### Property 5: Annotation Stacking Transitivity {#property-5-stacking-transitivity}

The implementation MUST prove that
if annotation A (lines a1..a2) is immediately above
annotation B (lines b1..b2)
with only whitespace, comments, or other annotations between them,
and `is_annotation_executed(B, ...) = Executed`,
then `is_annotation_executed(A, ...) = Executed`.

This follows from:
A and B resolve to the same target
(Phase 1 walks through all three: annotations, whitespace, and comments),
and if B's target is in the execution set,
A's target is too (same target).

### Property 6: Unknown Safety {#property-6-unknown-safety}

The implementation MUST prove that
unknown lines cannot produce false positives.
Specifically:

If `is_annotation_executed(annotation, ...) = Executed`, then:

- The annotation's target (from Phase 1)
  has `properties: Some(props)` —
  i.e., the target is not an unknown line.
- Every line between the directly-hit line L
  and the annotation's target (exclusive)
  has classification `Some(_)` —
  i.e., no unknown line exists on the propagation path.

In other words:
an `Executed` result is never based on crossing or landing on
an unclassified line.
Unknown lines block both target resolution (Phase 1)
and execution propagation (Phase 2),
so they can only produce `Unknown` or `NotExecuted` results,
never `Executed`.

### Property 7: Target Determinism {#property-7-target-determinism}

`annotation_target` is a pure function:
given the same annotation, classifications, and file length,
it always returns the same result.
This is free in Verus
(all `fn` in Verus are deterministic by construction).

### Property 8: Scope Well-formedness {#property-8-scope-well-formedness}

`build_scope_tree` produces a well-formed scope tree:

- Every scope has `open_line <= close_line`.
- If two scopes overlap,
  one strictly contains the other (proper nesting).
  No partial overlaps.

### Property 9: Execution Set Containment {#property-9-execution-set-containment}

The execution set always contains all directly-hit lines.
Formally:

If `coverage[line] == Hit`,
then `line ∈ execution_set(classifications, scopes, coverage)`.

Execution propagation only adds lines to the set —
it never removes directly-hit lines.

### Property 10: Annotation Target Bounds {#property-10-annotation-target-bounds}

If `annotation_target(annotation, ...) = Some(target)`,
then `target.line_number > annotation.end_line`.

The target is always strictly after the annotation.
This follows from the forward walk starting at
`annotation.end_line + 1`.

## 6. Worked Examples

See [coverage-model-examples.md](coverage-model-examples.md)
for worked examples illustrating each phase of the algorithm.
