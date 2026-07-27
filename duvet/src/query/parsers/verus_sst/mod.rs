// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Verus SST witness producer (milestone 1: parser, structure,
//! closure, witness construction).
//!
//! Implements the producer side of `design/witness/spec.md`:
//! §1.2/§1.3 output shapes, §4 producer obligations, §5.2 two-pass
//! construction, §5.5 the Verus artifact. Engine wiring (the
//! `produce : (artifacts, annotations) → Vec<Witness>` entry point
//! of §1.7 against engine types) is a later milestone; nothing in
//! here is reachable from `duvet query` yet.
//!
//! Trusted-base note (spec §4.1): this module trusts that the SST
//! log faithfully records what elaboration consulted — the same
//! category of axiom as trusting the verifier. The closure
//! computation over that record is our code, golden-tested here
//! (§4.3) and a candidate for verification in `duvet-coverage`.

// Until engine wiring lands (milestone 2), the only consumer is the
// test suite; remove this allow when the producer is reachable from
// `duvet query`.
#![allow(dead_code)]

pub mod closure;
pub mod sexpr;
pub mod structure;
pub mod witness;

use std::path::Path;
use structure::{ObligationGraph, StructureError};

/// Load and merge every `*-sst.vir` module log in a directory.
///
/// Synchronous `std::fs` on purpose: the async/vfs decision belongs
/// to engine wiring (milestone 2), and the golden tests want a
/// plain entry point.
pub fn load_dir(dir: &Path) -> Result<ObligationGraph, LoadError> {
    let mut sources = Vec::new();
    let entries = std::fs::read_dir(dir).map_err(|e| LoadError::Io(dir.display().to_string(), e))?;
    for entry in entries {
        let entry = entry.map_err(|e| LoadError::Io(dir.display().to_string(), e))?;
        let path = entry.path();
        if path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with("-sst.vir"))
        {
            let text = std::fs::read_to_string(&path)
                .map_err(|e| LoadError::Io(path.display().to_string(), e))?;
            sources.push((path.display().to_string(), text));
        }
    }
    if sources.is_empty() {
        return Err(LoadError::NoLogs(dir.display().to_string()));
    }
    // Deterministic merge order regardless of readdir order.
    sources.sort_by(|a, b| a.0.cmp(&b.0));

    let modules = sources
        .iter()
        .map(|(path, text)| {
            structure::parse_module(text).map_err(|e| LoadError::Parse(path.clone(), e))
        })
        .collect::<Result<Vec<_>, _>>()?;
    ObligationGraph::merge(modules).map_err(|e| LoadError::Parse(dir.display().to_string(), e))
}

/// Producer load error.
#[derive(Debug)]
pub enum LoadError {
    Io(String, std::io::Error),
    Parse(String, StructureError),
    NoLogs(String),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::Io(path, e) => write!(f, "{path}: {e}"),
            LoadError::Parse(path, e) => write!(f, "{path}: {e}"),
            LoadError::NoLogs(dir) => write!(f, "no *-sst.vir logs found in {dir}"),
        }
    }
}

impl std::error::Error for LoadError {}

#[cfg(test)]
mod tests;
