//! The request's Scripts tab (R-TEST-1, R-TEST-2, R-TEST-4): sub-tabs for the pre-request
//! script, the tests and the captures (R-TEST-11), each script filling the panel, with the
//! last run's results on their lines and "Re-run tests" (R-TEST-12).

use egui::{CollapsingHeader, Ui};
use tortuga_core::Request;
use tortuga_core::script::{ConsoleEntry, ScriptKind};

use super::request_editor::{EditorAction, EditorContext, EditorState, Rerun};
use super::script_editor::{LineMark, line_marks, script_editor, script_toolbar};
use super::{TestsView, key_value_table};
use crate::theme;

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
            "Pre-request",
            badge(
                !request.scripts.pre_request.trim().is_empty(),
                went_wrong(pre_marks).then(|| "✗".to_owned()),
                cx.breakpoints.pre_request.len(),
            ),
        ),
        (
            ScriptTab::Tests,
            "Tests",
            badge(
                !request.scripts.tests.trim().is_empty(),
                test_result,
                cx.breakpoints.tests.len(),
            ),
        ),
        (
            ScriptTab::Captures,
            "Captures",
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
                "Runs before variables are filled in and the request is sent.",
                &mut request.scripts.pre_request,
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
            reference(
                ui,
                "pre-request",
                "vars.get/set/unset/has, req.method/url/headers/body, req.setHeader(name, value), \
                 console.log/info/warn/error. Click a line number to set a breakpoint: the \
                 Console then shows the variables there.",
            );
            toggled.map(|line| EditorAction::ToggleBreakpoint(ScriptKind::PreRequest, line))
        }
        ScriptTab::Tests => {
            let rerun = script_toolbar(
                ui,
                "Runs after the response arrives.",
                &mut request.scripts.tests,
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
            reference(
                ui,
                "tests",
                "test(name, fn); expect(x).toBe/toEqual/toContain/toMatch/toHaveProperty/… (and .not) \
                 or Chai's expect(x).to.equal/.to.be.an(\"object\")/.to.have.property/…; \
                 res.status, res.json(), res.header(name), res.time, res.body; vars; \
                 console.log. Click a line number to set a breakpoint: the Console then shows \
                 the variables there.",
            );
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
    CollapsingHeader::new("Reference")
        .id_salt(("script-reference", id))
        .show(ui, |ui| theme::hint(ui, text));
}

fn captures(ui: &mut Ui, request: &mut Request, st: &mut EditorState) {
    if key_value_table(ui, "captures", &mut st.captures, "Variable", &[]) {
        request.capture = st
            .captures
            .iter()
            .filter(|c| c.is_active())
            .map(|c| (c.name.trim().to_owned(), c.value.trim().to_owned()))
            .collect();
    }
    theme::hint(
        ui,
        "Puts a value from the response into a variable: $.json.path, xpath://xml/path, \
         header:Name or status. \
         Captured values are kept until Tortuga closes and win over environment values.",
    );
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
