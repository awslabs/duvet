// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use std::process::Command;

fn main() {
    Command::new("sh")
        .arg("-c")
        .arg("make www/public/script.js")
        .output()
        .expect("failed to execute process");
}
