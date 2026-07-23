# Duvet Query: Design

**Date:** 2026-03-19
**Status:** Implemented

## 1. Overview

Duvet maintains a bidirectional link between specifications and
implementations through annotations in source code.
The `duvet report` command generates a static view of this traceability data.
But during development,
engineers need a fast, targeted feedback loop:
"have I annotated this requirement?
does my test cover it?
is the annotation actually executed?"

`duvet query` treats the project's annotations,
specifications,
and coverage data as a queryable database.
It answers specific questions about traceability status
during the development cycle,
with filtering to focus on exactly the work in progress.

### 1.1 Why "query"

The command is a development tool, not a gate.
It answers questions about the current state of traceability —
what's implemented,
what's tested,
what's executed,
what's duplicated.
The metaphor is a database query:
you have a corpus of annotations and specifications,
and you ask questions about it.

The enforcement gate will eventually live in `duvet report`,
which already has the snapshot mechanism for CI.
Query is the interactive feedback loop;
report is the CI gate.
See [Future: Report Integration](#future-report-integration)
for the path forward.

## 2. Check Types {#check-types}

Query supports five check types,
each answering a different question about traceability.
Checks are selected with `--check` (short: `-c`)
and can be combined:

```bash
duvet query --check implementation --check test
duvet query -c implementation,test,duplicates
```

### 2.1 Implementation (`--check implementation`) {#check-implementation}

**Question:** Do all specification requirements have corresponding
implementation annotations in source code?

Gathers all specification-level annotations (type `spec`) that are in scope,
then checks whether each requirement's quoted text is fully covered
by implementation annotations
(types `citation`, `implication`, `exception`).
TODO annotations are tracked separately as a secondary category —
a requirement covered only by TODOs is not considered implemented.

**Pass condition:** Every in-scope specification requirement is fully
covered by implementation annotations
(citation, implication, or exception).
No TODOs, no gaps.

**Result categories:**
- Fully implemented —
  all quoted text covered by implementation annotations
- Mixed —
  has both implementation and TODO annotations
  (stale TODO? incomplete implementation?)
- Incomplete —
  partially covered by implementations
- TODO only —
  only covered by TODO annotations
- Not implemented —
  no annotations reference this requirement

### 2.2 Test (`--check test`) {#check-test}

**Question:** Do all implementation annotations have corresponding test
annotations?

Gathers all `citation` annotations in scope
(the implementation annotations that need testing),
then checks whether each is fully covered by `test` annotations.
Implication and exception annotations do not require tests.
Test annotations without a matching implementation are not failures.

**Pass condition:** Every in-scope implementation annotation is fully covered
by test annotations.

**Result categories:**
- Fully tested —
  all quoted text covered by test annotations
- Incomplete —
  partially covered
- Not tested —
  no test annotations reference this implementation

### 2.3 Duplicates (`--check duplicates`) {#check-duplicates}

**Question:** Are there annotations of the same type that redundantly
cover the same specification text?

For each annotation type
(spec, citation, test, exception, todo, implication),
checks whether any annotation's quoted text is fully covered
by another annotation of the same type
targeting the same specification section.

**Motivation:** Annotations accumulate.
When engineers add annotations to satisfy implementation or test checks,
they tend to add new ones
without removing old ones in suboptimal locations.
This increases cognitive load —
a reader encounters the same requirement annotated in multiple places
and must determine which is authoritative.
The duplicates check enforces consolidation:
find the one best place for each annotation.

**Pass condition:** No annotation of any type is fully covered by
another annotation of the same type.

**Known limitation:** Zero duplicates is sometimes too strict.
There are legitimate cases where the same requirement text
appears in two test annotations —
for example,
two tests that both need to verify the same requirement
from different angles.
There is currently no mechanism to mark specific duplicates as acceptable.
This is a known gap that will be addressed in a future iteration.
See [Decision 8](decisions.md#decision-8) for discussion.

**Result categories:**
- Duplicates —
  annotation A fully covers annotation B (or vice versa), same type
- Some overlap —
  partial overlap between annotations of the same type
- Unique —
  no overlap with any other annotation of the same type

### 2.4 Coverage (`--check coverage`) {#check-coverage}

**Question:** Are test annotations actually executed,
and do they execute their corresponding implementation annotations?

This is the most complex check.
It composes three operations:

1. **Correlation:** Find which test annotations match which
   implementation annotations
   (same matching logic as the test check).
2. **Test execution:** For each test annotation,
   determine whether it was executed according to coverage data.
3. **Implementation execution:** For each executed test annotation,
   determine whether its matching implementation annotations
   were also executed.

A test annotation "passes" the coverage check when:
(a) it is executed,
(b) it has matching implementation annotations,
and (c) all matching implementation annotations are also executed.

**Pass condition:** All correlated test annotations pass.

**Implicit preconditions:** The coverage check assumes that annotations
exist and correlate.
If a test annotation has no matching implementation annotation,
that's a failure of the test check, not the coverage check.
However,
the coverage check also surfaces this case:
tests with no implementation annotations
to verify execution against
are collected into a dedicated
"Tests with no implementation" bucket,
reported with its own count,
and any non-empty bucket fails the run.
This is deliberately distinct from the failed-correlations count —
a failed correlation means an implementation exists but did not execute;
this bucket means there is nothing to correlate against.

**Structural annotations:**
When the two-phase coverage model determines
that an annotation targets a purely structural construct
(e.g., an interface with no executable code),
it returns `Structural` status.
The coverage check treats `Structural` as `NotExecuted` —
this is intentional.
If a test annotation targets structural code,
the test cannot verify execution of that code,
and the coverage check reports this as a failure.
Teams that annotate structural constructs
should use `type=implication` rather than `type=test`
for annotations that cannot be verified by execution.
See [Coverage Model Decision 6](coverage-model-decisions.md)
for the rationale.

### 2.5 Executed Coverage (`--check executed-coverage`) {#check-executed-coverage}

**Question:** Same as coverage,
but skipping test annotations that were not executed.

When running a single test with coverage,
most test annotations in the project will not be executed —
they belong to other tests.
The standard coverage check would report all of these as failures,
which is noise.
Executed coverage filters out the test annotations
that the coverage data shows as not executed,
giving a clean signal for the specific test being worked on.

This also prevents a subtle accumulation problem.
During single-test development,
you might add annotations that can never be executed
(e.g., placed after an unknown line that breaks the execution walk).
The full coverage check would catch these too,
but if you're only running one test at a time,
you won't see the failures for annotations belonging to other tests.
By the time you run the full suite,
you may have accumulated many such errors.
Executed coverage catches them immediately
for the test you're actively working on.

**Pass condition:** All executed test annotations have fully executed
implementation correlations.

## 3. Requirement Scoping {#requirement-scoping}

All checks support filtering to focus on specific requirements.
Both filters below are **cuts on the words of the specification**:
they select a slice of the spec, and that slice determines which
requirements are in scope to report on. They are applied to the
requirement side of each check only — never to the covering annotations.

A requirement is covered when its covering annotations, taken together,
tile its full quoted text (§4). Because the filters never remove a coverer
from that pool, **a filter can only change which requirements you look at —
it can never turn a covered requirement into a miss, nor a miss into a
pass.** Covering annotations that point at an out-of-scope slice of the
spec simply never pair with an in-scope requirement and fall away on their
own (they share a requirement's exact target or they are ignored).

Two orthogonal filters are available:

### 3.1 Section targeting (`--section`, `-s`)

Selects the spec slice by specification target (the `path#section` a
requirement lives under).
Supports both full section references and path-only references:

```bash
# Specific section
duvet query -c implementation -s "spec.md#section-2.1"

# All sections in a spec
duvet query -c test -s "spec.md"

# Multiple sections
duvet query -c implementation -s "spec.md#section-1,spec.md#section-2"
```

### 3.2 Quote filtering (`--quote`, `-q`)

Selects the spec slice by quoted text content:
a requirement is in scope when its quote contains the filter string.
Case-insensitive substring match.
Multiple `-q` flags combine with OR semantics:

```bash
# Only annotations quoting "MUST encrypt"
duvet query -c implementation -q "MUST encrypt"

# Multiple quote filters (OR)
duvet query -c test -q "MUST encrypt" -q "MUST decrypt"
```

**Motivation:** Section-level filtering isn't granular enough
when a section contains many requirements
and you're working on a subset.
Quote filtering gives a deterministic feedback loop:
"check exactly these requirements, nothing else."
This is particularly valuable for automated workflows
where you need to verify specific requirements
without noise from unrelated ones in the same section.

**Note:** Quote values must not be split on commas,
since requirement text frequently contains commas.
Each quote filter is a separate `-q` argument.

### 3.3 Combined filtering

When both `--section` and `--quote` are specified,
both filters must match (AND semantics):

```bash
# Annotations in section-2.1 that quote "MUST encrypt"
duvet query -c implementation -s "spec.md#section-2.1" -q "MUST encrypt"
```

### 3.4 Global scope

When neither filter is specified, all annotations are in scope.

## 4. Annotation Coverage Classification {#annotation-coverage}

The implementation, test, and duplicates checks all share a common
classification engine that determines how well a set of annotations
covers a target annotation's quoted text.

### 4.1 Quote matching via specification text coordinates

Matching is anchored to the specification text,
not performed directly between annotation quotes.
The algorithm:

1. Load the specification section
   that the target annotation references.
2. Normalize whitespace in both the section text
   and all annotation quotes.
3. Find the target annotation's quote within the section text —
   this gives a character range (start, end) in the section.
4. For each candidate covering annotation (same specification target),
   find its quote within the section text —
   another character range.
5. If the candidate's range overlaps with the target's range,
   mark those characters as covered.
6. The target is "fully covered"
   when every character in its range is covered by at least one candidate.

This approach is deterministic
and handles partial quotes naturally:
if an implementation quotes "MUST foo, MUST bar"
and a test quotes only "MUST foo",
the test covers the first part but not the second.

### 4.2 Primary and secondary classification

The classification engine accepts two sets of candidate annotations:
primary and secondary.
This enables nuanced reporting:

- **Implementation check:**
  Primary = implementation annotations (citation, implication, exception).
  Secondary = TODO annotations.
  A requirement covered only by TODOs is reported differently
  from one with real implementations.
- **Test check:**
  Primary = test annotations.
  Secondary = empty (not used).
- **Duplicates check:**
  Primary = annotations of the same type
  (the annotations are compared against themselves).
  Secondary = empty.

The result categories are:
- **Complete** — fully covered by primary annotations
- **Mixed** — has both primary and secondary coverage
- **Incomplete** — partially covered by primary only
- **Secondary only** — covered only by secondary annotations
- **No coverage** — no annotations cover this target

## 5. Annotation Execution {#annotation-execution}

The coverage and executed-coverage checks need to determine
whether an annotation was "executed" —
whether the code it targets was reached during a test run.

### 5.1 Execution model

An annotation is considered executed
if the first substantive line of code after the annotation
was executed according to coverage data.
The walk forward from the annotation skips whitespace
and stacked annotations
(execution is transitive through stacked annotations).

Two implementations exist, both in the verified `duvet-coverage` crate:

**Degraded model:**
The classifier-less execution detection.
Resolves the annotation to its target line
(the next non-annotation, non-whitespace line below it)
and reads execution status directly from the coverage report
at that line — no propagation.
Works for any language
but cannot see through declarations,
method signatures,
or other constructs that coverage tools don't report.
Its properties (D1–D4) are proven with Verus
alongside the two-phase model's.

**Two-phase coverage model (enhanced):**
Uses tree-sitter AST parsing to classify source lines,
then applies a two-phase algorithm:
(1) forward walk to find the annotation's target construct,
(2) backward propagation from executed lines
to determine which non-executable lines
(declarations, signatures) were reached.
This is formally specified in
[coverage-model-spec.md](coverage-model-spec.md)
and verified with Verus proofs.
Currently available for Java;
other languages use the degraded model.

The enhanced model is a transparent upgrade —
when a tree-sitter classifier exists for the file's language,
it is used automatically.
When no classifier exists,
the degraded model decides status
from direct coverage observation alone.
The goal is to make annotation placement intuitive:
put the annotation where it reads best
(before the method signature),
and the coverage model figures out that the method was executed.

### 5.2 Multiple coverage reports

The `--coverage-report` flag accepts multiple paths
(with glob expansion).
When multiple reports are provided,
they are parsed in parallel and checked independently.
An annotation is considered executed
if ANY report shows it as executed
(OR semantics across reports).

This supports workflows where coverage data comes from
multiple test runs or multiple coverage tools.

## 6. CLI Interface {#cli-interface}

```
duvet query [OPTIONS]

Options:
  -c, --check <CHECK>              Check types (comma-separated)
                                   [values: implementation, test, duplicates,
                                    coverage, executed-coverage]
  -s, --section <SECTION>          Section filter (repeatable)
  -q, --quote <QUOTE>              Quote text filter (repeatable)
  -r, --coverage-report <PATH>     Coverage report path(s), supports globs
  -f, --coverage-format <FORMAT>   Coverage format [values: jacoco-xml]
  -v, --verbose                    Enable verbose output
```

**Exit codes:**
- `0` — all checks passed
- `1` — one or more checks failed

**Examples:**
```bash
# Check implementation annotations for a specific section
duvet query -c implementation -s "spec.md#encryption"

# Check everything for a specific requirement
duvet query -c implementation,test,duplicates -q "MUST encrypt"

# Run coverage check with JaCoCo data
duvet query -c coverage -r "target/coverage/*.xml" -f jacoco-xml

# Quick check: did my single test execute its annotations?
duvet query -c executed-coverage \
  -r "target/coverage/single-test.xml" -f jacoco-xml
```

## 7. Future Work {#future-work}

### 7.1 Report integration {#future-report-integration}

The query checks will eventually be available in `duvet report`
for CI enforcement.
This requires solving the snapshot ratchet problem:
proving that changes only improve traceability, never regress it.

The ratchet needs a comparison baseline —
a previous snapshot to diff against.
The mechanism for identifying that baseline
(a specific commit, the previous CI run, uncommitted vs committed changes)
is an open design question.
One approach is to compare the working tree's snapshot
against the committed snapshot,
treating any regression
(requirement that had an implementation but lost it,
test that existed but was removed)
as a failure.
This has been prototyped in other projects
but not yet integrated into duvet.

### 7.2 Additional coverage formats

LCOV support will unblock Rust and Python coverage workflows.
Clover support would enable per-test coverage granularity,
allowing the coverage check to prove
"test X specifically validates implementation Y"
rather than "some test validates implementation Y."

### 7.3 Additional language classifiers

The two-phase coverage model currently has a tree-sitter classifier
for Java.
Classifiers for Rust, Python, and Kotlin are planned.
Each classifier enables the enhanced coverage model
for that language's files;
files without a classifier use the verified degraded model.

### 7.4 Acceptable duplicates

A mechanism for marking specific duplicates as intentional,
so the duplicates check can distinguish
"this is redundant, consolidate it"
from "this is intentionally duplicated across two tests."
The interface for this is not yet designed.
