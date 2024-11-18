use crate::Result;
use clap::Parser;
use std::path::PathBuf;
use xshell::{cmd, Shell};

#[derive(Debug, Default, Parser)]
pub struct Build {
    #[clap(long, default_value = "dev")]
    pub profile: String,
}

impl Build {
    pub fn run(&self, sh: &Shell) -> Result<PathBuf> {
        {
            let _dir = sh.push_dir("duvet/www");
            cmd!(sh, "make").run()?;
        }

        let args = vec!["--profile".to_string(), self.profile.clone()];

        cmd!(sh, "cargo build -p duvet {args...}").run()?;

        let path = if self.profile == "dev" {
            sh.current_dir().join("target/debug/duvet")
        } else {
            sh.current_dir()
                .join("target")
                .join(&self.profile)
                .join("duvet")
        };

        Ok(path)
    }
}
