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
| property-9-execution-set-containment | `execution_propagation.rs` (the propagation walk) | **DEFERRED to the LCOV dogfood session**: runtime half awaits the LCOV producer; proof-side annotation target chosen then | deferred (declared, not silently half-done) |

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

## Live run results (2026-07-28)

The self-hosting demo ran live: `duvet query -c coverage` over
duvet-coverage's own annotations, with fresh proof witnesses.

### Command sequence

```
cargo verus build -p duvet-coverage -- --log vir-sst --log-dir /tmp/sst-final
    # 65 verified, 0 errors, 11 module logs

# Direction 1 — full proof-only run (Decision 8 exercised):
cargo run -p duvet -- query -c coverage --coverage-source verus-sst=/tmp/sst-final
    # exit 1 (deliberate, see below)

# Direction 2 — sliced to the witness spec's proof-side properties:
cargo run -p duvet -- query -c coverage --coverage-source verus-sst=/tmp/sst-final \
    -s "design/witness/spec.md#property-w1-same-witness-discharge,...w2,...w3,...w4,...w5,...w6"
    # exit 0
```

### Verdict summary

- **6 of 6 W-property pairs DISCHARGED** (W1–W6), each with
  provenance naming its obligation and strength `consulted`:
  `report_discharged` (W1), `report_test_executed` (W2),
  `report_ever_executed` (W3), `failure_monotonicity` (W4),
  `by_execution_binding_implies_executed` (W5),
  `is_unwitnessed` (W6). Failed correlations: 0.
  Tests with no implementation: 0.
- **Full run exits 1 with exactly 10 unwitnessed test
  annotations** — all of them runtime `#[test]` annotations
  (proofs.rs property-2..6 + correctness-properties,
  execution_propagation.rs property-3 and property-9,
  scopes.rs scopes and property-11). This is Decision 8's
  subset-of-producers behavior, on purpose: a proof-only run
  must fail runtime-witnessed-only annotations, never silently.
  The failure output identifies each annotation and states that
  no configured producer yielded a witness (spec §3).
- **Sliced run (proof-side sections only) exits 0** with the same
  6 discharged pairs and 0 unwitnessed — the green direction.

### What the diagnosis found (placement, not engine bug)

The pre-fix run's 7 "plain W6" reports had two distinct causes,
both pinned by a new regression test
(`degraded_resolution_stacked_annotations_and_doc_comments`,
duvet/src/query/checks/coverage.rs):

1. **Stacked annotations resolve correctly.** Annotation lines are
   stamped `{Annotation}` (skippable), so every annotation in a
   stack resolves through the stack to the first real line below
   it. The stacking hypothesis was disconfirmed by instrumentation.
2. **Doc comments break degraded resolution.** `.rs` files have no
   classifier; in degraded mode a `///` line is unclassified and
   becomes the resolved target. The six witness.rs proof-fn test
   annotations had doc comments between the `//=` block and the fn
   header, so they resolved to comment lines — unelaborated in the
   SST artifact, hence plain W6 rather than not-proof-testable.
   Correct-but-surprising classifier-less behavior (the Java path
   avoids it only because its classifier marks comments skippable);
   fixed by placement, not by weakening dom(du) or NPT semantics:
   each annotation block moved below its docs, and spec §1.1 now
   carries a one-sentence placement note.

### Annotation placements added (the 9 missing pairs)

- W1/W2/W3/W6 → `type=implementation` on the bodies of the verified
  report fns themselves (the algorithm the ensures quantify over).
- W4 → on the `discharged` spec fn; W5 → on the `binds` spec fn —
  the functions those lemmas quantify over, and the lines their
  proof closures actually consult. No pair was forced: every
  placement is on lines genuinely implementing the cited property,
  and all six discharge as property-style pairs (no lemma-style
  expected-failure pair ships, per this plan's own rule).
- property-9 → the `collect_hit_lines` seed in `execution_set`;
  scopes → `build_scope_tree`; property-11 → its whole-file-scope
  branch. Their tests are runtime `#[test]`s: correctly unwitnessed
  in a proof-only run (Decision 8), pairing satisfied (no
  missing-implementation reports remain).

### Gates at completion

Verus 65 verified / 0 errors; `cargo test -p duvet query::`
125 passed (baseline 124 + the new regression test);
`cargo test -p duvet-coverage` 73 passed;
`cargo xtask checks --rustfmt-toolchain nightly-2025-11-09` exit 0;
`duvet report --ci true` exit 0 after snapshot regeneration
(snapshot delta: +29/−22 lines — the three retrofit quotes gained
`implementation` refs, witness.rs annotation blocks moved/added).

Remaining for the full decision-4 goal: a runtime (LCOV-family)
producer for Rust coverage, so the mixed run can discharge the 10
runtime pairs in the same invocation. Known follow-up: 14 older
`type=implication` annotations (predicates.rs, proofs.rs) also
resolve to comment lines under degraded resolution; harmless today
(implication coverers are not scored by proof witnesses in these
runs) but they should be re-placed in the optional retrofit pass.

## Live run re-verified after clause-granularity rooting (2026-07-29)

Decisions 17–20 landed in the producer (dom(du) = obligation
extents ∪ ensures clauses ∪ loop invariants ∪ proof asserts;
most-specific-wins rooting; labels from the artifact). The demo
re-ran on fresh logs:

```
cargo verus build -p duvet-coverage -- --log vir-sst --log-dir /tmp/sst-fresh
    # 65 verified, 0 errors, 11 module logs
cargo run -p duvet -- query -c coverage --coverage-source verus-sst=/tmp/sst-fresh
    # exit 1: 6/6 correlations, 10 unwitnessed runtime tests (Decision 8)
cargo run -p duvet -- query -c coverage --coverage-source verus-sst=/tmp/sst-fresh \
    -s "design/witness/spec.md#property-w1-...,...w2,...w3,...w4,...w5,...w6"
    # exit 0: 6/6 discharged, 0 failed, 0 unwitnessed
```

- **All 6 W-property pairs still DISCHARGE**, verdicts identical to
  2026-07-28. Their witnesses still root **extent units**: each
  test annotation resolves to the `pub proof fn` header line
  (below its docs, per the placement fix), which sits inside the
  obligation extent and inside no finer unit — so the extent is
  the correct most-specific unit and the labels remain the six
  fully-qualified fn paths. No pair rooted a clause, because no
  annotation targets a clause line.
- Clause-grain rooting is exercised by the golden corpus instead:
  of its 1990 elaborated project lines, 752 are now in dom(du)
  (was 235) — winning-unit kinds: 230 extent, 150 ensures,
  185 loop-invariant, 187 proof-assert — and the vacuity fixture's
  scenario-3 ensures line roots its own clause unit
  (`vacuity::self_contained ensures[0]`), as Decision 18 requires.
- The full run's 10 unwitnessed annotations are the same 10
  runtime `#[test]` annotations as 2026-07-28 (proofs.rs ×6,
  execution_propagation.rs ×2, scopes.rs ×2) — clause expansion
  moved none of them, as expected: their targets are `#[test]`
  fns outside the artifact.

## CI staging (decided 2026-07-28)

The full proof-only run exits 1 on the 10 unwitnessed runtime
test annotations — Decision 8 working as designed, but not a
gateable state. Staging decision:

- The 10 runtime `type=test` annotations are **disabled** (the
  annotation comment broken deliberately) with one grep-able
  marker token — `DUVET-DISABLED-AWAITING-LCOV` — on each, so the
  full proof-only run exits 0 and **CI gates on it from day one**.
  The marker is mechanically findable: re-enabling is a search,
  not a memory.
- The next dogfood session adds Rust runtime coverage
  (the `cargo llvm-cov` per-test harness, Decision 16's first
  example project); its opening move is re-enabling the marked
  annotations, and its exit criterion is the mixed run green with
  all pairs discharged — the full decision-4 goal.
- No asserted-failure wrapper is built for the proof-only red
  state: it is meant to be temporary, and the sliced-run
  regression coverage plus Decision 8's unit tests already pin
  the subset-of-producers behavior.
- The per-test harness requirement (one coverage invocation per
  test, one report = one witness) is Decision 16; the mixed run
  MUST NOT use an aggregate suite report — that would put an A2
  violation at the center of the feature's own demo.
