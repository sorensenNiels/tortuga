//! What to colour in a JSON or XML body (R-HTTP-24). The scanners are lenient: they
//! never fail, so half-typed, truncated or `{{variable}}`-filled text still gets colour.

use std::ops::Range;

use crate::vars;

/// The language a body is coloured as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Json,
    Xml,
}

/// A part of a body that gets a colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Token {
    /// A JSON object key, or an XML element name.
    Key,
    /// A JSON string value, an XML attribute value or CDATA section.
    String,
    Number,
    /// `true`, `false` and `null` in JSON; entities such as `&amp;` in XML.
    Literal,
    /// Brackets, commas and colons in JSON; `<`, `>`, `/` and `=` in XML.
    Punctuation,
    /// An XML attribute name.
    Attribute,
    /// XML comments, declarations (`<?xml …?>`) and `<!DOCTYPE …>`.
    Comment,
    /// A `{{variable}}`.
    Variable,
}

/// Byte ranges of the coloured parts of `text`, in order and on character boundaries.
pub fn spans(language: Language, text: &str) -> Vec<(Range<usize>, Token)> {
    let spans = match language {
        Language::Json => json(text),
        Language::Xml => xml(text),
    };
    with_variables(spans, text)
}

fn json(text: &str) -> Vec<(Range<usize>, Token)> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(&b) = bytes.get(i) {
        let start = i;
        match b {
            b'"' => {
                i = string_end(bytes, i);
                let next = bytes
                    .get(i..)
                    .and_then(|rest| rest.iter().find(|c| !c.is_ascii_whitespace()));
                let token = if next == Some(&b':') {
                    Token::Key
                } else {
                    Token::String
                };
                out.push((start..i, token));
            }
            b'-' | b'0'..=b'9' => {
                i += 1;
                while bytes.get(i).is_some_and(|c| {
                    c.is_ascii_digit() || matches!(c, b'.' | b'e' | b'E' | b'+' | b'-')
                }) {
                    i += 1;
                }
                out.push((start..i, Token::Number));
            }
            b'{' | b'}' | b'[' | b']' | b',' | b':' => {
                i += 1;
                out.push((start..i, Token::Punctuation));
            }
            c if c.is_ascii_alphabetic() || c == b'_' => {
                while bytes
                    .get(i)
                    .is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_' || !c.is_ascii())
                {
                    i += 1;
                }
                if matches!(&text[start..i], "true" | "false" | "null") {
                    out.push((start..i, Token::Literal));
                }
            }
            _ => i += 1,
        }
    }
    out
}

/// Where the JSON string starting at `start` (its `"`) ends: after its closing quote, or at
/// the end of its line if it has none.
fn string_end(bytes: &[u8], start: usize) -> usize {
    let mut i = start + 1;
    while let Some(&b) = bytes.get(i) {
        match b {
            b'"' => return i + 1,
            b'\n' => return i,
            // The escaped character is skipped, unless it ends the line.
            b'\\' if bytes.get(i + 1).is_some_and(|&c| c != b'\n') => i += 2,
            _ => i += 1,
        }
    }
    bytes.len()
}

fn xml(text: &str) -> Vec<(Range<usize>, Token)> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(&b) = bytes.get(i) {
        let start = i;
        // Byte by byte: `i` may be inside a character here.
        let rest = &bytes[i..];
        if rest.starts_with(b"<!--") {
            i = until(text, i + 4, "-->");
            out.push((start..i, Token::Comment));
        } else if rest.starts_with(b"<![CDATA[") {
            i = until(text, i + 9, "]]>");
            out.push((start..i, Token::String));
        } else if rest.starts_with(b"<?") {
            i = until(text, i + 2, "?>");
            out.push((start..i, Token::Comment));
        } else if rest.starts_with(b"<!") {
            i = until(text, i + 2, ">");
            out.push((start..i, Token::Comment));
        } else if b == b'<' {
            i = tag(bytes, i, &mut out);
        } else if b == b'&' {
            i = entity_end(bytes, i).unwrap_or(i + 1);
            if i > start + 1 {
                out.push((start..i, Token::Literal));
            }
        } else {
            i += 1;
        }
    }
    out
}

/// The end of `end` searched from `from`, or the end of the text if it isn't there.
fn until(text: &str, from: usize, end: &str) -> usize {
    text.get(from..)
        .and_then(|rest| rest.find(end))
        .map_or(text.len(), |at| from + at + end.len())
}

/// Colours the tag starting at `start` (its `<`) and returns where it ends.
fn tag(bytes: &[u8], start: usize, out: &mut Vec<(Range<usize>, Token)>) -> usize {
    let mut i = start + 1;
    if bytes.get(i) == Some(&b'/') {
        i += 1;
    }
    out.push((start..i, Token::Punctuation));
    let name = name_end(bytes, i);
    if name > i {
        out.push((i..name, Token::Key));
    }
    i = name;
    while let Some(&b) = bytes.get(i) {
        let at = i;
        match b {
            b'>' => {
                out.push((at..i + 1, Token::Punctuation));
                return i + 1;
            }
            b'/' if bytes.get(i + 1) == Some(&b'>') => {
                out.push((at..i + 2, Token::Punctuation));
                return i + 2;
            }
            // A new tag before this one closed: leave it to the caller.
            b'<' => return i,
            b'=' | b'/' => {
                i += 1;
                out.push((at..i, Token::Punctuation));
            }
            b'"' | b'\'' => {
                i = bytes
                    .get(i + 1..)
                    .and_then(|rest| rest.iter().position(|&c| c == b))
                    .map_or(bytes.len(), |p| i + 1 + p + 1);
                out.push((at..i, Token::String));
            }
            c if c.is_ascii_whitespace() => i += 1,
            _ => {
                i = name_end(bytes, i).max(i + 1);
                out.push((at..i, Token::Attribute));
            }
        }
    }
    i
}

/// Where the name starting at `start` ends. Names end at ASCII delimiters, so the end is
/// on a character boundary.
fn name_end(bytes: &[u8], start: usize) -> usize {
    let mut i = start;
    while bytes.get(i).is_some_and(|&c| {
        !c.is_ascii_whitespace() && !matches!(c, b'<' | b'>' | b'/' | b'=' | b'"' | b'\'')
    }) {
        i += 1;
    }
    i
}

/// The end of an entity such as `&amp;` or `&#x20;` starting at `start`, if one is there.
fn entity_end(bytes: &[u8], start: usize) -> Option<usize> {
    let body = bytes.get(start + 1..)?;
    let len = body.iter().take(12).position(|&c| c == b';')?;
    let name = &body[..len];
    (len > 0 && name.iter().all(|c| c.is_ascii_alphanumeric() || *c == b'#'))
        .then_some(start + 1 + len + 1)
}

/// `spans` with every `{{variable}}` in `text` as its own span, cutting the parts it lies in.
fn with_variables(spans: Vec<(Range<usize>, Token)>, text: &str) -> Vec<(Range<usize>, Token)> {
    let found = vars::spans(text);
    if found.is_empty() {
        return spans;
    }
    let mut out = Vec::with_capacity(spans.len() + found.len() * 2);
    let mut vars = found.into_iter().map(|(range, _)| range).peekable();
    for (range, token) in spans {
        let mut at = range.start;
        while let Some(var) = vars.peek().filter(|v| v.start < range.end).cloned() {
            if var.start > at {
                out.push((at..var.start, token));
            }
            if out.last().is_none_or(|(r, _)| r.end <= var.start) {
                out.push((var.clone(), Token::Variable));
            }
            at = at.max(var.end);
            if var.end > range.end {
                break;
            }
            vars.next();
        }
        if at < range.end {
            out.push((at..range.end, token));
        }
    }
    for var in vars {
        if out.last().is_none_or(|(r, _)| r.end <= var.start) {
            out.push((var, Token::Variable));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coloured(language: Language, text: &str) -> Vec<(&str, Token)> {
        spans(language, text)
            .into_iter()
            .map(|(r, t)| (&text[r], t))
            .collect()
    }

    /// R-HTTP-24: keys, strings, numbers and literals each get their colour.
    #[test]
    fn json_keys_values_and_punctuation_are_told_apart() {
        use Token::{Key, Literal, Number, Punctuation, String};
        let text = "{\"name\" : \"ü😀\", \"n\": -1.5e3, \"ok\": [true, null]}";
        assert_eq!(
            coloured(Language::Json, text),
            [
                ("{", Punctuation),
                ("\"name\"", Key),
                (":", Punctuation),
                ("\"ü😀\"", String),
                (",", Punctuation),
                ("\"n\"", Key),
                (":", Punctuation),
                ("-1.5e3", Number),
                (",", Punctuation),
                ("\"ok\"", Key),
                (":", Punctuation),
                ("[", Punctuation),
                ("true", Literal),
                (",", Punctuation),
                ("null", Literal),
                ("]", Punctuation),
                ("}", Punctuation),
            ]
        );
    }

    /// R-HTTP-24: escapes, unclosed strings and words that are not literals.
    #[test]
    fn json_strings_with_escapes_and_broken_text() {
        assert_eq!(
            coloured(Language::Json, r#""a\"b" "x"#),
            [(r#""a\"b""#, Token::String), ("\"x", Token::String)]
        );
        // An unclosed string ends at its line, so the next line is still coloured.
        assert_eq!(
            coloured(Language::Json, "\"open\n1"),
            [("\"open", Token::String), ("1", Token::Number)]
        );
        assert_eq!(coloured(Language::Json, "nullish trueü"), []);
        assert_eq!(coloured(Language::Json, ""), []);
        assert_eq!(coloured(Language::Json, "\"\\"), [("\"\\", Token::String)]);
    }

    /// R-HTTP-24: tags, attributes, comments, CDATA and entities.
    #[test]
    fn xml_tags_attributes_and_comments() {
        use Token::{Attribute, Comment, Key, Literal, Punctuation, String};
        let text = "<?xml version=\"1.0\"?><!-- ü --><a:b id='7' x=\"ü😀\">t &amp; <![CDATA[<c>]]></a:b><e/>";
        assert_eq!(
            coloured(Language::Xml, text),
            [
                ("<?xml version=\"1.0\"?>", Comment),
                ("<!-- ü -->", Comment),
                ("<", Punctuation),
                ("a:b", Key),
                ("id", Attribute),
                ("=", Punctuation),
                ("'7'", String),
                ("x", Attribute),
                ("=", Punctuation),
                ("\"ü😀\"", String),
                (">", Punctuation),
                ("&amp;", Literal),
                ("<![CDATA[<c>]]>", String),
                ("</", Punctuation),
                ("a:b", Key),
                (">", Punctuation),
                ("<", Punctuation),
                ("e", Key),
                ("/>", Punctuation),
            ]
        );
    }

    /// R-HTTP-24: half-typed and truncated XML never loses the rest of the text.
    #[test]
    fn xml_broken_text_is_coloured_as_far_as_it_goes() {
        use Token::{Comment, Key, Punctuation, String};
        assert_eq!(
            coloured(Language::Xml, "<a <b>"),
            [
                ("<", Punctuation),
                ("a", Key),
                ("<", Punctuation),
                ("b", Key),
                (">", Punctuation)
            ]
        );
        assert_eq!(
            coloured(Language::Xml, "<a x=\"open"),
            [
                ("<", Punctuation),
                ("a", Key),
                ("x", Token::Attribute),
                ("=", Punctuation),
                ("\"open", String)
            ]
        );
        assert_eq!(
            coloured(Language::Xml, "<!-- open"),
            [("<!-- open", Comment)]
        );
        assert_eq!(
            coloured(Language::Xml, "a & b &#x20; &;"),
            [("&#x20;", Token::Literal)]
        );
        assert_eq!(coloured(Language::Xml, "<"), [("<", Punctuation)]);
        assert_eq!(coloured(Language::Xml, ""), []);
    }

    /// R-HTTP-24: a `{{variable}}` is coloured as one, inside or outside strings.
    #[test]
    fn variables_cut_the_parts_they_are_in() {
        use Token::{Key, Punctuation, String, Variable};
        let text = "{\"id\": {{id}}, \"n\": \"a {{ name }} b\"}";
        assert_eq!(
            coloured(Language::Json, text),
            [
                ("{", Punctuation),
                ("\"id\"", Key),
                (":", Punctuation),
                ("{{id}}", Variable),
                (",", Punctuation),
                ("\"n\"", Key),
                (":", Punctuation),
                ("\"a ", String),
                ("{{ name }}", Variable),
                (" b\"", String),
                ("}", Punctuation),
            ]
        );
        assert_eq!(
            coloured(Language::Xml, "<id>{{id}}</id>"),
            [
                ("<", Punctuation),
                ("id", Key),
                (">", Punctuation),
                ("{{id}}", Variable),
                ("</", Punctuation),
                ("id", Key),
                (">", Punctuation),
            ]
        );
        assert_eq!(
            coloured(Language::Json, "{{a}}{{b}}"),
            [("{{a}}", Variable), ("{{b}}", Variable)]
        );
    }

    /// R-HTTP-24: spans are in order and on character boundaries for any text.
    #[test]
    fn spans_are_ordered_and_on_character_boundaries() {
        let texts = [
            "{\"ü😀\": [1, \"{{x}}\", {{y}}] , \"\\ü\"",
            "<ü a=\"😀\">&ü; <!-- {{c}} --> {{ open",
            &"{\"k\": \"ü\"}".repeat(1000),
        ];
        for text in texts {
            for language in [Language::Json, Language::Xml] {
                let mut at = 0;
                for (r, _) in spans(language, text) {
                    assert!(r.start >= at && r.start < r.end, "{r:?} in {text}");
                    assert!(text.is_char_boundary(r.start) && text.is_char_boundary(r.end));
                    at = r.end;
                }
            }
        }
    }
}
