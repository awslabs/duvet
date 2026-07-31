// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Verus SST witness producer: parser, structure, closure, and
//! witness construction.
//!
//! Implements the producer side of `design/witness/spec.md`:
//! §1.2/§1.3 output shapes, §4 producer obligations, §5.2 two-pass
//! construction, §5.3 discharge units, §5.5 the Verus artifact.
//! Engine wiring (the `produce` entry point of §1.7 against engine
//! types) lives in `crate::query::producers`; per §1.7 the types in
//! this module never escape it.
//!
//! The closure computation is golden-tested here (§4.3).

// Trusted-base note (spec §4.1):
//= design/witness/spec.md#obligation-closedness
//# Prover producers: closedness splits into
//# (a) the verifier's record faithfully reflects what elaboration
//# consulted — **axiom**, same category as trusting the verifier
//# itself — and
//# (b) the closure computation over that record is correct —
//# our code, which SHOULD be verified in `duvet-coverage`
//# (it is a pure graph fixpoint).

pub mod closure;
pub mod sexpr;
pub mod structure;
pub mod witness;

use std::path::Path;
use structure::{ObligationGraph, StructureError};

/// Load and merge every `*-sst.vir` (or gzipped `*-sst.vir.gz`)
/// module log in a directory.
///
/// Gzip support exists so the golden corpus can be checked into the
/// repository compactly (~21:1 on real SST text) and so users may
/// compress large log directories; the two forms are semantically
/// identical.
///
/// Synchronous `std::fs` on purpose: the async/vfs decision belongs
/// to the engine boundary, and the golden tests want a
/// plain entry point.
pub fn load_dir(dir: &Path) -> Result<ObligationGraph, LoadError> {
    let mut sources = Vec::new();
    let entries =
        std::fs::read_dir(dir).map_err(|e| LoadError::Io(dir.display().to_string(), e))?;
    for entry in entries {
        let entry = entry.map_err(|e| LoadError::Io(dir.display().to_string(), e))?;
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if name.ends_with("-sst.vir") {
            let text = std::fs::read_to_string(&path)
                .map_err(|e| LoadError::Io(path.display().to_string(), e))?;
            sources.push((path.display().to_string(), text));
        } else if name.ends_with("-sst.vir.gz") {
            use std::io::Read;
            let file = std::fs::File::open(&path)
                .map_err(|e| LoadError::Io(path.display().to_string(), e))?;
            let mut text = String::new();
            flate2::read::GzDecoder::new(file)
                .read_to_string(&mut text)
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
