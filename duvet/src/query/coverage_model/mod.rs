// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Coverage model v2: language-aware annotation execution checking.
//!
//! Implements the two-phase coverage model from `design/coverage-model-v2-spec.md`:
//! - Phase 1: Annotation target resolution (forward walk)
//! - Phase 2: Execution propagation (backward walk from hit lines)
//! - Phase 3: Composition of Phases 1 and 2

pub mod types;
pub mod target_resolution;
pub mod execution_propagation;
pub mod annotation_execution;
pub mod scopes;
pub mod proofs;
