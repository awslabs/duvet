# Duplicate Annotations — Decisions

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
spray door — and, once the space is stated formally, it turns out
"duplicate" names more than one thing, and each gets its own
answer.

## The formal space {#formal-space}

An annotation is a triple **(τ, q, t)**:

- **τ** — the claim form (type): `test`, `implementation`,
  `implication`, `exception`, `todo`, `spec`.
- **q** — the claim: a section plus quoted requirement text.
- **t** — the resolved target: the position the annotation
  resolves to (`resolve_target_line`), which is what binding
  scores.

There are exactly two coincidence relations — the two coordinates
on which annotations can collide:

- **Same claim** (D_Q): quotes mutually cover on the same section.
- **Same target** (D_T): same resolved target.

Type is deliberately **not** a third coincidence axis. Two
annotations do not duplicate each other *by* sharing a type. Type
determines the **price** of a claim — what evidence the checks
demand of it:

| Claim form | Price |
|---|---|
| `test` | must be witnessed; every bound witness must execute each covering implementation |
| `implementation` | must be executed under covering tests' witnesses |
| `implication`, `exception`, `todo`, `spec` | free — no checkable obligation |

Quotes and targets say *whether* annotations collide; type says
*what the collision means*. Every check in this document is a
predicate over the two partitions the coincidence relations
induce:

- **Claim classes** (annotations sharing a requirement):
  multiplicity caps, evidential distinctness, type coherence.
- **Target classes** (annotations sharing a position): fan-in
  bounds, mixed-form targets.

Two boundary conditions on the model, stated so nobody over-reads
it. First, "claim class" is well-defined only if mutual coverage
is an equivalence relation; that holds for exact/full coverage
and must be pinned against the code, and partial overlap (the
`some_overlap` bucket) remains its own pathology entirely outside
this framework. Second, an environmental fact about targets: code
formatters relocate comments, so annotations can land on lines the
author did not choose — same-target collisions can be tooling
artifacts.

One engine fact the type rules below depend on, stated as fact
because the checks implement it: **an `exception` or `implication`
covering a requirement exempts that requirement from the test
obligation** (the implementation and test checks in the query
engine). A free claim form does not merely coexist with a priced
one — it rewrites the obligations of the entire requirement.

## The governing principle

> **Duplicates are permissible exactly where a coverage check has
> the means to disambiguate them. Where no check can tell two
> copies apart, they must be unique.**

The corollary invariant, which every decision below preserves:

> **More information about annotations can justify a duplicate;
> less information never can. There is no path to PASS that does
> not go through evidence.**

A waiver marker (`duplicate-ok=reason`) was considered and
rejected at the outset: a declaration is exactly as cheap to spray
as the annotation it excuses. Under these decisions, duplicating
an annotation costs a distinct, passing verification artifact.

## Vocabulary

- **Duplicate set** — a maximal claim class restricted to one
  type: same-type annotations whose quotes mutually cover each
  other on the same section. Computed by the existing
  classification (`classify_annotation_coverage` of a type
  against itself).
- **Witness (w), binds(T, w), witnesses_for(T), discharged(T, I)**
  — as defined in `design/witness/spec.md` §1.5–1.6. In
  particular discharge quantifies **universally** over bound
  witnesses: every witness a test binds must execute the
  implementation. Witness multiplicity is a conjunction of
  obligations, not a tolerance — the model this document extends
  to annotation multiplicity.
- **Cap** — a configured per-type maximum duplicate-set size.

---

## Decision 1: Per-type caps are the sole enablement switch {#decision-1}

**Context:** How does a user opt into allowing duplicates, and how
is the legacy strict behavior preserved?

### Option A: A mode flag (allow-duplicates, old/new behavior versions)

- Con: a binary gate admits unbounded multiplicity the moment it
  opens.
- Con: mode flags fork the check's semantics permanently.

### Option B: Per-type numeric caps, default 1

A configured value per type, e.g. `test = 2`. Default for every
type is 1. Which types accept a raised cap, and where
configuration lives — file, flag, or both — are left open here.

- Pro: at cap 1 the check is bit-for-bit the historical strict
  check (Property [P-D3](#properties)). Nothing loosens until a
  user writes a bigger number, and that number is visible in
  review.
- Pro: the cap bounds proliferation even when every copy carries
  valid evidence — call-graph spray ("MUST foo" on the function,
  its caller, the caller's caller…) loses even when all sites
  execute.
- Pro: per-type control matches reality: the motivating case is
  test-type (`test = 2`, proof + PBT); implementation duplication
  has no accepted motivating case — the one candidate dissolves
  under inspection, recorded at the end of this document.

### Decision: Option B

**Consequences:** Raising a cap is an explicit, audited decision.
The duplicates report lists every allowed duplicate set with its
size and member locations, so multiplicity is always surfaced,
never silent. Cap semantics are monotone ([P-D4](#properties)):
raising N never flips a passing project to failing.

---

## Decision 2: Same claim, same target — always fails, for every type pair {#decision-2}

**Context:** Copies stacked at the same location: five identical
`//=` blocks above one function — or a `test` and an
`implementation` annotation carrying the same quote on the same
line.

### Option A: Same-type pairs only

Extend today's classification, which compares each type against
itself.

- Pro: no new classification machinery.
- Con: the argument for failing stacked copies —
  **indistinguishability** — never mentions types. Binding is a
  function of the resolved target, so two annotations at one
  target have identical witness sets by construction, whatever
  their types. Restricting to same-type would be an accident of
  the current implementation wearing a decision's clothes.
- Con: the cross-type stack is independently incoherent. "This
  artifact tests R" ∧ "this artifact implements R" at one position
  is the definition of an `implication` spelled out in two
  annotations; "we don't do R" ∧ anything-else-about-R is a
  contradiction wherever it sits.

### Option B: Fail for every type pair

Two annotations with the same claim and the same resolved target
FAIL unconditionally, regardless of cap and regardless of type.

### Decision: Option B

**Consequences:** "Same location" is defined as **same resolved
target**, not same
source line: stacked copies above one statement collapse to one
target and die here; copies scattered through a function body
resolve to different targets and are governed by the cap and by
the evidential rule stated later — indistinguishability decidable
only once evidence arrives. This decision is that rule's
statically decidable fragment.

---

## Decision 3: Per-type permissibility {#decision-3}

**Context:** [Decision 1](#decision-1) established the cap
mechanism and left open which types accept a raised cap. The
question conflates two things that separate cleanly: what the
evidence can support, and what a customer may configure.

What the evidence supports is the pricing table's answer — a
raised cap is meaningful exactly where a coverage check has some
perspective on each copy:

| Type | Coverage-check perspective |
|---|---|
| `test` | witnessed + discharged (W6, §1.6) |
| `implementation` | executed under covering tests |
| `spec` | none |
| `todo` | none — and a todo-covered requirement already fails the implementation check |
| `exception` | none — a declaration of absence has nothing to execute or prove |
| `implication` | none **today**; the arm-wrestle below |

### Option A: Every cap raisable, default 1

- Pro: symmetric, and never refuses a customer's configuration.
- Con: for the evidence-free forms, a raised cap does not admit
  verified duplicates — it admits copies no check can ever
  distinguish, which is exactly the duplication the governing
  principle forbids, now blessed by a config line.
- Con: the shipping asymmetry. A knob that never shipped can be
  added the day a customer arrives with a real case; a shipped
  knob can never be removed without a breaking change. Frivolous
  knobs are permanent.

### Option B: Caps raisable only where priced

`test` and `implementation` get raisable caps. `spec`, `todo`,
`exception`, and `implication` are unique with no knob — a
shipping decision, not a metaphysical constraint. If a real case
arrives, the knob is added then.

### Decision: Option B

**Implication was the arm-wrestle in the table.** Two readings
were debated:

**Reading 1 — intrinsically unique, like `exception`.** Rejected
because it mistakes the type: an exception's position is
incidental (a declaration of absence); an implication's position
is the claim ("*this artifact* is correct by construction").
Structurally it patterns with `implementation`, not `exception`.

**Reading 2 — unique today, contingent on future evidence.** What
forces uniqueness today is the evidence question, not the type's
nature: implications carry no checkable obligation, and the
proof-world answer (self-discharging implications, where a
verifier's elaboration is the implication's own evidence) was
deliberately deferred in the witness design ("nobody can state
the discharge obligation precisely yet"). Runtime execution of an
implication's position was considered as interim evidence and
rejected: it is evidence about the wrong property — liveness, not
construction.

Reading 2 stands. Revisit as a rider on the
self-discharging-implications feature, which owns its own
decision document.

---

## Decision 4: Free claim forms are exclusive within a claim class {#decision-4}

**Context:** [Decision 2](#decision-2) handles mixed types at one
position. What about mixed types across positions — an `exception`
and a `test` for the same requirement, an `implication` alongside
an `implementation`? Today the duplicates check is same-type-only
and never sees these coexistences.

The engine fact from [the formal space](#formal-space) makes this
urgent rather than cosmetic: an `exception` or `implication`
exempts its requirement from the test obligation. So a free form
added *next to* an existing priced form silently deflates the
priced form's obligations — an LLM (or a tired human) that learns
"tests need witnesses and implementations must execute" also
learns that `implication` requires nothing. Type-shopping is the
gate-evasion strategy the pricing table predicts, and the free
forms are its escape valve. A per-target rule cannot catch it: an
`exception`'s position is incidental, so it evades any co-location
check by sitting anywhere else.

### Option A: Leave it alone (today's behavior)

- Con: the exploit is live. Adding an `implication` next to an
  existing `implementation` annotation silently deflates the
  requirement's obligations, and nothing reports the coexistence.

### Option B: A pairwise contradiction matrix

Enumerate the type pairs and assign each a verdict
(`exception`+`test` = contradiction, `implication`+`test` =
incoherent, …).

- Pro: each verdict is individually explainable.
- Con: every cell needs its own metaphysical justification, argued
  pair by pair; a new annotation type multiplies the cells; and
  the matrix invites cell-by-cell relitigating.

### Option C: One exclusivity rule over the type multiset

> If a claim class contains an `exception`, `implication`, or
> `todo` annotation, that annotation must be the **only** member
> of the class. Any combination of `test` and `implementation`
> members is permitted, each billed on its own.

- Pro: the entire matrix falls out as derived consequences, with
  one uniform rationale: **a free form asserts something about the
  requirement's nature that changes everyone's obligations, so it
  does not get to share the claim with forms whose obligations it
  would deflate.**
- Pro: a future annotation type gets a verdict by answering one
  question — is it priced? — instead of a row of debates.

### Decision: Option C

Checking the rule against every pair the matrix would have argued:

| Coexistence (same claim) | Reading | Verdict under the rule |
|---|---|---|
| `exception` + `test` | "we don't do R" ∧ "we test R" | fail — contradiction |
| `exception` + `implementation` | "we don't do R" ∧ "R is implemented here" | fail — contradiction |
| `implication` + `implementation` | "R holds by construction" ∧ "R is enforced here, bill it" | fail — the exemption would silently unbill the implementation |
| `implication` + `test` | "not checkable by evidence" ∧ "tested" | fail — incompatible claims about R's nature |
| `todo` + `implementation` | "not yet done" ∧ "done here" | fail — and todo→implementation is the intended *lifecycle*: the todo is replaced, not joined; exclusivity enforces exactly that |
| `test` + `implementation` (different targets) | tested there, implemented here | permitted — this is `discharged(T, I)`, the system itself |

**Consequences:** This is a static check over claim classes — no
evidence needed; it belongs to the duplicates check. It is a
deliberate tightening of released behavior: cross-type
coexistence on one requirement silently passes today. How the
tightening ships, and how a project that believes it has a
legitimate coexistence records that belief, are settled below once
all policy values are on the table — the short answer is a
reviewed config line, never a comment.

---

## Decision 5: No inter-check plumbing — independent contracts {#decision-5}

**Context:** When the duplicates check passes a within-cap
duplicate set, how does the evidence obligation reach the coverage
check?

### Option A: Deferred-status plumbing

The duplicates check emits `deferred` items; the query engine
fails them if no coverage check runs in the same invocation.

- Con: couples the checks; a check's verdict depends on which
  other checks were requested.
- Con: complicates composition for no guarantee the independent
  design doesn't already provide.

### Option B: Independent contracts

The duplicates check enforces structure only (Decisions 1–4): run
alone, a within-cap duplicate set **passes**, with its size and
members surfaced in the report. The coverage check enforces
evidence natively and unconditionally — whether or not the
duplicates check ever runs (the next decision states the
evidential rule). Run the full report and the composition just
works; run one check and you get exactly that check's guarantee.

### Decision: Option B

**Consequences:** The domination claim that makes this safe, with
its two scoping conditions stated so nobody over-reads it:

1. For **test** duplicates, the full coverage check alone
   dominates: every copy is an independent T — unwitnessed → W6
   fail; witnessed → per-pair ∀w billing; evidentially identical →
   the distinctness failure defined next.
2. For **implementation** duplicates, coverage dominates the
   *duplicate-specific* badness only. An implementation annotation
   on a quote no test covers is invisible to coverage — but so is
   a single such annotation. That gap is the test check's job; the
   division of labor is unchanged.

The one composition gap, documented rather than plumbed: a project
that raises a cap and never runs coverage holds unbilled
duplicates. The report wording points at running both checks.

---

## Decision 6: Coverage fails evidentially indistinct duplicates (W8) {#decision-6}

**Context:** A project raises `test = 2`, and a requirement now
carries two test annotations in two places. What the coverage
check does with them today: it binds each annotation to its
witness set — the concrete units of evidence, an instrumented
test run, a proved clause — and verifies each annotation
independently: witnessed? discharged against every covering
implementation? Both copies can answer both questions by pointing
at the same evidence, and both pass, because nothing ever
compares the copies *to each other*. Per-instance discharge (W6
plus pair billing) is already enforced for every copy; what no
existing machinery asks is whether the two copies are two claims
or one claim written twice.

One question decides it: **do the copies bind the same witness
set?** Everything duvet can check about a test annotation — in
this check or any future one — is a function of its witness set.
If two copies bind exactly equal sets, no check can ever tell
them apart; the second copy adds no information. The governing
principle says duplicates are permissible exactly where coverage
can disambiguate them, and equal witness sets is precisely where
it cannot.

### Option A: Report the groups, never change the verdict

Surface "these copies are evidentially indistinguishable" as an
informational grouping — candidates for collapsing, no failure.
This option was briefly adopted during design, out of sympathy
for contained multiplicity ("silly but not intrinsically wrong").

- Con: it quietly guts the design's central claim. If
  indistinguishable copies never fail, coverage resolves only
  *rot* (unwitnessed, undischarged copies), not *redundancy* —
  and "the duplicates check can relax because coverage
  disambiguates the copies" stops being true. The grouping
  becomes a caption on a pass.
- Con: the governing principle decides this case directly.
  Allowing indistinguishable copies is not tolerance; it is the
  principle's own failure condition.

### Option B: Fail the indistinguishable groups

Within each claim class, group the same-type test members by
their bound witness set. **Every group of size ≥ 2 fails** —
those members are mutually indistinguishable. Singleton groups
pass.

### Decision: Option B

Property **W8 (evidential distinctness):** coverage PASS requires
that no two same-type mutually-covering test annotations bind the
same witness set.

**How you hit it in practice.** Three ways:

- Copy-paste: N copies scattered through one test function all
  bind that test's witnesses identically. Fails; collapse them.
- One merged suite report: two *genuinely different* tests run as
  a single instrumented invocation produce one fat witness, and
  both annotations bind it. Fails — and this is the case that
  feels like a false positive, because the tests really are
  different. But the evidence handed to duvet cannot see the
  difference, and duvet does not guess at distinctions its
  evidence cannot support. Two exits, both improvements: collapse
  the copies, or individuate the runs (per-test reports) so the
  sets differ. Coarser evidence buys fewer allowed duplicates —
  the monotonicity invariant surfacing as an incentive.
- Two test annotations inside one Verus discharge unit bind the
  same clause span. Fails. Copies on two *different* `ensures`
  clauses of one function pass — the prover artifact records a
  span per clause, so the sets were never equal. Two separately
  recorded prover obligations are two units of evidence; edgy,
  pinned by fixture, reopened if dogfooding says otherwise.

The constructions this admits are the motivating ones: proof +
PBT (`{w_verus}` vs `{w_jacoco}`), split tests with per-test
reports.

**There are no false positives in the strict sense.** Equal sets
are computed from the delivered evidence, not estimated; when the
rule fires, the indistinguishability is a fact. The unfair
*feeling* is always the merged-report case — evidence coarser
than reality — and the remediation message says so.

The formulation carries two deliberate precisions, both
arm-wrestled:

- **Distinct means unequal, never disjoint.** An earlier
  "pairwise distinct sets" phrasing invited misreading as
  disjointness, and disjointness would fail legitimate structure:
  `{extent, e₁} ≠ {extent, e₂}` passes despite the shared
  witness.
- **The verdict attaches to the indistinguishable group, not the
  class.** If A and B bind identically but C binds differently, A
  and B fail together and C is untouched. Coverage can prove the
  group mutually vacuous but cannot pick the canonical member; a
  human collapses it.

**Consequences:**

- [Decision 2](#decision-2)'s same-target rule is exactly this
  rule's statically decidable shadow: same target implies
  identical sets by construction, but not conversely — positions
  that always execute together are positionally distinct and
  evidentially identical, and only this rule sees them.
- **Robust to report duplication:** binding is content-determined
  (`executed_by` reads the coverage map), so delivering one
  report twice binds both copies identically for *every*
  annotation — `{w₁, w₂} = {w₁, w₂}`, still indistinct. Evidence
  cannot be photocopied into distinctness.
- Implementation surface is additive: the check already computes
  `bound_witnesses` per test; the new work is grouping by witness
  set, one new result bucket, one status conjunct — linear, no
  pairwise walk. Per the engine-glue requirement, the W8 verdict
  flows through a verified-layer predicate (set equality over the
  existing `binds` cells) with an exactness proof: members
  grouped ⟺ witness sets equal.
- Implementation-side distinctness (comparing execution profiles
  of implementation copies) is deferred to a follow-up; tests are
  where the motivating case lives.

---

## Decision 7: Evidence units come from the artifact, never from inference {#decision-7}

**Context:** Under one merged runtime report, two genuinely
different tests carrying the same claim bind identical witness
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

The same principle closes a tempting idea in the other direction,
which returns when the document reaches the target axis: within
one evidence unit there is **no execution gradient** — an
annotation on a function head and an annotation on the exact
enforcing line inside it bind identically under every witness that
runs the function. No evidence-side analysis can grade *placement
precision*; only the artifact's own structure (prover clause
spans) ever subdivides a unit.

---

## Decision 8: Executed-coverage mode carries no duplicate guarantees {#decision-8}

**Context:** `executed-coverage` skips `NotExecuted` tests by
design — it is the tight inner loop (run one test in seconds, not
the suite in minutes). Duplicate copies on tests that did not run
this invocation are unbilled there.

### Option A: A narrow tightening for the mode

When some members of a duplicate set executed and grouped
vacuously, fail the set even in executed mode.

- Con: it designs duplicate machinery into a debug loop. The
  mode's entire purpose is deliberate partiality — run one test in
  seconds without false positives from everything else. Building
  duplicate guarantees into it confuses the intentionalities.

### Option B: Out of scope, by intent

The mode is deliberately partial for everything; duplicates
inherit that partiality exactly as broken-but-not-run tests do.
The gate is the full coverage check.

### Decision: Option B

**Consequences:** Unknown-status positions are still billed even
in this mode — placement errors fail regardless — so spray at
positions that resolve to nothing has no refuge here either.

---

## Decision 9: Duplicate targets — the fan-in axis {#decision-9}

**Context:** Everything above polices coincidence of **claims**.
The formal space has a second coordinate, and it collides too:
many annotations with *different* claims all resolving to one
target — four, five, twelve annotations stacked on a single
function. Seen frequently in real usage, characteristically from
LLM sessions. The full matrix:

| | same target | different targets |
|---|---|---|
| same claim | dead — [Decision 2](#decision-2) | duplicate set — cap + W8 |
| different claims | **duplicate target (this decision)** | normal |

This is duplication of the *other* coordinate — not a duplicated
annotation but a **duplicated target**: fan-in where the cap
bounds fan-out. The duality is exact: a claim with many targets is
one thing said in many places; a target with many claims is many
things said in one place.

**Why it is not a correctness failure.** The evidence layer does
not collapse here the way it does for true duplicates: stacked
annotations share witness sets, but each claim's discharge runs
through *different* covering pairs, so every claim is individually
billed. The governing principle does not force a failure — these
claims are disambiguated, by quote and by pair verdict. And the
legitimate population is large: parsers and validators genuinely
implementing many MUSTs from one section are real, common code.

**Why it still smells.** The position is doing no work. The
annotation's position *is* part of its claim, and twelve claims on
one target mean position discriminates nothing — the stack is
informationally one comment saying "this function does §3." Costs:
lazy placement (citing the function head instead of the enforcing
lines, losing per-claim precision — on the prover side, the
difference between binding the extent and binding the clause
unit); rot amplification (a refactor re-homes twelve claims as one
careless block); and annotation dumping — spray rotated ninety
degrees. Per [Decision 7](#decision-7), no evidence-side analysis
can distinguish a well-placed stack from a lazy one within a unit,
so the tools here are static, and their bounds are policy rather
than correctness.

An asymmetry recorded for the follow-up on diagnostics: test-side
fan-in resolves cleanly through per-pair billing. Implementation-
side fan-in is the commonly observed shape, and a dumped
implementation annotation has a stranger property: it forces every
test citing that requirement to execute the dumped-on function, so
**the dump surfaces as failures on innocent test annotations, far
from the cause**. The evidence layer pushes back, but with poor
diagnostic locality.

### Option A: A gate with a default threshold

Fail targets bearing more than N annotations, N shipped with a
biting default.

- Con: no defensible *intermediate* N exists — 3 and 5 are
  arbitrary in a way 1 is not. A middle value fails some
  legitimately dense code while missing some stack it was meant
  to catch; count is a signal, not a discriminator. N = 1 is
  different in kind: "one target, one annotation" is not a
  threshold but the position-must-discriminate principle itself —
  [Decision 1](#decision-1)'s strict-by-default posture on this
  axis. Whether it is shippable as a default turns on how large
  the legitimately dense population actually is; an empirical
  question, taken up under Option C.
- Con: unlike every gate above, no evidence property is violated —
  each stacked claim is billed and distinguishable. Failing code
  the evidence supports would be the first rule in this document
  to do so.

### Option B: Disambiguate lazy stacks from dense code with evidence

Use the execution data to tell a lazily-stacked function head from
a precisely-annotated dense function.

- Con: impossible within a unit, per [Decision 7](#decision-7) —
  no execution gradient exists. (A related evidence-side idea
  survives as a parked observation at the end of this document.)

### Option C: Query first, snapshot ratchet, opt-in bounds

1. **A query, not (by default) a gate.** A `duplicate-targets`
   query lists every target bearing more than one annotation,
   sorted by count descending, with per-target type breakdown and
   distinct-section count. No verdict — a sorted list needs no
   threshold and no false-positive story; the human reads the top.
   Even inspection output owes an exactness obligation: a target
   appears iff ≥ 2 annotations resolve to it, with exact counts
   (P-T1 in the property list). The section count is a triage
   column, not a discriminator: `count=12 sections=1` reads "dense
   but coherent"; `count=12 sections=8` reads "look here first."
2. **Snapshot inclusion is the ratchet.** When the table lands in
   the snapshot, the PR that adds a sixth annotation to a crowded
   target surfaces as a snapshot diff in review — concentration
   becomes visible at the moment it grows, with no gate. This is
   the same posture as "raising a cap is visible in review."
3. **Opt-in bounds for teams that want the bite**: `count` (max
   annotations per target), `sections` (max distinct sections per
   target), `types` (distinct claim forms per target). `sections`
   and `types` default unlimited. The shipped default for `count`
   is deliberately left open between unlimited and 1: `count = 1`
   is [Decision 1](#decision-1)'s posture on this axis — strict
   by default, raised explicitly where density is real — but it
   fails the legitimately dense population on day one if that
   population is large. How large is measurable: survey the
   public duvet corpus (s2n-quic, s2n-tls, the AWS crypto
   tooling) for actual target fan-in; the default follows the
   data, and the survey is a follow-up. These are the first
   bounds in this document that fail code for a *smell* rather
   than a violated evidence property — the entry price of the
   fan-in axis, named honestly: teams opt into the opinion.

### Decision: Option C

**Mixed claim forms on one target** deserve their own default. A
position claimed as a test of R1 and an implementation of R2 means
one artifact is simultaneously test code and implementation code —
disjoint populations in every codebase we care about. The one
honest construction is the meta-requirement ("implementations MUST
validate the test vectors in Appendix A" — the test function that
runs them *is* that requirement's implementation while *testing*
the object-level ones). Even there, nothing requires target
*identity*: the reviewer wants the two annotations close, and
close is not identical. Default: `test`+`implementation` on one
target fails; the Appendix-A team relaxes that default and owns
the decision in review. Where such relaxations are recorded — and
where all the policy values this document has accumulated live —
is the next decision.

---

## Decision 10: Policy lives in configuration, not on the command line {#decision-10}

**Context:** The decisions above accumulate policy values — caps
(Decision 1), coherence cells (Decision 4), fan-in bounds and the
mixed-form default (Decision 9). Where do they live?

### Option A: CLI flags, possibly mirrored in config

- Con: a verification gate whose semantics vary per invocation is
  not a gate. Two people running the same commit could get
  different verdicts; CI and local runs drift.
- Con: the project's effective policy is reconstructable only from
  scripts, not readable from the tree.

### Option B: Configuration file only

All policy is checked-in configuration. The shipped defaults
encode this document's verdicts; a project overrides individual
cells; omitted cells take the defaults. (The key vocabulary the
policy is written in — what the settings are named and how the
file is structured — is left open here and settled two decisions
below, once the naming question is also on the table.)

### Decision: Option B

The principle: **the verdict is a function of the checked-in tree,
not of the invocation.** The command line selects which checks
run, points at evidence artifacts (inputs, not semantics), and
controls verbosity — display, never verdict.

**Consequences:**

- The discovery loop replaces flag experimentation: run the check
  verbose, see every group, every default, every below-threshold
  case; write the numbers you want into config; from then on the
  run is silent about everything you have accepted.
- Cell overrides are **rule-level policy declarations**, audited
  in review — a different thing from the per-instance waiver
  rejected in the governing principle. That door stays closed.
- Default changes become survivable: when a shipped default
  flips, release notes say "pin the old cell to keep prior
  behavior" — one config line, and the compatibility question gets
  a structural answer instead of a per-feature negotiation.
- The whole effective policy of a project is one readable section
  of one checked-in file.

---

## Decision 11: User-facing vocabulary — `implementation`, not `citation` {#decision-11}

**Context:** The code names the default annotation type
`citation`. Every user-facing surface the preceding decisions add
— config keys, report sections, failure messages — needs one name
for it, and this document has already been using `implementation`
throughout. Which name is canonical, and what happens to the
other?

### Option A: `citation` everywhere

- Con: it names the mechanics (the annotation quotes the spec),
  not the claim ("R is implemented here"). Every rule in this
  document prices the claim; the pricing table's rows read
  strangely against a name that describes quoting.

### Option B: Rename the code

- Con: eviscerates the codebase for a naming preference — every
  touched line is review burden with zero behavior change.

### Option C: Canonical user-facing name, alias at the parse boundary

`implementation` is the one name emitted anywhere a user reads:
config keys, reports, messages. `citation` remains accepted on
input — existing annotations keep parsing, forever — and is never
emitted. Internal code keeps its vocabulary.

### Decision: Option C

**Consequences:** One canonical output name prevents the drift
that two-names-for-one-thing invites. The rename never touches the
enum. The scope is the surfaces this document adds and new
user-facing output going forward — not a license to retrofit
existing code or existing output; any retroactive cleanup is its
own change.

---

## Decision 12: One config namespace per coincidence axis {#decision-12}

**Context:** [Decision 10](#decision-10) settled where policy
lives; its sketch wrote the keys flat. This settles the vocabulary
the policy is written in.

### Option A: Flat keys

`test`, `count`, `sections` side by side under `[duplicates]`.

- Con: `count` is ambiguous between the two partitions — "how many
  annotations may share a claim" versus "how many may share a
  target" — and the ambiguity is not hypothetical: it was
  committed during this design, in a draft that wrote `count`
  meaning fan-in where a reader had every reason to read
  multiplicity.

### Option B: Disambiguating key prefixes

`max-duplicates.test`, `max-target.count`.

- Pro: unambiguous.
- Con: a naming convention carries the structure instead of the
  structure carrying it. The formal space has exactly two
  partitions; the config file can mirror the model instead of
  encoding it into hyphenated names.

### Option C: Two namespaces mirroring the two partitions

```toml
[duplicates.claims]        # predicates over claim classes (D_Q)
test = 2                   # per-type multiplicity caps (Decision 1)
implementation = 1
# coherence-cell overrides (Decision 4)

[duplicates.targets]       # predicates over target classes (D_T)
count = 3                  # fan-in bound (Decision 9)
sections = 2               # distinct sections per target
# allowed type combinations: next decision
```

Every setting lives in exactly one namespace; no setting name
appears in both. "What does this key constrain" is answered by
position in the file.

### Decision: Option C

**Consequences:** Both priced types sit on equal footing under
`claims` — `implementation` caps are as configurable as `test`
caps, with no privileged case. Whether a team wants duplicate
implementations is the team's call, made in review by writing the
number. [Decision 3](#decision-3)'s verdicts on the free forms are
unaffected: those are not caps, and remain unique always
([P-D2](#properties)).

---

## Decision 13: Target type combinations are an allowed-set family {#decision-13}

**Context:** [Decision 9](#decision-9) named a `types` bound (max
distinct claim forms per target) and a mixed-form default
(`test`+`implementation` on one target fails). Stated as a number,
the bound is expressive only at 1.

### Option A: Numeric bound

- Con: `types = 2` says "any two forms may share a target" — it
  permits exactly the combination the mixed-form default forbids,
  and cannot say "exception+test is fine, test+implementation is
  not." The one distinction the axis exists to draw is the one a
  count cannot express, so the mixed-form default would survive as
  special-case machinery beside the bound.

### Option B: Pairwise cells

A verdict per type pair, like [Decision 4](#decision-4) Option B.

- Con: rejected there for the same reasons that apply here —
  per-cell metaphysics, invitation to relitigate, and every new
  annotation type multiplies the cells.

### Option C: A family of allowed type-sets

Config declares which *combinations* may share a target. Each
array element is one allowed set, `+` joining the types in it. A
target class G bearing two or more distinct types passes iff
`types(G) ⊆ S` for some allowed S. A single-type target always
passes — one type is not a combination, so a rule about sharing
never constrains it (singleton entries are legal but redundant):

```toml
[duplicates.targets]
types = ["exception+test", "implication+todo"]
# exception+test on one target: allowed
# implication+todo:             allowed
# exception+todo:               fails — inside no allowed set
```

`types = []` forbids all mixing; omitting the setting allows
every combination (the opt-in posture of this axis).

- Pro: downward-closed by construction — a subset of an allowed
  combination is always allowed, so permitting a combination never
  forbids its parts, and a lone annotation is never rejected by a
  rule about sharing.
- Pro: subsumes the numeric form — `types = 1` is `types = []`.
  One primitive instead of two.
- Pro: the mixed-form default stops being special-case machinery:
  the shipped default family is every combination **not**
  containing both `test` and `implementation`. The Appendix-A team
  adds one set to the family and owns it in review.
- Pro: it is [Decision 4](#decision-4)'s shape — allowed type
  combinations — transplanted to the other partition. The model's
  symmetry, showing up in the config.

### Decision: Option C

**Consequences:** [P-D4](#properties) extends to families: adding
an allowed set never flips a passing project to failing; removing
one never flips failing to passing. Exactness is
[P-T2](#properties).

---

## Decision 14: Scoped policy overrides — designed, deferred {#decision-14}

**Context:** Every policy value so far is global — one value per
setting per project. The legitimate-density population from
[Decision 9](#decision-9) invites finer grain: the packet-format
section genuinely hosts twelve MUSTs; the parser file genuinely
hosts fan-in. Today the only relief is loosening the global bound,
which surrenders the check everywhere to accommodate one place. Is
there a scoping scheme that stays policy — a rule about a class —
without becoming the per-instance waiver this document rejected at
the outset?

### Option A: Section scoping for everything

Overrides keyed by `spec.md#section-id`, applying to every
predicate.

- Con: ill-typed for target predicates. A claim class has exactly
  one section (its quote's); a target class has none — it may host
  annotations from eight sections and two specs. This was
  committed during design: a draft override scoped `count`
  (fan-in) under a section key, the precise coordinate a target
  class lacks.
- Con: the `sections` bound becomes circular under section scoping
  — bounding distinct-sections-per-target with a bound that itself
  varies by section.

### Option B: Strictest applicable bound

Where a target hosts annotations from several sections, the
strictest configured bound governs.

- Con: action-at-a-distance. Importing one quote from a stricter
  section onto an existing stack silently changes which bound
  governs the whole target — the effective bound moves under
  exactly the mutations pull requests contain.

### Option C: Most-permissive applicable bound

- Con: laundering. Route one annotation from the loose section
  onto any target and its bound rises — type-shopping in scope
  clothing. Rejected outright.

### Option D: Axis-matched scoping

Each predicate family scopes by the coordinate system its
equivalence classes live in; most-specific wins; unset scopes
inherit:

- Claim predicates: global → spec → section, keyed in the same
  `spec.md#section-id` form annotations use — same resolver, and a
  key naming a nonexistent section fails like a bad annotation
  reference.
- Target predicates: global → file path, keyed by the target's
  own address.

```toml
[duplicates.claims."rfc9000.md#packet-format"]
test = 3

[duplicates.targets."src/parser.rs"]
count = 15
```

- Pro: each override names its blast radius in its own key.
- Pro: bounds are stable under annotation arrival — a target's
  bound is a function of its address, and no arriving annotation
  from any section can change which bound applies.
- Pro: still rule-level. A section or a file is a class; the
  override is an audited statement about it, not an excuse for an
  instance. The waiver door stays closed. The granularity ladder
  is global → spec → section → per-annotation (rejected), and
  scope stops at the finest grain that is still a rule.
- Pro: the dense parser is bounded at its own address; the global
  stays tight.

### Decision: Option D is the design; shipping it is deferred

Two reasons to wait, recorded so the deferral is a decision and
not a drift. First, target-side scoping will immediately be asked
to take globs — one file is rarely the real class, `src/parser/**`
is — and glob keys need an overlap-precedence rule
(`src/parser.rs` vs `src/**`) that is real, unresolved design
surface. Second, no demonstrated need yet: global bounds plus the
discovery loop may be enough, and adding scope resolution to the
policy model before a user asks buys complexity without a
customer.

If it ships, it owes three properties, stated now so the future
decision inherits them: **resolution determinism** (the effective
value is a pure function of config plus the class's coordinates —
no key-order dependence), **unconfigured equivalence** (a config
with no scoped keys behaves identically to global-only), and
**shadowing monotonicity** (a scoped key changes verdicts only
inside its scope; nothing outside leaks).

---

## Decision 15: Release posture {#decision-15}

**Context:** Executed (runtime) coverage has shipped; proof
coverage has not. Two behavior changes are on the table: W8
([Decision 6](#decision-6)) changes the coverage verdict on inputs
containing indistinct duplicate pairs, and type coherence
([Decision 4](#decision-4)) makes the duplicates check fail
cross-type coexistences that silently pass today.

### Option A: A compatibility flag

Ship the new semantics behind `--no-distinctness` /
`--legacy-coherence` switches.

- Con: a waiver-by-declaration, rejected on the same grounds as
  every other waiver in this document — and a permanent escape
  hatch enters the interface for a transitional problem.

### Option B: Staged rollout — warn one release, enforce the next

- Pro: nobody gets ambushed; the warning release lists the exact
  collapse/split remediation.
- Con: unnecessary given the facts. It was designed under the
  assumption of a broad executed-coverage install base; the actual
  base is effectively one user, and proof coverage has not shipped
  at all.

### Option C: Ship strict with proof coverage; tightened defaults are pinnable

W8 ships **in the same release as proof coverage** — the check
simply always had these semantics; no lenient version ever
existed to be compatible with. Type coherence ships as a
tightened default with its cells pinnable per
[Decision 10](#decision-10) — the release notes name the cell
that restores prior behavior.

### Decision: Option C

**Consequences:** Enforcement invariance is stated honestly as
**P-C1′**: the new coverage verdict equals the old on every input
containing no indistinct duplicate pair; on inputs that do, new
coverage fails where old passed — and that delta is precisely the
defect class this feature defines. Both exits from the new failure
are improvements: collapse the copies (the report lists them) or
individuate the evidence. Legacy equivalence for the duplicates
check ([P-D3](#properties)) is likewise scoped: caps at 1 *and*
coherence cells at their pre-existing effective values reproduce
the historical verdict; the shipped coherence defaults are a
deliberate tightening.

A release-direction note, recorded because we got it wrong once
while designing this: for a verification gate, **loosening a rule
(fail→pass) is the dangerous direction** — it silently removes
protection a customer relied on to block bad states from
production. Tightening is visible; loosening is invisible until
the thing it would have caught ships. Any future loosening —
reopening scope refinement, relaxing a coherence cell's shipped
default — carries the same burden as removing a check.

---

## Rejected: multi-path implementation relaxation {#rejected-multi-path}

**Considered:** Weakening coverage's per-test aggregation (a
witnessed test currently fails if *any* covering implementation
was not executed by its witnesses) to a duplicate-class-aware
form, admitting "one requirement genuinely enforced at two call
sites" (validate-the-header on both encrypt and decrypt paths).

**Rejected because the case dissolves under inspection:**

- If the requirement text appears in two spec sections (encrypt's
  and decrypt's), the two annotations are not mutually covering —
  **not duplicates at all**; the check never fires.
- If both sites cite one section, duplicated enforcement logic is
  a factoring smell — hoist the shared validation and place one
  annotation on it.
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

## Considered: the evidence transpose — parked {#considered-transpose}

**Considered:** Reading the discharge matrix column-wise — per
implementation site, the set of witnesses that execute it. A site
executed by *every* delivered witness has a constant column:
`discharged(T, I)` is then trivially satisfiable by any test, so
the claim "R is implemented here" is unfalsifiable by execution
evidence. This would flag **hot-path dumping** — an implementation
annotation placed on always-executed code (setup, dispatch,
`main`) discharges against everything and evades both the static
fan-in view and pair billing.

**Parked, not adopted:** the transpose cannot serve the placement
question that motivated it — within one evidence unit there is no
execution gradient ([Decision 7](#decision-7)), so a constant
column cannot distinguish a lazily-placed annotation from a
precisely-placed one on a genuinely hot path (a requirement really
enforced on the universal request path legitimately executes under
everything). What it *does* measure — which discharge verdicts are
load-bearing and which are vacuously true — is a different
question, worth revisiting if hot-path dumping is observed in
practice. Cost when wanted: a per-site aggregation over cells
coverage already computes.

---

## Properties {#properties}

Pinned by fixture unless noted; W8's predicate and exactness proof
live in the verified layer.

- **P-D1 (stacked-spam soundness).** duplicates PASS ⟹ no
  mutually-covering pair — any types — shares a resolved target.
- **P-D2 (uniqueness of non-evidence types).** duplicates PASS ⟹
  every duplicate set of type spec/todo/exception/implication is a
  singleton.
- **P-D3 (scoped legacy equivalence).** With all caps at 1 and
  coherence cells at pre-existing effective values, the duplicates
  verdict is identical to the historical check on all inputs. The
  existing regression fixtures are its test suite, unchanged.
- **P-D4 (bound monotonicity).** For every configured bound (caps,
  fan-in bounds): raising it never flips a passing project to
  failing, lowering it never flips a failing project to passing.
- **P-D5 (free-form exclusivity).** duplicates PASS ⟹ every claim
  class containing an exception, implication, or todo annotation
  is a singleton.
- **P-T1 (fan-in exactness).** The duplicate-targets listing
  contains a target iff ≥ 2 annotations resolve to it; counts,
  type breakdowns, and section counts are exact.
- **P-T2 (type-family exactness).** With a configured family, a
  target class bearing two or more distinct types fails the
  type-combination rule iff its type set is a subset of no allowed
  set; single-type targets never fail it. Adding an allowed set
  never flips a passing project to failing; removing one never
  flips failing to passing (the family form of P-D4).
- **P-C1′ (scoped enforcement invariance).** New coverage verdict
  ≡ old on every input with no indistinct duplicate pair.
- **P-C2 (per-instance billing).** coverage PASS ⟹ every in-scope
  test annotation, duplicate or not, is witnessed and discharges
  against every covering implementation. (Already true — W6 plus
  the per-test aggregation; stated so independence is visibly
  safe.)
- **W8 (evidential distinctness).** coverage PASS ⟹ within every
  claim class, no witness-set group of same-type test members has
  size ≥ 2. Verified-layer predicate; exactness: grouped ⟺ sets
  equal. Distinct means unequal, not disjoint.
- **P-S0 (verdict determinism).** For a fixed set of checks and
  evidence inputs, the verdict is a function of the checked-in
  tree (source + config). Verbosity never changes a verdict.
- **P-S1 (no pass without evidence).** duplicates PASS ∧ coverage
  PASS ⟹ every duplicate copy resolves to a distinct target, every
  set is within cap, every claim class is type-coherent, every
  copy is individually billed, and every pair is evidentially
  distinct.

## Follow-ups

- Verify against the engine: the exact control flow by which
  exception/implication exempt the test obligation (implementation
  and test checks), so P-D5's fixture set covers the real paths;
  and that exact/full mutual coverage is transitive, so claim
  classes are well-defined.
- Integration fixtures: same-test-fn pair (fail W8); split-test
  pair with per-test reports (pass); fat-report distinct-tests
  pair (fail W8); duplicated-report pair (fail W8); Verus
  same-ensures pair (fail) and cross-ensures pair (pass);
  specificity-boundary binding (annotation on fn header vs inside
  clause spans); cap fixtures at N=1 (legacy parity) and N=2;
  coherence fixtures per row of the Decision 4 table; fan-in
  listing exactness; mixed-form target default.
- Config schema (`[duplicates.claims]` / `[duplicates.targets]`):
  caps, fan-in bounds, the type-set family, coherence cells,
  shipped defaults; `citation` → `implementation` alias at the
  parse boundary, never emitted.
- Survey the public duvet corpus (s2n-quic, s2n-tls, the AWS
  crypto tooling) for actual target fan-in frequency — settles
  Decision 9's open `count` default (unlimited vs 1).
- Scoped overrides (Decision 14), if demand arrives: glob keys for
  target scope and their overlap-precedence rule; whether spec
  scope without section scope is worth having; fixtures for the
  three scoping properties.
- `duplicate-targets` query, then snapshot inclusion once the
  format stabilizes.
- Implementation-side distinctness (execution profiles) — own
  follow-up.
- Implication duplicates — rider on self-discharging implications.
- Impl-dump diagnostic locality (pointing failures at the dumped
  annotation rather than the innocent tests) — revisit with the
  transpose observation if hot-path dumping is observed.
