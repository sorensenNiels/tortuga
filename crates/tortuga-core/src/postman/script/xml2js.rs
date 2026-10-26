//! Postman's `xml2js` library as Tortuga's `xml2Json` (R-COL-39, R-TEST-18).
//!
//! Postman scripts read XML with `const xml2js = require('xml2js');` and
//! `xml2js.parseString(text, {explicitArray: false}, (err, result) => { doc = result; })`.
//! With `explicitArray: false` the result has the shape `xml2Json` gives, and the
//! callback runs before `parseString` returns, so the call becomes
//! `doc = xml2Json(text)`, or `(callback)(null, xml2Json(text))` when the callback does
//! more. Other options give another shape and are left for the user.

use crate::import::js::{Translate, call_args, is_word_byte, member, rewrite, uses_any, word};

/// `code` translated with `at`, with `xml2js` read as `xml2Json`. The declarations
/// requiring it are removed only when nothing else uses the names they declare;
/// otherwise they stay, and `require` keeps the script from running.
pub(super) fn rewrite_with_xml2js(code: &str, at: Translate<'_>) -> String {
    let mut names: Vec<&str> = code.lines().filter_map(declared_name).collect();
    if names.is_empty() {
        return rewrite(code, at);
    }
    names.sort_unstable();
    names.dedup();
    let translate = |code: &str| rewrite(code, &|s| translate_at(s, &names, at));
    let without: String = code
        .split_inclusive('\n')
        .filter(|line| declared_name(line).is_none())
        .collect();
    let translated = translate(&without);
    if uses_any(&translated, &names) {
        translate(code)
    } else {
        translated
    }
}

fn translate_at(s: &str, names: &[&str], at: Translate<'_>) -> Option<(usize, String)> {
    parse_string(s, names, at).or_else(|| at(s))
}

/// `NAME.parseString(text, {explicitArray: false}, callback)`.
fn parse_string(s: &str, names: &[&str], at: Translate<'_>) -> Option<(usize, String)> {
    let i = names.iter().find_map(|name| word(s, name))?;
    let (i, method) = member(s, i)?;
    if method != "parseString" {
        return None;
    }
    let (end, args) = call_args(s, i, &|a| translate_at(a, names, at))?;
    let [text, options, callback] = args.as_slice() else {
        return None;
    };
    if !is_explicit_array_false(options) {
        return None;
    }
    let call = match assigned_result(callback) {
        Some(target) => format!("{target} = xml2Json({text})"),
        None => format!("({callback})(null, xml2Json({text}))"),
    };
    Some((end, call))
}

/// The name in a line that is only `const|let|var NAME = require('xml2js');`.
fn declared_name(line: &str) -> Option<&str> {
    let line = line.trim();
    let rest = ["const", "let", "var"].into_iter().find_map(|keyword| {
        line.strip_prefix(keyword)
            .filter(|rest| rest.starts_with(char::is_whitespace))
    })?;
    let rest = rest.trim_start();
    let (name, rest) = rest.split_at(name_len(rest));
    if name.is_empty() {
        return None;
    }
    let rest = rest.trim_start().strip_prefix('=')?.trim_start();
    let rest = rest
        .strip_prefix("require")?
        .trim_start()
        .strip_prefix('(')?;
    let rest = rest.strip_suffix(';').unwrap_or(rest).trim_end();
    let module = rest.strip_suffix(')')?.trim();
    matches!(module, "'xml2js'" | "\"xml2js\"").then_some(name)
}

/// Whether `options` is `{explicitArray: false}` and nothing else.
fn is_explicit_array_false(options: &str) -> bool {
    let Some(inner) = options
        .strip_prefix('{')
        .and_then(|rest| rest.strip_suffix('}'))
    else {
        return false;
    };
    let inner = inner.trim();
    let inner = inner.strip_suffix(',').unwrap_or(inner);
    inner.split_once(':').is_some_and(|(key, value)| {
        matches!(
            key.trim(),
            "explicitArray" | "'explicitArray'" | "\"explicitArray\""
        ) && value.trim() == "false"
    })
}

/// `V` when `callback` only assigns its result to `V`: `(err, result) => { V = result; }`
/// or the same as a `function`.
fn assigned_result(callback: &str) -> Option<&str> {
    let (params, body) = if let Some(rest) = callback.strip_prefix("function") {
        let (params, body) = rest.trim_start().strip_prefix('(')?.split_once(')')?;
        (params, body.trim_start())
    } else {
        let (params, body) = callback.strip_prefix('(')?.split_once(')')?;
        (params, body.trim_start().strip_prefix("=>")?.trim_start())
    };
    let [_, result] = params.split(',').map(str::trim).collect::<Vec<_>>()[..] else {
        return None;
    };
    let body = body.strip_prefix('{')?.strip_suffix('}')?.trim();
    let body = body.strip_suffix(';').unwrap_or(body);
    let (target, value) = body.split_once('=')?;
    let target = target.trim();
    let is_path = |s: &str| {
        !s.is_empty() && !s.starts_with('.') && s.bytes().all(|b| is_word_byte(b) || b == b'.')
    };
    (value.trim() == result && name_len(result) == result.len() && is_path(target))
        .then_some(target)
}

/// The length of the name `s` starts with (ASCII, so a char boundary).
fn name_len(s: &str) -> usize {
    s.bytes().take_while(|&b| is_word_byte(b)).count()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::time::Duration;

    use crate::import::js::Translation;
    use crate::model::{HttpMethod, Request, Response};
    use crate::postman::script::translate;
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

    fn run_on_xml(script: &str, xml: &str, vars: &BTreeMap<String, String>) -> ScriptRun {
        let response = Response {
            status: 200,
            status_text: "OK".into(),
            headers: Vec::new(),
            body: xml.as_bytes().to_vec(),
            elapsed: Duration::from_millis(3),
            timing: None,
        };
        let request = Request::new(HttpMethod::Post, "https://svc.test/apixml");
        crate::script::run(script, Phase::Test, vars, &request, Some(&response))
    }

    /// The test script of a real XML test suite, as Postman exports it.
    const REGISTRATION_CREATE: &str = r#"pm.test("Status code is 200", function () {
    pm.response.to.have.status(200);
});
if (pm.response.code == 200){
    pm.test("status", function(){
        const xml2js = require('xml2js');
        var jsonObject;
        xml2js.parseString(pm.response.text(),{explicitArray: false}, (err, result)=>{
            jsonObject = result;
        });
        var status = jsonObject.registrationCreateResponse.$.status;
        if (status != "success") {
            pm.execution.setNextRequest(null);
        }
        pm.expect(status).to.equal("success");
        var seq = (jsonObject.registrationCreateResponse.registration.$.registrationSequence);
        pm.collectionVariables.set("CharacterCheckRegistration", seq);
    })
} else {
    pm.execution.setNextRequest(null);
}"#;

    /// R-COL-39: the `require` line goes, `parseString` becomes `xml2Json`, and the
    /// script runs and reads the XML as it did in Postman.
    #[test]
    fn a_parse_string_with_explicit_array_false_becomes_xml2json() {
        let tortuga = complete(REGISTRATION_CREATE);
        assert_eq!(
            tortuga,
            r#"test("Status code is 200", function () {
    expect(res.status).toBe(200);
});
if (res.status == 200){
    test("status", function(){
        var jsonObject;
        jsonObject = xml2Json(res.body);
        var status = jsonObject.registrationCreateResponse.$.status;
        if (status != "success") {
            workflow.stop();
        }
        expect(status).toBe("success");
        var seq = (jsonObject.registrationCreateResponse.registration.$.registrationSequence);
        vars.set("CharacterCheckRegistration", seq);
    })
} else {
    workflow.stop();
}"#
        );

        let run = run_on_xml(
            &tortuga,
            r#"<registrationCreateResponse status="success"><registration registrationSequence="4711" name="Dyssegård 😀"/></registrationCreateResponse>"#,
            &BTreeMap::new(),
        );
        assert_eq!(run.error, None);
        assert!(run.tests.iter().all(|t| t.passed), "{:?}", run.tests);
        assert_eq!(run.tests.len(), 2);
        assert_eq!(
            run.vars["CharacterCheckRegistration"].as_deref(),
            Some("4711")
        );
        assert_eq!(run.flow, None);

        let failed = run_on_xml(
            &tortuga,
            r#"<registrationCreateResponse status="error"/>"#,
            &BTreeMap::new(),
        );
        assert_eq!(failed.failed_tests(), 1);
        assert_eq!(failed.flow, Some(crate::script::FlowControl::Stop));
    }

    /// R-COL-39: repeated elements are arrays and single ones are not, as xml2js gives
    /// them with `explicitArray: false`, so loops over them work unchanged.
    #[test]
    fn loops_over_repeated_elements_run_unchanged() {
        let tortuga = complete(
            r#"pm.test("values", function(){
    const xml2js = require("xml2js");
    var doc;
    xml2js.parseString(pm.response.text(), { "explicitArray": false, }, function (err, result) { doc = result; });
    var values = doc.registrationGetResponse.registration.itemValue;
    for (i = 0; i < values.length; i++){
        if (values[i].$.item == pm.collectionVariables.get("item")){
            pm.expect(values[i].$.value).to.equal("ABC ? ABC ü");
        }
    }
});"#,
        );
        assert!(
            tortuga.contains("    doc = xml2Json(res.body);\n"),
            "{tortuga}"
        );
        let vars = BTreeMap::from([("item".to_owned(), "BESKED".to_owned())]);
        let run = run_on_xml(
            &tortuga,
            r#"<registrationGetResponse><registration><itemValue item="X" value="x"/><itemValue item="BESKED" value="ABC ? ABC ü"/></registration></registrationGetResponse>"#,
            &vars,
        );
        assert_eq!(run.error, None);
        assert_eq!(run.tests.len(), 1);
        assert!(run.tests[0].passed, "{:?}", run.tests);
    }

    /// R-COL-39: a callback that does more is called with the result, under any name
    /// the script gives the library.
    #[test]
    fn other_callbacks_are_called_with_the_result() {
        let tortuga = complete(
            "var parser = require('xml2js')\n\
             let id;\n\
             parser.parseString(responseBody, {explicitArray: false}, (err, r) => {\n\
             \x20 if (err) { throw err; }\n\
             \x20 id = r.user.$.id;\n\
             });\n\
             pm.environment.set('id', id);",
        );
        assert_eq!(
            tortuga,
            "let id;\n\
             ((err, r) => {\n\
             \x20 if (err) { throw err; }\n\
             \x20 id = r.user.$.id;\n\
             })(null, xml2Json(res.body));\n\
             vars.set('id', id);"
        );
        let run = run_on_xml(&tortuga, r#"<user id="7"/>"#, &BTreeMap::new());
        assert_eq!(run.error, None);
        assert_eq!(run.vars["id"].as_deref(), Some("7"));
    }

    /// R-COL-39: other options give another shape, and other uses of the library have no
    /// counterpart: the `require` stays and the script is kept as comments.
    #[test]
    fn other_options_and_uses_are_left_for_the_user() {
        for postman in [
            // Without options every child is an array.
            "const xml2js = require('xml2js');\nxml2js.parseString(pm.response.text(), (e, r) => { d = r; });",
            "const xml2js = require('xml2js');\nxml2js.parseString(pm.response.text(), {explicitArray: false, mergeAttrs: true}, (e, r) => { d = r; });",
            "const xml2js = require('xml2js');\nxml2js.parseString(pm.response.text(), {explicitArray: true}, (e, r) => { d = r; });",
            "const xml2js = require('xml2js');\nconst p = new xml2js.Parser({explicitArray: false});",
            "const xml2js = require('xml2js'); let d;\nxml2js.parseString(pm.response.text(), {explicitArray: false}, (e, r) => { d = r; });",
            "const lib = require('xml2js-other');",
        ] {
            let code = partial(postman);
            assert!(code.contains("require("), "{code}");
        }
        // The declaration stays only where it is needed.
        let code = partial(
            "const xml2js = require('xml2js');\n\
             xml2js.parseString(pm.response.text(), {explicitArray: false}, (e, r) => { d = r; });\n\
             const b = new xml2js.Builder();",
        );
        assert!(code.contains("d = xml2Json(res.body);"), "{code}");
        assert!(
            code.starts_with("const xml2js = require('xml2js');"),
            "{code}"
        );
    }

    /// R-COL-39: only code is translated; strings and comments, also non-ASCII ones,
    /// are left alone.
    #[test]
    fn strings_and_comments_are_left_alone() {
        let tortuga = complete(
            "const xml2js = require('xml2js');\n\
             // xml2js.parseString(x, {explicitArray: false}, cb) 😀\n\
             const note = \"xml2js.parseString ü\";\n\
             xml2js.parseString('<a>ø</a>', {explicitArray: false}, (e, r) => { doc = r; });",
        );
        assert_eq!(
            tortuga,
            "// xml2js.parseString(x, {explicitArray: false}, cb) 😀\n\
             const note = \"xml2js.parseString ü\";\n\
             doc = xml2Json('<a>ø</a>');"
        );
    }
}
