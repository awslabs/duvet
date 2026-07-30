# Duvet Witness: Design Decisions

**Date:** 2026-07-26 (amendments through 2026-07-29)
**Status:** Decisions 1–20 ratified. The feature is implemented
against Decisions 1–15; the rooting expansion (Decisions 17–20)
is in progress, and Decision 16's example project ships with the
LCOV follow-up PR (see work items).

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
  *discharged* when the test binds at least one witness and
  **every** witness it binds executed the implementation
  (each witness individually must see both sides;
  quantifier set by Decision 14):

  ```
  witnesses_for(T)  =  { w : binds(T, w) }

  discharged(T, I)  ⟺  witnesses_for(T) ≠ ∅
                        ∧  ∀w ∈ witnesses_for(T) : executed(I, w)
  ```

  Discharge is duvet's bookkeeping claim and nothing more:
  the annotations bind to a real act of checking that actually
  reached the annotated implementation.
  It is **not** a claim that the test is a good test or the proof a
  meaningful proof (vacuity auditing — `assume`, `external_body`,
  trivially-true assertions — remains out of scope,
  the reviewer's job).

## Relationship to the correlation fix

Before this feature, the engine computed T's and I's execution
status independently across all reports,
so no single witness was ever required to have seen both —
contradicting the discharge definition above.
This document's machinery is that fix for the pair verdict:
the engine now computes discharge through the verified witness
layer (spec Property W1), and the old multi-report OR-fold
survives only as the global, non-correlating Property W3
(spec §6).
The position taken here: the original design intent
(per-witness binding) was right;
the recorded decision discussed an ambiguity that only exists for
aggregate reports, and the implementation generalized that
ambiguity into an overbroad fold.
**What remains of the fix is documentation, not code:
clarifying the original intent in `design/query/decisions.md`
and coverage-model spec §5.2** (see work items).
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

No pair discharges without a common witness:
a single witness that both binds T and executed I.

- Pro: The discharge claim means what users think it means.
- Con: Pairs that only ever passed via aggregate unions will fail.

### Decision: Option C

This decision fixes the same-witness requirement only:
every act of evidence credited to a pair must see both sides.
It deliberately does not settle the quantifier over a test's
bound witnesses — whether one common witness suffices or every
bound witness is held to the pair is a further question,
left open here (taken up in Decision 14).

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
provenance (producer, source artifact, discharge unit,
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

`ByExecution` claiming implies T executed in w by definition.
For `ByRootSpan` the same implication is a construction
requirement: a constructed witness's closure is **reflexive** —
it includes the discharge unit's own root span
(`root ∈ closure(root)`, a normative constraint on the closure
definition, Open Question 1) — so a witness always scores its own
annotation as executed, and binding implies execution under both
rules. Both rules are therefore instances of one predicate and
root-claiming is a refinement, never a loophole.
The reflexivity requirement is guarded by a producer unit test
(every constructed witness contains its own root span),
in the same spirit as the dogfood plan's discrimination control:
it tests the relation most likely to silently regress.
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

Stated over witnesses only — no formats in the vocabulary.
The quantifier in P1 is universal over a test's bound witnesses;
the reasoning that settles it is Decision 14's.

```
P1  discharged(T, I)  ⟺  witnesses_for(T) ≠ ∅
                          ∧ ∀w ∈ witnesses_for(T) : executed(I, w)
    where witnesses_for(T) = { w : binds(T, w) }
P2  test_executed(T)  ⟺  ∃w : binds(T, w)
P3  ever_executed(I)  ⟺  ∃w : executed(I, w)      (global; no correlation)
P4  Monotonicity, two separately-provable facts
    (discharged is their conjunction and has no single direction):
    P4a  Nonemptiness is monotone increasing: adding witnesses
         never falsifies test_executed(T) — the fail→pass
         transition at the empty boundary is "evidence was found"
         and is the only fail→pass transition there is.
    P4b  The universal clause is monotone decreasing: over an
         already-nonempty witnesses_for(T), adding a witness never
         flips a failing pair to passing; it may newly fail a
         passing pair — that is the vacuity being caught.
    The load-bearing safety statement is P4b. Spec W4 states the
    conjunction in implication form and is proven in that form.
P5  binds(T, w) under ByExecution ⟹ executed(T, w)
    (for ByRootSpan the same implication holds by the reflexive
    closure requirement, Decision 3 / Open Question 1)
```

These become a verified "Phase 4" in `duvet-coverage`
(the quantifier layer over the existing verified cells).
Direction: build it verified from the start rather than
verifying after the fact.
The engine implementation is scaffolding and may be rewritten
freely; the invariant is that the properties stay proven,
not that the pipeline stays the same.

The trusted base — one ledger, per producer family.
Each entry is named and carries a status:
**irreducible** (an axiom proper; will never be discharged by us)
or **dischargeable** (our pure code, trusted today, with a stated
mitigation and a verification target; when discharged it leaves
the ledger and joins the property inventory).
"Axiom" is reserved for the irreducible entries:
a debt that can be proven was never an axiom,
and labeling debts as axioms makes the irreducible ones look
negotiable.

```
A1  Closedness: every delivered coverage map is closed under the
    producer's reachability (CPU execution / obligation-graph
    closure).
    - runtime: instrumentation fidelity — IRREDUCIBLE.
    - prover (a): verifier faithfulness — IRREDUCIBLE
      (same category as trusting the verifier itself).
    - prover (b): closure computation — DISCHARGEABLE.
      Mitigation: golden-corpus tests + the reflexivity test
      (Decision 3). Target: Verus proof in duvet-coverage
      (a pure graph fixpoint).
A2  Individuation: each witness is the record of ONE act of
    checking (one test run, one obligation discharge).
    - runtime: the user's operational discipline; the artifact
      does not record how it was produced — IRREDUCIBLE.
      Transferred by worked example (per-test harness example
      projects), not by normative spec text; duvet does not
      attempt detection (Decision 11).
    - prover: holds by construction — no entry needed.
F1  Artifact-structure fidelity: the parse of the prover artifact
    into extents, ensures-clause spans, and loop-invariant spans
    defines dom(du) (Decision 13) and position→obligation
    attribution (Decision 12), including that binds reports only
    genuine rooting obligations — DISCHARGEABLE.
    Mitigation: golden-corpus producer tests (spec §4.3).
    Target: verification candidate in duvet-coverage.
    Failure direction under universal discharge (Decision 14):
    a spurious bind that misses I manufactures a FALSE FAILURE,
    never a false pass — the safe direction for this feature.
```

These properties and this ledger must be consolidated into the
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

### Option B: Needed (load-bearing) semantics

Use unsat cores to include only what the solver required.

- Pro: Would fail the running example — the verdict the user
  actually wants there.
- Feasibility: **demonstrated, not assumed**
  (solver-replay spike, 2026-07-29).
  Verus emits replayable solver queries (`--log smt`; measured
  free — no verification slowdown), and each ensures clause is
  guarded by a distinct `%%location_label%%N` constant — the same
  machinery Verus uses to localize *failing* postconditions.
  z3 replay with core extraction recovers per-clause needed sets,
  verified in both directions by a strict deletion matrix on a
  two-clause / disjoint-helpers probe
  ("clause 1 needed `helper_a` and not `helper_b`"),
  including the caller-side variant
  ("the caller needed the callee's clause 1, not clause 2").
- Con: The cost is not a bigger parse; it is a **new
  evidence-production category**. The producer must *drive the
  solver*: ~107 z3 invocations at duvet-coverage scale, two
  rewrite transforms between Verus's output and the executed
  queries (assert-naming, and an `ens%` guard rewrite validated
  on one probe shape only), results contingent on solver version
  and rlimit. Today's producer is a pure parser — artifact in,
  witnesses out, deterministic, testable against pinned goldens.
  A Needed producer is an experiment runner with its own trusted
  base (a pinned z3, the unverified rewrites, the label↔span
  join) — and the rewrites are exactly the glue where bugs
  migrate.
- Con: Cores are not guaranteed minimal (degradation direction:
  over-crediting, back toward Option A).
- Con: No productionized runtime analog exists to pair with it.
  The analog does exist in the research literature — *checked
  coverage* (Schuler & Zeller, ICST 2011): the backward dynamic
  slice from each test oracle, i.e. the lines whose values
  actually flowed into an assertion, rather than the lines that
  merely ran. That is the same start-from-the-check,
  keep-what-mattered relation that unsat cores compute on the
  proof side (Executed : Checked :: Consulted : Needed).
  But the production coverage tools (JaCoCo, Cobertura, LCOV)
  all emit executed-lines only, and the one published
  implementation (JavaSlicer) is research-grade. So the honest
  parity statement is: both worlds have a stronger rung in the
  literature, and neither rung has a production producer today.
  *(Amended 2026-07-29: this con originally claimed the runtime
  side had no analog at all, naming mutation testing as its
  nearest equivalent. That was a claim about the class of
  techniques; the true claim is about the instances — nothing
  productionized exists, which is the same status as unsat-core
  extraction on the prover side.)*

### Decision: Option A, with Needed reachable but deliberately not consumed

Same semantics as execution, now.
Needed strength is **reachable but not consumed** — the deferral
is an architectural boundary, chosen and named, not an artifact
limitation: **duvet parses artifacts; it does not drive
solvers.** Consuming Needed evidence would put a solver-execution
harness and its unverified rewrites inside duvet's trusted base,
and that trade is declined for now. Because the gap sits on the
far side of a named boundary rather than one dug by
under-consuming the artifact in hand, the deferral is legitimate
under Decision 17.
The witness's `strength` provenance field exists from day one so
that stronger witnesses (needed-semantics) can be added later
without changing the model — as a new producer category with its
own trusted-base ledger entries, when the trade is worth making.
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
graph — and for Verus that granularity is now known:
see Decision 18.

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

## Decision 12: Ambiguous discharge-unit attribution yields ALL candidate witnesses {#decision-12}

**Context:** Empirical (Verus SST corpus, milestone 1):
attribution of a test annotation's position to a discharge unit is
not always unique.
Verus generates obligations for derived/accessor code and stamps
them with the source range of the declaration they were generated
from — e.g. the `ExecutionStatus` enum declaration
(`types.rs:154–166`) roots two generated field-accessor
obligations with byte-identical extents.
Extent containment fails there (heads are signature-only),
and own-span ownership finds two owners.
*(Correction, post-Decision 13: the two-owner tie sits on the
extent lines 154–166; the closing line 167 was only reachable
through the deleted own-span step and is not proof-testable.)*
What should the producer do with N > 1 candidates?

### Option A: Pick one

- Con: A coin flip between obligations whose closures may differ —
  the pair's verdict becomes arbitrary. Rejected outright.

### Option B: Refuse; construct no witness

- Pro: Never guesses; matches the path-matching refusal posture.
- Con: The analogy to path-matching fails: there, choosing wrong
  scores against the wrong file's data; here every candidate is a
  genuine obligation yielding an individually honest witness.
- Con: Conflates two different situations under one W6 report —
  "no obligation exists at this position" and
  "two obligations exist here" — the worst diagnostics of the
  three options. Succeeds at doing nothing.

### Option C: Deliver one witness per owning obligation

- Pro: The model already handles it: `witnesses_for(T)` is
  set-valued, the pair verdict is a quantifier over that set, and
  runtime binding already allows
  multiple witnesses per test. Individuation (A2) holds per
  witness — this is N honest witnesses, not one merged map.
- Pro: Best diagnostics of the three outcomes:
  discharged → provenance names which obligation discharged
  (visible if it wasn't the intended one);
  bound-but-not-discharged → report lists the bound obligations by
  name, none of which executed the implementation;
  zero owners → W6 "unwitnessed" now unambiguously means
  "no obligation exists at this position."
- Con: A pair can discharge via an obligation the author did not
  intend (e.g. a generated accessor). Accepted: the claim is
  honest under consulted semantics and the provenance in the
  verdict makes it inspectable.

### Decision: Option C

This decision fixes the structure — all rooting obligations, one
witness each — and deliberately leaves the verdict over those N
witnesses to the quantifier question (Decision 14).

Producer attribution rule (Verus, generalizable):
this rule governs **ties** — N distinct obligations rooted at
the position at the same specificity level. When units of
different fineness nest around a position, rooting first
resolves to the finest containing unit; the tie rule then
applies among the obligations rooted at that level:
all of them are the discharge units, one witness each;
producers MUST NOT select arbitrarily among owners.
Zero rooted obligations means no proof witness;
Decision 13 defines how that is reported.
The original step-2 fallback (own-span ownership) is deleted:
it attributed body lines to the enclosing obligation,
conflating "consulted by" (the impl-side relation) with
"dischargeable at" (the test-side relation),
and produced witnesses their own annotations could not bind.

---

## Decision 13: Proof witnesses exist only at dischargeable positions {#decision-13}

**Context:** Empirical (milestone 1 integration):
Verus stamps many obligations with signature-only extents,
so a test annotation on a line inside a function *body*
falls inside no obligation's extent.
The step-2 fallback (own-span ownership) attributed such a line to
the enclosing obligation and constructed a witness from it —
but that witness's `ByRootSpan` claim region (the extent)
did not contain the annotation's position,
so the annotation could not bind the very witness built for it:
a dead witness, and a W6 report while the producer held the act of
checking in its hand.
Two geometries — one for materialization, one for binding —
gave contradictory answers about the same obligation.

The deeper diagnosis: a `type=test` annotation on an executable
body line is not a proof-world test at all.
Body lines are proof *ingredients* — the material consulted to
discharge the ensures — not claims.
Nothing dischargeable is rooted there.
It is the exact analog of a runtime test annotation placed outside
any test runner: there is nothing testable about that position.

### Option A: Widen the claim region to the ownership region

Make `ByRootSpan` carry extent ∪ own-file spans so the dead
witness becomes bindable.

- Pro: Dissolves the geometry mismatch mechanically.
- Con: Ratifies the wrong semantics.
  It patches the two geometries into agreement by adopting the
  *impl-side* relation ("consulted by") as the test-side binding
  rule, letting a test annotation on arbitrary body code claim the
  enclosing obligation as "its" test. Rejected.

### Option B: Restrict witness construction to dischargeable roots

A source position is **proof-testable** iff at least one
obligation is *rooted* there — iff it is in `dom(du)`.

```
dom(du) = proof-element positions only:
          fn/lemma headers (obligation extents),
          ensures clauses,
          loop invariants,
          proof asserts.
Executable body lines ∉ dom(du).
```

A test annotation whose resolved target is outside `dom(du)` gets
zero proof witnesses **by definition, not by machinery failure**,
and the report says so explicitly:
"this position is not proof-testable;
it can only be witnessed by an execution-style producer" —
distinct from W6's "no obligation exists at this position."
Such an annotation is picked up normally by runtime producers in a
mixed run (Decision 8's machinery, unchanged).

- Pro: Construction domain = binding domain, by definition.
  Dead witnesses and phantom binds become unrepresentable rather
  than fixed.
- Pro: `dom(du)` is read from the prover's artifact itself
  (extents, `:enss` clause spans, `LoopInv` spans),
  not inferred by source-language cleverness;
  the classifier stays dumb — it resolves annotation → target and
  nothing more.
- Con: Positions the old fallback silently attributed now refuse.
  Accepted: those attributions were the bug.

### Decision: Option B

Impl annotations are unaffected — body lines remain exactly right
as *implementation* targets, because `executed(I, w)` is
membership in a consulted closure and body lines are what closures
contain. The asymmetry is test-side only,
matching the semantics: tests are claims, implementations are
material.

Loop headers: deliberately **excluded** from `dom(du)` for now.
The artifact would support inclusion
(Verus isolates loops — `Loop :loop_isolation true`, own id —
so "this loop's invariants are preserved" is a genuine obligation),
but annotating the loop header rather than a specific invariant
adds cleverness ahead of need. Revisitable with evidence.

Supersedes: Decision 12's step-2 own-span fallback (deleted);
the claim-region proposal (Option A above), considered and
rejected in review.

---

## Decision 14: Discharge is universal over bound witnesses {#decision-14}

**Context:** Decisions 1–13 stated pair discharge as an
existential: `∃w : binds(T, w) ∧ executed(I, w)`.
Two late findings exposed the quantifier as wrong.
First, dual-domain positions exist — an exec fn's contract header
is both dischargeable (roots the fn's obligation) and executable
(runtime coverage records it) — so one T can bind both a proof
witness and a runtime witness, and they can disagree:
the proof consults a callee's contract while the runtime executes
its body.
Under ∃, adding a runtime producer could mask a proof-side
failure at the verdict level — a vacuous proof laundered behind a
passing test, which is the disease this feature exists to catch.
Second, the feature's goal was never
"at least one executed test annotation";
it is "**no vacuous test annotations**."
Every act of checking that a test annotation claims must actually
reach the implementation it is paired with.

### Option A: Existential discharge (∃), disagreement surfaced as diagnostics

The pair discharges if any bound witness reached I;
the report lists every bound witness with its per-witness verdict
so ✓/✗ splits are visible.

- Pro: Monotone in delivered witnesses (the original P4/W4).
- Pro: Ambiguity and dual-domain splits never fail CI.
- Con: Detection is visible but non-failing.
  A bound witness that never reaches I — a vacuous claim — is a
  footnote on a pass. Adding producers can weaken effective
  checking. Encodes "at least one," which is not the property.

### Option B: Universal discharge over bound witnesses (∀ + nonempty)

```
discharged(T, I)  ⟺  witnesses_for(T) ≠ ∅
                      ∧  ∀w ∈ witnesses_for(T) : executed(I, w)
```

- Pro: No vacuous claims. Every witness T binds is held to the
  pair; one bound witness that never reaches I fails the pair —
  and the CI run.
- Pro: Dissolves the dual-domain masking problem outright:
  runtime ✓ / proof ✗ is now a failure, and adding a producer can
  only strengthen checking. The "declared expectation" escape
  hatch (per-annotation intent syntax) becomes unnecessary.
- Pro: Same-witness discipline (Decision 1) is preserved and
  strengthened: each witness individually must see both sides.
- Con: Monotonicity inverts — adding a witness can newly fail a
  pair. Deliberate: that is the vacuity being caught.
  The sound direction survives as the new monotonicity:
  adding a witness can never flip a failing pair to passing.
- Con: Failures at multi-obligation positions (Decision 12) can
  be hard to read until the report can identify which obligation
  is which (see Open Question 2).

### Decision: Option B

Ambiguous positions are held to **all** their rooting
obligations: if a position roots N obligations, all N witnesses
must reach I. Failing there is correct pressure, not a spurious
failure — the remediation is to make the position unambiguous.
The canonical case: one `ensures A && B && C && D` clause when
the claim is about C alone is ambiguous by construction;
splitting it into four `ensures` clauses and annotating the C
clause is the fix, exactly parallel to the
"move your annotation" UX of Decision 13's not-proof-testable
bucket.
Failing without perfect legibility is the floor;
we do not convert a confusing failure into a pass.
Improving identification of which obligation failed is follow-up
work (Open Question 2), never a reason to weaken the verdict.

**The boundary of the claim, stated so it is not over-read:**
discharge under this decision eliminates vacuity at **witness
granularity** — every act of checking T claims must reach I.
Vacuity *below* witness granularity is not caught at Consulted
strength: Decision 7's running example (T on ensures #2, I inside
loop 1's body) passes, because the whole obligation's elaboration
consults the entire body. What P1 delivers is precisely
"no test annotation binds a witness whose consulted closure
misses I" — and the gap between that and full "no vacuous
pairings" is the gap the `strength` field reserves room for
(needed-semantics, Decision 7 Option B, deferred).
The feature ships with this boundary documented rather than
blocking on closing it.

Consequences:
- P1/W1 carry the universal quantifier;
  P4/W4 (monotonicity) is stated as the two facts in Decision 4
  (nonemptiness monotone increasing, universal clause monotone
  decreasing); the Phase 4 proofs are done against those
  statements.
- Per-witness ✓/✗ reporting (formerly proposed as a footnote on
  ∃-passes) becomes the **diagnosis attached to failures**:
  the report MUST list every bound witness with its per-witness
  result and strength.
- Duplicate annotation instances (two tests carrying the same
  citation) were already independent Ts and already failed
  individually; this decision governs the one-T-many-witnesses
  cases: dual-domain positions, one test in multiple reports,
  and Decision 12's multi-obligation positions.
- A test annotation executed in multiple runtime reports where
  only some reached I now fails: a per-run vacuity, reported as
  such.

Settles: the quantifier left open in Decision 1 (Option C) and
the verdict over Decision 12's N-witness positions.

---

## Decision 15: Per-file scoring-mode routing in the verified witness layer {#decision-15}

Spec §1.4 defines `executed(X, w)` as the coverage model's score —
"`is_annotation_executed`, or the degraded path." The Phase 4
formalization as first verified implemented only the first half: it
hardwired the classified two-phase scorer for every file. The engine
has always routed per file: the classified scorer where a language
classifier exists (Java only, today), the verified degraded path
(coverage-model-spec §7) where none does — every Rust file, including
this repository's — and a trust-boundary refusal (`Unknown`) for
defeated classification, `end_line == u64::MAX`, or an unclassified
file. Wiring the engine's verdicts to the verified functions (PR
review Finding 2, remediation rung 1) with that gap intact would have
made the wiring real only for classified files: the
proof-implementation divergence again, restricted to exactly the
files the dogfood does not have.

### Option A: Wire classified files only; keep the gap

- G2 becomes true only where a classifier exists.
- Two verdict paths persist for every degraded file — the finding's
  shape, quieter.

### Option B: Score every file with the degraded path

- One path, but the verified layer and the engine now *disagree* on
  classified files (comment-line skipping differs between scorers).
- Discards verified precision the engine actually uses.

### Option C: Route per file, inside the verified layer

- Thread a `ScoringMode` (`Classified` / `Degraded` / `Unscorable`)
  through the verified specs; route to the two already-verified
  scorers at one leaf function. `Unscorable` encodes the engine's
  refusals: such a file binds nothing and executes nothing — a
  uniform `false` contribution, matching §1.4's vacuous-claim rule.

### Decision: Option C

The routing choice per file is glue (G3, spec §4.4); both scorers on
either side of it
were verified before this decision (Phases 1–3). Proof structure
unchanged; 65 verified, 0 errors.

Process note: implemented 2026-07-28 during Finding 2 remediation,
without prior review; explained and ratified after the fact — this
record is that ratification. The gap was a Phase 4 formalization
defect measured against the already-ratified §1.4, not a change in
spec semantics.

---

## Decision 16: Runtime individuation is achieved by a per-test harness, transferred by example {#decision-16}

**Context:** Runtime coverage tools (JaCoCo, `cargo llvm-cov`,
grcov) produce **one aggregate report per suite run** by default —
many tests, one file. Feeding such a report to duvet satisfies
`ByExecution` claiming for every test annotation in it and
silently weakens every discharge that flows through it: an A2
violation by default, which Decision 11 deliberately does not
detect. The same problem was solved for JaCoCo before this
feature existed; the pattern is known.

### Option A: Normative spec text (a producer-obligation MUST on the user)

- Con: Duvet cannot check it and the user cannot be held to it
  mechanically — a MUST without an enforcer dresses an
  operational assumption as a requirement.

### Option B: Worked example projects, per tool

One example project per coverage tool demonstrating the harness
pattern: an outer harness enumerates the tests and runs **one
coverage invocation per test**, each producing one report file —
one report = one witness (Decision 2's degenerate case), witness
identity riding the artifact (name report files after tests).

- Pro: The discipline transfers by demonstration, which is the
  only mechanism available for an assumption duvet cannot verify.
- Con: Slower than a suite run. Accepted: per-test isolation is
  the cost of honest witnesses; there is no good simple answer.

### Decision: Option B

A2's runtime half remains what Decision 4's ledger says it is —
irreducible, the user's operational discipline — now with a
demonstrated path per tool instead of a commandment.
The untangling machinery (one artifact → many witnesses) remains
reserved for producers whose artifacts genuinely contain many
acts of checking (per-test formats, prover logs);
the LCOV/JaCoCo producers stay dumb on purpose.
Example projects are written as each runtime producer lands,
starting with `cargo llvm-cov` for the Rust dogfood.

---

## Decision 17: The granularity floor is the artifact's provable maximum {#decision-17}

**Context:** Two different deferrals hide inside "ship now,
improve later," and only one is acceptable.
Deferring **strength** (Consulted → Needed, Decision 7) defers
evidence that does not exist yet: unsat cores, feasibility
unproven, possibly prover changes. Deferring **granularity the
artifact already records** would be self-imposed weakening:
the report contains finer structure and the producer parses it as
a blob, shipping below the evidence in hand while claiming the
posture of iteration.

### Decision: Consume the artifact's demonstrated maximum from day one

A prover producer MUST consume its artifact at the maximum
granularity the artifact **provably supports** — where "provably
supports" means demonstrated by inspection of real artifacts, not
assumed. The Open Question 1 investigation is therefore
normative, not exploratory: whatever discharge-unit and closure
fineness the artifact demonstrably records sets the shipped
floor. Only what would require new evidence sources or prover
changes may be deferred.

Applied to Verus SST (spec §5.5; artifact investigation
concluded 2026-07-29):
the investigation this decision mandated is done, and the ceiling
**splits by relation**.
*Rooting* — which discharge unit an annotation attaches to —
is demonstrably sub-function: the artifact records a distinct
span for every ensures clause (`:enss`), loop invariant
(`LoopInv`), and proof assert, so clause-level discharge units
are the demonstrated maximum and consuming them is mandatory
under this decision (Decision 18).
*Closure* — what a unit's proof consulted — is function-level,
and not as a logging gap: Verus checks all of a function's
ensures clauses in one solver query (one `PostConditionSst`, one
folded `ens_exps` list) and assumes a callee's **entire** ensures
at every call site, so under Consulted semantics every unit in a
function consults the same set, and a per-clause closure computed
honestly from this artifact would equal the whole-function
closure. Function-level closure is therefore the artifact's
genuine ceiling for Consulted strength, and shipping at it
satisfies this decision.
Finer closure is a *strength* question, not a granularity
question — it requires needed-set evidence, whose feasibility and
declined consumption are recorded in Decision 7.

---

## Decision 18: Discharge units expand to clause granularity; every unit carries its function's closure {#decision-18}

**Context:** Empirical (SST artifact investigation, 2026-07-29):
the artifact records a distinct source span for every ensures
clause (`:enss` list entries), every loop invariant
(`LoopInv` nodes), and every proof assert — not just function
extents. Under Decision 17, consuming that structure is
mandatory: the demonstrated maximum sets the shipped floor.
The current producer roots at extents only, which is below the
evidence in hand.

### Decision: `dom(du)` is the union of all four unit kinds

```
dom(du) = obligation extents (fn/lemma headers)
        ∪ ensures-clause spans (:enss entries)
        ∪ loop-invariant spans (LoopInv nodes)
        ∪ proof-assert spans
```

Each unit kind roots discharge units by span containment;
the reflexivity requirement (`root ∈ closure(root)`, Decision 3)
holds at clause grain — confirmed empirically: clause-rooted
witnesses contain their own spans.

**Every unit inside a function carries the same function-level
consulted closure.** This is the artifact's ceiling for the
closure relation (Decision 17), and the boundary must be stated
plainly wherever the feature is described, because it bounds what
finer rooting buys:

- What clause-level units buy **today**: precise
  annotation-to-clause identity; failure reports that name the
  exact clause; discharge units already shaped right for a future
  strength upgrade.
- What they do **not** buy today: smaller witnesses.
  Within one function, discharge verdicts do not change —
  a clause-rooted witness and its function's witness have
  identical line sets at Consulted strength.

Conjunct arms (`ensures A && B && C`) have sub-spans in the
artifact but are not claim entries; they do not root units.
The remediation for a claim about one conjunct is user-side
clause splitting, consistent with Decision 14's ambiguity
posture. (Non-normative guidance, restated in the spec.)

---

## Decision 19: Rooting is most-specific-wins {#decision-19}

**Context:** With clause-level units in `dom(du)` (Decision 18),
an annotation on a clause line sits inside both the clause span
and the enclosing function extent. Which unit(s) does it root?

### Option A: Root every containing unit

Decision 12-style: the position roots the clause unit AND the
enclosing extent; all resulting witnesses are held to the pair
under Decision 14's universal quantifier.

- Pro: Consistent with the ambiguity rule's shape.
- Con: Multiplies obligations without adding information —
  within a function the closures are identical, so the extra
  witness is a duplicate at Consulted strength.
- Con: Under a future strength upgrade, the surviving
  function-level pairing cushions exactly the vacuity the
  upgrade is meant to expose: the fat witness passes where the
  clause's needed set would fail.

### Option B: The most specific unit containing the position wins

The annotation roots the finest unit whose span contains its
resolved position; the function extent is the fallback for
positions inside no finer unit. Decision 12's
all-rooting-obligations rule still governs genuine ties —
N distinct obligations rooted at byte-identical spans at the
*same* specificity level each yield a witness.

- Pro: Matches intent: an annotation placed on a clause means the
  clause. The user aimed; the rule preserves the aim.
- Pro: **Does not amplify the classifier's imprecision.**
  The T side of every pair is located by the degenerate text
  classifier — already the low-granularity, ambiguity-prone half
  of the pipeline. A rule that hoists a clause-line annotation to
  the whole function would stack a second lossy step on the
  first, erasing exactly the precision the annotation's placement
  expressed. Most-specific-wins keeps the rooting side from
  widening what the classifier already blurs.
- Pro: Makes a future strength upgrade mean something: the
  clause-rooted unit is held to its own needed set when that
  evidence arrives, with no fat sibling witness to hide behind.
- Con: Stricter under the future upgrade — a pass today can
  become a fail on the strength change. Deliberate: that is the
  vacuity being caught.

### Decision: Option B

Distinct from Decision 12: Decision 12
governs *ties* (several obligations, one position, same
specificity); this decision governs *nesting* (finer and coarser
units containing the same position). Nesting picks the finest;
ties within the chosen level still yield all owners.

---

## Decision 20: Discharge units are labeled from the artifact, `proof_note` when present {#decision-20}

**Context:** Failure legibility (Open Question 2) needs units
users can recognize. The artifact investigation found that
Verus's `proof_note` text lands in the SST artifact as
`ProofNoteLabel` wrapping the exact clause/invariant/assert it
annotates — a user-authored name, machine-recoverable.

### Decision: Label priority is proof_note, then span identity

A discharge unit's report label is its `proof_note` text when the
artifact records one, otherwise its span identity
(function path + unit kind + clause index/position) —
compiler-exact either way, since both come from the artifact, not
from duvet reading source text.

**Hazard, empirical (solver-replay spike, 2026-07-29, pinned
Verus):** `proof_note` on an *ensures clause* injects
unconstrained guard labels into the callee's `ens%` definition
and breaks verification of callers — loudly, at build time,
before duvet runs. Failure direction is safe (build breakage,
not misattribution), but until fixed upstream:
notes on loop invariants and proof asserts work today;
for ensures clauses, labels come from span identity.
An upstream Verus report is a work item.

---

## Open questions (not decided) {#open-questions}

1. **The exact closure definition, per prover.**
   A constructed witness's contents are the downward reachable
   set of the prover's dependency graph from the discharge unit —
   to be stated exactly, per prover.
   Settled constraints: the closure is **reflexive**
   (`root ∈ closure(root)`, Decision 3) and consumed at the
   artifact's demonstrated maximum granularity (Decision 17).
   *For Verus this question is resolved (2026-07-29): units at
   clause granularity, closure at function granularity — the
   split ceiling recorded in Decisions 17 and 18. The question
   remains open for each future prover producer (Lean, Strata,
   TLAPS, TLC): the same investigation is owed per artifact
   before its producer ships.*

2. **Identifying which obligation is which in failure reports.**
   Under Decision 14, a multi-witness failure lists the bound
   witnesses that did not reach I — but at ambiguous positions
   (generated obligations with byte-identical source ranges,
   conjunct-level obligations) the labels alone may not tell the
   user *which* claim at their position failed or how to move the
   annotation to disambiguate.
   Failing is the floor and is settled (Decision 14);
   how to make the failure legible — obligation labels,
   source-range breakdowns, remediation hints
   ("split the conjuncts") — is the open design.
   The mandatory per-witness result list (label, strength,
   per-witness ✓/✗) is the starting point, already normative.

3. **The full strength lattice.**
   The literature supplies two rungs per producer family, with an
   exact cross-family correspondence:

   | | weaker (presence) | stronger (influence) |
   |---|---|---|
   | runtime | `Executed` — the lines ran | `Checked` — the lines flowed into an assertion (backward dynamic slice from the oracle; Schuler & Zeller 2011) |
   | prover | `Consulted` — the elaboration reached the lines | `Needed` — the solver required the lines (unsat core / inductive validity core; Ghassabani et al. 2016/2017, Tomb & Joshi 2025) |

   Spec §1.3 currently names `Executed | Consulted` — the two
   rungs producers can actually deliver today — and should not
   name strengths nothing can produce. Open: whether and when to
   extend the vocabulary to
   `Executed | Checked | Consulted | Needed`, what report
   information each new rung requires from producers, and how to
   stage the consequence every rung up carries: a stronger
   strength shrinks line sets and **newly fails pairs** that
   passed at the weaker strength. That is Decision 14's inverted
   monotonicity, deliberate — but it should be predicted in
   writing before users hit it, per rung. This question is the
   contribution guide for future producer authors: reports that
   preserve per-check influence data (per-test slices, per-clause
   cores) keep these rungs reachable.

4. **Needed-semantics verdict soundness (pre-decision).**
   Recorded now so a Decision-12-shaped mistake cannot be re-made
   inside witness construction when a Needed producer is built.
   Minimal proof cores are not unique: a property can hold via
   many minimal cores, and an element can be *core-necessary*
   (in every minimal core), *core-possible* (in some minimal
   core but not all), or in none — Ghassabani et al.'s
   MUST-COV/MAY-COV categories. (Those source terms are avoided
   here: this document's MUST/MAY are RFC 2119 requirement
   keywords, an unrelated meaning.)
   The hazard: a solver returns *one* core. If I's lines are
   core-possible but absent from the extracted core, the pair
   fails — and which core the solver returned is arbitrary
   (Jaccard distances up to 0.878 between cores from different
   solvers/algorithms in Ghassabani's data). That is Decision
   12's rejected Option A — "pick one; the verdict becomes
   arbitrary" — resurfacing inside the witness's line sets.
   Constraints for the future decision: verdict-determining
   Needed semantics requires either core-possible membership
   (all-minimal-cores computation, expensive) or an explicit
   acknowledgment that failures are solver-dependent. Until
   then, Needed ships as reported strength / diagnostic only,
   never verdict-determining — a posture independently forced by
   trigger-instantiation artifacts (Tomb & Joshi §III-F: elements
   the SMT proof genuinely needed for quantifier triggering can
   report as unused, which under universal discharge would be a
   false CI failure). Two independent reasons, one posture.
   Note the present semantics already resolves this soundly:
   Consulted over-approximates even the core-possible set, so
   its only verdict-determining failure is definite
   irrelevance — over-crediting, never a false failure.

## Work items (not questions — known work) {#work-items}

- **Rooting expansion to clause granularity (Decisions 18–19).**
  Parse `:enss` clause spans, `LoopInv` spans, and proof-assert
  spans into `dom(du)`; implement most-specific-wins rooting;
  extend the reflexivity unit test over all four unit types;
  extend the golden corpus with a multi-clause fixture
  (the investigation's two-clause / disjoint-helpers probe is a
  ready-made candidate).
- **Unit labels from the artifact (Decision 20).**
  `ProofNoteLabel` extraction with span-identity fallback;
  ensures-clause labels span-only until the upstream fix.
- **Upstream Verus report:** `proof_note` on an ensures clause
  injects unconstrained guards into the callee's `ens%`
  definition and breaks callers (observed 2026-07-29, pinned
  Verus).
- **Dogfood re-annotation at clause level:** where a property
  maps to a specific clause, move the test annotation onto that
  clause — the first live exercise of Decision 19's rule.
- **Reflexivity check and guard (Decision 3).** One-time check
  against the existing SST run's constructed witnesses that every
  witness contains its own root span, plus the permanent producer
  unit test asserting `root ∈ closure(root)`.
- **LCOV-family runtime producer + Decision 16 example project:
  deferred together to the LCOV follow-up PR** (ruled 2026-07-29).
  The per-test-harness example that transfers A2 discipline
  (Decision 16) is documented alongside the runtime producer it
  demonstrates, not before it exists. First: the `cargo llvm-cov`
  one-invocation-per-test example for the Rust dogfood's mixed
  run; a JaCoCo example documents the already-solved pattern.
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
- **Retrofit pass over the pre-existing annotations** (after the
  feature lands): the old `type=implication` annotations on
  `duvet-coverage`'s Phase 1–3 proofs predate any discharge
  mechanism; they could be split into separated
  test/implementation elements discharged by this feature's
  machinery.
  Deliberately a subsequent PR, and possibly not worth it:
  the feature's own confidence already comes from the golden
  corpus tests and the dogfooded witness spec —
  retrofitting the old annotations would add confidence to the
  *rest of the codebase*, a different goal on its own timeline.
