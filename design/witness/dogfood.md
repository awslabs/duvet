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
  UNWITNESSED in a proof-only run — Decision 8, on purpose. Together
  with the CI-discharged meta-obligation tests in ci.yml (staging
  section), this is the ENTIRE residual set of the unsliced run;
  zero failed correlations and zero missing-implementation findings
  remain.

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

These rules are machine-enforced, not review discipline: the
coverage query's **unwitnessable verdict**
([Decision 22](decisions.md#decision-22),
[spec §3](spec.md#verdict-output)) statically flags any annotation
whose resolved target is a blank, comment, or attribute line —
computed from source text alone, never from a witness — and CI
gates the unsliced run on zero such findings
([#ci-wiring](#ci-wiring)).

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

The converse also holds: the meta-obligation implications ("MUST
be proven with Verus", "proof files MUST carry annotations") were
reclassified as implementation+test once their enforcing sites —
the proof files, the `[[source]]` scan config, and the CI verify
and snapshot steps — became scannable (per-source
`comment-style`). `type=implication` is reserved for text with no
enforcing site at all.

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
- The **Unwitnessable placement gate** then runs the full
  (unsliced) coverage query and asserts `Unwitnessable targets: 0`
  tree-wide — a new placement defect anywhere in the scanned
  sources fails CI. The unsliced run's discharge verdict stays
  ungated (Decision 8) until the LCOV follow-up; only the static
  placement count is asserted.

### Exit criterion

From [decisions.md Follow-ups](decisions.md#follow-ups): the LCOV
mixed-coverage producer supplies runtime witnesses that execute the
engine code. When the runtime annotations are witnessed, the
full-run (unsliced) gate is enabled — the mixed run MUST NOT use an
aggregate suite report (A2 violation).

## Staging: the unsliced proof-only residual {#staging}

Current tool-checked state of the unsliced
`duvet query -c coverage` run over fresh SST logs:
**0 failed correlations, 0 tests with no implementation,
0 unwitnessable targets, 32 successful correlations,
84 unwitnessed.**

**Unwitnessable = 0 is enforced, not audited.** The coverage
query's unwitnessable verdict
([Decision 22](decisions.md#decision-22),
[spec §3](spec.md#verdict-output)) statically flags any annotation
whose resolved target is not code, and the CI unwitnessable
placement gate ([#ci-wiring](#ci-wiring)) asserts the tree-wide
count stays zero on every push. Every unwitnessed row below
therefore resolves to an executable target — the precondition for
a runtime witness ever covering it.

Of the 84 unwitnessed test annotations, 78 are runtime test
annotations placed on executable targets inside
`#[test]`/`#[tokio::test]` code. They are the LCOV-pending set —
unwitnessed in a proof-only run by design (Decision 8) — and they
are Gate 3's exit criterion: when the LCOV producer lands, the
runtime rows of this table must go to zero.

The remaining 6 live in `.github/workflows/ci.yml`: test sides of
the repo/CI meta-obligations ("MUST be proven with Verus" ×4 on
the verify step; "proof files MUST carry annotations" ×2 on the
snapshot step — one test owner per quote, the duplicates check
enforces it). They are discharged by the CI run itself, which
duvet's coverage machinery cannot consume (no CI-status producer;
upstream follow-up). They persist in this table past Gate 3 until
duvet can witness CI-discharged tests.

| File | Unwitnessed (runtime, LCOV-pending) |
|---|---|
| duvet/src/query/parsers/verus_sst/tests.rs | 21 |
| duvet/src/query/witness.rs | 15 |
| duvet/src/query/producers.rs | 10 |
| duvet/src/query/result.rs | 9 |
| duvet-coverage/src/witness.rs (tests mod) | 6 |
| duvet-coverage/src/proofs.rs (tests) | 5 |
| duvet-coverage/src/scopes.rs (tests) | 3 |
| duvet/src/query/parsers/verus_sst/closure.rs | 2 |
| duvet/src/query/engine.rs | 2 |
| duvet-coverage/src/execution_propagation.rs (tests) | 2 |
| duvet-coverage/src/classify_postpass.rs (tests) | 2 |
| duvet-coverage/src/target_resolution.rs (tests) | 1 |
| **Total (runtime, LCOV-pending)** | **78** |
| .github/workflows/ci.yml (CI-discharged, persists past Gate 3) | 6 |
| **Total unwitnessed** | **84** |

## What is still missing

- **The full-run (unsliced) proof-only gate.** Runtime `type=test`
  annotations are unwitnessed in a proof-only run by design
  (Decision 8); they stay enabled, and the full-run gate is
  deferred to the LCOV follow-up (Decision 16, Follow-ups).
- **Richer CI-consumable report output** for the witness verdicts.
  The exit code + grep is the gate today; the human-oriented output
  is what CI logs show.
- **LCOV mixed-coverage producer** — the exit criterion above.
