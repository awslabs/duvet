// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Coverage model types and algorithms for duvet.
//!
//! This crate contains the pure-function coverage model from the
//! [coverage model spec](../design/query/coverage-model-spec.md).
//! When compiled with Verus, the correctness properties are machine-checked.

// Verus generates code patterns that trigger these warnings under normal rustc.
// The verus_keep_ghost cfg, unused proof variables, double-paren casts, and
// vstd imports are all required for `cargo verus build` verification.
// `unused_braces`: the `global size_of usize == 8;` directive (below) expands
// to braces that plain rustc flags but Verus needs.
#![allow(
    unused_imports,
    unused_variables,
    unused_parens,
    unused_braces,
    dead_code
)]
// Verus also requires source patterns that clippy flags as anti-patterns:
//   - `i = i + 1` rather than `i += 1` (assign_op_pattern)
//   - `vec.len() == 0` rather than `vec.is_empty()` (len_zero)
//   - `vec![...]` literals as Verus needs `Vec<T>` not `&[T]` (useless_vec)
//   - Branches that share a proof body but track different obstacles
//     (if_same_then_else)
//   - Explicit lifetimes that look elidable but help Verus's spec impls
//     (needless_lifetimes)
// Suppress these crate-wide; the proof patterns must remain as-is for
// `cargo verus build` to verify the algorithms.
#![allow(
    clippy::assign_op_pattern,
    clippy::len_zero,
    clippy::useless_vec,
    clippy::if_same_then_else,
    clippy::needless_lifetimes
)]

// The Verus proofs assume that u64-to-usize casts are lossless (usize >= 64 bits).
// This compile-time assertion ensures the proofs are only trusted on platforms
// where this holds. If this fails, the proofs' assume statements are unsound.
const _: () = assert!(
    core::mem::size_of::<usize>() >= core::mem::size_of::<u64>(),
    "duvet-coverage proofs require usize >= u64 (64-bit platform)"
);

use vstd::prelude::*;

verus! {
    // Fix the platform pointer width for verification so that u64 <-> usize
    // index casts are provably lossless instead of `assume`d. Matches the
    // runtime `const _` assertion above (usize >= u64); this crate is only
    // built/verified on 64-bit targets.
    global size_of usize == 8;
}

pub mod annotation_execution;
pub mod execution_propagation;
pub mod predicates;
pub mod proofs;
pub mod scopes;
pub mod target_resolution;
pub mod types;
