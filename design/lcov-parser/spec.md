# Duvet LCOV Coverage Parser: Specification

**Version:** 1.0.0
**Date:** 2026-08-03

The key words "MUST", "MUST NOT", "SHOULD", "SHOULD NOT", and "MAY" in this
document are to be interpreted as described in
[RFC 2119](https://www.rfc-editor.org/rfc/rfc2119).

## 1. Scope {#scope}

LCOV tracefiles (`.info` files, as produced by `geninfo`, `llvm-cov export
--format=lcov`, `cargo-llvm-cov`, and `grcov`) are line-oriented text: each
line is a record of the form `KEYWORD:payload`, and records are grouped into
per-source-file blocks delimited by `SF:` and `end_of_record`.

Terminology used throughout this specification: a *record* is a single line
of the tracefile, taken after line-terminator handling (Section 4) and
excluding blank lines. The parser *recognizes* a record by exact,
case-sensitive match on its leading characters: a record beginning with
`SF:` is an `SF` record, a record beginning with `DA:` is a `DA` record, a
record whose entire content is `end_of_record` is an `end_of_record` record,
and a record beginning with `KEYWORD:` for one of the keywords enumerated in
Section 2 is a record of that type. There is no case folding, no whitespace
trimming, and no prefix matching beyond these exact forms: `da:1,2`,
`  DA:1,2` (leading whitespace), and `DA 1,2` (missing `:`) are not `DA`
records.

This specification defines how duvet parses an LCOV tracefile into per-file,
per-line coverage data for the coverage model defined in
[coverage-model-spec.md](../query/coverage-model-spec.md). It covers record
selection, record syntax, report structure, path handling, and the
aggregation semantics — including the correctness properties the aggregation
core is verified against.

The parser's output feeds the same `CoverageReport` consumed by both the
classified and degraded verified models; the absent-line semantics of
[coverage-model-spec.md §1.4](../query/coverage-model-spec.md#coverage-data)
(a line the report has no opinion about is *unknown*, never a miss) therefore
begin at this parser.

## 2. Record Consumption {#record-consumption}

The parser MUST consume `DA` records.

The parser MUST ignore records of the following types:
`TN`, `FN`, `FNDA`, `FNF`, `FNH`, `BRDA`, `BRF`, `BRH`, `LF`, `LH`.

The parser MUST ignore records of types it does not recognize.

The parser MUST reject a record that begins with `DA`
but is not a `DA` record.

The parser MUST reject a record that begins with
`end_of_record` but is not an `end_of_record` record.

Rationale: `DA` is the only record type that carries per-line execution
counts, which is the only granularity the coverage model consumes. Function
(`FN*`) and branch (`BRDA`, `BR*`) records carry granularities the model does
not use; summary records (`LF`, `LH`) are derivable and untrusted. Unknown
record types are ignored rather than rejected because the LCOV ecosystem
grows record types over time (e.g. `VER`, `FNL`/`FNA` in recent geninfo) and
a strict parser would break against exactly the producer variance this parser
exists to absorb (see
[decisions.md, Decision 4](decisions.md#decision-4)).

Rationale for the near-miss rejections: `DA` and `end_of_record` are the two
record types the parser's output and block structure depend on, so a
near-miss of one of them is producer error or corruption, not a future
record type — no known LCOV record keyword extends `DA` or `end_of_record`,
and every surveyed producer emits the exact spelling (see
[decisions.md, Decision 9](decisions.md#decision-9)). Ignoring such a line
is not forward compatibility but silent structural corruption: an
`end_of_record` followed by a trailing space would leave its block open,
folding the next block's `DA` records into the wrong file; a `DA` record
missing its `:` would silently drop coverage that Section 3 promises to
either consume or reject.

## 3. DA Record Syntax {#da-record-syntax}

A `DA` record has the form:

```
DA:<line>,<count>[,<checksum>]
```

The parser MUST parse `<line>` as a decimal integer
greater than or equal to 1
and representable in an unsigned 64-bit integer.

The parser MUST parse `<count>` as a decimal integer
representable in an unsigned 64-bit integer.

The `<line>` and `<count>` fields MUST consist solely of
ASCII digits `0`-`9`.

The parser MUST accept and ignore the optional third
checksum field.

The parser MUST reject a `DA` record whose payload does
not conform to this syntax.

Rationale for the `<line>` bounds: LCOV line numbers are 1-based, so `0` is
producer error, and duvet keys lines as `u64` throughout the coverage
pipeline, so every parseable value is representable end-to-end with no
narrowing conversion; a value above 2^64 - 1 cannot be represented and is
rejected rather than silently truncated.

Rationale for the digit grammar: general-purpose integer parsers accept
more than producers emit (Rust's `str::parse` accepts a leading `+`).
No LCOV producer emits signed fields, so accepting them would be untested
surface with no consumer; the grammar pins exactly what is accepted.
Leading zeros consist of digits, so they conform and carry their numeric
value.

## 4. Report Structure {#report-structure}

An `SF:` record opens a source-file block; `end_of_record` closes it.

The parser MUST reject an `SF:` record that appears while
a source-file block is already open.

The parser MUST reject a `DA` record that appears outside
a source-file block.

The parser MUST reject an `end_of_record` record that
appears outside a source-file block.

The parser MUST accept end-of-input while a source-file
block is open, treating it as an implicit `end_of_record`.

The parser MUST ignore blank lines.

The parser MUST accept both LF and CRLF line endings.

The parser MUST reject input that is not valid UTF-8.

Rationale for the encoding rule: every known producer emits ASCII record
keywords and UTF-8 (in practice, almost always ASCII) paths. The parser
reads the tracefile as UTF-8 text, so non-UTF-8 input surfaces as an I/O
error at the offending line rather than being lossily decoded into a key
that can never match a source file.

## 5. Path Handling {#path-handling}

The `SF:` payload names the source file. Producers differ: raw
`llvm-cov export` and `cargo-llvm-cov` emit absolute paths; `grcov` and
`geninfo` configurations can emit build-relative paths, sometimes with a
leading `./`.

The parser MUST use the `SF:` payload verbatim as the
file key, except that a leading `./` MUST be stripped.

The parser MUST reject an `SF:` record whose payload is
empty after stripping.

No further normalization is performed. Matching a file key against duvet's
source files is the responsibility of the suffix-boundary rule in the
coverage check (`coverage_path_matches`), which already handles both absolute
and relative report paths; a leading `./` is the one producer artifact that
rule cannot absorb (see
[decisions.md, Decision 6](decisions.md#decision-6)).

## 6. Aggregation {#aggregation}

The parsed coverage for a file is a map from line number to execution count.

The count recorded for a line MUST be the sum of the
counts of every `DA` record for that line in every
source-file block naming that file.

The sum MUST saturate at the maximum unsigned 64-bit
value instead of overflowing.

A line with no `DA` record MUST be absent from the parsed
coverage for its file.

The parsed coverage MUST contain no branch data.

The absent-line requirement is load-bearing: downstream, an absent line is
"coverage has no opinion" (`Unknown`), while a present line with count 0 is a
definite `Miss`. Recording absent lines as misses would manufacture false
"not executed" verdicts; dropping recorded lines would hide real ones. See
[coverage-model-spec.md §1.4](../query/coverage-model-spec.md#coverage-data).

## 7. Aggregation Properties {#aggregation-properties}

The aggregation semantics of Section 6 are implemented by a verified core
(`duvet_coverage::lcov::aggregate_da_records`) operating on the lexed record
sequence. The following properties are machine-checked with Verus.

### Property 1: Domain Exactness {#property-1-domain-exactness}

A line number appears in the aggregated output
if and only if
at least one `DA` record for that line
appears in the input record sequence.

The parser never invents a line (which would manufacture a false `Miss` or
`Hit`) and never drops one (which would hide a real verdict as `Unknown`).

### Property 2: Count Correctness {#property-2-count-correctness}

The count aggregated for a line
equals the sum of the counts of every input record
for that line,
saturated at the maximum unsigned 64-bit value.

### Property 3: Ordered Uniqueness {#property-3-ordered-uniqueness}

The aggregated output is strictly sorted by line number,
so each line appears exactly once.

This makes the conversion from the verified core's output into the ordered
map consumed by the coverage check structurally trivial — the conversion glue
cannot silently merge or reorder entries.

### Property 4: Block-Structure Invariance {#property-4-block-structure-invariance}

Splitting a record sequence into consecutive blocks
and summing per block
yields the same per-line totals as
summing the whole sequence:
for all record sequences `a` and `b` and every line `L`,
`sum(a ++ b, L) = sum(a, L) + sum(b, L)`.

This is the formal statement that *how* a producer distributes `DA` records
across `SF` blocks cannot change the parsed coverage — only the multiset of
records matters. A future streaming or parallel re-organization of the parser
must preserve exactly this lemma.

## 8. Trusted Base {#trusted-base}

The verified core covers aggregation (Section 6 semantics over an
already-lexed record sequence). The remaining components are trusted glue,
each named here with its mitigation:

- **Line lexer** (`duvet/src/query/parsers/lcov.rs`): text → record
  sequence per file (Sections 2–5). Not verified — Verus string support is
  too thin to earn its keep today. Mitigated by exhaustive unit tests, a
  property-based render/lex round-trip test, a raw-byte totality fuzz test
  (arbitrary input must return `Ok` or `Err`, never panic), and three
  pinned real-producer corpora. A byte-level verified lexer is a named
  follow-up ([decisions.md, Decision 5](decisions.md#decision-5)).
- **Per-file grouping**: routing lexed records to their `SF:` file key.
  Exercised by the same tests.
- **Ordered-map conversion**: verified-core output → `BTreeMap`. Trivial by
  Property 3; unit-tested.
- **`FileCoverage::to_coverage_report`**: shared with the JaCoCo parser;
  Hit-priority merge, unit-tested where it is defined.
- **Verus toolchain** (verifier, Z3, vstd): trusted.
