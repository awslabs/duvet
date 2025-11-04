// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::Result;
use clap::Parser;
use duvet_core::{progress, vfs as fs};
use std::{fs::File, io::Write, path::Path};

static GITIGNORE: &str = r#"
reports/
"#;

#[derive(Debug, Parser)]
pub struct Init {
    #[clap(long)]
    /// Include the given specification in the configuration
    specification: Vec<String>,

    #[clap(flatten)]
    languages: Languages,
}

impl Init {
    pub async fn exec(&self) -> Result {
        let mut languages = self.languages;

        if languages.is_empty() {
            languages.detect().await;
        }

        let dir = Path::new(".duvet");

        std::fs::create_dir_all(dir)?;

        macro_rules! put {
            ($path:expr, $writer:expr) => {{
                let path = dir.join($path);
                let progress = progress!("Writing {}", path.display());
                if path.exists() {
                    progress!(progress, "Skipping {} - already exists", path.display());
                } else {
                    let mut out = File::create_new(&path)?;
                    let result: Result = ($writer)(&mut out);
                    result?;
                    out.flush()?;
                    drop(out);

                    progress!(progress, "Wrote {}", path.display());
                }
            }};
        }

        put!("config.toml", |out: &mut File| {
            writeln!(out, "'$schema' = {:?}", crate::config::schema::DEFAULT)?;
            writeln!(out)?;

            languages.write(out)?;

            if self.specification.is_empty() {
                writeln!(out, "# Include required specifications here")?;
                writeln!(out, "# [[specification]]")?;
                writeln!(
                    out,
                    "# source = {:?}",
                    "https://www.rfc-editor.org/rfc/rfc2324"
                )?;
            } else {
                for spec in &self.specification {
                    writeln!(out, "[[specification]]")?;
                    writeln!(out, "source = {spec:?}")?;
                }
            }

            writeln!(out, "[report.html]")?;
            writeln!(out, "enabled = true")?;
            // TODO detect git repo
            writeln!(out)?;

            writeln!(
                out,
                "# Enable snapshots to prevent requirement coverage regressions"
            )?;
            writeln!(out, "[report.snapshot]")?;
            writeln!(out, "enabled = true")?;

            Ok(())
        });

        put!(".gitignore", |out: &mut File| {
            write!(out, "{}", GITIGNORE.trim_start())?;
            Ok(())
        });

        Ok(())
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Parser)]
struct Languages {
    /// Include rules for the C language
    #[clap(long = "lang-c")]
    c: bool,
    /// Include rules for the Go language
    #[clap(long = "lang-go")]
    go: bool,
    /// Include rules for the Java language
    #[clap(long = "lang-java")]
    java: bool,
    /// Include rules for the JavaScript language
    #[clap(long = "lang-javascript")]
    javascript: bool,
    /// Include rules for the Python language
    #[clap(long = "lang-python")]
    python: bool,
    /// Include rules for the TypeScript language
    #[clap(long = "lang-typescript")]
    typescript: bool,
    /// Include rules for the Ruby language
    #[clap(long = "lang-ruby")]
    ruby: bool,
    /// Include rules for the Rust language
    #[clap(long = "lang-rust")]
    rust: bool,
}

impl Languages {
    fn is_empty(&self) -> bool {
        Self::default().eq(self)
    }

    fn write<O: Write>(&self, out: &mut O) -> Result {
        if self.is_empty() {
            writeln!(out, "# Specify source code patterns here")?;
            writeln!(out, "# [[source]]")?;
            writeln!(out, "# pattern = {:?}", "src/**/*.rs")?;
            writeln!(out)?;
            return Ok(());
        }

        macro_rules! write {
            ($lang:ident) => {
                if self.$lang {
                    lang::$lang(out)?;
                }
            };
        }

        write!(c);
        write!(go);
        write!(java);
        write!(javascript);
        write!(python);
        write!(typescript);
        write!(ruby);
        write!(rust);

        Ok(())
    }

    async fn detect(&mut self) {
        async fn check(path: &str) -> bool {
            fs::read_metadata(path).await.is_ok()
        }

        if check("CMakeLists.txt").await {
            self.c = true;
        }

        if check("go.mod").await {
            self.go = true;
        }

        if check("pom.xml").await || check("build.gradle").await || check("build.gradle.kts").await
        {
            self.java = true;
        }

        if check("package.json").await {
            self.javascript = true;
        }

        if check("requirements.txt").await
            || check("pyproject.toml").await
            || check("setup.py").await
        {
            self.python = true;
        }

        if check("tsconfig.json").await {
            self.typescript = true;
        }

        if check("Gemfile").await {
            self.ruby = true;
        }

        if check("Cargo.toml").await {
            self.rust = true;
        }
    }
}

mod lang {
    use super::*;

    macro_rules! lang {
        ($name:ident, $pattern:expr $(, $extra:expr)?) => {
            pub fn $name<O: Write>(out: &mut O) -> Result {
                writeln!(out, "[[source]]")?;
                writeln!(out, "pattern = {:?}", $pattern)?;
                $(
                    writeln!(out, "{}", $extra)?;
                )?
                writeln!(out)?;
                Ok(())
            }
        };
    }

    lang!(
        c,
        "src/**/*.c",
        format_args!(
            "comment-style = {{ meta = {:?}, content = {:?} }}",
            "*=", "*#"
        )
    );
    lang!(go, "src/**/*.go");
    lang!(java, "src/**/*.java");
    lang!(javascript, "src/**/*.js");
    lang!(
        python,
        "src/**/*.py",
        format_args!(
            "comment-style = {{ meta = {:?}, content = {:?} }}",
            "##=", "##%"
        )
    );
    lang!(typescript, "src/**/*.ts");
    lang!(
        ruby,
        "lib/**/*.rb",
        format_args!(
            "comment-style = {{ meta = {:?}, content = {:?} }}",
            "##=", "##%"
        )
    );
    lang!(rust, "src/**/*.rs");
}
