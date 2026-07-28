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
The requirement keywords MUST, MUST NOT, SHOULD, and MAY are to be
interpreted as described in RFC 2119.

The existing coverage model (Phases 1–3: target resolution,
execution propagation, annotation execution) is unchanged by this
specification and is referenced here as `executed` (§1.4).

---

## 1. Definitions {#definitions}

### 1.1 Test and implementation annotations {#annotations}

**T** denotes a `type=test` annotation and
**I** denotes a `type=implementation` annotation,
each resolved to the source lines it governs by the coverage
model's target resolution
(coverage-model-spec §2, including the degraded path).
Nothing in this specification depends on *how* resolution happens,
only that every annotation has a resolved target.
Placement note (degraded files): in a file with no language
classifier, resolution cannot recognize comment lines as skippable —
only blank lines and annotation lines are skipped — so an annotation
(stacked or not) MUST be the last comment block above the code it
targets; an intervening ordinary comment (e.g. a doc comment between
the annotation and a proof fn header) becomes the resolved target
itself, which a prover producer sees as an unelaborated position
(Property W6).

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
Closedness is a producer obligation (§4.1);
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
strength with its per-witness result (§3), so a reader can judge
each claim
(decisions.md, Decisions 7 and 14).

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

If w's `files` contains no map for X's file at all,
`executed(X, w)` is false.
Under §1.6's universal discharge this is verdict-determining, not
merely credit-withholding: a bound witness whose act never touched
the implementation's *file* is a failing vote on the pair — a
vacuous claim, per Decision 14 — never a skipped one.
(Surfaced during Phase 4 verification, 2026-07-27;
pinned by the verified layer's Decision-14 test.)

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
(Resolution yields at most one target line in the current model,
so containment is membership of that single line;
found during Phase 4 formalization, 2026-07-27.)

`ByExecution` is the runtime rule:
the report cannot record which test produced it,
so the test claims the witness by evidence —
its own lines are executed in it.
This rule is sound only under witness individuation (§4.2).

`ByRootSpan` is the prover rule:
the witness was constructed from the annotation's own position
(§5), so ownership is positional and holds by construction.
Positional comparison requires file identity: if the engine's
path-matching relation associates an annotation's file with more
than one witness file (or one witness file with more than one
source file), the engine MUST refuse the bind and report the
ambiguity rather than select — the same posture the producer takes
when translating positions into artifact coordinates.
Suffix matching that silently crosses two files ending in the same
path is a G1 (file-identity injectivity) violation and can
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
(decisions.md, Decision 14 — the goal is no vacuous test
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
Prover producers use it to construct witnesses (§5).
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
annotation; W1, W2, and W6 quantify only over witnesses binding
the annotation in question, so soundness follows.
W3 quantifies over delivered witnesses by definition.)

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
(decisions.md, Decision 14).

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
it is deliberately weaker than W1 and MUST NOT be used to
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
(decisions.md, Decision 14; this inverts the direction of the
original monotonicity property).
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
*(Tentative: if this resists proof as stated,
it may be restated or demoted to a tested property
without weakening W1–W4; see decisions.md, Decision 4.)*

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
(decisions.md, Decision 8).

---

## 3. Verdict output requirements {#verdict-output}

For every discharged pair, the output MUST name, in verbose
output, every bound witness (its label) and its strength (§1.3).
(Scoped to verbose output, 2026-07-27: a run at real scale
discharges hundreds of pairs, and naming every witness for each of
them by default would bury the failure reports this check exists
to surface. Failure output is never verbose-gated.)
For every pair that fails because a bound witness did not execute
the implementation (W1's universal clause), the output MUST list
**every** bound witness with its per-witness result
(executed I / did not execute I) and strength,
so the failing claim is identifiable —
the engine computes all of these to evaluate the verdict,
and the disagreement MUST never be silent
(decisions.md, Decision 14; legibility improvements are tracked
as Open Question 2 and never weaken the verdict).
For every unwitnessed test annotation (W6), the output MUST
identify the annotation and state that no configured producer
yielded a witness for it.

---

## 4. Producer obligations and trusted base {#producer-obligations}

The engine properties in §2 hold only relative to the following
producer obligations.
Where an obligation cannot be proven, it is a **named axiom** of
the trusted base and MUST be recorded as such.

One invariant frames all of them:
**producers deliver facts and never render verdicts.**
Delivering zero witnesses for an annotation is a fact
(possibly with a reason attached, §5.2's not-proof-testable),
not a failure; every failure — unwitnessed (W6),
undischarged (W1) — is an engine verdict over the delivered set.

### 4.1 Closedness {#obligation-closedness}

Every delivered `files` map MUST be closed under the producer's
reachability relation (§1.2).

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
  (decisions.md, Decision 11). **Axiom.**
- Prover producers: individuation holds by construction (§5);
  no axiom needed.

### 4.3 Producer testing {#obligation-testing}

Each producer MUST be unit-tested against golden artifacts
(a real report file; a real prover log),
since producers are unverified glue at the trust boundary.

---

## 5. Prover producers {#prover-producers}

### 5.1 Witnessable annotations {#witnessable-annotations}

Prover producers MUST construct witnesses for `type=test`
annotations only
(decisions.md, Decision 6;
self-discharging implication annotations are a deferred separate
feature).

### 5.2 Two-pass construction {#two-pass-construction}

A prover producer MUST parse its artifact once into a structure
of obligation nodes and reference edges,
and derive both of the following from it:

1. **Pass 1 — liveness.** An *aggregate executability map*
   (every line the verifier elaborated),
   used with the existing resolve/classify/score machinery to
   determine which witnessable annotations are live.
2. **Pass 2 — construction.** For each live annotation:
   its discharge unit (§5.3), that unit's closure (§5.4),
   and from the closure one witness with claim rule
   `ByRootSpan(discharge unit's extent)`.

The aggregate executability map MUST NOT be delivered as a
witness: it is many obligations wearing one map,
and delivering it would violate §4.2 by construction.

If the producer cannot unambiguously translate an annotation's
position into artifact coordinates (e.g. the source path matches
several artifact paths), it MUST abort the run with the ambiguity
rather than skip the position: silently dropping a position would
convert a configuration defect into a missing-witness verdict.
This is a deliberate posture, not an accident of implementation.

A source position is **proof-testable** iff at least one
obligation is rooted there — iff it is in the domain of the
discharge-unit map (§5.3).
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
inside each producer (decisions.md, Decision 9).
Its domain (`dom(du)`) MUST contain only positions where an
obligation is *rooted* — proof-element positions:
fn/lemma headers, ensures clauses, loop invariants, proof asserts.
Executable body lines MUST NOT be in the domain:
they are proof ingredients (consulted material), not claims,
and a test annotation there is not a proof-world test
(decisions.md, Decision 13).
Witness granularity is the image of the annotation's resolved
position under this mapping,
at the finest granularity the producer declares it supports.

Attribution MAY be ambiguous *within the domain*: provers stamp
generated obligations with the source range of the declaration
they were generated from,
so one proof-testable position can root several obligations.
When N obligations root a position, the producer MUST deliver one
witness per rooting obligation and MUST NOT select among them
(decisions.md, Decision 12).
Under Decision 14 the annotation is held to **all** of them:
every delivered witness at that position must execute I for the
pair to discharge, and the verdict's per-witness results name
which obligation(s) failed.
An ambiguous position that fails is correct pressure to
disambiguate — e.g. splitting a conjoined
`ensures A && B && C && D` into separate clauses and annotating
the intended one.

### 5.4 Closure {#closure}

A constructed witness's `files` maps MUST equal the source spans
of the downward reachable set of the prover's obligation graph,
starting from the discharge unit.
Only reachable nodes contribute;
nothing outside the reachable set may be included.
The precision of "reachable" is bounded by the granularity of the
prover's graph and is not yet fully specified
(decisions.md, Open Question 1);
what is normative now:
the closure MUST be a fixpoint (no truncation at a depth bound),
and the semantics is *consulted* (strength `Consulted`, §1.3),
not load-bearing dependency.

### 5.5 The Verus producer {#verus-producer}

Grounded empirically (2026-07-26, Verus 0.2026.05.24.ecee80a,
`cargo verus build -p duvet-coverage -- --log vir-sst`):

- The artifact is per-module `*-sst.vir` S-expression files.
  Top-level `FunctionSst` blocks carry a fully-qualified name
  (`Fun :path`) and their own source extent — these are the
  obligation nodes.
- Cross-obligation references appear as symbolic
  `(Fun :path ...)` forms, not inlined spans — these are the
  edges. A flat span inventory of one block covers essentially
  only its own extent, which is why §5.2's parse-into-structure
  requirement is a MUST and not an optimization.
- Sub-function structure (`:enss` clause lists,
  `LoopInv` nodes) exists in the artifact;
  finer-than-function discharge units are therefore expressible
  later without a format change.
- The Verus producer MUST treat `FunctionSst` blocks as nodes and
  `Fun :path` references as edges,
  and MUST support whole-proof-fn discharge units in its first
  version. Finer units MAY follow.
  Its attribution rule: an obligation is a discharge unit for a
  position iff it is *rooted* there —
  extent containment, plus proof-element spans
  (`:enss` clauses, `LoopInv` nodes, proof asserts) as they are
  supported; all rooting obligations, one witness each
  (§5.3, Decisions 12 and 13).
  Own-span (body-line) ownership is NOT attribution:
  executable body lines are outside `dom(du)`.
  Loop header lines are excluded from `dom(du)` initially
  (the artifact would support them via loop isolation;
  deliberately deferred).

---

## 6. Relationship to the coverage model specification {#relationship}

- Phases 1–3 of coverage-model-spec.md are unchanged;
  `executed` (§1.4) is defined in terms of them.
- coverage-model-spec.md §5.2 (multi-report folding) is
  superseded by Property W1 for pair discharge;
  the OR-fold shape survives only as Property W3
  (global, non-correlating).
  Amending that section's text is scoped to the correlation-fix
  work stream.
