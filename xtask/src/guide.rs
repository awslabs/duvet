use std::{fs::File, io::Write, path::Path};

use crate::{build::Build, Result};
use clap::Parser;
use xshell::{cmd, Shell};

#[derive(Debug, Default, Parser)]
pub struct Guide {
    #[clap(long)]
    pub dev: bool,
}

impl Guide {
    pub fn run(&self, sh: &Shell) -> Result {
        let configs = sh.read_dir("config")?;

        let dir = Path::new("guide").canonicalize()?;
        let build_dir = dir.join("build");

        if cmd!(sh, "which mdbook").quiet().run().is_err() {
            cmd!(sh, "cargo install --locked mdbook").run()?;
        }

        if cmd!(sh, "which taplo").quiet().run().is_err() {
            cmd!(sh, "cargo install --locked taplo-cli@0.9").run()?;
        }

        if cmd!(sh, "which typos").quiet().run().is_err() {
            cmd!(sh, "cargo install --locked typos-cli@1").run()?;
        }

        let bin = Build {
            profile: "dev".into(),
        }
        .run(sh)?;

        let _path = sh.push_env(
            "PATH",
            format!(
                "{}:{}",
                bin.parent().unwrap().display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        );

        let command_dir = dir.join("src/command");
        sh.create_dir(&command_dir)?;
        for command in ["init", "extract", "report"] {
            let output = cmd!(sh, "duvet {command} --help")
                .ignore_status()
                .output()?;
            let path = command_dir.join(command).with_extension("md");
            let mut file = File::create(path)?;
            writeln!(file, "# `{command}`")?;
            writeln!(file)?;
            writeln!(file, "```console")?;
            file.write_all(&output.stdout)?;
            writeln!(file, "```")?;
            file.flush()?;
        }

        let files = sh.read_dir("guide/src")?;
        cmd!(sh, "typos {files...}").run()?;

        // make sure the example config matches the schema
        let example_config = dir.join("src/example-config.toml");
        cmd!(sh, "taplo fmt {example_config}").run()?;
        // TODO we need to publish the spec first
        // cmd!(sh, "taplo lint {example_config}").run()?;

        let _dir = sh.push_dir(dir);
        if self.dev {
            cmd!(sh, "mdbook serve").run()?;
        } else {
            cmd!(sh, "mdbook build").run()?;
        }

        // copy over the config schemas
        let config_dir = build_dir.join("config");
        sh.create_dir(&config_dir)?;
        for file in configs {
            sh.copy_file(file, &config_dir)?;
        }

        Ok(())
    }
}
