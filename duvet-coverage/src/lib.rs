// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Coverage model types and algorithms for duvet.
//!
//! This crate contains the pure-function coverage model from the
//! [coverage model v2 spec](../design/coverage-model-v2-spec.md).
//! When compiled with Verus, the correctness properties are machine-checked.

use vstd::prelude::*;

pub mod types;
pub mod scopes;
pub mod target_resolution;
pub mod execution_propagation;
pub mod annotation_execution;
pub mod proofs;
