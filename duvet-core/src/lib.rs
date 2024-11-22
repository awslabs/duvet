// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

#[macro_export]
macro_rules! ensure {
    ($cond:expr) => {
        ensure!($cond, ());
    };
    ($cond:expr, $otherwise:expr) => {
        if !($cond) {
            return $otherwise;
        }
    };
}

#[cfg(any(test, feature = "testing"))]
pub mod testing;

mod cache;
pub mod contents;
pub mod diagnostic;
pub mod dir;
pub mod env;
pub mod file;
pub mod glob;
pub mod hash;
#[cfg(feature = "http")]
pub mod http;
pub mod path;
pub mod progress;
mod query;
pub mod vfs;

#[doc(hidden)]
pub mod macro_support;

pub use cache::Cache;
pub use duvet_macros::*;
pub use query::Query;

pub type Result<T = (), E = diagnostic::Error> = core::result::Result<T, E>;
