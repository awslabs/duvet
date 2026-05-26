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
| `executed-coverage` | Same as `coverage`, but only considers test annotations that appear in the supplied coverage data. Useful when iterating on a single test. |
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

Both flags are required for coverage checks. `--coverage-report` (short: `-r`) accepts a glob and may be repeated. `--coverage-format` (short: `-f`) currently supports `jacoco-xml`.

For each test annotation in scope, duvet determines which lines were executed during the run, finds the implementation annotations that cover those lines, and reports whether the test's claimed implementations were actually exercised.

### Two-phase coverage model

Coverage tools that operate on bytecode (e.g., JaCoCo) cannot report on source constructs that produce no executable instructions: method signatures, interface declarations, fields without initializers, and so on. Walking forward from an annotation to the next executable line breaks down whenever such a construct sits between the annotation and the code it describes.

For Java sources, duvet uses a verified two-phase coverage model from the `duvet-coverage` crate:

1. **Target resolution** — locate the source construct the annotation refers to (the next non-annotation, non-whitespace line below it).
2. **Execution propagation** — given the set of executed lines from the coverage report, derive which non-executable lines (declarations, scope openers) are transitively executed by virtue of being in a scope where some other line ran.

The algorithms are formally specified in [`design/query/coverage-model-spec.md`](https://github.com/awslabs/duvet/blob/main/design/query/coverage-model-spec.md) and proven correct with [Verus](https://verus-lang.github.io/verus/guide/). The properties cover absence of false positives, no cross-scope leakage, conservative handling of non-linear control flow (`goto`, `break` to label, etc.), and monotonicity under coverage refinement.

For source files without a language-specific classifier, the coverage check falls back to the original forward-walk over executed lines. Run with `--verbose` (`-v`) to see which path each file is using:

```console
$ duvet query -c coverage -r '**/*.xml' -f jacoco-xml --verbose
...
Coverage model: 12 file(s) using language-aware (verified) path, 3 file(s) using forward-walk fallback
  forward-walk fallback: src/main/python/loader.py
  forward-walk fallback: src/main/c/parser.c
  forward-walk fallback: src/main/rust/lib.rs
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
