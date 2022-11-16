// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::{Arguments, Error};
use insta::assert_json_snapshot;
use std::{ffi::OsString, path::Path};
use structopt::StructOpt;
use tempfile::TempDir;

type Result<T = (), E = Error> = core::result::Result<T, E>;

struct Env {
    dir: TempDir,
}

#[allow(dead_code)] // don't warn on unused testing framework code
impl Env {
    fn new() -> Result<Self> {
        Ok(Self {
            dir: tempfile::tempdir()?,
        })
    }

    fn put(&self, path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> Result {
        let path = self.dir.path().join(path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, contents)?;
        Ok(())
    }

    fn get(&self, path: impl AsRef<Path>) -> Result<String> {
        let path = self.dir.path().join(path);
        Ok(std::fs::read_to_string(path)?)
    }

    fn get_json(&self, path: impl AsRef<Path>) -> Result<serde_json::Value> {
        let path = self.dir.path().join(path);
        let file = std::fs::File::open(path)?;
        let value = serde_json::from_reader(file)?;
        Ok(value)
    }

    fn exec<I>(&self, args: I) -> Result
    where
        I: IntoIterator,
        I::Item: Into<OsString> + Clone,
    {
        std::env::set_current_dir(&self.dir)?;
        Arguments::from_iter_safe(
            ["duvet".into()]
                .into_iter()
                .chain(args.into_iter().map(|v| v.into())),
        )?
        .exec()?;
        Ok(())
    }
}

#[test]
fn markdown_report() -> Result {
    let env = Env::new()?;

    env.put(
        "src/my-code.rs",
        r#"
//= my-spec.md#testing
//# This quote MUST work
//# * with
//# * bullets
        "#,
    )?;

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

    env.exec([
        "report",
        "--source-pattern",
        "src/*.rs",
        "--json",
        "target/report.json",
    ])?;

    let out = env.get_json("target/report.json")?;

    assert_json_snapshot!(out["specifications"]);

    Ok(())
}
