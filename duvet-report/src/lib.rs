// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Small, I/O-free report algorithms whose correctness is checked by Verus.

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

/// The spans are non-empty, contiguous, and begin at byte zero.
pub open spec fn spans_well_formed(spans: Seq<SegmentSpan>) -> bool {
    &&& forall|i: int| 0 <= i < spans.len() ==>
        (#[trigger] spans[i]).start < spans[i].end
    &&& (spans.len() > 0 ==> spans[0].start == 0)
    &&& forall|i: int| 0 < i < spans.len() ==>
        (#[trigger] spans[i - 1]).end == (#[trigger] spans[i]).start
}

pub open spec fn repeated_label(label: usize, len: nat) -> Seq<usize> {
    Seq::new(len, |i: int| label)
}

/// Per-byte annotation coverage, with segment cuts erased.
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

proof fn lemma_well_formed_take(spans: Seq<SegmentSpan>, count: int)
    requires
        spans_well_formed(spans),
        0 <= count <= spans.len(),
    ensures
        spans_well_formed(spans.take(count)),
{
    assert forall|i: int| 0 <= i < spans.take(count).len() implies
        (#[trigger] spans.take(count)[i]).start < spans.take(count)[i].end by {
        assert(spans.take(count)[i] == spans[i]);
    }
    if count > 0 {
        assert(spans.take(count)[0] == spans[0]);
    }
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

proof fn lemma_canonicalize_prefix_preserves_denotation(
    input: Seq<SegmentSpan>,
    count: int,
)
    requires
        spans_well_formed(input),
        0 <= count <= input.len(),
    ensures
        spans_well_formed(canonicalize_prefix(input, count)),
        denote(canonicalize_prefix(input, count)) == denote(input.take(count)),
    decreases count,
{
    if count == 0 {
        reveal(canonicalize_prefix);
    } else {
        lemma_canonicalize_prefix_preserves_denotation(input, count - 1);
        lemma_well_formed_take(input, count - 1);
        let prior = canonicalize_prefix(input, count - 1);
        let input_prior = input.take(count - 1);
        let span = input[count - 1];
        assert(input.take(count) =~= input_prior.push(span)) by {
            input.lemma_take_succ_push(count - 1);
        }
        assert(span.start < span.end);
        if prior.len() == 0 {
            assert(input_prior.len() == 0);
            assert(span.start == 0);
        } else {
            assert(input_prior.len() > 0);
            assert(prior.last().end == input_prior.last().end);
            assert(input_prior.last().end == span.start);
        }
        lemma_push_preserves_well_formed(prior, span);
        lemma_push_preserves_denotation(prior, span);
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

/// Continue canonicalization from an arbitrary already-processed prefix.
pub open spec fn finish_spans(
    output: Seq<SegmentSpan>,
    suffix: Seq<SegmentSpan>,
) -> Seq<SegmentSpan>
    decreases suffix.len(),
{
    if suffix.len() == 0 {
        output
    } else {
        finish_spans(
            push_span(output, suffix.first()),
            suffix.drop_first(),
        )
    }
}

/// Splitting one span at any interior byte does not change canonicalization.
pub proof fn lemma_split_invariance(
    output: Seq<SegmentSpan>,
    suffix: Seq<SegmentSpan>,
    whole: SegmentSpan,
    left: SegmentSpan,
    right: SegmentSpan,
)
    requires
        whole.start == left.start,
        left.start < left.end,
        left.end == right.start,
        right.start < right.end,
        right.end == whole.end,
        left.label == whole.label,
        right.label == whole.label,
    ensures
        finish_spans(push_span(output, whole), suffix)
            == finish_spans(push_span(push_span(output, left), right), suffix),
{
    reveal(push_span);
    assert(push_span(output, whole) == push_span(push_span(output, left), right));
}

/// Canonicalize contiguous spans by removing boundaries that do not change
/// annotation coverage.
pub fn canonicalize_spans(input: &[SegmentSpan]) -> (output: Vec<SegmentSpan>)
    requires
        spans_well_formed(input@),
    ensures
        output@ == canonicalize_spec(input@),
        spans_well_formed(output@),
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
        if output.len() > 0 && output[output.len() - 1].label == span.label {
            let last = output.len() - 1;
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
