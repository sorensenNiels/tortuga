//! Pre-request and test scripts (R-TEST-1..3, D3, D24, D28): JavaScript run by Boa.
//!
//! A script sees `req`, `res` (tests only), `vars`, `test`, `expect` and `console`.
//! Everything else a browser or Node.js has (files, network, timers) does not exist, so
//! scripts can only compute. Runaway scripts are stopped by Boa's loop and recursion
//! limits. Inputs go in as one JSON value and results come back as one, so the script
//! API is plain JavaScript (the prelude below) rather than native bindings.

use std::collections::BTreeMap;

use boa_engine::{Context, Source};
use serde::{Deserialize, Serialize};

use crate::model::{KeyValue, Request, RequestBody, Response};

/// Loop iterations a script may run in total before it is stopped.
const LOOP_LIMIT: u64 = 10_000_000;
/// How deep functions may call each other.
const RECURSION_LIMIT: usize = 400;
/// Scripts see at most this much of a response body.
const MAX_BODY_BYTES: usize = 10 * 1024 * 1024;

/// When a script runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Before the request is sent (R-TEST-2): may set variables and headers.
    PreRequest,
    /// After the response arrived (R-TEST-1).
    Test,
}

/// One `test(...)` (R-TEST-1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestResult {
    pub name: String,
    pub passed: bool,
    /// Why it failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// What a script did.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ScriptRun {
    pub tests: Vec<TestResult>,
    /// `console.log` output, one entry per call.
    pub logs: Vec<String>,
    /// Variables set (`Some`) or unset (`None`) with `vars` (R-TEST-5).
    pub vars: BTreeMap<String, Option<String>>,
    /// Headers set with `req.setHeader` in a pre-request script (R-TEST-2).
    pub headers: Vec<KeyValue>,
    /// The error that stopped the script, if any; tests run before it still count.
    pub error: Option<String>,
}

impl ScriptRun {
    pub fn failed_tests(&self) -> usize {
        self.tests.iter().filter(|t| !t.passed).count()
    }
}

#[derive(Serialize)]
struct Input<'a> {
    phase: &'static str,
    vars: &'a BTreeMap<String, String>,
    req: RequestView,
    res: Option<ResponseView>,
}

#[derive(Serialize)]
struct RequestView {
    method: &'static str,
    url: String,
    headers: Vec<KeyValue>,
    body: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ResponseView {
    status: u16,
    status_text: String,
    headers: Vec<KeyValue>,
    body: String,
    time: u64,
    size: usize,
}

/// The script's view of the outcome; `ScriptRun` without the error.
#[derive(Deserialize)]
struct Output {
    tests: Vec<TestResult>,
    logs: Vec<String>,
    vars: BTreeMap<String, Option<String>>,
    headers: Vec<KeyValue>,
}

/// Runs `script` with the current variables. `request` is shown as `req`; `response`
/// as `res` (only for [`Phase::Test`]). Never panics: errors end up in
/// [`ScriptRun::error`].
pub fn run(
    script: &str,
    phase: Phase,
    vars: &BTreeMap<String, String>,
    request: &Request,
    response: Option<&Response>,
) -> ScriptRun {
    let input = Input {
        phase: match phase {
            Phase::PreRequest => "pre",
            Phase::Test => "test",
        },
        vars,
        req: request_view(request),
        res: response.map(response_view),
    };
    let input = match serde_json::to_string(&input) {
        Ok(json) => json,
        Err(e) => {
            return ScriptRun {
                error: Some(e.to_string()),
                ..ScriptRun::default()
            };
        }
    };

    let mut context = Context::default();
    let limits = context.runtime_limits_mut();
    limits.set_loop_iteration_limit(LOOP_LIMIT);
    limits.set_recursion_limit(RECURSION_LIMIT);

    // JSON is a valid JavaScript expression, so the input goes in as a literal.
    let prelude = format!("const __input = {input};\n{PRELUDE}");
    if let Err(e) = context.eval(Source::from_bytes(&prelude)) {
        return ScriptRun {
            error: Some(format!("Tortuga's script setup failed: {e}")),
            ..ScriptRun::default()
        };
    }
    let error = context
        .eval(Source::from_bytes(script))
        .err()
        .map(|e| describe(&e.to_string()));
    let output = context
        .eval(Source::from_bytes("JSON.stringify(__out)"))
        .ok()
        .and_then(|v| v.as_string().map(|s| s.to_std_string_escaped()))
        .and_then(|json| serde_json::from_str::<Output>(&json).ok());
    match output {
        Some(out) => ScriptRun {
            tests: out.tests,
            logs: out.logs,
            vars: out.vars,
            headers: out.headers,
            error,
        },
        None => ScriptRun {
            error: error.or_else(|| Some("The script's results could not be read.".into())),
            ..ScriptRun::default()
        },
    }
}

/// A readable message for a script error.
fn describe(error: &str) -> String {
    let first = error.lines().next().unwrap_or(error).trim();
    if first.contains("RuntimeLimitError") && first.contains("iteration") {
        "The script ran too long (an endless loop?) and was stopped.".into()
    } else if first.contains("RuntimeLimitError") || first.contains("recursion") {
        "The script called functions too deeply (endless recursion?) and was stopped.".into()
    } else {
        // Boa adds "at <main> (unknown …)" lines that say nothing to the user.
        first.to_owned()
    }
}

fn request_view(request: &Request) -> RequestView {
    let body = match &request.body {
        RequestBody::None => None,
        RequestBody::Text(t) | RequestBody::Json(t) => Some(t.clone()),
        RequestBody::Form(fields) => Some(crate::query::encode_form(fields)),
        RequestBody::Multipart(_) => Some(String::new()),
    };
    RequestView {
        method: request.method.as_str(),
        url: crate::query::join_url(&request.url, &request.params),
        headers: request
            .headers
            .iter()
            .filter(|h| h.is_active())
            .cloned()
            .collect(),
        body,
    }
}

fn response_view(response: &Response) -> ResponseView {
    let kept = &response.body[..response.body.len().min(MAX_BODY_BYTES)];
    ResponseView {
        status: response.status,
        status_text: response.status_text.clone(),
        headers: response.headers.clone(),
        body: String::from_utf8_lossy(kept).into_owned(),
        time: u64::try_from(response.elapsed.as_millis()).unwrap_or(u64::MAX),
        size: response.body.len(),
    }
}

/// The script API (R-TEST-3), in JavaScript. `__input` is defined before it runs.
const PRELUDE: &str = r#"
const __out = { tests: [], logs: [], vars: {}, headers: [] };
const __vars = Object.assign({}, __input.vars);

function __fmt(v) {
  if (typeof v === "string") return v;
  try {
    const s = JSON.stringify(v);
    return s === undefined ? String(v) : s;
  } catch (e) {
    return String(v);
  }
}
function __show(v) {
  if (v === undefined) return "undefined";
  if (typeof v === "string") return JSON.stringify(v);
  return __fmt(v);
}
function __headers(list) {
  const h = {};
  for (const { name, value } of list) {
    const k = name.toLowerCase();
    h[k] = k in h ? h[k] + ", " + value : value;
  }
  return h;
}
function __eq(a, b) {
  if (Object.is(a, b)) return true;
  if (typeof a !== "object" || typeof b !== "object" || a === null || b === null) return false;
  if (Array.isArray(a) !== Array.isArray(b)) return false;
  const ka = Object.keys(a);
  const kb = Object.keys(b);
  if (ka.length !== kb.length) return false;
  return ka.every((k) => Object.prototype.hasOwnProperty.call(b, k) && __eq(a[k], b[k]));
}
function __path(obj, path) {
  const parts = Array.isArray(path) ? path : String(path).split(".");
  let cur = obj;
  for (const p of parts) {
    if (cur === null || cur === undefined || !(p in Object(cur))) return { found: false };
    cur = cur[p];
  }
  return { found: true, value: cur };
}

// `workspace.token` sets {{token}} for the whole workspace (R-WS-8).
function __var(name) {
  const n = String(name);
  return n.startsWith("workspace.") && n.length > 10 ? n.slice(10) : n;
}

const vars = Object.freeze({
  get(name) {
    const v = __vars[String(name)];
    return v === null ? undefined : v;
  },
  has(name) {
    const v = __vars[String(name)];
    return v !== undefined && v !== null;
  },
  set(name, value) {
    const v = typeof value === "string" ? value : __fmt(value);
    __vars[__var(name)] = v;
    __out.vars[String(name)] = v;
  },
  unset(name) {
    __vars[__var(name)] = null;
    __out.vars[String(name)] = null;
  },
});

const console = (() => {
  const log = (...args) => { __out.logs.push(args.map(__fmt).join(" ")); };
  return Object.freeze({ log, info: log, warn: log, error: log, debug: log });
})();

const req = {
  method: __input.req.method,
  url: __input.req.url,
  headers: __headers(__input.req.headers),
  body: __input.req.body,
  setHeader(name, value) {
    if (__input.phase !== "pre") throw new Error("req.setHeader() only works in a pre-request script");
    this.headers[String(name).toLowerCase()] = String(value);
    __out.headers.push({ name: String(name), value: String(value) });
  },
};

const res = __input.res && {
  status: __input.res.status,
  statusText: __input.res.statusText,
  headers: __headers(__input.res.headers),
  body: __input.res.body,
  time: __input.res.time,
  size: __input.res.size,
  header(name) {
    return this.headers[String(name).toLowerCase()];
  },
  json() {
    try {
      return JSON.parse(this.body);
    } catch (e) {
      throw new Error("The response body is not JSON: " + e.message);
    }
  },
};

function test(name, fn) {
  try {
    fn();
    __out.tests.push({ name: String(name), passed: true });
  } catch (e) {
    const message = e && e.message !== undefined ? String(e.message) : String(e);
    __out.tests.push({ name: String(name), passed: false, message });
  }
}

function expect(actual) {
  const matchers = (negate) => {
    const check = (ok, what) => {
      if (Boolean(ok) === negate) {
        throw new Error("Expected " + __show(actual) + (negate ? " not " : " ") + what);
      }
    };
    return {
      toBe(e) { check(Object.is(actual, e), "to be " + __show(e)); },
      toEqual(e) { check(__eq(actual, e), "to equal " + __show(e)); },
      toBeTruthy() { check(actual, "to be truthy"); },
      toBeFalsy() { check(!actual, "to be falsy"); },
      toBeDefined() { check(actual !== undefined, "to be defined"); },
      toBeUndefined() { check(actual === undefined, "to be undefined"); },
      toBeNull() { check(actual === null, "to be null"); },
      toContain(item) {
        const ok = typeof actual === "string"
          ? actual.includes(String(item))
          : Array.isArray(actual) && actual.some((x) => __eq(x, item));
        check(ok, "to contain " + __show(item));
      },
      toMatch(re) {
        const r = re instanceof RegExp ? re : new RegExp(String(re));
        check(typeof actual === "string" && r.test(actual), "to match " + String(r));
      },
      toHaveLength(n) {
        check(actual !== null && actual !== undefined && actual.length === n, "to have length " + n);
      },
      toHaveProperty(path, ...value) {
        const p = __path(actual, path);
        const ok = p.found && (value.length === 0 || __eq(p.value, value[0]));
        check(ok, "to have property " + __show(path) + (value.length === 0 ? "" : " = " + __show(value[0])));
      },
      toBeGreaterThan(n) { check(actual > n, "to be greater than " + __show(n)); },
      toBeGreaterThanOrEqual(n) { check(actual >= n, "to be at least " + __show(n)); },
      toBeLessThan(n) { check(actual < n, "to be less than " + __show(n)); },
      toBeLessThanOrEqual(n) { check(actual <= n, "to be at most " + __show(n)); },
    };
  };
  const m = matchers(false);
  m.not = matchers(true);
  return m;
}
"#;

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::model::HttpMethod;

    fn response(status: u16, body: &str) -> Response {
        Response {
            status,
            status_text: "OK".into(),
            headers: vec![KeyValue::new("Content-Type", "application/json")],
            body: body.as_bytes().to_vec(),
            elapsed: Duration::from_millis(42),
        }
    }

    fn test_run(script: &str, res: &Response) -> ScriptRun {
        let req = Request::new(HttpMethod::Get, "https://api.test/users?page=2");
        run(script, Phase::Test, &BTreeMap::new(), &req, Some(res))
    }

    /// R-TEST-1: passing and failing tests, with readable reasons.
    #[test]
    fn tests_pass_and_fail_with_reasons() {
        let res = response(
            201,
            r#"{"id": 7, "tags": ["a", "b"], "user": {"name": "Ann"}}"#,
        );
        let out = test_run(
            r#"
            test("created", () => expect(res.status).toBe(201));
            test("wrong status", () => expect(res.status).toBe(200));
            test("json", () => {
              const body = res.json();
              expect(body.id).toBeGreaterThan(5);
              expect(body.tags).toContain("b");
              expect(body).toHaveProperty("user.name", "Ann");
              expect(body.tags).toEqual(["a", "b"]);
              expect(body.missing).toBeUndefined();
              expect(res.header("content-type")).toMatch(/json/);
              expect(res.time).toBeLessThan(1000);
            });
            test("not", () => expect(res.status).not.toBe(201));
            "#,
            &res,
        );
        assert_eq!(out.error, None);
        let summary: Vec<(&str, bool)> = out
            .tests
            .iter()
            .map(|t| (t.name.as_str(), t.passed))
            .collect();
        assert_eq!(
            summary,
            [
                ("created", true),
                ("wrong status", false),
                ("json", true),
                ("not", false)
            ]
        );
        assert_eq!(
            out.tests[1].message.as_deref(),
            Some("Expected 201 to be 200")
        );
        assert_eq!(
            out.tests[3].message.as_deref(),
            Some("Expected 201 not to be 201")
        );
        assert_eq!(out.failed_tests(), 2);
    }

    /// R-WS-8: `vars.set("workspace.token", …)` is reported by that name, and the script
    /// itself sees it as `token`.
    #[test]
    fn workspace_variables_from_scripts() {
        let req = Request::new(HttpMethod::Get, "https://api.test/");
        let out = run(
            r#"
            vars.set("workspace.token", "t-1");
            console.log(vars.get("token"));
            "#,
            Phase::PreRequest,
            &BTreeMap::new(),
            &req,
            None,
        );
        assert_eq!(out.error, None);
        assert_eq!(
            out.vars,
            BTreeMap::from([("workspace.token".to_owned(), Some("t-1".to_owned()))])
        );
        assert_eq!(out.logs, ["t-1"]);
    }

    /// R-TEST-2, R-TEST-5: variables and headers set by scripts.
    #[test]
    fn variables_and_headers() {
        let vars = BTreeMap::from([("user".to_owned(), "ann".to_owned())]);
        let req = Request::new(HttpMethod::Post, "https://api.test/");
        let out = run(
            r#"
            vars.set("greeting", "hi " + vars.get("user"));
            vars.set("count", 3);
            vars.set("obj", { a: 1 });
            vars.unset("user");
            req.setHeader("X-Signature", "abc");
            console.log("signed", { ok: true }, vars.has("user"));
            "#,
            Phase::PreRequest,
            &vars,
            &req,
            None,
        );
        assert_eq!(out.error, None);
        assert_eq!(
            out.vars,
            BTreeMap::from([
                ("count".to_owned(), Some("3".to_owned())),
                ("greeting".to_owned(), Some("hi ann".to_owned())),
                ("obj".to_owned(), Some(r#"{"a":1}"#.to_owned())),
                ("user".to_owned(), None),
            ])
        );
        assert_eq!(out.headers, [KeyValue::new("X-Signature", "abc")]);
        assert_eq!(out.logs, [r#"signed {"ok":true} false"#]);
    }

    #[test]
    fn set_header_only_before_sending() {
        let out = test_run(r#"req.setHeader("A", "b");"#, &response(200, "{}"));
        assert!(
            out.error
                .as_deref()
                .is_some_and(|e| e.contains("pre-request")),
            "{:?}",
            out.error
        );
    }

    /// R-TEST-3: errors stop the script but keep earlier results; endless loops are stopped.
    #[test]
    fn errors_and_limits() {
        let res = response(200, "not json");
        let out = test_run(
            r#"test("first", () => {}); res.json(); test("never", () => {});"#,
            &res,
        );
        assert_eq!(out.tests.len(), 1);
        assert!(
            out.error.as_deref().is_some_and(|e| e.contains("not JSON")),
            "{:?}",
            out.error
        );

        let out = test_run("while (true) {}", &res);
        assert!(
            out.error.as_deref().is_some_and(|e| e.contains("too long")),
            "{:?}",
            out.error
        );

        let out = test_run("function f() { return f(); } f();", &res);
        assert!(
            out.error
                .as_deref()
                .is_some_and(|e| e.contains("too deeply")),
            "{:?}",
            out.error
        );

        let out = test_run("this is not javascript", &res);
        assert!(out.error.is_some());

        let out = test_run(
            r#"test("no files", () => expect(typeof require).toBe("undefined"));
               test("no fetch", () => expect(typeof fetch).toBe("undefined"));"#,
            &res,
        );
        assert_eq!(out.failed_tests(), 0, "{:?}", out.tests);
    }
}
