//! Sending a request with everything around it (R-TEST-1..5): the pre-request script,
//! filling in variables, sending, captures and the test script. Used by the app for a
//! single send, and by workflows and the command-line runner for each step.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::cookies::CookieJar;
use crate::jsonpath::JsonPath;
use crate::model::{Request, Response};
use crate::script::{self, Phase, ScriptRun, TestResult};
use crate::{Client, vars};

/// Captures and a test script added on top of the request's own (a workflow step, R-FLOW-1).
#[derive(Debug, Clone, Default)]
pub struct Extra<'a> {
    pub capture: Option<&'a BTreeMap<String, String>>,
    pub tests: Option<&'a str>,
}

/// What happened when a request was run.
#[derive(Debug, Clone, Default)]
pub struct StepOutcome {
    /// The request as sent (variables filled in); `None` if it never got that far.
    pub sent: Option<Request>,
    /// The response, or why there is none.
    pub response: Option<Result<Response, String>>,
    /// The pre-request script's result, if the request has one.
    pub pre_request: Option<ScriptRun>,
    /// The test scripts' results (the request's, then the step's).
    pub tests: Vec<TestResult>,
    /// `console.log` output of all scripts, in order.
    pub logs: Vec<String>,
    /// Errors from scripts and captures.
    pub errors: Vec<String>,
    /// Variables set (`Some`) or unset (`None`) by scripts and captures (R-TEST-5).
    pub vars: BTreeMap<String, Option<String>>,
}

impl StepOutcome {
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
    let mut ctx = ctx.clone();
    let mut request = request.clone();

    if !request.scripts.pre_request.trim().is_empty() {
        let run = script::run(
            &request.scripts.pre_request,
            Phase::PreRequest,
            &ctx.variables,
            &request,
            None,
        );
        apply_vars(&mut ctx, &mut out, &run.vars);
        for header in &run.headers {
            request
                .headers
                .retain(|h| !h.name.trim().eq_ignore_ascii_case(header.name.trim()));
            request.headers.push(header.clone());
        }
        out.logs.extend(run.logs.iter().cloned());
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
    let response = match result {
        Ok(r) => r,
        Err(e) => {
            out.response = Some(Err(crate::error::message(&e)));
            return out;
        }
    };

    // Captures: the request's, then the step's (which may override them).
    let sources = request
        .capture
        .iter()
        .chain(extra.capture.into_iter().flatten());
    let mut captured = BTreeMap::new();
    for (name, from) in sources {
        match capture(from, &response) {
            Ok(value) => drop(captured.insert(name.clone(), Some(value))),
            Err(e) => out.errors.push(format!("Capture “{name}” ({from}): {e}")),
        }
    }
    apply_vars(&mut ctx, &mut out, &captured);

    let sent = out.sent.clone().unwrap_or_default();
    let scripts = [Some(request.scripts.tests.as_str()), extra.tests];
    for (n, tests) in scripts.into_iter().flatten().enumerate() {
        if tests.trim().is_empty() {
            continue;
        }
        let run = script::run(tests, Phase::Test, &ctx.variables, &sent, Some(&response));
        apply_vars(&mut ctx, &mut out, &run.vars);
        out.tests.extend(run.tests);
        out.logs.extend(run.logs);
        if let Some(e) = run.error {
            let which = if n == 0 {
                "Test script"
            } else {
                "Step test script"
            };
            out.errors.push(format!("{which}: {e}"));
        }
    }
    out.response = Some(Ok(response));
    out
}

fn apply_vars(
    ctx: &mut vars::Context,
    out: &mut StepOutcome,
    changes: &BTreeMap<String, Option<String>>,
) {
    for (name, value) in changes {
        match value {
            Some(v) => drop(ctx.variables.insert(name.clone(), v.clone())),
            None => drop(ctx.variables.remove(name)),
        }
        out.vars.insert(name.clone(), value.clone());
    }
}

/// A value from a response (R-TEST-4): `status`, `header:<name>`, or a `JSONPath`
/// expression on the JSON body (the first match; text as is, anything else as JSON).
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

/// Turns runtime variable changes into the map the app keeps (R-TEST-5).
pub fn apply_changes(
    runtime: &mut BTreeMap<String, String>,
    changes: &BTreeMap<String, Option<String>>,
) {
    for (name, value) in changes {
        match value {
            Some(v) => drop(runtime.insert(name.clone(), v.clone())),
            None => drop(runtime.remove(name)),
        }
    }
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
