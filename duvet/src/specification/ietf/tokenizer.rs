// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use core::fmt;
use duvet_core::{
    ensure,
    file::{Slice, SourceFile},
};
use once_cell::sync::Lazy;
use regex::Regex;

macro_rules! regex {
    ($str:literal) => {{
        static R: Lazy<Regex> = Lazy::new(|| Regex::new($str).unwrap());
        &*R
    }};
}

#[derive(Clone, Copy, Debug)]
pub enum Break {
    Line,
    Page,
}

#[derive(Clone)]
pub enum Token {
    Section {
        id: Slice<SourceFile>,
        title: Slice<SourceFile>,
        line: usize,
    },
    Appendix {
        id: Slice<SourceFile>,
        title: Slice<SourceFile>,
        line: usize,
    },
    NamedSection {
        title: Slice<SourceFile>,
        line: usize,
    },
    Break {
        value: Slice<SourceFile>,
        ty: Break,
        line: usize,
    },
    Content {
        value: Slice<SourceFile>,
        line: usize,
    },
    Header {
        value: Slice<SourceFile>,
        line: usize,
    },
}

impl Token {
    #[allow(dead_code)]
    pub fn line(&self) -> usize {
        match self {
            Token::Section { line, .. } => *line,
            Token::Appendix { line, .. } => *line,
            Token::NamedSection { line, .. } => *line,
            Token::Break { line, .. } => *line,
            Token::Content { line, .. } => *line,
            Token::Header { line, .. } => *line,
        }
    }

    fn section(
        id: Slice<SourceFile>,
        title: Slice<SourceFile>,
        line: usize,
        force_appendix: bool,
    ) -> Self {
        if !force_appendix && id.starts_with(char::is_numeric) {
            Token::Section { id, title, line }
        } else {
            Token::Appendix { id, title, line }
        }
    }
}

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Section { id, title, line } => {
                write!(f, " SECTION#{}(id={}, title={})", line, id, title)
            }
            Self::Appendix { id, title, line } => {
                write!(f, "APPENDIX#{}(id={}, title={})", line, id, title)
            }
            Self::NamedSection { title, line } => {
                write!(f, " SECTION#{}(title={})", line, title)
            }
            Self::Break {
                line,
                ty: Break::Page,
                value: _,
            } => write!(f, "   BREAK#{}", line),
            Self::Break {
                line,
                ty: Break::Line,
                value: _,
            } => write!(f, " NEWLINE#{}", line),
            Self::Content { value, line } => write!(f, " CONTENT#{}({})", line, value),
            Self::Header { value, line } => write!(f, "  HEADER#{}({})", line, value),
        }
    }
}

pub fn tokens(contents: &SourceFile) -> impl Iterator<Item = Token> + '_ {
    let tokens = lines(contents);
    let tokens = page_breaks(tokens);
    let tokens = line_breaks(tokens);
    let tokens = sections(tokens);
    let tokens = headers(tokens);
    named_sections(tokens)
}

macro_rules! expect_contents {
    ($token:expr) => {
        match $token {
            Token::Content { value, line } => (value, line),
            token => return token,
        }
    };
}

/// Transforms file contents into lines
fn lines(contents: &SourceFile) -> impl Iterator<Item = Token> + '_ {
    contents.lines().enumerate().map(move |(line, mut value)| {
        // look for Byte Order Mark and filter it out
        if line == 0 {
            value = value.trim_start_matches("\u{feff}");
        }

        // line numbers start at 1
        let line = line + 1;
        let value = contents.substr(value).unwrap();
        Token::Content { value, line }
    })
}

/// Looks for page breaks in `Content` tokens
fn page_breaks<I: Iterator<Item = Token>>(i: I) -> impl Iterator<Item = Token> {
    i.map(|token| {
        let (value, line) = expect_contents!(token);

        if &*value == "\u{C}" {
            return Token::Break {
                value,
                line,
                ty: Break::Page,
            };
        }

        Token::Content { value, line }
    })
}

/// Looks for line breaks in `Content` tokens
fn line_breaks<I: Iterator<Item = Token>>(i: I) -> impl Iterator<Item = Token> {
    i.map(|token| {
        let (value, line) = expect_contents!(token);

        if value.is_empty() || value.trim().is_empty() {
            return Token::Break {
                value,
                line,
                ty: Break::Line,
            };
        }

        Token::Content { value, line }
    })
}

/// Looks for headers/footers
fn headers<I: Iterator<Item = Token>>(i: I) -> impl Iterator<Item = Token> {
    i.map(|token| {
        let (value, line) = expect_contents!(token);

        let beginning_patterns = [
            regex!(r"^RFC [1-9][0-9]*  "),
            regex!(r"^\[Page [1-9][0-9]*\]"),
        ];

        let trim_start = value.trim_start();
        if trim_start.len() == value.len() {
            for pattern in beginning_patterns {
                if pattern.is_match(&value) {
                    return Token::Header { value, line };
                }
            }
        }

        let ending_patterns = [regex!(r" \[Page [1-9][0-9]*\]$")];

        let trim_end = value.trim_end();
        if trim_end.len() == value.len() {
            for pattern in ending_patterns {
                if pattern.is_match(&value) {
                    return Token::Header { value, line };
                }
            }
        }

        Token::Content { value, line }
    })
}

fn named_sections<I: Iterator<Item = Token>>(i: I) -> impl Iterator<Item = Token> {
    struct NamedSections<I: Iterator<Item = Token>> {
        state: State,
        queue: Queue,
        iter: I,
    }

    impl<I: Iterator<Item = Token>> Iterator for NamedSections<I> {
        type Item = Token;

        fn next(&mut self) -> Option<Token> {
            loop {
                if let Some(token) = self.queue.next() {
                    return Some(token);
                }

                if let Some(token) = self.iter.next() {
                    self.state.on_token(token, &mut self.queue);
                } else {
                    self.state.flush(&mut self.queue);
                    return self.queue.next();
                }
            }
        }
    }

    enum Queue {
        Zero,
        One(Token),
        Two(Token, Token),
        Three(Token, Token, Token),
    }

    impl Queue {
        fn push(&mut self, token: Token) {
            match core::mem::replace(self, Self::Zero) {
                Self::Three(_, _, _) => {
                    panic!("at capacity");
                }
                Self::Two(a, b) => {
                    *self = Self::Three(a, b, token);
                }
                Self::One(a) => {
                    *self = Self::Two(a, token);
                }
                Self::Zero => {
                    *self = Self::One(token);
                }
            }
        }

        fn next(&mut self) -> Option<Token> {
            match core::mem::replace(self, Self::Zero) {
                Self::Three(a, b, c) => {
                    *self = Self::Two(b, c);
                    Some(a)
                }
                Self::Two(a, b) => {
                    *self = Self::One(b);
                    Some(a)
                }
                Self::One(a) => Some(a),
                Self::Zero => None,
            }
        }
    }

    enum State {
        Init,
        // we have a line break
        First {
            break_token: Token,
        },
        // we have a line break and a named section - just waiting on another line break
        Second {
            break_token: Token,
            title: Slice,
            line: usize,
        },
    }

    impl State {
        fn on_token(&mut self, token: Token, queue: &mut Queue) {
            debug_assert!(matches!(queue, Queue::Zero));

            match (core::mem::replace(self, Self::Init), token) {
                (
                    Self::Init,
                    token @ Token::Break {
                        ty: Break::Line, ..
                    },
                ) => {
                    *self = Self::First { break_token: token };
                }
                (Self::Init, token) => {
                    queue.push(token);
                }
                (Self::First { break_token }, Token::Content { value, line }) => {
                    let patterns = [
                        "Acknowledgments",
                        "Acknowledgement",
                        "Acknowledgements",
                        "Index",
                        "Author's Address",
                        "Authors' Addresses",
                        "Normative References",
                        "Informative References",
                        "References",
                        "REFERENCES",
                        "AUTHORS' ADDRESSES",
                        "Full Copyright Statement",
                        "Security Considerations",
                        "Intellectual Property",
                        "Intellectual Property Statement",
                        "Working Group Information",
                        "Contributors",
                        "Editors' Addresses",
                        "IANA Considerations",
                        "Abstract",
                        "Status of this Memo",
                        "Status of This Memo",
                        "Copyright Notice",
                        "Table of Contents",
                        "Appendix",
                    ];

                    if patterns.contains(&&*value) {
                        *self = Self::Second {
                            break_token,
                            title: value,
                            line,
                        };
                    } else {
                        queue.push(break_token);
                        queue.push(Token::Content { value, line });
                    }
                }
                (
                    Self::First { break_token },
                    token @ Token::Break {
                        ty: Break::Line, ..
                    },
                ) => {
                    queue.push(break_token);
                    *self = Self::First { break_token: token };
                }
                (Self::First { break_token }, token) => {
                    queue.push(break_token);
                    queue.push(token);
                }
                (
                    Self::Second {
                        break_token,
                        title,
                        line,
                    },
                    token @ Token::Break {
                        ty: Break::Line, ..
                    },
                ) => {
                    let title = Token::NamedSection { title, line };
                    queue.push(break_token);
                    queue.push(title);
                    queue.push(token);
                }
                (
                    Self::Second {
                        break_token,
                        title,
                        line,
                    },
                    token,
                ) => {
                    let title = Token::Content { value: title, line };
                    queue.push(break_token);
                    queue.push(title);
                    queue.push(token);
                }
            }
        }

        fn flush(&mut self, queue: &mut Queue) {
            match core::mem::replace(self, Self::Init) {
                Self::Init => {}
                Self::First { break_token } => {
                    queue.push(break_token);
                }
                Self::Second {
                    break_token,
                    title,
                    line,
                } => {
                    queue.push(break_token);
                    queue.push(Token::Content { value: title, line });
                }
            }
        }
    }

    NamedSections {
        state: State::Init,
        queue: Queue::Zero,
        iter: i,
    }
}

fn sections<I: Iterator<Item = Token>>(i: I) -> impl Iterator<Item = Token> {
    Sections::new(i)
}

#[derive(Debug)]
struct Sections<T: Iterator<Item = Token>> {
    tokens: T,
    was_break: bool,
    prev_section: Option<Slice<SourceFile>>,
}

impl<T: Iterator<Item = Token>> Sections<T> {
    pub fn new(tokens: T) -> Self {
        Self {
            tokens,
            was_break: false,
            prev_section: None,
        }
    }

    fn on_token(&mut self, token: Token) -> Token {
        let token = self.on_token_impl(token);

        self.was_break = matches!(token, Token::Break { .. });

        if let Token::Section { id, .. } = &token {
            self.prev_section = Some(id.clone());
        }

        if let Token::Appendix { id, .. } = &token {
            self.prev_section = Some(id.clone());
        }

        token
    }

    fn on_token_impl(&mut self, token: Token) -> Token {
        let (value, line) = expect_contents!(token);

        let mut force_appendix = false;
        let mut section_candidate = &*value;

        for prefix in ["Appendix ", "APPENDIX ", "Annex "] {
            if let Some(value) = value.strip_prefix(prefix) {
                section_candidate = value;
                force_appendix = true;
                break;
            }
        }

        if force_appendix {
            let candidates = [
                regex!(r"^([A-Z])$"),
                regex!(r"^([A-Z])\.$"),
                regex!(r"^([A-Z])\.\s+(.*)"),
                regex!(r"^([A-Z]):\s+(.*)"),
                regex!(r"^([A-Z]) :\s+(.*)"),
                regex!(r"^([A-Z]) -\s+(.*)"),
                regex!(r"^([A-Z]) --\s+(.*)"),
                regex!(r"^([A-Z])\s+(.*)"),
            ];

            for candidate in candidates {
                if let Some(section) = candidate.captures(section_candidate) {
                    let id = section.get(1).unwrap();
                    let id = &section_candidate[id.range()];

                    let title = if let Some(title) = section.get(2) {
                        section_candidate[title.range()].trim()
                    } else {
                        &id[id.len()..]
                    };

                    if !self.section_check_candidate(id, title) {
                        continue;
                    }

                    let id = value.file().substr(id).unwrap();
                    let title = value.file().substr(title).unwrap();

                    return Token::section(id, title, line, true);
                }
            }
        }

        let candidates = [regex!(r"^(([A-Z]\.?)?[0-9\.]+):?\s+(.*)")];

        for candidate in candidates {
            if let Some(section) = candidate.captures(section_candidate) {
                let id = section.get(1).unwrap();
                let id = &section_candidate[id.range()].trim_end_matches('.');

                let title = section.get(3).unwrap();
                let title = &section_candidate[title.range()].trim();

                if self.section_check_candidate(id, title) {
                    let id = value.file().substr(id).unwrap();
                    let title = value.file().substr(title).unwrap();

                    return Token::section(id, title, line, force_appendix);
                }
            }
        }

        if regex!(r"^(([A-Z]\.)?[0-9\.]+)$").is_match(section_candidate) {
            let id = section_candidate.trim_end_matches('.');

            if self.section_check_candidate(id, "") {
                let id = value.file().substr(id).unwrap();

                let title = value.file().substr(&id[id.len()..]).unwrap();

                return Token::section(id, title, line, force_appendix);
            }
        }

        Token::Content { value, line }
    }

    fn section_check_candidate(&self, id: &str, title: &str) -> bool {
        ensure!(Self::section_check_toc(title), false);
        ensure!(Self::section_check_weird_title(title), false);

        // if we have a possibly weird title, then use a monotonicity check
        let check_monotonic = !Self::section_check_possible_weird_title(title);

        self.section_check_id(id, check_monotonic)
    }

    fn section_check_id(&self, id: &str, check_monotonic: bool) -> bool {
        for res in parse_id(id) {
            ensure!(res.is_ok(), false);
        }

        // if we previously had a break then it's likely a valid section
        if self.was_break && !check_monotonic {
            return true;
        }

        let Some(prev) = self.prev_section.as_ref() else {
            // if we don't have a section then make sure the first one is `1`
            return ["1", "1.0"].contains(&id);
        };

        section_id_monotonic(prev, id)
    }

    fn section_check_toc(title: &str) -> bool {
        // try to detect if this is a Table of Contents entry - they usually have period
        // separators
        ensure!(!title.contains("....."), false);
        ensure!(!title.contains(". . ."), false);
        ensure!(!title.contains(" . . "), false);

        true
    }

    fn section_check_weird_title(title: &str) -> bool {
        // try to filter out weird titles
        ensure!(!title.starts_with(';'), false);
        ensure!(!title.ends_with(['{', '[', '(', ';']), false);

        // check if the title contains too much spacing
        ensure!(!title.trim().contains("     "), false);

        true
    }

    fn section_check_possible_weird_title(title: &str) -> bool {
        // try to filter out weird titles
        ensure!(!title.trim_end_matches('|').contains("|"), false);

        true
    }
}

pub(super) fn section_id_monotonic(prev: &str, current: &str) -> bool {
    ensure!(prev != current, false);

    let prev_parts = parse_id(prev);
    let current_parts = parse_id(current);

    for (idx, (prev_part, current_part)) in prev_parts.zip(current_parts).enumerate() {
        let Some(prev_part): Option<Part> = prev_part.ok() else {
            return false;
        };
        let Some(current_part): Option<Part> = current_part.ok() else {
            return false;
        };

        // only the first part is allowed to be a number
        if idx > 0 {
            ensure!(matches!(current_part, Part::Num(_)), false);
        }

        // no need to keep comparing the parts
        if prev_part.is_next(&current_part) {
            break;
        }

        // the current part can't be less than the previous
        ensure!(prev_part == current_part, false);
    }

    true
}

impl<T: Iterator<Item = Token>> Iterator for Sections<T> {
    type Item = Token;

    fn next(&mut self) -> Option<Self::Item> {
        let token = self.tokens.next()?;
        let token = self.on_token(token);
        Some(token)
    }
}

fn parse_id(id: &str) -> impl Iterator<Item = Result<Part, ()>> + '_ {
    let mut digit_offset = 0;

    for (idx, c) in id.char_indices() {
        if c.is_ascii_digit() {
            digit_offset = idx;
            break;
        }
    }

    let (prefix, digits) = id.split_at(digit_offset);

    let prefix = if prefix.is_empty() {
        None
    } else {
        Some(prefix.trim_end_matches('.').parse())
    };

    prefix
        .into_iter()
        .chain(digits.split('.').map(|v| v.parse()))
        .enumerate()
        .map(|(idx, part)| {
            let part = part?;

            if idx == 0 {
                if let Part::Num(num) = part {
                    ensure!(num > 0, Err(()));
                }
            }

            // only the first part is allowed to be a letter
            if idx > 0 {
                ensure!(matches!(part, Part::Num(_)), Err(()));
            }

            Ok(part)
        })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Part {
    Num(u8),
    Appendix(char),
}

impl Part {
    fn is_next(&self, other: &Self) -> bool {
        match (self, other) {
            (Part::Num(a), Part::Num(b)) => (*a as usize) + 1 == *b as usize,
            (Part::Num(_), Part::Appendix(a)) => *a == 'A',
            (Part::Appendix(_), Part::Num(_)) => false,
            (Part::Appendix(a), Part::Appendix(b)) => (*a as u32 + 1) == *b as u32,
        }
    }
}

impl core::str::FromStr for Part {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Ok(v) = s.parse() {
            // RFCs don't exceed this value
            ensure!(v <= 199, Err(()));

            return Ok(Self::Num(v));
        }

        ensure!(s.len() == 1, Err(()));

        let c = s.chars().next().unwrap();
        ensure!(c.is_ascii_uppercase(), Err(()));

        Ok(Self::Appendix(c))
    }
}
