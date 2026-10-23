//! The request's Scripts tab (R-TEST-1, R-TEST-2, R-TEST-4): sub-tabs for the pre-request
//! script, the tests and the captures (R-TEST-11), each script filling the panel, with the
//! last run's results on their lines and "Re-run tests" (R-TEST-12).

use egui::{CollapsingHeader, Ui};
use tortuga_core::Request;
use tortuga_core::script::{ConsoleEntry, ScriptKind};

use super::request_editor::{EditorAction, EditorContext, EditorState, Rerun};
use super::script_editor::{LineMark, Snippet, line_marks, script_editor, script_toolbar};
use super::{TestsView, VarHelp, key_value_table};
use crate::theme;

/// Snippets for a pre-request script (R-TEST-15): (menu label, code).
const PRE_REQUEST_SNIPPETS: &[Snippet] = &[
    (
        || t!("snippet-set-variable"),
        "vars.set(\"name\", \"value\");",
    ),
    (
        || t!("snippet-set-header"),
        "req.setHeader(\"X-Request-Id\", String(Date.now()));",
    ),
    (
        || t!("snippet-timestamp"),
        "vars.set(\"now\", new Date().toISOString());",
    ),
    (
        || t!("snippet-random"),
        "vars.set(\"random\", String(Math.floor(Math.random() * 1000000)));",
    ),
    (
        || t!("snippet-log-request"),
        "console.log(req.method, req.url);",
    ),
];

/// Snippets for a test script (R-TEST-15): (menu label, code).
const TEST_SNIPPETS: &[Snippet] = &[
    (
        || t!("snippet-status-200"),
        "test(\"status is 200\", () => {\n  expect(res.status).toBe(200);\n});",
    ),
    (
        || t!("snippet-status-2xx"),
        "test(\"status is 2xx\", () => {\n  expect(res.status).to.be.within(200, 299);\n});",
    ),
    (
        || t!("snippet-response-time"),
        "test(\"response time under 500 ms\", () => {\n  expect(res.time).toBeLessThan(500);\n});",
    ),
    (
        || t!("snippet-body-json"),
        "test(\"body is JSON\", () => {\n  expect(res.header(\"content-type\")).toMatch(/json/);\n});",
    ),
    (
        || t!("snippet-body-property"),
        "test(\"body has an id\", () => {\n  expect(res.json()).toHaveProperty(\"id\");\n});",
    ),
    (
        || t!("snippet-save-field"),
        "vars.set(\"id\", String(res.json().id));",
    ),
    (|| t!("snippet-log-body"), "console.log(res.json());"),
    (|| t!("snippet-read-xml"), XML_SNIPPET),
    (
        || t!("snippet-save-cookie"),
        "if (res.cookie(\"session\") !== undefined) {\n  vars.set(\"session\", res.cookie(\"session\"));\n}",
    ),
    (
        || t!("snippet-workflow-repeat"),
        "// In a workflow run: run the step \"Check status\" again until the job is done.\nif (res.json().status === \"running\") {\n  workflow.next(\"Check status\");\n}",
    ),
    (
        || t!("snippet-workflow-stop"),
        "// In a workflow run: end this iteration after this step.\nif (res.status === 404) {\n  workflow.stop();\n}",
    ),
];

/// The test snippet that needs an XML response (R-TEST-18).
const XML_SNIPPET: &str = "// The XML body as an object: attributes under $, text under _ (as in Postman).\nconst xml = xml2Json(res.body);\nconsole.log(xml);";

/// Room under a script field for its "Reference" line.
const BELOW_FIELD: f32 = 64.0;

/// A sub-tab of the Scripts tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptTab {
    PreRequest,
    Tests,
    Captures,
}

impl ScriptTab {
    /// The sub-tab to open on: the tests, unless only the pre-request script or the
    /// captures have something.
    pub fn first_for(request: &Request) -> Self {
        if request.scripts.tests.trim().is_empty() {
            if !request.scripts.pre_request.trim().is_empty() {
                return Self::PreRequest;
            }
            if !request.capture.is_empty() {
                return Self::Captures;
            }
        }
        Self::Tests
    }
}

/// The last run's marks for a script, while it is the script that ran (R-TEST-11).
fn fresh_marks(
    run: Option<&TestsView>,
    kind: ScriptKind,
    ran: impl Fn(&TestsView) -> &str,
    now: &str,
) -> LineMarks {
    run.filter(|r| ran(r) == now)
        .map(|r| line_marks(&r.console, kind))
        .unwrap_or_default()
}

/// "2/3 ⏺1": the run's result or a dot for a script that has code, and its breakpoints.
fn badge(has_code: bool, result: Option<String>, breakpoints: usize) -> Option<String> {
    let mut parts = Vec::new();
    match result {
        Some(r) => parts.push(r),
        None if has_code => parts.push("•".to_owned()),
        None => {}
    }
    if breakpoints > 0 {
        parts.push(format!("⏺{breakpoints}"));
    }
    (!parts.is_empty()).then(|| parts.join(" "))
}

/// The last run's marks of the pre-request and test scripts, while they are the scripts
/// that ran (R-TEST-11).
type LineMarks = std::collections::BTreeMap<u32, LineMark>;

fn went_wrong(marks: &LineMarks) -> bool {
    marks
        .values()
        .any(|m| m.error.is_some() || !m.failed.is_empty())
}

/// The sub-tabs, with badges, red where the last run went wrong.
fn sub_tabs(
    ui: &mut Ui,
    request: &Request,
    st: &mut EditorState,
    cx: EditorContext<'_>,
    (pre_marks, test_marks): (&LineMarks, &LineMarks),
) {
    let run = cx.last_run;
    let capture_errors = run.map_or(0, |r| {
        r.errors
            .iter()
            .filter(|e| e.starts_with("Capture “"))
            .count()
    });
    let test_result =
        run.filter(|r| r.ran.tests == request.scripts.tests && !request.scripts.tests.is_empty())
            .map(|r| {
                let script_error = r.console.iter().any(|(k, e)| {
                    *k == ScriptKind::Tests && matches!(e, ConsoleEntry::Error { .. })
                });
                if r.tests.is_empty() && script_error {
                    "✗".to_owned()
                } else {
                    let passed = r.tests.iter().filter(|t| t.passed).count();
                    format!("{passed}/{}", r.tests.len())
                }
            });
    let tabs = [
        (
            ScriptTab::PreRequest,
            t!("scripts-pre-request"),
            badge(
                !request.scripts.pre_request.trim().is_empty(),
                went_wrong(pre_marks).then(|| "✗".to_owned()),
                cx.breakpoints.pre_request.len(),
            ),
        ),
        (
            ScriptTab::Tests,
            t!("tests"),
            badge(
                !request.scripts.tests.trim().is_empty(),
                test_result,
                cx.breakpoints.tests.len(),
            ),
        ),
        (
            ScriptTab::Captures,
            t!("captures"),
            (!request.capture.is_empty()).then(|| request.capture.len().to_string()),
        ),
    ];
    let mut failing = Vec::new();
    if went_wrong(pre_marks) {
        failing.push(ScriptTab::PreRequest);
    }
    if went_wrong(test_marks) {
        failing.push(ScriptTab::Tests);
    }
    if capture_errors > 0 {
        failing.push(ScriptTab::Captures);
    }
    theme::tabs_flagged(ui, &mut st.script_tab, &tabs, &failing);
}

/// The Scripts tab. Returns what the user asked for: a breakpoint toggled, or the tests
/// run again.
pub fn request_scripts(
    ui: &mut Ui,
    request: &mut Request,
    st: &mut EditorState,
    cx: EditorContext<'_>,
) -> Option<EditorAction> {
    let pre_marks = fresh_marks(
        cx.last_run,
        ScriptKind::PreRequest,
        |r| &r.ran.pre_request,
        &request.scripts.pre_request,
    );
    let test_marks = fresh_marks(
        cx.last_run,
        ScriptKind::Tests,
        |r| &r.ran.tests,
        &request.scripts.tests,
    );
    sub_tabs(ui, request, st, cx, (&pre_marks, &test_marks));

    match st.script_tab {
        ScriptTab::PreRequest => {
            script_toolbar(
                ui,
                &t!("scripts-pre-request-about"),
                "pre-request",
                &mut request.scripts.pre_request,
                PRE_REQUEST_SNIPPETS,
                None,
            );
            let toggled = script_editor(
                ui,
                "pre-request",
                &mut request.scripts.pre_request,
                rows_that_fit(ui),
                "vars.set(\"now\", String(Date.now()));\nreq.setHeader(\"X-Request-Id\", vars.get(\"now\"));",
                Some(&cx.breakpoints.pre_request),
                &pre_marks,
            );
            reference(ui, "pre-request", &t!("scripts-pre-request-reference"));
            toggled.map(|line| EditorAction::ToggleBreakpoint(ScriptKind::PreRequest, line))
        }
        ScriptTab::Tests => {
            let rerun = script_toolbar(
                ui,
                &t!("scripts-tests-about"),
                "tests",
                &mut request.scripts.tests,
                TEST_SNIPPETS,
                Some(cx.rerun == Rerun::Ready),
            );
            let toggled = script_editor(
                ui,
                "tests",
                &mut request.scripts.tests,
                rows_that_fit(ui),
                "test(\"status is 200\", () => expect(res.status).toBe(200));\ntest(\"has an id\", () => expect(res.json()).toHaveProperty(\"id\"));",
                Some(&cx.breakpoints.tests),
                &test_marks,
            );
            reference(ui, "tests", &t!("scripts-tests-reference"));
            if rerun {
                Some(EditorAction::RerunTests)
            } else {
                toggled.map(|line| EditorAction::ToggleBreakpoint(ScriptKind::Tests, line))
            }
        }
        ScriptTab::Captures => {
            captures(ui, request, st);
            None
        }
    }
}

/// As many rows as fill the panel above the reference line, at least a few.
fn rows_that_fit(ui: &Ui) -> usize {
    let row = ui.text_style_height(&egui::TextStyle::Monospace).max(1.0);
    let fit = ((ui.available_height() - BELOW_FIELD) / row)
        .floor()
        .max(4.0);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a small positive row count"
    )]
    let rows = fit as usize;
    rows
}

/// What a script can use, folded away under the field.
fn reference(ui: &mut Ui, id: &str, text: &str) {
    CollapsingHeader::new(t!("scripts-reference"))
        .id_salt(("script-reference", id))
        .show(ui, |ui| theme::hint(ui, text));
}

fn captures(ui: &mut Ui, request: &mut Request, st: &mut EditorState) {
    if key_value_table(
        ui,
        "captures",
        &mut st.captures,
        &t!("variable"),
        &VarHelp::default(),
    ) {
        request.capture = st
            .captures
            .iter()
            .filter(|c| c.is_active())
            .map(|c| (c.name.trim().to_owned(), c.value.trim().to_owned()))
            .collect();
    }
    theme::hint(ui, t!("scripts-captures-hint"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_on_the_tests_unless_only_something_else_has_content() {
        let mut r = Request::default();
        assert_eq!(ScriptTab::first_for(&r), ScriptTab::Tests);
        r.scripts.pre_request = "vars.set('a', '1');".into();
        assert_eq!(ScriptTab::first_for(&r), ScriptTab::PreRequest);
        r.scripts.tests = "test('t', () => {});".into();
        assert_eq!(ScriptTab::first_for(&r), ScriptTab::Tests);
        let mut c = Request::default();
        c.capture.insert("id".into(), "$.id".into());
        assert_eq!(ScriptTab::first_for(&c), ScriptTab::Captures);
    }

    /// R-TEST-15: every snippet is valid JavaScript, and the test snippets run on a
    /// typical JSON response without errors.
    #[test]
    fn snippets_parse_and_run() {
        use std::collections::BTreeMap;
        use tortuga_core::script::{self, Phase};
        use tortuga_core::{HttpMethod, KeyValue, Response};
        for (label, code) in PRE_REQUEST_SNIPPETS.iter().chain(TEST_SNIPPETS) {
            assert!(script::parses(code), "{}: {code}", label());
        }
        let req = Request::new(HttpMethod::Get, "https://a.test/");
        let res = Response {
            status: 200,
            status_text: "OK".into(),
            headers: vec![KeyValue::new("content-type", "application/json")],
            body: br#"{"id": 7}"#.to_vec(),
            elapsed: std::time::Duration::from_millis(5),
        };
        let all: Vec<&str> = TEST_SNIPPETS
            .iter()
            .filter(|(_, code)| *code != XML_SNIPPET)
            .map(|(_, c)| *c)
            .collect();
        let run = script::run(
            &all.join("\n"),
            Phase::Test,
            &BTreeMap::new(),
            &req,
            Some(&res),
        );
        assert_eq!(run.error, None);
        assert!(run.tests.iter().all(|t| t.passed), "{:?}", run.tests);
        assert_eq!(run.vars["id"].as_deref(), Some("7"));
        let xml = TEST_SNIPPETS
            .iter()
            .find_map(|(_, code)| (*code == XML_SNIPPET).then_some(*code))
            .expect("the XML snippet is in the list");
        let xml_res = Response {
            headers: vec![KeyValue::new("content-type", "application/xml")],
            body: br#"<user id="7">Ann</user>"#.to_vec(),
            ..res
        };
        let run = script::run(xml, Phase::Test, &BTreeMap::new(), &req, Some(&xml_res));
        assert_eq!(run.error, None);
        assert_eq!(run.console.len(), 1, "{:?}", run.console);
        let all: Vec<&str> = PRE_REQUEST_SNIPPETS.iter().map(|(_, c)| *c).collect();
        let run = script::run(
            &all.join("\n"),
            Phase::PreRequest,
            &BTreeMap::new(),
            &req,
            None,
        );
        assert_eq!(run.error, None);
    }

    /// R-TEST-15: a snippet picked from the menu lands in the script.
    #[test]
    fn a_snippet_is_inserted_from_the_menu() {
        use egui_kittest::kittest::Queryable;
        let mut script = "const a = 1;".to_owned();
        let mut h = crate::test_host::harness(|ui| {
            script_toolbar(ui, "about", "t", &mut script, TEST_SNIPPETS, None);
        });
        h.run();
        h.get_by_label("Snippets").click();
        h.run();
        h.get_by_label("Status is 200").click();
        h.run();
        drop(h);
        assert_eq!(
            script,
            "const a = 1;\n\ntest(\"status is 200\", () => {\n  expect(res.status).toBe(200);\n});"
        );
    }

    #[test]
    fn badges_show_results_and_breakpoints() {
        assert_eq!(badge(false, None, 0), None);
        assert_eq!(badge(true, None, 0).as_deref(), Some("•"));
        assert_eq!(
            badge(true, Some("2/3".into()), 1).as_deref(),
            Some("2/3 ⏺1")
        );
        assert_eq!(badge(false, None, 2).as_deref(), Some("⏺2"));
    }
}
