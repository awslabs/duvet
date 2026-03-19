# Duvet Coverage Model: Verus Formal Verification Status

**Date:** 2026-03-19
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

---

## What Is Claimed (for human review)

The following are the machine-checked claims. A reviewer should read these
ensures clauses and the spec predicates below, and verify that they match the
intended meaning from the [spec](coverage-model-v2-spec.md). The proof code
(loop invariants, ghost variables, assert chains) is mechanical and does not
need human review — Verus checked it.

### Core function: `execution_set`

```
fn execution_set(classifications, scopes, coverage) -> result
  requires
    // Every coverage line is a valid index into classifications
    forall|line| coverage.contains_key(line) ==> (line - 1) >= 0 && (line - 1) < classifications.len()
    // All scopes have close_line < MAX (no overflow)
    forall|i| scopes[i].close_line < u64::MAX
    // All scopes have open_line >= 1 (1-indexed lines)
    forall|i| scopes[i].open_line >= 1
  ensures
    // Property 9: every directly-hit line is in the result
    forall|line| coverage[line] == Hit ==> result.contains(line)
    // Property 1: every line in the result is validly there
    forall|line| result.contains(line) ==> validly_in_exec_set(line, ...)
    // Completeness: every line that should be in the result IS in the result
    forall|line| validly_in_exec_set(line, ...) ==> result.contains(line)
```

A line is `validly_in_exec_set` if it is directly hit, OR there exists a
`has_valid_path` to a hit line. `has_valid_path(line, hit_line, scope_idx)`
means ALL of:

- `hit_line` is directly hit (`coverage[hit_line] == Hit`)
- Both `line` and `hit_line` are in the same scope `scope_idx`
- Every line strictly between them is classified (`Some`), not `ScopeClose`,
  not `Statement`, not `ScopeOpen`
- The scope does NOT contain any `NonLinearControl` line
- `line` itself is not `ScopeClose` or `Statement`

### Properties proven as lemmas

| # | Property | What the lemma says |
|---|----------|---------------------|
| 1 | No False Positives | If `Executed`, there exists a hit line with a clear path in the same scope |
| 2 | No Cross-Scope Leakage | Propagation never crosses between sibling/unrelated scopes |
| 3 | Conservative Fallback | Propagation scope never contains NonLinearControl |
| 4 | Monotonicity | More hits → larger execution set |
| 5 | Stacking Transitivity | Stacked annotations resolve to the same target |
| 6 | Unknown Safety | `Executed` is never returned for unknown lines |
| 8 | Target Bounds | Target line number > annotation end line |
| 9 | Containment | Every hit line is in the execution set |

---

## What Is Trusted (assumptions a reviewer must accept)

### 1. Assume: u64-to-usize cast is lossless

```rust
// In check_line loop, execution_propagation.rs:
let idx: usize = ((check_line - 1) as usize);
// ...
assume(idx as int == check_line as int - 1);
```

**Why it's true:** `idx` is a valid index into an in-memory slice, so it fits
in `usize`. A compile-time assertion in `lib.rs` verifies `size_of::<usize>()
>= size_of::<u64>()`, ensuring this holds on the target platform.

**What depends on it:** `scope_has_non_linear_control` is derived from this
assume. Without it, the NLC scope detection can't connect exec-level
`classifications[idx]` to spec-level `classifications@[check_line - 1]`.

### 2. External body: `vec_from_btreeset`

```rust
fn vec_from_btreeset(s: &BTreeSet<u64>) -> Vec<u64>
  ensures
    forall|line| s.contains(line) ==> result.contains(line)
    forall|i| 0 <= i < result.len() ==> s.contains(result[i])
```

**Why it's true:** `s.iter().copied().collect()`. Trivial iterator conversion.
External because Verus doesn't support BTreeSet iterators.

### 3. External body: `collect_hit_lines`

```rust
fn collect_hit_lines(coverage: &CoverageReport) -> BTreeSet<u64>
  ensures
    forall|line| coverage[line] == Hit <==> result.contains(line)
```

**Why it's true:** Simple filter loop over BTreeMap entries. External because
Verus doesn't support BTreeMap iterators.

### 4. External body: `match_scope_pairs`

```rust
fn match_scope_pairs(classifications, file_length) -> Vec<(u64, u64)>
  ensures
    forall|i| pairs[i].0 <= pairs[i].1                    // open <= close
    forall|i, j| if pairs overlap, one contains the other  // proper nesting
```

**Why it's true:** Classic balanced-parentheses stack algorithm. Push on
ScopeOpen, pop on ScopeClose. Stack discipline guarantees nesting.

**What's missing from ensures:**
- `pairs[i].0 >= 1` (open lines are ≥ 1) — true because `line_num` starts at 1
- Pairs correspond to actual ScopeOpen/ScopeClose lines in classifications
- No duplicate pairs

**This is the only external body with non-trivial proof content.**

### 5. Classifier contract (not verified, by design)

The spec requires that language classifiers mark BOTH the source (goto) and
target (label) of non-linear control flow with `NonLinearControl`. This is a
contract on classifier implementations, not on the algorithm. See Property 3
in the spec.

---

## Spec Predicates Reference

These are the building blocks of the claims above. Defined in
`execution_propagation.rs` and `scopes.rs`.

| Predicate | Meaning |
|-----------|---------|
| `in_scope(line, scopes, i)` | `scopes[i].open_line <= line <= scopes[i].close_line` |
| `clear_path(line, hit_line, classifications)` | No obstacles (None/ScopeClose/Statement/ScopeOpen) between them |
| `scope_has_non_linear_control(classifications, scopes, i)` | Some line in scope has NonLinearControl |
| `has_valid_path(line, hit_line, classifications, scopes, i, coverage)` | Composes: hit, same scope, clear path, no NLC, line is not Statement/ScopeClose |
| `validly_in_exec_set(line, classifications, scopes, coverage)` | Directly hit OR has_valid_path to some hit line |
| `scopes_well_formed(scopes)` | Valid ranges + proper nesting |
| `scopes_match_classifications(scopes, classifications)` | Close lines have ScopeClose property |

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
