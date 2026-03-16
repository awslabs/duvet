// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use super::*;

fn parse(pattern: &str, value: &str) -> (AnnotationSet, Vec<String>) {
    let file = SourceFile::new("file.rs", value).unwrap();
    let pattern = Pattern::from_arg(pattern).unwrap();
    let (annotations, errors) = extract(&file, &pattern, Default::default(), None);
    let errors = errors.into_iter().map(|error| error.to_string()).collect();
    (annotations, errors)
}

macro_rules! snapshot {
    ($name:ident, $value:expr) => {
        // use a different pattern so we don't register these tests as part of the duvet report
        snapshot!($name, "//@=,//@#", $value);
    };
    ($name:ident, $pattern:expr, $value:expr) => {
        #[test]
        fn $name() {
            let mut settings = insta::Settings::clone_current();
            // ignore CWD
            settings.add_filter(
                &dbg!(duvet_core::env::current_dir()
                    .unwrap()
                    .as_ref()
                    .display()
                    .to_string()
                    .replace('/', "\\/")),
                "[CWD]",
            );
            let _bound = settings.bind_to_scope();
            insta::assert_debug_snapshot!(stringify!($name), parse($pattern, $value));
        }
    };
}

snapshot!(
    content_without_meta,
    r#"
    //@# This is some content without meta
    "#
);

snapshot!(
    meta_without_content,
    r#"
    //@= type=todo
    "#
);

snapshot!(
    type_citation,
    r#"
    //@= https://example.com/spec.txt
    //@# Here is my citation
    "#
);

snapshot!(
    type_citation_with_reason,
    r#"
    //@= https://example.com/spec.txt
    //@= reason=This is why the code does the feature
    //@# Here is my citation
    "#
);

snapshot!(
    type_test,
    r#"
    //@= https://example.com/spec.txt
    //@= type=test
    //@# Here is my citation
    "#
);

snapshot!(
    type_test_with_reason,
    r#"
    //@= https://example.com/spec.txt
    //@= type=test
    //@= reason=This is why this actually tests the feature
    //@# Here is my citation
    "#
);

snapshot!(
    type_todo,
    r#"
    //@= https://example.com/spec.txt
    //@= type=todo
    //@= feature=cool-things
    //@= tracking-issue=123
    //@# Here is my citation
    "#
);

snapshot!(
    type_exception,
    r#"
    //@= https://example.com/spec.txt
    //@= type=exception
    //@= reason=This isn't possible currently
    //@# Here is my citation
    "#
);

snapshot!(
    type_implication,
    r#"
    //@= https://example.com/spec.txt
    //@= type=implication
    //@# Here is my citation
    "#
);

snapshot!(
    type_implication_with_reason,
    r#"
    //@= https://example.com/spec.txt
    //@= type=implication
    //@= reason=This is implied by the protocol design
    //@# Here is my citation
    "#
);

snapshot!(
    type_exception_multiline_reason,
    r#"
    //@= https://example.com/spec.txt
    //@= type=exception
    //@= reason=There's a lot to justify here,
    //@= reason=so this reason needs to be split across two
    //@= reason=or even three lines.
    //@# Before encrypting input plaintext,
    "#
);

snapshot!(
    missing_new_line,
    r#"
    //@= https://example.com/spec.txt
    //@# Here is my citation"#
);
