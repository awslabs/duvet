// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::{args::FlagExt as _, Result};
use clap::Parser;
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
use xshell::{cmd, Shell};

#[derive(Debug, Parser)]
pub struct Tests {
    #[clap(long)]
    /// Enables the unit tests
    unit: Option<Option<bool>>,

    #[clap(long)]
    /// Enables the integration tests
    integration: Option<Option<bool>>,

    #[clap(long)]
    update_snapshots: Option<Option<bool>>,

    #[clap(long)]
    /// Enables all of the default tests
    default_tests: Option<Option<bool>>,
}

impl Tests {
    pub fn run(&self, sh: &Shell) -> Result {
        let bin = crate::build::Build {
            // compile in dev mode with optimizations
            profile: "release-debug".into(),
        }
        .run(sh)?;

        let default_tests = self.default_tests.is_enabled(true);

        self.download_rfcs(sh)?;

        if self.unit.is_enabled(default_tests) {
            if !sh.path_exists("duvet/src/specification/ietf/snapshots") {
                let _dir = sh.push_dir("duvet/src/specification/ietf");
                cmd!(sh, "tar -xf snapshots.tar.gz").run()?;
            }

            cmd!(sh, "cargo test").run()?;
        }

        if self.integration.is_enabled(default_tests) {
            self.integration(sh, &bin)?;
        }

        Ok(())
    }

    fn download_rfcs(&self, sh: &Shell) -> Result {
        let url = "https://www.rfc-editor.org/rfc/tar/RFC-all.tar.gz";

        let dir = "target/www.rfc-editor.org";
        sh.create_dir(dir)?;
        let _dir = sh.push_dir(dir);

        let tar_gz = Path::new("RFC-all.tar.gz");
        if !sh.path_exists(tar_gz) {
            eprintln!("downloading {url}");
            cmd!(sh, "curl --fail --output {tar_gz} {url}").run()?;
            cmd!(sh, "tar -xf {tar_gz}").run()?;
        }

        for file in sh.read_dir(".")? {
            if file.ends_with(tar_gz) {
                continue;
            }

            if let Some(ext) = file.extension().and_then(|v| v.to_str()) {
                if ext != "txt" {
                    sh.remove_path(file)?;
                }
            } else {
                sh.remove_path(file)?;
            }
        }

        Ok(())
    }

    fn integration(&self, sh: &Shell, bin: &Path) -> Result {
        let tests = sh.read_dir("integration")?;

        let tests = tests
            .into_iter()
            .filter_map(|path| {
                let ext = path.extension()?;
                if ext != "toml" {
                    return None;
                }
                let file = sh.read_file(&path).unwrap();
                let mut test: IntegrationTest = toml::from_str(&file).unwrap();
                test.name = path.file_stem().unwrap().to_string_lossy().to_string();
                Some(test)
            })
            .collect::<Vec<IntegrationTest>>();

        sh.create_dir("target/integration/")?;

        let mut targets = vec![];
        for test in &tests {
            targets.push(test.init(sh)?);
        }

        let prev_path = sh.var("PATH")?;
        let _path = sh.push_env(
            "PATH",
            format!("{}:{prev_path}", bin.parent().unwrap().display()),
        );

        if self.update_snapshots.is_enabled(false) {
            std::env::set_var("INSTA_UPDATE", "always");
        };

        for (test, target) in tests.iter().zip(targets.iter()) {
            test.run(target, sh)?;
        }

        Ok(())
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct IntegrationTest {
    #[serde(skip)]
    name: String,
    source: IntegrationSource,
    cmd: Vec<String>,
    #[serde(rename = "file", default)]
    files: Vec<IntegrationFile>,
    #[serde(default)]
    env: BTreeMap<String, String>,
    #[serde(default)]
    cwd: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct IntegrationFile {
    path: String,
    contents: String,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum IntegrationSource {
    Git { repo: String, version: String },
    Local { local: bool },
}

impl IntegrationSource {
    fn is_local(&self) -> bool {
        matches!(self, Self::Local { .. })
    }

    fn init(&self, test: &IntegrationTest, sh: &Shell) -> Result<PathBuf> {
        match self {
            Self::Git { repo, version } => {
                assert!(
                    test.files.is_empty(),
                    "virtual files are only supported for local tests"
                );
                Self::init_git(test, repo, version, sh)
            }
            Self::Local { local } => {
                assert!(*local);
                assert!(
                    !test.files.is_empty(),
                    "local tests need at least one virtual file"
                );
                let target = Self::init_local(test, sh)?;

                for IntegrationFile { path, contents } in test.files.iter() {
                    sh.write_file(target.join(path), contents)?;
                }

                Ok(target)
            }
        }
    }

    fn init_local(test: &IntegrationTest, sh: &Shell) -> Result<PathBuf> {
        let target = Path::new("target/integration").join(&test.name);
        let _ = sh.remove_path(&target);
        sh.create_dir(&target)?;
        Ok(target)
    }

    fn init_git(test: &IntegrationTest, repo: &str, version: &str, sh: &Shell) -> Result<PathBuf> {
        let target = Path::new("target/integration").join(&test.name);
        // allow LFS to run
        let _env = sh.push_env("GIT_CLONE_PROTECTION_ACTIVE", "false");

        if !sh.path_exists(&target) {
            cmd!(sh, "git clone --recurse-submodules {repo} {target}").run()?;
        }

        let _dir = sh.push_dir(&target);

        let target_hash = cmd!(sh, "git rev-parse {version}").read()?;
        let current_hash = cmd!(sh, "git rev-parse HEAD").read()?;

        if target_hash != current_hash {
            cmd!(sh, "git reset --hard {target_hash}").run()?;
            cmd!(sh, "git fetch --recurse-submodules").run()?;
        }

        Ok(target)
    }
}

impl IntegrationTest {
    fn init(&self, sh: &Shell) -> Result<PathBuf> {
        self.source.init(self, sh)
    }

    fn run(&self, target: &Path, sh: &Shell) -> Result {
        let Self { name, cmd, cwd, .. } = self;

        let (stderr, json_report, snapshot_report) = {
            let target_dir = if let Some(cwd) = cwd {
                target.join(cwd)
            } else {
                target.to_path_buf()
            };
            let _dir = sh.push_dir(&target_dir);
            let html_report = sh.current_dir().join("duvet_report.html");
            let json_report = sh.current_dir().join("duvet_report.json");
            let snapshot_report = sh.current_dir().join("duvet_snapshot.txt");

            // override this variable if we're in the duvet CI
            let _env = sh.push_env("CI", "false");
            let _env = sh.push_env("DUVET_INTERNAL_CI", "true");
            let _env = sh.push_env("DUVET_INTERNAL_CI_HTML", html_report.display().to_string());
            let _env = sh.push_env("DUVET_INTERNAL_CI_JSON", json_report.display().to_string());
            let _env = sh.push_env(
                "DUVET_INTERNAL_CI_SNAPSHOT",
                snapshot_report.display().to_string(),
            );

            let mut env = vec![];
            for (key, value) in &self.env {
                env.push(sh.push_env(key, value));
            }

            let mut stderr = String::new();

            for cmd in cmd {
                let mut args = cmd.split(' ');
                let runner = sh.cmd(args.next().unwrap()).args(args);
                // local tests are allowed to fail
                if self.source.is_local() {
                    let output = runner.ignore_status().output()?;
                    if !output.status.success() {
                        stderr.push_str(&format!("$ {cmd}\n"));
                        stderr.push_str(&format!("EXIT: {:?}\n", output.status.code()));
                        stderr.push_str(&String::from_utf8_lossy(&output.stderr));
                        continue;
                    }
                } else {
                    runner.run()?;
                }
            }

            if stderr.is_empty() {
                assert!(html_report.exists());
                assert!(json_report.exists());
                assert!(snapshot_report.exists());
            }

            (stderr, json_report, snapshot_report)
        };

        let mut settings = insta::Settings::clone_current();

        settings.set_snapshot_path(sh.current_dir().join("integration/snapshots"));
        settings.set_prepend_module_to_snapshot(false);

        if !stderr.is_empty() {
            settings.bind(|| {
                insta::assert_snapshot!(format!("{name}_stderr"), stderr);
            });
            return Ok(());
        }

        let json_file = sh.read_file(&json_report)?;
        let json: serde_json::Value = serde_json::from_str(&json_file)?;

        let snapshot = sh.read_file(&snapshot_report)?;

        settings.bind(|| {
            insta::assert_snapshot!(format!("{name}"), snapshot);
            insta::assert_json_snapshot!(format!("{name}_json"), json);
        });

        Ok(())
    }
}
