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

## Cross-crate annotation typing {#cross-crate-typing}

Proof witnesses from `duvet-coverage`'s SST logs can never reach
`duvet`-crate code: the obligation closure is bounded by the
verified crate. So an engine-side annotation quoting a witness
property's spec text is *structurally* undischargeable as an
implementation pair — not wrong, just not something a proof
witness can ever execute.

The rule: **engine-side (cross-crate) annotations quoting
proof-side property text carry `type=implication`** ("fundamentally
true or not testable [by this producer]"), never a bare citation.
The engine supports this honestly: implication and exception
coverers tile the requirement's quote but are never held to the
witness-executed correlation (`duvet query -c coverage` skips them
in the per-coverer discharge loop; the integration test
`query-coverage-implication-not-held` pins this). The LCOV
follow-up, whose runtime witnesses CAN reach engine code, is
expected to flip these back to `type=implementation`.

This rule was found the dogfood way. The first sliced run failed
W2, W3, and W6 while W1 passed, and the initial suspicion fell on
the both-annotations-on-the-checker-fn placement
([Decision 21](decisions.md#decision-21)) — discharge-unit
rooting, target resolution, or body lines being excluded from
witness maps. The producer's own parser, used as the oracle,
disconfirmed all three: every checker fn's extent witness contains
its own body lines, and the in-crate implementation annotations
all evaluated *executed*. The failing coverers were the
cross-crate ones — W1 passed simply because it is the one property
with no engine-side annotation quoting its text. Decision 21's
placement convention itself needed no change.

## CI wiring {#ci-wiring}

The sliced witness-query gate runs in CI (`.github/workflows/ci.yml`):

- The `verify` job passes `--log vir-sst --log-dir` to the proof
  run (same verification, now leaving its SST record behind),
  gzips the logs (~21:1), and uploads them as the
  `verus-sst-logs` artifact.
- The `dogfood` job (`needs: verify`) downloads the artifact and,
  after `duvet report --ci`, runs the witness query sliced to the
  eleven property sections (W1–W7, P1–P4), gated on its exit code:
  every property pair must discharge.

## What is still missing

- **The full-run (unsliced) proof-only gate.** The runtime
  `type=test` annotations are unwitnessed in a proof-only run by
  design ([Decision 8](decisions.md#decision-8)); they stay
  enabled, and the full-run gate is deferred to the LCOV follow-up
  ([Decision 16](decisions.md#decision-16),
  [Follow-ups](decisions.md#follow-ups)). Its exit criterion is
  the mixed run green with all pairs discharged — and the mixed
  run MUST NOT use an aggregate suite report (that would put an A2
  violation at the center of the feature's own demo).
- **Richer CI-consumable report output** for the witness verdicts.
  The exit code is the gate today; the human-oriented output is
  what CI logs show.
- The expectation that the first CI wiring will be imperfect:
  wire it, watch it, then tighten.
