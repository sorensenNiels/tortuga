//! Translating Bruno scripts to Tortuga's script API (R-COL-33, IDEA-91).
//!
//! Bruno's `test` and `expect` (Chai) are Tortuga's too, and so are `res.status`,
//! `res.statusText`, `res.headers`, `req.method`, `req.url`, `req.headers` and
//! `req.setHeader`. Rewritten: Bruno's variables (`bru.setVar`, `getEnvVar`, …) as
//! `vars`, `bru.interpolate` as `vars.replaceIn`, the response's and request's getters
//! (`res.getStatus()`, `res.getHeader(…)`, `req.getUrl()`, …), `bru.setNextRequest` and
//! `bru.runner` as `workflow` (R-FLOW-6), and the request's name as `info`. In Bruno
//! `res.body` is the parsed JSON, so it becomes `res.json()`.
//!
//! A translated script runs only when nothing of Bruno's API is left (`bru`,
//! `require`, `res(…)`, a getter Tortuga lacks) and it still parses; otherwise it is
//! kept as comments, translated as far as it went.

use crate::import::js::{
    Translation, call_args, empty_call, is_string_literal, literal_end, member, rewrite,
    starts_name, uses_any, word,
};
use crate::script;

/// Names a script may only use in Bruno.
const BRUNO_GLOBALS: [&str; 2] = ["bru", "require"];

/// What Tortuga's `res` and `req` have (see the script prelude).
const RES_MEMBERS: [&str; 10] = [
    "status",
    "statusText",
    "headers",
    "cookies",
    "body",
    "time",
    "size",
    "header",
    "cookie",
    "json",
];
const REQ_MEMBERS: [&str; 5] = ["method", "url", "headers", "body", "setHeader"];

/// Bruno functions that are Tortuga's under another name, with the same arguments.
const RENAMES: [(&str, &str); 15] = [
    ("bru.setVar", "vars.set"),
    ("bru.getVar", "vars.get"),
    ("bru.hasVar", "vars.has"),
    ("bru.deleteVar", "vars.unset"),
    ("bru.setEnvVar", "vars.set"),
    ("bru.getEnvVar", "vars.get"),
    ("bru.hasEnvVar", "vars.has"),
    ("bru.deleteEnvVar", "vars.unset"),
    // `{{process.env.X}}` is the variable `X` after the import (R-COL-31).
    ("bru.getProcessEnv", "vars.get"),
    ("bru.getCollectionVar", "vars.get"),
    ("bru.getFolderVar", "vars.get"),
    ("bru.getRequestVar", "vars.get"),
    ("bru.getGlobalEnvVar", "vars.get"),
    ("bru.interpolate", "vars.replaceIn"),
    ("res.getHeader", "res.header"),
];

/// Getters called without arguments, and what they are in Tortuga.
const GETTERS: [(&str, &str); 11] = [
    ("res.getStatus", "res.status"),
    ("res.getStatusText", "res.statusText"),
    ("res.getHeaders", "res.headers"),
    ("res.getBody", "res.json()"),
    ("res.getResponseTime", "res.time"),
    ("req.getUrl", "req.url"),
    ("req.getMethod", "req.method"),
    ("req.getHeaders", "req.headers"),
    ("req.getBody", "req.body"),
    ("req.getName", "info.requestName"),
    ("bru.getRequestName", "info.requestName"),
];

pub(super) fn translate(code: &str) -> Translation {
    let translated = rewrite(code, &translate_at);
    if uses_any(&translated, &BRUNO_GLOBALS)
        || uses_missing_member(&translated)
        || !script::parses(&translated)
    {
        Translation::Partial(translated)
    } else {
        Translation::Complete(translated)
    }
}

/// The translation of the Bruno call that `s` starts with, and how much it replaces.
fn translate_at(s: &str) -> Option<(usize, String)> {
    if !["bru.", "res.", "req."].iter().any(|p| s.starts_with(p)) {
        return None;
    }
    GETTERS
        .iter()
        .find_map(|(from, to)| Some((empty_call(s, word(s, from)?)?, (*to).to_owned())))
        .or_else(|| {
            RENAMES
                .iter()
                .find_map(|(from, to)| Some((word(s, from)?, (*to).to_owned())))
        })
        .or_else(|| Some((word(s, "res.body")?, "res.json()".to_owned())))
        .or_else(|| Some((word(s, "res.responseTime")?, "res.time".to_owned())))
        .or_else(|| request_header(s))
        .or_else(|| global_set(s))
        .or_else(|| flow(s))
}

/// `req.getHeader(name)` → Tortuga's headers, which are keyed in lower case.
fn request_header(s: &str) -> Option<(usize, String)> {
    let (end, args) = call_args(s, word(s, "req.getHeader")?, &translate_at)?;
    let [name] = args.as_slice() else {
        return None;
    };
    Some((end, format!("req.headers[String({name}).toLowerCase()]")))
}

/// `bru.setGlobalEnvVar(name, value)` → a workspace variable (R-WS-8).
fn global_set(s: &str) -> Option<(usize, String)> {
    let (end, args) = call_args(s, word(s, "bru.setGlobalEnvVar")?, &translate_at)?;
    let [name, value] = args.as_slice() else {
        return None;
    };
    let name = match name.get(..1) {
        Some(quote) if is_string_literal(name) => format!("{quote}workspace.{}", &name[1..]),
        _ => format!("\"workspace.\" + ({name})"),
    };
    Some((end, format!("vars.set({name}, {value})")))
}

/// `bru.setNextRequest(name)` and `bru.runner.…` → `workflow` (R-FLOW-6).
fn flow(s: &str) -> Option<(usize, String)> {
    if let Some(end) = word(s, "bru.runner.stopExecution").and_then(|i| empty_call(s, i)) {
        return Some((end, "workflow.stop()".to_owned()));
    }
    let at = word(s, "bru.setNextRequest").or_else(|| word(s, "bru.runner.setNextRequest"))?;
    let (end, args) = call_args(s, at, &translate_at)?;
    let [name] = args.as_slice() else {
        return None;
    };
    if name.trim() == "null" {
        Some((end, "workflow.stop()".to_owned()))
    } else {
        Some((end, format!("workflow.next({name})")))
    }
}

/// Whether code uses `res` or `req` in a way Tortuga's don't have: a member they lack,
/// or Bruno's `res(…)` query.
fn uses_missing_member(code: &str) -> bool {
    let bytes = code.as_bytes();
    let mut i = 0;
    while let Some(&byte) = bytes.get(i) {
        if let Some(end) = literal_end(code, i) {
            i = end;
            continue;
        }
        if byte.is_ascii_alphabetic() && starts_name(code, i) {
            for (object, members) in [("res", &RES_MEMBERS[..]), ("req", &REQ_MEMBERS[..])] {
                let Some(len) = word(&code[i..], object) else {
                    continue;
                };
                let after = i + len;
                if code[after..].trim_start().starts_with('(') {
                    return true;
                }
                if let Some((_, name)) = member(code, after)
                    && !members.contains(&name)
                {
                    return true;
                }
            }
        }
        i += 1;
    }
    false
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::time::Duration;

    use super::*;
    use crate::model::{HttpMethod, KeyValue, Request, Response};

    fn complete(code: &str) -> String {
        match translate(code) {
            Translation::Complete(code) => code,
            Translation::Partial(code) => panic!("partial: {code}"),
        }
    }

    fn partial(code: &str) -> String {
        match translate(code) {
            Translation::Partial(code) => code,
            Translation::Complete(code) => panic!("complete: {code}"),
        }
    }

    /// R-COL-33: Bruno's calls become Tortuga's; strings, comments and properties are
    /// left alone.
    #[test]
    fn calls_are_translated() {
        assert_eq!(
            complete("bru.setVar(\"token\", res.body.token);"),
            "vars.set(\"token\", res.json().token);"
        );
        assert_eq!(
            complete("const s = res.getStatus(); const t = res.getResponseTime();"),
            "const s = res.status; const t = res.time;"
        );
        assert_eq!(
            complete("expect(res.getHeader('content-type')).to.contain('json');"),
            "expect(res.header('content-type')).to.contain('json');"
        );
        assert_eq!(
            complete("const id = req.getHeader(\"X-Id\") + req.getUrl() + req.getMethod();"),
            "const id = req.headers[String(\"X-Id\").toLowerCase()] + req.url + req.method;"
        );
        assert_eq!(
            complete("bru.setEnvVar(\"a\", bru.interpolate(\"{{b}}\")); bru.deleteVar(\"c\");"),
            "vars.set(\"a\", vars.replaceIn(\"{{b}}\")); vars.unset(\"c\");"
        );
        assert_eq!(
            complete("bru.setGlobalEnvVar('g', 1); bru.setGlobalEnvVar(name, 2);"),
            "vars.set('workspace.g', 1); vars.set(\"workspace.\" + (name), 2);"
        );
        assert_eq!(
            complete(
                "bru.setNextRequest(\"Login\"); bru.setNextRequest(null); bru.runner.stopExecution();"
            ),
            "workflow.next(\"Login\"); workflow.stop(); workflow.stop();"
        );
        assert_eq!(
            complete("console.log(bru.getRequestName(), bru.getProcessEnv('KEY'));"),
            "console.log(info.requestName, vars.get('KEY'));"
        );
        let untouched =
            "// bru.setVar(\"x\") in a comment\nconst s = \"res.body ü😀\";\nfoo.bru.setVar(1);";
        assert_eq!(complete(untouched), untouched);
    }

    /// R-COL-33: what Tortuga has no counterpart for keeps the script from running.
    #[test]
    fn what_is_left_keeps_the_script_as_comments() {
        assert!(partial("bru.sendRequest({ url: 'x' });").contains("bru.sendRequest"));
        assert!(partial("const _ = require('lodash');").contains("require"));
        assert!(partial("bru.setVar('a', 1); bru.sleep(100);").starts_with("vars.set('a', 1);"));
        partial("expect(res('items.id')).to.equal(1);");
        partial("req.setBody({ a: 1 });");
        partial("const size = res.getSize();");
        partial("vars.set(\"a\", ;");
    }

    /// R-COL-33: a typical Bruno test script runs in Tortuga after translation.
    #[test]
    fn a_translated_test_script_runs() {
        let code = complete(
            "test(\"created\", function() {\n  expect(res.getStatus()).to.equal(201);\n  expect(res.body.name).to.eql(\"Ann ü😀\");\n  expect(res.getHeader(\"location\")).to.contain(\"/users/\");\n});\nbru.setVar(\"userId\", res.getBody().id);\n",
        );
        let request = Request::new(HttpMethod::Post, "https://shop.test/users");
        let response = Response {
            status: 201,
            status_text: "Created".into(),
            headers: vec![KeyValue::new("Location", "/users/7")],
            body: "{\"id\": 7, \"name\": \"Ann ü😀\"}".as_bytes().to_vec(),
            elapsed: Duration::from_millis(3),
            timing: None,
        };
        let run = script::run(
            &code,
            script::Phase::Test,
            &BTreeMap::new(),
            &request,
            Some(&response),
        );
        assert_eq!(run.error, None);
        assert_eq!(run.failed_tests(), 0, "{:#?}", run.tests);
        assert_eq!(run.tests.len(), 1);
        assert_eq!(run.vars.get("userId"), Some(&Some("7".to_owned())));
    }
}
