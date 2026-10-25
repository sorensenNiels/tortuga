//! Sending a request with everything around it (R-TEST-1..5): the pre-request script,
//! filling in variables, sending, captures and the test script. Used by the app for a
//! single send, and by workflows and the command-line runner for each step.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::cookies::CookieJar;
use crate::jsonpath::JsonPath;
use crate::model::{Request, Response};
use crate::script::{
    self, Breakpoints, ConsoleEntry, FlowControl, Phase, ScriptInfo, ScriptKind, ScriptRun,
    TestResult,
};
use crate::{Client, vars};

/// A capture or `vars.set` name starting with this goes to the workspace (R-WS-8):
/// `workspace.token` sets `{{token}}` for every project of the workspace.
pub const WORKSPACE_PREFIX: &str = "workspace.";

/// The variable a name sets: `(true, "token")` for `workspace.token`, else `(false, name)`.
pub fn target(name: &str) -> (bool, &str) {
    match name.strip_prefix(WORKSPACE_PREFIX) {
        Some(rest) if !rest.trim().is_empty() => (true, rest),
        _ => (false, name),
    }
}

/// Captures and a test script added on top of the request's own (a workflow step, R-FLOW-1),
/// and breakpoints in the request's scripts (R-TEST-10).
#[derive(Debug, Clone, Default)]
pub struct Extra<'a> {
    pub capture: Option<&'a BTreeMap<String, String>>,
    pub tests: Option<&'a str>,
    pub breakpoints: Option<&'a Breakpoints>,
    /// Compare the response with the request's examples (R-MOCK-8).
    pub check_examples: bool,
    /// The iteration of a workflow run, for scripts' `info` (R-TEST-16).
    pub info: ScriptInfo,
}

/// What happened when a request was run.
#[derive(Debug, Clone, Default)]
pub struct StepOutcome {
    /// The request as sent (variables filled in); `None` if it never got that far.
    pub sent: Option<Request>,
    /// `sent` as it may be shown, when it uses secrets: their values as `<secret:name>`
    /// and dynamic variables as written, like the request preview (R-HTTP-14, P6).
    pub masked: Option<Request>,
    /// The response, or why there is none.
    pub response: Option<Result<Response, String>>,
    /// The pre-request script's result, if the request has one.
    pub pre_request: Option<ScriptRun>,
    /// The test scripts' results (the request's, then the step's).
    pub tests: Vec<TestResult>,
    /// The console of all scripts, in order, with the script each line came from (R-TEST-9).
    pub console: Vec<(ScriptKind, ConsoleEntry)>,
    /// Errors from scripts and captures.
    pub errors: Vec<String>,
    /// Variables set (`Some`) or unset (`None`) by scripts and captures (R-TEST-5), by
    /// the name used, so `workspace.token` for the workspace (R-WS-8).
    pub vars: BTreeMap<String, Option<String>>,
    /// What the scripts asked a workflow run to do next, the last call (R-FLOW-6).
    pub flow: Option<FlowControl>,
}

impl StepOutcome {
    /// The request as sent, as it may be shown: secret values hidden (P6).
    pub fn shown(&self) -> Option<&Request> {
        self.masked.as_ref().or(self.sent.as_ref())
    }

    pub fn failed_tests(&self) -> usize {
        self.tests.iter().filter(|t| !t.passed).count()
    }

    /// The step went wrong: no response, a script or capture error, a failed test, or a
    /// 4xx/5xx response (R-FLOW-3).
    pub fn failed(&self) -> bool {
        !self.errors.is_empty()
            || self.failed_tests() > 0
            || match &self.response {
                Some(Ok(r)) => r.status >= 400,
                _ => true,
            }
    }

    /// Why it failed, in a few words.
    pub fn failure(&self) -> Option<String> {
        if let Some(e) = self.errors.first() {
            return Some(e.clone());
        }
        match &self.response {
            None => return Some("not sent".into()),
            Some(Err(e)) => return Some(e.clone()),
            _ => {}
        }
        if let Some(t) = self.tests.iter().find(|t| !t.passed) {
            return Some(format!(
                "test “{}” failed: {}",
                t.name,
                t.message.as_deref().unwrap_or("no reason given")
            ));
        }
        match &self.response {
            Some(Ok(r)) if r.status >= 400 => Some(format!("{} {}", r.status, r.status_text)),
            _ => None,
        }
    }
}

/// Runs `request` in `ctx`: pre-request script, send, captures, tests. The variables
/// the run sets are in [`StepOutcome::vars`]; `ctx` is not changed.
pub async fn run_step(
    client: &Client,
    jar: Option<&CookieJar>,
    request: &Request,
    ctx: &vars::Context,
    extra: Extra<'_>,
) -> StepOutcome {
    let mut out = StepOutcome::default();
    let default_breakpoints = Breakpoints::default();
    let breakpoints = extra.breakpoints.unwrap_or(&default_breakpoints);
    // The app's settings are the last level under the workspace's and project's (R-SET-1).
    let mut ctx = ctx.clone().with_app_settings(client.settings());
    let mut request = request.clone();

    if !request.scripts.pre_request.trim().is_empty() {
        let run = script::run_with_breakpoints(
            &request.scripts.pre_request,
            Phase::PreRequest,
            &ctx.variables,
            &request,
            None,
            extra.info,
            &breakpoints.pre_request,
        );
        apply_vars(&mut ctx, &mut out, &run.vars);
        if run.flow.is_some() {
            out.flow.clone_from(&run.flow);
        }
        for header in &run.headers {
            request.headers.retain(|h| !h.is_named(&header.name));
            request.headers.push(header.clone());
        }
        out.console.extend(
            run.console
                .iter()
                .map(|e| (ScriptKind::PreRequest, e.clone())),
        );
        let error = run.error.clone();
        out.pre_request = Some(run);
        if let Some(e) = error {
            out.errors.push(format!("Pre-request script: {e}"));
            return out;
        }
    }

    let prepared = match vars::prepare(&request, &ctx) {
        Ok(p) => p,
        Err(e) => {
            out.response = Some(Err(crate::error::message(&e)));
            return out;
        }
    };
    let result = client.execute_with_cookies(&prepared, jar).await;
    out.sent = Some(prepared);
    if vars::names_in(&request, &ctx)
        .iter()
        .any(|n| ctx.secrets.contains(n))
    {
        // It was prepared once already, so this does not fail.
        out.masked = vars::preview(&request, &ctx).ok();
    }
    let response = match result {
        Ok(r) => r,
        Err(e) => {
            out.response = Some(Err(crate::error::message(&e)));
            return out;
        }
    };

    let sent = out.sent.clone().unwrap_or_default();
    after_response(&request, &sent, &response, &mut ctx, &extra, &mut out);
    out.response = Some(Ok(response));
    out
}

/// What runs once a response is there: captures, then the test scripts (R-TEST-1,
/// R-TEST-4), then the contract check if asked for (R-MOCK-8). Also used to run the tests again on the last response without sending
/// (R-TEST-12). `sent` is the request as it was sent, shown to scripts as `req`.
pub fn after_response(
    request: &Request,
    sent: &Request,
    response: &Response,
    ctx: &mut vars::Context,
    extra: &Extra<'_>,
    out: &mut StepOutcome,
) {
    let no_breakpoints = std::collections::BTreeSet::new();
    let default_breakpoints = Breakpoints::default();
    let breakpoints = extra.breakpoints.unwrap_or(&default_breakpoints);
    // Captures: the request's, then the step's (which may override them).
    let sources = request
        .capture
        .iter()
        .chain(extra.capture.into_iter().flatten());
    let mut captured = BTreeMap::new();
    for (name, from) in sources {
        match capture(from, response) {
            Ok(value) => drop(captured.insert(name.clone(), Some(value))),
            Err(e) => out.errors.push(format!("Capture “{name}” ({from}): {e}")),
        }
    }
    apply_vars(ctx, out, &captured);

    let scripts = [Some(request.scripts.tests.as_str()), extra.tests];
    for (n, tests) in scripts.into_iter().flatten().enumerate() {
        if tests.trim().is_empty() {
            continue;
        }
        let (kind, lines) = if n == 0 {
            (ScriptKind::Tests, &breakpoints.tests)
        } else {
            (ScriptKind::StepTests, &no_breakpoints)
        };
        let run = script::run_with_breakpoints(
            tests,
            Phase::Test,
            &ctx.variables,
            sent,
            Some(response),
            extra.info,
            lines,
        );
        apply_vars(ctx, out, &run.vars);
        if run.flow.is_some() {
            out.flow = run.flow;
        }
        out.tests.extend(run.tests);
        out.console
            .extend(run.console.into_iter().map(|e| (kind, e)));
        if let Some(e) = run.error {
            let which = if n == 0 {
                "Test script"
            } else {
                "Step test script"
            };
            out.errors.push(format!("{which}: {e}"));
        }
    }

    if extra.check_examples
        && let Some(result) =
            crate::mock::contract::check(&request.examples, sent, response, &ctx.variables)
    {
        out.tests.push(result);
    }
}

fn apply_vars(
    ctx: &mut vars::Context,
    out: &mut StepOutcome,
    changes: &BTreeMap<String, Option<String>>,
) {
    for (name, value) in changes {
        let (_, var) = target(name);
        match value {
            Some(v) => drop(ctx.variables.insert(var.to_owned(), v.clone())),
            None => drop(ctx.variables.remove(var)),
        }
        out.vars.insert(name.clone(), value.clone());
    }
}

/// A value from a response (R-TEST-4): `status`, `header:<name>`, `xpath:<expression>`
/// on an XML body (the first match's text, R-HTTP-19), or a `JSONPath` expression on the
/// JSON body (the first match; text as is, anything else as JSON).
pub fn capture(from: &str, response: &Response) -> Result<String, String> {
    let from = from.trim();
    if from == "status" {
        return Ok(response.status.to_string());
    }
    if let Some(name) = from.strip_prefix("header:") {
        return response
            .header(name.trim())
            .map(str::to_owned)
            .ok_or_else(|| format!("no header “{}” in the response", name.trim()));
    }
    if let Some(expr) = from.strip_prefix("xpath:") {
        let body = std::str::from_utf8(&response.body)
            .map_err(|_| "the response body is not XML".to_owned())?;
        return crate::xpath::first_text(expr, body);
    }
    let path = JsonPath::parse(from).map_err(|e| crate::error::sentence(&e))?;
    let body: serde_json::Value = serde_json::from_slice(&response.body)
        .map_err(|_| "the response body is not JSON".to_owned())?;
    let found = path
        .select(&body)
        .into_iter()
        .next()
        .ok_or_else(|| "nothing in the response matches".to_owned())?;
    Ok(match found {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    })
}

/// Turns runtime variable changes into the map the app keeps (R-TEST-5). Workspace
/// names (R-WS-8) are applied without their prefix, as in a run with one project.
pub fn apply_changes(
    runtime: &mut BTreeMap<String, String>,
    changes: &BTreeMap<String, Option<String>>,
) {
    for (name, value) in changes {
        let (_, var) = target(name);
        match value {
            Some(v) => drop(runtime.insert(var.to_owned(), v.clone())),
            None => drop(runtime.remove(var)),
        }
    }
}

/// Splits changes into the project's and the workspace's (R-WS-8, without the prefix).
pub fn split_changes(
    changes: &BTreeMap<String, Option<String>>,
) -> (
    BTreeMap<String, Option<String>>,
    BTreeMap<String, Option<String>>,
) {
    let mut project = BTreeMap::new();
    let mut workspace = BTreeMap::new();
    for (name, value) in changes {
        match target(name) {
            (true, var) => drop(workspace.insert(var.to_owned(), value.clone())),
            (false, var) => drop(project.insert(var.to_owned(), value.clone())),
        }
    }
    (project, workspace)
}

/// The outcome of a step as plain data, for JSON reports (R-CLI-3).
#[derive(Debug, Clone, Serialize)]
pub struct StepSummary {
    pub status: Option<u16>,
    pub elapsed_ms: Option<u64>,
    pub tests: Vec<TestResult>,
    pub errors: Vec<String>,
    pub failed: bool,
}

impl From<&StepOutcome> for StepSummary {
    fn from(o: &StepOutcome) -> Self {
        let response = o.response.as_ref().and_then(|r| r.as_ref().ok());
        Self {
            status: response.map(|r| r.status),
            elapsed_ms: response.map(|r| u64::try_from(r.elapsed.as_millis()).unwrap_or(u64::MAX)),
            tests: o.tests.clone(),
            errors: o
                .errors
                .iter()
                .cloned()
                .chain(o.response.as_ref().and_then(|r| r.as_ref().err().cloned()))
                .collect(),
            failed: o.failed(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// R-WS-8: `workspace.` names go to the workspace, without the prefix.
    #[test]
    fn workspace_names_are_split_off() {
        assert_eq!(target("workspace.token"), (true, "token"));
        assert_eq!(target("token"), (false, "token"));
        assert_eq!(
            target("workspace."),
            (false, "workspace."),
            "no name after it"
        );
        let changes = BTreeMap::from([
            ("workspace.token".to_owned(), Some("t".to_owned())),
            ("workspace.old".to_owned(), None),
            ("id".to_owned(), Some("7".to_owned())),
        ]);
        let (project, workspace) = split_changes(&changes);
        assert_eq!(
            project,
            BTreeMap::from([("id".to_owned(), Some("7".to_owned()))])
        );
        assert_eq!(
            workspace,
            BTreeMap::from([
                ("old".to_owned(), None),
                ("token".to_owned(), Some("t".to_owned())),
            ])
        );
        let mut runtime = BTreeMap::from([("old".to_owned(), "x".to_owned())]);
        apply_changes(&mut runtime, &changes);
        assert_eq!(
            runtime,
            BTreeMap::from([
                ("id".to_owned(), "7".to_owned()),
                ("token".to_owned(), "t".to_owned()),
            ])
        );
    }
}
