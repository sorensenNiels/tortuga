//! Scanning JavaScript for the imports that translate scripts (R-COL-19, R-COL-31,
//! R-COL-32): names, calls and their arguments, outside strings and comments.
//!
//! The scanner only splits the text at ASCII characters, so any text is safe.

/// A script in Tortuga's script API.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Translation {
    /// Everything was translated: the script can run as it is.
    Complete(String),
    /// Parts still use the other tool's API, or the script doesn't parse: what could be
    /// translated, to finish by hand.
    Partial(String),
}

/// What a translation makes of the code that `s` starts with: how much of `s` it
/// replaces, and with what.
pub(crate) type Translate = fn(&str) -> Option<(usize, String)>;

/// `code` with every call `at` knows translated.
pub(crate) fn rewrite(code: &str, at: Translate) -> String {
    let bytes = code.as_bytes();
    let mut out = String::with_capacity(code.len());
    let mut copied = 0;
    let mut i = 0;
    while let Some(&byte) = bytes.get(i) {
        if let Some(end) = literal_end(code, i) {
            i = end;
            continue;
        }
        // An ASCII letter is always at a char boundary.
        let starts_word = byte.is_ascii_alphabetic() && starts_name(code, i);
        match starts_word.then(|| at(&code[i..])).flatten() {
            Some((len, replacement)) => {
                out.push_str(&code[copied..i]);
                out.push_str(&replacement);
                i += len;
                copied = i;
            }
            None => i += 1,
        }
    }
    out.push_str(&code[copied..]);
    out
}

/// `code` with the name `from` (not a property, string or comment) renamed to `to`.
pub(crate) fn renamed(code: &str, from: &str, to: &str) -> String {
    let bytes = code.as_bytes();
    let mut out = String::with_capacity(code.len());
    let mut copied = 0;
    let mut i = 0;
    while let Some(&byte) = bytes.get(i) {
        if let Some(end) = literal_end(code, i) {
            i = end;
            continue;
        }
        if is_word_byte(byte) && starts_name(code, i) {
            let len = bytes[i..].iter().take_while(|&&b| is_word_byte(b)).count();
            if &code[i..i + len] == from {
                out.push_str(&code[copied..i]);
                out.push_str(to);
                copied = i + len;
            }
            i += len;
        } else {
            i += 1;
        }
    }
    out.push_str(&code[copied..]);
    out
}

pub(crate) fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

/// Whether a name that is not a property (`x.pm`) starts at `i`, an ASCII byte.
pub(crate) fn starts_name(code: &str, i: usize) -> bool {
    let before = code.get(..i).unwrap_or_default();
    !before
        .bytes()
        .next_back()
        .is_some_and(|b| is_word_byte(b) || !b.is_ascii())
        && !before.trim_end().ends_with('.')
}

/// The length of `name` when `s` starts with it as a whole word.
pub(crate) fn word(s: &str, name: &str) -> Option<usize> {
    let rest = s.strip_prefix(name)?;
    (!rest.bytes().next().is_some_and(is_word_byte)).then_some(name.len())
}

fn skip_space(s: &str, i: usize) -> usize {
    s.get(i..)
        .map_or(i, |rest| i + (rest.len() - rest.trim_start().len()))
}

/// `.name` at `i`, with optional spaces: where it ends and the name.
pub(crate) fn member(s: &str, i: usize) -> Option<(usize, &str)> {
    let i = skip_space(s, i);
    let rest = s.get(i..)?.strip_prefix('.')?;
    let start = i + 1 + (rest.len() - rest.trim_start().len());
    let name = s.get(start..)?;
    let len = name.bytes().take_while(|&b| is_word_byte(b)).count();
    (len > 0).then(|| (start + len, &name[..len]))
}

/// `( … )` at `i`, with optional spaces: where it ends and the arguments, translated
/// with `at`.
pub(crate) fn call_args(s: &str, i: usize, at: Translate) -> Option<(usize, Vec<String>)> {
    let open = skip_space(s, i);
    if s.as_bytes().get(open) != Some(&b'(') {
        return None;
    }
    let end = group_end(s, open)?;
    let args = split_args(&s[open + 1..end - 1])
        .into_iter()
        .map(|arg| rewrite(arg, at))
        .collect();
    Some((end, args))
}

/// After `()` at `i`.
pub(crate) fn empty_call(s: &str, i: usize) -> Option<usize> {
    let open = skip_space(s, i);
    if s.as_bytes().get(open) != Some(&b'(') {
        return None;
    }
    let end = group_end(s, open)?;
    s.get(open + 1..end - 1)?.trim().is_empty().then_some(end)
}

/// After the bracket that closes the one at `open`.
fn group_end(s: &str, open: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut depth = 0usize;
    let mut i = open;
    while let Some(&byte) = bytes.get(i) {
        if let Some(end) = literal_end(s, i) {
            i = end;
            continue;
        }
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// The arguments between a call's parentheses, trimmed.
fn split_args(inner: &str) -> Vec<&str> {
    let bytes = inner.as_bytes();
    let mut args = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    let mut i = 0;
    while let Some(&byte) = bytes.get(i) {
        if let Some(end) = literal_end(inner, i) {
            i = end;
            continue;
        }
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b',' if depth == 0 => {
                args.push(inner[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    let last = inner[start..].trim();
    if !last.is_empty() {
        args.push(last);
    }
    args
}

/// When a string or comment starts at `i`: where it ends.
pub(crate) fn literal_end(s: &str, i: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    match (bytes.get(i)?, bytes.get(i + 1)) {
        (b'/', Some(b'/')) => Some(s[i..].find('\n').map_or(s.len(), |n| i + n)),
        (b'/', Some(b'*')) => Some(s[i + 2..].find("*/").map_or(s.len(), |n| i + 2 + n + 2)),
        (&quote @ (b'"' | b'\'' | b'`'), _) => {
            let mut j = i + 1;
            while let Some(&b) = bytes.get(j) {
                match b {
                    b'\\' => j += 2,
                    _ if b == quote => return Some(j + 1),
                    _ => j += 1,
                }
            }
            Some(s.len())
        }
        _ => None,
    }
}

pub(crate) fn is_string_literal(s: &str) -> bool {
    let bytes = s.as_bytes();
    match (bytes.first(), bytes.last()) {
        (Some(&first @ (b'"' | b'\'')), Some(&last)) => {
            bytes.len() >= 2 && last == first && literal_end(s, 0) == Some(s.len())
        }
        _ => false,
    }
}

/// `expr` in parentheses unless it is a plain name or property path.
pub(crate) fn grouped(expr: &str) -> String {
    let plain = !expr.is_empty()
        && expr
            .bytes()
            .all(|b| is_word_byte(b) || matches!(b, b'.' | b'[' | b']' | b'(' | b')'));
    if plain {
        expr.to_owned()
    } else {
        format!("({expr})")
    }
}

/// Whether code outside strings and comments uses one of `names` (not as a property).
pub(crate) fn uses_any(code: &str, names: &[&str]) -> bool {
    let bytes = code.as_bytes();
    let mut i = 0;
    while let Some(&byte) = bytes.get(i) {
        if let Some(end) = literal_end(code, i) {
            i = end;
            continue;
        }
        if is_word_byte(byte) && starts_name(code, i) {
            let len = bytes[i..].iter().take_while(|&&b| is_word_byte(b)).count();
            if names.contains(&&code[i..i + len]) {
                return true;
            }
            i += len;
        } else {
            i += 1;
        }
    }
    false
}
