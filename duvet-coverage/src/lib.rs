// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Coverage model types and algorithms for duvet.
//!
//! This crate contains the pure-function coverage model from the
//! [coverage model spec](../design/query/coverage-model-spec.md).
//! When compiled with Verus, the correctness properties are machine-checked.

// The Verus proofs assume that u64-to-usize casts are lossless (usize >= 64 bits).
// This compile-time assertion ensures the proofs are only trusted on platforms
// where this holds. If this fails, the proofs' assume statements are unsound.
const _: () = assert!(
    core::mem::size_of::<usize>() >= core::mem::size_of::<u64>(),
    "duvet-coverage proofs require usize >= u64 (64-bit platform)"
);

use vstd::prelude::*;

pub mod types;
pub mod scopes;
pub mod target_resolution;
pub mod execution_propagation;
pub mod annotation_execution;
pub mod proofs;
