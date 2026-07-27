// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! S-expression parser for Verus `*-sst.vir` logs.
//!
//! The grammar, established empirically against the SST POC corpus
//! (2026-07-26, Verus 0.2026.05.24.ecee80a):
//!
//! - **Lists**: `( expr* )`
//! - **Strings**: `"..."`, `\`-escapes tolerated for termination
//!   (the corpus contains none; escape sequences are preserved raw)
//! - **Comments**: `;` to end of line (the writer emits `;; section`
//!   separators between top-level groups)
//! - **Atoms**: any other run of characters up to whitespace, `(`,
//!   `)`, `"`, or `;`. Atoms include paths with `&%!$>-` characters
//!   (`vstd::seq::impl&%0::spec_index`, `classifications!$0`, `->`),
//!   keywords (`:name`), and `@`.
//!
//! Parsing is iterative (explicit stack, no call recursion): the real
//! logs are multi-megabyte with deep expression nesting, and this
//! parser must not depend on thread stack size to succeed.

use std::fmt;

/// One S-expression. Atoms and strings borrow from the input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Sexpr<'a> {
    /// Unquoted token, e.g. `FunctionSst`, `:name`, `@`, `impl&%0::f`.
    Atom(&'a str),
    /// Quoted string contents (without the surrounding quotes,
    /// escape sequences preserved raw).
    Str(&'a str),
    /// Parenthesized list.
    List(Vec<Sexpr<'a>>),
}

impl<'a> Sexpr<'a> {
    /// The atom's text, if this is an atom.
    pub fn as_atom(&self) -> Option<&'a str> {
        match self {
            Sexpr::Atom(s) => Some(s),
            _ => None,
        }
    }

    /// The string's contents, if this is a string.
    pub fn as_str(&self) -> Option<&'a str> {
        match self {
            Sexpr::Str(s) => Some(s),
            _ => None,
        }
    }

    /// The list's elements, if this is a list.
    pub fn as_list(&self) -> Option<&[Sexpr<'a>]> {
        match self {
            Sexpr::List(items) => Some(items),
            _ => None,
        }
    }
}

impl Drop for Sexpr<'_> {
    /// Iterative drop. The parser is deliberately non-recursive, but
    /// the *derived* drop of a deeply nested `List` recurses one
    /// stack frame per level — so without this, parsing a
    /// pathologically deep input would succeed and then overflow
    /// the stack on destruction (observed: the 100k-deep test blew
    /// the test-thread stack in exactly that way). Depth-safety has
    /// to hold for the whole lifecycle, not just construction.
    fn drop(&mut self) {
        let mut stack: Vec<Sexpr> = Vec::new();
        if let Sexpr::List(items) = self {
            stack.append(items);
        }
        while let Some(mut expr) = stack.pop() {
            if let Sexpr::List(items) = &mut expr {
                stack.append(items);
            }
            // `expr` drops here with an empty list: no recursion.
        }
    }
}

impl fmt::Display for Sexpr<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Sexpr::Atom(s) => write!(f, "{s}"),
            Sexpr::Str(s) => write!(f, "\"{s}\""),
            Sexpr::List(items) => {
                write!(f, "(")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, " ")?;
                    }
                    write!(f, "{item}")?;
                }
                write!(f, ")")
            }
        }
    }
}

/// Parse error with byte offset into the input.
#[derive(Debug, PartialEq, Eq)]
pub struct SexprError {
    pub offset: usize,
    pub kind: SexprErrorKind,
}

#[derive(Debug, PartialEq, Eq)]
pub enum SexprErrorKind {
    UnbalancedClose,
    UnclosedList,
    UnterminatedString,
}

impl fmt::Display for SexprError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self.kind {
            SexprErrorKind::UnbalancedClose => "unbalanced ')'",
            SexprErrorKind::UnclosedList => "unclosed '('",
            SexprErrorKind::UnterminatedString => "unterminated string",
        };
        write!(f, "{msg} at byte {}", self.offset)
    }
}

impl std::error::Error for SexprError {}

/// Parse every top-level expression in `input`.
pub fn parse_all(input: &str) -> Result<Vec<Sexpr<'_>>, SexprError> {
    let bytes = input.as_bytes();
    let mut pos = 0usize;
    // Invariant: `stack` holds the partially-built parent lists;
    // `current` is the list being filled (top-level when stack empty).
    let mut stack: Vec<Vec<Sexpr>> = Vec::new();
    let mut current: Vec<Sexpr> = Vec::new();

    while pos < bytes.len() {
        let b = bytes[pos];
        match b {
            b' ' | b'\t' | b'\r' | b'\n' => pos += 1,
            b';' => {
                while pos < bytes.len() && bytes[pos] != b'\n' {
                    pos += 1;
                }
            }
            b'(' => {
                stack.push(std::mem::take(&mut current));
                pos += 1;
            }
            b')' => {
                let done = std::mem::take(&mut current);
                match stack.pop() {
                    Some(parent) => {
                        current = parent;
                        current.push(Sexpr::List(done));
                    }
                    None => {
                        return Err(SexprError {
                            offset: pos,
                            kind: SexprErrorKind::UnbalancedClose,
                        })
                    }
                }
                pos += 1;
            }
            b'"' => {
                let start = pos + 1;
                let mut end = start;
                loop {
                    match bytes.get(end) {
                        None => {
                            return Err(SexprError {
                                offset: pos,
                                kind: SexprErrorKind::UnterminatedString,
                            })
                        }
                        Some(b'\\') => end += 2,
                        Some(b'"') => break,
                        Some(_) => end += 1,
                    }
                }
                // `end` may have skipped past EOF via an escape at the
                // last byte; re-check.
                if end > bytes.len() {
                    return Err(SexprError {
                        offset: pos,
                        kind: SexprErrorKind::UnterminatedString,
                    });
                }
                current.push(Sexpr::Str(&input[start..end]));
                pos = end + 1;
            }
            _ => {
                let start = pos;
                while pos < bytes.len() {
                    match bytes[pos] {
                        b' ' | b'\t' | b'\r' | b'\n' | b'(' | b')' | b'"' | b';' => break,
                        _ => pos += 1,
                    }
                }
                current.push(Sexpr::Atom(&input[start..pos]));
            }
        }
    }

    if !stack.is_empty() {
        return Err(SexprError {
            offset: input.len(),
            kind: SexprErrorKind::UnclosedList,
        });
    }
    Ok(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn atom(s: &str) -> Sexpr<'_> {
        Sexpr::Atom(s)
    }

    #[test]
    fn atoms_strings_lists() {
        let parsed = parse_all(r#"(@ "vacuity.rs:7:1: 9:2 (#0)" (Fun :path a::b))"#).unwrap();
        assert_eq!(
            parsed,
            vec![Sexpr::List(vec![
                atom("@"),
                Sexpr::Str("vacuity.rs:7:1: 9:2 (#0)"),
                Sexpr::List(vec![atom("Fun"), atom(":path"), atom("a::b")]),
            ])]
        );
    }

    #[test]
    fn exotic_atom_characters() {
        // Every character class observed outside strings in the SST
        // corpus: & % ! $ > - : _ @
        let parsed = parse_all("(vstd::seq::impl&%0::spec_index classifications!$0 -> :name)")
            .unwrap();
        assert_eq!(
            parsed,
            vec![Sexpr::List(vec![
                atom("vstd::seq::impl&%0::spec_index"),
                atom("classifications!$0"),
                atom("->"),
                atom(":name"),
            ])]
        );
    }

    #[test]
    fn comments_are_skipped() {
        let parsed = parse_all(";; trait_impls\n(a b) ; trailing\n(c)").unwrap();
        assert_eq!(
            parsed,
            vec![
                Sexpr::List(vec![atom("a"), atom("b")]),
                Sexpr::List(vec![atom("c")]),
            ]
        );
    }

    #[test]
    fn semicolon_inside_string_is_not_a_comment() {
        let parsed = parse_all(r#"("a;b" c)"#).unwrap();
        assert_eq!(
            parsed,
            vec![Sexpr::List(vec![Sexpr::Str("a;b"), atom("c")])]
        );
    }

    #[test]
    fn escaped_quote_does_not_terminate_string() {
        let parsed = parse_all(r#"("a\"b")"#).unwrap();
        assert_eq!(parsed, vec![Sexpr::List(vec![Sexpr::Str(r#"a\"b"#)])]);
    }

    #[test]
    fn deep_nesting_does_not_recurse() {
        // 100_000 levels: would overflow any call-recursive parser.
        let n = 100_000;
        let mut input = String::new();
        input.push_str(&"(".repeat(n));
        input.push('x');
        input.push_str(&")".repeat(n));
        let parsed = parse_all(&input).unwrap();
        assert_eq!(parsed.len(), 1);
    }

    #[test]
    fn errors_carry_position() {
        assert_eq!(
            parse_all("(a))").unwrap_err(),
            SexprError {
                offset: 3,
                kind: SexprErrorKind::UnbalancedClose
            }
        );
        assert_eq!(
            parse_all("((a)").unwrap_err().kind,
            SexprErrorKind::UnclosedList
        );
        assert_eq!(
            parse_all(r#"("abc"#).unwrap_err().kind,
            SexprErrorKind::UnterminatedString
        );
    }

    #[test]
    fn display_parse_round_trip() {
        // print ∘ parse = identity on parsed forms (the property that
        // makes Display a faithful witness of what was parsed).
        let inputs = [
            r#"(@ "f.rs:1:1: 2:2 (#0)" (FunctionSst :name (Fun :path a::b) :enss ()))"#,
            "(a (b (c d)) \"s\" impl&%0::f)",
            "x",
        ];
        for input in inputs {
            let once = parse_all(input).unwrap();
            let printed = once
                .iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join(" ");
            let twice = parse_all(&printed).unwrap();
            assert_eq!(once, twice, "round-trip diverged for {input:?}");
        }
    }
}
