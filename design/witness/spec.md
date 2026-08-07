# Duvet Witness: Formal Specification

**Version:** 0.1.0
**Date:** 2026-07-26
**Status:** Draft

This specification extends the
[coverage model specification](../query/coverage-model-spec.md)
with a *witness layer*:
the semantics by which annotation pairs are discharged by evidence
from multiple coverage producers —
runtime coverage reports and prover elaboration records —
under one correlation rule.

Design rationale lives in [decisions.md](decisions.md).
This document uses the normative keyword conventions of
[RFC 2119](https://www.rfc-editor.org/rfc/rfc2119).

The existing coverage model (Phases 1–3: target resolution,
execution propagation, annotation execution) is unchanged by this
specification and is referenced here as `executed` ([§1.4](#executed)).

---

## 1. Definitions {#definitions}

### 1.1 Test and implementation annotations {#annotations}

**T** denotes a `type=test` annotation and
**I** denotes a `type=implementation` annotation,
each resolved to the source lines it governs by the coverage
model's target resolution
([coverage-model-spec §2](../query/coverage-model-spec.md#annotation-target-resolution),
including the degraded path).
Nothing in this specification depends on *how* resolution happens,
only that every annotation has a resolved target.
Placement note (degraded files): in a file with no language
classifier, resolution cannot recognize comment lines as skippable —
only blank lines and annotation lines are skipped — so an annotation
(stacked or not) MUST be the last comment block above the code it
targets; an intervening ordinary comment (e.g. a doc comment between
the annotation and a proof fn header) becomes the resolved target
itself, which a prover producer sees as an unelaborated position
([Property W6](#property-w6-unwitnessed-test-annotations)).

### 1.2 Witness {#witness}

A witness is the record of **one act of checking**:
one test's execution, or one prover obligation's successful
verification.

```
Witness = {
    label:      String,            -- human-readable identity
    claim:      ClaimRule,         -- §1.5
    provenance: Provenance,        -- §1.3
    files:      Map<FilePath, CoverageReport>,
                                   -- per file: line → CoverageStatus,
                                   -- the existing verified type
}
```

A witness's `files` maps MUST be **closed**:
they contain every line the act touched,
under the producer's reachability relation
(execution for runtime producers,
obligation-graph closure for prover producers).
Closedness is a producer obligation ([§4.1](#obligation-closedness));
the engine never learns how it was achieved.

### 1.3 Provenance {#provenance}

```
Provenance = {
    producer:       String,   -- "jacoco", "lcov", "verus-sst", ...
    artifact:       String,   -- the source artifact (file, log dir)
    discharge_unit: Option<String>,
                              -- prover producers only: the obligation
                              -- the witness was constructed from
    strength:       Strength, -- Executed | Consulted
}
```

`strength` records what kind of claim the witness supports:
`Executed` (a runtime act ran these lines) or
`Consulted` (a prover's elaboration reached these lines).
Verdict output MUST report every bound witness's label and
strength with its per-witness result ([§3](#verdict-output)), so a reader can judge
each claim
(decisions.md, [Decision 7](decisions.md#decision-7) and
[Decision 14](decisions.md#decision-14)).

### 1.4 Executed {#executed}

For an annotation X and witness w:

```
executed(X, w)  ⟺  the coverage model scores X's resolved target
                    Executed against w.files
```

This is exactly the existing verified Phases 1–3
(`is_annotation_executed`, or the degraded path),
applied to one witness's coverage maps.
This specification adds no new per-annotation scoring semantics.
The verified Phase 4 layer implements the "or" per file: a
`ScoringMode` routes each file to the classified or the degraded
scorer, and engine trust-boundary refusals are encoded as
`Unscorable` — binds nothing, executes nothing
(decisions.md, [Decision 15](decisions.md#decision-15)).

If w's `files` contains no map for X's file at all,
`executed(X, w)` is false.
Under [§1.6](#discharge)'s universal discharge this is verdict-determining, not
merely credit-withholding: a bound witness whose act never touched
the implementation's *file* is a failing vote on the pair — a
vacuous claim, per [Decision 14](decisions.md#decision-14) —
never a skipped one.

### 1.5 Claim rules and binding {#claim-rules}

A test annotation must find *its own* witness.
Each witness carries one claim rule;
`binds` is total over (annotation, witness) pairs:

```
ClaimRule ::= ByExecution | ByRootSpan(file, line_range)

binds(T, w)  ⟺  match w.claim:
    ByExecution        → executed(T, w)
    ByRootSpan(f, r)   → T's resolved target EXISTS and falls
                          within r in file f
```

An annotation with no resolved target (e.g. a Structural
annotation) binds no ByRootSpan witness —
empty-target containment MUST NOT bind vacuously.
When resolution succeeds it yields exactly one target line —
a requirement owned by the resolution phase
([coverage-model-spec §2.4](../query/coverage-model-spec.md#annotation-target-resolution-properties)) —
so containment is membership of that single line.
A resolution model that yields multi-line targets MUST change
that specification and this binding rule explicitly.

`ByExecution` is the runtime rule:
the report cannot record which test produced it,
so the test claims the witness by evidence —
its own lines are executed in it.
This rule is sound only under witness individuation ([§4.2](#obligation-individuation)).

`ByRootSpan` is the prover rule:
the witness was constructed from the annotation's own position
([§5](#prover-producers)), so ownership is positional and holds by
construction — binding is a function of the root span alone,
never of the witness's maps
([Property W7](#property-w7-positional-binding-map-independence)).

Positional comparison presupposes **file identity**: before any
line of w's root span is compared with T's resolved target, the
engine must resolve which project file the witness's recorded
path refers to — and a recorded path can collide with several
project files (the witness says `src/x.rs`; the project contains
both `/a/src/x.rs` and `/b/src/x.rs`).
If the engine's path-matching relation associates an annotation's
file with more than one witness file (or one witness file with
more than one source file), the engine MUST refuse the bind and
report the ambiguity rather than select — the same posture the
producer takes when translating positions into artifact
coordinates.
The refused ambiguity is file identity and nothing else:
a position rooting several obligations yields several witnesses
([§5.3](#discharge-unit), one per obligation), and a closure
spanning several files is one witness with several per-file maps —
neither is ambiguous, and neither is refused.
Suffix matching that silently crosses two files ending in the same
path is a G1 (file-identity injectivity, [§4.4](#engine-glue)) violation and can
manufacture both false failures and, when closures overlap, false
discharges.

### 1.6 Discharge {#discharge}

```
witnesses_for(T)  =  { w ∈ delivered : binds(T, w) }

discharged(T, I)  ⟺  witnesses_for(T) ≠ ∅
                      ∧  ∀w ∈ witnesses_for(T) : executed(I, w)
```

In words: a pair (T, I) is discharged when the test binds at
least one witness and **every** witness it binds executed the
implementation. Each bound witness individually must see both
sides; one bound witness that never reaches the implementation is
a vacuous claim and fails the pair
(decisions.md, [Decision 14](decisions.md#decision-14) — the goal is no vacuous test
annotations, not at least one executed test annotation).

Discharge is a bookkeeping claim and nothing more:
the annotations bind to real acts of checking that actually
reached the annotated implementation.
It is NOT a claim that the test is a good test or the proof a
meaningful proof;
vacuity auditing of proof *content* (`assume`, `external_body`,
trivially-true assertions) is out of scope.

### 1.7 Producer {#producer}

A producer maps declared artifacts to witnesses:

```
produce : (artifacts, annotations) → Vec<Witness>
```

Runtime producers MAY ignore the `annotations` argument
(their witnesses pre-exist in the artifact).
Prover producers use it to construct witnesses ([§5](#prover-producers)).
The producer-internal artifact format MUST NOT escape the
producer; the engine consumes only `Vec<Witness>`.

The `annotations` argument is a **semantically inert
optimization**, never a semantic input.
The witness universe is defined by the artifact alone —
conceptually one potential witness per obligation —
and the argument only selects which members are materialized,
so that producers need not close every obligation to serve a few
annotations.
Normatively: every delivered witness MUST be a function of
(artifact, obligation) only — identical regardless of which
annotation caused its materialization, carrying no annotation
identity — and the filtering MUST be sound:
for every requested annotation, binding and discharge verdicts
over the materialized set MUST equal the verdicts over the full
universe.
(The filter only removes witnesses binding no requested
annotation; [W1](#property-w1-same-witness-discharge),
[W2](#property-w2-test-execution), and
[W6](#property-w6-unwitnessed-test-annotations) quantify only
over witnesses binding
the annotation in question, so soundness follows.
Stated as the consequence users rely on: every proof-testable
test annotation is witnessed by its own obligations' witnesses,
and no other annotation — and no other witness's contents
([Property W7](#property-w7-positional-binding-map-independence)
for the verified half) — influences that.
[W3](#property-w3-global-execution) quantifies over delivered
witnesses by definition.)

A witness's **identity** is the (artifact, obligation, unit)
triple; the `label` ([§1.2](#witness)) is presentation only.
Two distinct units MAY carry identical labels —
user-authored label text is not unique ([§5.5](#verus-producer)) —
so a producer MUST NOT key witness deduplication, or any other
identity-bearing decision, on the label.

---

## 2. Engine properties {#engine-properties}

These properties MUST be proven with Verus,
as a new phase of the verified coverage model
(the quantifier layer over the existing per-annotation cells).
The Verus proof files MUST carry duvet annotations citing the
anchors in this section.
The properties are stated over witnesses only;
no producer or format appears in their vocabulary.

### Property W1: Universal Same-Witness Discharge {#property-w1-same-witness-discharge}

The implementation MUST prove that it reports a pair (T, I)
discharged if and only if at least one delivered witness binds T
and every delivered witness that binds T executed I:

```
report_discharged(T, I, witnesses) = true
    ⟺  (∃ w ∈ witnesses : binds(T, w))
        ∧ (∀ w ∈ witnesses : binds(T, w) ⟹ executed(I, w))
```

No pair is discharged without a common witness, and no pair is
discharged while any witness bound to its test failed to reach
its implementation.
Evidence assembled from two different witnesses
(T bound by one, I executed by another) MUST NOT discharge;
a bound witness that did not execute I MUST fail the pair
(decisions.md, [Decision 14](decisions.md#decision-14)).

### Property W2: Test Execution {#property-w2-test-execution}

The implementation MUST prove that a test annotation is reported
executed if and only if some delivered witness binds it:

```
report_test_executed(T, witnesses) = true
    ⟺  ∃ w ∈ witnesses : binds(T, w)
```

### Property W3: Global Execution {#property-w3-global-execution}

The implementation MUST prove that an implementation annotation is
reported ever-executed if and only if some delivered witness
executed it:

```
report_ever_executed(I, witnesses) = true
    ⟺  ∃ w ∈ witnesses : executed(I, w)
```

This is a global property requiring no correlation;
it is deliberately weaker than [W1](#property-w1-same-witness-discharge)
and MUST NOT be used to
discharge pairs.

### Property W4: Failure Monotonicity {#property-w4-monotonicity}

The implementation MUST prove that adding a witness never flips a
failing pair to passing:

```
witnesses ⊆ witnesses'  ⟹
    (report_discharged(T, I, witnesses')
        ⟹ report_discharged(T, I, witnesses)
           ∨ ¬∃ w ∈ witnesses : binds(T, w))
```

Adding a witness MAY newly fail a previously-discharged pair —
that is deliberate: the added witness is a claim T now makes, and
if it does not reach I it is the vacuity being caught
(decisions.md, [Decision 14](decisions.md#decision-14)).
The only way adding witnesses turns a non-discharged pair into a
discharged one is by witnessing a previously *unwitnessed* test
(the `witnesses_for(T) = ∅` case), never by outvoting a bound
witness that failed.

### Property W5: Claim Refinement {#property-w5-claim-refinement}

The implementation MUST prove that binding under `ByExecution`
implies execution of the test in the same witness:

```
w.claim = ByExecution ∧ binds(T, w)  ⟹  executed(T, w)
```

so that positional claiming (`ByRootSpan`) is a refinement of
evidence claiming, never a loophole.

### Property W6: Unwitnessed Test Annotations Are Failures {#property-w6-unwitnessed-test-annotations}

The engine MUST report every test annotation for which no
delivered witness binds it —
across ALL configured producers —
as a failure, never silently:

```
¬∃ w ∈ witnesses : binds(T, w)   ⟹   T is reported unwitnessed
```

Consequence (intended): running a subset of producers MAY fail a
test annotation that the full set passes;
that behavior is correct
(decisions.md, [Decision 8](decisions.md#decision-8)).

### Property W7: Positional Binding is Map-Independent {#property-w7-positional-binding-map-independence}

The implementation MUST prove that binding under `ByRootSpan` does
not depend on the witness's coverage maps:

```
w.claim = w'.claim = ByRootSpan(f, r)
    ⟹  (binds(T, w) ⟺ binds(T, w'))
```

Which annotations a positional witness binds is a function of its
root span alone. Consequences: enlarging a proof witness's closure
never extends the set of test annotations it witnesses, and a
witness whose maps Hit-cover another test annotation's lines still
does not witness it — one proof's witness cannot capture another
proof's test annotation, however deep the dependency chain between
the proofs. This is [W5](#property-w5-claim-refinement)'s
complement: `ByExecution` binding is exactly map evidence;
`ByRootSpan` binding is exactly geometry.

---

## 3. Verdict output requirements {#verdict-output}

For every discharged pair, the output MUST name, in verbose
output, every bound witness (its label) and its strength ([§1.3](#provenance)).
Verbose-gated because a run at real scale
discharges hundreds of pairs, and naming every witness for each of
them by default would bury the failure reports this check exists
to surface; failure output is never verbose-gated.
For every pair that fails because a bound witness did not execute
the implementation (W1's universal clause), the output MUST list
**every** bound witness with its per-witness result
(executed I / did not execute I) and strength,
so the failing claim is identifiable —
the engine computes all of these to evaluate the verdict,
and the disagreement MUST never be silent
(decisions.md, [Decision 14](decisions.md#decision-14);
legibility improvements are tracked in
[Follow-ups](decisions.md#follow-ups) and never weaken the verdict).
For every unwitnessed test annotation (W6), the output MUST
identify the annotation and state that no configured producer
yielded a witness for it.

---

## 4. Producer obligations and trusted base {#producer-obligations}

The engine properties in [§2](#engine-properties) hold only relative to the following
producer obligations.
Where an obligation cannot be proven, it is a **named axiom** of
the trusted base and MUST be recorded as such.

One invariant frames all of them:
**producers deliver facts and never render verdicts.**
Delivering zero witnesses for an annotation is a fact
(possibly with a reason attached, [§5.2](#two-pass-construction)'s not-proof-testable),
not a failure; every failure — unwitnessed ([W6](#property-w6-unwitnessed-test-annotations)),
undischarged ([W1](#property-w1-same-witness-discharge)) — is an engine verdict over the delivered set.

### 4.1 Closedness {#obligation-closedness}

Every delivered `files` map MUST be closed under the producer's
reachability relation ([§1.2](#witness)).

- Runtime producers: closedness is delegated to the external tool
  (the runtime physically performed the closure).
  Instrumentation gaps under-close the map;
  the failure direction is withheld credit (false failure),
  never false discharge. **Axiom.**
- Prover producers: closedness splits into
  (a) the verifier's record faithfully reflects what elaboration
  consulted — **axiom**, same category as trusting the verifier
  itself — and
  (b) the closure computation over that record is correct —
  our code, which SHOULD be verified in `duvet-coverage`
  (it is a pure graph fixpoint).

### 4.2 Individuation {#obligation-individuation}

Every delivered witness MUST be the record of exactly one act of
checking.

- Runtime producers: individuation is the operator's
  responsibility — one instrumented run per test.
  The artifact does not record how it was produced,
  so duvet does not attempt detection
  (decisions.md, [Decision 11](decisions.md#decision-11)). **Axiom.**
- Prover producers: individuation holds by construction ([§5](#prover-producers));
  no axiom needed.

### 4.3 Producer testing {#obligation-testing}

Each producer MUST be unit-tested against golden artifacts
(a real report file; a real prover log),
since producers are unverified glue at the trust boundary.

### 4.4 Engine glue {#engine-glue}

The verified layer's guarantees ([§2](#engine-properties)) reach the user only through
unverified engine glue. Each glue component is named, bounded,
and unit-tested (the posture [§4.3](#obligation-testing) takes for producers):

- **G1 (file identity).** The engine adapter MUST map file paths
  to the model's file identities injectively and consistently
  across all annotations and witnesses in one run, and MUST
  deliver each witness's per-file maps free of duplicate
  identities. Ambiguous producer-path matches are refused at
  bind time, never selected among ([§1.5](#claim-rules)).
  What remains axiomatic: an absolute path is a faithful file
  identity.
- **G2 (call obligation).** The engine MUST compute every pair,
  test, and global verdict (Properties
  [W1](#property-w1-same-witness-discharge)–[W4](#property-w4-monotonicity),
  [W6](#property-w6-unwitnessed-test-annotations)) by calling the
  verified layer's functions, and MUST derive per-witness
  failure diagnostics ([§3](#verdict-output)) from the same verified cells;
  no parallel engine-side verdict computation may exist.
  The verified layer cannot check its own callers, so G2 is a
  trusted-base item — enforced by review and by the engine's
  end-to-end test suite.
- **G3 (mode routing).** The adapter MUST assign each
  annotation's file the scoring mode its classification actually
  selected — classified, degraded, or the trust-boundary refusal
  that binds nothing and executes nothing
  ([§1.4](#executed); decisions.md, [Decision 15](decisions.md#decision-15)). The adapter MUST establish
  the verified functions' preconditions at the boundary —
  filter or degrade before calling, never assume.
- **G4 (bound-set precomputation).** The glue MAY compute a test's
  bound-witness set once — each membership decided by the verified
  binding cell — and pass it to a bound-set-taking verified
  discharge entry point, provided the verified layer proves that
  entry point's verdict equal to [§1.6](#discharge) discharge over
  the full delivered set whenever the supplied set is exactly
  `witnesses_for(T)`, sound and complete by index.
  The exactness precondition MUST be established the G3 way:
  the set is assembled from the verified binding cells' results
  for the same test context and witness list, never recomputed
  engine-side.

---

## 5. Prover producers {#prover-producers}

A prover producer turns one prover's artifact into witnesses.
Its requirements come in two layers: this section's, which are
artifact-independent, and a per-producer statement of how that
producer meets them ([§5.5](#verus-producer) for Verus).
Every prover producer owes its artifact an investigation before
it ships: the discharge-unit and closure granularity the artifact
demonstrably records — demonstrated by inspection of real
artifacts, not assumed — MUST be established and consumed as that
producer's shipped floor
(decisions.md, [Decision 17](decisions.md#decision-17)).

### 5.1 Witnessable annotations {#witnessable-annotations}

Prover producers MUST construct witnesses for `type=test`
annotations only
(decisions.md, [Decision 6](decisions.md#decision-6);
self-discharging implication annotations are a deferred separate
feature).

### 5.2 Construction from annotations {#two-pass-construction}

Prover witnesses are constructed, not found: for each witnessable
annotation ([§5.1](#witnessable-annotations)) that is **live** —
its resolved target was elaborated by the verifier — the producer
determines the discharge unit(s) rooted at that position
([§5.3](#discharge-unit)), computes each unit's closure
([§5.4](#closure)), and delivers one witness per unit with claim
rule `ByRootSpan(discharge unit's extent)`.

If the producer cannot unambiguously translate an annotation's
position into artifact coordinates (e.g. the source path matches
several artifact paths), it MUST abort the run with the ambiguity
rather than skip the position: silently dropping a position would
convert a configuration defect into a missing-witness verdict.
This is a deliberate posture, not an accident of implementation.

A source position is **proof-testable** iff at least one
obligation is rooted there — iff it is in the domain of the
discharge-unit map ([§5.3](#discharge-unit)).
If a live annotation's resolved target is not proof-testable,
the producer MUST deliver no witness for it,
and the report MUST identify the annotation as
*not proof-testable* ("this position carries no dischargeable
obligation; it can only be witnessed by an execution-style
producer") — a report distinct from Property W6's
"no witness from any configured producer."
In a mixed run such an annotation binds runtime witnesses
normally.

### 5.3 Discharge unit {#discharge-unit}

The discharge unit of an annotation position is the prover
obligation that certifies that position
(the proof-world analog of "the test containing this annotation").
The mapping from position to unit is prover-specific and lives
inside each producer (decisions.md, [Decision 9](decisions.md#decision-9)).
Its domain (`dom(du)`) MUST contain only positions where an
obligation is *rooted* — proof-element positions,
at every granularity the artifact demonstrably records
(decisions.md, Decisions 13, 17, 18):
fn/lemma headers (obligation extents), ensures clauses,
loop invariants, and proof asserts.
Executable body lines MUST NOT be in the domain:
no obligation is rooted at a body line —
body lines are material a proof consults,
not claims a prover discharges —
so a test annotation there is category-mismatched,
and is reported *not proof-testable* rather than unwitnessed
(decisions.md, [Decision 13](decisions.md#decision-13)).

**Rooting is most-specific-wins** (decisions.md, Decision 20):
an annotation roots the finest unit whose span contains its
resolved position; the enclosing extent is the fallback for
positions inside no finer unit. A producer MUST NOT hoist an
annotation placed on a clause, invariant, or assert to the
enclosing function's unit.

Attribution MAY be ambiguous *within one specificity level*:
provers stamp generated obligations with the source range of the
declaration they were generated from,
so one proof-testable position can root several obligations at
the same level.
When N obligations root a position at the chosen level, the
producer MUST deliver one witness per rooting obligation and
MUST NOT select among them (decisions.md, Decision 12).
Under [Decision 14](decisions.md#decision-14) the annotation is held to **all** of them:
every delivered witness at that position must execute I for the
pair to discharge, and the verdict's per-witness results name
which obligation(s) failed.
An ambiguous position that fails is correct pressure to
disambiguate — e.g. splitting a conjoined
`ensures A && B && C && D` into separate clauses and annotating
the intended one. Conjunct arms do not root units
(decisions.md, [Decision 18](decisions.md#decision-18)); clause splitting is the remediation
for a claim about one conjunct.

### 5.4 Closure {#closure}

A constructed witness's `files` maps MUST equal the set of
**span-start lines** over the downward reachable set of the
prover's obligation graph, starting from the discharge unit
(decisions.md, [Decision 19](decisions.md#decision-19)):
a line enters a fill iff a span of a reached function begins on
that line — every span, at every nesting depth, declaration
spans included, so the header line of a wrapped signature is
addressable.
Reachability is transitive: the closure follows the obligation
graph's reference edges through any number of call or reference
hops — a lemma reaching a fn reaching a fn reaching a fn: all of
them enter — until a fixpoint.
The fill is a declared **projection** of the retained fact — the
per-function consulted span set the producer parses and keeps —
chosen for the consumer that exists today, positional annotation
evaluation (`target ∈ fill`), for which it is lossless; a future
consumer needing execution extents (runtime-map union, strength
comparison) MUST extend the producer to deliver the retained
spans rather than reinterpret the fill.
The map is not a claim that only those lines were consulted:
the three consumers named on record — LCOV-union comparison,
strength-lattice work, and report phrasing about specific
consulted lines — take the retained spans, never a
reinterpretation of this map
(decisions.md, [Decision 19](decisions.md#decision-19)).
Every function in the reachable set — exec, proof, and spec
alike — is **transparent** to this evaluation: the fill applies
the same span-start rule to the root's own body and, recursively,
to the body of every consulted function, with no special case at
the function barrier. Transparency is a traceability property of
what duvet reads, not a change to proof semantics: the prover
keeps reasoning by contract.

Comment and blank lines are excluded by **theorem**, not by
rule: spans anchor AST nodes, and ordinary comments and blanks
are not nodes, so no span begins on one. Producers MUST NOT
lexically classify lines at runtime, and MUST pin the theorem
with a test that lexes checked-in fixture sources against the
golden artifacts (guarding against macro-expansion span
placement). A doc-comment line MAY begin a span — doc comments
are attribute nodes — and the fill records it honestly. A line
the verifier never elaborated — unverified code — appears in no
span set and MUST NOT appear in any fill.

The closure MUST be **reflexive**: it includes the discharge
unit's own root node (`root ∈ closure(root)`), so the root
function's own span-start lines — its declaration line
included — are in every one of its witnesses' fills.
Binding stays positional and map-independent
([§1.5](#claim-rules),
[Property W7](#property-w7-positional-binding-map-independence)):
a root-span line on which no span begins binds its annotation by
geometry even when it is absent from the fill.
Producers MUST carry a unit test asserting reflexivity for every
constructed witness.
Only reachable nodes contribute;
nothing outside the reachable set may be included.
The closure MUST be computed at the finest granularity the
artifact demonstrably supports
(decisions.md, [Decision 17](decisions.md#decision-17) — deferral is legitimate only at the
artifact's ceiling or across a named architectural boundary);
what is normative now:
the closure MUST be a fixpoint (no truncation at a depth bound),
and the semantics is *consulted* (strength `Consulted`, [§1.3](#provenance)),
not load-bearing dependency.

**The closure ceiling MUST be stated per producer, because it
bounds what unit granularity buys.** Where a prover checks a
function's obligations in one solver query and assumes callee
contracts whole (Verus does both, §5.5), every discharge unit
inside a function carries the identical function-level consulted
closure: finer units buy precise identity and legible failures,
not smaller witnesses, and discharge verdicts within one function
do not differ across its units at Consulted strength.
Reports and documentation MUST NOT present clause-level units as
implying clause-level evidence.

### 5.5 The Verus producer {#verus-producer}

Grounded empirically (2026-07-26, Verus 0.2026.05.24.ecee80a,
`cargo verus build -p duvet-coverage -- --log vir-sst`;
artifact granularity investigation and solver-replay spike,
2026-07-29):

- The artifact is per-module `*-sst.vir` S-expression files.
  Top-level `FunctionSst` blocks carry a fully-qualified name
  (`Fun :path`) and their own source extent — these are the
  obligation nodes.
- Cross-obligation references appear as symbolic
  `(Fun :path ...)` forms, not inlined spans — these are the
  edges. A flat span inventory of one block covers essentially
  only its own extent, so the Verus producer MUST parse its
  artifact once into a structure of obligation nodes and
  reference edges and derive everything else from that
  structure — parse-into-structure is a MUST, not an
  optimization.
- Construction ([§5.2](#two-pass-construction)) is two-pass over
  that structure.
  Pass 1 (liveness) projects an *aggregate executability map* —
  every line the verifier elaborated — and runs the existing
  resolve/classify/score machinery over it to determine which
  witnessable annotations are live.
  Pass 2 (construction) re-interrogates the structure per live
  annotation: discharge units, closures, witnesses.
- The aggregate executability map MUST NOT be delivered as a
  witness: it is many obligations wearing one map, and delivering
  it would violate [§4.2](#obligation-individuation) by
  construction; it exists only as pass-1 scaffolding inside the
  producer.
- Sub-function structure carries per-unit spans: `:enss` clause
  lists (one span per ensures clause), `LoopInv` nodes, and
  proof-assert spans. Clause-level discharge units are the
  artifact's demonstrated rooting maximum.
- The closure is function-level, as a semantic fact of the
  encoding rather than a logging gap: all of a function's ensures
  clauses are checked in one solver query (one `PostConditionSst`,
  one folded `ens_exps` list), and a callee's entire ensures is
  assumed at every call site. Per-clause needed sets exist only
  at the solver level (recoverable by z3 replay with cores —
  demonstrated, and deliberately not consumed; decisions.md,
  [Decision 7](decisions.md#decision-7)).
- Fills are span-start-line sets ([§5.4](#closure), decisions.md,
  [Decision 19](decisions.md#decision-19)): per reached
  `FunctionSst`, the producer collects every span-shaped string
  in the block as an inclusive line range — the function's span
  set — and the node contributes exactly the start line of each
  range. Grounded empirically (2026-08-04, pinned Verus): no
  ordinary-comment or blank line begins a span anywhere in the
  golden corpus or the dogfood run (the §5.4 theorem, pinned by
  the tripwire test); exec statements inside verified functions
  do begin spans — the verifier traverses what it checks — and
  are claimed.
- The Verus producer MUST treat `FunctionSst` blocks as closure
  nodes and `Fun :path` references as edges, and MUST support
  discharge units of all four kinds: obligation extents,
  `:enss` clause spans, `LoopInv` spans, and proof-assert spans
  (decisions.md, Decision 18).
  Rooting is most-specific-wins ([§5.3](#discharge-unit),
  [Decision 20](decisions.md#decision-20));
  ties at one specificity level yield all owners
  ([Decision 12](decisions.md#decision-12) and
  [Decision 13](decisions.md#decision-13)).
  Own-span (body-line) ownership is NOT attribution:
  executable body lines are outside `dom(du)`.
  Loop header lines are excluded from `dom(du)` initially
  (the artifact would support them via loop isolation;
  deliberately deferred).
- Unit labels (decisions.md, Decision 21): `ProofNoteLabel` text
  when the artifact records it, otherwise span identity
  (function path + unit kind + clause index).
  Until the upstream `proof_note`-on-ensures defect is fixed,
  ensures-clause labels MUST come from span identity.

---

## 6. Relationship to the coverage model specification {#relationship}

- Phases 1–3 of
  [coverage-model-spec.md](../query/coverage-model-spec.md) are
  unchanged; `executed` ([§1.4](#executed)) is defined in terms of them.
- Pair discharge is
  [Property W1](#property-w1-same-witness-discharge)'s
  same-witness rule; a non-correlating OR over all delivered
  witnesses survives only as
  [Property W3](#property-w3-global-execution)
  (global, and never used to discharge pairs).
