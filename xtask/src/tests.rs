use crate::{args::FlagExt as _, Result};
use clap::Parser;
use serde::Deserialize;
use std::path::{Path, PathBuf};
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

    #[clap(flatten)]
    build: crate::build::Build,
}

impl Tests {
    pub fn run(&self, sh: &Shell) -> Result {
        let bin = self.build.run(sh)?;

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
            .filter_map(|test| {
                if !test.extension().map_or(false, |ext| ext == "toml") {
                    return None;
                }
                let file = sh.read_file(&test).unwrap();
                let test: IntegrationTest = toml::from_str(&file).unwrap();
                Some(test)
            })
            .collect::<Vec<IntegrationTest>>();

        sh.create_dir("target/integration/")?;

        for test in &tests {
            test.fetch(sh)?;
        }

        let prev_path = sh.var("PATH")?;
        let _path = sh.push_env(
            "PATH",
            format!("{}:{prev_path}", bin.parent().unwrap().display()),
        );

        if self.update_snapshots.is_enabled(false) {
            std::env::set_var("INSTA_UPDATE", "always");
        };

        for test in &tests {
            test.run(sh)?;
        }

        Ok(())
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct IntegrationTest {
    name: String,
    repo: String,
    version: String,
    cmd: Vec<String>,
    html_report: PathBuf,
}

impl IntegrationTest {
    fn fetch(&self, sh: &Shell) -> Result {
        let Self {
            name,
            repo,
            version,
            ..
        } = self;

        let target = format!("target/integration/{name}");
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

        Ok(())
    }

    fn run(&self, sh: &Shell) -> Result {
        let Self {
            name,
            cmd,
            html_report,
            ..
        } = self;

        let json_report = {
            let target = format!("target/integration/{name}");
            let _dir = sh.push_dir(&target);
            let json_report = sh.current_dir().join("duvet_report.json");
            let html_report = sh.current_dir().join(html_report);
            let _env = sh.push_env("DUVET_INTERNAL_CI_JSON", json_report.display().to_string());

            for cmd in cmd {
                let mut args = cmd.split(' ');
                sh.cmd(args.next().unwrap()).args(args).run()?;
            }

            assert!(html_report.exists());
            assert!(json_report.exists());

            json_report
        };

        let json_file = sh.read_file(&json_report)?;
        let json: serde_json::Value = serde_json::from_str(&json_file)?;

        let mut settings = insta::Settings::clone_current();

        settings.set_snapshot_path(sh.current_dir().join("integration/snapshots"));
        settings.set_prepend_module_to_snapshot(false);

        settings.bind(|| {
            insta::assert_json_snapshot!(name.to_string(), json);
        });

        Ok(())
    }
}
