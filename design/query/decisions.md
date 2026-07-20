# Duvet Query: Design Decisions

**Date:** 2026-03-19
**Status:** Implemented

This document captures the key design decisions for `duvet query`.
Each decision follows the "cake" format:
what options were considered,
what was chosen,
and why.

---

## Decision 1: Command naming {#decision-1}

**Context:** Duvet needs a command for interactive traceability checking
during development.

### Option A: `duvet audit`

A dedicated audit command that gates on traceability completeness.

- Pro: Clear intent — "audit" implies verification.
- Con: Implies a CI gate.
  Overlaps with `duvet report`,
  which already generates traceability reports.
  Creates confusion about which command to use in CI.

### Option B: `duvet check`

A check command, similar to `cargo check`.

- Pro: Familiar pattern for Rust developers.
- Con: `check` implies a binary pass/fail.
  The command's primary value is the detailed diagnostic output,
  not just the exit code.

### Option C: `duvet query`

Treat the project's annotations,
specifications,
and coverage data as a queryable database.
Ask specific questions about traceability status.

- Pro: Accurate metaphor —
  you're querying a corpus of data.
  Doesn't imply a gate.
  Naturally supports filtering (sections, quotes) as query predicates.
  Leaves room for `duvet report` to be the CI gate.
- Con: Less immediately obvious what it does.

### Decision: Option C (`duvet query`)

The command is a development feedback loop, not a CI gate.
The query metaphor accurately describes what it does:
you have a database of annotations and specifications,
and you ask questions about it.
The CI enforcement role belongs to `duvet report`,
which already has the snapshot mechanism.
Keeping them separate avoids confusion
about which command to use where.

---

## Decision 2: Annotation execution detection {#decision-2}

**Context:** The coverage check needs to determine
if a duvet annotation was "executed" during a test run.
Annotations are comments —
they don't appear in coverage data.

### Option A: Check the annotation line itself

Look for the annotation's line number in coverage data.

- Pro: Simple.
- Con: Doesn't work.
  Comment lines aren't executable
  and never appear in coverage data.

### Option B: Fixed line buffer after annotation

Check a fixed number of lines after the annotation boundary.

- Pro: Simple to implement.
- Con: Fragile.
  Arbitrary buffer might miss executable code
  or include unrelated code.
  Different code patterns need different buffer sizes.

### Option C: Walk forward to first substantive line

Use coverage data to identify executable vs non-executable lines.
Walk forward from the annotation,
skipping whitespace and stacked annotations,
until reaching a line that coverage data has an opinion about.

- Pro: Uses the coverage tool's own understanding of what's executable.
  Handles varying distances between annotation and code.
  Stacking (transitive execution) falls out naturally.
- Con: Can't see through constructs that coverage tools don't report
  (method signatures, declarations).
  This limitation is addressed by the two-phase coverage model
  (Decision 12).

### Decision: Option C (forward walk)

The coverage tool already knows which lines are executable.
Walking forward to the first such line
is the most accurate approach
that doesn't require language-specific knowledge.
The limitation with declarations is real
but addressed separately by the coverage model enhancement.

---

## Decision 3: Test vs implementation classification {#decision-3}

**Context:** The coverage check needs to distinguish
test annotations from implementation annotations
to verify that tests execute their corresponding implementations.

### Option A: Classify by file path

Use path patterns (`src/test/` vs `src/main/`)
to determine if a file is a test file or implementation file.

- Pro: Works for Java's conventional directory structure.
- Con: Breaks for Rust,
  where tests and implementations coexist in the same file.
  Language-specific.

### Option B: Classify by filename

Use filename patterns (`*Test.java`) to identify test files.

- Pro: Simple heuristic.
- Con: Same language-specificity problem as Option A.

### Option C: Use annotation types

Duvet annotations already carry type information
(`type=test`, `type=implementation`, etc.).
Use this directly.

- Pro: Language-agnostic.
  Works regardless of file organization.
  Leverages existing duvet infrastructure.
- Con: None identified.

### Decision: Option C (annotation types)

Annotation types are the correct abstraction.
They work for any language and any project structure.
File-based classification is a leaky heuristic
that breaks for languages
where tests and implementations share files.

---

## Decision 4: Annotation coverage matching {#decision-4}

**Context:** The implementation, test, and duplicates checks all need
to determine whether one annotation's quoted text "covers" another's.

### Option A: Exact quote matching

Require test and implementation annotations to quote identical text.

- Pro: Simple. Unambiguous.
- Con: Too restrictive.
  Tests often validate broader requirements
  than individual implementation pieces.
  An implementation might quote "MUST encrypt data"
  while a test quotes "MUST encrypt data using AES-256" —
  the test covers the implementation but the quotes aren't identical.

### Option B: Substring matching between annotations

Check if one annotation's quote is a substring of another's.

- Pro: Handles partial coverage.
- Con: Doesn't handle the case where multiple implementation annotations
  together cover a test annotation's broader quote.
  Direction-dependent (which is the substring of which?).

### Option C: Specification text as coordinate system

Normalize both quotes and the specification section text.
Find each quote's character range within the section.
Check for range overlap.

- Pro: Handles partial coverage naturally.
  Multiple annotations can together cover a broader quote.
  Deterministic.
  Anchored to the specification — the source of truth.
- Con: Requires the specification text to be available.
  Quotes must appear verbatim (after normalization)
  in the specification.

### Decision: Option C (specification text coordinates)

This is the only approach that handles all the real-world patterns:
partial quotes,
multiple annotations covering a broader requirement,
and bidirectional coverage checking.
Anchoring to the specification text
makes the matching deterministic and auditable —
you can always ask "where in the spec does this quote land?"

The requirement that quotes appear verbatim in the specification
is not a limitation — it's a feature.
If an annotation quotes text that doesn't appear in the spec,
that's a bug in the annotation.

---

## Decision 5: Coverage format abstraction {#decision-5}

**Context:** Different coverage tools produce different formats
with different capabilities.

### Option A: JaCoCo only

Support only JaCoCo XML,
the most common Java coverage format.

- Pro: Simple. Covers the immediate need.
- Con: Blocks Rust (LCOV), Python (coverage.py), and other languages.

### Option B: Single abstraction for all formats

One `CoverageData` type that all parsers produce.

- Pro: Downstream code doesn't care which parser produced the data.
- Con: Loses format-specific capabilities.
  Per-test coverage (Clover)
  can't be distinguished from aggregate coverage (JaCoCo).

### Option C: Tiered abstraction

A `CoverageData` enum with variants
for different granularity levels.
All formats produce at least `Generic` (aggregate) data.
Per-test formats additionally provide test-level granularity.

- Pro: Preserves format capabilities.
  Downstream code can optimize for per-test data when available
  and fall back to aggregate.
- Con: More complex abstraction.

### Decision: Option C (tiered abstraction)

The implementation currently only has the `Generic` variant
(aggregate coverage).
The enum is designed to accommodate per-test variants
when Clover or similar formats are added.
This avoids a breaking change later.

JaCoCo is the only implemented parser today.
LCOV is planned next (unblocks Rust and Python).
Clover would enable per-test granularity.

---

## Decision 6: Multiple coverage report handling {#decision-6}

**Context:** Coverage data may come from multiple test runs
or multiple coverage tools.
How should multiple reports be combined?

### Option A: Require a single merged report

Expect the user to merge reports before running duvet.

- Pro: Simple for duvet.
- Con: Pushes complexity to the user.
  Not all coverage tools support merging.
  Different formats can't be merged.

### Option B: OR semantics across reports

Parse all reports independently.
An annotation is executed if ANY report shows it as executed.

- Pro: Simple,
  correct for the common case
  (different test suites producing separate reports).
  No user-side merging needed.
- Con: Can't distinguish "test A executed this"
  from "test B executed this" with aggregate formats.

### Decision: Option B (OR semantics)

Multiple reports are parsed in parallel and checked independently.
OR semantics are correct for the primary use case:
combining coverage from unit tests,
integration tests,
and other test suites that produce separate reports.
The `--coverage-report` flag accepts multiple paths
with glob expansion.

---

## Decision 7: Executed coverage variant {#decision-7}

**Context:** Running a single test with coverage produces false
positives for every other test annotation that wasn't executed.

### Option A: No special handling

The coverage check reports all non-executed test annotations
as failures.

- Pro: Simple. Consistent.
- Con: Unusable for single-test development.
  If you're working on one test,
  you don't want failures for the 200 other tests you didn't run.

### Option B: Filter by section/quote

Use `--section` or `--quote` to narrow scope
to the test being worked on.

- Pro: Uses existing filtering. No new check type needed.
- Con: Requires knowing which section/quote the test covers.
  Doesn't help when you want to check
  "whatever this test happens to execute."

### Option C: Separate check type that filters to executed tests

A variant of the coverage check
that only evaluates test annotations
the coverage data shows as executed.

- Pro: Automatically focuses on the test(s) that ran.
  No manual filtering needed.
  Surfaces annotation placement errors immediately —
  if you add an annotation that can never be executed,
  you find out during single-test development.
- Con: Another check type to understand.

### Decision: Option C (`executed-coverage`)

The executed coverage check solves two problems:
(1) eliminates noise from unexecuted tests
during single-test development,
and (2) prevents accumulation of annotation placement errors.
The full coverage check catches bad placements too,
but during single-test development
you only see failures for the test you ran.
Executed coverage ensures that the test you're actively working on
has correct annotation placements,
so errors don't pile up silently
and surface all at once when the full suite runs.

---

## Decision 8: Duplicates check strictness {#decision-8}

**Context:** The duplicates check enforces
that no two annotations of the same type
redundantly cover the same specification text.
But some duplication may be intentional.

### Option A: Zero tolerance

Any duplicate is a failure. No exceptions.

- Pro: Simple rule. Forces consolidation.
- Con: Too strict.
  Two tests may legitimately need to verify the same requirement
  from different angles.
  Forcing them to share a single annotation
  may not be possible or desirable.

### Option B: Allow-list mechanism

Provide a way to mark specific duplicates as acceptable.

- Pro: Handles the legitimate duplication case.
- Con: The interface for this isn't obvious.
  Where does the allow-list live?
  How do you reference specific annotation pairs?

### Option C: Ship zero-tolerance now, add allow-list later

Start with the strict rule.
Address legitimate duplicates
when the pattern is better understood.

- Pro: Gets the check shipping.
  The strict rule is correct for the majority of cases.
  Real-world usage will clarify
  what the allow-list interface should look like.
- Con: Some projects may have legitimate duplicates
  that cause immediate failures.

### Decision: Option C (strict now, refine later)

The duplicates check ships with zero tolerance.
The known limitation is documented.
The allow-list mechanism will be designed
once we have more experience
with the patterns of legitimate duplication.

---

## Decision 9: Quote filtering {#decision-9}

**Context:** Section-level filtering (`--section`)
narrows scope to a specification section,
but a section may contain many requirements.
During development, you often work on a subset.

### Option A: Section filtering only

Only support `--section`.
If a section has 10 requirements and you're working on 2,
you see results for all 10.

- Pro: Simple.
  Sections are the natural unit of specification organization.
- Con: Noisy during focused development.
  For automated workflows, not deterministic enough —
  you can't say "check exactly these 2 requirements."

### Option B: Add quote text filtering

Allow filtering by quoted text content.
Case-insensitive substring match on annotation quotes.

- Pro: Granular.
  Deterministic —
  you can target exactly the requirements you're working on.
  Complements section filtering
  (AND semantics when both are specified).
- Con: Requires knowing the quote text.
  Substring matching could match unintended annotations
  if the filter is too broad.

### Decision: Option B (quote filtering)

Quote filtering provides the deterministic feedback loop
needed for focused development and automated workflows.
Combined with section filtering,
it gives precise control over which requirements are checked.
The substring matching is case-insensitive and intentionally simple —
complex query syntax would add cognitive load
without proportional benefit.

---

## Decision 10: Primary/secondary annotation classification {#decision-10}

**Context:** The implementation check needs to distinguish
between real implementations and TODOs.
A requirement covered only by TODOs is not "implemented,"
but it's also not "not implemented" — it's in progress.

### Option A: Binary classification

Annotations either cover a requirement or they don't.
No distinction between implementation types.

- Pro: Simple.
- Con: Loses the TODO signal.
  A requirement with a TODO annotation
  looks the same as one with no annotations at all.

### Option B: Primary/secondary with mixed reporting

The classification engine accepts two sets of candidates.
Primary annotations are the expected coverage
(implementations for the implementation check).
Secondary annotations are the fallback (TODOs).
The result distinguishes:
fully covered by primary,
mixed primary+secondary,
primary-only incomplete,
secondary-only,
and no coverage.

- Pro: Actionable diagnostics.
  "This requirement has an implementation AND a stale TODO"
  is different from "this requirement only has a TODO"
  is different from "this requirement has nothing."
- Con: More complex result model.

### Decision: Option B (primary/secondary)

The mixed category is the key insight.
When a requirement has both an implementation annotation
and a TODO annotation,
the user needs to know:
is the TODO stale
(the implementation is complete, remove the TODO)?
Or is the implementation incomplete
(the TODO marks remaining work)?
Binary classification can't surface this distinction.

The test check doesn't use secondary (passes empty),
and the duplicates check uses annotations as their own primary.
The machinery is general enough
to support all three checks with the same code.

---

## Decision 11: Config-driven CI mode {#decision-11}

**Context:** The original design included `[audit]` configuration
in `.duvet/config.toml` for CI enforcement —
enabled checks, section lists, coverage report paths.

### Option A: Implement config-driven mode in query

Add `[query]` configuration to `.duvet/config.toml`.
Running `duvet query` with no CLI arguments uses the config.

- Pro: CI-ready.
  Teams can enforce traceability in pipelines.
- Con: Duplicates the enforcement role of `duvet report`.
  Two commands that can both gate CI,
  with different configuration mechanisms.

### Option B: Defer to report integration

Keep query as a CLI-only development tool.
Move the enforcement capability into `duvet report`,
which already has the snapshot mechanism and CI integration.

- Pro: Single enforcement point.
  No configuration duplication.
  Report already handles the "is the project in good shape?" question.
- Con: Requires solving the snapshot ratchet problem
  (comparing snapshots across commits to detect regressions).
  This is non-trivial.

### Decision: Option B (defer to report)

Query stays CLI-only.
The enforcement gate belongs in report.
The snapshot ratchet problem is real but solvable —
the key insight is that you need a comparison baseline
(previous commit, previous CI run)
and a monotonicity check
(traceability only improves, never regresses).
This is future work.

---

## Decision 12: Two-phase coverage model as transparent upgrade {#decision-12}

**Context:** The forward-walk execution detection (Decision 2)
can't see through declarations,
method signatures,
and other constructs that coverage tools don't report.
The two-phase coverage model
(see [coverage-model-spec.md](coverage-model-spec.md))
uses tree-sitter AST parsing to understand source structure
and backward propagation to determine
which non-executable lines were reached.

### Option A: Replace the forward walk

Remove the basic forward-walk implementation.
All files use the two-phase model.

- Pro: Single code path.
- Con: Requires tree-sitter classifiers
  for every language duvet supports.
  Languages without classifiers
  would lose coverage checking entirely.

### Option B: Transparent upgrade with fallback

When a tree-sitter classifier exists for the file's language,
use the two-phase model.
Otherwise, use the basic forward-walk.

- Pro: No regression.
  Languages without classifiers keep working exactly as before.
  New classifiers can be added incrementally.
  The upgrade is invisible to users —
  annotations just work better.
- Con: Two code paths to maintain.

### Decision: Option B (transparent upgrade)

The goal is to make annotation placement intuitive
without breaking anything.
The basic forward-walk is the safe baseline.
The two-phase model is an enhancement
that activates when a classifier is available.
This lets us ship classifiers incrementally
(Java first, then Rust, Python, Kotlin)
without blocking the coverage check for any language.

The two-phase model is formally specified
and designed to be verified with Verus proofs.
See [coverage-model-spec.md](coverage-model-spec.md)
for the specification
and [coverage-model-decisions.md](coverage-model-decisions.md)
for the design decisions specific to the coverage model.

### Update: the forward-walk baseline was replaced by a verified degraded path {#decision-12-update}

Option B kept the **unverified** basic forward-walk as the classifier-less
baseline. Review (PR #227) surfaced two problems: the forward-walk was unverified
and untested, and — after `b74ebb0` made `build_execution_data` *refuse* covered
files with no classifier — it became unreachable dead code that still read as a
live second path (Finding #5). We changed direction:

- The unverified forward-walk (`LineMap` / `LineInfo` /
  `executed_status_for_unclassified` and the `update_*_lines` helpers) is
  **removed**.
- Classifier-less covered files are now scored by a **verified degraded path**
  (`duvet_coverage::degraded`, spec [§7](coverage-model-spec.md#degraded-mode)):
  the same forward-nearest target resolution as Phase 1, deciding status by
  reading coverage directly on the resolved line — no propagation, so it is sound
  without a scope tree. Proven properties D1 (direct observation), D2 (target
  bounds), D3 (stacking transitivity), D4 (agreement with the classified model).
- The `b74ebb0` refusal is **reversed**: classifier-less files are scored (lower
  fidelity, forward-nearest governance), not refused.

So Option B's intent — "languages without classifiers keep working, no
regression" — is preserved, but the baseline is now verified rather than an
untested heuristic. The two-code-path con remains (classified vs. degraded), and
both paths are now verified.
