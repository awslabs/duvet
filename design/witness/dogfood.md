# Dogfooding: duvet-coverage as its own witness test bed

**Status:** Living document. Revisit and update as the missing
pieces below land; the sections here are expected to change.

## Purpose

The witness feature is verified by itself and demonstrated on
itself. `duvet-coverage` carries the Verus proofs of the witness
properties ([spec §2](spec.md#engine-properties)), and those proof
fns carry `type=test` annotations citing the very spec sections
they prove — so running the witness feature over `duvet-coverage`
discharges the feature's own specification through the feature's
own machinery. The evidence for specific runs lives in commit
messages and in the test suite (golden corpus, engine end-to-end
tests, the regression tests pinning placement behavior), not here.

## Running the self-hosting demo

```
# 1. Produce fresh proof witnesses (SST logs):
cargo verus build -p duvet-coverage -- --log vir-sst --log-dir /tmp/sst

# 2. Full proof-only run over duvet-coverage's annotations:
cargo run -p duvet -- query -c coverage --coverage-source verus-sst=/tmp/sst

# 3. Sliced to the witness spec's proof-side properties:
cargo run -p duvet -- query -c coverage --coverage-source verus-sst=/tmp/sst \
    -s "design/witness/spec.md#property-w1-same-witness-discharge,..."
```

Expected shape of the results:

- The witness-property pairs (proof fn ↔ engine code) DISCHARGE,
  each with provenance naming the discharging obligation and
  strength `consulted`.
- The runtime `type=test` annotations (on `#[test]` fns) become
  UNWITNESSED in a proof-only run and fail it — this is
  [Decision 8](decisions.md#decision-8)'s subset-of-producers
  behavior, on purpose. Only the sliced run is green today.

## What is missing before this runs in CI

- **A gateable proof-only run.** The decided staging: disable the
  runtime `type=test` annotations with one grep-able marker token
  (`DUVET-DISABLED-AWAITING-LCOV`) so the full proof-only run
  exits 0 and CI gates on it; re-enabling is a search, not a
  memory. Not yet applied.
- **CI-consumable report output** for the witness verdicts
  (today's output is human-oriented).
- **Runtime execution witnesses** — the LCOV-family producer and
  the per-test harness example
  ([Decision 16](decisions.md#decision-16)), deferred to the LCOV
  follow-up PR ([Follow-ups](decisions.md#follow-ups)). Its
  opening move is re-enabling the marked annotations; its exit
  criterion is the mixed run green with all pairs discharged —
  and the mixed run MUST NOT use an aggregate suite report
  (that would put an A2 violation at the center of the feature's
  own demo).
- The expectation that the first CI wiring will be imperfect:
  wire it, watch it, then tighten.
