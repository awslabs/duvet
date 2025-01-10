// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

/// Synchronizes a value to the file system
///
/// When the `CI` environment variable is set, this method ensures the value matches
/// what is on disk.
pub fn sync(path: impl AsRef<Path>, value: impl AsRef<str>) {
    let path = path.as_ref();
    let value = value.as_ref();
    if std::env::var("CI").is_err() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, value).unwrap();
        return;
    }

    let actual = std::fs::read_to_string(path).unwrap();
    assert_eq!(actual, value);
}
