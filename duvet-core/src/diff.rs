use console::{style, Style};
use similar::{udiff::UnifiedHunkHeader, Algorithm, ChangeTag, TextDiff};
use std::{
    io::{self, Write},
    time::Duration,
};

pub fn dump<Output: Write>(mut o: Output, old: &str, new: &str) -> io::Result<()> {
    let diff = TextDiff::configure()
        .timeout(Duration::from_millis(200))
        .algorithm(Algorithm::Patience)
        .diff_lines(old, new);

    for group in diff.grouped_ops(4).into_iter() {
        if group.is_empty() {
            continue;
        }

        // find the previous text that doesn't have an indent
        let line = diff.iter_changes(&group[0]).next().unwrap().value();
        let scope = find_scope(old, line);

        let header = style(UnifiedHunkHeader::new(&group)).cyan();

        if scope != line {
            writeln!(o, "{header} {scope}")?;
        } else {
            writeln!(o, "{header}")?;
        }

        for op in group {
            for change in diff.iter_inline_changes(&op) {
                let (marker, style) = match change.tag() {
                    ChangeTag::Delete => ('-', Style::new().red()),
                    ChangeTag::Insert => ('+', Style::new().green()),
                    ChangeTag::Equal => (' ', Style::new().dim()),
                };
                write!(o, "{}", style.apply_to(marker).dim().bold())?;
                for &(emphasized, value) in change.values() {
                    if emphasized {
                        write!(o, "{}", style.clone().underlined().bold().apply_to(value))?;
                    } else {
                        write!(o, "{}", style.apply_to(value))?;
                    }
                }
            }
        }
    }

    Ok(())
}

/// Finds the most recent non-empty line with no indentation
fn find_scope<'a>(old: &'a str, mut line: &'a str) -> &'a str {
    let base = old.as_ptr() as usize;

    while line.is_empty() || line.starts_with(char::is_whitespace) {
        let len = (line.as_ptr() as usize).saturating_sub(base);

        let Some(subject) = old[..len].lines().next_back() else {
            break;
        };

        line = subject;
    }

    line
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_scope() {
        let text = r#"
header
foo

    bar

        baz
"#
        .trim_start();

        assert_eq!(find_scope(text, text.lines().next().unwrap()), "header");
        assert_eq!(find_scope(text, text.lines().nth(1).unwrap()), "foo");
        assert_eq!(find_scope(text, text.lines().last().unwrap()), "foo");
    }
}
