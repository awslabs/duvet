// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Per-test coverage witness production.
//!
//! Duvet's coverage check discharges a (test, implementation) annotation
//! pair only when a single coverage report shows BOTH executed
//! (design/query/decisions.md Decision 13; design/witness/spec.md
//! Property W1). That guarantee is exactly as strong as the report
//! partition: one aggregate report over the whole suite collapses the
//! check to "both are covered by the suite, somewhere, by something".
//!
//! This subcommand produces the strong partition: it runs EVERY `#[test]`
//! in the workspace in its own process and exports one LCOV tracefile per
//! test (`witnesses/<sanitized_name>.info`). Per-test isolation is
//! *created* by the per-process run and *preserved* by the per-test file —
//! see design/lcov-parser/decisions.md Decision 12. The files feed the CI
//! gate:
//!
//! ```console
//! $ duvet query -c coverage -r 'witnesses/*.info' -f lcov
//! ```
//!
//! Doc tests are not included: they are not annotated as duvet test
//! citations, and libtest cannot run a single doc test in isolation.
//!
//! Tests are trivially parallelizable across processes, so the subcommand
//! supports Lockbox-style worker sharding: `--total-workers N
//! --worker-number K` runs the tests whose index (in the sorted
//! enumeration) satisfies `index % N == K - 1`.

use crate::Result;
use anyhow::{anyhow, Context as _};
use clap::Parser;
use std::path::{Path, PathBuf};
use xshell::{cmd, Shell};

#[derive(Debug, Parser)]
pub struct Witnesses {
    /// Total number of workers the test list is partitioned across
    #[clap(long, default_value_t = 1)]
    total_workers: usize,

    /// This worker's 1-based number (runs tests with
    /// `index % total_workers == worker_number - 1`)
    #[clap(long, default_value_t = 1)]
    worker_number: usize,

    /// List this worker's tests without running them
    #[clap(long)]
    list: bool,

    /// Output directory for the per-test tracefiles
    #[clap(long, default_value = "witnesses")]
    out_dir: PathBuf,
}

/// One runnable test: a test function inside a specific test binary.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct TestCase {
    /// Cargo target name the binary was built from (disambiguates
    /// same-named tests in different crates/targets)
    target: String,
    /// libtest path of the test function (`module::test_name`)
    name: String,
    /// The instrumented test binary
    exe: PathBuf,
}

impl TestCase {
    /// Filesystem-safe unique name: `<target>__<test path>` with every
    /// non-alphanumeric character mapped to `_`.
    fn sanitized(&self) -> String {
        format!("{}__{}", self.target, self.name)
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect()
    }
}

impl Witnesses {
    pub fn run(&self, sh: &Shell) -> Result {
        if self.total_workers == 0 {
            return Err(anyhow!("--total-workers must be at least 1"));
        }
        if !(1..=self.total_workers).contains(&self.worker_number) {
            return Err(anyhow!(
                "--worker-number ({}) must be between 1 and --total-workers ({})",
                self.worker_number,
                self.total_workers
            ));
        }

        let (llvm_profdata, llvm_cov) = llvm_tools(sh)?;

        // The duvet crate `include_str!`s duvet/www/public/script.js, so the
        // web assets must exist before any duvet build.
        {
            let _dir = sh.push_dir("duvet/www");
            cmd!(sh, "make").run()?;
        }

        // The unit tests need the RFC corpus and the pinned IETF snapshots,
        // exactly like `cargo xtask test`.
        crate::tests::download_rfcs(sh)?;
        crate::tests::extract_ietf_snapshots(sh)?;

        let tests = enumerate_tests(sh)?;
        let total = tests.len();

        // Lockbox-style partition: index % total_workers == worker_number - 1
        // over the deterministic (sorted) enumeration.
        let mine: Vec<&TestCase> = tests
            .iter()
            .enumerate()
            .filter(|(index, _)| index % self.total_workers == self.worker_number - 1)
            .map(|(_, test)| test)
            .collect();

        eprintln!(
            "worker {}/{}: {} of {} tests",
            self.worker_number,
            self.total_workers,
            mine.len(),
            total
        );

        if self.list {
            for test in &mine {
                println!("{}", test.sanitized());
            }
            return Ok(());
        }

        sh.create_dir(&self.out_dir)?;
        let raw_dir = self.out_dir.join("raw");
        sh.create_dir(&raw_dir)?;

        let mut failed: Vec<String> = Vec::new();

        for (position, test) in mine.iter().enumerate() {
            let sanitized = test.sanitized();
            eprintln!("[{}/{}] {}", position + 1, mine.len(), sanitized);

            // One process per test. %p keeps child processes (if a test
            // spawns any) from clobbering the parent's profile.
            let profile_pattern = raw_dir.join(format!("{sanitized}-%p.profraw"));
            let exe = &test.exe;
            let name = &test.name;
            let run = {
                let _env = sh.push_env("LLVM_PROFILE_FILE", &profile_pattern);
                cmd!(sh, "{exe} --exact {name} --test-threads 1")
                    .quiet()
                    .ignore_status()
                    .output()?
            };

            if !run.status.success() {
                eprintln!("{}", String::from_utf8_lossy(&run.stdout));
                eprintln!("{}", String::from_utf8_lossy(&run.stderr));
                failed.push(sanitized.clone());
                continue;
            }

            // Collect this test's profraw files (one per process).
            let profraws: Vec<PathBuf> = sh
                .read_dir(&raw_dir)?
                .into_iter()
                .filter(|path| {
                    path.file_name().and_then(|v| v.to_str()).is_some_and(|v| {
                        v.starts_with(&format!("{sanitized}-")) && v.ends_with(".profraw")
                    })
                })
                .collect();
            if profraws.is_empty() {
                return Err(anyhow!(
                    "test {sanitized} produced no .profraw — is the binary instrumented?"
                ));
            }

            let profdata = raw_dir.join(format!("{sanitized}.profdata"));
            let profraw_args = &profraws;
            cmd!(
                sh,
                "{llvm_profdata} merge -sparse {profraw_args...} -o {profdata}"
            )
            .quiet()
            .run()?;

            // Dependency and toolchain sources cannot carry duvet
            // annotations; excluding them keeps each witness small.
            let ignore = r"[/\\]\.cargo[/\\]registry[/\\]|[/\\]rustc[/\\]";
            let tracefile = cmd!(
                sh,
                "{llvm_cov} export --format=lcov --instr-profile {profdata} --object {exe} --ignore-filename-regex {ignore}"
            )
            .quiet()
            .read()?;

            sh.write_file(self.out_dir.join(format!("{sanitized}.info")), tracefile)?;

            // Only the .info witnesses are kept; the intermediates are bulky.
            for profraw in &profraws {
                sh.remove_path(profraw)?;
            }
            sh.remove_path(profdata)?;
        }

        let _ = std::fs::remove_dir(&raw_dir); // succeeds only when empty

        if !failed.is_empty() {
            return Err(anyhow!(
                "{} test(s) failed during witness production: {}",
                failed.len(),
                failed.join(", ")
            ));
        }

        eprintln!(
            "worker {}/{}: wrote {} witnesses to {}",
            self.worker_number,
            self.total_workers,
            mine.len(),
            self.out_dir.display()
        );

        Ok(())
    }
}

/// Build every test binary in the workspace with coverage instrumentation
/// and enumerate their tests. Returns a deterministic (sorted) list.
fn enumerate_tests(sh: &Shell) -> Result<Vec<TestCase>> {
    // A dedicated target dir keeps instrumented artifacts from thrashing
    // the uninstrumented cache (RUSTFLAGS changes every fingerprint).
    let _target = sh.push_env("CARGO_TARGET_DIR", "target/witnesses-build");
    let _flags = sh.push_env("RUSTFLAGS", "-C instrument-coverage");
    // Instrumented build scripts run during the build; keep their profiles
    // out of the working directory.
    let _profile = sh.push_env(
        "LLVM_PROFILE_FILE",
        "target/witnesses-build/build-%p.profraw",
    );

    let json = cmd!(sh, "cargo test --workspace --no-run --message-format=json")
        .quiet()
        .read()?;

    let mut binaries: Vec<(String, PathBuf)> = Vec::new();
    for line in json.lines() {
        let Ok(message) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if message["reason"] != "compiler-artifact" || message["profile"]["test"] != true {
            continue;
        }
        let Some(executable) = message["executable"].as_str() else {
            continue;
        };
        let target = message["target"]["name"]
            .as_str()
            .ok_or_else(|| anyhow!("compiler-artifact without target name"))?
            .to_string();
        binaries.push((target, PathBuf::from(executable)));
    }

    if binaries.is_empty() {
        return Err(anyhow!("cargo produced no test binaries"));
    }

    let mut tests = Vec::new();
    for (target, exe) in &binaries {
        let listing = cmd!(sh, "{exe} --list --format terse")
            .quiet()
            .read()
            .with_context(|| format!("listing tests in {}", exe.display()))?;
        for line in listing.lines() {
            if let Some(name) = line.strip_suffix(": test") {
                tests.push(TestCase {
                    target: target.clone(),
                    name: name.to_string(),
                    exe: exe.clone(),
                });
            }
        }
    }

    tests.sort();
    Ok(tests)
}

/// Locate `llvm-profdata` and `llvm-cov` from the active toolchain's
/// llvm-tools component.
fn llvm_tools(sh: &Shell) -> Result<(PathBuf, PathBuf)> {
    let sysroot = cmd!(sh, "rustc --print sysroot").quiet().read()?;
    let host = cmd!(sh, "rustc -vV")
        .quiet()
        .read()?
        .lines()
        .find_map(|line| line.strip_prefix("host: ").map(str::to_string))
        .ok_or_else(|| anyhow!("could not determine host triple from rustc -vV"))?;

    let bin = Path::new(sysroot.trim())
        .join("lib/rustlib")
        .join(host)
        .join("bin");

    let llvm_profdata = bin.join("llvm-profdata");
    let llvm_cov = bin.join("llvm-cov");

    if !llvm_profdata.exists() || !llvm_cov.exists() {
        return Err(anyhow!(
            "llvm-tools not found under {} — install with `rustup component add llvm-tools`",
            bin.display()
        ));
    }

    Ok((llvm_profdata, llvm_cov))
}
