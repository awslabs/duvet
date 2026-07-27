# Dogfooding plan: duvet-coverage as its own witness test bed

**Date:** 2026-07-26
**Status:** Plan. Annotation edits not yet applied.

The self-hosting demo: run the witness feature over `duvet-coverage`
itself, with Verus SST witnesses discharging proof-side pairs and
(optionally) LCOV witnesses discharging runtime-side pairs,
through the same engine.

## Current annotation inventory (surveyed 2026-07-26)

- 21 `type=implication` — on proof fns and predicates,
  citing `coverage-model-spec.md` property sections.
- 11 `type=test` — **on runtime `#[test]` fns**
  (`test_property_2_sibling_scopes`, `try_block_propagation_...`,
  `collect_hit_lines_matches_hit_oracle`, ...), not on proofs.
- 2 `type=implementation` — scopes.rs only.

Consequence: the crate already has runtime test annotations per
property, but almost no implementation annotations to pair against,
and no proof fn is annotated `type=test`.

## Planned additions

For each property section below:

1. `type=implementation` on the algorithm lines the property
   governs (this is the missing half of every pair).
2. `type=test` on the proof fn that certifies the property
   (keeping its existing `type=implication` — annotations stack).

| Property section | New impl annotation on | New test annotation on | Predicted proof-witness verdict |
|---|---|---|---|
| property-2-no-cross-scope-leakage | `annotation_execution.rs` (`is_annotation_executed`) | `proofs::executed_annotation_has_no_cross_scope_leakage` | DISCHARGED — closure reaches annotation_execution.rs (POC-confirmed, corrected parse: 25 obligations, 6 files) |
| property-2 (discrimination control — **test fixture only, never annotated in the tree**) | same impl | `proofs::lemma_no_cross_scope_leakage` | NOT discharged — closure never reaches annotation_execution.rs (POC-confirmed). Lives as a producer/engine test asserting non-discharge; the shipped tree only carries annotations we believe true |
| property-3-conservative-fallback | `annotation_execution.rs` | `proofs::executed_annotation_conservative_fallback` | DISCHARGED (same family) |
| property-4-monotonicity | `annotation_execution.rs` | `proofs::executed_annotation_monotonic` | DISCHARGED (same family) |
| property-9-execution-set-containment | `execution_propagation.rs` (the propagation walk) | existing runtime test + a proof-side annotation TBD | runtime witness needed; proof side TBD |

The discrimination control is a test, not an annotation:
duvet never ships a pair expected to fail.

## Primary path: the new work annotates itself (bootstrap)

The retrofit table above is the *secondary* path.
The primary dogfood target is the witness feature's own new
artifacts, annotated in the new style from birth —
the old code's `type=implication` style existed only because no
discharge mechanism existed for either side when it was written.

For each new spec property (design/witness/spec.md, W1–W6):

- `type=implementation` on the new engine correlation code and
  producer code implementing it —
  discharged by **runtime witnesses** (the integration/unit tests
  that exercise that code).
- `type=test` on the Phase 4 Verus proof of the property
  (the quantifier layer in `duvet-coverage`) —
  discharged by **SST witnesses** from verifying `duvet-coverage`.

One section, both producer families, mixed provenance on the
feature's own spec — the decision-4 goal demonstrated on the thing
that implements it.
Sequencing: state and prove W1–W6 in Verus (milestone 3),
annotate proofs and code as they land,
and the self-hosting run checks them as soon as milestones 1+2
are wired.
Splitting existing `implication` annotations into
test/implementation pairs is a later, optional pass.

## What each run exercises

- **Proof-only run** (SST producer only):
  the executed_annotation pairs discharge (spec W1);
  every runtime `type=test` annotation on `#[test]` fns becomes
  UNWITNESSED and must be reported as a failure (spec W6) —
  this is Decision 8's subset-of-producers behavior, on purpose.
- **Mixed run** (SST + LCOV from `cargo llvm-cov`/grcov):
  runtime pairs discharge via runtime witnesses,
  proof pairs via proof witnesses, one engine, one report —
  the original decision-4 goal, demonstrated on our own crate.
- **Verdict provenance** (spec §3): the report must show
  "discharged by proof fn ..." vs "discharged by <lcov file>"
  with strengths Consulted vs Executed.

## Sequencing

Blocked on: milestone 1 (SST producer, thread 3) for closures;
milestone 2 (engine Witness type + binds/discharged) for verdicts.
Not blocked: the annotation edits themselves and re-running the
closure predictions with the corrected parser as soon as thread 3's
parser exists — do the pilot (property-2 pair, both rows) first
and validate placement against real target resolution before
bulk-annotating.
