// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use super::*;

fn tokens(contents: &str) -> Vec<Token> {
    Lex::new(contents).collect()
}

macro_rules! snapshot {
    ($name:ident, $contents:expr) => {
        #[test]
        fn $name() {
            insta::assert_debug_snapshot!(concat!(stringify!($name), "_tokens"), tokens($contents));
            insta::assert_debug_snapshot!(concat!(stringify!($name), "_tree"), parse($contents));
        }
    };
}

snapshot!(
    simple,
    r#"
# This is a test

Content goes here

## This is another test

More content goes here

### Nested section

Testing 123

## Up one

Another section
"#
);
