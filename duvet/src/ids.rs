// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Entity-typed deterministic IDs for the v2 report schema.
//!
//! Each entity type in the v2 schema has a prefixed ID computed from FNV-1a 64-bit
//! hashing of its content-derived key fields. The prefix indicates the entity type
//! and hash input schema:
//!
//! | Prefix  | Entity                  | Hash input                                                         |
//! |---------|-------------------------|--------------------------------------------------------------------|
//! | `repo-` | Repository              | `blob_link`                                                        |
//! | `src-`  | Inline source (spec)    | `contents` (raw file bytes)                                        |
//! | `lnk-`  | Linked source (code)    | `file_name \0 repository_id`                                       |
//! | `spc-`  | Specification           | `source_id \0 start \0 end` (decimal strings)                      |
//! | `sec-`  | Section                 | `source_id \0 start \0 end` (decimal strings)                      |
//! | `req-`  | Requirement             | `origin_id \0 s1 \0 e1 \0 s2 \0 e2 ... \0 source_id \0 line`       |
//! | `cite-` | Impl annotation         | `source_id \0 line \0 target_source_id`                            |
//!
//! All functions take pre-resolved string inputs and are independently testable.

use std::io::Write;

/// FNV-1a 64-bit hash function.
///
/// Deterministic, no dependencies, sufficient for expected annotation counts.
/// Uses the standard FNV-1a constants for 64-bit hashing.
///
/// Note: 64-bit hashes have a birthday-bound collision probability of ~1 in 2^32
/// (~4 billion) at around 77k entries. This is acceptable for any realistic
/// project, but IDs should not be assumed to be universally unique — a merge
/// tool should detect and handle collisions.
pub(crate) fn fnv1a_64(data: &[u8]) -> u64 {
    const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    let mut hash = FNV_OFFSET_BASIS;
    for &byte in data {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

fn prefixed_id(prefix: &str, data: &[u8]) -> String {
    format!("{prefix}{:016x}", fnv1a_64(data))
}

pub(crate) fn repo_id(blob_link: &str) -> String {
    prefixed_id("repo-", blob_link.as_bytes())
}

pub(crate) fn src_id(contents: &[u8]) -> String {
    prefixed_id("src-", contents)
}

pub(crate) fn lnk_id(file_name: &str, repository_id: &str) -> String {
    let mut buf = Vec::new();
    buf.extend_from_slice(file_name.as_bytes());
    buf.push(0);
    buf.extend_from_slice(repository_id.as_bytes());
    prefixed_id("lnk-", &buf)
}

pub(crate) fn spc_id(source_id: &str, start: usize, end: usize) -> String {
    let mut buf = Vec::new();
    let _ = write!(buf, "{source_id}\0{start}\0{end}");
    prefixed_id("spc-", &buf)
}

pub(crate) fn sec_id(source_id: &str, start: usize, end: usize) -> String {
    let mut buf = Vec::new();
    let _ = write!(buf, "{source_id}\0{start}\0{end}");
    prefixed_id("sec-", &buf)
}

pub(crate) fn req_id(
    origin_id: &str,
    ranges: &[(usize, usize)],
    source_id: &str,
    line: usize,
) -> String {
    let mut sorted: Vec<(usize, usize)> = ranges.to_vec();
    sorted.sort();
    let mut buf = Vec::new();
    let _ = write!(buf, "{origin_id}");
    for (start, end) in &sorted {
        let _ = write!(buf, "\0{start}\0{end}");
    }
    let _ = write!(buf, "\0{source_id}\0{line}");
    prefixed_id("req-", &buf)
}

pub(crate) fn cite_id(source_id: &str, line: usize, target_source_id: &str) -> String {
    let mut buf = Vec::new();
    let _ = write!(buf, "{source_id}\0{line}\0{target_source_id}");
    prefixed_id("cite-", &buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv1a_64_known_vectors() {
        // Reference: http://www.isthe.com/chongo/tech/comp/fnv/
        assert_eq!(fnv1a_64(b""), 0xcbf29ce484222325);
        assert_eq!(fnv1a_64(b"a"), 0xaf63dc4c8601ec8c);
        assert_eq!(fnv1a_64(b"foobar"), 0x85944171f73967e8);
    }

    #[test]
    fn repo_id_known_vector() {
        assert_eq!(
            repo_id("https://github.com/org/repo/blob/main"),
            "repo-2a688096043d13a6"
        );
    }

    #[test]
    fn src_id_known_vector() {
        assert_eq!(src_id(b"hello world"), "src-779a65e7023cd2e7");
    }

    #[test]
    fn lnk_id_known_vector() {
        assert_eq!(lnk_id("src/lib.rs", "repo-abc123"), "lnk-944cdb82e9f1060d");
    }

    #[test]
    fn lnk_id_no_repository() {
        assert_eq!(lnk_id("src/lib.rs", ""), "lnk-45b7397fa826f08e");
    }

    #[test]
    fn spc_id_known_vector() {
        assert_eq!(spc_id("src-aaa", 0, 98765), "spc-dfc8916cc1aac4ca");
    }

    #[test]
    fn sec_id_known_vector() {
        assert_eq!(sec_id("src-aaa", 0, 98765), "sec-dfc8916cc1aac4ca");
    }

    #[test]
    fn spc_and_sec_ids_share_hash_differ_by_prefix() {
        let spc = spc_id("src-aaa", 0, 100);
        let sec = sec_id("src-aaa", 0, 100);
        assert_eq!(&spc[4..], &sec[4..]);
        assert!(spc.starts_with("spc-"));
        assert!(sec.starts_with("sec-"));
    }

    #[test]
    fn req_id_known_vector() {
        assert_eq!(
            req_id("src-aaa", &[(10, 35)], "lnk-bbb", 7),
            req_id("src-aaa", &[(10, 35)], "lnk-bbb", 7)
        );
        // Different authoring sites must produce different IDs for the same spec range.
        assert_ne!(
            req_id("src-aaa", &[(10, 35)], "lnk-bbb", 7),
            req_id("src-aaa", &[(10, 35)], "lnk-ccc", 7)
        );
        // Different lines at the same authoring file must produce different IDs.
        assert_ne!(
            req_id("src-aaa", &[(10, 35)], "lnk-bbb", 7),
            req_id("src-aaa", &[(10, 35)], "lnk-bbb", 8)
        );
        // Range permutation must produce the same ID.
        assert_eq!(
            req_id("src-aaa", &[(10, 35), (40, 50)], "lnk-bbb", 7),
            req_id("src-aaa", &[(40, 50), (10, 35)], "lnk-bbb", 7)
        );
        // Different range sets must produce different IDs.
        assert_ne!(
            req_id("src-aaa", &[(10, 35)], "lnk-bbb", 7),
            req_id("src-aaa", &[(10, 35), (40, 50)], "lnk-bbb", 7)
        );
    }

    #[test]
    fn cite_id_known_vector() {
        assert_eq!(cite_id("lnk-bbb", 42, "src-aaa"), "cite-1a2fb6b5abf8ae91");
    }

    // ── Property-based tests ─────────────────────────────────────────────

    /// Every ID function produces `{prefix}` followed by exactly 16 lowercase hex chars.
    #[test]
    fn format_invariant() {
        use bolero::check;

        fn check_format(id: &str, prefix: &str) {
            assert!(id.starts_with(prefix), "{id} missing prefix {prefix}");
            let hex = &id[prefix.len()..];
            assert_eq!(hex.len(), 16, "{id} hex part wrong length");
            assert!(
                hex.chars()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
                "{id} contains non-hex chars"
            );
        }

        check!().with_type::<String>().for_each(|s| {
            check_format(&repo_id(s), "repo-");
            check_format(&src_id(s.as_bytes()), "src-");
            check_format(&lnk_id(s, s), "lnk-");
            check_format(&spc_id(s, 0, 100), "spc-");
            check_format(&sec_id(s, 0, 100), "sec-");
            check_format(&req_id(s, &[(0, 100)], s, 0), "req-");
            check_format(&cite_id(s, 0, s), "cite-");
        });
    }

    /// Calling each function twice with the same input produces the same output.
    #[test]
    fn determinism() {
        use bolero::check;

        check!().with_type::<String>().for_each(|s| {
            assert_eq!(repo_id(s), repo_id(s));
            assert_eq!(src_id(s.as_bytes()), src_id(s.as_bytes()));
            assert_eq!(lnk_id(s, s), lnk_id(s, s));
            assert_eq!(spc_id(s, 42, 99), spc_id(s, 42, 99));
            assert_eq!(sec_id(s, 42, 99), sec_id(s, 42, 99));
            assert_eq!(req_id(s, &[(42, 99)], s, 3), req_id(s, &[(42, 99)], s, 3));
            assert_eq!(cite_id(s, 7, s), cite_id(s, 7, s));
        });
    }

    /// Swapping field values across the \0 separator in lnk_id produces different IDs.
    #[test]
    fn separator_safety_lnk() {
        use bolero::check;

        check!().with_type::<(String, String)>().for_each(|(a, b)| {
            if a != b {
                assert_ne!(lnk_id(a, b), lnk_id(b, a));
            }
            // Embedding \0 in a field must not collide with the separator
            let fused = format!("{a}\0{b}");
            assert_ne!(lnk_id(&fused, ""), lnk_id(a, b));
        });
    }

    /// Swapping start/end in spc_id produces different IDs.
    #[test]
    fn separator_safety_spc() {
        use bolero::check;

        check!()
            .with_type::<(String, usize, usize)>()
            .for_each(|(s, start, end)| {
                if start != end {
                    assert_ne!(spc_id(s, *start, *end), spc_id(s, *end, *start));
                }
            });
    }

    /// Swapping start/end in sec_id produces different IDs.
    #[test]
    fn separator_safety_sec() {
        use bolero::check;

        check!()
            .with_type::<(String, usize, usize)>()
            .for_each(|(s, start, end)| {
                if start != end {
                    assert_ne!(sec_id(s, *start, *end), sec_id(s, *end, *start));
                }
            });
    }

    /// Swapping field values across the \0 separator in cite_id produces different IDs.
    #[test]
    fn separator_safety_cite() {
        use bolero::check;

        check!()
            .with_type::<(String, usize, String)>()
            .for_each(|(a, line, b)| {
                if a != b {
                    assert_ne!(cite_id(a, *line, b), cite_id(b, *line, a));
                }
            });
    }

    /// req_id is invariant under permutation of input ranges.
    #[test]
    fn req_id_range_permutation() {
        use bolero::check;

        check!()
            .with_type::<(String, String, usize, Vec<(usize, usize)>)>()
            .for_each(|(origin, source, line, ranges)| {
                let mut reversed = ranges.clone();
                reversed.reverse();
                assert_eq!(
                    req_id(origin, ranges, source, *line),
                    req_id(origin, &reversed, source, *line)
                );
            });
    }
}
