use crate::{args::FlagExt, Result};
use clap::Parser;
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
        crate::build::Build {
            release: Some(Some(false)),
        }
        .run(sh)?;

        {
            let toolchain = format!("+{}", self.rustfmt_toolchain);
            cmd!(sh, "cargo {toolchain} fmt --all -- --check").run()?;
        }

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
}
