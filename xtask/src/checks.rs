// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::{args::FlagExt, Result};
use anyhow::anyhow;
use clap::Parser;
use std::path::Path;
use xshell::{cmd, Shell};

#[derive(Debug, Parser)]
pub struct Checks {
    #[clap(long)]
    enforce_warnings: Option<Option<bool>>,

    #[clap(long, default_value = "nightly")]
    rustfmt_toolchain: String,
}

impl Checks {
    pub fn run(&self, sh: &Shell) -> Result {
        self.copyright(sh)?;

        {
            let toolchain = format!("+{}", self.rustfmt_toolchain);
            cmd!(sh, "cargo {toolchain} fmt --all -- --check").run()?;
        }

        if cmd!(sh, "which typos").quiet().run().is_err() {
            cmd!(sh, "cargo install --locked typos-cli").run()?;
        }

        cmd!(sh, "typos").run()?;

        crate::build::Build {
            profile: "dev".into(),
        }
        .run(sh)?;

        let mut clippy_args = vec![];
        if self.enforce_warnings.is_enabled(true) {
            clippy_args.extend(["-D", "warnings"]);
        };

        cmd!(
            sh,
            "cargo clippy --all-features --all-targets -- {clippy_args...}"
        )
        .run()?;
        Ok(())
    }

    fn copyright(&self, sh: &Shell) -> Result {
        let files = cmd!(sh, "git ls-tree -r --name-only HEAD").read()?;

        let mut is_ok = true;

        for file in files.lines() {
            let file = Path::new(file);
            let Some(ext) = file.extension().and_then(|v| v.to_str()) else {
                continue;
            };
            if !["rs", "js"].contains(&ext) {
                continue;
            }
            let contents = sh.read_file(file)?;
            let has_copyright = contents
                .lines()
                .take(3)
                .any(|line| line.contains("Copyright"));

            if !has_copyright {
                eprintln!("{} missing copyright header", file.display());
                is_ok = false;
            }
        }

        if !is_ok {
            return Err(anyhow!("Failed copyright check"));
        }

        Ok(())
    }
}
