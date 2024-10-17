use core::fmt;
use duvet_core::file::{Slice, SourceFile};
use once_cell::sync::Lazy;
use regex::Regex;

macro_rules! regex {
    ($str:literal) => {{
        static R: Lazy<Regex> = Lazy::new(|| Regex::new($str).unwrap());
        &*R
    }};
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
    Break {
        page: bool,
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
            Self::Break { line, page: true } => write!(f, "   BREAK#{}", line),
            Self::Break { line, page: false } => write!(f, " NEWLINE#{}", line),
            Self::Content { value, line } => write!(f, " CONTENT#{}({})", line, value),
            Self::Header { value, line } => write!(f, "  HEADER#{}({})", line, value),
        }
    }
}

pub fn tokens(contents: &SourceFile) -> Tokenizer<'_> {
    Tokenizer::new(contents)
}

#[derive(Debug)]
pub struct Tokenizer<'a> {
    lines: core::str::Lines<'a>,
    contents: &'a SourceFile,
    lineno: usize,
    was_break: bool,
    prev_section: Option<Slice<SourceFile>>,
}

impl<'a> Tokenizer<'a> {
    pub fn new(contents: &'a SourceFile) -> Self {
        Self {
            lines: contents.lines(),
            contents,
            lineno: 0,
            was_break: false,
            prev_section: None,
        }
    }

    fn on_line(&mut self, line: &'a str) -> Option<Token> {
        let token = self.on_line_impl(line);
        if let Some(token) = &token {
            self.was_break = matches!(token, Token::Break { .. });

            if let Token::Section { id, .. } = &token {
                self.prev_section = Some(id.clone());
            }

            if let Token::Appendix { id, .. } = &token {
                self.prev_section = Some(id.clone());
            }
        }
        token
    }

    fn on_line_impl(&mut self, line: &'a str) -> Option<Token> {
        let lineno = self.lineno;
        self.lineno += 1;

        let page_break = "\u{C}";
        if line == page_break {
            return Some(Token::Break {
                line: lineno,
                page: true,
            });
        }

        if line.is_empty() {
            return Some(Token::Break {
                line: lineno,
                page: false,
            });
        }

        if let Some(section) = self.try_section(line, lineno) {
            return Some(section);
        }

        let trimmed = line.trim_start();

        // if the trimmed line is empty then it's a break
        if trimmed.is_empty() {
            return Some(Token::Break {
                line: lineno,
                page: false,
            });
        }

        // filter out headers and footers
        if trimmed.len() == line.len() {
            let mut is_header = false;

            is_header |= regex!(r"^RFC [1-9][0-9]*  ").is_match(trimmed);

            is_header |= regex!(r" \[Page [1-9][0-9]*\]$").is_match(trimmed);

            if is_header {
                let value = self.contents.substr(line).unwrap();
                return Some(Token::Header {
                    value,
                    line: lineno,
                });
            }
        }

        let value = self.contents.substr(line).unwrap();
        Some(Token::Content {
            value,
            line: lineno,
        })
    }

    fn try_section(&mut self, line: &'a str, lineno: usize) -> Option<Token> {
        let mut force_appendix = false;
        let mut section_candidate = line;

        for prefix in ["Appendix ", "APPENDIX ", "Annex "] {
            if let Some(value) = line.strip_prefix(prefix) {
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
                    let id = section.get(1)?;
                    let id = &section_candidate[id.range()];

                    let title = if let Some(title) = section.get(2) {
                        section_candidate[title.range()].trim()
                    } else {
                        &id[id.len()..]
                    };

                    if !self.section_check_candidate(id, title) {
                        continue;
                    }

                    let id = self.contents.substr(id).unwrap();
                    let title = self.contents.substr(title).unwrap();

                    return Some(Token::section(id, title, lineno, true));
                }
            }
        }

        if let Some(section) = regex!(r"^(([A-Z]\.)?[0-9\.]+):?\s+(.*)").captures(section_candidate)
        {
            let id = section.get(1)?;
            let id = &section_candidate[id.range()].trim_end_matches('.');

            let title = section.get(3)?;
            let title = &section_candidate[title.range()].trim();

            if self.section_check_candidate(id, title) {
                let id = self.contents.substr(id).unwrap();
                let title = self.contents.substr(title).unwrap();

                return Some(Token::section(id, title, lineno, force_appendix));
            }
        }

        if regex!(r"^(([A-Z]\.)?[0-9\.]+)$").is_match(section_candidate) {
            let id = section_candidate.trim_end_matches('.');

            if self.section_check_candidate(id, "") {
                let id = self.contents.substr(id).unwrap();

                let title = self.contents.substr(&id[id.len()..]).unwrap();

                return Some(Token::section(id, title, lineno, force_appendix));
            }
        }

        None
    }

    fn section_check_candidate(&self, id: &str, title: &str) -> bool {
        ensure!(Self::section_check_toc(title), false);
        ensure!(Self::section_check_weird_title(title), false);

        // if we have a possibly weird title, then use a monotonicity check
        let check_monotonic = !Self::section_check_possible_weird_title(title);

        self.section_check_id(id, check_monotonic)
    }

    fn section_check_id(&self, id: &str, check_monotonic: bool) -> bool {
        for (idx, part) in id.split('.').enumerate() {
            match part.parse() {
                Ok(Part::Num(v)) => {
                    // the first section isn't allowed to be a `0`
                    if idx == 0 {
                        ensure!(v > 0, false);
                    }
                }
                Ok(Part::Appendix(_)) => {
                    // only the first part is allowed to be an appendix ID
                    ensure!(idx == 0, false);
                }
                _ => return false,
            }
        }

        // if we previously had a break then it's likely a valid section
        if self.was_break && !check_monotonic {
            return true;
        }

        let Some(prev) = self.prev_section.as_ref() else {
            // if we don't have a section then make sure the first one is `1`
            return ["1", "1.0"].contains(&id);
        };

        Self::section_id_monotonic(prev, id)
    }

    fn section_id_monotonic(prev: &str, current: &str) -> bool {
        ensure!(prev != current, false);

        let prev_parts = prev.split('.');
        let current_parts = current.split('.');

        for (idx, (prev_part, current_part)) in prev_parts.zip(current_parts).enumerate() {
            let Some(prev_part): Option<Part> = prev_part.parse().ok() else {
                return false;
            };
            let Some(current_part): Option<Part> = current_part.parse().ok() else {
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

impl Iterator for Tokenizer<'_> {
    type Item = Token;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let line = self.lines.next()?;
            if let Some(token) = self.on_line(line) {
                return Some(token);
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! tests {
        ($(($name:ident, $id:expr)),* $(,)?) => {
            $(
                #[tokio::test]
                async fn $name() {
                    test_range($id..$id + 100).await
                }
            )*
        }
    }

    tests!(
        (rfc_30xx, 3000),
        (rfc_31xx, 3100),
        (rfc_32xx, 3200),
        (rfc_33xx, 3300),
        (rfc_34xx, 3400),
        (rfc_35xx, 3500),
        (rfc_36xx, 3600),
        (rfc_37xx, 3700),
        (rfc_38xx, 3800),
        (rfc_39xx, 3900),
    );

    tests!(
        (rfc_40xx, 4000),
        (rfc_41xx, 4100),
        (rfc_42xx, 4200),
        (rfc_43xx, 4300),
        (rfc_44xx, 4400),
        (rfc_45xx, 4500),
        (rfc_46xx, 4600),
        (rfc_47xx, 4700),
        (rfc_48xx, 4800),
        (rfc_49xx, 4900),
    );

    tests!(
        (rfc_50xx, 5000),
        (rfc_51xx, 5100),
        (rfc_52xx, 5200),
        (rfc_53xx, 5300),
        (rfc_54xx, 5400),
        (rfc_55xx, 5500),
        (rfc_56xx, 5600),
        (rfc_57xx, 5700),
        (rfc_58xx, 5800),
        (rfc_59xx, 5900),
    );

    tests!(
        (rfc_60xx, 6000),
        (rfc_61xx, 6100),
        (rfc_62xx, 6200),
        (rfc_63xx, 6300),
        (rfc_64xx, 6400),
        (rfc_65xx, 6500),
        (rfc_66xx, 6600),
        (rfc_67xx, 6700),
        (rfc_68xx, 6800),
        (rfc_69xx, 6900),
    );

    tests!(
        (rfc_70xx, 7000),
        (rfc_71xx, 7100),
        (rfc_72xx, 7200),
        (rfc_73xx, 7300),
        (rfc_74xx, 7400),
        (rfc_75xx, 7500),
        (rfc_76xx, 7600),
        (rfc_77xx, 7700),
        (rfc_78xx, 7800),
        (rfc_79xx, 7900),
    );

    tests!(
        (rfc_80xx, 8000),
        (rfc_81xx, 8100),
        (rfc_82xx, 8200),
        (rfc_83xx, 8300),
        (rfc_84xx, 8400),
        (rfc_85xx, 8500),
        (rfc_86xx, 8600),
        (rfc_87xx, 8700),
        (rfc_88xx, 8800),
        (rfc_89xx, 8900),
    );

    tests!(
        (rfc_90xx, 9000),
        (rfc_91xx, 9100),
        (rfc_92xx, 9200),
        (rfc_93xx, 9300),
        (rfc_94xx, 9400),
        (rfc_95xx, 9500),
        (rfc_96xx, 9600),
        (rfc_97xx, 9700),
        (rfc_98xx, 9800),
        (rfc_99xx, 9900),
    );

    async fn test_range(range: core::ops::Range<usize>) {
        for rfc in range {
            test_rfc(rfc).await;
        }
    }

    async fn test_rfc(rfc: usize) {
        let etc = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../etc"));

        // don't exceed the current largest RFC number
        ensure!(rfc <= 9671);

        // these RFCs don't have any sections
        let empty = [
            3005, 3099, 3129, 3199, 3232, 3268, 3299, 3364, 3442, 3494, 3499, 3599, 3818,
        ];

        // these RFCs have empty section titles
        let empty_titles = [
            (3002, "4.1.1"),
            (3002, "4.1.2"),
            (3002, "4.1.3"),
            (3002, "4.2.1"),
            (3002, "4.2.2"),
            (3002, "4.3.1"),
            (3002, "4.3.2"),
            (3002, "4.3.3"),
            (3002, "4.3.4"),
            (3002, "4.4.1"),
            (3002, "4.4.2"),
            (3002, "4.4.3"),
            (3002, "4.5.1"),
            (3002, "4.5.2"),
            (3002, "4.5.3"),
            (3002, "4.5.4"),
            (3002, "4.5.5"),
            (3002, "4.5.6"),
            (3002, "4.6.1"),
            (3002, "4.6.2"),
            (3002, "4.7.1"),
            (3002, "4.7.2"),
            (3122, "A"),
            (3172, "A"),
            (3258, "A"),
            (3304, "2.1.1"),
            (3304, "2.1.2"),
            (3304, "2.1.3"),
            (3304, "2.1.4"),
            (3304, "2.1.5"),
            (3304, "2.1.6"),
            (3304, "2.1.7"),
            (3304, "2.1.8"),
            (3304, "2.1.9"),
            (3304, "2.1.10"),
            (3304, "2.1.11"),
            (3304, "2.1.12"),
            (3304, "2.2.1"),
            (3304, "2.2.2"),
            (3304, "2.2.3"),
            (3304, "2.2.4"),
            (3304, "2.2.5"),
            (3304, "2.2.6"),
            (3304, "2.2.7"),
            (3304, "2.2.8"),
            (3304, "2.2.9"),
            (3304, "2.2.10"),
            (3304, "2.2.11"),
            (3304, "2.3.1"),
            (3304, "2.3.2"),
            (3304, "2.3.3"),
            (3304, "2.3.4"),
            (3332, "A"),
            (3411, "A"),
            (3552, "A"),
            (4009, "B.1"),
            (4009, "B.2"),
            (4009, "B.3"),
            (4009, "B.4"),
            (4233, "A"),
            (4269, "B.1"),
            (4269, "B.2"),
            (4269, "B.3"),
            (4269, "B.4"),
            (4523, "A"),
            (4666, "A"),
            (4951, "A"),
            (4951, "B"),
            (4951, "C"),
        ];

        // RFCs that are missing `Appendix` prefix for top-level IDs
        let top_level_prefix_missing = [
            (3039, "C.1"),
            (3220, "C.1"),
            (3220, "G.1"),
            (3344, "C.1"),
            (3344, "G.1"),
            (3623, "B.1"),
            (3739, "C.1"),
            (3851, "B.1"),
            (3880, "B.1"),
        ];

        // RFCs that use numbers for appendix IDs
        let number_appendix_ids = [
            (3175, "1"),
            (3946, "1"),
            (3549, "1"),
            (4258, "1"),
            (4606, "1"),
        ];

        // RFCs that use roman numberals
        let roman_appendix_ids = [(5357, "I")];

        // RFCs that have indented sections
        let indented_sections = [(3003, "4")];

        // these RFCs skip/reorder sections
        let skips = [
            (1050, "11.1"),
            (1125, "11"),
            (3090, "10"),
            (3132, "4.1.2.4"),
            (3134, "1.2.31"),
            (3162, "2.3"),
            (3186, "2.3.5"),
            (3204, "3"),
            (3208, "9.7.3"),
            (3212, "10"),
            (3234, "1.4"),
            (3284, "5.6"),
            (3257, "8"),
            (3296, "5.6"),
            (3258, "7"),
            (3326, "8"),
            (3326, "7"),
            (3326, "9"),
            (3331, "11.0"),
            (3348, "5"),
            (3383, "10"),
            (3428, "16"),
            (3475, "9"),
            (3509, "10"),
            (3568, "8"),
            (3671, "3.13"),
            (3701, "5"),
            (3810, "5.1.7"),
            (3825, "6"),
            (3868, "7.3.4"),
            (3877, "3.3.5"),
            (3929, "10"),
            (4037, "16"),
            (4160, "4.6"),
            (4469, "9"),
            (4540, "3.5.16"),
            (4540, "5.3.17"),
            (4604, "8"),
            (4695, "E.1"),
            (4715, "10"),
            (4842, "18"),
            (4853, "6"),
            (5013, "10"),
            (5322, "7"),
            (5570, "5.1.5"),
            (5805, "4.4"),
            (5849, "6"),
            (5850, "5"),
            (5858, "8"),
            (5892, "8"),
            (6219, "11"),
            (6484, "1.5.4"),
            (6484, "5.4.8"),
            (6484, "5.6"),
            (6485, "9"),
            (6722, "5"),
            (6730, "12"),
        ];

        // these RFCs have duplicate sections
        let duplicate = [
            (3063, "6.2.1"),
            (3063, "A.5.2"),
            (3093, "3.2"),
            (3119, "11"),
            (3131, "10"),
            (3250, "3"),
            (3284, "5.4"),
            (3302, "6"),
            (3414, "12.1"),
            (3418, "6.1"),
            (3476, "8"),
            (3562, "3"),
            (3745, "6"),
            (3785, "6.1"),
            (3946, "1"), // uses both Appendix and Annex
            (4511, "C.2.1"),
            (4520, "A.8"),
            (4606, "1"), // uses both Appendix and Annex
            (4949, "7"),
            (5570, "2.4.2"),
            (5755, "10.2"),
        ];

        // _really_ messed up RFCs
        let janky_sections = [
            (3015, "A"),
            (3113, "8"),
            (3113, "9"),
            (3133, "1"),
            (3134, "1"),
            (3525, "A.1"),
            (3525, "I"),
            (3730, "1"), // Appendices repeat section counters
            (3730, "B"), // Appendices repeat section counters
        ];

        // ignore missing IDs
        if NOT_FOUND.iter().any(|nf| nf.contains(&rfc)) {
            return;
        }

        println!("rfc{rfc}");

        let file = duvet_core::http::get_cached_string(
            format!("https://www.rfc-editor.org/rfc/rfc{rfc}.txt"),
            etc.join(format!("rfc{rfc}.txt")),
        )
        .await
        .unwrap();

        let tokens = Tokenizer::new(&file).collect::<Vec<_>>();

        insta::assert_debug_snapshot!(format!("rfc{rfc}"), tokens);

        // don't do any checks right now
        if ERRORS.iter().any(|e| e.contains(&rfc)) {
            return;
        }

        let mut sections = vec![];

        let mut prev_section = None;

        let mut check_section = |id: &str, title: &str| {
            assert!(!id.is_empty());

            assert_eq!(empty_titles.contains(&(rfc, id)), title.is_empty());

            let prev = core::mem::replace(&mut prev_section, Some(id.to_string()));

            let Some(prev) = prev else {
                assert!(["1", "1.0"].contains(&id));
                return;
            };

            if *prev == *id {
                assert!(duplicate.contains(&(rfc, id)), "duplicate section: {id:?}");
                return;
            }

            let is_ok = Tokenizer::section_id_monotonic(&prev, id);

            if !janky_sections.contains(&(rfc, id)) {
                let key = &(rfc, id);
                let expected = !(skips.contains(key)
                    || indented_sections.contains(key)
                    || top_level_prefix_missing.contains(key)
                    || number_appendix_ids.contains(key)
                    || roman_appendix_ids.contains(key));

                assert_eq!(
                    is_ok, expected,
                    "unexpected section number: prev={prev:?} current={id:?}"
                );
            }
        };

        let mut line = 0;
        for token in tokens {
            // make sure we don't drop any lines
            assert_eq!(line, token.line());
            line = token.line() + 1;

            match &token {
                Token::Section { id, title, .. } => {
                    println!("  SECTION(id={id:?} title={title:?})");

                    check_section(id, title);

                    sections.push(token);
                }
                Token::Appendix { id, title, .. } => {
                    println!(" APPENDIX(id={id:?} title={title:?})");

                    check_section(id, title);

                    sections.push(token);
                }
                Token::Break { .. } => {
                    // TODO
                }
                Token::Content { .. } => {
                    // TODO
                }
                Token::Header { .. } => {
                    // TODO
                }
            }
        }

        assert_eq!(
            sections.is_empty(),
            empty.contains(&rfc),
            "RFC sections is empty"
        );
    }

    // these currently have parsing errors
    static ERRORS: &[&[usize]] = &[
        &[
            19, 70, 77, 98, 107, 155, 172, 194, 199, 230, 240, 254, 271, 293, 329, 330, 331, 332,
            333, 354,
            // TODO gap
        ],
        &[
            768, 778, 782, 783, 787, 789, 799, 800, 802, 803, 810, 869, 876, 887, 891, 892, 896,
            899, 904, 911, 914, 994, 995, 999, 1001, 1002, 1005, 1014, 1035, 1038, 1045, 1076,
            1099, 1123, 1138, 1142, 1148, 1163, 1180, 1190, 1195, 1199, 1244, 1245,
            1246,
            // TODO gap
        ],
        &[
            3064, // The first sections is `1.0.Introduction`
            3502, // This starts on 6.3.11
            3877, // The sections embed sequence diagrams
        ],
        &[
            5054, // this has a section with a title with lots of spaces
            5165, // this section has poorly formatted sections
        ],
        &[
            6503, // this embeds messages into the section
            6504, // this embeds messages into the section
            6917, // this embeds messages into the section
        ],
        &[
            7058, // This RFC embeds sequence diagrams in the sections
        ],
        &[
            9592, // This RFC embeds another RFC in the appendix, which fails the monotonic check
        ],
    ];

    /// Invalid IDs
    static NOT_FOUND: &[&[usize]] = &[
        &[
            // 3xxx
            3100, 3200, 3223, 3328, 3333, 3350, 3399, 3400, 3500, 3699, 3799, 3800, 3889, 3899,
            3900, 3907, 3908, 3999,
        ],
        &[
            // 4xxx
            4000, 4099, 4100, 4199, 4200, 4232, 4299, 4300, 4399, 4400, 4499, 4500, 4599, 4600,
            4637, 4658, 4751, 4699, 4700, 4799, 4800, 4809, 4899, 4900, 4921, 4922, 4989, 4999,
        ],
        &[
            // 5xxx
            5099, 5100, 5108, 5199, 5200, 5299, 5300, 5312, 5313, 5314, 5315, 5319, 5399, 5400,
            5499, 5500, 5599, 5600, 5699, 5700, 5799, 5800, 5809, 5821, 5822, 5823, 5899, 5900,
            5999,
        ],
        &[
            // 6xxx
            6000, 6099, 6100, 6102, 6103, 6199, 6200, 6299, 6300, 6399, 6400, 6499, 6500, 6523,
            6524, 6599, 6600, 6634, 6699, 6700, 6799, 6800, 6899, 6900, 6966, 6995, 6999,
        ],
        &[
            // 7xxx
            7000, 7099, 7327, 7907,
        ],
        &[
            // 8xxx
            8523, 8524, 8535, 8566, 8626, 8644, 8646, 8647, 8648, 8988,
        ],
        &[
            // 9xxx
            9123, 9379, 9563, 9602, 9609, 9610, 9621, 9622, 9623, 9626, 9627, 9628, 9633, 9634,
            9636, 9639, 9649, 9655, 9658, 9664, 9665, 9666, 9667, 9668, 9669, 9670,
        ],
    ];
}
