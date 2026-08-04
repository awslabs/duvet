# Corpus-vintage sources

These are the exact `duvet-coverage/src/*.rs` sources the checked-in
golden corpus (`../*-sst.vir.gz`, generated 2026-07-26, Verus
0.2026.05.24.ecee80a) was produced from — repository commit
`ea02bf5` ("feat: add duvet query with Verus verified coverage
model (#227)"), the last commit to touch these files before the
corpus date. The live sources have since drifted, so span line
numbers in the corpus can only be checked against THESE copies.

Sole consumer: the theorem tripwire test
(`no_span_starts_on_a_comment_or_blank_line`, `tests.rs`), which
lexes them to assert that no span in the corpus begins on an
ordinary-comment or blank line (design/witness/spec.md §5.4).
Test-side lexing of sources we control;
the runtime never lexes.

If the corpus is ever regenerated, regenerate this directory from
the same tree in the same step.
