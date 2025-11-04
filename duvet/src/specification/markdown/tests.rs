// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use super::{parse, tokenizer::tokens};
use duvet_core::file::SourceFile;

macro_rules! snapshot {
    ($name:ident, $contents:expr) => {
        mod $name {
            #[test]
            fn tokens() {
                let contents = super::SourceFile::new("index.md", $contents).unwrap();
                insta::assert_debug_snapshot!(
                    "tokens",
                    super::tokens(&contents).collect::<Vec<_>>()
                );
            }

            #[test]
            fn tree() {
                let contents = super::SourceFile::new("index.md", $contents).unwrap();
                insta::assert_debug_snapshot!("tree", super::parse(&contents));
            }
        }
    };
}

snapshot!(
    simple,
    r#"
# This is a test

Content goes here. Another
sentence here.
"#
);

snapshot!(
    multi_line_header,
    r#"
Foo *bar
baz*
======

Content goes here. Another
sentence here.
"#
);

snapshot!(
    multi_line_header_strong_heading_attrs,
    r#"
Foo **bar
baz** {#blah}
======

Content goes here. Another
sentence here.
"#
);

snapshot!(
    multi_line_header_link_heading_attrs,
    r#"
Foo **bar
baz** [I'm link](http://something) {#blah}
======

Content goes here. Another
sentence here.
"#
);

snapshot!(
    multiple,
    r#"
# This is a test

Content goes here. Another
sentence here.

## This is another test

More content goes here

### Nested section

Testing 123

## Up one

Another section
"#
);

snapshot!(
    list_example,
    r#"
# List example

Here is a list:
* Item 1
* Item 2
  * Item 2.1
* Item 3
  * Item 3.1
    * Item 3.1.1
    * Item 3.1.2
  * Item 3.2

Here is a numbered list:
1. Item 1
2. Item 2
3. Item 3

Here is a list with content:
* Item
  More content

  Other content

  * Testing

    Other test

Testing 123

* Item
More content
"#
);

snapshot!(
    duplicate_sections,
    r#"
# Duplicate header

testing 123

## Duplicate header

other test
"#
);

snapshot!(
    heading_attributes,
    r#"
# Heading with ID {#custom-id}

Content under heading with custom ID.

## Another heading {#another-id}

More content here.

# Regular heading

This heading doesn't have a custom ID.
"#
);
