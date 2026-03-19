# Duvet Coverage Model: Verus Formal Verification Status

**Date:** 2026-03-19
**Commit:** f056f67
**Verus version:** Built from source at `../verus/` (Z3 4.12.5 compiled for Amazon Linux 2)

## How to Run

```bash
# Normal build and tests
cargo build -p duvet-coverage
cargo test -p duvet-coverage    # 38 tests

# Verus verification (requires Verus on PATH)
export PATH="../verus/source/target-verus/release:$PATH"
cargo verus build -p duvet-coverage    # 31 verified, 0 errors

# Full duvet build (requires OPENSSL_DIR)
OPENSSL_DIR=/usr cargo check -p duvet
```

## Verification Summary

**31 verified, 0 errors. 38 tests. 3 external_body. 1 assume.**

## Properties Status

| # | Property | Status | How |
|---|----------|--------|-----|
| 1 | No False Positives | ✅ Proven | `ensures` on `execution_set` with `has_valid_path` spec predicate |
| 2 | No Cross-Scope Leakage | ✅ Proven | `proof fn lemma_no_cross_scope_leakage` — contradiction via `scopes_match_classifications` + `clear_path` |
| 3 | Conservative Fallback | ✅ Proven | `proof fn lemma_conservative_fallback` — propagation scope ≠ NLC scope, per refined spec |
| 4 | Monotonicity | ✅ Proven | `proof fn lemma_monotonicity` — valid path under E1 is valid under E2. Depends on completeness `ensures`. |
| 5 | Stacking Transitivity | ✅ Proven | Both annotations resolve to same target via `annotation_target` forward walk |
| 6 | Unknown Safety | ✅ Proven | `Executed` only returned when `target.properties` is `Some` (match structure) |
| 7 | Target Determinism | ✅ Free | All exec fns are deterministic in Verus |
| 8 | Annotation Target Bounds | ✅ Proven | `ensures result.is_some() ==> result.unwrap().line_number > annotation.end_line` |
| 9 | Execution Set Containment | ✅ Proven | `ensures forall|line| Hit ==> result.contains(line)` with loop invariants |
| 10 | Scope Well-formedness | ✅ Partially proven | `build_from_pairs` verified; `match_scope_pairs` is `external_body` |

## Spec Predicates (in `execution_propagation.rs`)

- `in_scope(line, scopes, scope_idx)` — line is within scope boundaries
- `propagated_within_scope(line, hit_line, scopes, scope_idx)` — both in same scope (Property 2)
- `clear_path(line, hit_line, classifications)` — no ScopeClose/Statement/ScopeOpen/None between them
- `scope_has_non_linear_control(classifications, scopes, scope_idx)` — scope contains NLC line
- `has_valid_path(line, hit_line, classifications, scopes, scope_idx, coverage)` — composes all above
- `validly_in_exec_set(line, classifications, scopes, coverage)` — directly hit OR has_valid_path

## Spec Predicates (in `scopes.rs`)

- `scope_contains(scopes, i, j)` — scope i strictly contains scope j
- `scopes_well_formed(scopes)` — valid ranges + proper nesting
- `scopes_match_classifications(scopes, classifications)` — close lines have ScopeClose property

## External Bodies (3)

| Function | File | What it does | Why external |
|----------|------|-------------|--------------|
| `match_scope_pairs` | scopes.rs | Balanced-parentheses matching | Stack algorithm with complex loop invariant |
| `collect_hit_lines` | execution_propagation.rs | BTreeMap iteration to collect Hit lines | Verus BTreeMap iterator support limited |
| `vec_from_btreeset` | execution_propagation.rs | BTreeSet to Vec conversion | Verus BTreeSet iterator support limited |

All three have `ensures` clauses that downstream proofs depend on. The ensures are trusted (not verified).

## Remaining Assumes (1)

### Assume: `scope_has_non_linear_control` in NLC scope branch

**Where:** `execution_propagation.rs`, in the `has_non_linear` else branch of the si loop.

**What it says:** `assume(scope_has_non_linear_control(classifications, scopes, si as int))`

**Why it's true:** The check_line loop just found a line with `NonLinearControl` via `props.contains(&LineProperty::NonLinearControl)`. That line is within the scope boundaries. This directly witnesses the existential in `scope_has_non_linear_control`.

**Why Verus can't prove it:** The index `idx = (check_line - 1) as usize` involves a `u64`-to-`usize` cast. Verus does not assume `usize` is 64-bit, so it cannot prove `idx as int == check_line as int - 1`. Without this, it cannot connect the exec-level `classifications[idx]` to the spec-level `classifications@[check_line as int - 1]`.

**How to close it:** Either (a) add a precondition bounding all line numbers to `usize::MAX`, or (b) refactor the check_line loop to avoid the `u64`-to-`usize` cast, or (c) wait for Verus to add platform-width assumptions.

## Previously Closed Assumes (3)

All three original `assume(false)` in the completeness proof were eliminated by:

1. **Assumes 1 & 2 (Sub-cases A and B):** Replaced `break` statements with a `done` flag in the loop condition (`&& !done`). This gives Verus the negation of the full condition after exit, enabling complete case analysis. Ghost variables `stopped_at_obstacle`, `current_in_result`, and the `done` invariant distinguish all exit paths.

2. **Assume 3 (Outer completeness):** Threaded per-exec_line completeness through the `di` and `si` loops using ghost snapshot subset invariants (`result_before_walk`, `result_before_scope`) to prove monotonicity of `result` across loop iterations.

## Spec Finding: Property 3 (Conservative Fallback) — RESOLVED

The original spec stated: "if any line in scope S has NonLinearControl, then for all lines L in S, L ∈ execution_set if and only if coverage[L] == Hit."

This was too strong for nested scopes. A line L in a NonLinearControl parent scope S can also be in a child scope S' that does NOT have NonLinearControl. The backward walk in S' can propagate to L, putting it in the execution set without L being directly hit.

**Resolution:** The spec (Property 3) has been refined to state that no propagation occurs WITHIN a scope containing NonLinearControl. Propagation through a child scope that lacks NonLinearControl is explicitly permitted, with a soundness argument based on sequential execution: a goto in the parent cannot redirect control flow within the child without first exiting the child (crossing a ScopeClose).

**What IS proven:** `lemma_conservative_fallback` proves that if a line is in the execution set via propagation, the propagation scope does not have NonLinearControl. The `has_valid_path` predicate includes `!scope_has_non_linear_control(classifications, scopes, scope_idx)`, and Verus verifies this from the `if !has_non_linear` guard in `execution_set`.

## Architecture

```
duvet-coverage/
├── Cargo.toml              # depends on vstd, verus_builtin, verus_builtin_macros
└── src/
    ├── lib.rs              # pub mod declarations, vstd import
    ├── types.rs            # LineProperty, LineClass, Scope, etc. + obeys_cmp_spec proof
    ├── scopes.rs           # build_scope_tree (two-pass), scopes_well_formed predicates
    ├── target_resolution.rs # annotation_target with ensures (Property 8)
    ├── execution_propagation.rs # execution_set with ensures (Properties 1, 9, completeness)
    │                        # + all spec predicates (has_valid_path, clear_path, etc.)
    ├── annotation_execution.rs  # is_annotation_executed
    └── proofs.rs           # proof fn lemmas for Properties 2, 3, 4, 5, 6
```

Dual-mode compilation:
- `cargo build`: `verus!` macro strips proof annotations, keeps exec code
- `cargo verus build`: full Verus verification with machine-checked proofs
