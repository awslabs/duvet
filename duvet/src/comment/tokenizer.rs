// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::comment::Pattern;
use duvet_core::file::{Slice, SourceFile};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Token {
    Meta {
        key: Slice<SourceFile>,
        value: Slice<SourceFile>,
        line: usize,
    },
    UnnamedMeta {
        value: Slice<SourceFile>,
        line: usize,
    },
    Content {
        value: Slice<SourceFile>,
        line: usize,
    },
}

impl Token {
    pub fn line_no(&self) -> usize {
        match self {
            Token::Meta { line, .. } => *line,
            Token::UnnamedMeta { line, .. } => *line,
            Token::Content { line, .. } => *line,
        }
    }
}

pub fn tokens<'a>(file: &'a SourceFile, style: &'a Pattern) -> Tokenizer<'a> {
    Tokenizer::new(&style.meta, &style.content, file)
}

pub struct Tokenizer<'a> {
    meta_prefix: &'a str,
    content_prefix: &'a str,
    file: &'a SourceFile,
    lines: core::iter::Enumerate<core::str::Lines<'a>>,
}

impl<'a> Tokenizer<'a> {
    fn new(meta_prefix: &'a str, content_prefix: &'a str, file: &'a SourceFile) -> Self {
        Self {
            meta_prefix,
            content_prefix,
            file,
            lines: file.lines().enumerate(),
        }
    }

    fn on_line(&mut self, line: &str, line_no: usize) -> Option<Token> {
        let content = line.trim_start();
        if content.is_empty() {
            return None;
        }

        if let Some(content) = content.strip_prefix(self.meta_prefix) {
            let content = content.trim_start();
            return self.on_meta(content, line_no);
        }

        if let Some(content) = content.strip_prefix(self.content_prefix) {
            let content = content.trim_start();
            return self.on_content(content, line_no);
        }

        None
    }

    fn on_content(&mut self, content: &str, line_no: usize) -> Option<Token> {
        let value = self.file.substr(content).unwrap();
        Some(Token::Content {
            value,
            line: line_no,
        })
    }

    fn on_meta(&mut self, meta: &str, line_no: usize) -> Option<Token> {
        let mut parts = meta.trim_start().splitn(2, '=');

        let key = parts.next().unwrap();
        let key = key.trim_end();
        let key = self.file.substr(key).unwrap();

        if let Some(value) = parts.next() {
            let value = value.trim();
            let value = self.file.substr(value).unwrap();
            Some(Token::Meta {
                key,
                value,
                line: line_no,
            })
        } else {
            Some(Token::UnnamedMeta {
                value: key,
                line: line_no,
            })
        }
    }
}

impl Iterator for Tokenizer<'_> {
    type Item = Token;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let (line_no, line) = self.lines.next()?;
            if let Some(token) = self.on_line(line, line_no) {
                return Some(token);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! snapshot_test {
        ($name:ident, $input:expr) => {
            snapshot_test!(
                $name,
                $input,
                Pattern {
                    meta: "//@=".into(),
                    content: "//@#".into(),
                }
            );
        };
        ($name:ident, $input:expr, $config:expr) => {
            #[tokio::test]
            async fn $name() {
                let source = SourceFile::new(file!(), $input).unwrap();
                let config = $config;
                let tokens: Vec<_> = tokens(&source, &config).collect();
                insta::assert_debug_snapshot!(stringify!($name), tokens);
            }
        };
    }

    snapshot_test!(empty, "");
    snapshot_test!(
        basic,
        r#"
        //@= thing goes here
        //@= meta=foo
        //@= meta2 = bar
        //@# content goes
        //@# here
        "#
    );
    snapshot_test!(
        only_unnamed,
        r#"
        //@= this is meta
        //@= this is other meta
        "#
    );
    snapshot_test!(
        duplicate_meta,
        r#"
        //@= meta=1
        //@= meta=2
        "#
    );
    snapshot_test!(
        configured,
        r#"
        /*
         *= meta=goes here
         *# content goes here
         */
        "#,
        Pattern {
            meta: "*=".into(),
            content: "*#".into(),
        }
    );
}
