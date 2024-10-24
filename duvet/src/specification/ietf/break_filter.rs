// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use super::tokenizer::{Break, Token};

/// Filters out duplicate breaks, headers, and headers misclassified as contents
pub fn break_filter<T: Iterator<Item = Token>>(tokens: T) -> impl Iterator<Item = Token> {
    let mut break_ty = None;
    tokens.filter(move |token| {
        let prev_break = core::mem::take(&mut break_ty);

        match token {
            Token::Section { .. } | Token::Appendix { .. } | Token::NamedSection { .. } => {}
            Token::Break { ty, .. } => {
                break_ty = Some(*ty);

                // dedupe breaks
                if prev_break.is_some() {
                    return false;
                }
            }
            Token::Content { .. } => {
                // if we previously had a page break then ignore the next line - it's a header that
                // didn't tokenize correctly
                if matches!(prev_break, Some(Break::Page)) {
                    break_ty = Some(Break::Line);
                    return false;
                }
            }
            Token::Header { value: _, line: _ } => {
                // set up a break since we skipped a line
                break_ty = Some(Break::Line);
                return false;
            }
        }

        true
    })
}
