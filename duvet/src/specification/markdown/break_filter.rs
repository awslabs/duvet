// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use super::tokenizer::Token;

/// Filters out duplicate breaks
pub fn break_filter<T: Iterator<Item = Token>>(tokens: T) -> impl Iterator<Item = Token> {
    let mut state = false;
    tokens.filter(move |token| {
        let prev_break = core::mem::take(&mut state);

        match token {
            Token::Section { .. } | Token::Content { .. } => true,
            Token::Break { .. } => {
                state = true;

                // dedup breaks
                !prev_break
            }
        }
    })
}
