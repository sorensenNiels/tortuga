//! Bruno's `.bru` files (R-COL-31): named blocks, each a dictionary of `key: value`
//! lines (`~` in front turns one off, `'''` starts a value of several lines), a text
//! (bodies, scripts, docs) or a list (`vars:secret [ … ]`).
//!
//! ```text
//! meta {
//!   name: Get user
//!   seq: 2
//! }
//!
//! body:json {
//!   { "name": "Ann" }
//! }
//! ```
//!
//! A block's lines are indented by two spaces and it ends with `}` (or `]`) at the
//! start of a line, which is how Bruno writes them.

/// One `key: value` line of a dictionary block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Pair {
    pub key: String,
    pub value: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Body {
    Dict(Vec<Pair>),
    Text(String),
    List(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Block {
    pub name: String,
    pub body: Body,
}

/// A parsed `.bru` file.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct BruFile {
    pub blocks: Vec<Block>,
}

/// Blocks whose content is text rather than `key: value` lines.
fn is_text_block(name: &str) -> bool {
    match name {
        "body:form-urlencoded" | "body:multipart-form" | "body:file" => false,
        "tests" | "docs" => true,
        _ => name.starts_with("body:") || name.starts_with("script:"),
    }
}

impl BruFile {
    pub fn block(&self, name: &str) -> Option<&Block> {
        self.blocks.iter().find(|b| b.name == name)
    }

    /// The pairs of the dictionary block `name`; none if there is no such block.
    pub fn dict(&self, name: &str) -> &[Pair] {
        match self.block(name).map(|b| &b.body) {
            Some(Body::Dict(pairs)) => pairs,
            _ => &[],
        }
    }

    /// The value of the enabled `key` in the dictionary block `name`.
    pub fn get(&self, name: &str, key: &str) -> Option<&str> {
        self.dict(name)
            .iter()
            .find(|p| p.enabled && p.key == key)
            .map(|p| p.value.as_str())
    }

    /// The text of the text block `name`; `None` if there is none or it is empty.
    pub fn text(&self, name: &str) -> Option<&str> {
        match self.block(name).map(|b| &b.body) {
            Some(Body::Text(text)) if !text.trim().is_empty() => Some(text),
            _ => None,
        }
    }

    /// The items of the list block `name`.
    pub fn list(&self, name: &str) -> &[String] {
        match self.block(name).map(|b| &b.body) {
            Some(Body::List(items)) => items,
            _ => &[],
        }
    }
}

/// Reads a `.bru` file. The error says which line could not be read.
pub(super) fn parse(text: &str) -> Result<BruFile, String> {
    let text = text.trim_start_matches('\u{feff}');
    let mut lines = text
        .lines()
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
        .enumerate();
    let mut file = BruFile::default();
    while let Some((n, line)) = lines.next() {
        let head = line.trim();
        if head.is_empty() {
            continue;
        }
        let (name, close) = if let Some(name) = head.strip_suffix('{') {
            (name.trim(), "}")
        } else if let Some(name) = head.strip_suffix('[') {
            (name.trim(), "]")
        } else {
            return Err(format!(
                "line {}: expected the start of a block, such as “meta {{”",
                n + 1
            ));
        };
        if name.is_empty() || name.contains(char::is_whitespace) {
            return Err(format!("line {}: “{head}” does not name a block", n + 1));
        }
        let mut content = Vec::new();
        let mut closed = false;
        for (_, line) in lines.by_ref() {
            if line.trim_end() == close {
                closed = true;
                break;
            }
            content.push(line);
        }
        if !closed {
            return Err(format!("line {}: block “{name}” is not closed", n + 1));
        }
        let body = if close == "]" {
            Body::List(list(&content))
        } else if is_text_block(name) {
            Body::Text(outdented(&content))
        } else {
            Body::Dict(pairs(&content))
        };
        file.blocks.push(Block {
            name: name.to_owned(),
            body,
        });
    }
    Ok(file)
}

/// A text block's lines without the two spaces Bruno indents them by.
fn outdented(lines: &[&str]) -> String {
    let lines: Vec<&str> = lines
        .iter()
        .map(|l| l.strip_prefix("  ").unwrap_or(l.trim_start_matches(' ')))
        .collect();
    lines.join("\n").trim_matches('\n').to_owned()
}

fn list(lines: &[&str]) -> Vec<String> {
    lines
        .iter()
        .flat_map(|l| l.split(','))
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_owned)
        .collect()
}

fn pairs(lines: &[&str]) -> Vec<Pair> {
    let mut out = Vec::new();
    let mut rest = lines.iter();
    while let Some(line) = rest.next() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (enabled, line) = match line.strip_prefix('~') {
            Some(line) => (false, line),
            None => (true, line),
        };
        let Some((key, value)) = key_and_value(line) else {
            continue;
        };
        let value = if value == "'''" {
            let mut text = Vec::new();
            for line in rest.by_ref() {
                if line.trim() == "'''" {
                    break;
                }
                text.push(*line);
            }
            dedented(&text)
        } else {
            value.to_owned()
        };
        out.push(Pair {
            key,
            value,
            enabled,
        });
    }
    out
}

/// `key: value`, or `"key: with colon": value`.
fn key_and_value(line: &str) -> Option<(String, &str)> {
    if let Some(quoted) = line.strip_prefix('"') {
        let end = quoted.find("\":")?;
        let key = quoted.get(..end)?.replace("\\\"", "\"");
        let value = quoted.get(end + 2..)?.trim();
        return Some((key, value));
    }
    let (key, value) = line.split_once(':')?;
    let key = key.trim();
    (!key.is_empty()).then(|| (key.to_owned(), value.trim()))
}

/// Lines of a `'''` value without the indentation they share.
fn dedented(lines: &[&str]) -> String {
    let indent = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start_matches(' ').len())
        .min()
        .unwrap_or(0);
    lines
        .iter()
        .map(|l| l.get(indent..).unwrap_or_else(|| l.trim_start()))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    const REQUEST: &str = "meta {\r\n  name: Get user ü😀\r\n  type: http\r\n  seq: 2\r\n}\r\n\r\nget {\r\n  url: {{baseUrl}}/users/:id?x=1\r\n  body: json\r\n  auth: inherit\r\n}\r\n\r\nheaders {\r\n  Accept: application/json\r\n  ~X-Off: 1\r\n  \"X-Odd: key\": a: b\r\n}\r\n\r\nbody:json {\r\n  {\r\n    \"name\": \"Ann\"\r\n  }\r\n}\r\n\r\nvars:pre-request {\r\n  note: '''\r\n    line one\r\n      line two\r\n  '''\r\n  after: 1\r\n}\r\n\r\nvars:secret [\r\n  token,\r\n  ~old, password\r\n]\r\n";

    /// R-COL-31: dictionaries (with disabled, quoted and multi-line entries), text and
    /// lists, with Windows line endings.
    #[test]
    fn reads_every_kind_of_block() {
        let file = parse(REQUEST).expect("parse");
        let names: Vec<_> = file.blocks.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "meta",
                "get",
                "headers",
                "body:json",
                "vars:pre-request",
                "vars:secret"
            ]
        );
        assert_eq!(file.get("meta", "name"), Some("Get user ü😀"));
        assert_eq!(file.get("get", "url"), Some("{{baseUrl}}/users/:id?x=1"));
        let headers = file.dict("headers");
        assert_eq!(headers.len(), 3);
        assert!(!headers[1].enabled);
        assert_eq!(file.get("headers", "X-Off"), None);
        assert_eq!(file.get("headers", "X-Odd: key"), Some("a: b"));
        assert_eq!(file.text("body:json"), Some("{\n  \"name\": \"Ann\"\n}"));
        assert_eq!(
            file.get("vars:pre-request", "note"),
            Some("line one\n  line two")
        );
        assert_eq!(file.get("vars:pre-request", "after"), Some("1"));
        assert_eq!(file.list("vars:secret"), ["token", "~old", "password"]);
        assert_eq!(file.dict("missing"), []);
        assert_eq!(file.text("docs"), None);
    }

    #[test]
    fn empty_and_broken_files() {
        assert_eq!(parse("").expect("empty"), BruFile::default());
        assert_eq!(parse("\u{feff}\n\n").expect("blank"), BruFile::default());
        let empty = parse("docs {\n}\n").expect("empty block");
        assert_eq!(empty.text("docs"), None);
        let err = parse("meta {\n  name: x\n").expect_err("not closed");
        assert!(
            err.contains("line 1") && err.contains("not closed"),
            "{err}"
        );
        let err = parse("hello\n").expect_err("no block");
        assert!(err.contains("line 1"), "{err}");
    }
}
