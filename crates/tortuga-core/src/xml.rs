//! XML bodies (R-HTTP-19): checking and formatting what the user types, pretty-printing
//! responses, and the tree the `XPath` filter and captures work on ([`crate::xpath`]).
//!
//! Parsing checks what makes a document unreadable: tags that don't match or aren't
//! closed, broken attributes, unknown entities, a second root element and text outside
//! it. It does not validate against a DTD or schema.

use quick_xml::Reader;
use quick_xml::events::{BytesRef, Event};

/// Escapes `text` for element text or a quoted attribute value. Control characters
/// other than newlines are dropped: XML 1.0 cannot hold them.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\n' => out.push_str("&#10;"),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

/// Returns a human-readable parse error, or `None` if `text` is well-formed XML.
/// Blank text counts as valid (nothing to send yet).
pub fn check(text: &str) -> Option<String> {
    if text.trim().is_empty() {
        return None;
    }
    Document::parse(text).err()
}

/// Pretty-prints `text` with two-space indents. Elements that hold only text stay on
/// one line; comments, the declaration and CDATA are kept as written. `None` if it is
/// not well-formed XML.
pub fn pretty(text: &str) -> Option<String> {
    Document::parse(text).ok().map(|doc| doc.pretty())
}

/// A parsed document. Nodes are stored in document order; the document itself is node 0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    pub(crate) nodes: Vec<Node>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Node {
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub kind: Kind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Kind {
    Document,
    Element(Element),
    /// Text and CDATA, with entities resolved; `raw` is the source as written.
    Text {
        value: String,
        raw: String,
    },
    /// A comment, processing instruction, declaration or DOCTYPE, as written.
    Markup(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Element {
    /// The name as written, with its prefix (`soap:Body`).
    pub name: String,
    pub attributes: Vec<(String, String)>,
    /// The start tag as written (`<a id="1">` or `<a/>`).
    pub start_tag: String,
    /// Written as `<a/>`.
    pub empty: bool,
}

/// `name` without its namespace prefix.
pub(crate) fn local(name: &str) -> &str {
    name.rsplit_once(':').map_or(name, |(_, local)| local)
}

impl Document {
    /// Parses `text`. The error says what is wrong and where.
    pub fn parse(text: &str) -> Result<Self, String> {
        Parser::new(text).run()
    }

    /// The text of `node` and everything below it, as `XPath`'s `string()` gives it.
    pub(crate) fn string_value(&self, node: usize) -> String {
        match &self.nodes[node].kind {
            Kind::Text { value, .. } => value.clone(),
            Kind::Markup(_) => String::new(),
            Kind::Document | Kind::Element(_) => {
                let mut out = String::new();
                self.collect_text(node, &mut out);
                out
            }
        }
    }

    fn collect_text(&self, node: usize, out: &mut String) {
        for &child in &self.nodes[node].children {
            match &self.nodes[child].kind {
                Kind::Text { value, .. } => out.push_str(value),
                Kind::Element(_) => self.collect_text(child, out),
                Kind::Document | Kind::Markup(_) => {}
            }
        }
    }

    /// The whole document, pretty-printed.
    pub fn pretty(&self) -> String {
        self.pretty_node(0)
    }

    /// `node` and what is below it, pretty-printed.
    pub(crate) fn pretty_node(&self, node: usize) -> String {
        let mut out = String::new();
        self.write(node, 0, &mut out);
        // Every line ends with a newline as it is written; the last one is not needed.
        if out.ends_with('\n') {
            out.pop();
        }
        out
    }

    fn write(&self, node: usize, depth: usize, out: &mut String) {
        match &self.nodes[node].kind {
            Kind::Document => {
                for &child in &self.nodes[node].children {
                    self.write(child, depth, out);
                }
            }
            Kind::Text { raw, .. } => {
                let raw = raw.trim();
                if !raw.is_empty() {
                    line(out, depth, &[raw]);
                }
            }
            Kind::Markup(raw) => line(out, depth, &[raw]),
            Kind::Element(element) => self.write_element(node, element, depth, out),
        }
    }

    fn write_element(&self, node: usize, element: &Element, depth: usize, out: &mut String) {
        let start = &element.start_tag;
        let end = format!("</{}>", element.name);
        let children = &self.nodes[node].children;
        if element.empty {
            line(out, depth, &[start]);
            return;
        }
        let only_text = children
            .iter()
            .all(|&c| matches!(self.nodes[c].kind, Kind::Text { .. }));
        if only_text {
            let raw: String = children
                .iter()
                .filter_map(|&c| match &self.nodes[c].kind {
                    Kind::Text { raw, .. } => Some(raw.as_str()),
                    Kind::Document | Kind::Element(_) | Kind::Markup(_) => None,
                })
                .collect();
            // Only whitespace inside is dropped; other text is kept exactly.
            let raw = if raw.trim().is_empty() { "" } else { &raw };
            line(out, depth, &[start, raw, &end]);
            return;
        }
        line(out, depth, &[start]);
        for &child in children {
            self.write(child, depth + 1, out);
        }
        line(out, depth, &[&end]);
    }
}

/// Writes `pieces` as one line, indented two spaces per `depth`.
fn line(out: &mut String, depth: usize, pieces: &[&str]) {
    for _ in 0..depth {
        out.push_str("  ");
    }
    out.extend(pieces.iter().copied());
    out.push('\n');
}

struct Parser<'a> {
    text: &'a str,
    nodes: Vec<Node>,
    /// The open elements, innermost last; the document at the bottom.
    open: Vec<usize>,
    /// The root element has been closed.
    root_done: bool,
    /// A DOCTYPE may declare entities this parser does not read: keep them as written.
    has_doctype: bool,
}

impl<'a> Parser<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            text,
            nodes: vec![Node {
                parent: None,
                children: Vec::new(),
                kind: Kind::Document,
            }],
            open: vec![0],
            root_done: false,
            has_doctype: false,
        }
    }

    fn run(mut self) -> Result<Document, String> {
        let mut reader = Reader::from_str(self.text);
        reader.config_mut().enable_all_checks(true);
        loop {
            let start = offset(reader.buffer_position());
            let event = reader
                .read_event()
                .map_err(|e| self.error(offset(reader.error_position()), &message(&e)))?;
            let end = offset(reader.buffer_position());
            let source = self.text.get(start..end).unwrap_or_default();
            match event {
                Event::Start(tag) | Event::Empty(tag) => {
                    let empty = source.ends_with("/>");
                    self.element(start, &tag, source, empty)?;
                }
                Event::End(_) => {
                    self.open.pop();
                    if self.open.len() == 1 {
                        self.root_done = true;
                    }
                }
                Event::Text(text) => {
                    let value = text
                        .xml10_content()
                        .map_err(|e| self.error(start, &e.to_string()))?;
                    self.text_node(start, &value, source)?;
                }
                Event::CData(data) => {
                    let value = data
                        .xml10_content()
                        .map_err(|e| self.error(start, &e.to_string()))?;
                    self.text_node(start, &value, source)?;
                }
                Event::GeneralRef(entity) => {
                    let value = self.entity(start, &entity)?;
                    self.text_node(start, &value, source)?;
                }
                Event::DocType(_) => {
                    self.has_doctype = true;
                    self.push(Kind::Markup(source.trim().to_owned()));
                }
                Event::Comment(_) | Event::Decl(_) | Event::PI(_) => {
                    self.push(Kind::Markup(source.trim().to_owned()));
                }
                Event::Eof => break,
            }
        }
        if let Some(&open) = self.open.get(1..).and_then(<[usize]>::last)
            && let Kind::Element(element) = &self.nodes[open].kind
        {
            let name = element.name.clone();
            return Err(self.error(self.text.len(), &format!("<{name}> is not closed")));
        }
        if !self.root_done {
            return Err(self.error(self.text.len(), "there is no root element"));
        }
        Ok(Document { nodes: self.nodes })
    }

    fn element(
        &mut self,
        at: usize,
        tag: &quick_xml::events::BytesStart<'_>,
        source: &str,
        empty: bool,
    ) -> Result<(), String> {
        if self.root_done {
            return Err(self.error(at, "there is more than one root element"));
        }
        let name = String::from_utf8_lossy(tag.name().as_ref()).into_owned();
        let mut attributes = Vec::new();
        for attribute in tag.attributes() {
            let attribute = attribute.map_err(|e| self.error(at, &e.to_string()))?;
            let key = String::from_utf8_lossy(attribute.key.as_ref()).into_owned();
            let value = attribute
                .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                .map_err(|e| self.error(at, &message(&e)))?;
            attributes.push((key, value.into_owned()));
        }
        let node = self.push(Kind::Element(Element {
            name,
            attributes,
            start_tag: source.to_owned(),
            empty,
        }));
        if empty {
            if self.open.len() == 1 {
                self.root_done = true;
            }
        } else {
            self.open.push(node);
        }
        Ok(())
    }

    /// Adds text to the open element, joining it with text just before it (text,
    /// entities and CDATA next to each other are one text node).
    fn text_node(&mut self, at: usize, value: &str, source: &str) -> Result<(), String> {
        if self.open.len() == 1 {
            if value.trim().is_empty() {
                return Ok(());
            }
            let what = if self.root_done { "after" } else { "before" };
            return Err(self.error(at, &format!("there is text {what} the root element")));
        }
        let parent = self.current();
        if let Some(&last) = self.nodes[parent].children.last()
            && let Kind::Text { value: v, raw } = &mut self.nodes[last].kind
        {
            v.push_str(value);
            raw.push_str(source);
            return Ok(());
        }
        self.push(Kind::Text {
            value: value.to_owned(),
            raw: source.to_owned(),
        });
        Ok(())
    }

    fn entity(&self, at: usize, entity: &BytesRef<'_>) -> Result<String, String> {
        if entity.is_char_ref() {
            return match entity.resolve_char_ref() {
                Ok(Some(c)) => Ok(c.to_string()),
                Ok(None) | Err(_) => Err(self.error(at, "invalid character reference")),
            };
        }
        let name = entity
            .decode()
            .map_err(|e| self.error(at, &e.to_string()))?;
        if let Some(value) = quick_xml::escape::resolve_predefined_entity(&name) {
            return Ok(value.to_owned());
        }
        if self.has_doctype {
            return Ok(format!("&{name};"));
        }
        Err(self.error(at, &format!("unknown entity &{name};")))
    }

    fn current(&self) -> usize {
        *self.open.last().expect("the document is always open")
    }

    fn push(&mut self, kind: Kind) -> usize {
        let parent = self.current();
        let id = self.nodes.len();
        self.nodes.push(Node {
            parent: Some(parent),
            children: Vec::new(),
            kind,
        });
        self.nodes[parent].children.push(id);
        id
    }

    /// "Invalid XML at line L, column C: what".
    fn error(&self, at: usize, what: &str) -> String {
        let (line, column) = line_column(self.text, at);
        format!("Invalid XML at line {line}, column {column}: {what}")
    }
}

/// quick-xml's message without its category prefix.
fn message(e: &quick_xml::Error) -> String {
    match e {
        quick_xml::Error::Syntax(e) => e.to_string(),
        quick_xml::Error::IllFormed(e) => e.to_string(),
        quick_xml::Error::InvalidAttr(e) => e.to_string(),
        quick_xml::Error::Io(_)
        | quick_xml::Error::Encoding(_)
        | quick_xml::Error::Escape(_)
        | quick_xml::Error::Namespace(_) => e.to_string(),
    }
}

fn offset(position: u64) -> usize {
    usize::try_from(position).unwrap_or(usize::MAX)
}

/// 1-based line and column (in characters) of byte offset `at`.
fn line_column(text: &str, at: usize) -> (usize, usize) {
    let mut line = 1;
    let mut column = 1;
    for (i, c) in text.char_indices() {
        if i >= at {
            break;
        }
        if c == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    (line, column)
}

#[cfg(test)]
mod tests {
    use super::*;

    impl Document {
        fn root(&self) -> Option<usize> {
            self.nodes[0]
                .children
                .iter()
                .copied()
                .find(|&c| matches!(self.nodes[c].kind, Kind::Element(_)))
        }
    }

    #[test]
    fn blank_text_is_valid() {
        assert_eq!(check(""), None);
        assert_eq!(check("  \n"), None);
    }

    #[test]
    fn well_formed_documents_pass_the_check() {
        for ok in [
            "<a/>",
            "<?xml version=\"1.0\"?>\n<!-- hi --><a x='1'><b>t &amp; &#252;</b><![CDATA[<x>]]></a>\n",
            "<soap:Envelope xmlns:soap=\"http://schemas.xmlsoap.org/soap/envelope/\"><soap:Body/></soap:Envelope>",
            "<!DOCTYPE a [<!ENTITY e \"x\">]><a>&e;</a>",
            "<a>ü😀</a>",
        ] {
            assert_eq!(check(ok), None, "{ok}");
        }
    }

    #[test]
    fn errors_say_what_is_wrong_and_where() {
        let err = check("<a>\n  <b></c>\n</a>").expect("error");
        assert!(err.starts_with("Invalid XML at line 2, column"), "{err}");
        assert!(err.contains("</b>"), "{err}");
        for (bad, says) in [
            ("<a>", "<a> is not closed"),
            ("<a/><b/>", "more than one root element"),
            ("<a/>tail", "text after the root element"),
            ("text", "text before the root element"),
            ("<!-- only -->", "no root element"),
            ("<a>&nbsp;</a>", "unknown entity &nbsp;"),
            ("<a x=1/>", "Invalid XML"),
            ("<a x='1' x='2'/>", "Invalid XML"),
        ] {
            let err = check(bad).unwrap_or_else(|| panic!("{bad} should fail"));
            assert!(err.contains(says), "{bad}: {err}");
        }
    }

    #[test]
    fn error_columns_count_characters() {
        let err = check("<a>ü😀</b>").expect("error");
        assert!(err.contains("line 1, column 6"), "{err}");
    }

    #[test]
    fn pretty_printing_indents_elements_and_keeps_text_inline() {
        let text = "<?xml version=\"1.0\"?><order id=\"7\"><!-- note --><item>ü &amp; 😀</item><empty/><none></none><b><c>1</c></b></order>";
        assert_eq!(
            pretty(text).expect("pretty"),
            "<?xml version=\"1.0\"?>\n<order id=\"7\">\n  <!-- note -->\n  <item>ü &amp; 😀</item>\n  <empty/>\n  <none></none>\n  <b>\n    <c>1</c>\n  </b>\n</order>"
        );
    }

    #[test]
    fn pretty_printing_replaces_existing_indentation() {
        let text = "<a>\n\t\t<b>x</b>\n      <c>\n  </c>\n</a>\n";
        assert_eq!(
            pretty(text).expect("pretty"),
            "<a>\n  <b>x</b>\n  <c></c>\n</a>"
        );
        assert_eq!(pretty(&pretty(text).expect("once")), pretty(text));
    }

    #[test]
    fn pretty_printing_keeps_cdata_as_written() {
        assert_eq!(
            pretty("<a><b><![CDATA[x < y]]></b></a>").expect("pretty"),
            "<a>\n  <b><![CDATA[x < y]]></b>\n</a>"
        );
    }

    #[test]
    fn invalid_xml_is_not_pretty_printed() {
        assert_eq!(pretty("<a>"), None);
        assert_eq!(pretty("{\"json\": true}"), None);
    }

    #[test]
    fn text_values_resolve_entities_and_join_cdata() {
        let doc = Document::parse("<a>x &lt; <![CDATA[y]]>&#33;<b>z</b></a>").expect("parse");
        let root = doc.root().expect("root");
        assert_eq!(doc.string_value(root), "x < y!z");
    }

    #[test]
    fn very_long_documents_parse() {
        let items = "<i>ü</i>".repeat(20_000);
        let doc = Document::parse(&format!("<r>{items}</r>")).expect("parse");
        assert_eq!(doc.nodes[doc.root().expect("root")].children.len(), 20_000);
    }
}
