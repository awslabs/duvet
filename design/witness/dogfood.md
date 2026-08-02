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

- Every **proof-side** witness-property pair (proof fn ↔ verified-crate
  implementation) DISCHARGES: the bound witnesses carry coverage for
  the in-crate implementation annotations, and the verified
  `is_executed_by` cell confirms it. Spec text quoted by a witnessed
  proof test is covered ONLY by implementation annotations inside the
  witness's line set — engine-side copies of that text were removed
  (they return with the LCOV producer) or narrowed to the clauses the
  engine genuinely owns.
- The runtime `type=test` annotations (on `#[test]` fns) are
  UNWITNESSED in a proof-only run — Decision 8, on purpose. This is
  the ENTIRE residual failure set of the unsliced run (see the
  staging section below); zero failed correlations and zero
  missing-implementation findings remain.

## Placement rules the closure work established {#placement-rules}

Hard-won by root-causing the 2026-08-02 failed-correlation set:

- **No interleaved prose inside annotation stacks.** These files have
  no language classifier, so degraded resolution skips only blank and
  annotation lines; an ordinary `//` comment between stacked
  annotation blocks becomes the upper blocks' resolved target —
  unwitnessable. Prose goes ABOVE the stack (spec §1.1's placement
  note, itself now cited from `target_resolution.rs`).
- **Attributes go above the stack too.** A `#[allow(...)]` between the
  stack and the fn header displaces the target the same way.
- **Rule definitions live on consulted lines.** Enum-variant
  declarations are not in any witness's line set; the ClaimRule rule
  quotes moved to the `binds` spec fn's match arms.
- **One implementation owner per proof-tested spec text**, at the site
  whose `ensures` proves it; engine adapters keep a pointer comment.

## Cross-crate pairs: implementation-true, witness-pending {#cross-crate-posture}

The engine-side annotations quoting witness-property spec text are
**`type=implementation`** — they genuinely implement the reporting
requirements (the integration tests exercise exactly that
behavior), and they ARE testable by runtime witnesses. What's
missing is witness EVIDENCE for those runtime tests, which arrives
with the LCOV follow-up.

`type=implication` means "fundamentally true or not testable" —
that is false for this code. Reclassifying an obligation because
its pair can't discharge yet would be deciding a property is true
instead of proving it.

All seven engine-side W2/W3/W6 annotations are now removed (the
first five in the correlation-floor commit; the last two — W2 on
`bound_witnesses`, W6 on the engine's unwitnessed branch — in the
2026-08-02 closure, when fresh SST logs showed them failing the
sliced gate). The verified crate's implementation annotations own
the property text, formulas included. The LCOV producer re-adds
the engine annotations and expands the floor.

## CI wiring: correlation floor {#ci-wiring}

The sliced witness-query gate runs in CI (`.github/workflows/ci.yml`):

- The `verify` job passes `--log vir-sst --log-dir` to the proof
  run, gzips the logs (~21:1), and uploads them as the
  `verus-sst-logs` artifact.
- The `dogfood` job (`needs: verify`) downloads the artifact and,
  after `duvet report --ci`, runs the **Witness query dogfood**
  step: the full W1–W7 / P1–P4 slice must discharge with plain
  exit 0 — no thresholds, no expected-failure list. Sections in
  the slice can never backslide.

### Exit criterion

From [decisions.md Follow-ups](decisions.md#follow-ups): the LCOV
mixed-coverage producer supplies runtime witnesses that execute the
engine code. When the runtime annotations are witnessed, the
full-run (unsliced) gate is enabled — the mixed run MUST NOT use an
aggregate suite report (A2 violation).

## Staging: the unsliced proof-only residual {#staging}

Measured 2026-08-02 (post gate1 closure, fresh SST logs, unsliced
`duvet query -c coverage`): **0 failed correlations, 0 tests with
no implementation, 32 successful correlations, 75 unwitnessed.**

Every one of the 75 unwitnessed test annotations resolves to a
runtime `#[test]`/`#[tokio::test]` construct (audited: no comment,
attribute, or declaration targets remain). They are the
LCOV-pending set — unwitnessed in a proof-only run by design
(Decision 8) — and they are Gate 3's exit criterion: when the LCOV
producer lands, this table must go to zero.

| File | Unwitnessed (runtime, LCOV-pending) |
|---|---|
| duvet/src/query/parsers/verus_sst/tests.rs | 21 |
| duvet/src/query/witness.rs | 15 |
| duvet/src/query/producers.rs | 10 |
| duvet/src/query/result.rs | 6 |
| duvet-coverage/src/witness.rs (tests mod) | 6 |
| duvet-coverage/src/proofs.rs (tests) | 6 |
| duvet-coverage/src/scopes.rs (tests) | 3 |
| duvet/src/query/parsers/verus_sst/closure.rs | 2 |
| duvet-coverage/src/execution_propagation.rs (tests) | 2 |
| duvet-coverage/src/classify_postpass.rs (tests) | 2 |
| duvet/src/query/engine.rs | 1 |
| duvet-coverage/src/target_resolution.rs (tests) | 1 |
| **Total** | **75** |

## What is still missing

- **The full-run (unsliced) proof-only gate.** Runtime `type=test`
  annotations are unwitnessed in a proof-only run by design
  (Decision 8); they stay enabled, and the full-run gate is
  deferred to the LCOV follow-up (Decision 16, Follow-ups).
- **Richer CI-consumable report output** for the witness verdicts.
  The exit code + grep is the gate today; the human-oriented output
  is what CI logs show.
- **LCOV mixed-coverage producer** — the exit criterion above.
