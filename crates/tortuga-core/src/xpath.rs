//! An `XPath` subset to filter XML responses and capture values from them (R-HTTP-19).
//!
//! Supported: absolute and relative location paths with `/` and `//`, element names
//! (`soap:Body` as written, or `Body` for any prefix), `*`, `@name`, `@*`, `text()`,
//! `node()`, `.` and `..`, and predicates `[n]`, `[last()]`, `[last()-n]`, `[path]` and
//! `[path <op> literal]` with `= != < <= > >=`, where `path` is a relative path such as
//! `@id`, `name` or `text()` and the literal a quoted string or a number.

use std::cmp::Ordering;

use crate::xml::{Document, Kind, local};

/// A parsed expression.
#[derive(Debug, Clone, PartialEq)]
pub struct XPath(Path);

/// What a path found: a node of the document, or an attribute of an element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Item {
    Node(usize),
    /// The element, and the attribute's position on it.
    Attribute(usize, usize),
}

impl Item {
    /// Sorts items in document order: an element, then its attributes, then its children.
    fn order(self) -> (usize, usize) {
        match self {
            Self::Node(n) => (n, 0),
            Self::Attribute(n, i) => (n, i + 1),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Path {
    /// Starts at the document (`/…`); otherwise at the context node.
    absolute: bool,
    steps: Vec<(Separator, Step)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Separator {
    /// `/`: children of the current nodes.
    Child,
    /// `//`: the current nodes and everything below them, then their children.
    Descendants,
}

#[derive(Debug, Clone, PartialEq)]
struct Step {
    test: Test,
    predicates: Vec<Predicate>,
}

#[derive(Debug, Clone, PartialEq)]
enum Test {
    /// `.`
    Current,
    /// `..`
    Parent,
    /// A child element by name; `None` is `*`.
    Element(Option<String>),
    /// An attribute by name; `None` is `@*`.
    Attribute(Option<String>),
    /// `text()`
    Text,
    /// `node()`: child elements and text.
    Node,
}

#[derive(Debug, Clone, PartialEq)]
enum Predicate {
    /// `[n]`, counted from 1.
    Position(usize),
    /// `[last()]` (0) or `[last()-n]`.
    FromLast(usize),
    /// `[path]` or `[path <op> literal]`: some node the path finds passes the test.
    Test(Path, Option<(Op, Literal)>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Debug, Clone, PartialEq)]
enum Literal {
    Text(String),
    Number(f64),
}

impl XPath {
    /// Parses `expr`. The error says what is wrong and where.
    pub fn parse(expr: &str) -> Result<Self, String> {
        let mut p = Parser {
            text: expr.trim(),
            at: 0,
        };
        if p.done() {
            return Err("the expression is empty".to_owned());
        }
        let path = p.path()?;
        if !p.done() {
            return Err(p.error("unexpected text"));
        }
        Ok(Self(path))
    }

    /// The matches in document order.
    pub fn select(&self, doc: &Document) -> Vec<Item> {
        evaluate(&self.0, doc, Item::Node(0))
    }
}

impl Document {
    /// The text of what a path found: an element's text and its descendants', an
    /// attribute's value.
    pub fn item_text(&self, item: Item) -> String {
        match item {
            Item::Node(n) => self.string_value(n),
            Item::Attribute(n, i) => self
                .attribute(n, i)
                .map(|(_, v)| v.clone())
                .unwrap_or_default(),
        }
    }

    /// What a path found, to show: elements pretty-printed, text and attribute values as is.
    pub fn item_xml(&self, item: Item) -> String {
        match item {
            Item::Node(n) => match &self.nodes[n].kind {
                Kind::Document | Kind::Element(_) | Kind::Markup(_) => self.pretty_node(n),
                Kind::Text { value, .. } => value.clone(),
            },
            Item::Attribute(..) => self.item_text(item),
        }
    }

    fn attribute(&self, node: usize, i: usize) -> Option<&(String, String)> {
        match &self.nodes.get(node)?.kind {
            Kind::Element(e) => e.attributes.get(i),
            Kind::Document | Kind::Text { .. } | Kind::Markup(_) => None,
        }
    }
}

/// Filters `doc` with `expr` (R-HTTP-19): every match, elements pretty-printed, one per
/// line or block.
pub fn query(expr: &str, doc: &Document) -> Result<String, String> {
    let path = XPath::parse(expr)?;
    let found = path.select(doc);
    if found.is_empty() {
        return Err("nothing in the response matches".to_owned());
    }
    Ok(found
        .into_iter()
        .map(|item| doc.item_xml(item))
        .collect::<Vec<_>>()
        .join("\n"))
}

fn evaluate(path: &Path, doc: &Document, context: Item) -> Vec<Item> {
    let mut current = vec![if path.absolute {
        Item::Node(0)
    } else {
        context
    }];
    for (separator, step) in &path.steps {
        let contexts = match separator {
            Separator::Child => current,
            Separator::Descendants => current
                .into_iter()
                .flat_map(|item| descendants_or_self(doc, item))
                .collect(),
        };
        let mut next = Vec::new();
        for item in contexts {
            let mut found = candidates(doc, item, &step.test);
            for predicate in &step.predicates {
                found = filter(doc, found, predicate);
            }
            next.extend(found);
        }
        next.sort_by_key(|i| i.order());
        next.dedup();
        current = next;
    }
    current
}

/// `item` and the nodes below it that can have children: the document and elements.
fn descendants_or_self(doc: &Document, item: Item) -> Vec<Item> {
    let Item::Node(start) = item else {
        return vec![item];
    };
    let mut out = Vec::new();
    let mut stack = vec![start];
    while let Some(n) = stack.pop() {
        out.push(Item::Node(n));
        let children = doc.nodes[n].children.iter().rev();
        stack.extend(children.filter(|&&c| matches!(doc.nodes[c].kind, Kind::Element(_))));
    }
    out
}

fn candidates(doc: &Document, item: Item, test: &Test) -> Vec<Item> {
    let node = match item {
        Item::Node(n) => n,
        Item::Attribute(n, _) => {
            return match test {
                Test::Current => vec![item],
                Test::Parent => vec![Item::Node(n)],
                Test::Element(_) | Test::Attribute(_) | Test::Text | Test::Node => vec![],
            };
        }
    };
    let children = || doc.nodes[node].children.iter().copied();
    match test {
        Test::Current => vec![item],
        Test::Parent => doc.nodes[node].parent.map(Item::Node).into_iter().collect(),
        Test::Element(name) => children()
            .filter(|&c| match &doc.nodes[c].kind {
                Kind::Element(e) => name.as_deref().is_none_or(|n| name_matches(n, &e.name)),
                Kind::Document | Kind::Text { .. } | Kind::Markup(_) => false,
            })
            .map(Item::Node)
            .collect(),
        Test::Text => children()
            .filter(|&c| is_text(doc, c))
            .map(Item::Node)
            .collect(),
        Test::Node => children()
            .filter(|&c| is_text(doc, c) || matches!(doc.nodes[c].kind, Kind::Element(_)))
            .map(Item::Node)
            .collect(),
        Test::Attribute(name) => match &doc.nodes[node].kind {
            Kind::Element(e) => e
                .attributes
                .iter()
                .enumerate()
                .filter(|(_, (key, _))| match name {
                    Some(n) => name_matches(n, key),
                    // Namespace declarations are not attributes in XPath.
                    None => key != "xmlns" && !key.starts_with("xmlns:"),
                })
                .map(|(i, _)| Item::Attribute(node, i))
                .collect(),
            Kind::Document | Kind::Text { .. } | Kind::Markup(_) => vec![],
        },
    }
}

/// Text that is more than the whitespace between elements.
fn is_text(doc: &Document, node: usize) -> bool {
    matches!(&doc.nodes[node].kind, Kind::Text { value, .. } if !value.trim().is_empty())
}

/// A name with a prefix matches as written; without one, it matches any prefix (so
/// `Body` finds `soap:Body` without declaring namespaces).
fn name_matches(wanted: &str, name: &str) -> bool {
    if wanted.contains(':') {
        wanted == name
    } else {
        wanted == local(name)
    }
}

fn filter(doc: &Document, items: Vec<Item>, predicate: &Predicate) -> Vec<Item> {
    match predicate {
        Predicate::Position(n) => items.get(n.wrapping_sub(1)).copied().into_iter().collect(),
        Predicate::FromLast(n) => items
            .len()
            .checked_sub(n + 1)
            .and_then(|i| items.get(i).copied())
            .into_iter()
            .collect(),
        Predicate::Test(path, test) => items
            .into_iter()
            .filter(|&item| {
                let found = evaluate(path, doc, item);
                match test {
                    None => !found.is_empty(),
                    Some((op, literal)) => found
                        .into_iter()
                        .any(|f| compare(&doc.item_text(f), *op, literal)),
                }
            })
            .collect(),
    }
}

fn compare(value: &str, op: Op, literal: &Literal) -> bool {
    let ordering = match literal {
        Literal::Text(text) => Some(value.cmp(text)),
        Literal::Number(number) => value
            .trim()
            .parse::<f64>()
            .ok()
            .and_then(|v| v.partial_cmp(number)),
    };
    match op {
        Op::Eq => ordering == Some(Ordering::Equal),
        Op::Ne => ordering.is_some_and(|o| o != Ordering::Equal),
        Op::Lt => ordering == Some(Ordering::Less),
        Op::Le => matches!(ordering, Some(Ordering::Less | Ordering::Equal)),
        Op::Gt => ordering == Some(Ordering::Greater),
        Op::Ge => matches!(ordering, Some(Ordering::Greater | Ordering::Equal)),
    }
}

struct Parser<'a> {
    text: &'a str,
    at: usize,
}

impl Parser<'_> {
    fn done(&self) -> bool {
        self.at >= self.text.len()
    }

    fn rest(&self) -> &str {
        self.text.get(self.at..).unwrap_or_default()
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn eat(&mut self, s: &str) -> bool {
        if self.rest().starts_with(s) {
            self.at += s.len();
            true
        } else {
            false
        }
    }

    fn skip_spaces(&mut self) {
        while let Some(c) = self.peek().filter(|c| c.is_whitespace()) {
            self.at += c.len_utf8();
        }
    }

    fn error(&self, what: &str) -> String {
        if self.done() {
            format!("{what} at the end")
        } else {
            format!(
                "{what} at position {} (“{}”)",
                self.text.get(..self.at).unwrap_or_default().chars().count() + 1,
                self.rest().chars().take(12).collect::<String>()
            )
        }
    }

    fn path(&mut self) -> Result<Path, String> {
        let mut steps = Vec::new();
        let absolute = self.peek() == Some('/');
        let mut separator = Separator::Child;
        if self.eat("//") {
            separator = Separator::Descendants;
        } else if self.eat("/") && self.at_path_end() {
            // `/` alone: the document.
            return Ok(Path { absolute, steps });
        }
        loop {
            steps.push((separator, self.step()?));
            separator = if self.eat("//") {
                Separator::Descendants
            } else if self.eat("/") {
                Separator::Child
            } else {
                return Ok(Path { absolute, steps });
            };
        }
    }

    fn at_path_end(&self) -> bool {
        self.done() || matches!(self.peek(), Some(']' | ' '))
    }

    fn step(&mut self) -> Result<Step, String> {
        let test = if self.eat("..") {
            Test::Parent
        } else if self.eat(".") {
            Test::Current
        } else if self.eat("@") {
            Test::Attribute(self.name_test()?)
        } else if self.eat("text()") {
            Test::Text
        } else if self.eat("node()") {
            Test::Node
        } else {
            Test::Element(self.name_test()?)
        };
        let mut predicates = Vec::new();
        while self.eat("[") {
            self.skip_spaces();
            predicates.push(self.predicate()?);
            self.skip_spaces();
            if !self.eat("]") {
                return Err(self.error("expected ]"));
            }
        }
        Ok(Step { test, predicates })
    }

    /// A name such as `item` or `soap:Body`, or `*` (`None`).
    fn name_test(&mut self) -> Result<Option<String>, String> {
        if self.eat("*") {
            return Ok(None);
        }
        let name: String = self
            .rest()
            .chars()
            .take_while(|&c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | ':'))
            .collect();
        if name.is_empty() || name.starts_with(['-', '.', ':']) {
            return Err(self.error("expected a name"));
        }
        self.at += name.len();
        Ok(Some(name))
    }

    fn predicate(&mut self) -> Result<Predicate, String> {
        if let Some(n) = self.integer() {
            return match n {
                0 => Err(self.error("positions count from 1")),
                n => Ok(Predicate::Position(n)),
            };
        }
        if self.eat("last()") {
            self.skip_spaces();
            if !self.eat("-") {
                return Ok(Predicate::FromLast(0));
            }
            self.skip_spaces();
            return self
                .integer()
                .map(Predicate::FromLast)
                .ok_or_else(|| self.error("expected a number after last()-"));
        }
        let path = self.path()?;
        self.skip_spaces();
        let Some(op) = self.op() else {
            return Ok(Predicate::Test(path, None));
        };
        self.skip_spaces();
        let literal = self.literal()?;
        if matches!(literal, Literal::Text(_)) && !matches!(op, Op::Eq | Op::Ne) {
            return Err(self.error("compare with a number"));
        }
        Ok(Predicate::Test(path, Some((op, literal))))
    }

    /// A whole number such as `3`; nothing is consumed if there is none.
    fn integer(&mut self) -> Option<usize> {
        let digits = self.rest().chars().take_while(char::is_ascii_digit).count();
        let n = self.rest().get(..digits)?.parse().ok()?;
        self.at += digits;
        Some(n)
    }

    fn op(&mut self) -> Option<Op> {
        [
            ("!=", Op::Ne),
            ("<=", Op::Le),
            (">=", Op::Ge),
            ("=", Op::Eq),
            ("<", Op::Lt),
            (">", Op::Gt),
        ]
        .into_iter()
        .find_map(|(s, op)| self.eat(s).then_some(op))
    }

    fn literal(&mut self) -> Result<Literal, String> {
        if let Some(quote) = self.peek().filter(|c| matches!(c, '\'' | '"')) {
            self.at += 1;
            let Some(end) = self.rest().find(quote) else {
                return Err(self.error("unclosed quote"));
            };
            let text = self.rest().get(..end).unwrap_or_default().to_owned();
            self.at += end + 1;
            return Ok(Literal::Text(text));
        }
        self.number()
            .map(Literal::Number)
            .ok_or_else(|| self.error("expected a quoted text or a number"))
    }

    /// A number such as `3`, `-1.5`; nothing is consumed if there is none.
    fn number(&mut self) -> Option<f64> {
        let text: String = self
            .rest()
            .chars()
            .enumerate()
            .take_while(|&(i, c)| c.is_ascii_digit() || c == '.' || (i == 0 && c == '-'))
            .map(|(_, c)| c)
            .collect();
        let number = text.parse::<f64>().ok()?;
        self.at += text.len();
        Some(number)
    }
}

/// The first match of `expr` in the XML `body`, as text (R-HTTP-19, for captures).
pub fn first_text(expr: &str, body: &str) -> Result<String, String> {
    let path = XPath::parse(expr)?;
    let doc = Document::parse(body).map_err(|_| "the response body is not XML".to_owned())?;
    path.select(&doc)
        .into_iter()
        .next()
        .map(|item| doc.item_text(item))
        .ok_or_else(|| "nothing in the response matches".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORDERS: &str = r#"<?xml version="1.0"?>
<shop xmlns:x="urn:x">
  <order id="1" status="open"><total>9.5</total><item>pen</item><item>ink</item></order>
  <order id="2" status="paid"><total>20</total><item>book</item></order>
  <x:note lang="en">Grüße 😀</x:note>
</shop>"#;

    fn run(expr: &str) -> Vec<String> {
        let doc = Document::parse(ORDERS).expect("parse");
        XPath::parse(expr)
            .unwrap_or_else(|e| panic!("{expr}: {e}"))
            .select(&doc)
            .into_iter()
            .map(|i| doc.item_text(i))
            .collect()
    }

    #[test]
    fn paths_select_elements_attributes_and_text() {
        assert_eq!(run("/shop/order/@id"), ["1", "2"]);
        assert_eq!(run("shop/order/item"), ["pen", "ink", "book"]);
        assert_eq!(run("//item/text()"), ["pen", "ink", "book"]);
        assert_eq!(run("//order[2]/total"), ["20"]);
        assert_eq!(run("/shop/*/@lang"), ["en"]);
        assert_eq!(run("//order[1]/@*"), ["1", "open"]);
        assert_eq!(run("//item[1]"), ["pen", "book"]);
    }

    #[test]
    fn predicates_filter_by_position_and_value() {
        assert_eq!(run("//order[last()]/@id"), ["2"]);
        assert_eq!(run("//order[last()-1]/@id"), ["1"]);
        assert_eq!(run("//order[@status='paid']/item"), ["book"]);
        assert_eq!(run("//order[@status != \"paid\"]/@id"), ["1"]);
        assert_eq!(run("//order[total > 10]/@id"), ["2"]);
        assert_eq!(run("//order[total <= 9.5]/@id"), ["1"]);
        assert_eq!(run("//order[item='ink']/@id"), ["1"]);
        assert_eq!(run("//order[@missing]"), Vec::<String>::new());
        assert_eq!(run("//item[.='book']/../@id"), ["2"]);
    }

    #[test]
    fn names_without_a_prefix_match_any_prefix() {
        assert_eq!(run("//note"), ["Grüße 😀"]);
        assert_eq!(run("//x:note"), ["Grüße 😀"]);
        assert_eq!(run("//y:note"), Vec::<String>::new());
    }

    #[test]
    fn namespace_declarations_are_not_attributes() {
        assert_eq!(run("/shop/@*"), Vec::<String>::new());
    }

    #[test]
    fn the_root_path_is_the_whole_document() {
        let doc = Document::parse("<a><b>1</b></a>").expect("parse");
        assert_eq!(query("/", &doc).expect("query"), "<a>\n  <b>1</b>\n</a>");
    }

    #[test]
    fn query_shows_elements_as_xml_and_values_as_text() {
        let doc = Document::parse(ORDERS).expect("parse");
        assert_eq!(
            query("//order[2]", &doc).expect("query"),
            "<order id=\"2\" status=\"paid\">\n  <total>20</total>\n  <item>book</item>\n</order>"
        );
        assert_eq!(query("//order/@id", &doc).expect("query"), "1\n2");
        assert_eq!(
            query("//nothing", &doc).expect_err("no match"),
            "nothing in the response matches"
        );
    }

    #[test]
    fn bad_expressions_say_where() {
        for (bad, says) in [
            ("", "empty"),
            ("/shop/", "expected a name at the end"),
            ("//order[", "expected a name at the end"),
            ("//order[0]", "positions count from 1"),
            ("//order[@id='1'", "expected ] at the end"),
            ("//order[@id < 'a']", "compare with a number"),
            ("//order[@id = 'x]", "unclosed quote"),
            ("/shop ü", "unexpected text at position 6"),
            ("(//item)[1]", "expected a name"),
            ("//item[1.5]", "expected ]"),
        ] {
            let err = XPath::parse(bad).expect_err(bad);
            assert!(err.contains(says), "{bad}: {err}");
        }
    }

    #[test]
    fn first_text_captures_the_first_match() {
        assert_eq!(first_text("//order/@id", ORDERS).expect("capture"), "1");
        assert_eq!(
            first_text("//order", "{\"a\": 1}").expect_err("json"),
            "the response body is not XML"
        );
        assert_eq!(
            first_text("//none", ORDERS).expect_err("none"),
            "nothing in the response matches"
        );
    }
}
