# Duvet Duplicates: Formal Specification

**Version:** 0.1.0
**Date:** 2026-08-07
**Status:** Draft

This specification defines the semantics of duvet's `duplicates`
check: predicates over the two partitions annotations induce —
**claim classes** (annotations sharing a claim) and **target
classes** (annotations sharing a resolved target) — together with
the checked-in configuration that carries their policy. One check
evaluates both axes; the two axes are two namespaces of one
configuration, never two invocations.

Design rationale lives in [decisions.md](decisions.md).
This document uses the normative keyword conventions of
[RFC 2119](https://www.rfc-editor.org/rfc/rfc2119).

The coverage model and the witness layer are unchanged by this
specification. Evidence billing of duplicate copies is the
coverage check's own contract; nothing here depends on it or
alters it ([Decision 5](decisions.md#decision-5)).

---

## 1. Definitions {#definitions}

### 1.1 Annotation model {#annotation-model}

An annotation is a triple **(τ, q, t)**:

- **τ** — the claim form: one of `test`, `implementation`,
  `implication`, `exception`, `todo`, `spec`.
- **q** — the claim ([§1.2](#claims)).
- **t** — the resolved target ([§1.4](#targets)).

Naming ([Decision 10](decisions.md#decision-10)):
`implementation` is the canonical user-facing name of the claim
form the codebase calls `Citation`. Every user-facing surface this
specification defines — configuration keys, report sections,
failure messages — MUST emit the name `implementation` and MUST
NOT emit the name `citation`. Wherever a claim form is read as
input, `citation` MUST be accepted as an alias for
`implementation`.

### 1.2 Claims and claim classes {#claims}

The **claim** of an annotation is the pair (target section,
normalized quote): the specification section the annotation
names, plus its quoted requirement text after whitespace
normalization.

Two annotations **share a claim** if and only if their target
sections are identical and their normalized quotes are identical.

A **claim class** is a maximal set of annotations sharing a claim,
taken across all claim forms. A **duplicate set** is the
restriction of a claim class to a single claim form.

Sharing a claim is an equivalence relation by construction
(equality of the pair), so claim classes are well-defined. This
pins the design vocabulary "quotes mutually cover each other": for
a pair of quotes on one section, mutual full coverage holds
exactly when the normalized quotes are equal. Coverage of one
claim by several others together is not claim sharing; it is
subsumption ([§2.3](#subsumption)).

An annotation whose normalized quote is empty is a section-level
reference, not a claim: it participates in no claim class and no
rule in [§2](#duplicates-check) applies to it.

A `spec`-form annotation names requirement text itself rather than
claiming anything about an artifact, and every extracted
requirement is present as one. Its claim class membership
therefore counts only for [§2.2](#caps) (spec duplicate sets are
unique); the cross-form rules ([§2.4](#exclusivity)) range over a
class's non-`spec` members.

### 1.3 Claim coverage {#claim-coverage}

A claim q is **fully covered** by a set of claims Q if and only if
every position of q's normalized quote, located in its section,
is occupied by the located normalized quote of at least one member
of Q on the same section. This is the engine's existing annotation
coverage relation; this specification adds no new matching
semantics.

### 1.4 Resolved targets and target classes {#targets}

The **resolved target** of an annotation is the source position
its annotation block resolves to under the coverage model's target
resolution (the classified or the degraded path), identified as
the pair (source file, resolved line). Nothing in this
specification depends on *how* resolution happens, only that it is
a pure function of the checked-in tree.

A **target class** is a maximal set of annotations sharing a
resolved target.

An annotation whose target does not resolve (defeated
classification, or resolution yielding no line) participates in no
target class. Claim-axis rules ([§2.2](#caps)–[§2.4](#exclusivity))
still apply to it. Environmental note, non-normative: code
formatters relocate comments, so same-target collisions can be
tooling artifacts; the rules over target classes are policy about
placement, not accusations of intent.

### 1.5 Pricing {#pricing}

The **priced forms** are `test` and `implementation`: forms whose
claims carry checkable obligations under the coverage check (a
test must be witnessed and discharge; an implementation must be
executed under covering tests' witnesses).

The **free forms** are `exception`, `implication`, and `todo`:
forms whose claims carry no checkable obligation. Engine fact this
specification depends on: a free-form claim on a requirement
exempts that requirement from the test obligation — it rewrites
the obligations of the entire requirement, not merely its own.

The `spec` form names requirement text itself and is neither
priced nor free; it is unique always ([§2.2](#caps)).

---

## 2. The duplicates check: claim axis {#duplicates-check}

The `duplicates` check evaluates the rules of this section and of
[§3](#duplicate-targets) over the in-scope annotation set. Scope selection (spec-slice
filtering) selects the input set; it does not change the rules.

### 2.1 Same claim, same target {#same-claim-same-target}

Two annotations that share a claim and share a resolved target
MUST fail the duplicates check, regardless of their claim forms
and regardless of any configured cap.
([Decision 2](decisions.md#decision-2);
[P-D1](#property-p-d1))

Same location is defined as same *resolved target*, never same
source line: stacked copies above one statement collapse to one
target and fail here; copies that resolve to different targets are
governed by the cap ([§2.2](#caps)).

### 2.2 Multiplicity caps {#caps}

Each claim form has a **cap**: a positive integer, default 1
([§4.3](#defaults)).

A duplicate set whose size exceeds its form's cap MUST fail the
duplicates check.
([Decision 1](decisions.md#decision-1))

Caps MUST be configurable only for the priced forms. The caps of
`spec`, `todo`, `exception`, and `implication` are fixed at 1 and
MUST NOT be configurable.
([Decision 3](decisions.md#decision-3);
[P-D2](#property-p-d2))

A duplicate set within its cap MUST pass this check
([Decision 5](decisions.md#decision-5)), and when its size exceeds
one it MUST be reported with its size and every member's location,
so multiplicity is always surfaced, never silent.
([Decision 1](decisions.md#decision-1))

### 2.3 Subsumption {#subsumption}

An annotation whose claim is fully covered
([§1.3](#claim-coverage)) by the claims of same-form annotations
outside its own claim class MUST fail the duplicates check,
regardless of any configured cap.

Rationale, non-normative: this is the historical check's
union-coverage semantics, separated from the cap rule. A fragment
copy claims nothing its coverers do not already claim; no cap
admits it, because the motivating case for a raised cap (two
evidence artifacts for one requirement) is copies of the *same*
claim, not fragments of it. Together with [§2.2](#caps) at cap 1,
this rule reproduces the historical verdict
([P-D3](#property-p-d3)).

### 2.4 Claim-form family {#exclusivity}

A claim class whose non-`spec` members bear two or more distinct
claim forms MUST fail the duplicates check unless the set of
non-`spec` claim forms is admitted by the configured claim-form
family ([§4.2](#schema-claims)). A claim class whose non-`spec`
members bear a single claim form MUST NOT fail this rule: copies
of one form are the caps' business ([§2.2](#caps)).
([Decision 4](decisions.md#decision-4);
[P-D5](#property-p-d5))

The default claim-form family admits exactly the form sets
containing no free form ([§4.3](#defaults)) — any combination of
`test` and `implementation` members may share a claim, each billed
on its own; a free form never shares a claim with another form.
An empty family (`types = []`) admits no multi-form set: it
forbids all multi-form claim sharing, `test` + `implementation`
included. Derived verdicts under the default family, for the
record:

| Coexistence (same claim) | Verdict |
|---|---|
| `exception` + `test` | fail — contradiction |
| `exception` + `implementation` | fail — contradiction |
| `implication` + `implementation` | fail — silent unbilling |
| `implication` + `test` | fail — incompatible claims about R's nature |
| `todo` + `implementation` | fail — the todo is replaced, not joined |
| `test` + `implementation` (different targets) | pass — the system itself |

This rule is a deliberate tightening of released behavior
([Decision 14](decisions.md#decision-14)): cross-form coexistence
on one requirement silently passes the historical check. The
restoration path is a configuration line ([§4.2](#schema-claims)),
never a comment.

### 2.5 Partial overlap {#partial-overlap}

Annotations whose quotes overlap without full coverage in either
direction (the `some_overlap` population) are outside this
specification's model. Three layers, separately stated:

Verdict: a partial overlap MUST NOT fail the duplicates check,
matching the historical check.

Content: the check MUST identify every partial-overlap pair in
its analysis.

Presentation, non-normative note: display of the analysis
population is verbosity-tiered — the default report omits it,
`--verbose` shows it; verbosity never changes the verdict
([§4.1](#policy-source)). This is deliberately weaker than
[§2.2](#caps)'s unconditional surfacing: an admitted duplicate is
a standing liability kept in view at every verbosity
([Decision 1](decisions.md#decision-1)); a partial overlap is a
model-boundary disclosure, not a liability the check prices.

### 2.6 Verdict {#duplicates-verdict}

The duplicates check MUST fail if and only if at least one rule of
[§2.1](#same-claim-same-target)–[§2.4](#exclusivity) or of the
target axis ([§3.2](#fan-in-bounds)–[§3.3](#type-combinations))
fires.

Run alone, a passing verdict claims structure only. Evidence
billing of every copy is the coverage check's contract, enforced
whether or not this check runs ([Decision 5](decisions.md#decision-5)).
The `executed-coverage` mode carries no duplicate guarantees; it
is deliberately partial for everything, and duplicates inherit
that partiality ([Decision 7](decisions.md#decision-7)).

---

## 3. The duplicates check: target axis {#duplicate-targets}

The target axis polices fan-in of annotations onto resolved
targets. It is a listing first and a gate only where configuration
opts in or a shipped family default bites
([Decision 8](decisions.md#decision-8)). It is not a separate
check: one invocation evaluates both axes, mirroring the one
`[duplicates]` configuration table.

### 3.1 Listing {#fan-in-listing}

The duplicates check's report MUST include the fan-in listing: it
MUST contain a target if and only if two or more annotations
resolve to it.
([P-T1](#property-p-t1))

Each listed target MUST report its exact annotation count, its
per-form breakdown, and its distinct-section count, and the
listing MUST be sorted by annotation count descending.

The distinct-section count is a triage column, not a
discriminator: `count=12 sections=1` reads "dense but coherent";
`count=12 sections=8` reads "look here first."

### 3.2 Fan-in bounds {#fan-in-bounds}

Two opt-in bounds, both unlimited by default
([§4.3](#defaults)):

- `count` — the maximum number of annotations in one target class.
- `sections` — the maximum number of distinct sections claimed by
  one target class.

When a bound is configured, a target class exceeding it MUST fail
the duplicates check. When a bound is not configured, no target
class fails it.

These are the only rules in this specification that fail code for
a smell rather than a violated evidence property; teams opt into
the opinion.

### 3.3 Target form combinations {#type-combinations}

Configuration declares a **family of allowed form sets** for
target classes ([§4.2](#schema-targets)).

A target class bearing two or more distinct claim forms MUST fail
the duplicates check unless its form set is a subset of some
member of the family. A target class bearing a single claim form
MUST NOT fail this rule.
([Decision 12](decisions.md#decision-12);
[P-T2](#property-p-t2))

The default family admits every form set that does not contain
both `test` and `implementation`
([Decision 8](decisions.md#decision-8)): a position claimed as
test code and implementation code simultaneously fails by default;
the meta-requirement case relaxes the default in configuration and
owns the decision in review.

An empty family (`types = []`) forbids all form mixing on a
target.

### 3.4 Snapshot inclusion {#snapshot}

Future work, recorded so the deferral is a decision: once the
listing format stabilizes, the listing lands in the snapshot so
concentration growth surfaces as a snapshot diff in review. Not
part of this version.

---

## 4. Configuration {#configuration}

### 4.1 Policy source {#policy-source}

Every policy value in this specification MUST be read from
checked-in configuration. The shipped defaults
([§4.3](#defaults)) apply where configuration is silent.

Command-line options MUST NOT change any verdict: the command line
selects which checks run, points at evidence artifacts, and
controls verbosity — display, never verdict.
([Decision 9](decisions.md#decision-9);
[P-S0](#property-p-s0))

### 4.2 Schema {#schema}

Policy lives in two namespaces mirroring the two coincidence
axes ([Decision 11](decisions.md#decision-11)). Every setting
lives in exactly one namespace; no setting name appears in both.

#### 4.2.1 `[duplicates.claims]` — predicates over claim classes {#schema-claims}

- `test` — cap for `test` duplicate sets. Positive integer.
- `implementation` — cap for `implementation` duplicate sets.
  Positive integer.
- `types` — the claim-form family: an array of allowed form sets,
  each set written as form names joined by `+`. A claim class
  whose non-`spec` members bear two or more distinct claim forms
  fails [§2.4](#exclusivity) unless that form set is a subset of
  some member of the family. This is the
  coherence-cell mechanism of
  [Decision 14](decisions.md#decision-14): the release notes name
  the family value that restores prior behavior.

#### 4.2.2 `[duplicates.targets]` — predicates over target classes {#schema-targets}

- `count` — fan-in bound ([§3.2](#fan-in-bounds)). Positive
  integer.
- `sections` — distinct-section bound ([§3.2](#fan-in-bounds)).
  Positive integer.
- `types` — the target-form family ([§3.3](#type-combinations)):
  an array of allowed form sets, `+`-joined.

#### 4.2.3 Shared rules {#schema-shared}

Form names in configuration use the canonical vocabulary
([§1.1](#annotation-model)); `citation` MUST be accepted as an
alias for `implementation` and MUST NOT appear in any emitted
output.

An unrecognized key under `[duplicates.claims]` or
`[duplicates.targets]` MUST be a configuration error, so a typo
never silently takes a default.

Configuring a cap for a form other than `test` or
`implementation` MUST be a configuration error
([§2.2](#caps)).

Example:

```toml
[duplicates.claims]
test = 2                        # proof + PBT: two evidence artifacts
# implementation defaults to 1
# types defaults to the free-form-exclusive family (§4.3)

[duplicates.targets]
count = 15                      # opt-in fan-in bound
types = ["exception+test"]      # allow this combination on one target
```

### 4.3 Defaults {#defaults}

| Setting | Default | Meaning at default |
|---|---|---|
| `claims.test` | 1 | test duplicate sets are singletons |
| `claims.implementation` | 1 | implementation duplicate sets are singletons |
| `claims.types` | all form sets containing no free form | free forms exclusive; `test`/`implementation` may share a claim |
| `targets.count` | unlimited | listing only. Confirmed by the corpus fan-in survey: the legitimately dense population is large (s2n-quic alone bears 149 multi-annotation targets, 113 of them at count 2), so a biting default fails real dense code on day one |
| `targets.sections` | unlimited | listing only |
| `targets.types` | all form sets not containing both `test` and `implementation` | mixed test/implementation targets fail |

### 4.4 Restoring the historical verdict {#restoring-legacy}

Non-normative recipe. The historical check is reachable by
configuration, up to [P-D3](#property-p-d3)'s two residuals:

```toml
[duplicates.claims]
# caps default to 1 — that is the historical behavior
types = ["spec+test+implementation+exception+todo+implication"]

[duplicates.targets]
# count/sections default unlimited
types = ["spec+test+implementation+exception+todo+implication"]
```

A family containing the all-forms set admits every combination
(subset closure), turning [§2.4](#exclusivity) and
[§3.3](#type-combinations) off. Subsumption ([§2.3](#subsumption))
needs no knob: it is historical behavior. What no configuration
restores: cross-form same-claim-same-target stacks
([§2.1](#same-claim-same-target) is deliberately knobless) and the
empty-quote accident ([§1.2](#claims)).

### 4.5 Scoped overrides {#scoped-overrides}

Designed and deferred ([Decision 13](decisions.md#decision-13)).
All values in this version are global. This version states no
requirements for the deferred feature; the invariants a future
scoped-overrides design owes — resolution determinism,
unconfigured equivalence, shadowing monotonicity — live with the
decision that deferred it.

---

## 5. Properties {#properties}

Each property is pinned by an integration fixture, except where
noted.

### P-D1 — stacked-spam soundness {#property-p-d1}

If the duplicates check passes, no two annotations sharing a claim
share a resolved target, across all claim forms.
([§2.1](#same-claim-same-target))

### P-D2 — uniqueness of non-priced forms {#property-p-d2}

If the duplicates check passes, every duplicate set of form
`spec`, `todo`, `exception`, or `implication` is a singleton.
([§2.2](#caps))

### P-D3 — scoped legacy equivalence {#property-p-d3}

With every cap at 1, both form families admitting every form set,
and both fan-in bounds unset, the duplicates verdict is identical
to the historical check's verdict on all inputs, except:

- inputs containing an empty-quote annotation ([§1.2](#claims)) —
  the historical check failed them by accident of an early return;
- inputs containing a cross-form same-claim-same-target stack —
  [§2.1](#same-claim-same-target) is deliberately unconditional
  ([Decision 2](decisions.md#decision-2)) and has no knob.

Everything that was a decision is pinnable; the residuals are one
deliberate unconditional rule and one accident. The existing
regression fixtures are this property's test suite, unchanged.

### P-D4 — bound monotonicity {#property-p-d4}

For every configured numeric bound (caps, `count`, `sections`):
raising it never flips a passing project to failing, and lowering
it never flips a failing project to passing.

### P-D5 — free-form exclusivity {#property-p-d5}

Under the default claim-form family: if the duplicates check
passes, every claim class whose non-`spec` members include an
`exception`, `implication`, or `todo` annotation has exactly one
non-`spec` member. Jointly enforced: mixed-form classes fail the
claim-form family rule, single-form copies exceed the free caps.
([§2.4](#exclusivity); [§2.2](#caps))

### P-T1 — fan-in exactness {#property-p-t1}

The fan-in listing in the duplicates report contains a target if
and only if two or more annotations resolve to it; counts, form
breakdowns, and section counts are exact.
([§3.1](#fan-in-listing))

### P-T2 — form-family exactness {#property-p-t2}

With a configured family, a target class bearing two or more
distinct claim forms fails the form-combination rule if and only
if its form set is a subset of no allowed set; single-form target
classes never fail it. Adding an allowed set never flips a passing
project to failing; removing one never flips a failing project to
passing.
([§3.3](#type-combinations))

### P-S0 — verdict determinism {#property-p-s0}

For a fixed set of checks and evidence inputs, every verdict in
this specification is a function of the checked-in tree (source
plus configuration). Verbosity never changes a verdict.
([§4.1](#policy-source))

Fixture note: pinned on the verbosity axis (one tree, default and
`--verbose`, exit codes pinned equal). The full function-of-the-tree
claim ranges over every command-line option and is held
architecturally — policy values reach the checks only through
checked-in configuration ([§4.1](#policy-source)) — rather than by
fixture enumeration.

### P-S1 — no pass without evidence {#property-p-s1}

If the duplicates check passes and the coverage check passes:
every duplicate copy resolves to a distinct target, every
duplicate set is within its cap, every claim class satisfies the
claim-form family, and every copy is individually billed by
coverage. The billing conjunct is the coverage check's own
contract, restated here so the independence of
[Decision 5](decisions.md#decision-5) is visibly safe.

Fixture note: no single fixture pins the conjunction. The
structural conjuncts are pinned by the per-rule fixtures of
[§2](#duplicates-check)–[§3](#duplicate-targets); the billing
conjunct is the coverage check's own contract with its own suite.
P-S1 is their composition, safe by check independence
([Decision 5](decisions.md#decision-5)).
