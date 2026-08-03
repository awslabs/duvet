# LCOV fixture corpora

Real-producer LCOV tracefiles pinned against their exact sources
(`design/lcov-parser/decisions.md`, Decision 7). The corpus pin tests in
`duvet/src/query/parsers/lcov.rs` assert *which lines appear at all* — the
behavior hand-written fixtures get wrong — so a Rust/LLVM/Verus toolchain
behavior change surfaces as a fixture diff instead of silent semantic drift.

The pinned `.info` files carry absolute `SF:/tmp/...` paths from generation;
the parser treats `SF:` values as opaque map keys, so they are harmless.

## Regeneration

Toolchain: Rust 1.97.1 (`llvm-profdata`/`llvm-cov` from its `llvm-tools`
component), Verus 0.2026.05.24.ecee80a.

### macro-corpus / two-macros

Each `lib.rs` was `src/lib.rs` of a minimal `cargo init --lib` crate
(edition 2021, no dependencies) at `/tmp/<corpus>`:

```console
$ RUSTFLAGS="-C instrument-coverage" \
  LLVM_PROFILE_FILE="$PWD/cov-%m.profraw" \
  cargo +1.97.1 test $FILTER
$ llvm-profdata merge -sparse cov-*.profraw -o cov.profdata
$ llvm-cov export --format=lcov --instr-profile cov.profdata \
  target/debug/deps/<test-binary> > lcov.info
```

`$FILTER` is `exercise_covered_half` for macro-corpus (so `never_run_test`
is compiled but filtered out at runtime — its body pins the
compiled-but-not-run `#[test]` shape) and empty for two-macros.

### verus-corpus

```console
$ verus vexample.rs --compile -o vexample -- -C instrument-coverage
$ LLVM_PROFILE_FILE="$PWD/cov.profraw" ./vexample
$ llvm-profdata merge -sparse cov.profraw -o cov.profdata
$ llvm-cov export --format=lcov --instr-profile cov.profdata vexample > lcov.info
```

This corpus deliberately pins that `proof { }` block lines *inside exec
functions* report HIT under today's Verus erasure, while `spec fn`/`proof fn`
bodies and `requires`/`ensures` lines are entirely absent. If a future Verus
changes ghost erasure, the pin test fails and the change is surfaced rather
than silently absorbed.
