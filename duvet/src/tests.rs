// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::{Arguments, Result};
use clap::Parser;
use duvet_core::diagnostic::{Context as _, IntoDiagnostic as _};
use insta::assert_json_snapshot;
use std::{
    ffi::OsString,
    io::Read,
    path::{Path, PathBuf},
};
use tempfile::TempDir;

macro_rules! assert_error_snapshot {
    ($error:expr) => {
        let error = strip_ansi_escapes::strip_str($error.snapshot().to_string());
        insta::assert_snapshot!(error);
    };
}

struct Env {
    dir: TempDir,
}

#[allow(dead_code)] // don't warn on unused testing framework code
impl Env {
    fn new() -> Result<Self> {
        let dir = tempfile::tempdir()?;
        duvet_core::env::set_current_dir(dir.path().into());
        Ok(Self { dir })
    }

    fn put(&self, path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> Result<String> {
        let path = path.as_ref();
        let full_path = self.path(path);
        if let Some(parent) = full_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&full_path, contents)?;
        let path = full_path.display().to_string();
        Ok(path)
    }

    fn get(&self, path: impl AsRef<Path>) -> Result<String> {
        let mut out = String::new();
        self.get_file(path)?.read_to_string(&mut out)?;
        Ok(out)
    }

    fn get_json(&self, path: impl AsRef<Path>) -> Result<serde_json::Value> {
        let file = self.get_file(path)?;
        let value = serde_json::from_reader(file)?;
        Ok(value)
    }

    fn get_file(&self, path: impl AsRef<Path>) -> Result<std::fs::File> {
        let path = self.path(path);
        let file = std::fs::File::open(&path)
            .into_diagnostic()
            .wrap_err_with(|| path.display().to_string())?;
        Ok(file)
    }

    fn path(&self, path: impl AsRef<Path>) -> PathBuf {
        self.dir.path().join(path)
    }

    async fn exec<I>(&self, args: I) -> Result
    where
        I: IntoIterator,
        I::Item: Into<OsString> + Clone,
    {
        Arguments::try_parse_from(
            ["duvet".into()]
                .into_iter()
                .chain(args.into_iter().map(|v| v.into())),
        )
        .into_diagnostic()?
        .exec()
        .await?;
        Ok(())
    }
}

#[tokio::test]
async fn markdown_report() -> Result {
    let env = Env::new()?;

    env.put(
        "my-spec.md",
        r#"
# My spec

here is a spec

## Testing

This quote MUST work
* with
* bullets
        "#,
    )?;

    let code = env.put(
        "src/my-code.rs",
        r#"
//= my-spec.md#testing
//# This quote MUST work
//# * with
//# * bullets
        "#,
    )?;

    let target = env.path("target/report.json");

    env.exec([
        "report",
        "--source-pattern",
        &code,
        "--json",
        &target.display().to_string(),
    ])
    .await?;

    let out = env.get_json(&target)?;

    assert_json_snapshot!(out["specifications"]["my-spec.md"]);

    Ok(())
}

#[tokio::test]
async fn inner_whitespace() -> Result {
    let env = Env::new()?;

    env.put(
        "my-spec.md",
        r#"
# Testing

This      SHOULD         ignore        whitespace.
        "#,
    )?;

    let code = env.put(
        "src/my-code.rs",
        r#"
//= my-spec.md#testing
//# This SHOULD             ignore         whitespace.
        "#,
    )?;

    let out = env.path("target/report.json");

    env.exec([
        "report",
        "--source-pattern",
        &code,
        "--json",
        &out.display().to_string(),
    ])
    .await?;

    let out = env.get_json(&out)?;

    assert_json_snapshot!(out["specifications"]["my-spec.md"]);

    Ok(())
}

#[tokio::test]
async fn invalid_section() -> Result {
    let env = Env::new()?;

    env.put(
        "my-spec.md",
        r#"
# Section

here is a spec
        "#,
    )?;

    let code = env.put(
        "src/my-code.rs",
        r#"
//= my-spec.md#foo
//# This quote MUST NOT work
        "#,
    )?;

    let target = env.path("target/report.json");

    let err = env
        .exec([
            "report",
            "--source-pattern",
            &code,
            "--json",
            &target.display().to_string(),
        ])
        .await
        .unwrap_err();

    assert_error_snapshot!(err);

    Ok(())
}

#[tokio::test]
async fn invalid_quote() -> Result {
    let env = Env::new()?;

    env.put(
        "my-spec.md",
        r#"
# Section

here is a spec
        "#,
    )?;

    let code = env.put(
        "src/my-code.rs",
        r#"
//= my-spec.md#section
//# Here is missing text
        "#,
    )?;

    let target = env.path("target/report.json");

    let err = env
        .exec([
            "report",
            "--source-pattern",
            &code,
            "--json",
            &target.display().to_string(),
        ])
        .await
        .unwrap_err();

    assert_error_snapshot!(err);

    Ok(())
}
