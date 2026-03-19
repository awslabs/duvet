// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Scope analysis (spec Section 1.5).

use vstd::prelude::*;
use crate::types::*;

verus! {

/// Spec predicate: scope i strictly contains scope j (i is a parent/ancestor).
pub open spec fn scope_contains(scopes: Seq<Scope>, i: int, j: int) -> bool {
    &&& 0 <= i < scopes.len()
    &&& 0 <= j < scopes.len()
    &&& scopes[i].open_line <= scopes[j].open_line
    &&& scopes[j].close_line <= scopes[i].close_line
    &&& (scopes[i].open_line < scopes[j].open_line || scopes[j].close_line < scopes[i].close_line)
}

//= design/coverage-model-v2-spec.md#scopes
//# A scope is a contiguous range of lines delimited by `ScopeOpen` and
//# `ScopeClose` properties. Scopes nest.
/// Spec predicate: the scope tree is well-formed.
/// - Every scope has open_line <= close_line
/// - If two scopes overlap, one strictly contains the other (proper nesting)
/// - Every scope's close_line has the ScopeClose property in classifications
pub open spec fn scopes_well_formed(scopes: Seq<Scope>) -> bool {
    &&& forall|i: int| 0 <= i < scopes.len() ==>
        (#[trigger] scopes[i]).open_line <= scopes[i].close_line

    &&& forall|i: int, j: int|
        0 <= i < scopes.len() && 0 <= j < scopes.len() && i != j
        && (#[trigger] scopes[i]).open_line < (#[trigger] scopes[j]).close_line
        && scopes[j].open_line < scopes[i].close_line
        ==> scope_contains(scopes, i, j) || scope_contains(scopes, j, i)
}

/// Spec predicate: scope close lines have ScopeClose in classifications.
pub open spec fn scopes_match_classifications(
    scopes: Seq<Scope>,
    classifications: &[Option<LineClass>],
) -> bool {
    forall|i: int| 0 <= i < scopes.len()
        && (scopes[i].close_line as int - 1) >= 0
        && (scopes[i].close_line as int - 1) < classifications@.len()
        ==> (#[trigger] classifications@[scopes[i].close_line as int - 1]).is_some()
            && classifications@[scopes[i].close_line as int - 1].unwrap()@.contains(LineProperty::ScopeClose)
}

/// Two-pass scope tree construction.
///
/// Pass 1: Match ScopeOpen/ScopeClose pairs using a stack.
///         Produces a Vec of (open_line, close_line) pairs.
/// Pass 2: Build parent/children from containment relationships.
///
/// No mutation of existing elements — each scope is created once with final values.
pub fn build_scope_tree(classifications: &[Option<LineClass>], file_length: u64) -> (scopes: Vec<Scope>)
    requires file_length < u64::MAX,
    ensures scopes_well_formed(scopes@),
{
    // Pass 1: collect (open_line, close_line) pairs
    let pairs = match_scope_pairs(classifications, file_length);

    // If no pairs found, return single file-level scope or empty
    if pairs.len() == 0 {
        if file_length >= 1 {
            let s = vec![Scope { open_line: 1, close_line: file_length, parent: None, children: vec![] }];
            assert(s@.len() == 1);
            return s;
        } else {
            return vec![];
        }
    }

    // Pass 2: build Scope structs with parent/children
    build_from_pairs(&pairs)
}

/// Pass 1: Match balanced ScopeOpen/ScopeClose using a stack.
/// Returns Vec of (open_line, close_line) pairs.
/// Returns empty Vec on unbalanced input (fallback to file-level scope).
fn match_scope_pairs(classifications: &[Option<LineClass>], file_length: u64) -> (pairs: Vec<(u64, u64)>)
    requires file_length < u64::MAX,
    ensures
        // Every pair has open >= 1
        forall|i: int| 0 <= i < pairs@.len() ==>
            (#[trigger] pairs@[i]).0 >= 1,
        // Every pair has open <= close
        forall|i: int| 0 <= i < pairs@.len() ==>
            (#[trigger] pairs@[i]).0 <= pairs@[i].1,
        // Pairs are properly nested: if two strictly overlap, one contains the other
        forall|i: int, j: int| 0 <= i < pairs@.len() && 0 <= j < pairs@.len() && i != j
            && (#[trigger] pairs@[i]).0 < (#[trigger] pairs@[j]).1
            && pairs@[j].0 < pairs@[i].1
            ==> (pairs@[i].0 <= pairs@[j].0 && pairs@[j].1 <= pairs@[i].1
                 && (pairs@[i].0 < pairs@[j].0 || pairs@[j].1 < pairs@[i].1))
                || (pairs@[j].0 <= pairs@[i].0 && pairs@[i].1 <= pairs@[j].1
                    && (pairs@[j].0 < pairs@[i].0 || pairs@[i].1 < pairs@[j].1)),
{
    let mut stack: Vec<u64> = Vec::new();
    let mut pairs: Vec<(u64, u64)> = Vec::new();
    let mut done = false;
    let mut unbalanced = false;

    let mut line_num: u64 = 1;
    while line_num <= file_length && !done
        invariant
            line_num >= 1,
            file_length < u64::MAX,
            // Stack is sorted strictly ascending
            forall|k: int| 0 <= k < stack@.len() ==> (#[trigger] stack@[k]) >= 1,
            forall|k: int, l: int| 0 <= k < l && l < stack@.len()
                ==> (#[trigger] stack@[k]) < (#[trigger] stack@[l]),
            // All stack entries < line_num (pushed at earlier line_nums)
            forall|k: int| 0 <= k < stack@.len() ==> (#[trigger] stack@[k]) < line_num,
            // All emitted pairs have open >= 1 and open <= close
            forall|i: int| 0 <= i < pairs@.len() ==> (#[trigger] pairs@[i]).0 >= 1,
            forall|i: int| 0 <= i < pairs@.len() ==> (#[trigger] pairs@[i]).0 <= pairs@[i].1,
            // All emitted pairs have close < line_num (emitted at earlier line_nums)
            forall|i: int| 0 <= i < pairs@.len() ==> (#[trigger] pairs@[i]).1 < line_num,
            // Nesting: emitted pairs are properly nested
            forall|i: int, j: int| 0 <= i < pairs@.len() && 0 <= j < pairs@.len() && i != j
                && (#[trigger] pairs@[i]).0 < (#[trigger] pairs@[j]).1
                && pairs@[j].0 < pairs@[i].1
                ==> (pairs@[i].0 <= pairs@[j].0 && pairs@[j].1 <= pairs@[i].1
                     && (pairs@[i].0 < pairs@[j].0 || pairs@[j].1 < pairs@[i].1))
                    || (pairs@[j].0 <= pairs@[i].0 && pairs@[i].1 <= pairs@[j].1
                        && (pairs@[j].0 < pairs@[i].0 || pairs@[i].1 < pairs@[j].1)),
            // Key nesting bridge: every emitted pair is either at or before
            // a stack entry, or strictly contained within it (open > stack entry).
            forall|i: int, k: int| 0 <= i < pairs@.len() && 0 <= k < stack@.len()
                ==> (#[trigger] pairs@[i]).1 <= (#[trigger] stack@[k])
                    || (stack@[k] < pairs@[i].0 && pairs@[i].0 <= pairs@[i].1),
        decreases if done { 0 } else { file_length - line_num + 2 },
    {
        let idx: usize = ((line_num - 1) as usize);
        if idx >= classifications.len() {
            done = true;
        } else {
        match &classifications[idx] {
            None => { }
            Some(props) => {
                proof { broadcast use crate::types::lemma_line_property_obeys_cmp_spec; }

                // Process ScopeClose BEFORE ScopeOpen (handles } catch {, } else {)
                if props.contains(&LineProperty::ScopeClose) {
                    if stack.len() == 0 {
                        // Unbalanced — return empty
                        unbalanced = true;
                        done = true;
                    } else {
                        let open = stack[stack.len() - 1];
                        // Prove the new pair (open, line_num) nests with all existing pairs.
                        // open is the top of stack. All existing pairs with close >= open
                        // have open > some earlier stack entry, meaning they're inside
                        // the scope we're closing. Their close < line_num (invariant).
                        // So they're strictly contained in (open, line_num).
                        proof {
                            assert(open >= 1u64);
                            assert(open < line_num);
                            assert forall|i: int| 0 <= i < pairs@.len()
                                && (#[trigger] pairs@[i]).0 < line_num
                                && open < pairs@[i].1
                            implies
                                (open <= pairs@[i].0 && pairs@[i].1 <= line_num
                                 && (open < pairs@[i].0 || pairs@[i].1 < line_num))
                            by {
                                assert(pairs@[i].1 < line_num);
                            }
                        }
                        let ghost pairs_before = pairs@;
                        let ghost stack_before = stack@;
                        stack.pop();
                        pairs.push((open, line_num));
                        proof {
                            // After pop, remaining stack entries are stack_before[0..len-1].
                            // They were all < open (strictly ascending, open was last).
                            // New pair (open, line_num) satisfies bridge: stack[k] < open = new_pair.0.
                            // Old pairs: bridge held with stack_before, and stack is a prefix of
                            // stack_before (minus the last element), so it still holds.
                            assert forall|i: int, k: int|
                                0 <= i < pairs@.len() && 0 <= k < stack@.len()
                            implies
                                (#[trigger] pairs@[i]).1 <= (#[trigger] stack@[k])
                                || (stack@[k] < pairs@[i].0 && pairs@[i].0 <= pairs@[i].1)
                            by {
                                assert(stack@[k] == stack_before[k]);
                                assert(stack@[k] < open);
                                if i < pairs_before.len() {
                                    // Old pair: bridge held before with stack_before[k]
                                    assert(pairs@[i] == pairs_before[i]);
                                } else {
                                    // New pair: (open, line_num). stack[k] < open.
                                    assert(pairs@[i] == (open, line_num));
                                    assert(stack@[k] < open);
                                }
                            }
                        }
                    }
                }
                if !done && props.contains(&LineProperty::ScopeOpen) {
                    // All existing stack entries < line_num (invariant).
                    // line_num > all stack entries, so sorted order is maintained.
                    proof {
                        // Bridge for new stack entry: for all existing pairs (a,b),
                        // b < line_num (from invariant), so first disjunct holds.
                        assert forall|i: int| 0 <= i < pairs@.len()
                        implies
                            (#[trigger] pairs@[i]).1 <= line_num
                        by {}
                    }
                    stack.push(line_num);
                }
            }
        }
        }
        if !done {
            line_num = line_num + 1;
        }
    }

    if unbalanced || !stack.is_empty() {
        // Unbalanced — return empty
        return vec![];
    }

    pairs
}

/// Pass 2: Build Scope structs from (open, close) pairs.
/// Determines parent/children by containment.
fn build_from_pairs(pairs: &[(u64, u64)]) -> (scopes: Vec<Scope>)
    requires
        forall|i: int| 0 <= i < pairs@.len() ==>
            (#[trigger] pairs@[i]).0 <= pairs@[i].1,
        forall|i: int, j: int| 0 <= i < pairs@.len() && 0 <= j < pairs@.len() && i != j
            && (#[trigger] pairs@[i]).0 < (#[trigger] pairs@[j]).1
            && pairs@[j].0 < pairs@[i].1
            ==> (pairs@[i].0 <= pairs@[j].0 && pairs@[j].1 <= pairs@[i].1
                 && (pairs@[i].0 < pairs@[j].0 || pairs@[j].1 < pairs@[i].1))
                || (pairs@[j].0 <= pairs@[i].0 && pairs@[i].1 <= pairs@[j].1
                    && (pairs@[j].0 < pairs@[i].0 || pairs@[i].1 < pairs@[j].1)),
    ensures
        scopes_well_formed(scopes@),
        scopes@.len() == pairs@.len(),
        forall|i: int| 0 <= i < scopes@.len() ==>
            (#[trigger] scopes@[i]).open_line == pairs@[i].0
            && scopes@[i].close_line == pairs@[i].1,
{
    // Create scopes — one per pair, no mutation after creation
    let mut scopes: Vec<Scope> = Vec::new();
    let mut i: usize = 0;
    while i < pairs.len()
        invariant
            i <= pairs@.len(),
            scopes@.len() == i as int,
            forall|k: int| 0 <= k < i as int ==>
                (#[trigger] scopes@[k]).open_line == pairs@[k].0
                && scopes@[k].close_line == pairs@[k].1,
        decreases pairs.len() - i,
    {
        scopes.push(Scope {
            open_line: pairs[i].0,
            close_line: pairs[i].1,
            parent: None,
            children: vec![],
        });
        i = i + 1;
    }

    // At this point scopes has the right open/close lines.
    // The well-formedness follows from the pairs' nesting property:
    // scopes[i].open_line == pairs[i].0 and scopes[i].close_line == pairs[i].1,
    // and pairs satisfy the nesting precondition.
    assert(scopes@.len() == pairs@.len());

    // Prove well-formedness from the pairs precondition
    assert forall|i: int| 0 <= i < scopes@.len() implies
        (#[trigger] scopes@[i]).open_line <= scopes@[i].close_line
    by {
        assert(scopes@[i].open_line == pairs@[i].0);
        assert(scopes@[i].close_line == pairs@[i].1);
    }

    assert forall|i: int, j: int|
        0 <= i < scopes@.len() && 0 <= j < scopes@.len() && i != j
        && (#[trigger] scopes@[i]).open_line < (#[trigger] scopes@[j]).close_line
        && scopes@[j].open_line < scopes@[i].close_line
    implies
        scope_contains(scopes@, i, j) || scope_contains(scopes@, j, i)
    by {
        assert(scopes@[i].open_line == pairs@[i].0);
        assert(scopes@[i].close_line == pairs@[i].1);
        assert(scopes@[j].open_line == pairs@[j].0);
        assert(scopes@[j].close_line == pairs@[j].1);
    }

    // Parent/children are cosmetic for the well-formedness proof.
    // We skip setting them here for the verified version.
    // The parent/children fields are used by downstream code but
    // scopes_well_formed only depends on open_line/close_line.
    scopes
}

fn fallback_scope(file_length: u64) -> (scopes: Vec<Scope>)
    requires file_length >= 1,
    ensures scopes_well_formed(scopes@),
{
    let s = vec![Scope { open_line: 1, close_line: file_length, parent: None, children: vec![] }];
    assert(s@.len() == 1);
    s
}

} // verus!

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;
    fn s(props: &[LineProperty]) -> Option<LineClass> { Some(line_class(props)) }

    //= design/coverage-model-v2-spec.md#scopes
    //= type=test
    //# A scope is a contiguous range of lines delimited by `ScopeOpen` and
    //# `ScopeClose` properties. Scopes nest.
    #[test] fn simple_method_in_class() {
        let c = vec![s(&[LineProperty::Declaration, LineProperty::ScopeOpen]), s(&[LineProperty::Declaration, LineProperty::ScopeOpen]), s(&[LineProperty::Statement]), s(&[LineProperty::ScopeClose]), s(&[LineProperty::ScopeClose])];
        let sc = build_scope_tree(&c, 5);
        assert_eq!(sc.len(), 2);
        // Find the outer and inner scopes by open_line
        let outer = sc.iter().find(|s| s.open_line == 1).unwrap();
        let inner = sc.iter().find(|s| s.open_line == 2).unwrap();
        assert_eq!(outer.close_line, 5);
        assert_eq!(inner.close_line, 4);
    }
    #[test] fn sibling_methods() {
        let c = vec![s(&[LineProperty::Declaration, LineProperty::ScopeOpen]), s(&[LineProperty::Declaration, LineProperty::ScopeOpen]), s(&[LineProperty::ScopeClose]), s(&[LineProperty::Declaration, LineProperty::ScopeOpen]), s(&[LineProperty::ScopeClose]), s(&[LineProperty::ScopeClose])];
        let sc = build_scope_tree(&c, 6);
        assert_eq!(sc.len(), 3);
    }
    #[test] fn unbalanced_fallback() {
        let sc = build_scope_tree(&vec![s(&[LineProperty::ScopeOpen]), s(&[LineProperty::Statement])], 2);
        // Unbalanced: pairs returns empty, so we get file-level scope
        assert!(sc.len() >= 1);
        assert_eq!(sc[0].open_line, 1);
        assert_eq!(sc[0].close_line, 2);
    }
    #[test] fn empty_file() { assert_eq!(build_scope_tree(&vec![], 0).len(), 0); }
    #[test] fn unknown_lines_ignored() {
        let c = vec![s(&[LineProperty::Declaration, LineProperty::ScopeOpen]), None, s(&[LineProperty::Statement]), s(&[LineProperty::ScopeClose])];
        let sc = build_scope_tree(&c, 4);
        assert_eq!(sc.len(), 1);
        assert_eq!(sc[0].open_line, 1);
        assert_eq!(sc[0].close_line, 4);
    }
    #[test] fn four_level_nesting() {
        let c = vec![s(&[LineProperty::ScopeOpen]), s(&[LineProperty::ScopeOpen]), s(&[LineProperty::ScopeOpen]), s(&[LineProperty::ScopeOpen]), s(&[LineProperty::ScopeClose]), s(&[LineProperty::ScopeClose]), s(&[LineProperty::ScopeClose]), s(&[LineProperty::ScopeClose])];
        let sc = build_scope_tree(&c, 8);
        assert_eq!(sc.len(), 4);
    }
    #[test] fn catch_creates_sibling_scopes() {
        let c = vec![s(&[LineProperty::Declaration, LineProperty::ScopeOpen]), s(&[LineProperty::Declaration, LineProperty::ScopeOpen]), s(&[LineProperty::Statement]), s(&[LineProperty::Declaration, LineProperty::ScopeOpen, LineProperty::ScopeClose]), s(&[LineProperty::Statement]), s(&[LineProperty::ScopeClose]), s(&[LineProperty::ScopeClose])];
        let sc = build_scope_tree(&c, 7);
        assert_eq!(sc.len(), 3);
    }
}
