//! A light JavaScript lexer for scripts: [`beautify`] (R-TEST-7) puts one statement per
//! line, indented by nesting, with a space after commas; [`scan`] finds what breakpoints
//! need (R-TEST-10). Strings, template literals, regular expressions and comments are
//! recognised and left unchanged, so commented-out code (R-COL-19) survives. The user's
//! line breaks are kept, with at most one blank line in a row. Never fails: broken
//! scripts are handled as far as they go.

/// Spaces per nesting level.
const INDENT: &str = "  ";

/// Words after which `/` starts a regular expression, not a division.
const BEFORE_REGEX: &[&str] = &[
    "return",
    "typeof",
    "instanceof",
    "in",
    "of",
    "new",
    "delete",
    "void",
    "throw",
    "case",
    "do",
    "else",
    "yield",
    "await",
];

/// Words that continue a statement after its block's `}` (`} else {`).
const AFTER_BLOCK: &[&str] = &["else", "catch", "finally", "while"];

/// Words followed by a space before their `(`: `if (a)`.
const BEFORE_PAREN: &[&str] = &["if", "for", "while", "switch", "catch", "with"];

/// Operators written with a space on both sides, longest first. `+ - * / %` are not
/// among them on their own: they can be unary (`-1`).
const SPACED: &[&str] = &[
    ">>>=", "===", "!==", "**=", "<<=", ">>=", ">>>", "&&=", "||=", "??=", "==", "!=", "<=", ">=",
    "=>", "&&", "||", "??", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "<<", ">>", "=", "<",
    ">", "?",
];

/// `src` beautified. Formatting the result again changes nothing.
pub fn beautify(src: &str) -> String {
    let chars: Vec<char> = src.chars().collect();
    let mut w = Writer::default();
    let mut i = 0;
    while let Some(&c) = chars.get(i) {
        match c {
            '\n' => {
                w.source_newline();
                i += 1;
            }
            ' ' | '\t' | '\r' => {
                w.space();
                i += 1;
            }
            '"' | '\'' => i = w.verbatim(&chars, i, quoted(&chars, i)),
            '`' => i = w.verbatim(&chars, i, template(&chars, i)),
            '/' if chars.get(i + 1) == Some(&'/') => {
                let end = (i..chars.len())
                    .find(|&j| chars[j] == '\n')
                    .unwrap_or(chars.len());
                w.line_comment(&chars[i..end].iter().collect::<String>());
                i = end;
            }
            '/' if chars.get(i + 1) == Some(&'*') => {
                let end = find_from(&chars, i + 2, &['*', '/']).map_or(chars.len(), |j| j + 2);
                i = w.verbatim(&chars, i, end);
            }
            '/' if w.regex_may_start() => i = w.verbatim(&chars, i, regex(&chars, i)),
            '{' if next_code(&chars, i + 1).map(|j| chars[j]) == Some('}') => {
                w.space_before_brace();
                w.code("{}");
                i = next_code(&chars, i + 1).map_or(chars.len(), |j| j + 1);
            }
            '{' | '(' | '[' => {
                w.open(c);
                if c == '{' {
                    w.request_break();
                }
                i += 1;
            }
            '}' => {
                w.break_line();
                w.close('}');
                match after_block(&chars, i + 1) {
                    AfterBlock::Bracket => {}
                    AfterBlock::Word => w.space(),
                    AfterBlock::Other => w.request_break(),
                }
                i += 1;
            }
            ')' | ']' => {
                w.close(c);
                i += 1;
            }
            ';' => {
                w.code(";");
                if w.in_statement_list() {
                    w.request_break();
                }
                i += 1;
            }
            '?' if chars.get(i + 1) == Some(&'.')
                && !chars.get(i + 2).is_some_and(char::is_ascii_digit) =>
            {
                // Optional chaining, `a?.b`, not the `?` of `a ? .5 : 1`.
                w.code("?.");
                i += 2;
            }
            ':' => {
                w.code(":");
                w.space();
                i += 1;
            }
            c if SPACED.iter().any(|op| op.starts_with(c))
                && let Some(op) = SPACED.iter().find(|op| starts_with(&chars[i..], op)) =>
            {
                w.operator(op);
                i += op.chars().count();
            }
            ',' => {
                w.code(",");
                if chars.get(i + 1).is_some_and(|n| !n.is_whitespace()) {
                    w.space();
                }
                i += 1;
            }
            _ => {
                let end = (i + 1..chars.len())
                    .find(|&j| !is_word(chars[j]) || !is_word(c))
                    .unwrap_or(chars.len());
                w.code(&chars[i..end].iter().collect::<String>());
                i = end;
            }
        }
    }
    w.finish()
}

/// What [`scan`] learns about one line of a script.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineStart {
    /// The line starts in code, not inside a string, template literal or comment.
    pub in_code: bool,
    /// The last character of code before the line, comments skipped; `None` at the start.
    pub after: Option<char>,
}

/// Every line of a script, the names it uses and what to colour (R-TEST-14).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Scan {
    /// One per line, the first line first.
    pub lines: Vec<LineStart>,
    /// Identifiers used in code, not after a `.`, each once, in order of first use.
    pub names: Vec<String>,
    /// Byte ranges of the parts that get a colour, in order.
    pub spans: Vec<(std::ops::Range<usize>, Token)>,
}

/// A part of a script that the editor colours.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Token {
    Keyword,
    String,
    Number,
    Comment,
    Regex,
}

/// Words coloured as keywords.
const KEYWORDS: &[&str] = &[
    "async",
    "await",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "default",
    "delete",
    "do",
    "else",
    "extends",
    "false",
    "finally",
    "for",
    "function",
    "if",
    "in",
    "instanceof",
    "let",
    "new",
    "null",
    "of",
    "return",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "typeof",
    "undefined",
    "var",
    "void",
    "while",
    "yield",
];

/// Scans `src` for its lines and names (R-TEST-10).
pub fn scan(src: &str) -> Scan {
    let chars: Vec<char> = src.chars().collect();
    // Byte offset of each character, and of the end.
    let bytes: Vec<usize> = src
        .char_indices()
        .map(|(b, _)| b)
        .chain([src.len()])
        .collect();
    let span = |from: usize, to: usize, token: Token| {
        let at = |i: usize| bytes[i.min(chars.len())];
        (at(from)..at(to), token)
    };
    let mut out = Scan {
        lines: vec![LineStart {
            in_code: true,
            after: None,
        }],
        names: Vec::new(),
        spans: Vec::new(),
    };
    let mut last = String::new();
    let mut after: Option<char> = None;
    let mut i = 0;
    // Lines that start inside `chars[from..to]`, a string or comment.
    let skip = |out: &mut Scan, from: usize, to: usize| {
        let inside = chars[from..to.min(chars.len())]
            .iter()
            .filter(|c| **c == '\n');
        out.lines.extend(inside.map(|_| LineStart {
            in_code: false,
            after: None,
        }));
    };
    while let Some(&c) = chars.get(i) {
        let end = match c {
            '\n' => {
                out.lines.push(LineStart {
                    in_code: true,
                    after,
                });
                i += 1;
                continue;
            }
            c if c.is_whitespace() => {
                i += 1;
                continue;
            }
            '"' | '\'' => quoted(&chars, i),
            '`' => template(&chars, i),
            '/' if chars.get(i + 1) == Some(&'/') => {
                let end = (i..chars.len())
                    .find(|&j| chars[j] == '\n')
                    .unwrap_or(chars.len());
                out.spans.push(span(i, end, Token::Comment));
                i = end;
                continue;
            }
            '/' if chars.get(i + 1) == Some(&'*') => {
                let end = find_from(&chars, i + 2, &['*', '/']).map_or(chars.len(), |j| j + 2);
                out.spans.push(span(i, end, Token::Comment));
                skip(&mut out, i, end);
                i = end;
                continue;
            }
            '/' if regex_may_follow(&last) => regex(&chars, i),
            c if is_word(c) => {
                let end = (i..chars.len())
                    .find(|&j| !is_word(chars[j]))
                    .unwrap_or(chars.len());
                let word: String = chars[i..end].iter().collect();
                if c.is_ascii_digit() {
                    out.spans.push(span(i, end, Token::Number));
                } else if after != Some('.') && KEYWORDS.contains(&word.as_str()) {
                    out.spans.push(span(i, end, Token::Keyword));
                }
                if !c.is_ascii_digit() && after != Some('.') && !out.names.contains(&word) {
                    out.names.push(word.clone());
                }
                last = word;
                after = chars.get(end - 1).copied();
                i = end;
                continue;
            }
            _ => i + 1,
        };
        match c {
            '"' | '\'' | '`' => out.spans.push(span(i, end, Token::String)),
            // A division is one character; a regular expression is more.
            '/' if end > i + 1 => out.spans.push(span(i, end, Token::Regex)),
            _ => {}
        }
        skip(&mut out, i, end);
        last = chars[i..end.min(chars.len())].iter().collect();
        after = chars.get(end.min(chars.len()) - 1).copied();
        i = end.max(i + 1);
    }
    out
}

/// Whether a `/` after the code `last` starts a regular expression.
fn regex_may_follow(last: &str) -> bool {
    match last.chars().last() {
        None => true,
        Some(c) if is_word(c) => BEFORE_REGEX.contains(&last),
        Some(c) => !matches!(c, ')' | ']' | '}'),
    }
}

/// One opening `{`, `(` or `[`, and the indent of the lines inside it.
#[derive(Debug, Clone, Copy)]
struct Frame {
    open: char,
    indent: usize,
}

#[derive(Debug, Default)]
struct Writer {
    out: String,
    /// The current line, without its indent.
    line: String,
    /// Indent of the current line, fixed by its first character.
    line_indent: usize,
    /// Indent given to what opens on the current line: several openers on one line
    /// (`test("x", () => {`) indent their contents only once.
    opened_indent: Option<usize>,
    stack: Vec<Frame>,
    /// A line break is due before the next code, unless a `//` comment follows.
    break_due: bool,
    /// Source newlines since the last code; two or more make one blank line.
    newlines: usize,
    /// The last code written, to tell a regular expression from a division.
    last: String,
    /// A space is due before the next code on this line.
    space_due: bool,
}

impl Writer {
    fn indent_here(&self) -> usize {
        self.stack.last().map_or(0, |f| f.indent)
    }

    fn start_line(&mut self, indent: usize) {
        if self.newlines >= 2 && !self.out.is_empty() {
            self.out.push('\n');
        }
        self.newlines = 0;
        self.line_indent = indent;
        self.opened_indent = None;
    }

    fn write(&mut self, text: &str) {
        self.write_at(text, None);
    }

    /// Writes `text`; a new line gets `indent`, or that of the innermost bracket.
    fn write_at(&mut self, text: &str, indent: Option<usize>) {
        if self.break_due {
            self.break_line();
        }
        if self.line.is_empty() {
            self.start_line(indent.unwrap_or_else(|| self.indent_here()));
        } else if self.space_due {
            self.line.push(' ');
        }
        self.space_due = false;
        self.newlines = 0;
        self.line.push_str(text);
    }

    fn code(&mut self, text: &str) {
        self.write(text);
        text.clone_into(&mut self.last);
    }

    fn verbatim(&mut self, chars: &[char], from: usize, to: usize) -> usize {
        let to = to.min(chars.len());
        self.code(&chars[from..to].iter().collect::<String>());
        to
    }

    fn line_comment(&mut self, text: &str) {
        // A comment after a statement stays on its line.
        self.break_due = false;
        self.write(text);
    }

    fn space(&mut self) {
        self.space_due = !self.line.is_empty();
    }

    fn open(&mut self, c: char) {
        match c {
            '{' => self.space_before_brace(),
            '(' if BEFORE_PAREN.contains(&self.last.as_str()) => self.space_due = true,
            _ => {}
        }
        self.code(&c.to_string());
        let indent = *self.opened_indent.get_or_insert(self.line_indent + 1);
        self.stack.push(Frame { open: c, indent });
    }

    /// Writes the closing bracket `c`; on a line of its own it lines up with the line
    /// that opened it.
    fn close(&mut self, c: char) {
        let frame = self.stack.pop();
        let indent = frame.map_or(0, |f| f.indent.saturating_sub(1));
        let text = c.to_string();
        self.write_at(&text, Some(indent));
        self.last = text;
    }

    /// `{` after code on the same line gets a space: `) {`, `return {`, `= {`.
    fn space_before_brace(&mut self) {
        if !self.line.is_empty() && !self.line.ends_with(['(', '[', '{']) {
            self.space_due = true;
        }
    }

    fn operator(&mut self, op: &str) {
        self.space_due = !self.line.is_empty();
        self.code(op);
        self.space_due = true;
    }

    /// `;` ends a statement, except inside `for (…;…;…)`.
    fn in_statement_list(&self) -> bool {
        self.stack.last().is_none_or(|f| f.open == '{')
    }

    fn regex_may_start(&self) -> bool {
        regex_may_follow(&self.last)
    }

    fn request_break(&mut self) {
        self.break_due = true;
    }

    fn break_line(&mut self) {
        self.break_due = false;
        self.space_due = false;
        if self.line.is_empty() {
            return;
        }
        for _ in 0..self.line_indent {
            self.out.push_str(INDENT);
        }
        self.out.push_str(self.line.trim_end());
        self.out.push('\n');
        self.line.clear();
    }

    fn source_newline(&mut self) {
        if self.line.is_empty() && !self.break_due {
            self.newlines += 1;
        } else {
            self.break_line();
            self.newlines = 1;
        }
    }

    fn finish(mut self) -> String {
        self.break_line();
        self.out.truncate(self.out.trim_end().len());
        self.out
    }
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

/// Index of the first `pattern` at or after `from`.
fn find_from(chars: &[char], from: usize, pattern: &[char]) -> Option<usize> {
    (from..chars.len()).find(|&j| chars[j..].starts_with(pattern))
}

/// The next character that is not whitespace.
fn next_code(chars: &[char], from: usize) -> Option<usize> {
    (from..chars.len()).find(|&j| !chars[j].is_whitespace())
}

/// End (exclusive) of the string starting with the quote at `start`; a string ends at a
/// line break if its quote is missing.
fn quoted(chars: &[char], start: usize) -> usize {
    let quote = chars[start];
    let mut j = start + 1;
    while let Some(&c) = chars.get(j) {
        match c {
            '\\' => j += 2,
            '\n' => return j,
            c if c == quote => return j + 1,
            _ => j += 1,
        }
    }
    chars.len()
}

/// End (exclusive) of the template literal at `start`, `${…}` parts included.
fn template(chars: &[char], start: usize) -> usize {
    let mut j = start + 1;
    while let Some(&c) = chars.get(j) {
        match c {
            '\\' => j += 2,
            '`' => return j + 1,
            '$' if chars.get(j + 1) == Some(&'{') => {
                let mut depth = 0;
                while let Some(&c) = chars.get(j) {
                    match c {
                        '{' => depth += 1,
                        '}' if depth == 1 => break,
                        '}' => depth -= 1,
                        '"' | '\'' => j = quoted(chars, j) - 1,
                        '`' => j = template(chars, j) - 1,
                        _ => {}
                    }
                    j += 1;
                }
                j += 1;
            }
            _ => j += 1,
        }
    }
    chars.len()
}

/// End (exclusive) of the regular expression at `start`, with its flags.
fn regex(chars: &[char], start: usize) -> usize {
    let mut j = start + 1;
    let mut in_class = false;
    while let Some(&c) = chars.get(j) {
        match c {
            '\\' => j += 1,
            '\n' => return j,
            '[' => in_class = true,
            ']' => in_class = false,
            '/' if !in_class => {
                return (j + 1..chars.len())
                    .find(|&k| !is_word(chars[k]))
                    .unwrap_or(chars.len());
            }
            _ => {}
        }
        j += 1;
    }
    chars.len()
}

fn starts_with(chars: &[char], text: &str) -> bool {
    let mut rest = chars.iter();
    text.chars().all(|t| rest.next() == Some(&t))
}

/// What follows a block's `}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AfterBlock {
    /// `)`, `,`, `;`, `.` or `]`: right after it, `});`.
    Bracket,
    /// `else`, `catch`, `finally` or `while`: on the same line after a space.
    Word,
    /// Anything else starts a new line.
    Other,
}

fn after_block(chars: &[char], from: usize) -> AfterBlock {
    let Some(j) = next_code(chars, from) else {
        return AfterBlock::Other;
    };
    if matches!(chars[j], ')' | ',' | ';' | '.' | ']') {
        return AfterBlock::Bracket;
    }
    let word: String = chars[j..].iter().take_while(|c| is_word(**c)).collect();
    if AFTER_BLOCK.contains(&word.as_str()) {
        AfterBlock::Word
    } else {
        AfterBlock::Other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(src: &str, expected: &str) {
        let out = beautify(src);
        assert_eq!(out, expected, "\n--- got ---\n{out}\n");
        assert_eq!(beautify(&out), out, "formatting twice changes nothing");
    }

    #[test]
    fn one_statement_per_line_indented_by_nesting() {
        check(
            r#"test("ok",()=>{expect(res.status).toBe(200);expect(res.json()).toHaveProperty("id");});"#,
            r#"test("ok", () => {
  expect(res.status).toBe(200);
  expect(res.json()).toHaveProperty("id");
});"#,
        );
    }

    #[test]
    fn blocks_keep_else_catch_and_closing_brackets_on_their_line() {
        check(
            "if (a) { x(); } else { y(); }\ntry { z(); } catch (e) { console.log(e); } finally { w(); }\nf({ a: 1 }, [1,2]);",
            "if (a) {\n  x();\n} else {\n  y();\n}\ntry {\n  z();\n} catch (e) {\n  console.log(e);\n} finally {\n  w();\n}\nf({\n  a: 1\n}, [1, 2]);",
        );
    }

    #[test]
    fn badly_indented_lines_are_reindented_and_blank_runs_collapsed() {
        check(
            "      vars.set(\"a\", \"1\");\n\n\n\nif (x) {\n            y();\n        }\n",
            "vars.set(\"a\", \"1\");\n\nif (x) {\n  y();\n}",
        );
    }

    #[test]
    fn strings_templates_regexes_and_comments_are_untouched() {
        check(
            "const s = \"a;{b}, c\"; const t = `x ${ {a:1}.a } ;{`; const r = /[;{]\\/x,y/g;\n// pm.test(\"a\", function () { a;b });\n/* keep {;}\n   this */ z();",
            "const s = \"a;{b}, c\";\nconst t = `x ${ {a:1}.a } ;{`;\nconst r = /[;{]\\/x,y/g;\n// pm.test(\"a\", function () { a;b });\n/* keep {;}\n   this */ z();",
        );
    }

    #[test]
    fn division_is_not_a_regex_and_comments_stay_after_statements() {
        check(
            "const half = res.time / 2; // ms\nconst q = (a) / b / c;",
            "const half = res.time / 2; // ms\nconst q = (a) / b / c;",
        );
    }

    #[test]
    fn for_loops_keep_their_semicolons_on_one_line() {
        check(
            "for (let i = 0; i < 3; i++) { log(i); }",
            "for (let i = 0; i < 3; i++) {\n  log(i);\n}",
        );
    }

    #[test]
    fn operators_keywords_and_colons_get_spaces() {
        check(
            "if(a===1&&b!=null){x+=1;y=-2;z=a?.b??c;o={k:v,n:!m}}else{w=a>=b?1:2}",
            "if (a === 1 && b != null) {\n  x += 1;\n  y = -2;\n  z = a?.b ?? c;\n  o = {\n    k: v, n: !m\n  }\n} else {\n  w = a >= b ? 1: 2\n}",
        );
    }

    /// What a script does must not change: a varied script still parses once beautified.
    #[test]
    fn beautified_scripts_still_parse() {
        let script = r#"const body=res.json();const ids=body.items.map(i=>i.id).filter(id=>id>0);
if(ids.length===0){throw new Error(`no ids in ${res.status}`)}else if(/^2/.test(String(res.status))){vars.set("first",String(ids[0]))}
function check(x){return{ok:x>0,label:'n,'+x}}
for(const id of ids){test("id "+id,()=>{expect(check(id).ok).toBe(true);expect(id/2).toBeGreaterThan(0)})}
try{JSON.parse("{")}catch(e){console.log(e.message)}finally{vars.unset("tmp")}
// pm.test("kept", function () { pm.expect(1).to.equal(1); });
const half=res.time/2,obj={a:[1,2,{b:3}],c:{}};let n=0;do{n++}while(n<3);"#;
        assert!(crate::script::parses(script), "the test script is valid");
        let out = beautify(script);
        assert!(crate::script::parses(&out), "{out}");
        assert!(out.contains("// pm.test(\"kept\", function () { pm.expect(1).to.equal(1); });"));
        assert_eq!(beautify(&out), out);
    }

    /// R-TEST-10
    #[test]
    fn scan_finds_lines_in_code_and_names() {
        let s = scan(
            "const a = 1;\nconst s = `x\ny`; /* c\nd */ f(a.b, \"😀\", ü);\n  if (a)\nx = /re/;",
        );
        let lines: Vec<(bool, Option<char>)> =
            s.lines.iter().map(|l| (l.in_code, l.after)).collect();
        assert_eq!(
            lines,
            [
                (true, None),
                (true, Some(';')),
                (false, None),
                (false, None),
                (true, Some(';')),
                (true, Some(')')),
            ]
        );
        assert_eq!(s.names, ["const", "a", "s", "f", "ü", "if", "x"]);
        assert_eq!(scan("").lines.len(), 1);
        assert!(
            !scan("`open\n").lines[1].in_code,
            "inside a template literal"
        );
        assert!(
            scan("\"open\n").lines[1].in_code,
            "a string ends at the line break"
        );
    }

    /// R-TEST-14
    #[test]
    fn scan_finds_what_to_colour() {
        let src = "const ü = \"s\"; // c\nif (a.if) x = /r/g + 12 / y;";
        let coloured: Vec<(&str, Token)> = scan(src)
            .spans
            .into_iter()
            .map(|(r, t)| (&src[r], t))
            .collect();
        assert_eq!(
            coloured,
            [
                ("const", Token::Keyword),
                ("\"s\"", Token::String),
                ("// c", Token::Comment),
                ("if", Token::Keyword),
                ("/r/g", Token::Regex),
                ("12", Token::Number),
            ],
            "a property named like a keyword is not one"
        );
    }

    #[test]
    fn edge_cases_never_panic() {
        assert_eq!(beautify(""), "");
        assert_eq!(beautify("   \n\n"), "");
        check("const ü😀 = \"ü😀\";", "const ü😀 = \"ü😀\";");
        check("x = {};", "x = {};");
        // Broken scripts: unbalanced brackets, unterminated strings and comments.
        for broken in [
            "}}} a(",
            "a(\"unterminated",
            "/* open",
            "`${",
            "x = /[/",
            "a\\",
        ] {
            let once = beautify(broken);
            assert_eq!(beautify(&once), once, "{broken}");
        }
        let long = "a();".repeat(5_000);
        assert_eq!(beautify(&long).lines().count(), 5_000);
    }
}
