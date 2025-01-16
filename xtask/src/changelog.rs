// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::Result;
use clap::Parser;
use xshell::{cmd, Shell};

#[derive(Debug, Default, Parser)]
pub struct Changelog {}

impl Changelog {
    pub fn run(&self, sh: &Shell) -> Result {
        cmd!(
            sh,
            "npx conventional-changelog-cli -p conventionalcommits -i CHANGELOG.md -s"
        )
        .run()?;

        Ok(())
    }
}
