# Vacuity golden fixture

`vacuity-sst.vir` is the Verus SST elaboration record of `vacuity.rs`,
used by the `verus_sst` producer's golden tests to pin the honest
boundary of consulted-closure semantics (design/witness/spec.md §5.4,
design/witness/decisions.md Decision 7; empirical background in the
SST POC findings, 2026-07-26).

Three scenarios:

1. `vacuous_proof` (line 32) — vacuous premise, never mentions the
   implementation. Its closure's project spans are its own extent
   only; a pair against any impl annotation fails. The vacuity
   defense that consulted semantics does provide.
2. `vacuous_proof_mentioning` (line 43) — vacuous premise, but the
   `ensures` textually mentions `spec_add_one`. Elaboration records
   the mention, so the closure includes `spec_add_one` and the pair
   IS credited. Deliberate: consulted semantics cannot catch
   mention-without-need; this is the flagship case for the
   needed-semantics strengthening that Decision 7 reserves room for.
3. `self_contained` (line 54) — vacuous `ensures`, impl annotation
   inside the same fn's body (line 57). Discharge units carry their
   function's closure (design/witness/spec.md §5.4), which
   self-includes the body, so the pair IS credited — at clause
   grain too, now that the ensures clause is its own unit.
   Catching it requires needed semantics.

Plus a label fixture:

4. `noted_loop` (line 65) — a loop with a `proof_note`'d invariant
   and a `proof_note`'d assert alongside unnoted siblings, pinning
   proof_note label extraction (spec §5.5) (`ProofNoteLabel` text when
   recorded, span identity otherwise).

Plus a fill fixture:

5. `commented_body` (line 92) — a verified fn with an interior
   comment line (96) in its body, pinning spec §5.4's
   span-start-line fills: no span begins on a comment line, so it
   appears in no witness's fill (the old extent sweep marked it
   Hit), while its code neighbors do. The unverified `fn main()`
   below the `verus!` block (line 103) pins the same fact at file
   grain: no SST record, no fill, ever.

## Regeneration

Pinned Verus: 0.2026.05.24.ecee80a (matches `vstd` in Cargo.lock and
`VERUS_VERSION` in `.github/workflows/ci.yml`).

From this directory (cwd matters — spans record paths relative to it):

```console
$ verus vacuity.rs --log vir-sst --log-dir sst
$ mv sst/root-sst.vir vacuity-sst.vir && rmdir sst
$ rm -f vacuity            # discard the compiled binary
```

Expected: `8 verified, 0 errors`, and spans in the log read
`vacuity.rs:<line>:<col>: ...` (relative path, no directory prefix).
