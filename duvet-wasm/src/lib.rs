// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! A hermetic WebAssembly component that runs duvet's coverage checks.
//!
//! The host stages the whole project (config, sources, and any pre-downloaded
//! specification files) into an in-memory filesystem via the `run` export, and
//! gets back an inspectable [`check-report`](crate::exports::duvet::checks) —
//! the full v2 JSON report plus a derived pass/fail verdict. Nothing touches an
//! ambient filesystem or the network, so the component needs no WASI
//! capabilities beyond what std implicitly imports (clocks).

wit_bindgen::generate!({
    world: "duvet",
    path: "wit",
});

use duvet::api::{self, CheckOptions};
use duvet_core::{env, path::Path, vfs::Mem, Cache};
use std::sync::Arc;

struct Component;

impl Guest for Component {
    fn run(input: RunInput) -> Result<CheckReport, String> {
        run_checks(input).map_err(|e| format!("{e:?}"))
    }
}

fn run_checks(input: RunInput) -> duvet_core::Result<CheckReport> {
    let root = Path::from(input.root.as_str());

    let mem = Mem::new();
    for file in &input.files {
        // Stage each file at an absolute path under the virtual root so it
        // matches how duvet resolves paths against the working directory.
        mem.insert(root.join(&file.path), file.contents.clone());
    }

    let options = CheckOptions {
        require_citations: input.require_citations,
        require_tests: input.require_tests,
    };

    // The duvet pipeline spawns tasks via `tokio::spawn`/`JoinSet`, so it must
    // run inside a tokio runtime. Use the single-threaded current-thread
    // runtime (wasm has no OS threads) with no IO/time driver — the in-memory
    // filesystem resolves synchronously.
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .map_err(|e| duvet_core::error!("failed to build runtime: {e}"))?;

    // Install a fresh cache, the in-memory filesystem, and the virtual working
    // directory on every runtime thread (there is only one on wasm).
    let cache = Cache::default();
    {
        let cache = cache.clone();
        let mem = mem.clone();
        let root = root.clone();
        cache.setup_thread();
        mem.setup_thread();
        env::set_current_dir(root.clone());
        env::set_args(Arc::from([String::from("duvet"), String::from("report")]));
    }

    let result = runtime.block_on(api::check(options))?;

    // Return any files the run produced (reports, extracted requirements)
    // that weren't part of the input, so the host can inspect the outputs.
    let inputs: std::collections::HashSet<Path> =
        input.files.iter().map(|f| root.join(&f.path)).collect();
    let mut outputs = vec![];
    for (path, contents) in mem.snapshot() {
        if inputs.contains(&path) {
            continue;
        }
        // Report the output path relative to the virtual root.
        let rel = path
            .strip_prefix(&root)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| path.to_string());
        outputs.push(File {
            path: rel,
            contents: contents.data().to_vec(),
        });
    }

    let violations = result.violations.iter().map(map_violation).collect();

    Ok(CheckReport {
        ok: result.ok,
        report_json: result.report_json,
        violations,
        outputs,
    })
}

fn map_violation(v: &api::Violation) -> Violation {
    use api::ViolationKind as K;
    Violation {
        target: v.target.clone(),
        line: v.line as u32,
        kind: match v.kind {
            K::MissingCitation => ViolationKind::MissingCitation,
            K::CitationWithoutSpec => ViolationKind::CitationWithoutSpec,
            K::MissingTest => ViolationKind::MissingTest,
            K::TestWithoutCitation => ViolationKind::TestWithoutCitation,
        },
        message: v.kind.message().to_string(),
    }
}

export!(Component);
