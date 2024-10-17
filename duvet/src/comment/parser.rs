use super::{tokenizer::Token, Comment, Meta};
use duvet_core::file::{Slice, SourceFile};

pub fn parse<T: IntoIterator<Item = Token>>(tokens: T) -> Parser<T::IntoIter> {
    Parser {
        prev_line: 0,
        comment: Comment::default(),
        tokens: tokens.into_iter(),
    }
}

pub struct Parser<T> {
    prev_line: usize,
    comment: Comment,
    tokens: T,
}

impl<T: Iterator<Item = Token>> Parser<T> {
    fn on_token(&mut self, token: Token) -> Option<Comment> {
        let line_no = token.line_no();
        // if the line number isn't the next expected one then flush
        let prev = self.flush_if(line_no > self.prev_line + 1);
        self.prev_line = line_no;

        match token {
            Token::Meta {
                key,
                value,
                line: _,
            } => self.push_meta(Meta {
                key: Some(key.clone()),
                value: value.clone(),
            }),
            Token::UnnamedMeta { value, line: _ } => self.push_meta(Meta {
                key: None,
                value: value.clone(),
            }),
            Token::Content { value, line: _ } => {
                self.push_contents(value.clone());
                None
            }
        }
        .or(prev)
    }

    fn push_meta(&mut self, meta: Meta) -> Option<Comment> {
        let prev = self.flush_if(!self.comment.contents.is_empty());
        self.comment.meta.push(meta);
        prev
    }

    fn push_contents(&mut self, contents: Slice<SourceFile>) {
        ensure!(!self.comment.meta.is_empty());
        self.comment.contents.push(contents);
    }

    fn flush_if(&mut self, cond: bool) -> Option<Comment> {
        if cond {
            self.flush()
        } else {
            None
        }
    }

    fn flush(&mut self) -> Option<Comment> {
        let annotation = core::mem::take(&mut self.comment);
        ensure!(!annotation.meta.is_empty(), None);
        ensure!(!annotation.contents.is_empty(), None);
        Some(annotation)
    }
}

impl<T: Iterator<Item = Token>> Iterator for Parser<T> {
    type Item = Comment;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let Some(token) = self.tokens.next() else {
                return self.flush();
            };
            if let Some(annotation) = self.on_token(token) {
                return Some(annotation);
            }
        }
    }
}
