# Duvet LCOV Parser: Design Decisions

**Date:** 2026-08-03
**Status:** Implemented

This document captures the key design decisions for the LCOV coverage
parser. Each decision follows the "cake" format: what options were
considered, what was chosen, and why. The normative requirements these
decisions produce live in [spec.md](spec.md).

---

## Decision 1: DA records only {#decision-1}

**Context:** LCOV tracefiles carry line (`DA`), function (`FN`/`FNDA`), and
branch (`BRDA`) granularities, plus test names (`TN`) and summary counters.
The duvet coverage model consumes one thing: per-line Hit/Miss/absent.

### Option A: Parse everything, expose everything

- Pro: No information loss; future consumers ready-made.
- Con: duvet has no consumer for function or branch granularity today.
  Every parsed-but-unconsumed field is untested surface that must be kept
  correct against producer variance for no benefit.

### Option B: Parse `DA` only; ignore the rest

- Pro: The parser's output domain is exactly the model's input domain.
  Smallest possible surface; every parsed field has a consumer and a test.
- Con: Branch coverage (`BRDA`) users get line granularity only.

**Chosen: Option B.** The JaCoCo parser set the precedent (it reads
`<sourcefile>/<line>` and skips `<class>`/`<method>`). If branch coverage is
ever consumed, `BRDA` parsing is added *with* its consumer, not before.
Ignored record types are enumerated normatively in
[spec.md §2](spec.md#record-consumption).

---

## Decision 2: Duplicate `DA` records merge by summing {#decision-2}

**Context:** A tracefile may contain multiple `DA` records for the same
(file, line) — within one `SF` block (some producers emit duplicates for
template/generic instantiations) or across blocks (concatenated tracefiles,
one block per test binary for the same source).

### Option A: Last record wins

- Pro: Trivial.
- Con: Order-dependent — the same multiset of records parses differently
  depending on producer ordering. A line hit in binary A and unhit in
  binary B would flap between Hit and Miss.

### Option B: Maximum count wins

- Pro: Order-independent; preserves Hit-ness.
- Con: Loses additivity — `lcov -a` (the reference merge tool) *sums*
  counts when merging tracefiles, so max diverges from the ecosystem's
  own merge semantics.

### Option C: Sum counts

- Pro: Order-independent (proved: spec.md Property 4); matches `lcov -a`
  semantics, so parsing a pre-merged file and parsing its parts agree;
  preserves Hit-ness (a hit anywhere keeps count > 0).
- Con: Counts can in principle overflow u64 (see Decision 3).

**Chosen: Option C.** Sum is the only option that is both order-independent
and consistent with the LCOV ecosystem's own merge tool.

---

## Decision 3: Saturating sum, not overflow error {#decision-3}

**Context:** Summing u64 counts can in principle exceed u64::MAX.

### Option A: Hard error on overflow

- Pro: No silent value change.
- Con: Adds a failure path no real producer can trigger — a count that
  large exceeds 10^19 executions. Every consumer must handle an error
  that cannot occur.

### Option B: Saturate at u64::MAX

- Pro: Total function; the model only distinguishes count == 0 from
  count > 0, and saturation can never cross that boundary (a saturated
  sum had nonzero addends). The Hit/Miss verdict is unaffected by
  construction.
- Con: The stored count is not the true sum in the unreachable case.

**Chosen: Option B.** Verified as spec.md Property 2: the aggregated count
equals `min(true sum, u64::MAX)`.

---

## Decision 4: Unknown record types ignored, not rejected {#decision-4}

**Context:** The LCOV format grows record types over time (`VER`, `FNL`/
`FNA` in recent geninfo). Producers differ in which they emit.

### Option A: Reject unknown records (strict mode)

- Pro: Catches typos and corruption early.
- Con: Breaks against exactly the producer variance the parser exists to
  absorb; every new geninfo release becomes a potential duvet break.

### Option B: Ignore unknown records

- Pro: Forward-compatible; matches the behavior of the ecosystem's own
  consumers.
- Con: A corrupted `DA` keyword (e.g. `DX:12,1`) is silently skipped.
- Con: Same class, different variant — a whitespace-indented record
  (`  DA:1,2`) does not match any known keyword and is silently dropped
  too. No producer indents records; a hand-edited tracefile could.

**Chosen: Option B.** The structural records the parser depends on
(`SF`, `DA`, `end_of_record`) are still strictly validated — malformed
*payloads* of known records are hard errors; unknown *keywords* are not.

---

## Decision 5: Verified aggregation core, unverified line lexer {#decision-5}

**Context:** We want machine-checked properties for this parser, but Verus
string support is too thin to verify text lexing without disproportionate
effort.

### Option A: Verify nothing (jacoco precedent)

- Pro: Least effort.
- Con: The aggregation semantics (Decision 2/3) are exactly the kind of
  invariant a future change (streaming parse, witness layer) silently
  breaks. Proofs act as static integration tests; zero is the wrong
  amount.

### Option B: Verify everything including the lexer (byte-level)

- Pro: Smallest trusted base.
- Con: Verus `String` support is thin; a byte-level (`&[u8]`) lexer proof
  is feasible but a multiple of the whole feature's cost today.

### Option C: Split at the record boundary — verified aggregation over
lexed records; lexer stays trusted glue

- Pro: The semantic content (what the map *means*) is proved
  (spec.md §7, Properties 1–4). The lexer is small, total, and testable
  empirically: exhaustive unit tests, a property-based render/lex
  round-trip, and three pinned real-producer corpora.
- Con: The lexer is trusted. A lexing bug (wrong line number parsed)
  passes the proofs.

**Chosen: Option C.** The trusted base is named in
[spec.md §8](spec.md#trusted-base).

**Performance envelope (accepted, documented):** the parser is in-memory,
per report file:

- `parse_report_blocking` reads the whole tracefile into a `String`
  (`duvet_core::vfs::read_string`), and the lexer buffers every `DA` record
  per file key before aggregation. Peak memory is O(tracefile size);
  big-monorepo tracefiles reach hundreds of MB.
- `aggregate_da_records` bisects for each record's insertion point
  (O(log n) per record) but inserts into a sorted `Vec`, and the insert
  shift is O(n) per record in the worst case. Ascending line order — what
  every observed producer emits — makes the shift empty: O(n log n) total.
  Descending input hits the shift every time: O(n²) total.

**Named follow-ups:**

- *Byte-level verified lexer* (Verus `Vec<u8>` support is adequate) if the
  lexer ever grows beyond trivial.
- *Streaming lexer*: lex incrementally instead of slurping the tracefile,
  bounding peak memory by the open block plus the accumulated records
  rather than the whole report text.
- *Verified sort-then-fold aggregation*: append all records, sort by line,
  fold each group once — O(n log n) total regardless of input order, with
  the same ensures (Properties 1–3). Property 4 licenses regrouping into
  blocks; full order-insensitivity additionally needs a
  permutation-invariance lemma for `sum_counts` (provable by induction via
  `to_multiset`). Blocked on sort machinery: vstd's `Seq::sort_by` is
  spec-level only — there is no verified *exec* sort and no assumed spec
  for `slice::sort` — so this route needs a hand-verified exec merge sort
  (a verified insertion sort would reproduce the O(n²) descending case),
  or an assumed `slice::sort` contract, which would grow the trusted base
  this crate exists to shrink.

---

## Decision 6: Path normalization is `./`-strip only {#decision-6}

**Context:** `SF:` paths vary by producer: absolute (llvm-cov,
cargo-llvm-cov), build-relative (grcov configurations), sometimes with a
leading `./`. Duvet matches report paths to source files with a single
suffix-at-`/`-boundary rule (`coverage_path_matches`), which already handles
absolute and relative paths uniformly.

### Option A: No normalization

- Pro: Verbatim keys, zero policy.
- Con: A leading `./` defeats the suffix rule: `./src/lib.rs` is not a
  suffix of `/abs/pkg/src/lib.rs`, so a real match is silently missed.

### Option B: Full normalization (resolve `..`, symlinks, absolutize)

- Pro: Robust against exotic producers.
- Con: Requires filesystem access inside the parser and invents behavior
  for paths that may not exist locally (CI-produced reports); no observed
  producer needs it.

### Option C: Strip a leading `./`, nothing else

- Pro: Fixes the one observed artifact the suffix rule cannot absorb;
  keys stay otherwise verbatim; no filesystem dependence.
- Con: `../` or embedded `/./` segments are not handled (no producer
  observed emitting them).

**Chosen: Option C.** Normative in [spec.md §5](spec.md#path-handling). If
an exotic producer surfaces, extend by decision, not by silent behavior.

---

## Decision 7: Fixture corpora are committed producer output {#decision-7}

**Context:** LCOV's interesting behavior for Rust is *which lines appear at
all*: macro-interior lines mostly absent, closing braces present, cfg'd-out
code absent, Verus ghost code absent but `proof { }` block lines inside exec
fns reported HIT.

### Option A: Hand-written minimal tracefiles only

- Pro: Small, readable, targeted.
- Con: Tests the parser against our *beliefs* about producers, not against
  producers. The absent-line behavior is precisely what hand-writing gets
  wrong.

### Option B: Committed real corpora (source + pinned `.info` from real
toolchains) plus hand-written edge cases

- Pro: Pins actual `rustc -C instrument-coverage`/llvm-cov behavior and
  actual Verus output; a toolchain behavior change surfaces as a fixture
  diff, not a silent semantic drift. The Verus `proof { }` HIT behavior is
  deliberately pinned so a future Verus change is visible.
- Con: Corpora carry absolute `SF:/tmp/...` paths and mangled symbol
  names; they are regenerable but not pretty.

**Chosen: Option B.** Corpora live in `duvet/tests/lcov-corpora/` with
their generation recipes; `SF:` values are opaque map keys to the parser,
so the `/tmp` paths in pinned files are harmless.

---

## Decision 8: Line keys are u64 end-to-end {#decision-8}

**Context:** The original coverage tables (`FileCoverage`, introduced with
the JaCoCo parser) keyed lines as `u32` — an unrecorded artifact of the
JaCoCo source (Java class files store line numbers as u16, so u32 was
"obviously enough" and nobody wrote it down). The verified coverage core
(`CoverageReport`, scopes, annotation execution) keys lines as `u64`. The
LCOV parser initially inherited the `u32`, which forced a
`u32 → u64 → u32 → u64` round-trip across the pipeline with a panicking
`try_from` in the glue, justified by a two-hop argument.

### Option A: Keep `u32` tables; narrow at the glue

- Pro: Smaller keys; no change to the pre-existing shared type.
- Con: A runtime `expect` in the trusted glue whose safety argument spans
  two modules; every future parser must re-derive it.

### Option B: Key lines as `u64` everywhere

- Pro: Deletes the panic point and every narrowing conversion; the parser
  output domain equals the verified core's domain, so conversions are
  identity or `collect()`. One less trusted assertion.
- Con: The JaCoCo `nr` parse now accepts values (≥ 2^32) that JaCoCo can
  never emit — dead leniency, harmless because nothing downstream narrows.

**Chosen: Option B.** Bugs migrate to the glue; the type change makes the
glue smaller. Normative in [spec.md §3](spec.md#da-record-syntax) (`<line>`
bounded by u64).

---

## Decision 9: Near-misses of structural keywords are hard errors {#decision-9}

**Context:** Decision 4 ignores unrecognized record types for forward
compatibility, but the original spec never defined *record* or
*recognition*, so "unrecognized" silently included *near-misses* of the two
structural keywords: `end_of_record ` (trailing space), `end_of_recordX`,
`DA1,2` (missing `:`), `DA 4,1`. For `end_of_record` the failure is
structural corruption: the block stays open, subsequent `DA` records fold
into the previous file (if the next block names the same file the counts
merge silently), and the eventual error at the next `SF:` points at the
wrong line with a misleading message. For `DA`, coverage silently vanishes
despite §3's "MUST reject a `DA` record whose payload does not conform."

A survey of the ecosystem (geninfo man page; lcov 1.16 and 2.x readers; the
Rust `lcov` crate; JS `lcov-parse`; producers geninfo, llvm-cov, grcov,
coverage.py, istanbul) found: every producer emits the exact spelling with
no trailing whitespace; lcov 2.x itself treats a non-matching line as a
fatal `format` error by default; the Rust crate hard-errors on both
near-misses; and **no implementation anywhere leaves the block open on
`end_of_record `** — the current silent-skip was unique to duvet and
strictly worse than every reference behavior.

### Option A: Keep uniform ignore semantics for every unrecognized line

- Pro: One rule, no special cases; maximally lenient.
- Con: The failure mode is silent data corruption in exactly the two record
  types the parser's output depends on, and no ecosystem implementation
  shares this behavior.

### Option B: Normalize like lcov 2.x (strip trailing whitespace, then match)

- Pro: Accepts hand-edited files the way the reference reader does.
- Con: Accepting `end_of_record ` is untested surface no producer emits —
  the same argument that pinned the §3 digit grammar. Does nothing for
  `DA1,2`, which lcov 2.x rejects anyway.

### Option C: Hard-error on records that begin with `DA` or `end_of_record`
but are not exact matches

- Pro: Converts silent corruption into a diagnostic naming the offending
  line; consistent with §3's "the grammar pins exactly what is accepted";
  matches or is stricter than every surveyed reader, and breaks no known
  producer.
- Con: A future LCOV record type whose keyword extends `DA` or
  `end_of_record` would be rejected. No such keyword exists in any known
  producer (`TN`, `SF`, `FN`, `FNDA`, `FNF`, `FNH`, `FNL`, `FNA`, `DA`,
  `BRDA`, `BRF`, `BRH`, `LF`, `LH`, `VER`); if one appears, extend by
  decision.

**Chosen: Option C.** Recognition is defined in [spec.md §1](spec.md#scope)
(a record is a line; recognition is exact, case-sensitive match), and the
rejections are normative in [spec.md §2](spec.md#record-consumption).
Genuinely different keywords (`DX:`, `VER:`, `#comment` lines from
geninfo's `--comment` flag, whitespace-indented records) remain ignored
under Decision 4.
