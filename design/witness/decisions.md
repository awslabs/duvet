# Duvet Witness: Design Decisions

**Date:** 2026-07-26
**Status:** Draft — under discussion. Nothing here is implemented.

## Context

Duvet already has a code coverage feature.
A specification quote can be annotated twice in source:
once with `type=test` on the code that checks the behavior,
and once with `type=implementation` on the code that provides it.
Duvet then reads coverage reports (JaCoCo XML today)
and confirms that the test annotation's code actually executes
the implementation annotation's code —
catching vacuous pairs, where a test claims to cover an
implementation it never touches.

This feature adds **new kinds of coverage sources: provers.**
A Verus proof does not execute anything,
but a successful verification leaves an elaboration record
(source spans of everything the verifier consulted),
and that record can serve the same role a coverage report serves
for a runtime test.
Verus is first; Lean, Strata, TLAPS, and TLC are intended to follow
without rearchitecting.
Getting one abstraction that carries both runtime reports and
prover records is the design problem this document decides.

## Vocabulary

These terms are used throughout; the decisions are stated in them.

- **T** — a `type=test` annotation:
  a comment in source, resolved (by the existing verified
  target-resolution phase) to the code lines it governs.
- **I** — a `type=implementation` annotation, resolved the same way.
- **Witness (w)** — the record of **one act of checking**:
  one test's execution, or one prover obligation's successful
  verification.
  Concretely: a label, a claim rule, provenance,
  and per-file line sets (what that act touched).
- **executed(X, w)** — annotation X's resolved lines score
  `Executed` against w's line sets.
  This is the existing verified model (Phases 1–3), unchanged.
- **binds(T, w)** — w is *T's own act*.
  How a test annotation claims a witness; see Decision 3.
- **Discharge** — the obligation a test annotation carries is
  *discharged* when one single witness both belongs to the test and
  executed the implementation:

  ```
  discharged(T, I)  ⟺  ∃w : binds(T, w) ∧ executed(I, w)
  ```

  Discharge is duvet's bookkeeping claim and nothing more:
  the annotations bind to a real act of checking that actually
  reached the annotated implementation.
  It is **not** a claim that the test is a good test or the proof a
  meaningful proof (vacuity auditing — `assume`, `external_body`,
  trivially-true assertions — remains out of scope,
  the reviewer's job).

## Relationship to the correlation fix

The current engine computes T's and I's execution status
independently across all reports,
so no single witness is ever required to have seen both —
which contradicts the discharge definition above.
That is being fixed in its own work stream.
The position taken here: the original design intent
(per-witness binding) was right;
the recorded decision discussed an ambiguity that only exists for
aggregate reports, and the implementation generalized that
ambiguity into an overbroad fold.
**The fix's scope therefore includes clarifying the original
intent in `design/query/decisions.md` and spec §5.2**,
not just changing code —
and if the fix is built on this document's machinery,
that clarification work expands accordingly.
This document does not restate that work.

---

## Decision 1: Discharge requires a single common witness {#decision-1}

**Context:** With multiple coverage sources present,
what must be true of the evidence for a pair (T, I)?

### Option A: Independent existentials

T executed somewhere, I executed somewhere.

- Pro: Trivial with aggregate reports; no witness identity needed.
- Con: "Somewhere" may be two unrelated acts.
  A test annotation gets credit for an implementation
  it never exercises — the vacuity the feature exists to catch.

### Option B: Union-of-all-reports

Merge everything, then check both sides in the union.

- Con: Equivalent to Option A. Steel-manned and rejected:
  sharded CI resolves per-file without a union;
  two instruments measuring one run are one witness delivered as
  several files (packaging, not semantics);
  unit-suite-covers-T-plus-integration-suite-covers-I passing is
  itself the vacuity.
  No question duvet asks is answered by the union of all reports.

### Option C: Same-witness discharge

`discharged(T, I) ⟺ ∃w : binds(T, w) ∧ executed(I, w)`

- Pro: The discharge claim means what users think it means.
- Con: Pairs that only ever passed via aggregate unions will fail.

### Decision: Option C

Global properties ("every implementation annotation is executed by
*something*") remain independent existentials over witnesses and
need no correlation machinery; only the pair verdict is same-witness.

---

## Decision 2: The witness is the unit; artifacts are packaging {#decision-2}

**Context:** Today one report file equals one coverage blob.
Provers emit one artifact (a log directory) containing evidence for
many independent obligations;
per-test runtime formats (Clover-style) contain many per-test
sections in one file.

### Option A: One artifact = one coverage blob (status quo)

- Con: Erases witness identity inside per-test formats and prover
  logs; makes Decision 1 unimplementable at useful granularity.

### Option B: Producers emit `Vec<Witness>`

A producer maps its artifact(s) to zero or more witnesses.
JaCoCo/LCOV: one file → one witness.
Per-test formats: one file → one witness per test section.
Provers: one log → one witness per constructed discharge unit.

- Pro: One-to-one and one-to-many are the same interface.
- Pro: The prover-internal format never escapes its producer.

### Decision: Option B

A witness carries: a label,
a claim rule (Decision 3),
provenance (source artifact, production rule, discharge unit,
strength),
and per-file **closed** line sets (Axiom A1, Decision 4).

---

## Decision 3: Binding is per-witness data {#decision-3}

**Context:** A test annotation must find *its* witness.
Runtime reports are amnesiac about which test produced them;
prover witnesses are constructed from the annotation itself
(Decision 6), so their ownership is known at birth.

### Option A: One universal evidence rule (execution only)

Every witness is claimed by whichever test annotations score
Executed in it.

- Con: Proof witnesses overlap in a way runtime witnesses don't
  (lemmas invoke lemmas, so lemma P's lines appear in caller Q's
  closure). Under evidence-only claiming,
  Q's witness could discharge P's pair even though P never reaches
  the implementation — vacuity re-enters through the closure.

### Option B: A witness-kind taxonomy with per-kind modes

- Con: Considered and deleted.
  The taxonomy existed to serve an aggregate-union mode that
  Decision 1 rejected; complexity around a case that doesn't exist.

### Option C: Each witness carries its claim rule as data

- `ByExecution` (runtime): T claims w iff T's resolved lines are
  Executed in w. Sound only under per-witness individuation
  (Axiom A2).
- `ByRootSpan` (proof): T claims w iff T's resolved lines fall
  inside w's root span (its discharge unit's extent).
  Individuation holds by construction.

### Decision: Option C, closed set for now

`ByExecution` claiming implies T executed in w,
so both rules are instances of one predicate and root-claiming is a
refinement, never a loophole.
The witness object is the "by" in "I executed by T";
there is no separate connectedness relation.
A third claim rule is not currently nameable across
JaCoCo/LCOV/Clover/Verus/Lean/Strata/TLAPS/TLC;
the enum starts closed and may grow.
*(Least-ratified decision in this document; challenge welcome.)*

---

## Decision 4: Properties first; the quantifier layer gets verified {#decision-4}

**Context:** The correlation bug lived in unverified glue because
the property it violated was never stated.
The verified model (Phases 1–3) scores one annotation against one
coverage map and is unchanged;
the load-bearing quantifiers above it were informal.

### Decision: The property inventory is the verified boundary

Stated over witnesses only — no formats in the vocabulary:

```
P1  discharged(T, I)  ⟺  ∃w : binds(T, w) ∧ executed(I, w)
P2  test_executed(T)  ⟺  ∃w : binds(T, w)
P3  ever_executed(I)  ⟺  ∃w : executed(I, w)      (global; no correlation)
P4  Monotonicity: adding a witness never un-discharges a pair;
    removing one never discharges a pair.
P5  binds(T, w) under ByExecution ⟹ executed(T, w)
    (tentative — may be difficult to prove as stated;
    if it resists, it may be restated or demoted to a tested
    property without weakening P1–P4)
```

These become a verified "Phase 4" in `duvet-coverage`
(the quantifier layer over the existing verified cells).
Direction: build it verified from the start rather than
verifying after the fact.
The engine implementation is scaffolding and may be rewritten
freely; the invariant is that the properties stay proven,
not that the pipeline stays the same.

Named axioms (trusted base, per producer family):

```
A1  Closedness: every delivered coverage map is closed under the
    producer's reachability (CPU execution / obligation-graph
    closure).
A2  Individuation: each witness is the record of ONE act of
    checking (one test run, one obligation discharge).
```

Asymmetry, recorded deliberately:
runtime A1 and A2 are axiomatic
(A1: instrumentation fidelity; A2: the user's operational
discipline — the artifact does not record how it was produced).
Prover A1 splits into
(a) verifier faithfulness — irreducibly axiomatic — and
(b) closure computation — our pure code, provable,
a verification candidate in `duvet-coverage`.
Prover A2 holds by construction.
Duvet does not attempt to detect runtime A2 violations
(considered and set aside; see Decision 11).

These properties and axioms must be consolidated into the
specification proper (not live only in this decision record),
and dogfooded: duvet's own spec annotations already sit on proof
elements inside `duvet-coverage`,
so the feature's specification is annotatable by the feature.

---

## Decision 5: Proof witnesses are constructed, two-pass {#decision-5}

**Context:** A prover log is one artifact holding evidence for many
obligations. Which witnesses should exist,
and how are they produced?

### Option A: Eagerly close every obligation in the log

- Con: Converges on "everything the verifier compiled";
  wastes work on unannotated obligations;
  produces a pile of unclaimed witnesses.

### Option B: Annotation-driven construction, two passes

Pass 1 (find live annotations):
project the parsed log to an **aggregate executability map**
(every elaborated line) and run the existing
resolve/classify/score machinery against it to determine which
witnessable annotations (Decision 6) are live.
The verified degraded classifier path
(forward-nearest governance, no tree-sitter required)
suffices initially;
a language-specific classifier arrives when finer granularity
demands it.
Pass 2 (construct witnesses):
for each live annotation, re-interrogate the parsed **structure** —
discharge unit `du(T)`, then its closure —
producing that witness's tailored map.
If no discharge unit contains T's resolved target,
construction yields nothing and `test_executed(T)` is false through
the ordinary empty-`witnesses_for` path.

### Decision: Option B, with two hard constraints

- **The parser emits structure, not just a flat map.**
  Pass 1 and pass 2 are different interrogations of the same
  artifact: the aggregate map is a projection that forgets
  obligation boundaries; the closure is a traversal that needs them.
  Parse once into spans + obligation graph; derive both from it.
- **The aggregate elaboration map is never a witness.**
  It is many obligations wearing one map
  (an A2 violation by construction)
  and exists only as pass-1 scaffolding inside the producer.

Witness granularity is not a producer choice:
it is the image of the annotation's resolved position under the
prover's discharge-unit map (`du`),
at the finest granularity the producer declares it supports.

---

## Decision 6: Witnesses are constructed from test annotations only (for now) {#decision-6}

**Context:** Prover witnesses are constructed *from annotations*
(Decision 5), so something must decide which annotation kinds are
witnessable. More annotation kinds may exist in the future.

### Option A: Test annotations only

- Pro: Exact parity with the runtime feature —
  the test/implementation pair is the unit of correlation there too.
- Pro: Smallest surface; defers no currently-needed capability.

### Option B: Also implication annotations, self-discharging

In the proof world an implication annotation could discharge
*itself*: the verifier proving the annotated artifact makes it
simultaneously the implementation and its correctness evidence,
with "elaborated under a successful verification" as the useful
quantity and no paired test annotation at all.

- Pro: Genuinely attractive for proof-heavy codebases.
- Con: New annotation semantics with no runtime analog;
  unclear whether it is a new claim rule, a new annotation type,
  or just P3 over proof witnesses.
  Nobody can state its discharge obligation precisely yet.

### Decision: Option A now; Option B deferred to a separate feature

Discussed and deliberately deferred, not rejected:
self-discharging implications are a future feature with their own
decision document, not a rider on this one.

---

## Decision 7: Strength semantics stay at execution parity {#decision-7}

**Context:** A runtime witness proves the test *executed* the
implementation's lines — not that the test would fail if those
lines were wrong.
What is the proof-side analog, and how strong should it be?

Running example, used throughout this decision.
Consider a function with two independent `ensures` clauses whose
body is two sequential loops:
loop 1 establishes ensures #1 via its loop invariant,
and loop 2 establishes ensures #2.
Loop 2's entry state is justified by an invariant that loop 1
established.
The test annotation is placed on ensures #2;
the implementation annotation is placed on a line inside loop 1's
body.
The desired verdict is FAIL:
ensures #2 is not discharged by loop 1's work,
so these annotations are mispaired.

### Option A: Consulted (use) semantics — parity with execution

The witness's line sets are what the obligation's elaboration
consulted.
Exactly as strong as runtime coverage, no stronger.

- Pro: Symmetric weakness; honest; available today from the
  elaboration record.
- Con: Cannot fail the running example:
  the whole obligation's elaboration consults the entire body,
  loop 1 included, so the mispaired annotations are credited.

### Option B: Needed (load-bearing) semantics now

Use unsat cores to include only what the solver required.

- Pro: Would fail the running example — the verdict the user
  actually wants there.
- Con: Feasibility unproven; cores are not guaranteed minimal
  (degradation direction: over-crediting, back toward Option A);
  cost unknown; and the runtime side has no analog
  (its equivalent is mutation testing, also not built).

### Decision: Option A, with room reserved for strengthening

Same semantics as execution, now.
The witness's `strength` provenance field exists from day one so
that stronger witnesses (needed-semantics, clause granularity)
can be added later without changing the model —
if a way is found, that is wonderful; it is not assumed.
The exact contents of a constructed witness are deferred to the
closure definition (Open Question 1),
with the intent stated here:
the closure walks the prover's dependency graph downward from the
chosen discharge unit and includes **exactly the reachable set** —
no more, no less.
When loop 2's proof needs an invariant established by loop 1,
the executed lines required for that invariant to hold ARE
slurped in — which may or may not include loop 1's annotated
lines: an annotation on an irrelevant side effect in loop 1's body
is not reachable and never enters;
one on the lines that support the needed invariant is, and does.
How precise this can be depends on the granularity of the prover's
graph — if loop 1 carries five invariants and loop 2 needs one,
whether only that one and its dependencies enter the closure
depends on whether the five are five nodes or one —
and that granularity is unexplored, for Verus included.

---

## Decision 8: A test annotation with no witness anywhere is a failure {#decision-8}

**Context:** With multiple producers configured,
a test annotation might get its witness from any of them —
a proof annotation might be discharged only by the runtime world,
or vice versa.

### Decision: Zero witnesses across ALL configured producers is a reported failure, never silence

`test_executed(T)` false — no producer, runtime or prover,
yielded any witness binding T — must surface as a serviceable
finding (an annotation of unknown provenance),
because an annotation for which no witness can be produced is
exactly the unsoundness this feature exists to prevent.
Consequence: running a subset of producers can legitimately fail
what the full set passes (proof-only run flags a test annotation
whose witness is runtime-only), and that behavior is correct.
Consequence for testing: the feature needs a mixed-provenance test
matrix — annotations proved-but-never-executed,
executed-but-never-proved, both, and neither —
run through the same engine.

---

## Decision 9: Discharge-unit mapping is per-prover, inside each producer's parser {#decision-9}

**Context:** Every prover producer must map an annotation's
resolved position to the discharge unit that certifies it
(the `du` map: whole proof fn / lemma; ensures of an exec fn;
single ensures clause; single conjunct) —
the proof-world analog of the test framework's
"machinery for identifying a test," with no runtime precedent.
Should there be a shared `du` framework across provers?

### Option A: Design a shared discharge-unit framework now

- Con: Zero prover producers exist yet.
  Any shared abstraction would be speculation about artifacts we
  have not parsed — the classic premature framework.

### Option B: Each prover's parser owns its `du` map

- Pro: The discharge unit is discovered during parsing anyway;
  the mapping is inseparable from the artifact's structure.
- Con: Possibly duplicated machinery later.

### Decision: Option B; commonality assessed deliberately later

Expect little sharing.
Revisit only after a second prover producer exists and the
duplication, if any, is visible rather than imagined.

---

## Decision 10: Coverage sources are declared as (producer, artifacts) pairs {#decision-10}

**Context:** Today coverage enters through two CLI flags:
`--coverage-report` (artifact paths, globs, repeatable) and
`--coverage-format` (one format applied to every path in the
invocation).
One global format cannot express two producers in one run,
and this feature requires at least runtime + prover together
(Decision 8's mixed-provenance behavior depends on it).

### Option A: Keep one global format per invocation

- Con: Cannot mix producers;
  Decision 8 becomes untestable and unusable.

### Option B: Each declared source pairs a producer with its artifacts

A coverage source is (producer, artifact path/glob),
declared repeatably;
N producers is N declarations, with no upper bound.
Today's flags become the one-source degenerate case.

### Decision: Option B

The declaration surface (CLI flags vs. `.duvet/config.toml`)
follows existing tool conventions and is settled at
implementation time;
the shape — per-source producer + artifacts, any number of
sources — is the decision.
Grouping several files into one witness
(two instruments measuring one run) is a future affordance of the
same declaration shape, not built now.

---

## Decision 11: No detection of aggregate runtime reports {#decision-11}

**Context:** A whole-suite runtime report (many tests, one file)
satisfies `ByExecution` claiming for every test annotation in it,
weakening every discharge that flows through it (Axiom A2).
Should duvet try to detect and report this?

### Option A: Heuristic detection

One witness claimed by many test annotations suggests an
aggregate report; surface it in output.

- Pro: Turns a silent weakening into a visible one.
- Con: It is a heuristic against unknown intent.
  Duvet cannot know how the customer produced the report or what
  they mean by it — a widely-claimed witness may be exactly what
  they intend.
  Obvious at thirty tests in one file; unclear everywhere else.

### Option B: No detection

A2 remains the user's operational responsibility,
as recorded in Decision 4.

### Decision: Option B, deliberately

Considered and set aside.
If wanted later, it is a separate feature with its own decisions,
not a rider on this one.

---

## Open questions (not decided) {#open-questions}

1. **The exact closure definition.**
   A constructed witness's contents are the downward reachable set
   of the prover's dependency graph from the discharge unit —
   to be stated exactly, per prover.
   Precision is bounded by the graph's granularity
   (five invariants on one loop: five nodes or one?),
   which is unexplored for every prover, Verus included.
   Resolution path: not further discussion —
   build the first Verus producer prototype and inspect what the
   artifact actually supports.

## Work items (not questions — known work) {#work-items}

- Enumerate the Verus `du` map: annotation position → discharge
  unit, case by case (whole proof fn / lemma; ensures of an exec
  fn; single ensures clause; single conjunct) — inside the Verus
  producer, per Decision 9.
- Provenance field shape and verdict output format
  (which witness discharged each pair, at what strength).
- Per-witness scoring cost: classification and path-matching need a
  per-file cache above the witness loop.
- The mixed-provenance test matrix (Decision 8).
- Specification: drafted as `design/witness/spec.md`
  (definitions, Properties W1–W6 with human-language MUSTs paired
  with formal statements, producer obligations, prover-producer
  requirements grounded in the SST POC).
  Remaining: duvet annotations linking `duvet-coverage` proof
  elements to the spec's anchors (dogfooding).
- Amending `design/query/decisions.md` and spec §5.2 to state the
  original per-witness intent (scoped to the correlation-fix work
  stream).
