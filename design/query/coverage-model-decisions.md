# Coverage Model: Design Decisions

**Authors:** Seebees
**Date:** 2026-03-16
**Status:** Implemented

## Context

Duvet's `coverage` check verifies that test annotations
actually execute their corresponding implementation annotations
by cross-referencing source annotations with code coverage reports.
The current implementation
(in `duvet/src/query/checks/coverage.rs`,
function `is_annotation_executed`)
walks forward from an annotation
looking for an executed line in the coverage report.
Any line that is not whitespace,
not an annotation,
and not in the coverage report
is classified as `Unknown` and breaks the walk.

This works for languages and coverage tools
where every meaningful source line appears in the coverage report.
It breaks down for Java (and other compiled languages)
where bytecode-based coverage tools like JaCoCo
cannot report on source constructs that produce no bytecode:
method signatures,
interface declarations,
variable declarations without initializers,
comments,
closing braces,
and Java annotations like `@Override`.

The result is that developers must place duvet annotations
in unnatural positions —
inside method bodies after the first executable statement —
rather than in the most obvious location
(before the method signature, on the declaration).
This makes the annotations harder to write,
harder to read,
and more fragile.

We want to fix this so that annotations can go
in the most obvious location
and still be verified by coverage data.

### Why this matters

Duvet is a compliance tool.
Its coverage check is the mechanism by which teams verify
that their tests actually exercise the code they claim to test.
If annotations must go in awkward positions,
developers are more likely to place them incorrectly,
and the resulting coverage data is harder to audit.
The goal is to make the "obvious" annotation placement
also a "correct" one.

### Investigation summary

We investigated:

- **Java code coverage tools**
  (JaCoCo, Cobertura, IntelliJ Coverage, Kover, OpenClover, JCov) —
  all bytecode-based tools share the same blind spots
  because `javac` doesn't emit bytecode for structural declarations.
  OpenClover uses source-level instrumentation
  and is slightly better (catches more expression-level detail)
  but still cannot report method signatures,
  interface declarations,
  or closing braces as "executed"
  because they are not statements.

- **Other languages' coverage tools** —
  the same pattern holds.
  Bytecode/IR tools (Go's `go test -cover`, Rust's `llvm-cov`)
  have similar gaps.
  Only interpreter-level tools
  (Python's `coverage.py`, JavaScript's Istanbul)
  can report every source line,
  because they trace at the interpreter level.

- **The jverify project**
  uses `com.sun.tools.javac.tree`
  (the Java compiler's own AST)
  to understand Java source structure.
  This is the authoritative source of
  "what is a statement vs a declaration" for Java,
  but it's a Java library — not usable from Rust.

- **Language-specific parsers** —
  Java has `com.sun.tools.javac`,
  Go has `go/ast` (stdlib),
  Python has `ast` (stdlib),
  Rust has `syn`.
  Each is authoritative
  but only usable from its own language.

The fundamental insight:
**no coverage tool change can fix this problem**.
The issue is that duvet doesn't understand source code structure.
It treats source files as flat sequences of lines
and relies entirely on the coverage tool
to tell it which lines matter.
We need to give duvet its own understanding of source structure.

## Decision 1: How to classify source lines

When considering how to give duvet source-structure awareness, we considered
four approaches:

### Option A: Regex heuristics

Pattern-match source lines with regular expressions to guess what they are.
For example, `/^\s*(public|private|protected).*\(.*\)\s*\{/` to detect method
signatures in Java.

- Pro: No new dependencies. Simple to implement for a single language.
- Con: Fragile — breaks on multi-line signatures, unusual formatting, edge
  cases. Different regex set needed per language. Not provably correct.
  Maintenance burden grows with each language. We cannot build a formally
  verified system on regex heuristics.

### Option B: Coverage tool method boundary data

JaCoCo already reports method start lines in `<method line="N">`. The duvet
JaCoCo parser (`duvet/src/query/parsers/jacoco.rs`) already extracts this into
`functions: FxHashMap<String, String>`. We could use this to infer which lines
are method preamble.

- Pro: No new dependencies. Uses data already being parsed.
- Con: Only works for JaCoCo. Doesn't help with interfaces, comments, variable
  declarations, or any non-method construct. Coverage-format-specific — would
  need different logic for each coverage tool. Doesn't generalize.

### Option C: Language-specific companion tools

For each language, write a small tool in that language that uses the language's
own compiler AST to classify lines. For Java, this would use
`com.sun.tools.javac.tree` (like jverify does). For Go, `go/ast`. For Python,
the `ast` module. Duvet would shell out to these tools or read their output.

- Pro: Authoritative — uses the actual compiler's understanding of the source.
  Maximally accurate. No edge cases where the parser disagrees with the
  compiler.
- Con: Requires N tools for N languages, each written in a different language.
  Large maintenance surface. Deployment complexity — users need the companion
  tool installed for their language. Duvet becomes a polyglot project.

### Option D: tree-sitter (AST parser library with Rust bindings)

Use tree-sitter, a parser generator framework with grammars for many languages
and native Rust bindings. tree-sitter parses source into a concrete syntax tree
that distinguishes statements from declarations, identifies scope boundaries,
and handles comments.

- Pro: Single dependency covers all languages duvet supports. Runs in-process
  (no shelling out). Rust-native. Widely used (GitHub syntax highlighting,
  Neovim, Helix editor). Grammars maintained by active communities. Accurate
  enough for syntactic classification — we don't need semantic analysis like
  type resolution, just "is this line a statement or a declaration?"
- Con: Third-party parser — could theoretically disagree with the real compiler
  on edge cases (e.g., bleeding-edge language features). New dependency with
  maintenance cost. Grammar quality varies by language.

### Decision: Option D (tree-sitter)

tree-sitter gives us a single, in-process, Rust-native solution
that covers all languages duvet supports today
(Java, Rust, Python, C, JavaScript, Go).
The alternative of companion tools per language (Option C)
is more authoritative
but the operational cost is too high —
duvet would need to ship and maintain tools
in Java, Go, Python, C, and Rust.
The regex approach (Option A) is too fragile
to build a provably correct system on.
The JaCoCo method boundary approach (Option B) is too narrow —
it only helps with one construct (methods)
in one coverage format.

For edge cases where tree-sitter disagrees with the real compiler,
the system falls back to conservative behavior
(the current strict rules),
so disagreements cannot produce false positives —
only false negatives
(failing to recognize a line as non-executable,
which causes the old strict behavior).

## Decision 2: Forward walk, backward walk, or both

When considering how to use line classifications to determine if an annotation
is executed, we considered three approaches:

### Option A: Forward walk only (current approach, enhanced)

Keep the current forward-walk model but teach it to skip newly classified
non-executable lines (method signatures, comments, etc.) — treating them like
whitespace.

- Pro: Minimal change to existing code. Easy to understand.
- Con: Creates cross-method leakage. We identified this specific failure mode
  during design: if method signatures and closing braces are both "skippable,"
  the walk can chain from one method's annotation through `}`, through
  whitespace, through the next method's signature, and into the next method's
  body — producing false positives. Example:

  ```java
  public void foo() {
      //= spec.md#section-1
      //# MUST do X
      doX();                    // executed
  }                             // "skip" (inert)
                                // "skip" (whitespace)
  public void bar() {           // "skip" (inert)
      doY();                    // executed — FALSE POSITIVE for section-1
  ```

  This is a fundamental flaw, not an edge case.

### Option B: Forward walk with scope boundaries

Forward walk, but stop at closing braces (`}`). Skip method signatures and
comments but treat `}` as a wall.

- Pro: Prevents cross-method leakage. Relatively simple.
- Con: The semantics of "what does this annotation target?" are implicit. The
  walk conflates two questions: "what source construct does this annotation
  describe?" and "was that construct executed?" These are subtly different
  questions that benefit from being answered separately.

### Option C: Two-phase model (forward target resolution + backward execution propagation)

Phase 1 (forward): Walk forward from the annotation to determine what source
construct it targets (method signature? variable declaration? statement?). This
is purely structural — no coverage data involved.

Phase 2 (backward): Walk backward from executed lines to determine which
non-executable lines can be considered "executed by association." A method
signature is "executed" if the first statement in its body was executed. A
variable declaration `int x;` is "executed" if the next statement was executed.

Phase 3 (compose): Check if the annotation's target (from Phase 1) is in the
execution set (from Phase 2).

- Pro: Clean separation of concerns. Each phase is independently testable and
  provable. The backward walk naturally respects scope boundaries — you can't
  propagate execution backward through a `}` because that would exit the scope.
  No cross-method leakage by construction. The forward walk answers "what did
  the developer intend to annotate?" and the backward walk answers "was that
  code executed?" — both questions are answered directly.
- Con: More complex than a single walk. Requires scope analysis. Two algorithms
  to implement and prove instead of one.

### Decision: Option C (two-phase model)

The forward-only approaches (A and B) conflate two distinct questions:
"what does this annotation target?"
and "was that target executed?"
Separating them makes each question easier to answer correctly
and easier to prove correct.

The cross-method leakage problem identified with Option A
is a fundamental flaw —
it means the forward-walk-with-skipping approach
cannot be made correct without scope analysis anyway.
Once you need scope analysis,
the two-phase model is the natural architecture.

The backward propagation direction is key:
execution flows backward from an executed statement
to the non-executable lines above it within the same scope.
This is the correct mental model —
"this method was entered, therefore its signature was reached" —
and it naturally stops at scope boundaries without special cases.

## Decision 3: Line classification granularity

### Option A: Single classification per line

Each line gets exactly one `LineClass` value. A line like `public void foo() {`
would be classified as either `Declaration` or `ScopeOpen`, not both.

- Pro: Simpler data model. Each line has one unambiguous classification.
- Con: Loses information. `public void foo() {` is genuinely both a declaration
  and a scope opener. Forcing a choice means downstream logic can't distinguish
  "method signature that opens a scope" from "bare `{`" or from "method
  signature without `{` on the same line." Similarly, `AES_GCM_128(128, 12, 16)`
  is both a declaration and executable code (constructor call).

### Option B: Multiple classifications per line

Each line gets a set of `LineProperty` values. A line can be simultaneously
`Declaration` and `ScopeOpen`, or `Declaration` and `Statement`.

- Pro: Preserves all information from the AST. Downstream logic can make
  nuanced decisions. Handles compound constructs naturally.
- Con: More complex data model. Walk logic must handle sets instead of single
  values. Proof obligations are more complex.

### Decision: Option B (multiple classifications per line)

The AST genuinely assigns multiple properties to single lines. Discarding this
information would force arbitrary choices that lose precision. The additional
complexity in the walk logic is manageable — the rules for each property are
independent.

This decision resolves several open questions uniformly:

- `public void foo() {` → {Declaration, ScopeOpen}
- `AES_GCM_128(128, 12, 16),` → {Declaration, Statement}
- `private static final X = new Y();` → {Declaration, Statement}
- Python's `def foo():` → {Declaration, ScopeOpen}

## Decision 4: Handling non-linear control flow

### Option A: Ignore it

Don't check for `goto`, labels, or other non-linear control flow. Assume
linear execution within scopes.

- Pro: Simpler. Most languages duvet targets don't have `goto`.
- Con: Incorrect for C and Go. Could produce false positives.

### Option B: Detect and bail out per-scope

If a scope contains any `goto` or label, disable backward propagation for that
entire scope. Fall back to the current strict behavior.

- Pro: Conservative — cannot produce false positives. Simple rule. The AST
  parser can detect `goto`/label nodes trivially.
- Con: Overly conservative for scopes where the `goto` doesn't actually affect
  the annotation in question. But this is a safe over-approximation.

### Option C: Analyze control flow graph

Build a CFG and determine whether non-linear control flow actually affects the
specific lines in question.

- Pro: Maximally precise.
- Con: Massive complexity increase. CFG construction is a significant
  undertaking. Not worth it for the rare case of `goto` in C code with duvet
  annotations.

### Decision: Option B (detect and bail out)

Java doesn't have `goto`. Python doesn't have `goto`. Rust doesn't have `goto`.
JavaScript doesn't have `goto`. The only languages where this matters are C and
Go, and in practice, `goto` in annotated code is rare. The conservative
fallback is safe and simple. If a future need arises for more precision,
Option C can be pursued incrementally without changing the model.

## Decision 5: Formal verification approach

### Option A: No formal verification

Test the implementation thoroughly but don't prove correctness.

- Pro: Faster to ship. No Verus dependency.
- Con: The correctness properties (no false positives, no cross-scope leakage)
  are exactly the kind of invariants that are hard to test exhaustively but
  straightforward to prove. A bug here means silently reporting untested code
  as tested — the worst possible failure mode for a compliance tool.

### Option B: Verus proofs in a separate repository

Write the spec and proofs in a separate repo. Manually keep them in sync with
the implementation.

- Pro: Doesn't add Verus as a build dependency for duvet.
- Con: Spec and implementation drift apart over time. No mechanical guarantee
  of correspondence.

### Option C: Verus proofs in the duvet repository

Write the core model as Verus-verified Rust code within the duvet repo. The
verified code is the actual implementation, not a separate spec.

- Pro: Spec and implementation are the same code. Cannot drift. The proofs
  verify the actual logic that runs in production.
- Con: Adds Verus to the build toolchain. Verus supports a subset of Rust, so
  the verified code may need to be structured differently. May need a thin
  adapter layer between the verified core and the rest of duvet.

### Decision: Option C (Verus proofs in the duvet repo)

Duvet is a compliance tool.
A false positive in the coverage check means
untested code is reported as tested —
a silent correctness failure.
This is exactly the scenario
where formal verification pays for itself.

Verus operates on Rust code directly,
so the verified functions can be the actual implementation.
The subset-of-Rust limitation is manageable
because the core logic
(walks over arrays of enum values)
is simple algorithmically —
it's the invariants that are subtle.
A thin adapter layer will bridge
between Verus-compatible types
and duvet's existing types
(`Arc<Annotation>`, async contexts).

## Decision 6: Policy for structural annotations

### Option A: Require `type=implication` for structural targets

If an annotation targets an interface, abstract method, or other purely
declarative construct, require the annotation to use `type=implication`.

- Pro: Explicit. The developer acknowledges the annotation can't be verified
  by execution.
- Con: Prescriptive. Forces a specific annotation type based on code structure
  rather than intent. An annotation on an interface might genuinely be an
  `implementation` annotation — the interface *is* the implementation of the
  requirement.

### Option B: Report structural status in diagnostics, don't enforce policy

When an annotation targets a structural construct, report this in the output
but don't fail the check or require a specific annotation type.

- Pro: Informative without being prescriptive. Lets teams decide their own
  policy. Provides the information needed to make good decisions.
- Con: Doesn't prevent teams from having unverifiable annotations that they
  think are verified.

### Decision: Option B (report, don't enforce)

The coverage check's job is to report facts: "this annotation was executed,"
"this annotation was not executed," "this annotation targets structural code
that cannot be verified by execution." Policy decisions about what to do with
structural annotations belong to the team using duvet, not to duvet itself.

This doesn't preclude adding an opt-in strict mode later. But the default
should be informative, not prescriptive.

## Decision 7: Representing unclassified lines (Unknown)

### Context

The current duvet implementation initializes every line as `Unknown` and then
reclassifies lines as it gathers information from coverage data, annotation
detection, and whitespace detection. Lines that remain `Unknown` after all
passes are lines duvet cannot reason about — they break the forward walk and
produce `AnnotationExecutionStatus::Unknown`.

The spec as originally drafted omitted `Unknown` from `LineProperty`,
implicitly assuming the tree-sitter classifier can always classify every line.
This is not realistic:

- tree-sitter grammars may not cover every construct in every language version.
- When no classifier exists for a language, the fallback path must treat
  unclassified lines the same way the current implementation does.
- Classifier bugs should produce false negatives (conservative), never false
  positives. This requires `Unknown` lines to block propagation.

### Option A: Unknown as a LineProperty variant

Add `Unknown` to the `LineProperty` enum. A line's `LineClass` could be
`{Unknown}` or theoretically `{Unknown, ScopeOpen}`.

- Pro: Uniform — everything is a `LineProperty`.
- Con: `Unknown` combined with other properties is semantically incoherent. If
  we know a line is `ScopeOpen`, it's not unknown. The combination `{Unknown,
  ScopeOpen}` should never occur, but the type system doesn't prevent it. This
  complicates proofs — every property-level lemma must account for the
  possibility of `Unknown` appearing alongside known properties.

### Option B: Unknown as Option\<LineClass\>

The classification function returns `Option<Set<LineProperty>>` (equivalently,
`Option<LineClass>`). `None` means the line could not be classified. `Some(s)`
means the line was classified with property set `s`.

- Pro: Clean separation. `Unknown` is the absence of classification, not a
  classification itself. The type system prevents incoherent combinations like
  `{Unknown, ScopeOpen}`. Proofs only need to reason about `LineProperty`
  values when the classification is `Some` — the `None` case is handled
  uniformly as "block propagation, report Unknown."
- Con: Slightly more verbose in code — every access to line properties must
  handle the `Option` wrapper.

### Decision: Option B (Option\<LineClass\>)

`Unknown` is not a property of a line — it's the absence of classification.
Modeling it as `Option<LineClass>` (where `None` means unknown) is the correct
abstraction. This keeps the `LineProperty` enum clean and makes proofs simpler:
when reasoning about property sets, we never encounter `Unknown` mixed in.

The classification function signature becomes:

```
classify: (language: Language, source: File) → Vec<Option<LineClass>>
```

Where `LineClass = Set<LineProperty>` and `LineProperty` does not include an
`Unknown` variant.

In the algorithms:

- **Phase 1 (target resolution)**: If the forward walk encounters a `None`
  line, it returns `Some(TargetLine { line_number, properties: None })`. This
  allows Phase 3 to report `ExecutionStatus::Unknown` with the specific line
  number, giving developers actionable diagnostics ("annotation targets
  unclassified line N") rather than a generic "dangling annotation."

- **Phase 2 (execution propagation)**: `None` lines block backward
  propagation, same as `ScopeClose` and `Statement`. If we don't know what a
  line is, we cannot safely propagate through it.

- **Phase 3 (annotation execution check)**: If the target's properties are
  `None`, return `ExecutionStatus::Unknown`.

This preserves the current behavior exactly: in the fallback path (no
classifier available), all non-whitespace, non-annotation, non-coverage lines
are `None`, and the walk stops at them — identical to the current `Unknown`
behavior.

## Decision 8: Duvet annotations in Verus proofs (requirements traceability for the model itself)

### Context

The coverage model is formally verified with Verus (Decision 5). The spec
document defines five correctness properties (no false positives, no
cross-scope leakage, conservative fallback, monotonicity, stacking
transitivity) plus a sixth property for Unknown safety. Each property is proven
by a Verus `proof fn`.

Duvet's purpose is requirements traceability — linking specifications to
implementations. The coverage model's own spec and proofs should practice what
duvet preaches: the Verus proof files should carry duvet annotations linking
each proof back to the spec requirement it satisfies.

### Option A: No traceability (just write proofs)

Write the Verus proofs without duvet annotations. Trust that the proof function
names and comments are sufficient to establish correspondence with the spec.

- Pro: Simpler. No overhead.
- Con: The correspondence between spec requirements and proofs is informal.
  Someone reading the proofs must manually find the matching spec section.
  Ironic for a tool whose entire purpose is mechanized traceability.

### Option B: Duvet annotations in Verus proof files

Add duvet `//=` and `//#` annotations to the Verus proof files, linking each
`proof fn` to the spec section it proves. Run `duvet report` to verify
coverage of the spec by the proofs.

- Pro: The coverage model's own development demonstrates duvet's value
  proposition. The spec-to-proof correspondence is mechanically verified.
  Developers can run `duvet report` to see which spec requirements have proofs
  and which don't.
- Con: Requires the spec document to have stable section anchors. Adds a duvet
  configuration step to the proof workflow.

### Decision: Option B (duvet annotations in Verus proofs)

The spec document will use markdown heading anchors (e.g.,
`## Section Name {#anchor-name}`) to provide stable references. The Verus proof
files will carry duvet annotations pointing to these anchors:

```rust
verus! {

//= coverage-model-spec.md#property-1-no-false-positives
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

The spec document will use normative language for the properties themselves
(e.g., "The implementation MUST prove Property 1") while keeping the algorithm
descriptions as-is (they are already precise pseudocode). This gives a
reasonable relation between the design spec and the actual proofs without
requiring a full RFC-style rewrite of every function description.

A new Task 0 in the implementation plan will configure duvet to track the spec
document and verify that the Verus proofs provide coverage.

## Decision 9: Classifier contract for unclassifiable lines

### Context

Decision 7 established that classification returns `Option<LineClass>` where
`None` means unknown. This decision addresses the contract for classifier
implementations: what must a classifier do when it encounters a line it cannot
classify?

### Option A: Silently skip unclassifiable lines

If the classifier can't determine a line's properties, leave it out of the
result or default to some property like `Whitespace`.

- Pro: Simple implementation.
- Con: Silent misclassification. A line incorrectly classified as `Whitespace`
  would be skipped during propagation, potentially allowing false positives.
  This violates the core safety invariant.

### Option B: Return None for unclassifiable lines

The classifier must return `None` for any line it cannot confidently classify.
The `LineClassifier` trait makes this explicit in its return type.

- Pro: Conservative by construction. Unclassifiable lines block propagation
  (Decision 7), so classifier gaps produce false negatives, never false
  positives. The trait signature enforces this — implementors cannot forget to
  handle the unknown case because the return type requires it.
- Con: May produce more `Unknown` results than necessary if a classifier is
  overly cautious. But this is the safe direction.

### Decision: Option B (return None for unclassifiable lines)

The `LineClassifier` trait signature becomes:

```rust
pub trait LineClassifier {
    fn classify(&self, source: &str) -> Vec<Option<LineClass>>;
}
```

Each element in the returned `Vec` corresponds to a source line (1-indexed).
`None` means the classifier could not determine the line's properties. `Some(s)`
means the line has property set `s`.

When no classifier is available for a language, the fallback behavior is: all
lines start as `None`, then coverage data, annotation detection, and whitespace
detection reclassify the lines they can identify — exactly matching the current
implementation's behavior where lines start as `Unknown` and get reclassified
incrementally.

## Decision 10: BTreeMap/BTreeSet for coverage model collections

### Context

The coverage model uses `BTreeMap<u64, CoverageStatus>` for `CoverageReport`
and `BTreeSet<u64>` for the execution set. The alternative is `HashMap`/
`HashSet`, which are also in `std::collections` and satisfy the "no external
dependencies" constraint.

### Option A: HashMap/HashSet

Use hash-based collections for O(1) average-case lookups.

- Pro: Faster for large inputs. Standard choice for most Rust code.
- Con: Non-deterministic iteration order. Verus's subset of Rust may not
  support `HashMap`/`HashSet` (they depend on `RandomState` and the `hash`
  trait machinery). Makes proofs harder — iteration order affects reasoning
  about loops over collections.

### Option B: BTreeMap/BTreeSet

Use tree-based collections for deterministic, sorted iteration.

- Pro: Deterministic iteration order. Verus supports `BTreeMap`/`BTreeSet`
  more naturally because they don't depend on hashing. Sorted order makes
  debugging and test output reproducible. Proofs over sorted iteration are
  simpler.
- Con: O(log n) lookups instead of O(1). For the sizes involved (thousands of
  lines per file, not millions), this is negligible.

### Decision: Option B (BTreeMap/BTreeSet)

The coverage model operates on per-file data (typically hundreds to low
thousands of lines). The O(log n) vs O(1) difference is irrelevant at this
scale. Deterministic iteration is valuable for Verus proofs, reproducible test
output, and debuggability. The BTree collections are the right choice for code
that will be formally verified.

---

## Decision 11: Scope builder — ScopeClose before ScopeOpen on same line {#decision-11}

**Context:** Lines like `} catch (Exception e) {` or `} else {`
have both `ScopeClose` and `ScopeOpen` properties.
The scope builder must process these correctly
to produce a well-formed scope tree.

### Option A: Ignore ScopeClose when ScopeOpen is present

Only process ScopeOpen. The previous scope never closes.

- Pro: Simpler logic.
- Con: Produces unbalanced scope trees.
  The scope tree falls back to a single file-level scope,
  making scope-aware propagation useless.

### Option B: Process ScopeClose before ScopeOpen

When both properties are present on the same line,
close the previous scope first, then open the new one.
`} catch {` means: close the try block, then open the catch block.

- Pro: Produces correct sibling scopes for try/catch/finally,
  if/else, and similar patterns.
  The scope tree accurately reflects the source structure.
- Con: Slightly more complex logic.

### Decision: Option B (ScopeClose before ScopeOpen)

The `} catch {` pattern is fundamental to Java
and similar languages.
Processing ScopeClose before ScopeOpen
produces the correct scope tree:
try and catch blocks are siblings under the method scope.
This is required for scope-aware propagation
to work correctly with try/catch patterns.

---

## Decision 12: Separate `duvet-coverage` crate {#decision-12}

**Context:** The coverage model logic
(types, scope analysis, target resolution, execution propagation,
annotation execution check, correctness proofs)
needs to be formally verified with Verus.

### Option A: Keep in `duvet` crate

Keep the coverage model code in `duvet/src/query/`.

- Pro: No new crate to manage.
- Con: Verus's subset-of-Rust constraints
  (no `Arc`, no async, no IO, no trait objects)
  would leak into the rest of duvet.
  The coverage model types are used by both
  the coverage check and the classifier —
  keeping them in `duvet` creates implicit coupling.

### Option B: Extract to `duvet-coverage` crate

Move the pure-function coverage model
into a dedicated workspace crate.
The crate has no external dependencies
(only `std::collections` and Verus build macros).

- Pro: Verus isolation —
  the subset-of-Rust constraints stay in one crate.
  Single source of truth for coverage model types.
  Clean dependency: `duvet` depends on `duvet-coverage`,
  not the other way around.
  The crate can be verified independently
  (`cargo verus build -p duvet-coverage`).
- Con: Another crate to maintain.
  Thin adapter layer needed between Verus-compatible types
  and duvet's existing types.

### Decision: Option B (separate crate)

Verus isolation is the primary driver.
The coverage model operates on plain data types
(`Vec`, `BTreeMap`, `BTreeSet`, `u64`)
with no IO, no async, no `Arc`.
Keeping it in its own crate means
the Verus constraints don't affect
the rest of duvet's codebase.
The adapter layer in `duvet/src/query/checks/coverage.rs`
bridges between duvet's `Arc<Annotation>` world
and duvet-coverage's plain-data world.

---

## Decision 13: Classifier mutual exclusivity contract {#decision-13}

**Context:** Tree-sitter AST nodes can span multiple lines.
When a multi-line node (e.g., a fluent builder chain
classified as `local_variable_declaration`)
contains annotation or comment lines,
the classifier marks those lines with `Statement`
and `Declaration` properties from the parent node.
This produces classifications like `{Annotation, Statement, Declaration}`
which are semantically incorrect —
a comment line is not executable code.

### Option A: Fix in the algorithm

Change the forward walk and backward propagation
to handle contaminated classifications.
Check `Annotation` before `Statement` in all walks.

- Pro: No classifier change needed.
- Con: Implicit ordering dependency.
  Every algorithm that inspects line properties
  must know to check `Annotation` first.
  Fragile — a future algorithm might not follow this convention.
  The classification is still semantically wrong.

### Option B: Fix in the classifier (post-processing pass)

After AST classification,
strip `Statement`, `Declaration`, `ScopeOpen`, `ScopeClose`,
and `NonLinearControl` from any line
that has `Annotation`, `Comment`, or `Whitespace`.

- Pro: Clean data — algorithms operate on correct classifications.
  No ordering dependencies.
  Language-agnostic rule that all classifiers apply.
  The mutual exclusivity invariant is explicit and documented.
- Con: Extra pass over the classification data.

### Decision: Option B (post-processing pass)

The classification should be correct at the source.
Algorithms should not need to work around
incorrect classifications.
The post-processing pass is simple, language-agnostic,
and makes the mutual exclusivity invariant explicit.
The spec documents this as a classifier contract
(see [spec §1.3](#mutual-exclusivity-contract)).
