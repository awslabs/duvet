// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

// Stamp a git commit + dirty suffix into `DUVET_VERSION_SUFFIX` so
// `duvet --version` can distinguish a released build from a local one.
// When git isn't available (e.g. crate built from the crates.io tarball),
// the suffix is empty and --version prints just the semver.

use std::process::Command;

fn main() {
    // Re-run when HEAD moves or the index changes.
    println!("cargo:rerun-if-changed=../.git/HEAD");
    println!("cargo:rerun-if-changed=../.git/index");

    let hash = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());

    let suffix = match hash {
        Some(h) => {
            let dirty = Command::new("git")
                .args(["status", "--porcelain"])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| !o.stdout.is_empty())
                .unwrap_or(false);
            let tag = if dirty { "-dirty" } else { "" };
            format!(" ({h}{tag})")
        }
        None => String::new(),
    };

    println!("cargo:rustc-env=DUVET_VERSION_SUFFIX={suffix}");
}
