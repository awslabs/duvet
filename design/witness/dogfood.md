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

- The **same-crate** witness-property pairs (proof fn ↔
  `duvet-coverage` implementation) DISCHARGE: their bound
  witnesses carry coverage for the in-crate implementation
  annotations, and the verified `is_executed_by` cell confirms it.
- The **cross-crate** implementation annotations (in
  `duvet/src/query/{engine,result,witness}.rs`) covering the same
  spec text FAIL to discharge: proof witnesses from
  `duvet-coverage`'s SST logs structurally cannot reach
  `duvet`-crate code. Per [Decision 8](decisions.md#decision-8)
  (subset of producers), ALL covering implementations must be
  witnessed — so the pairs fail, and that is **correct behavior**.
- The runtime `type=test` annotations (on `#[test]` fns) become
  UNWITNESSED in a proof-only run — also Decision 8, on purpose.
  Only the sliced run is gated today.

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

### Why W2/W3/W6 fail and W1 passes

The correlation engine checks ALL covering implementations for a
test annotation's quoted spec text. When any covering implementation
is not executed by the bound witnesses, the correlation fails.

- **W2, W3, W6** each have covering implementation annotations in
  `duvet/src/query/engine.rs`, `duvet/src/query/result.rs`, and/or
  `duvet/src/query/witness.rs`. Proof witnesses have no coverage
  data for those `duvet`-crate files, so those impls evaluate
  `executed=false` — failing the correlation despite the same-crate
  impl succeeding.
- **W1** (`property-w1-same-witness-discharge`) has no engine-side
  annotation quoting its text — its only covering implementation
  is entirely within `duvet-coverage/src/witness.rs`. No
  cross-crate impl enters the covering set, so discharge succeeds.

The same-crate discharge mechanism works correctly: the proof
witness carries coverage for the in-crate implementation target
lines (confirmed via oracle: `witness[10] executed=true
has_file_id=true`). The failure is entirely due to cross-crate
implementations present in the correlation's covering set.

## CI wiring: expected-failure contract {#ci-wiring}

The sliced witness-query gate runs in CI (`.github/workflows/ci.yml`):

- The `verify` job passes `--log vir-sst --log-dir` to the proof
  run, gzips the logs (~21:1), and uploads them as the
  `verus-sst-logs` artifact.
- The `dogfood` job (`needs: verify`) downloads the artifact and,
  after `duvet report --ci`, runs the **Witness query dogfood**
  step with the expected-failure contract:

### The contract

The step asserts:

1. **Same-crate pairs discharge:** `Successful correlations ≥ 8`
   (the proof-fn ↔ in-crate-impl pairs all pass).
2. **Failed set matches expected-failures exactly:** the count of
   `Failed correlations` equals the number of LCOV-pending
   *sections*, and every location listed in
   [`dogfood-expected-failures.txt`](dogfood-expected-failures.txt)
   appears in the output as "Not executed implementation".
3. A **new failure** (unknown to the expected-failures file) fails
   CI.
4. An **unexpected pass** (a listed pair no longer appearing as
   failed) also fails CI — the file cannot rot silently.

The mechanism is intentionally simple: count-and-grep over the
query output. First wiring: wire it, watch it, then tighten.

### Exit criterion

From [decisions.md Follow-ups](decisions.md#follow-ups): the LCOV
mixed-coverage producer supplies runtime witnesses that execute the
engine code. When all cross-crate pairs discharge:

1. `dogfood-expected-failures.txt` is deleted (empty list = fully
   green).
2. The CI step simplifies back to exit-code gating (exit 0 = all
   pairs discharged).
3. The full-run (unsliced) gate is enabled — the mixed run MUST
   NOT use an aggregate suite report (A2 violation).

## What is still missing

- **The full-run (unsliced) proof-only gate.** Runtime `type=test`
  annotations are unwitnessed in a proof-only run by design
  (Decision 8); they stay enabled, and the full-run gate is
  deferred to the LCOV follow-up (Decision 16, Follow-ups).
- **Richer CI-consumable report output** for the witness verdicts.
  The exit code + grep is the gate today; the human-oriented output
  is what CI logs show.
- **LCOV mixed-coverage producer** — the exit criterion above.
