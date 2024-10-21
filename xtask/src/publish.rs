use crate::Result;
use clap::Parser;
use xshell::{cmd, Shell};

#[derive(Debug, Default, Parser)]
pub struct Publish {}

impl Publish {
    pub fn run(&self, sh: &Shell) -> Result {
        crate::build::Build::default().run(sh)?;

        cmd!(sh, "git diff --exit-code").run()?;

        for pkg in ["duvet-macros", "duvet-core", "duvet"] {
            let _dir = sh.push_dir(pkg);
            cmd!(sh, "cargo publish --allow-dirty").run()?;
        }

        Ok(())
    }
}
