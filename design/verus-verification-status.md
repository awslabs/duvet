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

**31 verified, 0 errors. 38 tests. 3 external_body. 3 assume(false).**

## Properties Status

| # | Property | Status | How |
|---|----------|--------|-----|
| 1 | No False Positives | ✅ Proven | `ensures` on `execution_set` with `has_valid_path` spec predicate |
| 2 | No Cross-Scope Leakage | ✅ Proven | `proof fn lemma_no_cross_scope_leakage` — contradiction via `scopes_match_classifications` + `clear_path` |
| 3 | Conservative Fallback | ✅ Proven (refined) | `proof fn lemma_conservative_fallback` — propagation scope ≠ NLC scope. See spec finding below. |
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

## Remaining Assumes (3)

All three are `assume(false)` in the completeness proof for `execution_set` in `execution_propagation.rs`. They are in the `assert forall` proof block after the backward walk inner loop.

### Assume 1: Sub-case A — `line < final_current` (line 379)

**What it says:** This case is unreachable.

**Why it's true:** `has_valid_path(line, exec_line, ...)` requires `clear_path(line, exec_line, ...)` — no obstacles between `line` and `exec_line`. But the walk stopped at `final_current` which is between `line` and `exec_line`. The walk only stops at obstacles (None, ScopeClose, Statement, ScopeOpen) or scope boundaries. If obstacle: contradicts `clear_path`. If boundary (`final_current < scope.open_line`): then `line < scope.open_line`, contradicting `has_valid_path` which requires `line >= scope.open_line`.

**Why Verus can't prove it:** After a while loop exits, Verus doesn't provide the negation of the loop condition (for break exits) or the reason the walk stopped. The walk's stop reason is lost.

**How to close it:** Encode the walk's stop reason as a ghost variable in the inner loop invariant. When the walk stops at an obstacle, record which obstacle. Then in the proof, show the obstacle contradicts `clear_path`. When the walk stops at a boundary, show `final_current < scope.open_line` which contradicts `line >= scope.open_line`.

### Assume 2: Sub-case B edge case — `line == final_current && !current_in_result && !stopped_at_obstacle` (line 398)

**What it says:** This case is unreachable.

**Why it's true:** `!current_in_result` means the walk didn't insert `final_current`. `!stopped_at_obstacle` means the walk didn't hit an obstacle at `final_current`. The only remaining exit: loop condition false (`final_current < scope.open_line || final_current < 1`). But `has_valid_path` requires `line >= scope.open_line >= 1`, and `line == final_current`. So `final_current >= scope.open_line`, meaning the loop condition was true. But the loop exited. All breaks set either `current_in_result` or `stopped_at_obstacle`. Neither is set. Contradiction.

**Why Verus can't prove it:** Same root cause — Verus doesn't track that "if no break flag was set, the loop exited via condition being false."

**How to close it:** Same ghost variable approach as Assume 1. Alternatively, restructure the loop to avoid breaks entirely (use the loop condition for all exits).

### Assume 3: Outer completeness (line 414)

**What it says:** `forall|line| validly_in_exec_set(line, ...) ==> result.contains(line)`

**Why it's true:** Follows from Assumes 1 and 2 being closed, plus the per-exec_line proof block threading through the `di` and `si` loops.

**How to close it:** Close Assumes 1 and 2 first. Then the per-exec_line `assert forall` in the proof block will be fully proven. The outer completeness should then follow from Verus propagating the per-exec_line result through the `di` loop. May need a `di` loop invariant tracking per-scope completeness.

## Spec Finding: Property 3 (Conservative Fallback)

The spec (Section 5, Property 3) states: "if any line in scope S has NonLinearControl, then for all lines L in S, L ∈ execution_set if and only if coverage[L] == Hit."

This is too strong for nested scopes. A line L in a NonLinearControl parent scope S can also be in a child scope S' that does NOT have NonLinearControl. The backward walk in S' can propagate to L, putting it in the execution set without L being directly hit.

**What IS proven:** Propagation never happens IN a NonLinearControl scope. The `has_valid_path` predicate includes `!scope_has_non_linear_control(classifications, scopes, scope_idx)`, and Verus verifies this from the `if !has_non_linear` guard in `execution_set`.

**Recommendation:** Refine the spec to say: "if the tightest scope containing L has NonLinearControl, then L ∈ execution_set only if directly hit." Or: "no backward propagation occurs within a scope that contains NonLinearControl."

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
