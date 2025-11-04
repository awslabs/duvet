// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use super::whitespace;
use core::{fmt, ops::Range};
use triple_accel::levenshtein::levenshtein_search_simd_with_opts as text_search;

#[derive(Clone, Copy, Debug)]
pub enum Kind {
    Exact,
    Fuzzy,
}

impl Kind {
    #[inline]
    pub fn is_fuzzy(&self) -> bool {
        matches!(self, Self::Fuzzy)
    }
}

pub fn find(needle: &str, haystack: &str) -> Option<(Range<usize>, Kind)> {
    if needle.is_empty() {
        return None;
    }

    macro_rules! try_find {
        ($find:expr, $kind:expr) => {
            if let Some(range) = $find {
                return Some((range, $kind));
            }
        };
    }

    // try finding without ignoring whitespace first
    try_find!(fast_find(needle, haystack), Kind::Exact);

    let normalized_search = NormalizedSearch::new(needle, haystack);

    try_find!(normalized_search.find(fast_find), Kind::Exact);

    try_find!(normalized_search.find(fuzzy_find), Kind::Fuzzy);

    None
}

fn fast_find(needle: &str, haystack: &str) -> Option<Range<usize>> {
    haystack.find(needle).map(|start| {
        let end = start + needle.len();
        debug_assert_eq!(&haystack[start..end], needle);
        start..end
    })
}

/// TODO we should probably deprecate this - it's better to enforce strict matching
fn fuzzy_find(needle: &str, haystack: &str) -> Option<Range<usize>> {
    text_search(
        needle.as_bytes(),
        haystack.as_bytes(),
        1,
        triple_accel::SearchType::Best,
        triple_accel::levenshtein::LEVENSHTEIN_COSTS,
        false,
    )
    .map(|m| m.start..m.end)
    .next()
}

struct NormalizedSearch<'a> {
    needle: String,
    haystack: String,
    original_haystack: &'a str,
    offset_map: Vec<usize>,
}

impl fmt::Debug for NormalizedSearch<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        struct Mapping<'a> {
            formatted: &'a str,
            original: &'a str,
            mapping: &'a [usize],
        }

        impl fmt::Debug for Mapping<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                let mut m = f.debug_map();
                for (idx, ch) in self.formatted.char_indices() {
                    let start = self.mapping[idx];
                    let end = self.mapping[idx + 1];
                    let c = &self.original[start..end];
                    m.entry(&ch, &(c, start..end));
                }
                m.finish()
            }
        }

        f.debug_struct("NormalizedSearch")
            .field("needle", &self.needle)
            .field("haystack", &self.haystack)
            .field(
                "mapping",
                &Mapping {
                    formatted: &self.haystack,
                    original: self.original_haystack,
                    mapping: &self.offset_map,
                },
            )
            .finish()
    }
}

impl<'a> NormalizedSearch<'a> {
    fn new(needle: &str, original_haystack: &'a str) -> Self {
        let needle = whitespace::normalize(needle);
        let (haystack, offset_map) = whitespace::normalize_mapped(original_haystack);
        Self {
            needle,
            haystack,
            original_haystack,
            offset_map,
        }
    }

    fn find(&self, find: fn(&str, &str) -> Option<Range<usize>>) -> Option<Range<usize>> {
        let range = find(&self.needle, &self.haystack)?;
        let start = self.offset_map[range.start];
        let end = self.offset_map[range.end];

        // trim any whitespace at the end
        let original = &self.original_haystack[start..end];
        let end = start + original.trim_end().len();

        Some(start..end)
    }
}

#[cfg(test)]
mod tests {
    use super::Kind;
    use core::ops::Range;

    fn find<'a>(needle: &str, haystack: &'a str) -> Option<(Range<usize>, Kind, &'a str)> {
        super::find(needle, haystack).map(|(r, kind)| (r.clone(), kind, &haystack[r]))
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
    find_test!(
        punctuation_test,
        "  Second   Sentence.  ",
        " First    sentence.   Second Sentence.   Third  Sentence.   "
    );

    fn normalize_whitespace(value: &str) -> (String, Vec<usize>) {
        let (normalized, mapping) = super::whitespace::normalize_mapped::<Vec<usize>>(value);

        dbg!(value, &normalized);
        let mut prev: Option<char> = None;

        for (idx, ch) in normalized.char_indices() {
            if let Some(prev) = prev {
                if prev.is_whitespace() || !prev.is_alphanumeric() {
                    assert!(!ch.is_whitespace());
                }
            }
            prev = Some(ch);

            let start = mapping[idx];
            let end = mapping[idx + ch.len_utf8()];
            let c = &value[start..end];
            assert!(!c.is_empty(), "{mapping:?}");
        }

        (normalized, mapping)
    }

    #[test]
    fn normalize_test() {
        bolero::check!().with_type::<String>().for_each(|s| {
            let _ = normalize_whitespace(s);
        });
    }

    #[test]
    fn foo_test() {
        let (a, _) = normalize_whitespace("This is a test.Foo.[F]");
        let (b, _) = normalize_whitespace("  This is    a    test.  Foo  . [F]");
        let (c, _) = normalize_whitespace(" This is    a    test.  Foo  . [F]  ");
        let (d, _) = normalize_whitespace("This    is    a    test.  Foo  . [    F ]  ");
        assert_eq!(a, b);
        assert_eq!(a, c);
        assert_eq!(a, d);
    }
}
