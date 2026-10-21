//! Translating Postman scripts to Tortuga's script API (R-COL-19, IDEA-48).
//!
//! The calls most collections use are rewritten: `pm.test`, `pm.expect(…)` with Chai's
//! common assertions, `pm.response.to.have.status(…)`/`.header(…)`/`.be.ok`, the
//! response's code, status, time, size, headers, cookies and body, getting and setting
//! variables (also the data row's, and `replaceIn`), `pm.info` (R-COL-21) and
//! `setNextRequest` as `workflow.next`/`stop` (R-FLOW-6). A translated script runs only when nothing of Postman's API is left and
//! it still parses; otherwise it is kept as comments, translated as far as it went.
//!
//! The scanner only looks at code outside strings and comments, and only splits the
//! text at ASCII characters, so any text is safe.

use crate::script;

/// A Postman script in Tortuga's script API.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Translation {
    /// Everything was translated: the script can run as it is.
    Complete(String),
    /// Parts still use Postman's API, or the script doesn't parse: what could be
    /// translated, to finish by hand.
    Partial(String),
}

/// The note above a translated script.
pub(super) const NOTE: &str =
    "// Translated from Postman's `pm` API by the import: check that it tests what it did.\n";

/// Names a script may only use in Postman. A script that still uses one is not run.
const POSTMAN_GLOBALS: [&str; 14] = [
    "pm",
    "postman",
    "tests",
    "request",
    "responseBody",
    "responseCode",
    "responseHeaders",
    "responseTime",
    "responseCookies",
    "require",
    "CryptoJS",
    "cheerio",
    "xml2Json",
    "_",
];

/// Postman names that are renamed as they are. Each ends where an identifier ends.
const RENAMES: [(&str, &str); 18] = [
    ("pm.test", "test"),
    ("pm.response.code", "res.status"),
    ("pm.response.status", "res.statusText"),
    ("pm.response.responseTime", "res.time"),
    ("pm.response.responseSize", "res.size"),
    ("pm.response.json", "res.json"),
    ("pm.response.headers.get", "res.header"),
    ("pm.request.method", "req.method"),
    ("pm.request.url", "req.url"),
    ("postman.getEnvironmentVariable", "vars.get"),
    ("postman.setEnvironmentVariable", "vars.set"),
    ("pm.variables.replaceIn", "vars.replaceIn"),
    ("pm.cookies.get", "res.cookie"),
    ("pm.info.eventName", "info.event"),
    ("pm.info.requestName", "info.requestName"),
    ("pm.info.iteration", "info.iteration"),
    ("pm.info.iterationCount", "info.iterationCount"),
    ("pm.iterationData.get", "vars.get"),
];

pub(super) fn translate(code: &str) -> Translation {
    let translated = rewrite(code);
    if uses_postman_api(&translated) || !script::parses(&translated) {
        Translation::Partial(translated)
    } else {
        Translation::Complete(translated)
    }
}

/// `code` with every call this module knows translated.
fn rewrite(code: &str) -> String {
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
        match starts_word.then(|| translate_at(&code[i..])).flatten() {
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

/// The translation of the call that `s` starts with, and how much of `s` it replaces.
fn translate_at(s: &str) -> Option<(usize, String)> {
    if !(s.starts_with("pm.") || s.starts_with("postman.")) {
        return None;
    }
    expect_call(s)
        .or_else(|| response_assertion(s))
        .or_else(|| variables_call(s))
        .or_else(|| {
            let end = empty_call(s, word(s, "pm.response.text")?)?;
            Some((end, "res.body".to_owned()))
        })
        .or_else(|| {
            let end = word(s, "postman.clearEnvironmentVariable")?;
            Some((end, "vars.unset".to_owned()))
        })
        .or_else(|| cookies_call(s))
        .or_else(|| next_request(s))
        .or_else(|| {
            RENAMES
                .iter()
                .find_map(|(from, to)| Some((word(s, from)?, (*to).to_owned())))
        })
}

// ------------------------------------------------------------------ calls

/// `pm.expect(actual).to.….matcher(…)` → `expect(actual).matcher(…)`.
fn expect_call(s: &str) -> Option<(usize, String)> {
    let (i, args) = call_args(s, word(s, "pm.expect")?)?;
    // A message as the second argument has no place in `expect`.
    let [actual] = args.as_slice() else {
        return None;
    };
    let chain = chain(s, i)?;
    let (end, subject, matcher) = assertion(s, &chain, actual)?;
    finish(s, end, &subject, chain.negate, &matcher)
}

/// `pm.response.to.have.status(200)`, `.header(name[, value])` and `.be.ok`.
fn response_assertion(s: &str) -> Option<(usize, String)> {
    let chain = chain(s, word(s, "pm.response")?)?;
    let (end, args) = match chain.name {
        "ok" => (chain.end, Vec::new()),
        _ => call_args(s, chain.end)?,
    };
    let (subject, matcher) = match (chain.name, args.as_slice()) {
        ("ok", []) => ("res.status".to_owned(), "toBe(200)".to_owned()),
        ("status", [code]) if is_string_literal(code) => {
            ("res.statusText".to_owned(), format!("toBe({code})"))
        }
        ("status", [code]) => ("res.status".to_owned(), format!("toBe({code})")),
        ("header", [name]) => (format!("res.header({name})"), "toBeDefined()".to_owned()),
        ("header", [name, value]) => (format!("res.header({name})"), format!("toBe({value})")),
        _ => return None,
    };
    finish(s, end, &subject, chain.negate, &matcher)
}

/// `pm.environment.set(…)` and the like → `vars.set(…)`. Postman's globals are
/// workspace variables (R-WS-8).
fn variables_call(s: &str) -> Option<(usize, String)> {
    let rest = s.strip_prefix("pm.")?;
    let (scope, rest) = rest.split_once('.')?;
    if scope == "iterationData" {
        // The data row's values are variables in a workflow run (R-FLOW-4).
        let end = word(rest, "has")?;
        return Some(("pm.iterationData.".len() + end, "vars.has".to_owned()));
    }
    let op = ["get", "set", "unset", "has"]
        .into_iter()
        .find(|op| word(rest, op).is_some())?;
    let at = "pm.".len() + scope.len() + 1 + op.len();
    match (scope, op) {
        ("environment" | "variables" | "collectionVariables", _) | ("globals", "get" | "has") => {
            Some((at, format!("vars.{op}")))
        }
        ("globals", _) => {
            let (end, args) = call_args(s, at)?;
            let (name, rest) = args.split_first()?;
            let name = match name.strip_prefix(['"', '\'']) {
                Some(_) if is_string_literal(name) => {
                    format!("{}workspace.{}", &name[..1], &name[1..])
                }
                _ => format!("\"workspace.\" + {}", grouped(name)),
            };
            let args: Vec<&str> = std::iter::once(name.as_str())
                .chain(rest.iter().map(String::as_str))
                .collect();
            Some((end, format!("vars.{op}({})", args.join(", "))))
        }
        _ => None,
    }
}

/// `pm.cookies.has(name)` and `pm.cookies.toObject()`: the cookies the response set
/// (R-COL-21).
fn cookies_call(s: &str) -> Option<(usize, String)> {
    if let Some(end) = word(s, "pm.cookies.toObject").and_then(|i| empty_call(s, i)) {
        return Some((end, "res.cookies".to_owned()));
    }
    let (end, args) = call_args(s, word(s, "pm.cookies.has")?)?;
    let [name] = args.as_slice() else {
        return None;
    };
    Some((end, format!("(res.cookie({name}) !== undefined)")))
}

/// `postman.setNextRequest(name)` → `workflow.next(name)`, and `(null)` →
/// `workflow.stop()` (R-FLOW-6).
fn next_request(s: &str) -> Option<(usize, String)> {
    let at =
        word(s, "postman.setNextRequest").or_else(|| word(s, "pm.execution.setNextRequest"))?;
    let (end, args) = call_args(s, at)?;
    let [name] = args.as_slice() else {
        return None;
    };
    if name.trim() == "null" {
        Some((end, "workflow.stop()".to_owned()))
    } else {
        Some((end, format!("workflow.next({name})")))
    }
}

/// The words of a Chai chain such as `.to.not.have.property`, up to the one that asserts.
struct Chain<'a> {
    /// After the asserting word.
    end: usize,
    name: &'a str,
    negate: bool,
    deep: bool,
}

fn chain(s: &str, mut i: usize) -> Option<Chain<'_>> {
    let mut negate = false;
    let mut deep = false;
    loop {
        let (next, name) = member(s, i)?;
        i = next;
        match name {
            "to" | "be" | "been" | "is" | "that" | "which" | "and" | "has" | "have" | "with"
            | "at" | "of" | "same" | "does" | "still" => {}
            "not" => negate = !negate,
            "deep" => deep = true,
            _ => {
                return Some(Chain {
                    end: i,
                    name,
                    negate,
                    deep,
                });
            }
        }
    }
}

/// The subject and Jest matcher for a Chai assertion on `actual`, and where it ends.
fn assertion(s: &str, chain: &Chain<'_>, actual: &str) -> Option<(usize, String, String)> {
    let property = |matcher: &str| Some((chain.end, actual.to_owned(), matcher.to_owned()));
    match chain.name {
        "true" => return property("toBe(true)"),
        "false" => return property("toBe(false)"),
        "null" => return property("toBeNull()"),
        "undefined" => return property("toBeUndefined()"),
        "ok" => return property("toBeTruthy()"),
        // Chai's `exist` fails for `null` too.
        "exist" => {
            let subject = format!("{} ?? undefined", grouped(actual));
            return Some((chain.end, subject, "toBeDefined()".to_owned()));
        }
        _ => {}
    }
    let (end, args) = call_args(s, chain.end)?;
    let matcher = match chain.name {
        "equal" | "equals" | "eq" if chain.deep => "toEqual",
        "equal" | "equals" | "eq" => "toBe",
        "eql" | "eqls" => "toEqual",
        "include" | "includes" | "contain" | "contains" => "toContain",
        "property" => "toHaveProperty",
        "lengthOf" | "length" => "toHaveLength",
        "match" | "matches" => "toMatch",
        "above" | "gt" | "greaterThan" => "toBeGreaterThan",
        "below" | "lt" | "lessThan" => "toBeLessThan",
        "least" | "gte" => "toBeGreaterThanOrEqual",
        "most" | "lte" => "toBeLessThanOrEqual",
        "a" | "an" => return type_check(end, &args, actual),
        _ => return None,
    };
    let wanted = if matcher == "toHaveProperty" {
        1..=2
    } else {
        1..=1
    };
    wanted.contains(&args.len()).then(|| {
        (
            end,
            actual.to_owned(),
            format!("{matcher}({})", args.join(", ")),
        )
    })
}

/// `.to.be.a("string")` → `expect(typeof x).toBe("string")`.
fn type_check(end: usize, args: &[String], actual: &str) -> Option<(usize, String, String)> {
    let [kind] = args else { return None };
    if !is_string_literal(kind) {
        return None;
    }
    let name = kind.get(1..kind.len() - 1)?.to_ascii_lowercase();
    let subject = match name.as_str() {
        "array" => format!("Array.isArray({actual})"),
        // `typeof` and Chai disagree on objects (arrays, `null`).
        "string" | "number" | "boolean" | "undefined" | "function" => {
            format!("typeof {}", grouped(actual))
        }
        _ => return None,
    };
    let matcher = if name == "array" {
        "toBe(true)".to_owned()
    } else {
        format!("toBe(\"{name}\")")
    };
    Some((end, subject, matcher))
}

/// `expect(subject)[.not].matcher`, unless the chain goes on (`.and.…`), which is left
/// for the user.
fn finish(
    s: &str,
    end: usize,
    subject: &str,
    negate: bool,
    matcher: &str,
) -> Option<(usize, String)> {
    if member(s, end).is_some() {
        return None;
    }
    let not = if negate { ".not" } else { "" };
    Some((end, format!("expect({subject}){not}.{matcher}")))
}

// ------------------------------------------------------------------ scanning

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

/// Whether a name that is not a property (`x.pm`) starts at `i`, an ASCII byte.
fn starts_name(code: &str, i: usize) -> bool {
    let before = code.get(..i).unwrap_or_default();
    !before
        .bytes()
        .next_back()
        .is_some_and(|b| is_word_byte(b) || !b.is_ascii())
        && !before.trim_end().ends_with('.')
}

/// The length of `name` when `s` starts with it as a whole word.
fn word(s: &str, name: &str) -> Option<usize> {
    let rest = s.strip_prefix(name)?;
    (!rest.bytes().next().is_some_and(is_word_byte)).then_some(name.len())
}

fn skip_space(s: &str, i: usize) -> usize {
    s.get(i..)
        .map_or(i, |rest| i + (rest.len() - rest.trim_start().len()))
}

/// `.name` at `i`, with optional spaces: where it ends and the name.
fn member(s: &str, i: usize) -> Option<(usize, &str)> {
    let i = skip_space(s, i);
    let rest = s.get(i..)?.strip_prefix('.')?;
    let start = i + 1 + (rest.len() - rest.trim_start().len());
    let name = s.get(start..)?;
    let len = name.bytes().take_while(|&b| is_word_byte(b)).count();
    (len > 0).then(|| (start + len, &name[..len]))
}

/// `( … )` at `i`, with optional spaces: where it ends and the arguments, translated.
fn call_args(s: &str, i: usize) -> Option<(usize, Vec<String>)> {
    let open = skip_space(s, i);
    if s.as_bytes().get(open) != Some(&b'(') {
        return None;
    }
    let end = group_end(s, open)?;
    let args = split_args(&s[open + 1..end - 1])
        .into_iter()
        .map(rewrite)
        .collect();
    Some((end, args))
}

/// After `()` at `i`.
fn empty_call(s: &str, i: usize) -> Option<usize> {
    let (end, args) = call_args(s, i)?;
    args.is_empty().then_some(end)
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
fn literal_end(s: &str, i: usize) -> Option<usize> {
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

fn is_string_literal(s: &str) -> bool {
    let bytes = s.as_bytes();
    match (bytes.first(), bytes.last()) {
        (Some(&first @ (b'"' | b'\'')), Some(&last)) => {
            bytes.len() >= 2 && last == first && literal_end(s, 0) == Some(s.len())
        }
        _ => false,
    }
}

/// `expr` in parentheses unless it is a plain name or property path.
fn grouped(expr: &str) -> String {
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

/// Whether code outside strings and comments uses a name only Postman has.
fn uses_postman_api(code: &str) -> bool {
    let bytes = code.as_bytes();
    let mut i = 0;
    while let Some(&byte) = bytes.get(i) {
        if let Some(end) = literal_end(code, i) {
            i = end;
            continue;
        }
        if is_word_byte(byte) && starts_name(code, i) {
            let len = bytes[i..].iter().take_while(|&&b| is_word_byte(b)).count();
            if POSTMAN_GLOBALS.contains(&&code[i..i + len]) {
                return true;
            }
            i += len;
        } else {
            i += 1;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::time::Duration;

    use super::*;
    use crate::model::{HttpMethod, KeyValue, Request, Response};
    use crate::script::{Phase, ScriptRun};

    fn complete(code: &str) -> String {
        match translate(code) {
            Translation::Complete(code) => code,
            Translation::Partial(code) => panic!("not translated completely:\n{code}"),
        }
    }

    fn partial(code: &str) -> String {
        match translate(code) {
            Translation::Partial(code) => code,
            Translation::Complete(code) => panic!("translated completely:\n{code}"),
        }
    }

    fn run_test(script: &str) -> ScriptRun {
        let response = Response {
            status: 200,
            status_text: "OK".into(),
            headers: vec![KeyValue::new("Content-Type", "application/json")],
            body: br#"{"id": 7, "name": "Ann", "tags": ["a"], "gone": null}"#.to_vec(),
            elapsed: Duration::from_millis(42),
        };
        let request = Request::new(HttpMethod::Get, "https://api.test/users");
        crate::script::run(
            script,
            Phase::Test,
            &BTreeMap::new(),
            &request,
            Some(&response),
        )
    }

    /// R-COL-19: a typical Postman test script becomes one that runs and still tests
    /// the same things.
    #[test]
    fn a_typical_test_script_runs_after_translation() {
        let postman = r#"
pm.test("Status code is 200", function () {
    pm.response.to.have.status(200);
});
pm.test("Body", () => {
    const json = pm.response.json();
    pm.expect(json.id).to.eql(7);
    pm.expect(json.name).to.be.a('string');
    pm.expect(json.tags).to.be.an("array").that;
    pm.expect(json).to.have.property("name", "Ann");
    pm.expect(json.tags).to.include("a");
    pm.expect(json.gone).to.not.exist;
    pm.expect(pm.response.responseTime).to.be.below(1000);
    pm.expect(pm.response.text()).to.match(/Ann/);
    pm.response.to.have.header("Content-Type");
    pm.response.to.be.ok;
});
pm.test("Fails", () => pm.expect(pm.response.code).to.not.equal(200));
pm.environment.set("userId", pm.response.json().id);
"#;
        // `.that` dangling after `an("array")` keeps that one call untranslated.
        assert!(partial(postman).contains(r#"pm.expect(json.tags).to.be.an("array").that;"#));

        let postman = postman.replace(".that;", ";");
        let tortuga = complete(&postman);
        assert!(tortuga.contains(
            "test(\"Status code is 200\", function () {\n    expect(res.status).toBe(200);\n});"
        ));
        for line in [
            "const json = res.json();",
            "expect(json.id).toEqual(7);",
            "expect(typeof json.name).toBe(\"string\");",
            "expect(Array.isArray(json.tags)).toBe(true);",
            "expect(json).toHaveProperty(\"name\", \"Ann\");",
            "expect(json.tags).toContain(\"a\");",
            "expect(json.gone ?? undefined).not.toBeDefined();",
            "expect(res.time).toBeLessThan(1000);",
            "expect(res.body).toMatch(/Ann/);",
            "expect(res.header(\"Content-Type\")).toBeDefined();",
            "expect(res.status).toBe(200);",
            "expect(res.status).not.toBe(200)",
            "vars.set(\"userId\", res.json().id);",
        ] {
            assert!(tortuga.contains(line), "missing {line:?} in\n{tortuga}");
        }

        let run = run_test(&tortuga);
        assert_eq!(run.error, None);
        let results: Vec<(&str, bool)> = run
            .tests
            .iter()
            .map(|t| (t.name.as_str(), t.passed))
            .collect();
        assert_eq!(
            results,
            [
                ("Status code is 200", true),
                ("Body", true),
                ("Fails", false)
            ],
            "{:?}",
            run.tests
        );
        assert_eq!(run.vars["userId"].as_deref(), Some("7"));
    }

    /// R-COL-19: variables of every Postman scope; globals are workspace variables.
    #[test]
    fn variables_of_every_scope() {
        let tortuga = complete(
            "pm.variables.set('a', pm.collectionVariables.get('b'));\n\
             if (pm.environment.has('c')) { pm.environment.unset('c'); }\n\
             pm.globals.set('token', 't');\n\
             pm.globals.unset(name + 1);\n\
             postman.setEnvironmentVariable('d', postman.getEnvironmentVariable('e'));\n\
             postman.clearEnvironmentVariable('d');\n\
             console.log(pm.globals.get('token'), pm.request.method, pm.request.url.toString());",
        );
        assert_eq!(
            tortuga,
            "vars.set('a', vars.get('b'));\n\
             if (vars.has('c')) { vars.unset('c'); }\n\
             vars.set('workspace.token', 't');\n\
             vars.unset(\"workspace.\" + (name + 1));\n\
             vars.set('d', vars.get('e'));\n\
             vars.unset('d');\n\
             console.log(vars.get('token'), req.method, req.url.toString());"
        );
    }

    /// R-COL-19: Chai's flags and the deep, numeric and boolean assertions.
    #[test]
    fn chai_assertions() {
        let cases = [
            ("pm.expect(a).to.equal(1)", "expect(a).toBe(1)"),
            (
                "pm.expect(a).to.deep.equal({x: 1})",
                "expect(a).toEqual({x: 1})",
            ),
            ("pm.expect(a).not.to.be.true", "expect(a).not.toBe(true)"),
            ("pm.expect(a).to.be.false", "expect(a).toBe(false)"),
            ("pm.expect(a).to.be.null", "expect(a).toBeNull()"),
            ("pm.expect(a).to.be.undefined", "expect(a).toBeUndefined()"),
            ("pm.expect(a || b).to.be.ok", "expect(a || b).toBeTruthy()"),
            (
                "pm.expect(a || b).to.exist",
                "expect((a || b) ?? undefined).toBeDefined()",
            ),
            (
                "pm.expect(a).to.have.lengthOf(2)",
                "expect(a).toHaveLength(2)",
            ),
            (
                "pm.expect(a).to.be.above(2)",
                "expect(a).toBeGreaterThan(2)",
            ),
            (
                "pm.expect(a).to.be.at.least(2)",
                "expect(a).toBeGreaterThanOrEqual(2)",
            ),
            (
                "pm.expect(a).to.be.at.most(2)",
                "expect(a).toBeLessThanOrEqual(2)",
            ),
            (
                "pm.expect(a, 'msg').to.equal(1)",
                "pm.expect(a, 'msg').to.equal(1)",
            ),
            (
                "pm.expect(a).to.be.a('object')",
                "pm.expect(a).to.be.a('object')",
            ),
            (
                "pm.response.to.have.status(\"Created\")",
                "expect(res.statusText).toBe(\"Created\")",
            ),
            (
                "pm.response.to.have.header('X-Id', '1')",
                "expect(res.header('X-Id')).toBe('1')",
            ),
            ("pm.response.headers.get('X-Id')", "res.header('X-Id')"),
            (
                "pm.expect(pm.response.status).to.equal(\"OK\")",
                "expect(res.statusText).toBe(\"OK\")",
            ),
        ];
        for (postman, tortuga) in cases {
            assert_eq!(rewrite(postman), tortuga, "translating {postman:?}");
        }
    }

    /// R-COL-19: what is not translated keeps the script from running, translated as far
    /// as it goes.
    #[test]
    fn untranslated_calls_keep_the_script_from_running() {
        let code =
            partial("pm.environment.set('a', 1);\npm.sendRequest('https://x.test', () => {});");
        assert_eq!(
            code,
            "vars.set('a', 1);\npm.sendRequest('https://x.test', () => {});"
        );
        for postman in [
            "tests['ok'] = responseCode.code === 200;",
            "const id = _.get(json, 'id');",
            "pm.expect(a).to.equal(1).and.to.be.above(0);",
            "pm.expect(a).to.equal(1",
            "pm.test(\"broken\", () => {",
        ] {
            partial(postman);
        }
    }

    /// R-COL-19: strings and comments are left alone, also with non-ASCII text.
    #[test]
    fn strings_and_comments_are_left_alone() {
        let postman = "// pm.test is Postman's\n\
                       /* pm.expect(x) */\n\
                       pm.test(\"pm.response ü😀\", () => console.log('pm.environment', `pm.test`));\n";
        assert_eq!(
            complete(postman),
            "// pm.test is Postman's\n\
             /* pm.expect(x) */\n\
             test(\"pm.response ü😀\", () => console.log('pm.environment', `pm.test`));\n"
        );
        assert_eq!(complete(""), "");
        assert_eq!(
            rewrite("😀pm.test x.pm.test x. pm.test"),
            "😀pm.test x.pm.test x. pm.test"
        );
        // Names that only look like Postman's are not translated.
        assert_eq!(
            complete("const x = y.pm; mypm.test(1);"),
            "const x = y.pm; mypm.test(1);"
        );
    }

    /// R-COL-21: cookies, `pm.info`, `replaceIn` and the data row translate to
    /// Tortuga's API (R-TEST-16), and the result runs and finds the same values.
    #[test]
    fn cookies_info_replace_in_and_data_rows() {
        let tortuga = complete(
            "pm.test('cookies', () => {\n\
             \x20 pm.expect(pm.cookies.get('session')).to.equal('abc');\n\
             \x20 pm.expect(pm.cookies.has('session')).to.be.true;\n\
             \x20 pm.expect(pm.cookies.has(\"nope\")).to.be.false;\n\
             \x20 pm.expect(pm.cookies.toObject()).to.have.property('theme', 'dark');\n\
             });\n\
             pm.test('info', () => {\n\
             \x20 pm.expect(pm.info.eventName).to.equal('test');\n\
             \x20 pm.expect(pm.info.requestName).to.equal('Log in');\n\
             \x20 pm.expect(pm.info.iteration).to.equal(0);\n\
             \x20 pm.expect(pm.info.iterationCount).to.equal(1);\n\
             });\n\
             pm.environment.set('greeting', pm.variables.replaceIn('Hi {{user}}, {{nobody}}'));\n\
             if (pm.iterationData.has('user')) { pm.environment.set('who', pm.iterationData.get('user')); }\n",
        );
        for line in [
            "expect(res.cookie('session')).toBe('abc');",
            "expect((res.cookie('session') !== undefined)).toBe(true);",
            "expect(res.cookies).toHaveProperty('theme', 'dark');",
            "expect(info.event).toBe('test');",
            "expect(info.iterationCount).toBe(1);",
            "vars.set('greeting', vars.replaceIn('Hi {{user}}, {{nobody}}'));",
            "if (vars.has('user')) { vars.set('who', vars.get('user')); }",
        ] {
            assert!(tortuga.contains(line), "missing {line:?} in\n{tortuga}");
        }

        let response = Response {
            status: 200,
            status_text: "OK".into(),
            headers: vec![
                KeyValue::new("Set-Cookie", "session=abc; Path=/; HttpOnly"),
                KeyValue::new("set-cookie", "theme=dark"),
            ],
            body: Vec::new(),
            elapsed: Duration::from_millis(5),
        };
        let mut request = Request::new(HttpMethod::Post, "https://api.test/login");
        request.name = "Log in".into();
        let vars = BTreeMap::from([("user".to_owned(), "Zoë 😀".to_owned())]);
        let run = crate::script::run(&tortuga, Phase::Test, &vars, &request, Some(&response));
        assert_eq!(run.error, None);
        assert!(run.tests.iter().all(|t| t.passed), "{:?}", run.tests);
        assert_eq!(run.tests.len(), 2);
        assert_eq!(
            run.vars["greeting"].as_deref(),
            Some("Hi Zoë 😀, {{nobody}}")
        );
        assert_eq!(run.vars["who"].as_deref(), Some("Zoë 😀"));
    }

    /// R-COL-21, R-FLOW-6: `setNextRequest` becomes `workflow.next`, and `null` ends
    /// the run with `workflow.stop()`.
    #[test]
    fn set_next_request_controls_the_workflow() {
        assert_eq!(
            complete("postman.setNextRequest('Get user');"),
            "workflow.next('Get user');"
        );
        assert_eq!(
            complete("if (pm.response.code === 404) { pm.execution.setNextRequest(null); }"),
            "if (res.status === 404) { workflow.stop(); }"
        );
        let run = run_test(&complete("postman.setNextRequest(pm.info.requestName);"));
        assert_eq!(
            run.flow,
            Some(crate::script::FlowControl::Next("Untitled request".into()))
        );
        // `pm.info.requestId` has no counterpart: kept for the user.
        assert!(partial("console.log(pm.info.requestId);").contains("pm.info.requestId"));
    }
}
