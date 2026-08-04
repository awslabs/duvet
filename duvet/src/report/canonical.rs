// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Segment canonicalization whose correctness is checked by Verus.

#![allow(
    unused_imports,
    unused_variables,
    unused_parens,
    dead_code,
    clippy::assign_op_pattern
)]

use vstd::{assert_seqs_equal, prelude::*};

verus! {

/// A half-open byte span carrying one interned annotation-set label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SegmentSpan {
    pub start: usize,
    pub end: usize,
    pub label: usize,
}

/// The spans are non-empty, contiguous, and begin at `offset`.
pub open spec fn spans_well_formed_at(spans: Seq<SegmentSpan>, offset: usize) -> bool {
    &&& forall|i: int| 0 <= i < spans.len() ==>
        (#[trigger] spans[i]).start < spans[i].end
    &&& (spans.len() > 0 ==> spans[0].start == offset)
    &&& forall|i: int| 0 < i < spans.len() ==>
        (#[trigger] spans[i - 1]).end == (#[trigger] spans[i]).start
}

/// The public byte-zero invariant used by line canonicalization.
pub open spec fn spans_well_formed(spans: Seq<SegmentSpan>) -> bool {
    spans_well_formed_at(spans, 0)
}

/// A canonical sequence has no boundary between equal labels.
pub open spec fn coalesced(spans: Seq<SegmentSpan>) -> bool {
    forall|i: int| 0 < i < spans.len() ==>
        (#[trigger] spans[i - 1]).label != (#[trigger] spans[i]).label
}

pub open spec fn repeated_label(label: usize, len: nat) -> Seq<usize> {
    Seq::new(len, |i: int| label)
}

/// Per-byte annotation coverage, with segment cuts erased. This "unpacks" the
/// compressed span representation into one label entry per covered byte, so two
/// span lists denote the same sequence exactly when they mean the same thing.
/// It is the semantic ground truth the proofs preserve across canonicalization.
pub open spec fn denote(spans: Seq<SegmentSpan>) -> Seq<usize>
    decreases spans.len(),
{
    if spans.len() == 0 {
        Seq::empty()
    } else {
        let span = spans.last();
        denote(spans.drop_last()) + repeated_label(
            span.label,
            (span.end as int - span.start as int) as nat,
        )
    }
}

/// Equivalent front-recursive denotation, useful when proving uniqueness of
/// maximal runs.
pub open spec fn denote_forward(spans: Seq<SegmentSpan>) -> Seq<usize>
    decreases spans.len(),
{
    if spans.len() == 0 {
        Seq::empty()
    } else {
        let span = spans.first();
        repeated_label(
            span.label,
            (span.end as int - span.start as int) as nat,
        ) + denote_forward(spans.drop_first())
    }
}

proof fn lemma_denote_forward_push(spans: Seq<SegmentSpan>, span: SegmentSpan)
    ensures
        denote_forward(spans.push(span)) == denote_forward(spans)
            + repeated_label(span.label, (span.end as int - span.start as int) as nat),
    decreases spans.len(),
{
    if spans.len() == 0 {
        reveal_with_fuel(denote_forward, 2);
        assert(spans.push(span).first() == span);
        assert(spans.push(span).drop_first() =~= Seq::<SegmentSpan>::empty());
        assert(denote_forward(spans) =~= Seq::<usize>::empty());
        assert(denote_forward(spans.push(span))
            =~= repeated_label(span.label, (span.end as int - span.start as int) as nat));
    } else {
        lemma_denote_forward_push(spans.drop_first(), span);
        reveal_with_fuel(denote_forward, 2);
        assert((spans.push(span)).first() == spans.first());
        assert((spans.push(span)).drop_first() =~= spans.drop_first().push(span));
        vstd::seq_lib::lemma_concat_associative(
            repeated_label(
                spans.first().label,
                (spans.first().end as int - spans.first().start as int) as nat,
            ),
            denote_forward(spans.drop_first()),
            repeated_label(span.label, (span.end as int - span.start as int) as nat),
        );
        assert(denote_forward(spans.push(span))
            == repeated_label(
                    spans.first().label,
                    (spans.first().end as int - spans.first().start as int) as nat,
                )
                + (denote_forward(spans.drop_first())
                    + repeated_label(
                        span.label,
                        (span.end as int - span.start as int) as nat,
                    )));
    }
}

proof fn lemma_denote_forward(spans: Seq<SegmentSpan>)
    ensures
        denote_forward(spans) == denote(spans),
    decreases spans.len(),
{
    reveal(spans_well_formed_at);
    reveal(Seq::drop_first);
    if spans.len() == 0 {
        reveal(denote_forward);
        reveal(denote);
    } else {
        let prefix = spans.drop_last();
        let span = spans.last();
        lemma_denote_forward(prefix);
        lemma_denote_forward_push(prefix, span);
        reveal_with_fuel(denote, 2);
        assert(spans =~= prefix.push(span));
    }
}

proof fn lemma_well_formed_drop_first(spans: Seq<SegmentSpan>, offset: usize)
    requires
        spans_well_formed_at(spans, offset),
        spans.len() > 0,
    ensures
        spans_well_formed_at(spans.drop_first(), spans.first().end),
{
    assert forall|i: int| 0 <= i < spans.drop_first().len() implies
        (#[trigger] spans.drop_first()[i]).start < spans.drop_first()[i].end by {
        assert(spans.drop_first()[i] == spans[i + 1]);
    }
    if spans.drop_first().len() > 0 {
        assert(spans[1 - 1].end == spans[1].start);
        assert(spans.drop_first()[0] == spans[1]);
        assert(spans.drop_first()[0].start == spans.first().end);
    }
    assert forall|i: int| 0 < i < spans.drop_first().len() implies
        (#[trigger] spans.drop_first()[i - 1]).end
            == (#[trigger] spans.drop_first()[i]).start by {
        assert(spans[(i + 1) - 1].end == spans[i + 1].start);
        assert(spans.drop_first()[i - 1] == spans[i]);
        assert(spans.drop_first()[i] == spans[i + 1]);
    }
}

proof fn lemma_coalesced_drop_first(spans: Seq<SegmentSpan>)
    requires
        coalesced(spans),
        spans.len() > 0,
    ensures
        coalesced(spans.drop_first()),
{
    reveal(coalesced);
    reveal(Seq::drop_first);
    assert forall|i: int| 0 < i < spans.drop_first().len() implies
        (#[trigger] spans.drop_first()[i - 1]).label
            != (#[trigger] spans.drop_first()[i]).label by {
        assert(spans[(i + 1) - 1].label != spans[i + 1].label);
        assert(spans.drop_first()[i - 1] == spans[i]);
        assert(spans.drop_first()[i] == spans[i + 1]);
    }
}

/// A well-formed maximal-run representation is uniquely determined by its
/// per-byte labels.
pub proof fn lemma_coalesced_denotation_unique_at(
    left: Seq<SegmentSpan>,
    right: Seq<SegmentSpan>,
    offset: usize,
)
    requires
        spans_well_formed_at(left, offset),
        spans_well_formed_at(right, offset),
        coalesced(left),
        coalesced(right),
        denote(left) == denote(right),
    ensures
        left == right,
    decreases left.len() + right.len(),
{
    reveal(coalesced);
    lemma_denote_forward(left);
    lemma_denote_forward(right);
    if left.len() == 0 || right.len() == 0 {
        reveal(denote_forward);
        if left.len() > 0 {
            assert(denote_forward(left).len() > 0);
        }
        if right.len() > 0 {
            assert(denote_forward(right).len() > 0);
        }
    } else {
        let left_span = left.first();
        let right_span = right.first();
        let left_len = (left_span.end as int - left_span.start as int) as nat;
        let right_len = (right_span.end as int - right_span.start as int) as nat;
        reveal_with_fuel(denote_forward, 2);
        assert(denote_forward(left)[0] == left_span.label);
        assert(denote_forward(right)[0] == right_span.label);
        assert(left_span.label == right_span.label);

        if left_len < right_len {
            assert(left.drop_first().len() > 0);
            assert(left.len() > 1);
            assert(denote_forward(left)[left_len as int]
                == left.drop_first().first().label);
            assert(denote_forward(right)[left_len as int] == right_span.label);
            assert(left[1 - 1].label != left[1].label);
            assert(left.drop_first().first() == left[1]);
            assert(left.drop_first().first().label != left_span.label);
            assert(false);
        }
        if right_len < left_len {
            assert(right.drop_first().len() > 0);
            assert(right.len() > 1);
            assert(denote_forward(right)[right_len as int]
                == right.drop_first().first().label);
            assert(denote_forward(left)[right_len as int] == left_span.label);
            assert(right[1 - 1].label != right[1].label);
            assert(right.drop_first().first() == right[1]);
            assert(right.drop_first().first().label != right_span.label);
            assert(false);
        }
        assert(left_len == right_len);
        assert(left_span == right_span);
        let left_tail = left.drop_first();
        let right_tail = right.drop_first();
        assert_seqs_equal!(denote_forward(left_tail), denote_forward(right_tail), i => {
            assert(denote_forward(left)[left_len as int + i]
                == denote_forward(left_tail)[i]);
            assert(denote_forward(right)[right_len as int + i]
                == denote_forward(right_tail)[i]);
        });
        lemma_well_formed_drop_first(left, offset);
        lemma_well_formed_drop_first(right, offset);
        lemma_coalesced_drop_first(left);
        lemma_coalesced_drop_first(right);
        lemma_denote_forward(left_tail);
        lemma_denote_forward(right_tail);
        lemma_coalesced_denotation_unique_at(left_tail, right_tail, left_span.end);
        assert(left =~= left_tail.insert(0, left_span));
        assert(right =~= right_tail.insert(0, right_span));
    }
}

proof fn lemma_repeated_label_concat(label: usize, left: nat, right: nat)
    ensures
        repeated_label(label, left) + repeated_label(label, right)
            == repeated_label(label, left + right),
{
    assert_seqs_equal!(
        repeated_label(label, left) + repeated_label(label, right),
        repeated_label(label, left + right),
        i => {
            if i < left {
                assert((repeated_label(label, left) + repeated_label(label, right))[i]
                    == repeated_label(label, left)[i]);
            } else {
                assert((repeated_label(label, left) + repeated_label(label, right))[i]
                    == repeated_label(label, right)[i - left]);
            }
        }
    );
}

/// Append a span to an already-canonical prefix, coalescing an equal label.
pub open spec fn push_span(
    output: Seq<SegmentSpan>,
    span: SegmentSpan,
) -> Seq<SegmentSpan> {
    if output.len() > 0 && output.last().label == span.label {
        output.update(
            output.len() - 1,
            SegmentSpan {
                start: output.last().start,
                end: span.end,
                label: span.label,
            },
        )
    } else {
        output.push(span)
    }
}

/// Fold the first `count` spans into their unique maximal-run representation.
pub open spec fn canonicalize_prefix(
    input: Seq<SegmentSpan>,
    count: int,
) -> Seq<SegmentSpan>
    recommends
        0 <= count <= input.len(),
    decreases count,
{
    if count <= 0 {
        Seq::empty()
    } else {
        push_span(
            canonicalize_prefix(input, count - 1),
            input[count - 1],
        )
    }
}

pub open spec fn canonicalize_spec(input: Seq<SegmentSpan>) -> Seq<SegmentSpan> {
    canonicalize_prefix(input, input.len() as int)
}

proof fn lemma_push_preserves_denotation(
    output: Seq<SegmentSpan>,
    span: SegmentSpan,
)
    requires
        spans_well_formed(output),
        span.start < span.end,
        output.len() == 0 || output.last().end == span.start,
    ensures
        denote(push_span(output, span)) == denote(output.push(span)),
{
    reveal(push_span);
    reveal_with_fuel(denote, 3);
    if output.len() > 0 && output.last().label == span.label {
        let last = output.last();
        let merged = SegmentSpan {
            start: last.start,
            end: span.end,
            label: span.label,
        };
        let prefix = output.drop_last();
        let left_len = (last.end as int - last.start as int) as nat;
        let right_len = (span.end as int - span.start as int) as nat;
        lemma_repeated_label_concat(
            span.label,
            left_len,
            right_len,
        );
        assert(output =~= prefix.push(last));
        assert(push_span(output, span) =~= prefix.push(merged));
        assert((merged.end as int - merged.start as int)
            == (last.end as int - last.start as int)
                + (span.end as int - span.start as int));
        assert(0 <= last.end as int - last.start as int);
        assert(0 <= span.end as int - span.start as int);
        assert(0 <= merged.end as int - merged.start as int);
        assert((merged.end as int - merged.start as int) as nat == left_len + right_len);
        assert((prefix.push(merged)).drop_last() =~= prefix);
        assert(denote(prefix.push(merged))
            == denote(prefix) + repeated_label(span.label, left_len + right_len));
        assert(output.drop_last() =~= prefix);
        assert(denote(output)
            == denote(prefix) + repeated_label(span.label, left_len));
        assert((output.push(span)).drop_last() =~= output);
        assert(denote(output.push(span))
            == denote(output) + repeated_label(span.label, right_len));
        vstd::seq_lib::lemma_concat_associative(
            denote(prefix),
            repeated_label(span.label, left_len),
            repeated_label(span.label, right_len),
        );
        assert(denote(output.push(span))
            == denote(prefix)
                + repeated_label(span.label, left_len)
                + repeated_label(span.label, right_len));
    }
}

// Well-formedness survives truncation: any prefix of a well-formed list is
// itself well-formed. The inductive step below reasons about `input.take(..)`
// but only has `spans_well_formed` for the whole input, so it needs this.
//
// None of this is automatic: `spans_well_formed` is three quantified clauses,
// and Verus does not unfold `take`'s indexing inside a quantifier on its own.
// Each clause is re-established by handing the solver the bridging fact that a
// prefix element is definitionally the same element as the original.
proof fn lemma_well_formed_take(spans: Seq<SegmentSpan>, count: int)
    requires
        spans_well_formed(spans),
        0 <= count <= spans.len(),
    ensures
        spans_well_formed(spans.take(count)),
{
    // Clause 1: every span is non-empty (start < end).
    assert forall|i: int| 0 <= i < spans.take(count).len() implies
        (#[trigger] spans.take(count)[i]).start < spans.take(count)[i].end by {
        assert(spans.take(count)[i] == spans[i]);
    }
    // Clause 2: the first span starts at byte 0 (guarded: empty prefix has none).
    if count > 0 {
        assert(spans.take(count)[0] == spans[0]);
    }
    // Clause 3: spans are contiguous. This relates two adjacent elements, so it
    // needs the bridge for both `i - 1` and `i`.
    assert forall|i: int| 0 < i < spans.take(count).len() implies
        (#[trigger] spans.take(count)[i - 1]).end
            == (#[trigger] spans.take(count)[i]).start by {
        assert(spans.take(count)[i - 1] == spans[i - 1]);
        assert(spans.take(count)[i] == spans[i]);
    }
}

proof fn lemma_push_preserves_well_formed(
    output: Seq<SegmentSpan>,
    span: SegmentSpan,
)
    requires
        spans_well_formed(output),
        span.start < span.end,
        output.len() == 0 ==> span.start == 0,
        output.len() > 0 ==> output.last().end == span.start,
    ensures
        spans_well_formed(push_span(output, span)),
{
    reveal(push_span);
    assert forall|i: int| 0 <= i < push_span(output, span).len() implies
        (#[trigger] push_span(output, span)[i]).start
            < push_span(output, span)[i].end by {
    }
    assert forall|i: int| 0 < i < push_span(output, span).len() implies
        (#[trigger] push_span(output, span)[i - 1]).end
            == (#[trigger] push_span(output, span)[i]).start by {
    }
}

proof fn lemma_push_preserves_coalesced(
    output: Seq<SegmentSpan>,
    span: SegmentSpan,
)
    requires
        coalesced(output),
    ensures
        coalesced(push_span(output, span)),
{
    reveal(push_span);
    assert forall|i: int| 0 < i < push_span(output, span).len() implies
        (#[trigger] push_span(output, span)[i - 1]).label
            != (#[trigger] push_span(output, span)[i]).label by {
    }
}

// Canonicalizing the first `count` spans denotes the same per-byte label
// sequence as taking the first `count` spans raw (and stays well-formed).
//
// `count` generalizes the real goal (the `count == input.len()` case) so we can
// induct on prefix length; `decreases count` proves the induction terminates.
// The step folds in one more span with `push_span` on top of the induction
// hypothesis and shows that extra step is invisible to `denote`.
proof fn lemma_canonicalize_prefix_preserves_denotation(
    input: Seq<SegmentSpan>,
    count: int,
)
    requires
        spans_well_formed(input),
        0 <= count <= input.len(),
    ensures
        spans_well_formed(canonicalize_prefix(input, count)),
        coalesced(canonicalize_prefix(input, count)),
        denote(canonicalize_prefix(input, count)) == denote(input.take(count)),
    decreases count,
{
    if count == 0 {
        // Base case: both sides are the empty sequence. `reveal` unfolds the
        // hidden recursive definition far enough to see it bottoms out at empty.
        reveal(canonicalize_prefix);
    } else {
        // Move 1: the recursive call IS the induction hypothesis. Afterwards
        // Verus knows `prior` is well-formed and denote(prior) == denote(input_prior).
        lemma_canonicalize_prefix_preserves_denotation(input, count - 1);
        lemma_well_formed_take(input, count - 1);
        let prior = canonicalize_prefix(input, count - 1);
        let input_prior = input.take(count - 1);
        let span = input[count - 1];
        // Move 2: rewrite the goal's RHS as "prefix, plus one span appended", so
        // both sides are phrased the same way. `=~=` is extensional (equal at
        // every index) equality, which the solver won't infer unaided here.
        assert(input.take(count) =~= input_prior.push(span)) by {
            input.lemma_take_succ_push(count - 1);
        }
        assert(span.start < span.end);
        // Move 3: establish the "seam" precondition the push lemmas need, namely
        // that `span` butts up exactly against the end of `prior`.
        if prior.len() == 0 {
            // `push_span` never shrinks, so an empty canonical prefix means an
            // empty input prefix; then `span == input[0]`, which starts at 0.
            assert(input_prior.len() == 0);
            assert(span.start == 0);
        } else {
            // Canonicalization merges interior cuts but never moves the final
            // endpoint, so `prior` and `input_prior` end at the same byte; then
            // contiguity of `input` places `span.start` right there.
            assert(input_prior.len() > 0);
            assert(prior.last().end == input_prior.last().end);
            assert(input_prior.last().end == span.start);
        }
        // Move 4: the one-step push lemmas. The denotation lemma is the heart:
        // coalescing-append (push_span) and plain append denote the same thing,
        // which is where "merging equal labels is invisible" gets used.
        lemma_push_preserves_well_formed(prior, span);
        lemma_push_preserves_coalesced(prior, span);
        lemma_push_preserves_denotation(prior, span);
        // Move 5: unfold `denote` one notch on each side (its body is hidden by
        // default to avoid trigger loops; fuel 2 peels the last span off once).
        // Both sides append the SAME repeated_label run, so once the IH equates
        // the prefixes the whole equation chains shut.
        reveal(canonicalize_prefix);
        reveal_with_fuel(denote, 2);
        assert((prior.push(span)).drop_last() =~= prior);
        assert((prior.push(span)).last() == span);
        assert(denote(prior.push(span))
            == denote(prior)
                + repeated_label(
                    span.label,
                    (span.end as int - span.start as int) as nat,
                ));
        assert((input_prior.push(span)).drop_last() =~= input_prior);
        assert((input_prior.push(span)).last() == span);
        assert(denote(input_prior.push(span))
            == denote(input_prior)
                + repeated_label(
                    span.label,
                    (span.end as int - span.start as int) as nat,
                ));
    }
}

/// Canonicalization preserves the complete per-byte coverage denotation.
pub proof fn lemma_canonicalize_preserves_denotation(input: Seq<SegmentSpan>)
    requires
        spans_well_formed(input),
    ensures
        spans_well_formed(canonicalize_spec(input)),
        coalesced(canonicalize_spec(input)),
        denote(canonicalize_spec(input)) == denote(input),
{
    lemma_canonicalize_prefix_preserves_denotation(input, input.len() as int);
    assert(input.take(input.len() as int) =~= input);
}

/// If canonical outputs are equal, the inputs have identical per-byte
/// annotation coverage. Canonicalization therefore cannot erase a real
/// coverage difference.
pub proof fn lemma_canonical_equality_implies_denotation(
    left: Seq<SegmentSpan>,
    right: Seq<SegmentSpan>,
)
    requires
        spans_well_formed(left),
        spans_well_formed(right),
        canonicalize_spec(left) == canonicalize_spec(right),
    ensures
        denote(left) == denote(right),
{
    lemma_canonicalize_preserves_denotation(left);
    lemma_canonicalize_preserves_denotation(right);
}

/// Equal per-byte coverage has one canonical maximal-run representation.
pub proof fn lemma_canonicalize_complete(
    left: Seq<SegmentSpan>,
    right: Seq<SegmentSpan>,
)
    requires
        spans_well_formed(left),
        spans_well_formed(right),
        denote(left) == denote(right),
    ensures
        canonicalize_spec(left) == canonicalize_spec(right),
{
    lemma_canonicalize_preserves_denotation(left);
    lemma_canonicalize_preserves_denotation(right);
    lemma_coalesced_denotation_unique_at(
        canonicalize_spec(left),
        canonicalize_spec(right),
        0,
    );
}

/// Canonicalize contiguous spans by removing boundaries that do not change
/// annotation coverage.
pub fn canonicalize_spans(input: &[SegmentSpan]) -> (output: Vec<SegmentSpan>)
    requires
        spans_well_formed(input@),
    ensures
        output@ == canonicalize_spec(input@),
        spans_well_formed(output@),
        coalesced(output@),
        denote(output@) == denote(input@),
{
    proof {
        lemma_canonicalize_preserves_denotation(input@);
    }
    let mut output: Vec<SegmentSpan> = Vec::new();
    let mut i: usize = 0;
    while i < input.len()
        invariant
            0 <= i <= input@.len(),
            output@ == canonicalize_prefix(input@, i as int),
        decreases input.len() - i,
    {
        let span = input[i];
        if !output.is_empty() && output[output.len() - 1].label == span.label {
            let last = output.len() - 1;
            // Replace the whole span to mirror push_span's Seq::update, making
            // the executable step directly match the loop invariant.
            output[last] = SegmentSpan {
                start: output[last].start,
                end: span.end,
                label: span.label,
            };
        } else {
            output.push(span);
        }
        i = i + 1;
    }
    output
}

/// The span list a boundary/label pair denotes: span `j` runs from
/// `boundaries[j]` to `boundaries[j + 1]` and carries `labels[j]`.
pub open spec fn induced_spans(
    boundaries: Seq<usize>,
    labels: Seq<usize>,
) -> Seq<SegmentSpan> {
    Seq::new(
        labels.len(),
        |j: int|
            SegmentSpan {
                start: boundaries[j],
                end: boundaries[j + 1],
                label: labels[j],
            },
    )
}

/// Validate boundary shape and construct canonical spans without exposing a
/// proof precondition to ordinary Rust callers.
///
/// `None` means the inputs are not a valid segmentation: fewer than two
/// boundaries, a first boundary other than zero, a label count that does not
/// match the boundary count, or a non-increasing boundary pair. `Some` carries
/// the canonicalization of exactly the spans those boundaries induce.
pub fn canonicalize_boundaries(
    boundaries: &[usize],
    labels: &[usize],
) -> (output: Option<Vec<SegmentSpan>>)
    ensures
        output is Some ==> {
            &&& output->Some_0@ == canonicalize_spec(induced_spans(boundaries@, labels@))
            &&& spans_well_formed(output->Some_0@)
            &&& coalesced(output->Some_0@)
            &&& denote(output->Some_0@) == denote(induced_spans(boundaries@, labels@))
        },
{
    if boundaries.len() < 2 || boundaries[0] != 0 {
        return None;
    }
    if boundaries.len() - 1 != labels.len() {
        return None;
    }
    let mut spans: Vec<SegmentSpan> = Vec::new();
    let mut i: usize = 0;
    while i < labels.len()
        invariant
            boundaries@.len() == labels@.len() + 1,
            0 < boundaries@.len(),
            boundaries@[0] == 0,
            0 <= i <= labels@.len(),
            spans@.len() == i,
            forall|j: int| 0 <= j < i ==> (#[trigger] spans@[j]) == (SegmentSpan {
                start: boundaries@[j],
                end: boundaries@[j + 1],
                label: labels@[j],
            }),
            spans_well_formed(spans@),
        decreases labels.len() - i,
    {
        let start = boundaries[i];
        let end = boundaries[i + 1];
        if start >= end {
            return None;
        }
        spans.push(SegmentSpan { start, end, label: labels[i] });
        i = i + 1;
    }
    // The loop invariant pins every element of `spans`, but element-wise
    // agreement with the spec sequence is not sequence equality until the
    // extensional rule is applied.
    assert(spans@ =~= induced_spans(boundaries@, labels@));
    Some(canonicalize_spans(&spans))
}

} // verus!

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coalesces_only_equal_adjacent_labels() {
        let input = vec![
            SegmentSpan {
                start: 0,
                end: 3,
                label: 7,
            },
            SegmentSpan {
                start: 3,
                end: 5,
                label: 7,
            },
            SegmentSpan {
                start: 5,
                end: 8,
                label: 9,
            },
        ];

        assert_eq!(
            canonicalize_spans(&input),
            [
                SegmentSpan {
                    start: 0,
                    end: 5,
                    label: 7,
                },
                SegmentSpan {
                    start: 5,
                    end: 8,
                    label: 9,
                },
            ]
        );
    }
}
