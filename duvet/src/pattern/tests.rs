// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use std::path::{Path, PathBuf};

fn parse(pattern: &str, value: &str) -> Result<Vec<Annotation>, anyhow::Error> {
    let pattern = Pattern::from_arg(pattern).unwrap();
    let path = Path::new("file.rs");
    let mut annotations = Default::default();
    pattern.extract(value, path, &mut annotations)?;

    let annotations = annotations
        .into_iter()
        .map(|mut annotation| {
            // make the manifest dir consistent on all platforms
            annotation.manifest_dir = PathBuf::from("/");
            annotation
        })
        .collect();

    Ok(annotations)
}

macro_rules! snapshot {
    ($name:ident, $value:expr) => {
        snapshot!($name, "//=,//#", $value);
    };
    ($name:ident, $pattern:expr, $value:expr) => {
        #[test]
        fn $name() {
            insta::assert_debug_snapshot!(stringify!($name), parse($pattern, $value));
        }
    };
}

snapshot!(
    content_without_meta,
    r#"
    //# This is some content without meta
    "#
);

snapshot!(
    meta_without_content,
    r#"
    //= type=todo
    "#
);

snapshot!(
    type_citation,
    r#"
    //= https://example.com/spec.txt
    //# Here is my citation
    "#
);

snapshot!(
    type_test,
    r#"
    //= https://example.com/spec.txt
    //= type=test
    //# Here is my citation
    "#
);

snapshot!(
    type_todo,
    r#"
    //= https://example.com/spec.txt
    //= type=todo
    //= feature=cool-things
    //= tracking-issue=123
    //# Here is my citation
    "#
);

snapshot!(
    type_exception,
    r#"
    //= https://example.com/spec.txt
    //= type=exception
    //= reason=This isn't possible currently
    //# Here is my citation
    "#
);

snapshot!(
    missing_new_line,
    r#"
    //= https://example.com/spec.txt
    //# Here is my citation"#
);
