// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Verified aggregation core for the LCOV coverage parser.
//!
//! The LCOV parser is split at the record boundary (see
//! `design/lcov-parser/decisions.md`, Decision 5): the line lexer in
//! `duvet/src/query/parsers/lcov.rs` turns tracefile text into per-file
//! [`DaRecord`] sequences (trusted glue, fixture- and property-tested), and
//! this module turns a record sequence into per-line summed counts — with the
//! aggregation semantics of `design/lcov-parser/spec.md` §6 machine-checked
//! (§7, Properties 1–4).
//!
//! The properties function as static integration tests: a future
//! reorganization of the parser (streaming, parallel per-block parsing,
//! producer quirks in how records are distributed across `SF` blocks) cannot
//! change the parsed coverage without failing verification here.

use verus_builtin_macros::verus;
// Ghost-only import; see the note in `lib.rs`.
#[cfg(feature = "verify")]
use vstd::prelude::*;

verus! {

/// A lexed `DA:<line>,<count>` record. The lexer guarantees `line >= 1`
/// (spec §3), but nothing in this module depends on that: the aggregation
/// properties hold for arbitrary `u64` lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DaRecord {
    pub line: u64,
    pub count: u64,
}

/// Spec: the exact (unsaturated) sum of the counts of every record for
/// `line` in `records`. Defined by recursion on the *back* of the sequence so
/// that prefix-extension steps (`take(k+1)` vs `take(k)`) unfold directly.
pub open spec fn sum_counts(records: Seq<DaRecord>, line: u64) -> nat
    decreases records.len(),
{
    if records.len() == 0 {
        0
    } else {
        sum_counts(records.drop_last(), line) + if records.last().line == line {
            records.last().count as nat
        } else {
            0
        }
    }
}

/// Spec: some record for `line` exists in `records`.
pub open spec fn has_line(records: Seq<DaRecord>, line: u64) -> bool {
    exists|i: int| 0 <= i < records.len() && records[i].line == line
}

/// Spec: saturating conversion of an exact sum to `u64`.
pub open spec fn saturate_u64(n: nat) -> u64 {
    if n > u64::MAX { u64::MAX } else { n as u64 }
}

/// Spec: `line` appears as a key in the aggregated output.
pub open spec fn contains_line(s: Seq<(u64, u64)>, line: u64) -> bool {
    exists|i: int| 0 <= i < s.len() && s[i].0 == line
}

/// Spec: output keys are strictly increasing (sorted and unique).
pub open spec fn lines_sorted(s: Seq<(u64, u64)>) -> bool {
    forall|i: int, j: int| 0 <= i < j < s.len() ==> s[i].0 < s[j].0
}

/// A record sequence with no record for `line` sums to zero for `line`.
pub proof fn lemma_sum_absent(records: Seq<DaRecord>, line: u64)
    requires
        !has_line(records, line),
    ensures
        sum_counts(records, line) == 0,
    decreases records.len(),
{
    if records.len() > 0 {
        assert(records.last().line != line) by {
            assert(records[records.len() - 1] == records.last());
        }
        assert(!has_line(records.drop_last(), line)) by {
            if has_line(records.drop_last(), line) {
                let i = choose|i: int|
                    0 <= i < records.drop_last().len() && records.drop_last()[i].line == line;
                assert(records[i] == records.drop_last()[i]);
            }
        }
        lemma_sum_absent(records.drop_last(), line);
    }
}

/// Extending a prefix by one record adds exactly that record's contribution.
pub proof fn lemma_sum_take_step(records: Seq<DaRecord>, k: int, line: u64)
    requires
        0 <= k < records.len(),
    ensures
        sum_counts(records.take(k + 1), line) == sum_counts(records.take(k), line) + if records[k].line == line {
            records[k].count as nat
        } else {
            0
        },
{
    assert(records.take(k + 1).drop_last() =~= records.take(k));
    assert(records.take(k + 1).last() == records[k]);
}

/// Extending a prefix by one record extends the line domain by exactly that
/// record's line.
pub proof fn lemma_has_line_take_step(records: Seq<DaRecord>, k: int, line: u64)
    requires
        0 <= k < records.len(),
    ensures
        has_line(records.take(k + 1), line) <==> (has_line(records.take(k), line)
            || records[k].line == line),
{
    let t1 = records.take(k + 1);
    let t0 = records.take(k);
    if has_line(t1, line) {
        let i = choose|i: int| 0 <= i < t1.len() && t1[i].line == line;
        if i < k {
            assert(t0[i] == t1[i]);
        } else {
            assert(t1[i] == records[k]);
        }
    }
    if has_line(t0, line) {
        let i = choose|i: int| 0 <= i < t0.len() && t0[i].line == line;
        assert(t1[i] == t0[i]);
    }
    if records[k].line == line {
        assert(t1[k] == records[k]);
    }
}

//= design/lcov-parser/spec.md#property-4-block-structure-invariance
//= type=implication
//# Splitting a record sequence into consecutive blocks
//# and summing per block
//# yields the same per-line totals as
//# summing the whole sequence:
//# for all record sequences `a` and `b` and every line `L`,
//# `sum(a ++ b, L) = sum(a, L) + sum(b, L)`.
/// Property 4 (Block-Structure Invariance): summing is additive over
/// concatenation, so how a producer distributes `DA` records across `SF`
/// blocks — or how a future parser batches them — cannot change per-line
/// totals. Only the multiset of records matters.
pub proof fn lemma_sum_concat(a: Seq<DaRecord>, b: Seq<DaRecord>, line: u64)
    ensures
        sum_counts(a + b, line) == sum_counts(a, line) + sum_counts(b, line),
    decreases b.len(),
{
    if b.len() == 0 {
        assert(a + b =~= a);
    } else {
        lemma_sum_concat(a, b.drop_last(), line);
        assert((a + b).drop_last() =~= a + b.drop_last());
        assert((a + b).last() == b.last());
    }
}

/// The insert branch of [`aggregate_da_records`] re-establishes its loop
/// invariants: inserting record `k`'s `(line, count)` as a *new* entry at
/// its sorted position `pos` preserves sortedness, domain soundness, count
/// correctness, and completeness for the extended prefix `take(k + 1)`.
///
/// The `requires` are exactly what the exec code knows at the insert: the
/// binary search's position facts — strictly smaller left of `pos`,
/// strictly greater from `pos` on, so the line is genuinely new — plus the
/// loop invariants for `take(k)`.
proof fn lemma_insert_reestablishes_invariants(
    records: Seq<DaRecord>,
    k: int,
    old_out: Seq<(u64, u64)>,
    pos: int,
)
    requires
        0 <= k < records.len(),
        0 <= pos <= old_out.len(),
        lines_sorted(old_out),
        forall|i: int| 0 <= i < pos ==> (#[trigger] old_out[i]).0 < records[k].line,
        forall|i: int| pos <= i < old_out.len() ==> (#[trigger] old_out[i]).0 > records[k].line,
        forall|i: int|
            0 <= i < old_out.len() ==> has_line(records.take(k), (#[trigger] old_out[i]).0),
        forall|i: int|
            0 <= i < old_out.len() ==> (#[trigger] old_out[i]).1 == saturate_u64(
                sum_counts(records.take(k), old_out[i].0),
            ),
        forall|l: u64| #[trigger] has_line(records.take(k), l) ==> contains_line(old_out, l),
    ensures
        ({
            let new_out = old_out.insert(pos, (records[k].line, records[k].count));
            let prefix1 = records.take(k + 1);
            &&& lines_sorted(new_out)
            &&& forall|i: int|
                0 <= i < new_out.len() ==> has_line(prefix1, (#[trigger] new_out[i]).0)
            &&& forall|i: int|
                0 <= i < new_out.len() ==> (#[trigger] new_out[i]).1 == saturate_u64(
                    sum_counts(prefix1, new_out[i].0),
                )
            &&& forall|l: u64| #[trigger] has_line(prefix1, l) ==> contains_line(new_out, l)
        }),
{
    let r_line = records[k].line;
    let r_count = records[k].count;
    let prefix = records.take(k);
    let prefix1 = records.take(k + 1);
    let new_out = old_out.insert(pos, (r_line, r_count));

    // r_line is genuinely new: no old entry holds it (position facts), so
    // no record for it exists in the prefix (completeness, contrapositive)
    // and its prefix sum is zero — record k alone contributes.
    assert(!contains_line(old_out, r_line)) by {
        if contains_line(old_out, r_line) {
            let m = choose|m: int| 0 <= m < old_out.len() && old_out[m].0 == r_line;
            assert(old_out[m].0 != r_line);
        }
    }
    assert(!has_line(prefix, r_line));
    lemma_sum_absent(prefix, r_line);
    lemma_sum_take_step(records, k, r_line);
    assert(sum_counts(prefix1, r_line) == r_count as nat);

    // Index layout of the inserted sequence.
    assert forall|j: int| 0 <= j < pos implies new_out[j] == old_out[j] by {}
    assert(new_out[pos] == (r_line, r_count));
    assert forall|j: int| pos < j < new_out.len() implies new_out[j] == old_out[j - 1] by {}

    assert(lines_sorted(new_out)) by {
        assert forall|i: int, j: int| 0 <= i < j < new_out.len() implies new_out[i].0
            < new_out[j].0 by {
            // Case split on which side of pos each index falls; same-side
            // pairs follow from old sortedness through the index layout.
            if i < pos && j == pos {
                assert(new_out[i].0 == old_out[i].0 && old_out[i].0 < r_line);
            } else if i == pos && j > pos {
                assert(pos <= j - 1 < old_out.len());
                assert(new_out[j].0 == old_out[j - 1].0 && old_out[j - 1].0 > r_line);
            } else if i < pos && j > pos {
                assert(new_out[i].0 == old_out[i].0 && old_out[i].0 < r_line);
                assert(pos <= j - 1 < old_out.len());
                assert(new_out[j].0 == old_out[j - 1].0 && old_out[j - 1].0 > r_line);
            }
        }
    }

    assert forall|i: int| 0 <= i < new_out.len() implies has_line(
        prefix1,
        (#[trigger] new_out[i]).0,
    ) by {
        lemma_has_line_take_step(records, k, new_out[i].0);
    }

    assert forall|i: int| 0 <= i < new_out.len() implies (#[trigger] new_out[i]).1
        == saturate_u64(sum_counts(prefix1, new_out[i].0)) by {
        if i == pos {
            assert(new_out[i] == (r_line, r_count));
            assert(r_count as nat <= u64::MAX);
        } else {
            let oi = if i < pos { i } else { i - 1 };
            assert(new_out[i] == old_out[oi]);
            // Every surviving entry has a different line (position facts),
            // so record k leaves its sum unchanged.
            assert(old_out[oi].0 != r_line);
            lemma_sum_take_step(records, k, old_out[oi].0);
        }
    }

    assert forall|l: u64| #[trigger] has_line(prefix1, l) implies contains_line(new_out, l) by {
        lemma_has_line_take_step(records, k, l);
        if l == r_line {
            assert(new_out[pos].0 == l);
        } else {
            assert(has_line(prefix, l));
            let m = choose|m: int| 0 <= m < old_out.len() && old_out[m].0 == l;
            if m < pos {
                assert(new_out[m].0 == l);
            } else {
                assert(new_out[m + 1].0 == l);
            }
        }
    }
}

/// Aggregate a file's lexed `DA` records into per-line saturating-summed
/// counts (spec §6), returning `(line, count)` pairs strictly sorted by line.
///
/// The `ensures` are spec §7 Properties 1–3:
/// - Property 1 (Domain Exactness): output lines are exactly the input's
///   record lines — nothing invented, nothing dropped.
/// - Property 2 (Count Correctness): each output count is the saturated
///   exact sum for its line.
/// - Property 3 (Ordered Uniqueness): strictly sorted keys, so the caller's
///   ordered-map conversion cannot silently merge or reorder.
pub fn aggregate_da_records(records: &[DaRecord]) -> (out: Vec<(u64, u64)>)
    ensures
        //= design/lcov-parser/spec.md#property-3-ordered-uniqueness
        //= type=implication
        //# The aggregated output is strictly sorted by line number,
        //# so each line appears exactly once.
        lines_sorted(out@),
        //= design/lcov-parser/spec.md#property-1-domain-exactness
        //= type=implication
        //# A line number appears in the aggregated output
        //# if and only if
        //# at least one `DA` record for that line
        //# appears in the input record sequence.
        forall|i: int| 0 <= i < out@.len() ==> has_line(records@, (#[trigger] out@[i]).0),
        //= design/lcov-parser/spec.md#property-2-count-correctness
        //= type=implication
        //# The count aggregated for a line
        //# equals the sum of the counts of every input record
        //# for that line,
        //# saturated at the maximum unsigned 64-bit value.
        forall|i: int|
            0 <= i < out@.len() ==> (#[trigger] out@[i]).1 == saturate_u64(
                sum_counts(records@, out@[i].0),
            ),
        forall|l: u64| #[trigger] has_line(records@, l) ==> contains_line(out@, l),
{
    let mut out: Vec<(u64, u64)> = Vec::new();
    let mut k: usize = 0;

    while k < records.len()
        invariant
            k <= records.len(),
            lines_sorted(out@),
            forall|i: int|
                0 <= i < out@.len() ==> has_line(
                    records@.take(k as int),
                    (#[trigger] out@[i]).0,
                ),
            forall|i: int|
                0 <= i < out@.len() ==> (#[trigger] out@[i]).1 == saturate_u64(
                    sum_counts(records@.take(k as int), out@[i].0),
                ),
            forall|l: u64| #[trigger] has_line(records@.take(k as int), l) ==> contains_line(
                out@,
                l,
            ),
        decreases records.len() - k,
    {
        //= design/lcov-parser/spec.md#aggregation
        //= type=implementation
        //# The count recorded for a line MUST be the sum of the
        //# counts of every `DA` record for that line in every
        //# source-file block naming that file.
        let r_line = records[k].line;
        let r_count = records[k].count;
        proof {
            assert(records@[k as int].line == r_line);
            assert(records@[k as int].count == r_count);
        }

        // Find the insertion/update position: the first index whose line is
        // >= r_line. Binary search: `out` is strictly sorted (loop invariant
        // `lines_sorted`), so `out[i].0 < r_line` is monotone in `i` and the
        // standard lo/hi bisection applies. Complexity note: bisection is
        // O(log n) per record, but the `Vec::insert` below shifts O(n) per
        // record in the worst case, so the shift — not the search — bounds
        // the total. Real producers emit `DA` records in ascending line
        // order, so the insertion point is the end, the shift is empty, and
        // the common case is O(n log n) total. *Descending* input shifts the
        // whole vector on every insert: O(n^2) total. No observed producer
        // emits descending records; if one surfaces, the fix is the
        // sort-then-fold reorganization named in decisions.md Decision 5
        // follow-ups, not further tuning here.
        let mut lo: usize = 0;
        let mut hi: usize = out.len();
        while lo < hi
            invariant
                lo <= hi <= out@.len(),
                lines_sorted(out@),
                forall|i: int| 0 <= i < lo as int ==> (#[trigger] out@[i]).0 < r_line,
                forall|i: int| hi as int <= i < out@.len() ==> (#[trigger] out@[i]).0 >= r_line,
            decreases hi - lo,
        {
            let mid = lo + (hi - lo) / 2;
            if out[mid].0 < r_line {
                proof {
                    // Everything at or left of mid is < r_line by sortedness.
                    assert forall|i: int| 0 <= i < mid as int + 1 implies (#[trigger] out@[i]).0
                        < r_line by {
                        if i < mid as int {
                            assert(out@[i].0 < out@[mid as int].0);
                        }
                    }
                }
                lo = mid + 1;
            } else {
                proof {
                    // Everything at or right of mid is >= r_line by sortedness.
                    assert forall|i: int| mid as int <= i < out@.len() implies (#[trigger] out@[i]).0
                        >= r_line by {
                        if i > mid as int {
                            assert(out@[mid as int].0 < out@[i].0);
                        }
                    }
                }
                hi = mid;
            }
        }
        let pos = lo;

        let ghost old_out = out@;
        let ghost prefix = records@.take(k as int);
        let ghost prefix1 = records@.take(k as int + 1);

        if pos < out.len() && out[pos].0 == r_line {
            // Existing entry: saturating add.
            //= design/lcov-parser/spec.md#aggregation
            //= type=implementation
            //# The sum MUST saturate at the maximum unsigned 64-bit
            //# value instead of overflowing.
            let cur = out[pos].1;
            let new_count = if cur > u64::MAX - r_count {
                u64::MAX
            } else {
                cur + r_count
            };
            proof {
                lemma_sum_take_step(records@, k as int, r_line);
                let s = sum_counts(prefix, r_line);
                assert(cur == saturate_u64(s));
                assert(sum_counts(prefix1, r_line) == s + r_count as nat);
                assert(new_count == saturate_u64(s + r_count as nat)) by {
                    if s > u64::MAX {
                        assert(cur == u64::MAX);
                    } else {
                        assert(cur == s as u64);
                    }
                }
            }
            out[pos] = (r_line, new_count);
            proof {
                // Every entry other than `pos` has a different line (strict
                // sortedness), so its sum is unchanged by record k.
                assert forall|i: int| 0 <= i < out@.len() implies (#[trigger] out@[i]).1
                    == saturate_u64(sum_counts(prefix1, out@[i].0)) by {
                    if i != pos as int {
                        assert(out@[i] == old_out[i]);
                        assert(old_out[i].0 != r_line) by {
                            if i < pos as int {
                                assert(old_out[i].0 < old_out[pos as int].0);
                            } else {
                                assert(old_out[pos as int].0 < old_out[i].0);
                            }
                        }
                        lemma_sum_take_step(records@, k as int, old_out[i].0);
                    }
                }
                assert forall|i: int| 0 <= i < out@.len() implies has_line(
                    prefix1,
                    (#[trigger] out@[i]).0,
                ) by {
                    lemma_has_line_take_step(records@, k as int, out@[i].0);
                    if i != pos as int {
                        assert(out@[i] == old_out[i]);
                    }
                }
                assert forall|l: u64| #[trigger] has_line(prefix1, l) implies contains_line(
                    out@,
                    l,
                ) by {
                    lemma_has_line_take_step(records@, k as int, l);
                    if has_line(prefix, l) {
                        let i = choose|i: int| 0 <= i < old_out.len() && old_out[i].0 == l;
                        assert(out@[i].0 == l);
                    } else {
                        assert(out@[pos as int].0 == l);
                    }
                }
                assert(lines_sorted(out@)) by {
                    assert forall|i: int, j: int| 0 <= i < j < out@.len() implies out@[i].0
                        < out@[j].0 by {
                        assert(out@[i].0 == old_out[i].0);
                        assert(out@[j].0 == old_out[j].0);
                    }
                }
            }
        } else {
            // No entry for r_line exists: `pos` is its sorted insertion
            // point. Invariant maintenance is
            // `lemma_insert_reestablishes_invariants`; here we only supply
            // its strictness precondition (the search gives >= at and right
            // of `pos`; equality at `pos` is excluded by this branch's
            // test, and beyond `pos` by sortedness through `pos`).
            proof {
                assert forall|i: int| pos as int <= i < old_out.len() implies (#[trigger] old_out[i]).0
                    > r_line by {
                    if i > pos as int {
                        assert(old_out[pos as int].0 < old_out[i].0);
                    }
                }
                lemma_insert_reestablishes_invariants(records@, k as int, old_out, pos as int);
            }
            out.insert(pos, (r_line, r_count));
            proof {
                assert(out@ =~= old_out.insert(pos as int, (r_line, r_count)));
            }
        }

        k = k + 1;
    }

    proof {
        assert(records@.take(records@.len() as int) =~= records@);
    }
    out
}

} // verus!
