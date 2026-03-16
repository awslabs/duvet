# Duvet Coverage Model v2: Formal Specification

**Version:** 0.1.0-draft
**Date:** 2026-03-16

## 1. Definitions

### 1.1 Source File

A source file is an ordered sequence of lines, indexed from 1 to N.

```
File = Vec<Line>  where  Line = (line_number: u64, content: String)
```

### 1.2 Line Properties

Each line in a source file has a set of properties. A line may have multiple
properties simultaneously (e.g., a method signature that opens a scope).

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

### 1.3 Classification Function

The classification function is the only language-specific component:

```
classify: (language: Language, source: File) → Vec<LineClass>
```

All subsequent operations are language-agnostic — they operate on `LineClass`
values, not on source text.

### 1.4 Coverage Data

Coverage data maps line numbers to execution status:

```
CoverageReport = Map<line_number, CoverageStatus>

CoverageStatus ::=
    | Hit       -- line was executed at least once
    | Miss      -- line is executable but was not executed
```

Lines not present in the coverage report are not considered executable by the
coverage tool. This is distinct from `Miss`, which means the tool considers the
line executable but it was not reached.

### 1.5 Scopes

A scope is a contiguous range of lines delimited by `ScopeOpen` and
`ScopeClose` properties. Scopes nest.

```
Scope = {
    open_line:  u64,
    close_line: u64,
    parent:     Option<ScopeId>,
    children:   Vec<ScopeId>,
}
```

The scope tree is derived from the classification data by matching `ScopeOpen`
and `ScopeClose` lines (balanced parentheses). For indentation-based languages
(Python), the AST parser identifies scope boundaries from block nodes, and the
classification function assigns `ScopeOpen`/`ScopeClose` to the first and last
lines of each block.

Lines not contained in any scope are in the "file scope" (implicit top-level
scope spanning the entire file).

### 1.6 Annotations

An annotation occupies one or more contiguous lines, all classified with the
`Annotation` property.

```
AnnotationSpan = {
    start_line: u64,
    end_line:   u64,
}
```

## 2. Phase 1: Annotation Target Resolution

### 2.1 Purpose

Given an annotation span, determine the source construct it targets by walking
forward. This phase is purely structural — it does not consult coverage data.
It answers: "what did the developer intend to annotate?"

### 2.2 Definition

```
TargetLine = {
    line_number: u64,
    properties:  LineClass,
}

annotation_target(
    annotation: AnnotationSpan,
    classifications: Vec<LineClass>,
    file_length: u64,
) → Option<TargetLine>
```

### 2.3 Algorithm

```
fn annotation_target(annotation, classifications, file_length):
    let current = annotation.end_line + 1

    while current <= file_length:
        let props = classifications[current]

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
            return None

        // Any other combination: this is the target.
        // Could be Statement, Declaration, ScopeOpen, NonLinearControl,
        // or any compound like {Declaration, ScopeOpen}.
        return Some(TargetLine { line_number: current, properties: props })

    // Reached end of file without finding a target.
    return None
```

### 2.4 Properties

- **Deterministic**: For a given annotation and classification, the target is
  always the same.
- **No coverage dependency**: Target resolution is independent of coverage data.
- **Stacking**: Stacked annotations all resolve to the same target (the walk
  skips through intermediate annotations).

## 3. Phase 2: Execution Propagation

### 3.1 Purpose

Given the set of lines reported as executed by the coverage tool, compute the
full set of lines that can be considered "executed by association." This phase
answers: "given that line X was executed, what other lines in the same scope
were necessarily reached?"

### 3.2 Definition

```
execution_set(
    classifications: Vec<LineClass>,
    scopes: Vec<Scope>,
    coverage: CoverageReport,
) → Set<u64>
```

### 3.3 Algorithm

```
fn execution_set(classifications, scopes, coverage):
    let directly_executed = { line | coverage[line] == Hit }
    let result = directly_executed.clone()

    for each scope S in scopes:
        // Conservative fallback: if scope contains non-linear control flow,
        // do not propagate in this scope.
        if any line L in S where classifications[L] contains NonLinearControl:
            continue

        for each line L in directly_executed where L is within S:
            // Walk backward from L
            let current = L - 1

            while current >= S.open_line:
                let props = classifications[current]

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

### 3.4 Properties

- **Scope-bounded**: Propagation never crosses a `ScopeClose` boundary.
- **Statement-bounded**: Propagation stops at another `Statement` (which has
  its own coverage status from the coverage tool).
- **Conservative under non-linear control**: If a scope contains `goto` or
  labels, no propagation occurs in that scope.

## 4. Phase 3: Annotation Execution Check

### 4.1 Purpose

Compose Phase 1 and Phase 2 to determine whether an annotation is executed.

### 4.2 Definition

```
ExecutionStatus ::=
    | Executed       -- target line is in the execution set
    | NotExecuted    -- target line is reachable but not in the execution set
    | Structural     -- target is purely declarative with no executable code
                        in its scope; cannot be verified by execution
    | Unknown        -- cannot determine (non-linear control flow, etc.)

is_annotation_executed(
    annotation: AnnotationSpan,
    classifications: Vec<LineClass>,
    scopes: Vec<Scope>,
    coverage: CoverageReport,
) → ExecutionStatus
```

### 4.3 Algorithm

```
fn is_annotation_executed(annotation, classifications, scopes, coverage):
    // Phase 1: What does this annotation target?
    let target = annotation_target(annotation, classifications, file_length)

    match target:
        None →
            return Structural  // annotation targets nothing (dangling or EOF)

        Some(target_line) →
            let props = target_line.properties

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
                    classifications[line] contains Statement
                if not has_any_statements:
                    return Structural  // e.g., interface with no executable code
                else:
                    return NotExecuted  // there are statements, they just weren't executed

            return NotExecuted
```

## 5. Correctness Properties

These properties are to be proven with Verus.

### Property 1: No False Positives

If `is_annotation_executed(annotation, ...) = Executed`, then there exists a
line L such that:

- `coverage[L] == Hit` (L is directly reported as executed)
- L is in the same scope as the annotation's target
- Every line between L and the annotation's target (exclusive) has properties
  that are a subset of {Whitespace, Comment, Annotation, Declaration, ScopeOpen}
- No line between L and the annotation's target has the `ScopeClose` property

### Property 2: No Cross-Scope Leakage

For any two lines A and B where A is in scope S1 and B is in scope S2 and
S1 ≠ S2 and S1 is not a parent of S2 and S2 is not a parent of S1:

- If `coverage[A] == Hit` and `coverage[B] != Hit`, then B ∉ execution_set

In other words: execution of a line in one scope never causes a line in a
sibling or unrelated scope to appear in the execution set.

### Property 3: Conservative Fallback

If any line in scope S has the `NonLinearControl` property, then for all lines
L in S:

- L ∈ execution_set if and only if `coverage[L] == Hit`

No backward propagation occurs in scopes containing non-linear control flow.

### Property 4: Monotonicity

Let E1 and E2 be two coverage reports where E1 ⊆ E2 (E2 reports all the same
hits as E1, plus possibly more). Then:

- execution_set(classifications, scopes, E1) ⊆ execution_set(classifications, scopes, E2)

Adding more executed lines can only increase the execution set, never decrease
it.

### Property 5: Annotation Stacking Transitivity

If annotation A (lines a1..a2) is immediately above annotation B (lines
b1..b2) with only whitespace between them, and
`is_annotation_executed(B, ...) = Executed`, then
`is_annotation_executed(A, ...) = Executed`.

This follows from: A and B resolve to the same target (Phase 1 walks through
both), and if B's target is in the execution set, A's target is too (same
target).

## 6. Worked Examples

### 6.1 Annotation before method signature (Java)

```java
1:  //= spec.md#section-1              Annotation
2:  //# MUST do X                       Annotation
3:  public void foo() {                 {Declaration, ScopeOpen}
4:      int temp;                       {Declaration}
5:      doX();                          {Statement}  ← coverage: Hit
6:  }                                   {ScopeClose}
```

Phase 1: Annotation (lines 1-2) → target = line 3 {Declaration, ScopeOpen}.

Phase 2: Line 5 is Hit. Walk backward in scope (lines 3-6):
- Line 4: {Declaration} → propagate. result += {4}
- Line 3: {ScopeOpen} → propagate, stop. result += {3}

Execution set = {3, 4, 5}.

Phase 3: Target (line 3) ∈ execution set → **Executed**.

### 6.2 Annotation on interface (Java)

```java
1:  //= spec.md#keyring                Annotation
2:  //# MUST define OnEncrypt           Annotation
3:  public interface IKeyring {         {Declaration, ScopeOpen}
4:      OnEncryptOutput OnEncrypt(      {Declaration}
5:          OnEncryptInput input        {Declaration}
6:      );                              {Declaration}
7:  }                                   {ScopeClose}
```

Phase 1: Annotation (lines 1-2) → target = line 3 {Declaration, ScopeOpen}.

Phase 2: No line in scope (3-7) has `coverage[line] == Hit`. No propagation.
Execution set = {} (for this scope).

Phase 3: Target (line 3) ∉ execution set. Target is {Declaration, ScopeOpen}
(no Statement). Scope contains no Statements → **Structural**.

### 6.3 Cross-method leakage prevention

```java
1:  public void foo() {                 {Declaration, ScopeOpen}
2:      //= spec.md#section-1          Annotation
3:      //# MUST do X                   Annotation
4:      doX();                          {Statement}  ← coverage: Hit
5:  }                                   {ScopeClose}
6:                                      {Whitespace}
7:  public void bar() {                 {Declaration, ScopeOpen}
8:      doY();                          {Statement}  ← coverage: Hit
9:  }                                   {ScopeClose}
```

Phase 1: Annotation (lines 2-3) → target = line 4 {Statement}.

Phase 2: Line 4 is Hit in scope (1-5). Walk backward:
- Line 3: {Annotation} → propagate. result += {3}
- Line 2: {Annotation} → propagate. result += {2}
- Line 1: {ScopeOpen} → propagate, stop. result += {1}

Line 8 is Hit in scope (7-9). Walk backward:
- Line 7: {ScopeOpen} → propagate, stop. result += {7}

Execution set = {1, 2, 3, 4, 7, 8}.

Phase 3: Target (line 4) ∈ execution set → **Executed**.

Note: Line 8's execution does NOT propagate to lines 5 or 6. The `ScopeClose`
at line 5 and the scope boundary at line 7 prevent leakage.

### 6.4 Variable declaration without initializer

```java
1:  public void foo() {                 {Declaration, ScopeOpen}
2:      //= spec.md#section-1          Annotation
3:      //# MUST compute X              Annotation
4:      int result;                     {Declaration}
5:      result = computeX();            {Statement}  ← coverage: Hit
6:  }                                   {ScopeClose}
```

Phase 1: Annotation (lines 2-3) → target = line 4 {Declaration}.

Phase 2: Line 5 is Hit. Walk backward in scope (1-6):
- Line 4: {Declaration} → propagate. result += {4}
- Line 3: {Annotation} → propagate. result += {3}
- Line 2: {Annotation} → propagate. result += {2}
- Line 1: {ScopeOpen} → propagate, stop. result += {1}

Execution set = {1, 2, 3, 4, 5}.

Phase 3: Target (line 4) ∈ execution set → **Executed**.

### 6.5 Stacked annotations

```java
1:  public void foo() {                 {Declaration, ScopeOpen}
2:      //= spec.md#section-1          Annotation
3:      //# MUST do X                   Annotation
4:      //= spec.md#section-2          Annotation
5:      //# MUST do Y                   Annotation
6:      doXandY();                      {Statement}  ← coverage: Hit
7:  }                                   {ScopeClose}
```

Phase 1 for annotation A (lines 2-3): Walk forward → line 4 is Annotation →
skip → line 5 is Annotation → skip → line 6 is Statement → target = line 6.

Phase 1 for annotation B (lines 4-5): Walk forward → line 6 is Statement →
target = line 6.

Both annotations target line 6. Line 6 is Hit → both are **Executed**.

### 6.6 C code with goto (conservative fallback)

```c
1:  void foo() {                        {Declaration, ScopeOpen}
2:      //= spec.md#section-1          Annotation
3:      //# MUST do X                   Annotation
4:      int x;                          {Declaration}
5:      goto skip;                      {NonLinearControl, Statement}  ← coverage: Hit
6:      do_x();                         {Statement}  ← coverage: Miss
7:  skip:                               {NonLinearControl}
8:      do_y();                         {Statement}  ← coverage: Hit
9:  }                                   {ScopeClose}
```

Phase 2: Scope (1-9) contains NonLinearControl (lines 5, 7) → **no
propagation**. Execution set = {5, 8} (only directly hit lines).

Phase 1: Annotation (lines 2-3) → target = line 4 {Declaration}.

Phase 3: Target (line 4) ∉ execution set → **NotExecuted**.

This is the conservative fallback. Without `goto`, line 4 would have been in
the execution set via backward propagation from line 5. With `goto`, we can't
be sure line 4 was actually reached, so we don't propagate.
