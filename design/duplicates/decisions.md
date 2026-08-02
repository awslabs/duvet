# Duplicate Annotations — Decisions

**Status:** Decisions 1–8 proposed, not yet implemented.
References to `design/witness/` are forward references to the
witness-coverage work (in flight, not yet on `main`).

## Context

The duplicates check today classifies each annotation type against
itself: two same-type annotations whose quotes mutually cover each
other on the same section land in the `duplicates` bucket and the
check hard-FAILs. There is no escape hatch.

That default is right far more often than it is wrong. Duplicate
annotations are usually spray: copies accumulate ("one over here,
and over here, and over here"), nobody cleans them up, and each
copy is a claim with no additional substance. The failure mode is
cheap to create — especially for an LLM emitting annotations — and
expensive to unwind.

But it is not *never* right to duplicate. The motivating case: an
implementation annotation whose requirement is evidenced twice —
a test annotation on a Verus proof, and a second test annotation on
an executed property-based test. Binding is positional, so one
annotation physically cannot sit on both artifacts. Two evidence
artifacts structurally force two annotations, and the duplicates
check punishes exactly the behavior we want to encourage.

This document decides how to admit that case without opening the
spray door.

## The governing principle

> **Duplicates are permissible exactly where a coverage check has
> the means to disambiguate them. Where no check can tell two
> copies apart, they must be unique.**

Applied per-type, this yields the table in
[Decision 3](#decision-3). Applied per-instance, it yields the
distinctness rule in [Decision 5](#decision-5): two copies whose
delivered evidence is identical were not, in fact, disambiguated —
regardless of what their declarations say.

The corollary invariant, which every decision below preserves:

> **More information about annotations can justify a duplicate;
> less information never can. There is no path to PASS that does
> not go through evidence.**

A waiver marker (`duplicate-ok=reason`) was considered and
rejected at the outset: a declaration is exactly as cheap to spray
as the annotation it excuses. Under these decisions, duplicating
an annotation costs a distinct, passing verification artifact.

## Vocabulary

- **Duplicate set** — a maximal set of same-type annotations whose
  quotes mutually cover each other on the same section. Computed
  by the existing classification (`classify_annotation_coverage`
  of a type against itself).
- **Resolved target** — the target line an annotation resolves to
  (`resolve_target_line`); the position the binding model scores.
- **Witness (w), binds(T, w), witnesses_for(T), discharged(T, I)**
  — as defined in `design/witness/spec.md` §1.5–1.6. In
  particular discharge quantifies **universally** over bound
  witnesses: every witness a test binds must execute the
  implementation. Witness multiplicity is a conjunction of
  obligations, not a tolerance — the model this document extends
  to annotation multiplicity.
- **Cap** — the per-type maximum duplicate-set size
  ([Decision 1](#decision-1)).

---

## Decision 1: Per-type caps are the sole enablement switch {#decision-1}

**Context:** How does a user opt into allowing duplicates, and how
is the legacy strict behavior preserved?

### Option A: A mode flag (`--allow-duplicates`, old/new behavior versions)

- Con: a binary gate admits unbounded multiplicity the moment it
  opens.
- Con: mode flags fork the check's semantics permanently.

### Option B: Per-type numeric caps, default 1

`--max-duplicates <TYPE=N>` (repeatable), e.g.
`--max-duplicates test=2`. Only `test` and `implementation` may be
raised ([Decision 3](#decision-3)). Default for every type is 1.

- Pro: at cap 1 the check is bit-for-bit the historical strict
  check (Property [P-D3](#properties)). Nothing loosens until a
  user writes a bigger number, and that number is visible in
  config review.
- Pro: the cap bounds proliferation even when every copy carries
  valid evidence — call-graph spray ("MUST foo" on the function,
  its caller, the caller's caller…) loses even when all sites
  execute.
- Pro: per-type control matches reality: the motivating case is
  test-type (`test=2`, proof + PBT); implementation duplication
  has no accepted motivating case
  ([Rejected: multi-path](#rejected-multi-path)).

### Decision: Option B

**Consequences:** Raising a cap is an explicit, audited decision.
The duplicates report lists every allowed duplicate set with its
size and member locations, so multiplicity is always surfaced,
never silent. Cap semantics are monotone
([P-D4](#properties)): raising N never flips a passing project to
failing.

---

## Decision 2: Same-resolved-target duplicates always fail {#decision-2}

**Context:** Copies stacked at the same location — five identical
`//=` blocks above one function — resolve to the same target line.

**Decision:** Two same-type duplicates with the same resolved
target FAIL the duplicates check unconditionally, regardless of
cap. No evidence can ever distinguish two annotations at one
location: binding is a function of the resolved target, so their
witness sets are identical by construction. This is the statically
visible shadow of [Decision 5](#decision-5)'s evidential rule —
the fragment decidable without running anything.

"Same location" is defined as **same resolved target**, not same
source line: stacked copies above one statement collapse to one
target and die here; copies scattered through a function body
resolve to different targets and are governed by the cap and by
[Decision 5](#decision-5).

---

## Decision 3: Per-type permissibility {#decision-3}

**Context:** Which annotation types can ever justify duplicates?

Applying the governing principle per-type:

| Type | Coverage-check perspective | Duplicate verdict |
|---|---|---|
| `test` | witnessed + discharged (W6, §1.6) | cap may be raised |
| `implementation` | executed under covering tests' witnesses | cap may be raised |
| `spec` | none | unique, always |
| `todo` | none — and a todo-covered requirement already fails the implementation check, so a duplicate todo is noise on a fail | unique, always |
| `exception` | none — a declaration of absence has nothing to execute or prove | unique, always |
| `implication` | none **today** | unique **today**; see below |

**Implication is its own animal.** An exception's position is
incidental; an implication's position is the claim ("*this
artifact* is correct by construction"). Structurally it patterns
with `implementation`, not with `exception`. What forces
uniqueness today is the evidence question: implications carry no
checkable obligation, and the proof-world answer (self-discharging
implications, where a verifier's elaboration is the implication's
own evidence) was deliberately deferred in the witness design
("Option B: self-discharging implications" — nobody can state the
discharge obligation precisely yet). Runtime execution of an
implication's position is evidence about the wrong property: it
shows liveness, not construction.

**Decision:** unique today; revisit as a rider on the
self-discharging-implications feature, which owns its own decision
document.

---

## Decision 4: No inter-check plumbing — independent contracts {#decision-4}

**Context:** When the duplicates check passes a within-cap
duplicate set, how does the evidence obligation reach the coverage
check?

### Option A: Deferred-status plumbing

The duplicates check emits `deferred` items; the query engine
fails them if no coverage check runs in the same invocation.

- Con: couples the checks; a check's verdict depends on which
  other checks were requested.
- Con: complicates command-line composition for no guarantee the
  independent design doesn't already provide.

### Option B: Independent contracts

The duplicates check enforces structure only (Decisions 1–3): run
alone, a within-cap duplicate set **passes**, with its size and
members surfaced in the report. The coverage check enforces
evidence (Decision 5) natively and unconditionally — whether or
not the duplicates check ever runs. Run the full report and the
composition just works; run one check and you get exactly that
check's guarantee.

### Decision: Option B

**Consequences:** The domination claim that makes this safe, with
its two scoping conditions stated so nobody over-reads it:

1. For **test** duplicates, the full coverage check alone
   dominates: every copy is an independent T — unwitnessed → W6
   fail; witnessed → per-pair ∀w billing; evidentially identical →
   [Decision 5](#decision-5) fail.
2. For **implementation** duplicates, coverage dominates the
   *duplicate-specific* badness only. A citation on a quote no
   test covers is invisible to coverage — but so is a single such
   citation. That gap is the test check's job; the division of
   labor is unchanged.

The one composition gap, documented rather than plumbed: a project
that raises a cap and never runs coverage holds unbilled
duplicates. The report wording points at
`--check duplicates,coverage`.

---

## Decision 5: Coverage fails evidentially indistinct duplicates (W8) {#decision-5}

**Context:** Per-instance discharge (W6 + pair billing) is already
enforced by the coverage check for every duplicate copy — each is
an independent T. What existing correlation does **not** do is
compare copies *to each other*: two copies in one test function
both answer "do you have evidence?" by pointing at the same
evidence, and both pass. The current machinery interrogates each
annotation separately and never notices the evidence is one unit.

**Decision:** The coverage check fails any duplicate set
containing two members with identical bound witness sets:

```
indistinct(A, B)  ⟺  witnesses_for(A) = witnesses_for(B)
```

Property **W8 (pairwise distinctness):** coverage PASS requires
every same-type mutually-covering pair (A, B) of test annotations
to satisfy `witnesses_for(A) ≠ witnesses_for(B)`.

This is the governing principle applied per-instance: identical
witness sets mean the tool provably cannot tell the copies apart —
disambiguation did not happen in fact. The verdict is on the
**set**: coverage can prove the copies mutually vacuous but cannot
pick the canonical one; a human collapses them.

**Consequences:**

- The constructions this catches: N copies scattered through one
  test function (all bind that test's witnesses identically); two
  real tests under one merged suite report (both bind the single
  fat witness — see [Decision 6](#decision-6) for why failing this
  is correct); two test annotations inside one Verus discharge
  unit.
- The constructions this admits: proof + PBT (`{w_verus}` vs
  `{w_jacoco}`); two tests with per-test reports; two test
  annotations on two different `ensures` clauses of one Verus
  function (distinct clause units → distinct sets — the artifact
  records a span per clause, so cross-clause copies were never
  identical). The same-citation-on-two-ensures case is admitted
  deliberately: two separately recorded prover obligations are two
  units of evidence. Edgy; pinned by fixture; reopen if dogfooding
  says otherwise.
- **Robust to report duplication:** binding is content-determined
  (`executed_by` reads the coverage map), so delivering one report
  twice binds both copies identically for *every* annotation —
  `{w₁, w₂} = {w₁, w₂}`, still indistinct. Evidence cannot be
  photocopied into distinctness.
- Implementation surface is additive: the check already computes
  `bound_witnesses` per test; the new work is grouping duplicate
  classes and comparing sets, one new result bucket, one status
  conjunct. Per the engine-glue requirement, the W8 verdict flows
  through a verified-layer predicate (set equality over the
  existing `binds` cells) with an exactness proof: report groups
  A and B ⟺ `witnesses_for(A) = witnesses_for(B)`.
- Citation-side distinctness (comparing execution profiles of
  implementation copies) is deferred to a follow-up; tests are
  where the motivating case lives.

---

## Decision 6: Evidence units come from the artifact, never from inference {#decision-6}

**Context:** Under one merged runtime report, two genuinely
different tests carrying the same citation bind identical witness
sets and fail W8. Could duvet subdivide the report — e.g. by "the
test function the annotation sits in," using the classifier's
scope tree — to tell them apart?

### Option A: Scope-refined evidence units

Refine the runtime unit from "the report" to (report, executed
enclosing scope of the annotation's target).

- Con: requires language-specific scope classification plus
  walk-out-to-enclosing-test logic that does not exist, purchased
  per language, forever.
- Con: **the unit it defines is not the discharge unit.** A merged
  report is a line union; it cannot attribute a helper or
  implementation line to one test versus another, and without a
  call graph "test A's run" cannot even be delimited. The only
  fact scope refinement establishes is "the lines immediately
  inside this body lit up" — it distinguishes annotation
  *positions*, not evidence. A distinctness rule built on it
  proves the wrong thing.
- Con: contradicts the witness spec's stated posture: runtime
  individuation is the operator's responsibility ("one
  instrumented run per test… duvet does not attempt detection").

### Option B: Units are what the artifact records

Verus gets sub-root units because the prover artifact records a
span per ensures clause, loop invariant, and proof assert. JaCoCo
records a merged line union and nothing about how it was produced
— so the unit is the report, and finer units require finer
artifacts: one instrumented run per test.

### Decision: Option B. Scope refinement is rejected, not deferred.

**Consequences:** The fat-report W8 failure is designed incentive,
not accident: merged report → indistinguishable duplicates →
collapse them or individuate your runs. Coarse evidence buys fewer
allowed duplicates; finer evidence buys more. This is the
monotonicity invariant surfacing as an incentive, and it fails
toward strictness at every granularity.

---

## Decision 7: Executed-coverage mode carries no duplicate guarantees {#decision-7}

**Context:** `executed-coverage` skips `NotExecuted` tests by
design — it is the tight inner loop (run one test in seconds, not
the suite in minutes). Duplicate copies on tests that did not run
this invocation are unbilled there.

**Decision:** Out of scope, by intent. The mode is deliberately
partial for everything; duplicates inherit that partiality exactly
as broken-but-not-run tests do. The gate is the full coverage
check. (Unknown-status positions are still billed even in this
mode — placement errors fail regardless — so spray at positions
that resolve to nothing has no refuge here either.)

---

## Decision 8: Release posture {#decision-8}

**Context:** Executed (runtime) coverage has shipped; proof
coverage has not. W8 changes the coverage verdict on inputs
containing indistinct duplicate pairs — a loosened-then-tightened
release sequence would break released behavior.

**Decision:** W8 ships **in the same release as proof coverage**,
while the executed-coverage install base is effectively one user.
No compatibility flag: a `--no-duplicate-distinctness` switch is a
waiver-by-declaration, rejected on the same grounds as every other
waiver in this document.

**Consequences:** Enforcement invariance is stated honestly as
**P-C1′**: the new coverage verdict equals the old on every input
containing no indistinct duplicate pair; on inputs that do, new
coverage fails where old passed — and that delta is precisely the
defect class this feature defines. Both exits from the new failure
are improvements: collapse the copies (the report lists them) or
individuate the evidence.

A release-direction note, recorded because we got it wrong once
while designing this: for a verification gate, **loosening a rule
(fail→pass) is the dangerous direction** — it silently removes
protection a customer relied on to block bad states from
production. Tightening is visible; loosening is invisible until
the thing it would have caught ships. Any future loosening of W8
(e.g. reopening scope refinement) carries the same burden as
removing a check.

---

## Rejected: multi-path implementation relaxation {#rejected-multi-path}

**Considered:** Weakening coverage's per-test aggregation (a
witnessed test currently fails if *any* covering implementation
was not executed by its witnesses) to a duplicate-class-aware
form, admitting "one requirement genuinely enforced at two call
sites" (validate-the-header on both encrypt and decrypt paths).

**Rejected because the case dissolves under inspection:**

- If the requirement text appears in two spec sections (encrypt's
  and decrypt's), the two citations are not mutually covering —
  **not duplicates at all**; the check never fires.
- If both sites cite one section, duplicated enforcement logic is
  a factoring smell — hoist the shared validation and place one
  citation on it.
- If the same requirement text appears twice within one section,
  that is a malformed spec — a known, separate bug.

No surviving instance justified the semantic change. The
class-aware aggregation sketched during design is a conservative
extension (identical to current behavior on duplicate-free
projects), so rejecting now costs nothing if a real case ever
arrives; it gets its own decision then.

A useful consequence of keeping the strict aggregation, recorded
here: **call-graph spray is bounded by test granularity.** A
fine-grained unit test's witnesses cannot execute distant sprayed
copies, so every copy the test can't reach fails pair discharge.
The cap bounds spray's count; testing hygiene bounds its spatial
extent; decay (copies rotting into never-executed) is caught by
W6. Healthy spray only survives where testing is coarse.

---

## Properties {#properties}

Pinned by fixture unless noted; W8's predicate and exactness proof
live in the verified layer.

- **P-D1 (stacked-spam soundness).** duplicates PASS ⟹ no
  same-type mutually-covering pair shares a resolved target.
- **P-D2 (uniqueness of non-evidence types).** duplicates PASS ⟹
  every duplicate set of type spec/todo/exception/implication is a
  singleton.
- **P-D3 (legacy equivalence).** With all caps at 1, the
  duplicates verdict is identical to the historical check on all
  inputs. The existing regression fixtures are its test suite,
  unchanged.
- **P-D4 (cap monotonicity).** N ≤ N′ ⟹ (PASS at N ⟹ PASS at N′).
- **P-C1′ (scoped enforcement invariance).** New coverage verdict
  ≡ old on every input with no indistinct duplicate pair.
- **P-C2 (per-instance billing).** coverage PASS ⟹ every in-scope
  test annotation, duplicate or not, is witnessed and discharges
  against every covering implementation. (Already true — W6 plus
  the per-test aggregation; stated so deferral is visibly safe.)
- **W8 (pairwise distinctness).** coverage PASS ⟹ every same-type
  mutually-covering test-annotation pair has distinct bound
  witness sets. Verified-layer predicate; exactness: grouped ⟺
  sets equal.
- **P-S1 (no pass without evidence).** duplicates PASS ∧ coverage
  PASS ⟹ every duplicate copy resolves to a distinct target, every
  set is within cap, every copy is individually billed, and every
  pair is evidentially distinct.

## Follow-ups

- Integration fixtures: same-test-fn pair (fail W8); split-test
  pair with per-test reports (pass); fat-report distinct-tests
  pair (fail W8); duplicated-report pair (fail W8); Verus
  same-ensures pair (fail) and cross-ensures pair (pass);
  specificity-boundary binding (annotation on fn header vs inside
  clause spans); cap fixtures at N=1 (legacy parity) and N=2.
- Citation-side distinctness (execution profiles) — own follow-up.
- `--max-duplicates` CLI + config plumbing and help text.
- Implication duplicates — rider on self-discharging implications.
