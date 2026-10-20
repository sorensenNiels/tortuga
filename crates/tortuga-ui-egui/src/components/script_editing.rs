//! Editing keys for scripts (R-TEST-14): Tab and Shift+Tab indent and outdent, Enter keeps
//! the indent, Ctrl+/ comments lines out; and putting a snippet or a test into a script
//! (R-TEST-13, R-TEST-15). Plain functions on the text and the selection, both in
//! characters as egui counts them.

/// Spaces per indent level.
const INDENT: &str = "  ";

/// A text after an edit and the selection in it, as (start, end) character offsets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edited {
    pub text: String,
    pub selection: (usize, usize),
}

/// Byte offset of character `at`, or the end.
fn byte(text: &str, at: usize) -> usize {
    text.char_indices().nth(at).map_or(text.len(), |(b, _)| b)
}

fn chars(text: &str) -> usize {
    text.chars().count()
}

/// The lines a selection touches, as their start character offsets. A selection that ends
/// at the very start of a line does not take that line.
fn touched_lines(text: &str, (start, end): (usize, usize)) -> Vec<usize> {
    let mut starts = vec![0];
    for (i, c) in text.chars().enumerate() {
        if c == '\n' {
            starts.push(i + 1);
        }
    }
    let last = if end > start && starts.contains(&end) {
        end - 1
    } else {
        end
    };
    starts
        .iter()
        .enumerate()
        .filter(|&(n, &s)| {
            let next = starts.get(n + 1).copied().unwrap_or(usize::MAX);
            next > start && s <= last
        })
        .map(|(_, &s)| s)
        .collect()
}

/// Inserts `insert` at each of `at` (character offsets, ascending).
fn insert_at(text: &str, at: &[usize], insert: &str) -> String {
    let mut out = String::with_capacity(text.len() + at.len() * insert.len());
    for (i, c) in text.chars().enumerate() {
        if at.contains(&i) {
            out.push_str(insert);
        }
        out.push(c);
    }
    if at.contains(&chars(text)) {
        out.push_str(insert);
    }
    out
}

fn sorted((a, b): (usize, usize)) -> (usize, usize) {
    (a.min(b), a.max(b))
}

/// Tab: two spaces at the cursor, or every touched line indented.
pub fn indent(text: &str, selection: (usize, usize)) -> Edited {
    let (start, end) = sorted(selection);
    let width = chars(INDENT);
    if start == end {
        let b = byte(text, start);
        return Edited {
            text: format!("{}{INDENT}{}", &text[..b], &text[b..]),
            selection: (start + width, start + width),
        };
    }
    let lines = touched_lines(text, (start, end));
    Edited {
        text: insert_at(text, &lines, INDENT),
        selection: (start + width, end + width * lines.len()),
    }
}

/// Shift+Tab: up to one indent level off every touched line.
pub fn outdent(text: &str, selection: (usize, usize)) -> Edited {
    let (start, end) = sorted(selection);
    let lines = touched_lines(text, (start, end));
    let all: Vec<char> = text.chars().collect();
    let mut remove = Vec::new();
    for &line in &lines {
        let spaces = all[line..]
            .iter()
            .take(chars(INDENT))
            .take_while(|c| **c == ' ')
            .count();
        let n = if all.get(line) == Some(&'\t') {
            1
        } else {
            spaces
        };
        remove.extend(line..line + n);
    }
    let text: String = all
        .iter()
        .enumerate()
        .filter(|(i, _)| !remove.contains(i))
        .map(|(_, c)| c)
        .collect();
    let before = |at: usize| remove.iter().filter(|&&r| r < at).count();
    Edited {
        text,
        selection: (start - before(start), end - before(end)),
    }
}

/// Enter: a line break with the indent of the line, one level more after `{`, `[` or
/// `(`; between brackets (`{|}`) the closing one goes on a line of its own.
pub fn newline(text: &str, selection: (usize, usize)) -> Edited {
    let (start, end) = sorted(selection);
    let (sb, eb) = (byte(text, start), byte(text, end));
    let line_start = text[..sb].rfind('\n').map_or(0, |i| i + 1);
    let indent: String = text[line_start..]
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect();
    let opens = text[line_start..sb].trim_end().chars().last();
    let closes = text[eb..].chars().next();
    let deeper = matches!(opens, Some('{' | '[' | '('));
    let inner = if deeper {
        format!("{indent}{INDENT}")
    } else {
        indent.clone()
    };
    let between = matches!(
        (opens, closes),
        (Some('{'), Some('}')) | (Some('['), Some(']')) | (Some('('), Some(')'))
    );
    let inserted = if between {
        format!("\n{inner}\n{indent}")
    } else {
        format!("\n{inner}")
    };
    let cursor = start + 1 + chars(&inner);
    Edited {
        text: format!("{}{inserted}{}", &text[..sb], &text[eb..]),
        selection: (cursor, cursor),
    }
}

/// Ctrl+/: comments the touched lines out with `// `, or back in when they all are.
/// Blank lines are left alone; the selection covers the lines afterwards.
pub fn toggle_comment(text: &str, selection: (usize, usize)) -> Edited {
    let lines = touched_lines(text, sorted(selection));
    let all: Vec<&str> = text.split('\n').collect();
    // `touched_lines` gives start offsets; the line numbers are their positions.
    let mut offset = 0;
    let mut numbers = Vec::new();
    for (n, line) in all.iter().enumerate() {
        if lines.contains(&offset) {
            numbers.push(n);
        }
        offset += chars(line) + 1;
    }
    let code: Vec<usize> = numbers
        .iter()
        .copied()
        .filter(|&n| !all[n].trim().is_empty())
        .collect();
    let commented = !code.is_empty() && code.iter().all(|&n| all[n].trim_start().starts_with("//"));
    let indent = code
        .iter()
        .map(|&n| all[n].len() - all[n].trim_start().len())
        .min()
        .unwrap_or(0);
    let mut out: Vec<String> = all.iter().map(|l| (*l).to_owned()).collect();
    for &n in &code {
        let line = all[n];
        out[n] = if commented {
            let at = line.len() - line.trim_start().len();
            let rest = &line[at + 2..];
            format!("{}{}", &line[..at], rest.strip_prefix(' ').unwrap_or(rest))
        } else {
            // The indent is ASCII spaces and tabs, so `indent` is a char boundary.
            format!("{}// {}", &line[..indent], &line[indent..])
        };
    }
    let text = out.join("\n");
    let first = numbers
        .first()
        .map_or(0, |&n| out[..n].iter().map(|l| chars(l) + 1).sum::<usize>());
    let last = numbers.last().map_or(0, |&n| {
        out[..=n].iter().map(|l| chars(l) + 1).sum::<usize>() - 1
    });
    Edited {
        text,
        selection: (first, last),
    }
}

/// `block` on lines of its own: after the line with the cursor (in place of it when it is
/// blank), or after the script with a blank line between. The cursor goes after it.
pub fn insert_block(text: &str, cursor: Option<usize>, block: &str) -> Edited {
    let (head, tail) = match cursor {
        Some(c) => {
            let at = byte(text, c);
            let start = text[..at].rfind('\n').map_or(0, |i| i + 1);
            let end = text[at..].find('\n').map_or(text.len(), |i| at + i);
            if text[start..end].trim().is_empty() {
                (text[..start].to_owned(), text[end..].to_owned())
            } else {
                (format!("{}\n", &text[..end]), text[end..].to_owned())
            }
        }
        None if text.trim().is_empty() => (String::new(), String::new()),
        None => {
            let kept = text.trim_end_matches('\n');
            (format!("{kept}\n\n"), String::new())
        }
    };
    let cursor = chars(&head) + chars(block);
    Edited {
        text: format!("{head}{block}{tail}"),
        selection: (cursor, cursor),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edited(text: &str, selection: (usize, usize)) -> Edited {
        Edited {
            text: text.to_owned(),
            selection,
        }
    }

    #[test]
    fn tab_indents_at_the_cursor_or_every_selected_line() {
        assert_eq!(indent("ab", (1, 1)), edited("a  b", (3, 3)));
        // "a\nb\nc" with "a" to "b" selected: lines 1 and 2.
        assert_eq!(indent("a\nb\nc", (0, 3)), edited("  a\n  b\nc", (2, 7)));
        // Ending at the start of a line does not take that line.
        assert_eq!(indent("a\nb", (0, 2)), edited("  a\nb", (2, 4)));
        assert_eq!(indent("ü😀\nx", (0, 4)), edited("  ü😀\n  x", (2, 8)));
    }

    #[test]
    fn shift_tab_takes_one_level_off() {
        assert_eq!(outdent("    a", (4, 4)), edited("  a", (2, 2)));
        assert_eq!(outdent(" a\n  b\nc", (0, 7)), edited("a\nb\nc", (0, 4)));
        assert_eq!(outdent("\ta", (1, 1)), edited("a", (0, 0)));
        assert_eq!(
            outdent("a", (1, 1)),
            edited("a", (1, 1)),
            "nothing to take off"
        );
    }

    #[test]
    fn enter_keeps_the_indent_and_opens_blocks() {
        assert_eq!(newline("  a", (3, 3)), edited("  a\n  ", (6, 6)));
        assert_eq!(
            newline("if (x) {", (8, 8)),
            edited("if (x) {\n  ", (11, 11))
        );
        assert_eq!(
            newline("  f({})", (5, 5)),
            edited("  f({\n    \n  })", (10, 10))
        );
        assert_eq!(
            newline("ab", (0, 2)),
            edited("\n", (1, 1)),
            "replaces the selection"
        );
        assert_eq!(newline("", (0, 0)), edited("\n", (1, 1)));
    }

    #[test]
    fn blocks_go_on_lines_of_their_own() {
        assert_eq!(insert_block("", None, "x();"), edited("x();", (4, 4)));
        assert_eq!(
            insert_block("a();\n\n\n", None, "x();"),
            edited("a();\n\nx();", (10, 10))
        );
        assert_eq!(
            insert_block("a();\nb();", Some(1), "x();"),
            edited("a();\nx();\nb();", (9, 9))
        );
        assert_eq!(
            insert_block("a();\n  \nb();", Some(6), "x();"),
            edited("a();\nx();\nb();", (9, 9))
        );
        assert_eq!(insert_block("ü😀", Some(2), "x"), edited("ü😀\nx", (4, 4)));
    }

    #[test]
    fn ctrl_slash_comments_lines_out_and_back_in() {
        let code = "  a();\n\n    b();";
        let out = toggle_comment(code, (0, chars(code)));
        assert_eq!(
            out.text, "  // a();\n\n  //   b();",
            "same column, blank lines alone"
        );
        let back = toggle_comment(&out.text, out.selection);
        assert_eq!(back.text, code);
        assert_eq!(toggle_comment("x", (0, 0)).text, "// x");
        assert_eq!(toggle_comment("//x", (1, 1)).text, "x");
        assert_eq!(toggle_comment("", (0, 0)).text, "");
        assert_eq!(toggle_comment("ü😀", (1, 1)).text, "// ü😀");
    }
}
