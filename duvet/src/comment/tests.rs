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

/// `Annotation::line_range()` must span only annotation-comment lines,
/// never real code. The coverage override stamps `{Annotation}` over that exact
/// range (`query/checks/coverage.rs`); if the range could extend onto a
/// `Statement`/`ScopeClose` line it would unbalance the scope tree. This pins
/// the parser-side guarantee: a block is a *contiguous* run of meta/content
/// lines (the tokenizer emits tokens only for `//@=` / `//@#` lines and the
/// parser flushes on any line gap), so `line_range()` never reaches the code
/// that follows. Twin of `annotation_line_is_pure_even_across_multiline_span`.
#[test]
fn annotation_line_range_covers_only_comment_lines() {
    // A multi-line annotation embedded in code: a blank line and a real
    // statement follow immediately, with no gap before the statement.
    let source = "\
public class Foo {
    void bar() {
        //@= https://example.com/spec.txt
        //@= type=implication
        //@# This spans several lines
        doWork();
    }
}";
    let (annotations, errors) = parse("//@=,//@#", source);
    assert!(errors.is_empty(), "unexpected parse errors: {errors:?}");
    assert_eq!(annotations.len(), 1, "expected exactly one annotation");

    let lines: Vec<&str> = source.lines().collect();
    let annotation = annotations.iter().next().unwrap();
    let (start, end) = annotation.line_range();

    // Every line in the (1-based, inclusive) range is a comment line.
    for line_num in start..=end {
        let content = lines[(line_num - 1) as usize].trim_start();
        assert!(
            content.starts_with("//@=") || content.starts_with("//@#"),
            "line {line_num} in line_range() is not an annotation comment: {content:?}"
        );
    }

    // The line immediately after the range is the real statement — proving the
    // range stops before code rather than swallowing it.
    let after = lines[end as usize].trim_start();
    assert_eq!(after, "doWork();");
}
