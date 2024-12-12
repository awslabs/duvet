// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use core::ops::Range;
use triple_accel::levenshtein::levenshtein_search as text_search;

/// In order to make text matching a little nicer to work with, we split on any punctuation,
/// rather than require strict matching
static PUNCTUATION: &[char] = &[
    '!', '"', '#', '$', '%', '&', '\'', '(', ')', '*', '+', ',', '-', '.', '/', ':', ';', '<', '=',
    '>', '?', '@', '[', '\\', ']', '^', '_', '`', '{', '|', '}', '~',
];

pub fn find(needle: &str, haystack: &str) -> Option<Range<usize>> {
    if needle.is_empty() {
        return None;
    }

    // try finding without ignoring whitespace first
    fast_find(needle, haystack)
        .or_else(|| fuzzy_find(needle, haystack))
        .or_else(|| slow_find(needle, haystack))
}

fn fast_find(needle: &str, haystack: &str) -> Option<Range<usize>> {
    haystack.find(needle).map(|start| {
        let end = start + needle.bytes().len();
        debug_assert_eq!(&haystack[start..end], needle);
        start..end
    })
}

/// TODO we should probaly deprecate this - it's better to enforce strict matching
fn fuzzy_find(needle: &str, haystack: &str) -> Option<Range<usize>> {
    text_search(needle.as_bytes(), haystack.as_bytes())
        .filter(|m| m.k < 2)
        .min_by_key(|m| (m.k, m.start))
        .map(|m| m.start..m.end)
}

fn slow_find(needle: &str, haystack: &str) -> Option<Range<usize>> {
    let (needle, _) = normalize_whitespace(needle);
    let (haystack, offset_map) = normalize_whitespace(haystack);
    let range = fuzzy_find(&needle, &haystack)?;

    let start = offset_map[range.start];
    let end = offset_map[range.end];

    Some(start..end)
}

fn normalize_whitespace(value: &str) -> (String, Vec<usize>) {
    let mut offset_map = Vec::with_capacity(value.len() + 1);
    let mut out = String::with_capacity(value.len());

    let value_start = value.as_ptr() as usize;
    let mut trimmed_end = 0;

    for word in value.split_whitespace() {
        for word in word.split_inclusive(PUNCTUATION) {
            let start = word.as_ptr() as usize - value_start;
            let end = start + word.len();
            trimmed_end = end;

            if !out.is_empty() {
                out.push(' ');
                offset_map.push(start);
            }
            out.push_str(word);
            offset_map.extend(start..end);
        }
    }

    offset_map.push(trimmed_end);

    debug_assert_eq!(out.len() + 1, offset_map.len());

    (out, offset_map)
}

#[cfg(test)]
mod tests {
    use core::ops::Range;

    fn find<'a>(needle: &str, haystack: &'a str) -> Option<(Range<usize>, &'a str)> {
        super::find(needle, haystack).map(|r| (r.clone(), &haystack[r]))
    }

    macro_rules! find_test {
        ($name:ident, $needle:expr, $haystack:expr) => {
            #[test]
            fn $name() {
                insta::assert_debug_snapshot!(stringify!($name), find($needle, $haystack));
            }
        };
    }

    find_test!(empty, "", "");
    find_test!(start, "a", "a b c d");
    find_test!(start_2, "a b", "a b c d");
    find_test!(middle, "b", "a b c d");
    find_test!(middle_2, "b c", "a b c d");
    find_test!(end, "d", "a b c d");
    find_test!(end_2, "c d", "a b c d");
    find_test!(
        ws_difference,
        "     this       should   ignore whitespace      differences",
        "         this             should       ignore       whitespace            differences"
    );
    find_test!(
        hyphenated_haystack,
        "this is a new-line",
        "this is a new-\nline"
    );
    find_test!(
        hyphenated_needle,
        "this is a new-\nline",
        "this is a new-line"
    );
}
