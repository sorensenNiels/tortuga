//! Editing keys for scripts (R-TEST-14): Tab and Shift+Tab indent and outdent, Enter keeps
//! the indent, Ctrl+/ comments lines out; the line commands, smart Home and bracket pairs
//! (R-TEST-17); and putting a snippet or a test into a script (R-TEST-13, R-TEST-15).
//! Plain functions on the text and the selection, both in characters as egui counts them.

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
    if start != end {
        return indent_lines(text, selection);
    }
    let width = chars(INDENT);
    let b = byte(text, start);
    Edited {
        text: format!("{}{INDENT}{}", &text[..b], &text[b..]),
        selection: (start + width, start + width),
    }
}

/// Ctrl+]: every touched line indented, also the cursor's line without a selection
/// (R-TEST-17).
pub fn indent_lines(text: &str, selection: (usize, usize)) -> Edited {
    let (start, end) = sorted(selection);
    let width = chars(INDENT);
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

/// Which way lines move or are copied (R-TEST-17).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
}

/// The touched lines as one block: from the first line's start to the end of the last
/// line, before its line break.
fn line_block(all: &[char], selection: (usize, usize)) -> (usize, usize) {
    let text: String = all.iter().collect();
    let lines = touched_lines(&text, sorted(selection));
    let (first, last) = (
        lines.first().copied().unwrap_or(0),
        lines.last().copied().unwrap_or(0),
    );
    let end = all
        .iter()
        .skip(last)
        .position(|c| *c == '\n')
        .map_or(all.len(), |n| last + n);
    (first, end)
}

/// The start of the line that character `at` is on.
fn line_start(all: &[char], at: usize) -> usize {
    all[..at.min(all.len())]
        .iter()
        .rposition(|c| *c == '\n')
        .map_or(0, |n| n + 1)
}

fn joined(parts: &[&[char]]) -> String {
    parts.iter().flat_map(|p| p.iter()).collect()
}

fn unchanged(text: &str, selection: (usize, usize)) -> Edited {
    Edited {
        text: text.to_owned(),
        selection: sorted(selection),
    }
}

/// Alt+↑ / Alt+↓: the touched lines swap places with the line above or below; the
/// selection moves along.
pub fn move_lines(text: &str, selection: (usize, usize), direction: Direction) -> Edited {
    let all: Vec<char> = text.chars().collect();
    let (start, end) = sorted(selection);
    let (first, last) = line_block(&all, (start, end));
    let block = &all[first..last];
    match direction {
        Direction::Up if first > 0 => {
            let above = line_start(&all, first - 1);
            let line = &all[above..first - 1];
            let shift = first - above;
            Edited {
                text: joined(&[&all[..above], block, &['\n'], line, &all[last..]]),
                selection: (start - shift, end - shift),
            }
        }
        Direction::Down if last < all.len() => {
            let below = all
                .iter()
                .skip(last + 1)
                .position(|c| *c == '\n')
                .map_or(all.len(), |n| last + 1 + n);
            let line = &all[last + 1..below];
            let shift = below - last;
            Edited {
                text: joined(&[&all[..first], line, &['\n'], block, &all[below..]]),
                selection: (start + shift, end + shift),
            }
        }
        Direction::Up | Direction::Down => unchanged(text, selection),
    }
}

/// Shift+Alt+↑ / ↓: the touched lines once more, above or below them; the selection
/// stays on the upper copy going up and goes to the lower one going down.
pub fn copy_lines(text: &str, selection: (usize, usize), direction: Direction) -> Edited {
    let all: Vec<char> = text.chars().collect();
    let (start, end) = sorted(selection);
    let (first, last) = line_block(&all, (start, end));
    let block = &all[first..last];
    let shift = match direction {
        Direction::Up => 0,
        Direction::Down => last - first + 1,
    };
    Edited {
        text: joined(&[&all[..last], &['\n'], block, &all[last..]]),
        selection: (start + shift, end + shift),
    }
}

/// Ctrl+Shift+K: the touched lines go, with their line break; the cursor goes to the
/// start of the line now there.
pub fn delete_lines(text: &str, selection: (usize, usize)) -> Edited {
    let all: Vec<char> = text.chars().collect();
    let (first, last) = line_block(&all, selection);
    let (from, to, cursor) = if last < all.len() {
        (first, last + 1, first)
    } else if first > 0 {
        // The last line: the line break before it goes instead.
        (first - 1, last, line_start(&all, first - 1))
    } else {
        (0, last, 0)
    };
    Edited {
        text: joined(&[&all[..from], &all[to..]]),
        selection: (cursor, cursor),
    }
}

/// Shift+Alt+A: `/* … */` around the selection, or off again when the selection is one
/// such comment. Without a selection, an empty comment with the cursor inside.
pub fn block_comment(text: &str, selection: (usize, usize)) -> Edited {
    let all: Vec<char> = text.chars().collect();
    let (start, end) = sorted(selection);
    let picked: String = all[start..end].iter().collect();
    if start == end {
        return Edited {
            text: joined(&[
                &all[..start],
                &['/', '*', ' ', ' ', '*', '/'],
                &all[start..],
            ]),
            selection: (start + 3, start + 3),
        };
    }
    let inner = picked
        .strip_prefix("/*")
        .and_then(|p| p.strip_suffix("*/"))
        .filter(|_| picked.len() >= 4);
    let replaced = match inner {
        Some(inner) => {
            let inner = inner.strip_prefix(' ').unwrap_or(inner);
            inner.strip_suffix(' ').unwrap_or(inner).to_owned()
        }
        None => format!("/* {picked} */"),
    };
    let tail: String = all[end..].iter().collect();
    let head: String = all[..start].iter().collect();
    Edited {
        selection: (start, start + chars(&replaced)),
        text: format!("{head}{replaced}{tail}"),
    }
}

/// Home: where the cursor at `at` goes, the line's first character that isn't a space,
/// or the line's start when it is there already.
pub fn smart_home(text: &str, at: usize) -> usize {
    let all: Vec<char> = text.chars().collect();
    let at = at.min(all.len());
    let start = line_start(&all, at);
    let code = start
        + all[start..]
            .iter()
            .take_while(|c| **c == ' ' || **c == '\t')
            .count();
    if at == code { start } else { code }
}

/// The closing partner of a bracket or quote that is paired as it is typed.
fn partner(c: char) -> Option<char> {
    match c {
        '(' => Some(')'),
        '[' => Some(']'),
        '{' => Some('}'),
        '"' | '\'' | '`' => Some(c),
        _ => None,
    }
}

/// The text with the selection replaced by `with`, the cursor after it.
pub fn replace_selection(text: &str, selection: (usize, usize), with: &str) -> Edited {
    let (start, end) = sorted(selection);
    let (sb, eb) = (byte(text, start), byte(text, end));
    let cursor = start + chars(with);
    Edited {
        text: format!("{}{with}{}", &text[..sb], &text[eb..]),
        selection: (cursor, cursor),
    }
}

/// The selected text.
pub fn selected(text: &str, selection: (usize, usize)) -> &str {
    let (start, end) = sorted(selection);
    &text[byte(text, start)..byte(text, end)]
}

/// Typing `c` with brackets and quotes paired (R-TEST-17): an opening one wraps the
/// selection or puts in its partner, a closing one next to the cursor is stepped over.
/// `None` when `c` is just typed.
pub fn type_paired(text: &str, selection: (usize, usize), c: char) -> Option<Edited> {
    let all: Vec<char> = text.chars().collect();
    let (start, end) = sorted(selection);
    let next = all.get(end).copied();
    let before = start.checked_sub(1).and_then(|i| all.get(i)).copied();
    if start == end && next == Some(c) && matches!(c, ')' | ']' | '}' | '"' | '\'' | '`') {
        return Some(Edited {
            text: text.to_owned(),
            selection: (start + 1, start + 1),
        });
    }
    let close = partner(c)?;
    if start != end {
        let picked: String = all[start..end].iter().collect();
        let wrapped = format!("{c}{picked}{close}");
        let mut edited = replace_selection(text, (start, end), &wrapped);
        edited.selection = (start + 1, end + 1);
        return Some(edited);
    }
    let quote = c == close;
    // A quote after a word is an apostrophe or ends a string; before one it starts a
    // string that is already there.
    if quote
        && (before.is_some_and(|b| b.is_alphanumeric() || b == '\\')
            || next.is_some_and(char::is_alphanumeric))
    {
        return None;
    }
    // A bracket before a word wraps that word, so it is typed alone.
    if !quote && next.is_some_and(|n| n.is_alphanumeric() || n == '_' || n == '$') {
        return None;
    }
    let mut edited = replace_selection(text, (start, end), &format!("{c}{close}"));
    edited.selection = (start + 1, start + 1);
    Some(edited)
}

/// Backspace between an empty pair, `(|)`, deletes both (R-TEST-17). `None` otherwise.
pub fn backspace_pair(text: &str, selection: (usize, usize)) -> Option<Edited> {
    let (start, end) = sorted(selection);
    if start != end || start == 0 {
        return None;
    }
    let all: Vec<char> = text.chars().collect();
    let (open, close) = (all.get(start - 1)?, all.get(start)?);
    (partner(*open) == Some(*close)).then(|| Edited {
        text: joined(&[&all[..start - 1], &all[start + 1..]]),
        selection: (start - 1, start - 1),
    })
}

/// Shift+Alt+F: the script beautified, the cursor where it was as far as the text goes.
pub fn beautify(text: &str, selection: (usize, usize)) -> Edited {
    let text = tortuga_core::js_format::beautify(text);
    let cursor = sorted(selection).0.min(chars(&text));
    Edited {
        text,
        selection: (cursor, cursor),
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

    /// R-TEST-17: Ctrl+] indents the cursor's line too.
    #[test]
    fn indent_lines_takes_the_cursor_line() {
        assert_eq!(indent_lines("a\nbc", (3, 3)), edited("a\n  bc", (5, 5)));
        assert_eq!(indent_lines("", (0, 0)), edited("  ", (2, 2)));
    }

    /// R-TEST-17: lines move past their neighbour, the selection with them; at the top or
    /// bottom nothing happens.
    #[test]
    fn lines_move_up_and_down() {
        let up = Direction::Up;
        let down = Direction::Down;
        assert_eq!(move_lines("a\nb\nc", (2, 2), up), edited("b\na\nc", (0, 0)));
        assert_eq!(
            move_lines("a\nb\nc", (2, 2), down),
            edited("a\nc\nb", (4, 4))
        );
        assert_eq!(
            move_lines("a\nb\nc", (0, 3), down),
            edited("c\na\nb", (2, 5))
        );
        assert_eq!(move_lines("a\nb", (0, 1), up), edited("a\nb", (0, 1)));
        assert_eq!(move_lines("a\nb", (3, 3), down), edited("a\nb", (3, 3)));
        assert_eq!(move_lines("ü😀\nx", (3, 4), up), edited("x\nü😀", (0, 1)));
        assert_eq!(move_lines("", (0, 0), up), edited("", (0, 0)));
    }

    /// R-TEST-17: copying keeps the selection on the upper copy, or moves it down.
    #[test]
    fn lines_are_copied_up_and_down() {
        assert_eq!(
            copy_lines("a\nb", (0, 1), Direction::Up),
            edited("a\na\nb", (0, 1))
        );
        assert_eq!(
            copy_lines("a\nb", (3, 3), Direction::Down),
            edited("a\nb\nb", (5, 5))
        );
        assert_eq!(
            copy_lines("", (0, 0), Direction::Down),
            edited("\n", (1, 1))
        );
        assert_eq!(
            copy_lines("ü😀", (1, 1), Direction::Down),
            edited("ü😀\nü😀", (4, 4))
        );
    }

    /// R-TEST-17: deleting takes the lines and one line break.
    #[test]
    fn lines_are_deleted() {
        assert_eq!(delete_lines("a\nb\nc", (2, 2)), edited("a\nc", (2, 2)));
        assert_eq!(delete_lines("a\nb\nc", (0, 3)), edited("c", (0, 0)));
        assert_eq!(delete_lines("a\nbc", (3, 3)), edited("a", (0, 0)));
        assert_eq!(delete_lines("x", (1, 1)), edited("", (0, 0)));
        assert_eq!(delete_lines("", (0, 0)), edited("", (0, 0)));
    }

    /// R-TEST-17: a block comment goes around the selection and comes off again.
    #[test]
    fn block_comments_wrap_and_unwrap() {
        let on = block_comment("a b c", (2, 3));
        assert_eq!(on, edited("a /* b */ c", (2, 9)));
        assert_eq!(
            block_comment(&on.text, on.selection),
            edited("a b c", (2, 3))
        );
        assert_eq!(block_comment("ab", (1, 1)), edited("a/*  */b", (4, 4)));
        assert_eq!(block_comment("ü😀", (0, 2)).text, "/* ü😀 */");
    }

    /// R-TEST-17: Home goes to the code, then to the line's start.
    #[test]
    fn home_goes_to_the_code_then_the_start() {
        assert_eq!(smart_home("x\n    ab", 8), 6);
        assert_eq!(smart_home("x\n    ab", 6), 2);
        assert_eq!(smart_home("x\n    ab", 3), 6);
        assert_eq!(smart_home("", 0), 0);
        assert_eq!(smart_home("ü😀", 2), 0);
    }

    /// R-TEST-17: brackets and quotes are paired, wrap a selection and are stepped over.
    #[test]
    fn brackets_and_quotes_are_paired() {
        assert_eq!(type_paired("f", (1, 1), '('), Some(edited("f()", (2, 2))));
        assert_eq!(type_paired("f()", (2, 2), ')'), Some(edited("f()", (3, 3))));
        assert_eq!(
            type_paired("ab", (0, 2), '"'),
            Some(edited("\"ab\"", (1, 3)))
        );
        assert_eq!(
            type_paired("x ", (2, 2), '\''),
            Some(edited("x ''", (3, 3)))
        );
        assert_eq!(type_paired("don", (3, 3), '\''), None, "an apostrophe");
        assert_eq!(type_paired("x", (0, 0), '('), None, "before a word");
        assert_eq!(type_paired("x", (1, 1), 'a'), None);
        assert_eq!(
            type_paired("ü😀", (2, 2), '['),
            Some(edited("ü😀[]", (3, 3)))
        );
    }

    /// R-TEST-17: Backspace between an empty pair takes both.
    #[test]
    fn backspace_deletes_an_empty_pair() {
        assert_eq!(backspace_pair("f()", (2, 2)), Some(edited("f", (1, 1))));
        assert_eq!(backspace_pair("f(x)", (2, 2)), None);
        assert_eq!(backspace_pair("()", (0, 0)), None);
        assert_eq!(backspace_pair("ü\"\"", (2, 2)), Some(edited("ü", (1, 1))));
    }

    #[test]
    fn selections_are_replaced_and_read() {
        assert_eq!(
            replace_selection("aü😀b", (1, 3), "x"),
            edited("axb", (2, 2))
        );
        assert_eq!(selected("aü😀b", (3, 1)), "ü😀");
    }
}
