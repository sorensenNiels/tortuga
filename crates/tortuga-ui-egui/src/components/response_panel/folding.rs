//! Folding the objects and arrays of formatted JSON, and the elements of formatted XML
//! (R-HTTP-26). Folds are found from the formatted text's lines and indents, so they work
//! on the filter's result and on a truncated body too: a section whose end is cut off has no fold.

use std::collections::BTreeSet;
use std::ops::Range;

use super::Syntax;

/// A section that can be folded: the line that opens it up to the line that closes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Fold {
    /// Where the opening line starts: the fold's id, and where its toggle is drawn.
    pub start: usize,
    /// Where the opening line ends (before its line break).
    pub open_end: usize,
    /// Where the closing line ends: everything after `open_end` up to here is hidden.
    pub end: usize,
    /// The closing line without its indent (`}`, `],` or `</a>`), shown after a folded line.
    pub closing: Range<usize>,
    /// Lines inside, without the opening and closing line.
    pub lines: usize,
}

impl Fold {
    /// The bytes hidden while folded.
    fn hidden(&self) -> Range<usize> {
        self.open_end + 1..self.end
    }
}

/// Every section of `text` that can be folded, in order of their start.
pub(super) fn folds(text: &str, syntax: Syntax) -> Vec<Fold> {
    if syntax == Syntax::Plain {
        return Vec::new();
    }
    // Opening lines not closed yet: (fold so far, indent, line number).
    let mut open: Vec<(Fold, usize, usize)> = Vec::new();
    let mut out = Vec::new();
    let mut start = 0;
    for (number, raw) in text.split('\n').enumerate() {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        let line_end = start + line.len();
        let content = line.trim_start_matches([' ', '\t']);
        let indent = line.len() - content.len();
        let content = content.trim_end();
        if !content.is_empty() {
            // A line at the same or a smaller indent ends what is open deeper.
            while let Some((_, open_indent, _)) = open.last()
                && *open_indent >= indent
            {
                let Some((mut fold, open_indent, opened_at)) = open.pop() else {
                    break;
                };
                if open_indent == indent && closes(content, syntax) && number > opened_at + 1 {
                    fold.end = line_end;
                    fold.closing = start + indent..start + indent + content.len();
                    fold.lines = number - opened_at - 1;
                    out.push(fold);
                }
            }
            if opens(content, syntax) {
                let fold = Fold {
                    start,
                    open_end: line_end,
                    end: line_end,
                    closing: 0..0,
                    lines: 0,
                };
                open.push((fold, indent, number));
            }
        }
        start += raw.len() + 1;
    }
    out.sort_by_key(|f| f.start);
    out
}

/// Whether a line (without its indent) opens a section.
fn opens(line: &str, syntax: Syntax) -> bool {
    match syntax {
        Syntax::Json => line.ends_with('{') || line.ends_with('['),
        // A start tag on its own: not an end tag, comment, declaration, empty element,
        // or an element with its text on the same line.
        Syntax::Xml => {
            line.starts_with('<')
                && !line.starts_with("</")
                && !line.starts_with("<?")
                && !line.starts_with("<!")
                && line.ends_with('>')
                && !line.ends_with("/>")
                && !line.contains("</")
        }
        Syntax::Plain => false,
    }
}

/// Whether a line (without its indent) closes a section.
fn closes(line: &str, syntax: Syntax) -> bool {
    match syntax {
        Syntax::Json => line.starts_with('}') || line.starts_with(']'),
        Syntax::Xml => line.starts_with("</"),
        Syntax::Plain => false,
    }
}

/// A list of rows: its address and length.
type RowsKey = (usize, usize);

/// The folded sections of one text, and the rows left to draw.
#[derive(Debug, Clone, Default)]
pub(super) struct Folded {
    /// The text's address and length: folds belong to one text.
    text: (usize, usize),
    /// The `start` of each folded section.
    starts: BTreeSet<usize>,
    /// The rows to draw, for the rows' address and length and `starts`.
    visible: Option<(RowsKey, Vec<Range<usize>>)>,
}

impl Folded {
    /// Forgets the folds of another text.
    pub fn follow(&mut self, text: &str) {
        let key = (text.as_ptr() as usize, text.len());
        if self.text != key {
            *self = Self {
                text: key,
                ..Self::default()
            };
        }
    }

    pub fn is_folded(&self, fold: &Fold) -> bool {
        self.starts.contains(&fold.start)
    }

    pub fn toggle(&mut self, fold: &Fold) {
        if !self.starts.remove(&fold.start) {
            self.starts.insert(fold.start);
        }
        self.visible = None;
    }

    pub fn set_all(&mut self, folds: &[Fold], folded: bool) {
        self.starts = if folded {
            folds.iter().map(|f| f.start).collect()
        } else {
            BTreeSet::new()
        };
        self.visible = None;
    }

    pub fn any(&self) -> bool {
        !self.starts.is_empty()
    }

    /// Unfolds every section that hides `at`, so a search match can be shown.
    pub fn reveal(&mut self, folds: &[Fold], at: usize) {
        let before = self.starts.len();
        for fold in folds.iter().filter(|f| f.hidden().contains(&at)) {
            self.starts.remove(&fold.start);
        }
        if self.starts.len() != before {
            self.visible = None;
        }
    }

    /// Works out which of `rows` [`Self::visible`] leaves out, if not done for these rows.
    pub fn prepare(&mut self, folds: &[Fold], rows: &[Range<usize>]) {
        let key = (rows.as_ptr() as usize, rows.len());
        if self.any() && self.visible.as_ref().is_none_or(|(k, _)| *k != key) {
            let hidden: Vec<Range<usize>> = folds
                .iter()
                .filter(|f| self.starts.contains(&f.start))
                .map(Fold::hidden)
                .collect();
            self.visible = Some((key, without(rows, &hidden)));
        }
    }

    /// `rows` without those inside folded sections, as [`Self::prepare`] found them.
    pub fn visible<'a>(&'a self, rows: &'a [Range<usize>]) -> &'a [Range<usize>] {
        let key = (rows.as_ptr() as usize, rows.len());
        match &self.visible {
            Some((k, v)) if self.any() && *k == key => v,
            _ => rows,
        }
    }
}

/// The rows that start outside every one of `hidden`. Both lists are in order of their start.
fn without(rows: &[Range<usize>], hidden: &[Range<usize>]) -> Vec<Range<usize>> {
    // Nested sections fall inside the outer one: merge them into ranges that don't overlap.
    let mut merged: Vec<Range<usize>> = Vec::with_capacity(hidden.len());
    for h in hidden {
        match merged.last_mut() {
            Some(last) if h.start <= last.end => last.end = last.end.max(h.end),
            _ => merged.push(h.clone()),
        }
    }
    let mut next = merged.iter().peekable();
    rows.iter()
        .filter(|row| {
            while next.next_if(|h| h.end <= row.start).is_some() {}
            !next.peek().is_some_and(|h| h.contains(&row.start))
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(text: &str) -> Vec<Range<usize>> {
        let mut out = Vec::new();
        let mut start = 0;
        for line in text.split('\n') {
            out.push(start..start + line.len());
            start += line.len() + 1;
        }
        out
    }

    fn shown<'a>(text: &'a str, rows: &[Range<usize>]) -> Vec<&'a str> {
        rows.iter().map(|r| &text[r.clone()]).collect()
    }

    const JSON: &str =
        "{\n  \"a\": [\n    1,\n    2\n  ],\n  \"b\": {},\n  \"ü😀\": {\n    \"c\": \"{\"\n  }\n}";

    /// R-HTTP-26: objects and arrays that span lines fold; empty ones and strings with braces don't.
    #[test]
    fn json_objects_and_arrays_fold_to_their_closing_line() {
        let folds = folds(JSON, Syntax::Json);
        let summary: Vec<(&str, &str, usize)> = folds
            .iter()
            .map(|f| {
                (
                    &JSON[f.start..f.open_end],
                    &JSON[f.closing.clone()],
                    f.lines,
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                ("{", "}", 8),
                ("  \"a\": [", "],", 2),
                ("  \"ü😀\": {", "}", 1)
            ]
        );
    }

    #[test]
    fn folded_sections_hide_their_rows_and_closing_line() {
        let folds = folds(JSON, Syntax::Json);
        let rows = lines(JSON);
        let mut folded = Folded::default();
        folded.follow(JSON);
        folded.toggle(&folds[1]);
        assert_eq!(
            shown(JSON, {
                folded.prepare(&folds, &rows);
                folded.visible(&rows)
            }),
            [
                "{",
                "  \"a\": [",
                "  \"b\": {},",
                "  \"ü😀\": {",
                "    \"c\": \"{\"",
                "  }",
                "}"
            ]
        );
        // Folding the outer section too hides everything but its first line.
        folded.toggle(&folds[0]);
        assert_eq!(
            shown(JSON, {
                folded.prepare(&folds, &rows);
                folded.visible(&rows)
            }),
            ["{"]
        );
        folded.toggle(&folds[0]);
        folded.toggle(&folds[1]);
        assert_eq!(
            {
                folded.prepare(&folds, &rows);
                folded.visible(&rows)
            }
            .len(),
            rows.len()
        );
    }

    #[test]
    fn revealing_a_match_unfolds_the_sections_around_it() {
        let folds = folds(JSON, Syntax::Json);
        let mut folded = Folded::default();
        folded.follow(JSON);
        folded.set_all(&folds, true);
        let at = JSON.find("\"c\"").expect("in the text");
        folded.reveal(&folds, at);
        assert!(!folded.is_folded(&folds[0]) && !folded.is_folded(&folds[2]));
        assert!(folded.is_folded(&folds[1]));
    }

    #[test]
    fn folds_are_forgotten_for_another_text() {
        let folds = folds(JSON, Syntax::Json);
        let mut folded = Folded::default();
        folded.follow(JSON);
        folded.set_all(&folds, true);
        let other = JSON.to_owned();
        folded.follow(&other);
        assert!(!folded.any());
    }

    #[test]
    fn xml_elements_with_children_fold() {
        let xml = "<?xml version=\"1.0\"?>\n<a id=\"1\">\n  <b>text</b>\n  <c/>\n  <!-- note -->\n  <d>\n    <e/>\n  </d>\n</a>";
        let found: Vec<(&str, &str)> = folds(xml, Syntax::Xml)
            .iter()
            .map(|f| (&xml[f.start..f.open_end], &xml[f.closing.clone()]))
            .collect();
        assert_eq!(found, [("<a id=\"1\">", "</a>"), ("  <d>", "</d>")]);
    }

    #[test]
    fn a_section_cut_off_by_truncation_or_plain_text_has_no_fold() {
        assert_eq!(folds("{\n  \"a\": [\n    1,", Syntax::Json).len(), 0);
        assert_eq!(folds("{\n  1\n}", Syntax::Plain).len(), 0);
        assert_eq!(folds("", Syntax::Json).len(), 0);
    }

    #[test]
    fn crlf_lines_fold_without_the_carriage_return() {
        let text = "{\r\n  \"a\": 1\r\n}";
        let folds = folds(text, Syntax::Json);
        assert_eq!(folds.len(), 1);
        assert_eq!(&text[folds[0].closing.clone()], "}");
        assert_eq!(folds[0].end, text.len());
    }

    /// Long lines are cut into several rows; all of a hidden line's rows are hidden.
    #[test]
    fn every_row_of_a_hidden_long_line_is_hidden() {
        let text = "[\n  \"abcdef\"\n]";
        let rows = vec![0..1, 2..7, 7..12, 13..14];
        let folds = folds(text, Syntax::Json);
        let mut folded = Folded::default();
        folded.follow(text);
        folded.toggle(&folds[0]);
        folded.prepare(&folds, &rows);
        assert_eq!(shown(text, folded.visible(&rows)), ["["]);
    }
}
