// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Verified post-pass for classifier output (spec §1.3 mutual-exclusivity).
//!
//! The tree-sitter walk stamps every line spanned by an AST node with that
//! node's property — including lines where no code *starts*. A multi-line
//! `local_variable_declaration` paints `Statement` on a pure-comment
//! continuation line, for example. The post-pass removes these spurious
//! *semantic* properties (`Statement`, `Declaration`, `NonLinearControl`) from
//! non-code-start lines, but **never** removes *structural* properties
//! (`ScopeOpen`, `ScopeClose`).
//!
//! # Proven property
//!
//! `clean_classifications` guarantees **structural preservation**: for every
//! line that carries `ScopeOpen` or `ScopeClose` in the input, the output
//! retains that property. This is the bridge between the classifier's output
//! and the `scopes_match_classifications` precondition that Property 2 (no
//! cross-scope leakage) requires.
//!
//! # Trusted base
//!
//! The `code_start` vector is produced by the (unverified) tree-sitter walk.
//! We trust it to be the correct length. The proven property holds regardless
//! of its *contents* — even if `code_start` is all-false or all-true, scope
//! delimiters are preserved. That is: the proof does not depend on
//! `code_start` being correct, only on the function's own logic.

use crate::types::*;
use std::collections::BTreeSet;
use vstd::prelude::*;

verus! {

/// Post-pass: clean a classifier's per-line property sets by removing spurious
/// semantic properties from non-code-start lines, while preserving structural
/// scope delimiters unconditionally.
///
/// # Arguments
/// * `classifications` — the per-line property sets (0-indexed; element i
///   corresponds to source line i+1). `None` entries pass through unchanged.
/// * `code_start` — parallel boolean vector. `code_start[i]` is true iff a
///   real code/structural AST node *starts* on line i+1. Length must equal
///   `classifications.len()`.
///
/// # Ensures (structural preservation)
/// For every index `i` where the input `classifications[i]` contains
/// `ScopeOpen`, the output also contains `ScopeOpen` at that index.
/// Symmetrically for `ScopeClose`.
pub fn clean_classifications(
    classifications: &mut Vec<Option<BTreeSet<LineProperty>>>,
    code_start: &[bool],
)
    requires
        old(classifications).len() == code_start@.len(),
    ensures
        classifications.len() == old(classifications).len(),
        // STRUCTURAL PRESERVATION: ScopeOpen never stripped
        forall|i: int| 0 <= i < old(classifications).len()
            && (#[trigger] old(classifications)@[i]).is_some()
            && old(classifications)@[i].unwrap()@.contains(LineProperty::ScopeOpen)
            ==> classifications@[i].is_some()
                && classifications@[i].unwrap()@.contains(LineProperty::ScopeOpen),
        // STRUCTURAL PRESERVATION: ScopeClose never stripped
        forall|i: int| 0 <= i < old(classifications).len()
            && (#[trigger] old(classifications)@[i]).is_some()
            && old(classifications)@[i].unwrap()@.contains(LineProperty::ScopeClose)
            ==> classifications@[i].is_some()
                && classifications@[i].unwrap()@.contains(LineProperty::ScopeClose),
{
    let len = classifications.len();
    let mut idx: usize = 0;

    while idx < len
        invariant
            len == classifications.len(),
            len == code_start@.len(),
            0 <= idx <= len,
            // All processed lines preserve ScopeOpen
            forall|i: int| 0 <= i < idx as int
                && (#[trigger] old(classifications)@[i]).is_some()
                && old(classifications)@[i].unwrap()@.contains(LineProperty::ScopeOpen)
                ==> classifications@[i].is_some()
                    && classifications@[i].unwrap()@.contains(LineProperty::ScopeOpen),
            // All processed lines preserve ScopeClose
            forall|i: int| 0 <= i < idx as int
                && (#[trigger] old(classifications)@[i]).is_some()
                && old(classifications)@[i].unwrap()@.contains(LineProperty::ScopeClose)
                ==> classifications@[i].is_some()
                    && classifications@[i].unwrap()@.contains(LineProperty::ScopeClose),
            // Unprocessed lines are unchanged
            forall|i: int| idx as int <= i < len as int
                ==> classifications@[i] == old(classifications)@[i],
        decreases len - idx,
    {
        if let Some(ref mut props) = classifications[idx] {
            let has_annotation = props.contains(&LineProperty::Annotation);
            let has_whitespace = props.contains(&LineProperty::Whitespace);
            let has_comment = props.contains(&LineProperty::Comment);
            let is_code_start = code_start[idx];

            if has_annotation || has_whitespace || (has_comment && !is_code_start) {
                // Non-code line: strip semantic properties only.
                // ScopeOpen and ScopeClose are NEVER removed.
                props.remove(&LineProperty::Statement);
                props.remove(&LineProperty::Declaration);
                props.remove(&LineProperty::NonLinearControl);
            } else if has_comment && is_code_start {
                // Trailing comment on a real code line: canonicalize to pure
                // code (the model reads Comment behind a `len == 1` guard, so
                // this is aesthetics, not correctness).
                props.remove(&LineProperty::Comment);
            }
        }

        idx = idx + 1;
    }
}

} // verus!

#[cfg(test)]
mod tests {
    use super::*;

    fn lc(props: &[LineProperty]) -> Option<BTreeSet<LineProperty>> {
        Some(props.iter().copied().collect())
    }

    /// The core bug: a `} // comment` line must retain ScopeClose after cleaning.
    #[test]
    fn scope_close_preserved_on_comment_line() {
        let mut classifications = vec![
            lc(&[LineProperty::ScopeClose, LineProperty::Comment]),
        ];
        let code_start = vec![false]; // no code starts here (it's an end-line)

        clean_classifications(&mut classifications, &code_start);

        let props = classifications[0].as_ref().unwrap();
        assert!(
            props.contains(&LineProperty::ScopeClose),
            "ScopeClose must survive cleaning on a comment line, got: {:?}",
            props
        );
    }

    /// ScopeOpen on an annotation line must also survive.
    #[test]
    fn scope_open_preserved_on_annotation_line() {
        let mut classifications = vec![
            lc(&[LineProperty::ScopeOpen, LineProperty::Annotation, LineProperty::Declaration]),
        ];
        let code_start = vec![true];

        clean_classifications(&mut classifications, &code_start);

        let props = classifications[0].as_ref().unwrap();
        assert!(
            props.contains(&LineProperty::ScopeOpen),
            "ScopeOpen must survive cleaning, got: {:?}",
            props
        );
        // Declaration should be stripped (annotation line)
        assert!(
            !props.contains(&LineProperty::Declaration),
            "Declaration should be stripped from annotation line, got: {:?}",
            props
        );
    }

    /// Statement IS stripped from a non-code-start comment line.
    #[test]
    fn statement_stripped_from_comment_only_line() {
        let mut classifications = vec![
            lc(&[LineProperty::Statement, LineProperty::Comment]),
        ];
        let code_start = vec![false];

        clean_classifications(&mut classifications, &code_start);

        let props = classifications[0].as_ref().unwrap();
        assert!(!props.contains(&LineProperty::Statement));
        assert!(props.contains(&LineProperty::Comment));
    }

    /// Statement is kept on a code-start line with trailing comment.
    #[test]
    fn statement_kept_on_code_start_line() {
        let mut classifications = vec![
            lc(&[LineProperty::Statement, LineProperty::Comment]),
        ];
        let code_start = vec![true];

        clean_classifications(&mut classifications, &code_start);

        let props = classifications[0].as_ref().unwrap();
        assert!(props.contains(&LineProperty::Statement));
        // Comment is stripped (trailing comment canonicalization)
        assert!(!props.contains(&LineProperty::Comment));
    }

    /// None entries pass through unchanged.
    #[test]
    fn none_entries_unchanged() {
        let mut classifications: Vec<Option<BTreeSet<LineProperty>>> = vec![None, None];
        let code_start = vec![false, true];

        clean_classifications(&mut classifications, &code_start);

        assert!(classifications[0].is_none());
        assert!(classifications[1].is_none());
    }

    /// Full scenario: the exact `} // end bar` case from the bug report.
    /// Line 4 in `public class Foo { void bar() { doX(); } // end bar }`.
    #[test]
    fn closing_brace_with_trailing_comment_regression() {
        // Simulates the classifier output for line 4: `} // end bar`
        // The walk_node puts ScopeClose (from the block end) and Comment
        // (from the line_comment node). code_start is false because the
        // block *ends* here, it doesn't *start* here.
        let mut classifications = vec![
            lc(&[LineProperty::ScopeClose, LineProperty::Comment]),
        ];
        let code_start = vec![false];

        clean_classifications(&mut classifications, &code_start);

        let props = classifications[0].as_ref().unwrap();
        assert!(
            props.contains(&LineProperty::ScopeClose),
            "ScopeClose MUST survive: this is the Property 2 precondition. Got: {:?}",
            props
        );
        // Comment stays (it's the authoritative non-code property)
        assert!(props.contains(&LineProperty::Comment));
    }
}
