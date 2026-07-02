// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Builds the `duvet-wasm` component and runs its host-embedder test.

use crate::Result;
use clap::Parser;
use xshell::{cmd, Shell};

const TARGET: &str = "wasm32-wasip2";

#[derive(Debug, Default, Parser)]
pub struct Wasm {
    #[clap(long, default_value = "dev")]
    pub profile: String,

    /// Skip the wasmtime embedder test (build only).
    #[clap(long)]
    pub no_test: bool,
}

impl Wasm {
    pub fn run(&self, sh: &Shell) -> Result {
        // Make sure the component target is available.
        cmd!(sh, "rustup target add {TARGET}").run()?;

        let profile = &self.profile;
        cmd!(
            sh,
            "cargo build -p duvet-wasm --target {TARGET} --profile {profile}"
        )
        .run()?;

        // Guard against the wasm build silently pulling in native-only,
        // wasm-hostile dependencies.
        self.audit_dependencies(sh)?;

        if !self.no_test {
            // The embedder test instantiates the component with wasmtime and
            // checks a known fixture end-to-end. wasmtime's MSRV is newer than
            // duvet's pinned toolchain, and it's a host-only dev-dependency
            // (not part of the shipped component), so build the test with
            // stable rather than the pinned toolchain.
            cmd!(sh, "cargo +stable test -p duvet-wasm --test embedder").run()?;
        }

        Ok(())
    }

    /// Asserts the wasm dependency graph is free of the known blockers.
    fn audit_dependencies(&self, sh: &Shell) -> Result {
        let tree = cmd!(
            sh,
            "cargo tree -p duvet-wasm --target {TARGET} --edges no-dev --prefix none"
        )
        .read()?;

        for blocker in ["mimalloc", "reqwest", "native-tls", "hyper", "mio"] {
            if tree.lines().any(|line| line.starts_with(blocker)) {
                anyhow::bail!(
                    "wasm build unexpectedly depends on `{blocker}` — it must be gated off for \
                     target_family=\"wasm\""
                );
            }
        }

        Ok(())
    }
}
