// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::Result;
use insta::assert_json_snapshot;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

struct Env {
    dir: TempDir,
}

#[allow(dead_code)] // don't warn on unused testing framework code
impl Env {
    fn new() -> Result<Self> {
        let dir = tempfile::tempdir()?;
        Ok(Self { dir })
    }

    fn put(&self, path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> Result<String> {
        let path = self.path(path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, contents)?;
        Ok(path.display().to_string())
    }

    fn get(&self, path: impl AsRef<Path>) -> Result<String> {
        let path = self.path(path);
        Ok(std::fs::read_to_string(path)?)
    }

    fn get_json(&self, path: impl AsRef<Path>) -> Result<serde_json::Value> {
        let path = self.path(path);
        let file = std::fs::File::open(path)?;
        let value = serde_json::from_reader(file)?;
        Ok(value)
    }

    fn path(&self, path: impl AsRef<Path>) -> PathBuf {
        self.dir.path().join(path)
    }

    async fn exec<I>(&self, args: I) -> Result
    where
        I: IntoIterator,
        I::Item: Into<String> + Clone,
    {
        duvet_core::env::set_args(
            ["duvet".into()]
                .into_iter()
                .chain(args.into_iter().map(|v| v.into()))
                .collect(),
        );
        duvet_core::env::set_current_dir(self.dir.path().into());
        crate::run().await?;
        Ok(())
    }
}

#[tokio::test]
async fn markdown_report() -> Result {
    let env = Env::new()?;

    let spec = env.put(
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
        format!(
            r#"
//= {spec}#testing
//# This quote MUST work
//# * with
//# * bullets
        "#,
        ),
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

    assert_json_snapshot!(out["specifications"][&spec]);

    Ok(())
}

#[tokio::test]
async fn inner_whitespace() -> Result {
    let env = Env::new()?;

    let spec = env.put(
        "my-spec.md",
        r#"
# Testing

This      SHOULD         ignore        whitespace.
        "#,
    )?;

    let code = env.put(
        "src/my-code.rs",
        format!(
            r#"
//= {spec}#testing
//# This SHOULD             ignore         whitespace.
            "#
        ),
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

    assert_json_snapshot!(out["specifications"][&spec]);

    Ok(())
}
