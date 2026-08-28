# Query

Duvet's `query` command is a development-time tool that answers focused questions about the project's traceability state. Where [`report`](./reports.md) produces the full artifact for CI, `query` is a fast feedback loop for the work in progress.

## Checks

Each question `query` answers is a check, selected with `--check` (short: `-c`). Multiple checks can be combined in one invocation:

```console
$ duvet query --check implementation
$ duvet query -c implementation,test
$ duvet query -c implementation -c test
```

| Check | Question |
|-------|----------|
| `implementation` | Do all in-scope specification requirements have implementation annotations? |
| `test` | Do all in-scope implementation annotations have corresponding test annotations? |
| `coverage` | Do test annotations actually execute their corresponding implementation annotations? |
| `executed-coverage` | Same as `coverage`, but skips test annotations the supplied coverage data shows as not executed. Useful when iterating on a single test. |
| `duplicates` | Are there annotations of the same type that redundantly cover the same specification text? |

Annotations of type `citation`, `implication`, and `exception` count as implementation. `todo` annotations are tracked separately — a requirement covered only by `todo` annotations is reported as "TODO only" by the `implementation` check, not as implemented.

## Scope filters

By default each check considers every annotation in the project. Two flags narrow the scope:

* `--section <PATH[#SECTION]>` (short: `-s`, repeatable, comma-separated): restrict to annotations targeting specific specification sections.
* `--quote <TEXT>` (short: `-q`, repeatable): restrict to annotations whose quoted text contains the given substring (case-insensitive).

```console
$ duvet query -c implementation -s spec.md
$ duvet query -c implementation -s 'spec.md#section-1'
$ duvet query -c test -q 'MUST encrypt'
$ duvet query -c implementation,test -s spec.md -q 'header'
```

## Coverage checks

The `coverage` and `executed-coverage` checks correlate test annotations with executed implementation annotations using a coverage report:

```console
$ duvet query -c coverage \
    --coverage-report 'target/jacoco/*.xml' \
    --coverage-format jacoco-xml
```

Both flags are required for coverage checks. `--coverage-report` (short: `-r`) accepts a glob and may be repeated. `--coverage-format` (short: `-f`) supports `jacoco-xml` and `lcov`.

The `lcov` format reads LCOV tracefiles (`.info`), as produced by `llvm-cov export --format=lcov` (and its wrappers `cargo-llvm-cov` and `grcov`) for Rust/C/C++, `coverage.py` + `lcov` toolchains for Python, and `geninfo` generally. Only `DA` (per-line execution count) records are consumed; function and branch records are ignored, and a line absent from the tracefile is treated as "no opinion" rather than a miss. The full behavior is specified in [`design/lcov-parser/spec.md`](https://github.com/awslabs/duvet/blob/main/design/lcov-parser/spec.md), and the aggregation semantics are machine-checked with Verus.

**Produce one tracefile per test.** The coverage check discharges a (test, implementation) pair only when a *single* report shows both executed, so each report should be one measurement: one test, run in its own process, exporting its own tracefile. A single aggregate tracefile over the whole suite weakens every correlation to "both are covered by the suite, somewhere, by something." For Rust, run each test binary's tests individually under `RUSTFLAGS="-C instrument-coverage"` with a unique `LLVM_PROFILE_FILE` per test, then `llvm-profdata merge` and `llvm-cov export --format=lcov` per test (duvet's own CI does exactly this; see `cargo xtask witnesses` in the duvet repository). Pass the resulting files with a glob — `-r 'witnesses/*.info'` — so each file enters the check as its own witness. Never merge or concatenate the witness files: duvet's parser sums counts across all blocks in a file, so a merged file provably collapses the per-test partition back to suite-level co-coverage (see [`design/lcov-parser/decisions.md` Decision 12](https://github.com/awslabs/duvet/blob/main/design/lcov-parser/decisions.md)).

Report paths (`SF:` payloads) are matched against your source files by path suffix at `/` boundaries: an absolute report path like `/build/pkg/src/lib.rs` matches the source file `src/lib.rs` and vice versa, as long as the components agree. The one normalization applied is stripping a leading `./` from the report path; anything else — embedded `./` segments, `..`, stray whitespace — is kept verbatim and will prevent a match, so if a report unexpectedly shows no coverage for a file, compare the `SF:` line against the source path component by component.

For each test annotation in scope, duvet determines which lines were executed during the run, finds the implementation annotations that cover those lines, and reports whether the test's claimed implementations were actually exercised.

### Two-phase coverage model

Coverage tools that operate on bytecode (e.g., JaCoCo) cannot report on source constructs that produce no executable instructions: method signatures, interface declarations, fields without initializers, and so on. Walking forward from an annotation to the next executable line breaks down whenever such a construct sits between the annotation and the code it describes.

For Java sources, duvet uses a verified two-phase coverage model from the `duvet-coverage` crate:

1. **Target resolution** — locate the source construct the annotation refers to (the next non-annotation, non-whitespace line below it).
2. **Execution propagation** — given the set of executed lines from the coverage report, derive which non-executable lines (declarations, scope openers) are transitively executed by virtue of being in a scope where some other line ran.

The algorithms are formally specified in [`design/query/coverage-model-spec.md`](https://github.com/awslabs/duvet/blob/main/design/query/coverage-model-spec.md) and proven correct with [Verus](https://verus-lang.github.io/verus/guide/). The properties cover absence of false positives, no cross-scope leakage, conservative handling of non-linear control flow (`goto`, `break` to label, etc.), and monotonicity under coverage refinement.

For source files without a language-specific classifier, the coverage check uses a verified degraded model: the annotation resolves to its target line and status is read directly from the coverage report at that line, with no propagation. This is lower-fidelity than the two-phase model — constructs invisible to the coverage tool (declarations, signatures) cannot be credited — but its properties are proven with the same Verus machinery. Run with `--verbose` (`-v`) to see which path each file is using:

```console
$ duvet query -c coverage -r '**/*.xml' -f jacoco-xml --verbose
...
Coverage model: 12 file(s) language-aware (verified), 3 file(s) degraded — no classifier (verified)
  degraded (no classifier, verified): src/main/python/loader.py
  degraded (no classifier, verified): src/main/c/parser.c
  degraded (no classifier, verified): src/main/rust/lib.rs
```

## Verbose output

`--verbose` (short: `-v`) shows passing checks in full detail (matched annotations, source slices) in addition to the failures that are always shown. Use it during development when you want to inspect the matched state, not just the diagnostic for what's missing.

## Exit code

`duvet query` exits 0 when every requested check passes, and 1 otherwise. The command is suitable for git hooks or per-commit validation; for CI gates, prefer [`duvet report --ci`](./reports.md#snapshot), which uses the snapshot mechanism for stability across refactors.

## Relationship to `duvet report`

`query` and `report` are complementary:

| | `duvet query` | `duvet report` |
|--|---------------|----------------|
| Purpose | Interactive feedback during development | Authoritative artifact for CI |
| Output | Diagnostics on stderr, exit code | HTML / JSON / snapshot files |
| Filtering | `--section`, `--quote`, `--check` | All requirements always |
| Coverage check | Yes (`--check coverage`) | Not yet |
| CI gating | Use exit code for fast checks | Snapshot via `--ci` for stability |

`query` is the right command for "have I annotated this requirement yet?" `report` is the right command for "is the project's traceability state still acceptable?"
