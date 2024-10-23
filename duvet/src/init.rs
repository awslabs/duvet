// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::{annotation::AnnotationType, comment, config, manifest, Result};
use clap::Parser;
use duvet_core::{env, path::Path};
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

static DETECTORS: &[(&str, &[&str], fn(&mut Vec<manifest::Source>, &Path))] = &[
    ("Rust", &["Cargo.toml"], |sources, dir| {
        sources.push(manifest::Source {
            pattern: "src/**/*.rs".parse().unwrap(),
            root: dir.into(),
            comment_style: Default::default(),
            default_type: Default::default(),
        });
        sources.push(manifest::Source {
            pattern: "test/**/*.rs".parse().unwrap(),
            root: dir.into(),
            comment_style: Default::default(),
            default_type: AnnotationType::Test,
        });
    }),
    ("JavaScript", &["package.json"], |sources, dir| {
        sources.push(manifest::Source {
            pattern: "**/*.js".parse().unwrap(),
            root: dir.into(),
            comment_style: Default::default(),
            default_type: Default::default(),
        });
    }),
    ("TypeScript", &["tsconfig.json"], |sources, dir| {
        sources.push(manifest::Source {
            pattern: "**/*.ts".parse().unwrap(),
            root: dir.into(),
            comment_style: Default::default(),
            default_type: Default::default(),
        });
    }),
    (
        "Python",
        &["requirements.txt", "setup.py"],
        |sources, dir| {
            sources.push(manifest::Source {
                pattern: "**/*.py".parse().unwrap(),
                root: dir.into(),
                comment_style: comment::Pattern {
                    meta: "#=".into(),
                    content: "##".into(),
                },
                default_type: Default::default(),
            });
        },
    ),
    (
        "Java",
        &["build.gradle", "settings.gradle"],
        |sources, dir| {
            sources.push(manifest::Source {
                pattern: "main/**/*.java".parse().unwrap(),
                root: dir.into(),
                comment_style: Default::default(),
                default_type: Default::default(),
            });
            sources.push(manifest::Source {
                pattern: "test/**/*.java".parse().unwrap(),
                root: dir.into(),
                comment_style: Default::default(),
                default_type: AnnotationType::Test,
            });
        },
    ),
];

#[derive(Debug, Parser)]
pub struct Init {
    #[clap(long, default_value = "auto")]
    color: ColorChoice,
}

impl Init {
    pub async fn exec(&self) -> Result {
        let path = self.path().await?;

        if path.exists() {
            return Err(anyhow::anyhow!("{} already exists", path.display()).into());
        }

        let mut stdout = StandardStream::stdout(self.color);

        let root = config::project_from_config(&path)
            .await
            .expect("could not load root");

        let detected = self.detect_sources(&root, &mut stdout).await;

        let mut out = Vec::new();

        {
            use std::io::Write;

            let mut out = std::io::Cursor::new(&mut out);
            let mut needs_break = false;

            macro_rules! w {
                ($($tt:tt)*) => {
                    if core::mem::take(&mut needs_break) {
                        writeln!(out).unwrap();
                    }
                    writeln!(out, $($tt)*).unwrap();
                }
            }

            w!("version = \"1\"");
            needs_break = true;

            for source in detected {
                w!("[source]");

                w!("pattern = {:?}", source.pattern);

                w!(
                    "comment-style = {{ meta = {:?}, content = {:?} }}",
                    source.comment_style.meta,
                    source.comment_style.content
                );

                if source.default_type != AnnotationType::default() {
                    w!("type = {:?}", source.default_type.to_string());
                }

                needs_break = true;
            }
        }

        tokio::fs::create_dir_all(path.parent().unwrap()).await?;
        tokio::fs::write(&path, out).await?;

        stdout.write_status("Initialized", format_args!("{path}"), None)?;

        Ok(())
    }

    async fn path(&self) -> Result<Path> {
        Ok(env::current_dir()?.join(config::DEFAULT_PATH))
    }

    async fn detect_sources(
        &self,
        dir: &Path,
        stdout: &mut StandardStream,
    ) -> Vec<manifest::Source> {
        let mut sources = vec![];

        for (name, paths, source) in DETECTORS {
            for path in paths.iter() {
                if dir.join(path).exists() {
                    source(&mut sources, dir);
                    stdout
                        .write_status(
                            "Detected",
                            format_args!("{name} project"),
                            Some(Color::Blue),
                        )
                        .unwrap();
                    break;
                }
            }
        }

        if sources.is_empty() {
            stdout
                .write_status("Detection", "skipped", Some(Color::Yellow))
                .unwrap();
        }

        sources
    }
}

trait StatusExt: std::io::Write + WriteColor {
    fn write_status<T: core::fmt::Display>(
        &mut self,
        status: &str,
        value: T,
        color: Option<Color>,
    ) -> std::io::Result<()> {
        let max_len = 12;
        let spacing = max_len - status.len();
        for _ in 0..spacing {
            write!(self, " ")?;
        }
        let color = color.unwrap_or(Color::Green);
        self.set_color(ColorSpec::new().set_fg(Some(color)).set_bold(true))?;
        write!(self, "{status}")?;
        self.reset()?;
        writeln!(self, " {value}")?;
        Ok(())
    }
}

impl<T: std::io::Write + WriteColor> StatusExt for T {}
