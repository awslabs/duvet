use crate::{args::FlagExt as _, Result};
use clap::Parser;
use std::path::PathBuf;
use xshell::{cmd, Shell};

#[derive(Debug, Default, Parser)]
pub struct Build {
    #[clap(long)]
    pub release: Option<Option<bool>>,
}

impl Build {
    pub fn run(&self, sh: &Shell) -> Result<PathBuf> {
        {
            let _dir = sh.push_dir("duvet/www");
            cmd!(sh, "make").run()?;
        }

        let mut args = vec![];

        let is_release = self.release.is_enabled(true);

        if is_release {
            args.push("--release".to_string());
        }

        cmd!(sh, "cargo build -p duvet {args...}").run()?;

        let path = if is_release {
            sh.current_dir().join("target/release/duvet")
        } else {
            sh.current_dir().join("target/debug/duvet")
        };

        Ok(path)
    }
}
