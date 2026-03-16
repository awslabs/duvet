# Duvet Coverage Model v2: Implementation Plan

**Date:** 2026-03-16
**Status:** Draft
**Depends on:** coverage-model-v2-spec.md, coverage-model-v2-decisions.md

## Overview

This plan implements the language-aware coverage model specified in
`coverage-model-v2-spec.md`. The work is organized into 9 tasks with explicit
dependencies. Tasks 1 and 2 can run in parallel. Task 0 is a prerequisite
for Task 1.

```
Task 0 (duvet init for spec) ─→ Task 1 (Verus proofs + duvet annotations) ──┐
                                                                              ├─→ Task 4 (target resolution)
                                 Task 2 (tree-sitter) ─→ Task 3 ────────────┤
                                        (classify)       (scopes)           ├─→ Task 5 (execution propagation)
                                                                              │
                                                                              └─→ Task 6 (compose + replace is_annotation_executed)
                                                                                       │
                                                                                       ├─→ Task 7 (integration tests)
                                                                                       │
                                 Task 2 ────────────────────────────────────────────→ Task 8 (additional languages)
```

## Task 0: Duvet Configuration for Spec Traceability

**Objective:** Configure duvet to track the coverage model spec document so
that `duvet report` can verify the Verus proofs provide coverage of the spec
requirements.

**Location:** `duvet/` project root

**Guidance:**

- Run `duvet init` or manually create `.duvet/config.toml` to register
  `design/coverage-model-v2-spec.md` as a tracked specification.
- The spec uses markdown heading anchors (`{#anchor-name}`) for stable
  references. Verify duvet can parse these as section targets.
- The normative requirements in the spec use MUST language (e.g., "The
  implementation MUST prove Property 1"). These are the requirements that
  Verus proof annotations will reference.
- Verify that `duvet report` can scan `.rs` files inside `verus! { }` blocks
  for `//=` and `//#` annotations. Duvet annotations are standard Rust
  comments, so this should work without modification.

**Files to create/modify:**

- `.duvet/config.toml` — add spec document registration

**Demo:** `duvet report` runs and shows the spec requirements as uncovered
(since no proofs exist yet).

**Depends on:** Nothing.

## Task 1: Verus Proof Scaffolding

**Objective:** Define the core types and algorithms in Verus-compatible Rust.
Prove the six correctness properties from the spec. Each proof function MUST
carry duvet annotations linking it to the corresponding spec property.

**Location:** `duvet/src/query/coverage_model/`

**Guidance:**

- Define `LineProperty`, `LineClass` (as a set/bitflags of `LineProperty`),
  `AnnotationSpan`, `Scope`, `CoverageReport`, `ExecutionStatus` as
  Verus-compatible types.
- Classification uses `Option<LineClass>` — `None` means unknown (see
  Decision 7). The `LineProperty` enum does NOT contain an `Unknown` variant.
- Implement `annotation_target()` as a `proof fn` / `spec fn` matching
  Section 2 of the spec. The function takes `Vec<Option<LineClass>>` and
  returns `Option<TargetLine>` where `TargetLine.properties` is
  `Option<LineClass>`.
- Implement `execution_set()` as a `proof fn` / `spec fn` matching Section 3
  of the spec. Unknown (`None`) lines block backward propagation.
- Implement `is_annotation_executed()` as a `proof fn` / `spec fn` matching
  Section 4 of the spec. When the target has `properties: None`, return
  `ExecutionStatus::Unknown`.
- Prove all six properties:
  - Property 1: No false positives (including: no unknown lines on the path)
  - Property 2: No cross-scope leakage
  - Property 3: Conservative fallback (NonLinearControl)
  - Property 4: Monotonicity
  - Property 5: Annotation stacking transitivity
  - Property 6: Unknown safety
- Every `proof fn` that proves a spec property MUST have duvet annotations:
  ```rust
  verus! {

  //= coverage-model-v2-spec.md#property-1-no-false-positives
  //# The implementation MUST prove that if is_annotation_executed
  //# returns Executed, then there exists a directly-hit line in the
  //# same scope with a clear path to the target.
  proof fn property_no_false_positives(/* ... */)
      requires /* ... */
      ensures /* ... */
  {
      // ...
  }

  } // verus!
  ```
- The Verus code should also compile as regular Rust (with the verus macros
  gated behind a feature flag) so the same types and functions are used in
  production.

**Files to create:**

- `duvet/src/query/coverage_model/mod.rs` — module root, re-exports
- `duvet/src/query/coverage_model/types.rs` — LineProperty, LineClass, Scope, etc.
- `duvet/src/query/coverage_model/target_resolution.rs` — Phase 1
- `duvet/src/query/coverage_model/execution_propagation.rs` — Phase 2
- `duvet/src/query/coverage_model/annotation_execution.rs` — Phase 3 composition
- `duvet/src/query/coverage_model/proofs.rs` — Properties 1-6

**Demo:** `cargo verus` (or equivalent) passes on the spec module. Types
compile under normal `cargo build` with verus macros disabled. `duvet report`
shows the spec properties as covered by the proof annotations.

**Depends on:** Task 0 (duvet config must exist for annotations to be tracked).

## Task 2: tree-sitter Integration and Java Classifier

**Objective:** Add tree-sitter as a dependency. Implement the `classify`
function for Java — mapping tree-sitter AST node types to `LineProperty` sets.

**Location:** `duvet/src/query/classify/`

**Guidance:**

- Add `tree-sitter` and `tree-sitter-java` crates to `duvet/Cargo.toml`.
- Define a trait:
  ```rust
  pub trait LineClassifier {
      fn classify(&self, source: &str) -> Vec<Option<LineClass>>;
  }
  ```
  Each element corresponds to a source line (1-indexed). `None` means the
  classifier could not determine the line's properties. `Some(s)` means the
  line has property set `s`. The classifier MUST return `None` for any line
  it cannot confidently classify (see Decision 9).
- Implement `JavaClassifier` using tree-sitter-java.
- Walk the tree-sitter CST. For each node, determine which `LineProperty`
  values apply to the lines it spans. Key mappings:

  | tree-sitter node type | LineProperty |
  |----------------------|--------------|
  | `method_declaration` | Declaration (signature lines), ScopeOpen (line with `{`) |
  | `class_declaration` | Declaration, ScopeOpen |
  | `interface_declaration` | Declaration, ScopeOpen |
  | `enum_declaration` | Declaration, ScopeOpen |
  | `expression_statement` | Statement |
  | `return_statement` | Statement |
  | `throw_statement` | Statement |
  | `local_variable_declaration` (with init) | Statement, Declaration |
  | `local_variable_declaration` (no init) | Declaration |
  | `field_declaration` (with init) | Statement, Declaration |
  | `field_declaration` (no init) | Declaration |
  | `if_statement` | Statement, ScopeOpen |
  | `for_statement` | Statement, ScopeOpen |
  | `while_statement` | Statement, ScopeOpen |
  | `try_statement` | ScopeOpen |
  | `catch_clause` | Declaration, ScopeOpen |
  | `block` end | ScopeClose |
  | `line_comment` | Comment |
  | `block_comment` | Comment |
  | `import_declaration` | Declaration |
  | `package_declaration` | Declaration |
  | `marker_annotation` (`@Override` etc.) | Declaration |
  | `enum_constant` (with args) | Declaration, Statement |
  | `enum_constant` (no args) | Declaration |

- Lines that are blank after trimming → Some({Whitespace}).
- Lines that are duvet annotations (detected by existing comment parsing) →
  Some({Annotation}). The classifier should integrate with duvet's existing
  annotation detection or accept annotation line ranges as input.
- Lines that the tree-sitter walk does not visit and that are not blank or
  annotations → `None` (unknown). This is the conservative default.

**Files to create:**

- `duvet/src/query/classify/mod.rs` — trait definition, language detection
- `duvet/src/query/classify/java.rs` — JavaClassifier

**Tests:**

- Method signature: `public void foo() {` → Some({Declaration, ScopeOpen})
- Interface declaration: `public interface I {` → Some({Declaration, ScopeOpen})
- Abstract method: `void foo();` inside interface → Some({Declaration})
- Enum constant with args: `AES_128(128, 12, 16),` → Some({Declaration, Statement})
- Enum constant without args: `FOO,` → Some({Declaration})
- Variable with init: `int x = 5;` → Some({Statement, Declaration})
- Variable without init: `int x;` → Some({Declaration})
- Closing brace: `}` → Some({ScopeClose})
- Comment: `// hello` → Some({Comment})
- Java annotation: `@Override` → Some({Declaration})
- Import: `import java.util.List;` → Some({Declaration})
- Blank line → Some({Whitespace})
- Multi-line statement: first and continuation lines → Some({Statement})
- Unrecognized construct → None

**Demo:** Unit tests pass. A representative Java source file (e.g.,
`NativeRawAesKeyring.java` from the MPL project) is correctly classified.

**Depends on:** Nothing (can run in parallel with Task 0 and Task 1).

## Task 3: Scope Analysis

**Objective:** Implement scope tree construction from line classifications.

**Location:** `duvet/src/query/coverage_model/scopes.rs`

**Guidance:**

- Implement `build_scope_tree(classifications: &[Option<LineClass>]) -> Vec<Scope>`
  that pairs `ScopeOpen` and `ScopeClose` lines into a balanced scope tree.
- Only `Some` classifications contribute to scope construction. `None` lines
  are ignored — they cannot be `ScopeOpen` or `ScopeClose`.
- Handle nested scopes (method inside class, if inside method, etc.).
- Handle the implicit file-level scope (lines not inside any explicit scope).
- For Python: tree-sitter-python produces `block` nodes with start/end
  positions. The classifier (Task 2, future Python classifier) will assign
  `ScopeOpen`/`ScopeClose` to the first/last lines of each block. Scope
  analysis doesn't need to know about indentation — it just reads the
  properties.
- Error handling: unbalanced scopes (more opens than closes or vice versa)
  should produce a diagnostic, not a panic. Fall back to treating the entire
  file as one scope.

**Files to create:**

- `duvet/src/query/coverage_model/scopes.rs`

**Tests:**

- Simple method in class: class scope contains method scope.
- Nested: class → method → if → for (4 levels).
- Multiple methods in one class: sibling scopes.
- Unbalanced braces: graceful fallback.
- Empty file: single file-level scope.
- File with unknown lines interspersed: scope tree built from known lines only.

**Demo:** Scope tree correctly built for test Java files.

**Depends on:** Task 2 (needs classifications to build scopes from).

## Task 4: Annotation Target Resolution

**Objective:** Implement Phase 1 of the spec — the forward walk from an
annotation to its target.

**Location:** `duvet/src/query/coverage_model/target_resolution.rs`

**Guidance:**

- Implement `annotation_target()` matching the algorithm in Section 2.3 of the
  spec.
- Input: `AnnotationSpan`, `Vec<Option<LineClass>>`, file length.
- Output: `Option<TargetLine>` where `TargetLine.properties` is
  `Option<LineClass>`.
- When the forward walk encounters a `None` line, return
  `Some(TargetLine { line_number, properties: None })` — not `None`. This
  allows Phase 3 to report `Unknown` with the specific line number.
- This function must match the Verus spec from Task 1 exactly. If the Verus
  proof uses a different representation, provide a thin adapter.

**Tests:**

- Annotation before method sig → targets the method sig line with Some(props).
- Annotation before statement → targets the statement with Some(props).
- Annotation before `}` → returns None (dangling).
- Annotation at end of file → returns None.
- Stacked annotations → both target the same line.
- Annotation before `int x;` → targets the declaration with Some(props).
- Annotation before interface → targets the interface declaration with Some(props).
- Annotation with blank lines and comments between it and target → skips them.
- Annotation before unknown line → returns Some(TargetLine { properties: None }).

**Demo:** All placement patterns from `design/duvet-patterns.md` in the MPL
project resolve to the expected target.

**Depends on:** Task 1 (types), Task 3 (scope tree for context, though target
resolution itself doesn't use scopes — it's a pure forward walk).

## Task 5: Execution Propagation

**Objective:** Implement Phase 2 of the spec — backward walk from executed
lines to compute the execution set.

**Location:** `duvet/src/query/coverage_model/execution_propagation.rs`

**Guidance:**

- Implement `execution_set()` matching the algorithm in Section 3.3 of the
  spec.
- Input: `Vec<Option<LineClass>>`, `Vec<Scope>`, `CoverageReport`.
- Output: `Set<u64>` (the execution set).
- Key behaviors to implement:
  - Backward propagation through Whitespace, Comment, Annotation, Declaration.
  - Stop at ScopeClose (wall).
  - Stop at Statement (has its own coverage status).
  - Stop at ScopeOpen (include it, then stop).
  - Stop at unknown (`None`) lines (wall — same as ScopeClose).
  - Skip entire scope if it contains NonLinearControl.
- This function must match the Verus spec from Task 1 exactly.

**Tests:**

- Executed statement propagates backward through declaration and whitespace.
- Propagation stops at `}` (ScopeClose).
- Propagation stops at another statement.
- Propagation stops at ScopeOpen (includes it).
- Propagation stops at unknown (`None`) line.
- Propagation disabled in scope with NonLinearControl.
- Multiple executed lines in same scope: each propagates independently.
- Nested scopes: propagation in inner scope doesn't leak to outer scope.
- Empty scope (no executed lines): execution set is empty for that scope.
- Unknown line between two executed lines: propagation from each stops at the
  unknown line independently.

**Demo:** Execution set correctly computed for the worked examples in Section 6
of the spec, including examples 6.7 and 6.8.

**Depends on:** Task 1 (types + proofs), Task 3 (scope tree).

## Task 6: New `is_annotation_executed()`

**Objective:** Compose target resolution and execution propagation into the
new `is_annotation_executed()`. Replace the current implementation in
`duvet/src/query/checks/coverage.rs`.

**Location:** Modify `duvet/src/query/checks/coverage.rs`

**Guidance:**

- Implement `is_annotation_executed()` matching Section 4.3 of the spec.
- Replace the current `is_annotation_executed()` function (which does a
  forward-only walk over `LineInfo` values).
- Replace the current `LineInfo` enum usage with the new `Option<LineClass>`
  types.
- Update `build_line_map_for_file()` to:
  1. Detect the file's language (from extension).
  2. If a classifier exists for that language, use it to produce
     `Vec<Option<LineClass>>`.
  3. Build the scope tree.
  4. Merge coverage data (existing logic).
  5. Mark annotation lines (existing logic).
  6. Mark whitespace lines (existing logic, now redundant if classifier
     handles it).
- **Backward compatibility:** When no classifier is available for a file's
  language, fall back to the current behavior. All lines start as `None`,
  then coverage data, annotation detection, and whitespace detection
  reclassify the lines they can identify. Lines that remain `None` are
  unknown — identical to the current `LineInfo::Unknown` behavior. The
  forward walk stops at `None` lines and returns `Unknown`, preserving
  the existing semantics exactly.
- Update `build_source_line_map()` to pass language info through.

**Files to modify:**

- `duvet/src/query/checks/coverage.rs` — main changes
- `duvet/src/query/coverage.rs` — update types if needed
- `duvet/src/query/engine.rs` — pass language/classifier info

**Tests:**

- Existing integration tests must continue to pass (no regression).
- The fallback path (no classifier) produces identical results to current
  behavior.

**Demo:** `duvet query -c coverage` works with existing integration tests.
No snapshot changes for existing tests.

**Depends on:** Task 4, Task 5.

## Task 7: Integration Tests

**Objective:** Add integration tests covering the new coverage model behaviors
that were previously impossible.

**Location:** `integration/` directory (following existing pattern, e.g.,
`integration/query-coverage-all-pass.toml`)

**Guidance:**

Add integration test TOML files following the existing pattern. Each test
defines a `.duvet/config.toml`, source files, spec files, and JaCoCo XML
coverage reports inline.

**Tests to add:**

1. **Annotation before method signature** — annotation on line above
   `public void foo() {`, with JaCoCo reporting the first statement inside
   the method as Hit. Expected: annotation is Executed.

2. **Annotation on interface** — annotation above `public interface I {`.
   No coverage data for the interface. Expected: annotation is Structural.

3. **Annotation before `int x;`** — annotation above a variable declaration
   without initializer, with the next statement (assignment) as Hit.
   Expected: annotation is Executed.

4. **Cross-method non-leakage** — annotation in method A, method B is
   executed but method A is not. Expected: annotation is NotExecuted.

5. **Stacked annotations across declaration** — two annotations stacked,
   followed by a method signature, followed by an executed statement.
   Expected: both annotations are Executed.

6. **Comment between annotation and target** — annotation, then a
   non-annotation comment, then an executed statement. Expected: annotation
   is Executed (comment is skipped).

7. **Annotation before closing brace** — annotation immediately before `}`.
   Expected: annotation target is None (dangling).

8. **Unknown line between annotation and target** — annotation, then an
   unclassifiable line, then an executed statement. Expected: annotation is
   Unknown (the unknown line blocks target resolution).

**Demo:** `cargo test` passes including all new integration tests. Snapshots
are committed.

**Depends on:** Task 6.

## Task 8: Additional Language Classifiers

**Objective:** Add tree-sitter grammars and classifiers for other languages
duvet supports.

**Location:** `duvet/src/query/classify/`

**Guidance:**

For each language, add the tree-sitter grammar crate to `Cargo.toml` and
implement the `LineClassifier` trait. The Java classifier from Task 2
establishes the pattern. All classifiers MUST return `None` for lines they
cannot confidently classify.

**Languages (in priority order):**

1. **Rust** — `tree-sitter-rust`. Duvet itself is Rust, and Rust projects
   (s2n-quic, s2n-tls) are major duvet users.
   - Key mappings: `fn` → {Declaration, ScopeOpen}, `struct`/`enum`/`trait` →
     {Declaration, ScopeOpen}, `let` with init → {Statement, Declaration},
     `let` without init → {Declaration}, `impl` → {Declaration, ScopeOpen}.

2. **Python** — `tree-sitter-python`. Indentation-based scoping.
   - Key difference: `def foo():` → {Declaration, ScopeOpen}. The `block`
     node's last line gets {ScopeClose}. No `}` to match.

3. **C** — `tree-sitter-c`. Has `goto`.
   - Key difference: `goto` and labels → {NonLinearControl}. This triggers
     the conservative fallback per scope.

4. **JavaScript/TypeScript** — `tree-sitter-javascript`, `tree-sitter-typescript`.
   - Similar to Java. Arrow functions and class expressions need attention.

5. **Go** — `tree-sitter-go`. Has `goto` (restricted).
   - Same `goto` handling as C.

**Files to create (per language):**

- `duvet/src/query/classify/<language>.rs`

**Tests (per language):**

- Representative source file correctly classified.
- Language-specific constructs handled (Python indentation, C goto, etc.).
- Unrecognized constructs return `None`.

**Demo:** Classification works for each language's representative source files.

**Depends on:** Task 2 (pattern established).

## Summary

| Task | Description | Depends on | Estimated complexity |
|------|-------------|-----------|---------------------|
| 0 | Duvet config for spec traceability | — | Low |
| 1 | Verus proof scaffolding + duvet annotations | 0 | High (proof engineering) |
| 2 | tree-sitter + Java classifier | — | Medium |
| 3 | Scope analysis | 2 | Low-Medium |
| 4 | Annotation target resolution | 1, 3 | Low |
| 5 | Execution propagation | 1, 3 | Medium |
| 6 | Replace is_annotation_executed | 4, 5 | Medium |
| 7 | Integration tests | 6 | Low-Medium |
| 8 | Additional language classifiers | 2 | Medium (per language) |

Tasks 0, 1, and 2 are the critical path. Task 0 is quick. Tasks 1 and 2 can
run in parallel after Task 0 completes.
