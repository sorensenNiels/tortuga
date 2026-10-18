//! A `JSONPath` subset to filter JSON responses (R-HTTP-13, D18).
//!
//! Supported: `$` (optional), `.name`, `['name']`, `[n]` (negative counts from the end),
//! `[a:b]`, `[*]` / `.*`, `..name` / `..*` / `..[…]` (recursive), unions `[a,b]` /
//! `['a','b']`, and filters `[?(@.field)]` / `[?(@.field <op> value)]` with
//! `== != < <= > >=`, where `@` may be followed by `.name`, `['name']` and `[n]`.

use serde_json::Value;

/// One step of a path.
#[derive(Debug, Clone, PartialEq)]
enum Segment {
    /// Apply the selector to the current nodes.
    Child(Selector),
    /// Apply the selector to the current nodes and everything below them.
    Descendants(Selector),
}

#[derive(Debug, Clone, PartialEq)]
enum Selector {
    Name(String),
    Wildcard,
    Index(i64),
    Slice(Option<i64>, Option<i64>),
    Union(Vec<Selector>),
    Filter(Filter),
}

/// `?(@.path)` or `?(@.path <op> literal)`.
#[derive(Debug, Clone, PartialEq)]
struct Filter {
    path: Vec<Selector>,
    test: Option<(Op, Value)>,
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

/// A parsed expression.
#[derive(Debug, Clone, PartialEq)]
pub struct JsonPath(Vec<Segment>);

impl JsonPath {
    /// Parses `expr`. The error says what is wrong and where.
    pub fn parse(expr: &str) -> Result<Self, String> {
        let mut p = Parser {
            text: expr.trim(),
            at: 0,
        };
        p.eat('$');
        let mut segments = Vec::new();
        while !p.done() {
            segments.push(p.segment()?);
        }
        Ok(Self(segments))
    }

    /// The matching values, in document order.
    pub fn select<'a>(&self, root: &'a Value) -> Vec<&'a Value> {
        let mut nodes = vec![root];
        for segment in &self.0 {
            nodes = match segment {
                Segment::Child(sel) => nodes.into_iter().flat_map(|n| apply(sel, n)).collect(),
                Segment::Descendants(sel) => nodes
                    .into_iter()
                    .flat_map(descendants)
                    .flat_map(|n| apply(sel, n))
                    .collect(),
            };
        }
        nodes
    }
}

/// Filters `root` with `expr`: the matches as a JSON array (or the value itself for an
/// expression without wildcards that matches once).
pub fn query(expr: &str, root: &Value) -> Result<Value, String> {
    let path = JsonPath::parse(expr)?;
    let found = path.select(root);
    let single = path
        .0
        .iter()
        .all(|s| matches!(s, Segment::Child(Selector::Name(_) | Selector::Index(_))));
    Ok(match found.as_slice() {
        [one] if single => (*one).clone(),
        _ => Value::Array(found.into_iter().cloned().collect()),
    })
}

/// `node` and every value below it, parents first.
fn descendants(node: &Value) -> Vec<&Value> {
    let mut out = vec![node];
    let mut i = 0;
    while i < out.len() {
        let children: Vec<&Value> = match out[i] {
            Value::Array(items) => items.iter().collect(),
            Value::Object(map) => map.values().collect(),
            _ => Vec::new(),
        };
        // Depth-first order: a node's children come right after it.
        let at = i + 1;
        out.splice(at..at, children);
        i += 1;
    }
    out
}

fn apply<'a>(sel: &Selector, node: &'a Value) -> Vec<&'a Value> {
    match (sel, node) {
        (Selector::Name(name), Value::Object(map)) => map.get(name).into_iter().collect(),
        (Selector::Wildcard, Value::Object(map)) => map.values().collect(),
        (Selector::Wildcard, Value::Array(items)) => items.iter().collect(),
        (Selector::Index(i), Value::Array(items)) => index(items, *i).into_iter().collect(),
        (Selector::Slice(from, to), Value::Array(items)) => {
            let len = i64::try_from(items.len()).unwrap_or(i64::MAX);
            let clamp = |i: i64| {
                usize::try_from(if i < 0 { (len + i).max(0) } else { i.min(len) }).unwrap_or(0)
            };
            let (a, b) = (clamp(from.unwrap_or(0)), clamp(to.unwrap_or(len)));
            items.get(a..b.max(a)).unwrap_or_default().iter().collect()
        }
        (Selector::Union(parts), _) => parts.iter().flat_map(|p| apply(p, node)).collect(),
        (Selector::Filter(f), Value::Array(items)) => {
            items.iter().filter(|v| f.matches(v)).collect()
        }
        (Selector::Filter(f), Value::Object(map)) => {
            map.values().filter(|v| f.matches(v)).collect()
        }
        _ => Vec::new(),
    }
}

fn index(items: &[Value], i: i64) -> Option<&Value> {
    let len = i64::try_from(items.len()).ok()?;
    let i = if i < 0 { len + i } else { i };
    items.get(usize::try_from(i).ok()?)
}

impl Filter {
    fn matches(&self, item: &Value) -> bool {
        let mut nodes = vec![item];
        for sel in &self.path {
            nodes = nodes.into_iter().flat_map(|n| apply(sel, n)).collect();
        }
        let Some(value) = nodes.first() else {
            return false;
        };
        match &self.test {
            None => true,
            Some((op, literal)) => compare(value, *op, literal),
        }
    }
}

fn compare(value: &Value, op: Op, literal: &Value) -> bool {
    use std::cmp::Ordering;
    let ordering = match (value, literal) {
        (Value::Number(a), Value::Number(b)) => a
            .as_f64()
            .zip(b.as_f64())
            .and_then(|(a, b)| a.partial_cmp(&b)),
        (Value::String(a), Value::String(b)) => Some(a.cmp(b)),
        (a, b) if a == b => Some(Ordering::Equal),
        _ => None,
    };
    match op {
        Op::Eq => ordering == Some(Ordering::Equal),
        Op::Ne => ordering != Some(Ordering::Equal),
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
        &self.text[self.at..]
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(c) {
            self.at += c.len_utf8();
            true
        } else {
            false
        }
    }

    fn eat_str(&mut self, s: &str) -> bool {
        if self.rest().starts_with(s) {
            self.at += s.len();
            true
        } else {
            false
        }
    }

    fn skip_spaces(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.at += 1;
        }
    }

    fn error(&self, what: &str) -> String {
        if self.done() {
            format!("{what} at the end")
        } else {
            format!(
                "{what} at position {} (“{}”)",
                self.at + 1,
                self.rest().chars().take(12).collect::<String>()
            )
        }
    }

    fn segment(&mut self) -> Result<Segment, String> {
        if self.eat_str("..") {
            let sel = if self.peek() == Some('[') {
                self.bracket()?
            } else {
                self.dotted()?
            };
            return Ok(Segment::Descendants(sel));
        }
        if self.eat('.') {
            return Ok(Segment::Child(self.dotted()?));
        }
        if self.peek() == Some('[') {
            return Ok(Segment::Child(self.bracket()?));
        }
        // `items[0]` without the leading `$.`: read as `$.items[0]`.
        if self.at == 0 || (self.at == 1 && self.text.starts_with('$')) {
            return Ok(Segment::Child(self.dotted()?));
        }
        Err(self.error("expected . or ["))
    }

    /// The part after `.` or `..`: a name or `*`.
    fn dotted(&mut self) -> Result<Selector, String> {
        if self.eat('*') {
            return Ok(Selector::Wildcard);
        }
        let end = self
            .rest()
            .find(|c: char| c == '.' || c == '[' || c.is_whitespace())
            .unwrap_or(self.rest().len());
        if end == 0 {
            return Err(self.error("expected a name"));
        }
        let name = self.rest()[..end].to_owned();
        self.at += end;
        Ok(Selector::Name(name))
    }

    /// `[…]`: names, indexes, a slice, `*` or a filter.
    fn bracket(&mut self) -> Result<Selector, String> {
        self.eat('[');
        self.skip_spaces();
        let sel = if self.eat('*') {
            Selector::Wildcard
        } else if self.eat('?') {
            Selector::Filter(self.filter()?)
        } else {
            let mut parts = vec![self.bracket_part()?];
            self.skip_spaces();
            while self.eat(',') {
                self.skip_spaces();
                parts.push(self.bracket_part()?);
                self.skip_spaces();
            }
            if parts.len() == 1 {
                parts.remove(0)
            } else {
                Selector::Union(parts)
            }
        };
        self.skip_spaces();
        if !self.eat(']') {
            return Err(self.error("expected ]"));
        }
        Ok(sel)
    }

    fn bracket_part(&mut self) -> Result<Selector, String> {
        if let Some(quote @ ('\'' | '"')) = self.peek() {
            return Ok(Selector::Name(self.quoted(quote)?));
        }
        let from = self.integer();
        self.skip_spaces();
        if self.eat(':') {
            self.skip_spaces();
            let to = self.integer();
            return Ok(Selector::Slice(from, to));
        }
        from.map(Selector::Index)
            .ok_or_else(|| self.error("expected a name in quotes, a number, * or ?(…)"))
    }

    fn integer(&mut self) -> Option<i64> {
        let len = self
            .rest()
            .char_indices()
            .take_while(|&(i, c)| c.is_ascii_digit() || (i == 0 && c == '-'))
            .count();
        let n = self.rest()[..len].parse().ok()?;
        self.at += len;
        Some(n)
    }

    fn quoted(&mut self, quote: char) -> Result<String, String> {
        self.eat(quote);
        let mut out = String::new();
        loop {
            let Some(c) = self.peek() else {
                return Err(self.error("a quote is not closed"));
            };
            self.at += c.len_utf8();
            match c {
                '\\' => {
                    if let Some(next) = self.peek() {
                        self.at += next.len_utf8();
                        out.push(next);
                    }
                }
                c if c == quote => return Ok(out),
                c => out.push(c),
            }
        }
    }

    /// After `?`: `(@.path)` or `(@.path op literal)`.
    fn filter(&mut self) -> Result<Filter, String> {
        self.skip_spaces();
        if !self.eat('(') {
            return Err(self.error("expected ( after ?"));
        }
        self.skip_spaces();
        if !self.eat('@') {
            return Err(self.error("expected @ in the filter"));
        }
        let mut path = Vec::new();
        loop {
            if self.eat('.') {
                path.push(self.filter_name()?);
            } else if self.peek() == Some('[') {
                self.eat('[');
                self.skip_spaces();
                path.push(self.bracket_part()?);
                self.skip_spaces();
                if !self.eat(']') {
                    return Err(self.error("expected ]"));
                }
            } else {
                break;
            }
        }
        self.skip_spaces();
        let op = [
            ("==", Op::Eq),
            ("!=", Op::Ne),
            ("<=", Op::Le),
            (">=", Op::Ge),
            ("<", Op::Lt),
            (">", Op::Gt),
        ]
        .into_iter()
        .find(|(s, _)| self.eat_str(s))
        .map(|(_, op)| op);
        let test = match op {
            Some(op) => {
                self.skip_spaces();
                Some((op, self.literal()?))
            }
            None => None,
        };
        self.skip_spaces();
        if !self.eat(')') {
            return Err(self.error("expected ) to end the filter"));
        }
        Ok(Filter { path, test })
    }

    /// A name after `@.` (stops at operators and spaces too).
    fn filter_name(&mut self) -> Result<Selector, String> {
        let end = self
            .rest()
            .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '-' || c == '$'))
            .unwrap_or(self.rest().len());
        if end == 0 {
            return Err(self.error("expected a name"));
        }
        let name = self.rest()[..end].to_owned();
        self.at += end;
        Ok(Selector::Name(name))
    }

    fn literal(&mut self) -> Result<Value, String> {
        if let Some(quote @ ('\'' | '"')) = self.peek() {
            return Ok(Value::String(self.quoted(quote)?));
        }
        for (word, value) in [
            ("true", Value::Bool(true)),
            ("false", Value::Bool(false)),
            ("null", Value::Null),
        ] {
            if self.eat_str(word) {
                return Ok(value);
            }
        }
        let len = self
            .rest()
            .find(|c: char| !(c.is_ascii_digit() || matches!(c, '-' | '+' | '.' | 'e' | 'E')))
            .unwrap_or(self.rest().len());
        let number =
            serde_json::from_str::<serde_json::Number>(&self.rest()[..len]).map_err(|_| {
                self.error("expected a number, a string in quotes, true, false or null")
            })?;
        self.at += len;
        Ok(Value::Number(number))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn doc() -> Value {
        json!({
            "store": {
                "books": [
                    {"title": "A", "price": 8, "tags": ["x"]},
                    {"title": "B", "price": 12, "isbn": "1"},
                    {"title": "C", "price": 23, "isbn": "2"}
                ],
                "bike": {"color": "red", "price": 19}
            },
            "odd key": 1
        })
    }

    fn q(expr: &str) -> Value {
        query(expr, &doc()).unwrap_or_else(|e| panic!("{expr}: {e}"))
    }

    #[test]
    fn names_and_indexes() {
        assert_eq!(q("$.store.bike.color"), json!("red"));
        assert_eq!(q("$['store']['bike']['color']"), json!("red"));
        assert_eq!(q("$[\"odd key\"]"), json!(1));
        assert_eq!(q("$.store.books[0].title"), json!("A"));
        assert_eq!(q("$.store.books[-1].title"), json!("C"));
        assert_eq!(
            q("store.bike.price"),
            json!(19),
            "the leading $ is optional"
        );
        assert_eq!(q("$.nothing"), json!([]));
        assert_eq!(q("$"), doc());
    }

    #[test]
    fn wildcards_slices_and_unions() {
        assert_eq!(q("$.store.books[*].title"), json!(["A", "B", "C"]));
        assert_eq!(q("$.store.books.*.price"), json!([8, 12, 23]));
        assert_eq!(q("$.store.books[0:2].title"), json!(["A", "B"]));
        assert_eq!(q("$.store.books[-2:].title"), json!(["B", "C"]));
        assert_eq!(q("$.store.books[:1].title"), json!(["A"]));
        assert_eq!(q("$.store.books[0,2].title"), json!(["A", "C"]));
        assert_eq!(q("$.store.bike['color','price']"), json!(["red", 19]));
    }

    #[test]
    fn recursive_descent() {
        assert_eq!(q("$..price"), json!([8, 12, 23, 19]));
        assert_eq!(q("$..books[1].title"), json!(["B"]));
        assert_eq!(q("$.store..isbn"), json!(["1", "2"]));
    }

    #[test]
    fn filters() {
        assert_eq!(q("$.store.books[?(@.isbn)].title"), json!(["B", "C"]));
        assert_eq!(q("$.store.books[?(@.price < 10)].title"), json!(["A"]));
        assert_eq!(
            q("$.store.books[?(@.price >= 12)].title"),
            json!(["B", "C"])
        );
        assert_eq!(q("$.store.books[?(@.title == 'B')].price"), json!([12]));
        assert_eq!(
            q("$.store.books[?(@.title != \"B\")].price"),
            json!([8, 23])
        );
        assert_eq!(q("$.store.books[?(@.tags[0] == 'x')].title"), json!(["A"]));
        assert_eq!(q("$..[?(@.color == 'red')].price"), json!([19]));
    }

    #[test]
    fn errors_say_where() {
        let err = |expr: &str| query(expr, &doc()).expect_err(expr);
        assert!(
            err("$.store[").contains("at the end"),
            "{}",
            err("$.store[")
        );
        assert!(err("$.store.books[?(@.price <)]").contains("expected a number"));
        assert!(
            err("$.store x").contains("position"),
            "{}",
            err("$.store x")
        );
        assert!(err("$['a").contains("not closed"));
        assert!(err("$.store.books[?@.x]").contains("expected ("));
    }
}
