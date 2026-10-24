//! Pre-request and test scripts (R-TEST-1..3, D3, D24, D28): JavaScript run by Boa.
//!
//! A script sees `req`, `res` (tests only), `vars`, `test`, `expect`, `console`, `info`
//! (R-TEST-16), `workflow` (R-FLOW-6) and `xml2Json` (R-TEST-18).
//! Everything else a browser or Node.js has (files, network, timers) does not exist, so
//! scripts can only compute. Runaway scripts are stopped by Boa's loop and recursion
//! limits. Inputs go in as one JSON value and results come back as one, so the script
//! API is plain JavaScript (the prelude below) rather than native bindings; only
//! `xml2Json` is native, since it takes any text the script has.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use boa_engine::{
    Context, JsArgs, JsNativeError, JsResult, JsValue, NativeFunction, Source, js_string,
};
use serde::{Deserialize, Serialize};

use crate::model::{KeyValue, Request, RequestBody, Response};

mod breakpoints;

/// Loop iterations a script may run in total before it is stopped.
const LOOP_LIMIT: u64 = 10_000_000;
/// How deep functions may call each other.
const RECURSION_LIMIT: usize = 400;
/// Scripts see at most this much of a response body.
const MAX_BODY_BYTES: usize = 10 * 1024 * 1024;
/// Source names, so errors and logs can tell the user's lines from the prelude's.
const SCRIPT_PATH: &str = "script";
const PRELUDE_PATH: &str = "prelude";

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

/// Where a script runs, for `info` (R-TEST-16): the iteration of a workflow run (from
/// 0) and how many there are. A request sent on its own is iteration 0 of 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ScriptInfo {
    pub iteration: usize,
    pub iterations: usize,
}

impl Default for ScriptInfo {
    fn default() -> Self {
        Self {
            iteration: 0,
            iterations: 1,
        }
    }
}

/// What a script asked a workflow run to do after its step (R-FLOW-6, D50): run the
/// step with this name next, or end the iteration. The last call wins.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FlowControl {
    Next(String),
    Stop,
}

/// Which of a request's scripts something came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptKind {
    PreRequest,
    Tests,
    /// A workflow step's own tests (R-FLOW-1).
    StepTests,
}

/// Lines to take a snapshot at in a request's scripts (R-TEST-10), counted from 1.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Breakpoints {
    pub pre_request: BTreeSet<u32>,
    pub tests: BTreeSet<u32>,
}

impl Breakpoints {
    pub fn is_empty(&self) -> bool {
        self.pre_request.is_empty() && self.tests.is_empty()
    }
}

/// `console.log`, `info`, `warn`, `error` or `debug`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Log,
    Info,
    Warn,
    Error,
    Debug,
}

/// One line of a script's console (R-TEST-9), in the order things happened. Values are
/// JSON; what JSON can't hold comes as `{"$type": "undefined" | "function" | "NaN" | …}`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ConsoleEntry {
    Log {
        level: LogLevel,
        args: Vec<serde_json::Value>,
        line: Option<u32>,
    },
    Test {
        name: String,
        passed: bool,
        message: Option<String>,
        line: Option<u32>,
    },
    /// A breakpoint was passed (R-TEST-10): the variables in scope there, by name.
    Break {
        line: u32,
        values: Vec<(String, serde_json::Value)>,
    },
    /// The error that stopped the script.
    Error { message: String, line: Option<u32> },
    /// Something the user should know, e.g. a breakpoint that could not be set.
    Note { message: String },
}

impl ConsoleEntry {
    /// A log line as plain text, the way a terminal shows it; `None` for other entries.
    pub fn log_text(&self) -> Option<String> {
        let Self::Log { args, .. } = self else {
            return None;
        };
        let text: Vec<String> = args
            .iter()
            .map(|a| match a {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            })
            .collect();
        Some(text.join(" "))
    }
}

/// What a script did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScriptRun {
    pub tests: Vec<TestResult>,
    /// Logs, tests, breakpoint hits and errors, in order (R-TEST-9).
    pub console: Vec<ConsoleEntry>,
    /// Variables set (`Some`) or unset (`None`) with `vars` (R-TEST-5).
    pub vars: BTreeMap<String, Option<String>>,
    /// Headers set with `req.setHeader` in a pre-request script (R-TEST-2).
    pub headers: Vec<KeyValue>,
    /// `workflow.next(…)` or `workflow.stop()`, the last one called (R-FLOW-6).
    pub flow: Option<FlowControl>,
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
    info: ScriptInfo,
    req: RequestView,
    res: Option<ResponseView>,
}

#[derive(Serialize)]
struct RequestView {
    name: String,
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
    /// Cookies the response set (R-TEST-16): name → value.
    cookies: BTreeMap<String, String>,
    body: String,
    time: u64,
    size: usize,
}

/// The script's view of the outcome; `ScriptRun` without the error.
#[derive(Deserialize)]
struct Output {
    tests: Vec<TestResult>,
    console: Vec<ConsoleEntry>,
    /// Console entries left out after the first [`MAX_CONSOLE`] (in the prelude).
    dropped: u64,
    vars: BTreeMap<String, Option<String>>,
    headers: Vec<KeyValue>,
    flow: Option<FlowControl>,
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
    run_with_breakpoints(
        script,
        phase,
        vars,
        request,
        response,
        ScriptInfo::default(),
        &BTreeSet::new(),
    )
}

/// [`run`] at `info`'s place in a workflow run, recording the variables in scope
/// whenever a line in `breakpoints` is passed (R-TEST-10).
pub fn run_with_breakpoints(
    script: &str,
    phase: Phase,
    vars: &BTreeMap<String, String>,
    request: &Request,
    response: Option<&Response>,
    info: ScriptInfo,
    breakpoints: &BTreeSet<u32>,
) -> ScriptRun {
    let instrumented = breakpoints::instrument(script, breakpoints);
    let notes = instrumented
        .skipped
        .iter()
        .map(|(line, why)| ConsoleEntry::Note {
            message: format!("The breakpoint on line {line} was not set: {why}."),
        });
    let mut run = run_code(&instrumented.code, phase, vars, request, response, info);
    run.console.splice(0..0, notes);
    run
}

fn run_code(
    script: &str,
    phase: Phase,
    vars: &BTreeMap<String, String>,
    request: &Request,
    response: Option<&Response>,
    info: ScriptInfo,
) -> ScriptRun {
    let input = Input {
        phase: match phase {
            Phase::PreRequest => "pre",
            Phase::Test => "test",
        },
        vars,
        info,
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
    if let Err(e) = context.register_global_callable(
        js_string!("xml2Json"),
        1,
        NativeFunction::from_fn_ptr(xml2json),
    ) {
        return ScriptRun {
            error: Some(format!("Tortuga's script setup failed: {e}")),
            ..ScriptRun::default()
        };
    }

    // JSON is a valid JavaScript expression, so the input goes in as a literal.
    let prelude = format!("const __input = {input};\n{PRELUDE}");
    if let Err(e) = context.eval(Source::from_bytes(&prelude).with_path(Path::new(PRELUDE_PATH))) {
        return ScriptRun {
            error: Some(format!("Tortuga's script setup failed: {e}")),
            ..ScriptRun::default()
        };
    }
    let error = context
        .eval(Source::from_bytes(script).with_path(Path::new(SCRIPT_PATH)))
        .err()
        .map(|e| {
            let text = e.to_string();
            (describe(&text), script_line(&text))
        });
    let output = context
        .eval(Source::from_bytes("JSON.stringify(__out)"))
        .ok()
        .and_then(|v| v.as_string().map(|s| s.to_std_string_escaped()))
        .and_then(|json| serde_json::from_str::<Output>(&json).ok());
    let mut run = match output {
        Some(mut out) => {
            if out.dropped > 0 {
                out.console.push(ConsoleEntry::Note {
                    message: format!("{} more console lines were left out.", out.dropped),
                });
            }
            ScriptRun {
                tests: out.tests,
                console: out.console,
                vars: out.vars,
                headers: out.headers,
                flow: out.flow,
                error: None,
            }
        }
        None => ScriptRun {
            error: Some("The script's results could not be read.".into()),
            ..ScriptRun::default()
        },
    };
    if let Some((message, line)) = error {
        run.error = Some(match line {
            Some(l) => format!("{message} (line {l})"),
            None => message.clone(),
        });
        run.console.push(ConsoleEntry::Error { message, line });
    }
    run
}

/// `xml2Json(text)` (R-TEST-18): XML text as an object, as Postman's gives it
/// ([`crate::xml::to_json`]). Text that isn't XML throws, saying where.
fn xml2json(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let text = args
        .get_or_undefined(0)
        .to_string(context)?
        .to_std_string_escaped();
    let json = crate::xml::to_json(&text).map_err(|e| {
        JsNativeError::typ().with_message(format!("xml2Json got text that is not XML. {e}"))
    })?;
    JsValue::from_json(&json, context)
}

/// The line of the user's script an error happened on: the innermost stack frame in it.
fn script_line(error: &str) -> Option<u32> {
    let marker = format!("({SCRIPT_PATH}:");
    error.match_indices(&marker).find_map(|(at, m)| {
        let rest = error.get(at + m.len()..)?;
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        digits.parse().ok()
    })
}

/// Whether `code` is valid JavaScript, checked without running it.
pub fn parses(code: &str) -> bool {
    boa_engine::Script::parse(Source::from_bytes(code), None, &mut Context::default()).is_ok()
}

/// A readable message for a script error.
fn describe(error: &str) -> String {
    let first = error.lines().next().unwrap_or(error).trim();
    if first.contains("RuntimeLimitError") && first.contains("iteration") {
        "The script ran too long (an endless loop?) and was stopped.".into()
    } else if first.contains("RuntimeLimitError") || first.contains("recursion") {
        "The script called functions too deeply (endless recursion?) and was stopped.".into()
    } else {
        // Boa adds the position, and "at <main> (…)" lines that say nothing to the user;
        // the line number is shown on its own.
        let position = format!(" ({SCRIPT_PATH}:");
        match first.find(&position) {
            Some(at) if first.ends_with(')') => first[..at].to_owned(),
            _ => first.to_owned(),
        }
    }
}

fn request_view(request: &Request) -> RequestView {
    let body = match &request.body {
        RequestBody::None => None,
        RequestBody::Text(t) | RequestBody::Json(t) | RequestBody::Xml(t) => Some(t.clone()),
        RequestBody::Form(fields) => Some(crate::query::encode_form(fields)),
        RequestBody::Multipart(_) => Some(String::new()),
    };
    RequestView {
        name: request.name.clone(),
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
        cookies: set_cookies(response),
        body: String::from_utf8_lossy(kept).into_owned(),
        time: u64::try_from(response.elapsed.as_millis()).unwrap_or(u64::MAX),
        size: response.body.len(),
    }
}

/// The cookies `response` sets: the name and value of each `Set-Cookie` header, the
/// last one for a name.
fn set_cookies(response: &Response) -> BTreeMap<String, String> {
    response
        .headers
        .iter()
        .filter(|h| h.is_named("set-cookie"))
        .filter_map(|h| {
            let pair = h.value.split(';').next()?;
            let (name, value) = pair.split_once('=')?;
            let name = name.trim();
            (!name.is_empty()).then(|| (name.to_owned(), value.trim().to_owned()))
        })
        .collect()
}

/// The script API (R-TEST-3), in JavaScript. `__input` is defined before it runs.
const PRELUDE: &str = r#"
const __out = { tests: [], console: [], dropped: 0, vars: {}, headers: [], flow: null };
const __MAX_CONSOLE = 1000;
const __MAX_HITS = 200;
let __hits = 0;

function __push(entry) {
  if (__out.console.length < __MAX_CONSOLE) __out.console.push(entry);
  else __out.dropped += 1;
}
// The line of the user's script in a stack: its innermost frame.
function __line(stack) {
  const m = /\(script:(\d+):/.exec(String(stack));
  return m ? Number(m[1]) : null;
}
// A value as JSON for the console; what JSON can't hold becomes {"$type": …}.
function __snap(v, depth, seen) {
  const d = depth === undefined ? 0 : depth;
  const s = seen || new Set();
  if (v === undefined) return { $type: "undefined" };
  if (typeof v === "function") return { $type: "function", name: v.name || "" };
  if (typeof v === "number" && !Number.isFinite(v)) return { $type: String(v) };
  if (typeof v === "bigint") return { $type: "bigint", value: String(v) };
  if (typeof v === "symbol") return { $type: "symbol", value: String(v) };
  if (v === null || typeof v !== "object") return v;
  if (v instanceof Date) return { $type: "date", value: String(v) };
  if (v instanceof RegExp) return { $type: "regexp", value: String(v) };
  if (v instanceof Error) return { $type: "error", value: String(v) };
  if (s.has(v)) return { $type: "circular" };
  if (d >= 8) return { $type: "more" };
  s.add(v);
  let out;
  if (Array.isArray(v) || v instanceof Set) {
    out = [...v].slice(0, 200).map((x) => __snap(x, d + 1, s));
  } else if (v instanceof Map) {
    out = {};
    for (const [k, x] of [...v].slice(0, 200)) out[String(k)] = __snap(x, d + 1, s);
  } else {
    out = {};
    for (const k of Object.keys(v).slice(0, 200)) out[k] = __snap(v[k], d + 1, s);
  }
  s.delete(v);
  return out;
}
// A breakpoint (R-TEST-10): records every name in `names` that `get` can read here.
function __bp(line, names, get) {
  if (__hits >= __MAX_HITS) return;
  __hits += 1;
  const values = [];
  for (const n of names) {
    let v;
    try { v = get(n); } catch (e) { continue; }
    if (typeof v === "function") continue;
    values.push([n, __snap(v)]);
  }
  __push({ kind: "break", line, values });
}
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
  replaceIn(text) {
    return String(text).replace(/\{\{\s*([^{}]+?)\s*\}\}/g, (whole, name) => {
      const v = __vars[name];
      return v === undefined || v === null ? whole : v;
    });
  },
  // The variables a script sees, by name (R-TEST-19), as Postman's `toObject()`.
  toObject() {
    return Object.fromEntries(Object.entries(__vars).filter(([, v]) => v !== null && v !== undefined));
  },
});

// New names are properties of the global object, not declarations, so a script that
// declares its own `info` or `workflow` (written before they existed) still runs.
globalThis.info = Object.freeze({
  event: __input.phase === "pre" ? "prerequest" : "test",
  requestName: __input.req.name,
  iteration: __input.info.iteration,
  iterationCount: __input.info.iterations,
});

globalThis.workflow = Object.freeze({
  next(name) {
    if (typeof name !== "string" || name.trim() === "") {
      throw new Error("workflow.next() needs the name of a step");
    }
    __out.flow = { next: name };
  },
  stop() {
    __out.flow = "stop";
  },
});

// Postman's `btoa` and `atob` (R-TEST-19): Base64 of text whose characters are bytes.
const __b64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
globalThis.btoa = function btoa(data) {
  const s = String(data);
  let out = "";
  for (let i = 0; i < s.length; i += 3) {
    const n = Math.min(3, s.length - i);
    const b = [0, 1, 2].map((j) => (j < n ? s.charCodeAt(i + j) : 0));
    if (b.some((c) => c > 255)) {
      throw new Error("btoa() takes characters up to U+00FF only; encode other text first");
    }
    const v = (b[0] << 16) | (b[1] << 8) | b[2];
    out += __b64[v >> 18] + __b64[(v >> 12) & 63];
    out += (n > 1 ? __b64[(v >> 6) & 63] : "=") + (n > 2 ? __b64[v & 63] : "=");
  }
  return out;
};
globalThis.atob = function atob(data) {
  const s = String(data).replace(/[\t\n\f\r ]/g, "").replace(/={1,2}$/, "");
  if (s.length % 4 === 1 || /[^A-Za-z0-9+/]/.test(s)) throw new Error("atob() got text that is not Base64");
  let out = "";
  let v = 0;
  let bits = 0;
  for (const c of s) {
    v = (v << 6) | __b64.indexOf(c);
    bits += 6;
    if (bits >= 8) {
      bits -= 8;
      out += String.fromCharCode((v >> bits) & 255);
      v &= (1 << bits) - 1;
    }
  }
  return out;
};

const console = (() => {
  const at = (level) => (...args) => {
    __push({ kind: "log", level, args: args.map((a) => __snap(a)), line: __line(new Error().stack) });
  };
  return Object.freeze({ log: at("log"), info: at("info"), warn: at("warn"), error: at("error"), debug: at("debug") });
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
  cookies: __input.res.cookies,
  body: __input.res.body,
  time: __input.res.time,
  size: __input.res.size,
  header(name) {
    return this.headers[String(name).toLowerCase()];
  },
  cookie(name) {
    return Object.prototype.hasOwnProperty.call(this.cookies, String(name)) ? this.cookies[String(name)] : undefined;
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
  const line = __line(new Error().stack);
  try {
    fn();
    __out.tests.push({ name: String(name), passed: true });
    __push({ kind: "test", name: String(name), passed: true, message: null, line });
  } catch (e) {
    const message = e && e.message !== undefined ? String(e.message) : String(e);
    __out.tests.push({ name: String(name), passed: false, message });
    // Where it failed: the user's line in the error's stack, else the test's own line.
    __push({ kind: "test", name: String(name), passed: false, message, line: __line(e && e.stack) || line });
  }
}
// Postman's `pm.test.skip` (R-TEST-19): the test does not run or count, the console says so.
test.skip = function skip(name) {
  __push({ kind: "note", message: "Skipped test: " + String(name) });
};

// `expect` takes Jest's style, `expect(x).toBe(1)`, and Chai's, `expect(x).to.equal(1)`
// (R-TEST-8). Both report "Expected <actual> [not] <what>".
function __type(v) {
  if (v === null) return "null";
  if (Array.isArray(v)) return "array";
  if (v instanceof RegExp) return "regexp";
  if (v instanceof Date) return "date";
  return typeof v;
}
function __includes(actual, item, deep) {
  const same = (x, y) => (deep ? __eq(x, y) : x === y);
  if (typeof actual === "string") return actual.includes(String(item));
  if (Array.isArray(actual)) return actual.some((x) => (deep ? __eq(x, item) : Object.is(x, item)));
  if (actual instanceof Set) return [...actual].some((x) => same(x, item));
  if (actual instanceof Map) return [...actual.values()].some((x) => same(x, item));
  if (actual !== null && typeof actual === "object" && item !== null && typeof item === "object") {
    return Object.keys(item).every((k) => k in actual && same(actual[k], item[k]));
  }
  return false;
}
function __size(v) {
  if (v instanceof Set || v instanceof Map) return v.size;
  return v === null || v === undefined ? undefined : v.length;
}

// A text as the second argument is Chai's message, put before a failure (R-COL-40).
function expect(actual, flags) {
  const f = Object.assign(
    { not: false, deep: false, nested: false, own: false, any: false, ordered: false, contains: false, message: "" },
    typeof flags === "string" ? { message: flags } : flags,
  );
  const with_ = (more) => expect(actual, Object.assign({}, f, more));
  const check = (ok, what) => {
    if (Boolean(ok) === f.not) {
      const message = "Expected " + __show(actual) + (f.not ? " not " : " ") + what;
      throw new Error(f.message ? f.message + ": " + message : message);
    }
  };
  const a = {};
  const chain = (name, get) => Object.defineProperty(a, name, { get, enumerable: false });
  const words = ["to", "be", "been", "is", "that", "which", "and", "has", "have", "with", "at", "of", "same", "but", "does", "still", "also"];
  for (const w of words) chain(w, () => a);
  chain("not", () => with_({ not: !f.not }));
  chain("deep", () => with_({ deep: true }));
  chain("nested", () => with_({ nested: true }));
  chain("own", () => with_({ own: true }));
  chain("any", () => with_({ any: true }));
  chain("all", () => with_({ any: false }));
  chain("ordered", () => with_({ ordered: true }));
  // Chai's checks that are properties, not calls: `.to.be.true`.
  const prop = (name, ok, what) => chain(name, () => { check(ok(), what); return a; });
  prop("ok", () => actual, "to be truthy");
  prop("true", () => actual === true, "to be true");
  prop("false", () => actual === false, "to be false");
  prop("null", () => actual === null, "to be null");
  prop("undefined", () => actual === undefined, "to be undefined");
  prop("NaN", () => Number.isNaN(actual), "to be NaN");
  prop("exist", () => actual !== null && actual !== undefined, "to exist");
  prop("empty", () => {
    if (typeof actual === "string" || Array.isArray(actual)) return actual.length === 0;
    if (actual instanceof Set || actual instanceof Map) return actual.size === 0;
    return actual !== null && typeof actual === "object" && Object.keys(actual).length === 0;
  }, "to be empty");

  const methods = {
    // Jest
    toBe(e) { check(Object.is(actual, e), "to be " + __show(e)); },
    toEqual(e) { check(__eq(actual, e), "to equal " + __show(e)); },
    toStrictEqual(e) { check(__eq(actual, e), "to equal " + __show(e)); },
    toBeTruthy() { check(actual, "to be truthy"); },
    toBeFalsy() { check(!actual, "to be falsy"); },
    toBeDefined() { check(actual !== undefined, "to be defined"); },
    toBeUndefined() { check(actual === undefined, "to be undefined"); },
    toBeNull() { check(actual === null, "to be null"); },
    toBeNaN() { check(Number.isNaN(actual), "to be NaN"); },
    toContain(item) { check(__includes(actual, item, false), "to contain " + __show(item)); },
    toContainEqual(item) { check(__includes(actual, item, true), "to contain " + __show(item)); },
    toMatch(re) {
      const r = re instanceof RegExp ? re : new RegExp(String(re));
      check(typeof actual === "string" && r.test(actual), "to match " + String(r));
    },
    toHaveLength(n) { check(__size(actual) === n, "to have length " + n); },
    toHaveProperty(path, ...value) {
      const p = __path(actual, path);
      const ok = p.found && (value.length === 0 || __eq(p.value, value[0]));
      check(ok, "to have property " + __show(path) + (value.length === 0 ? "" : " = " + __show(value[0])));
    },
    toBeGreaterThan(n) { check(actual > n, "to be greater than " + __show(n)); },
    toBeGreaterThanOrEqual(n) { check(actual >= n, "to be at least " + __show(n)); },
    toBeLessThan(n) { check(actual < n, "to be less than " + __show(n)); },
    toBeLessThanOrEqual(n) { check(actual <= n, "to be at most " + __show(n)); },
    toBeCloseTo(n, digits) {
      const d = digits === undefined ? 2 : digits;
      check(Math.abs(actual - n) < Math.pow(10, -d) / 2, "to be close to " + __show(n));
    },
    toBeInstanceOf(c) { check(actual instanceof c, "to be an instance of " + (c && c.name)); },

    // Chai
    equal(e) {
      if (f.deep) return a.eql(e);
      check(actual === e, "to equal " + __show(e));
      return a;
    },
    eql(e) { check(__eq(actual, e), "to deeply equal " + __show(e)); return a; },
    a(type) {
      const t = String(type).toLowerCase();
      check(__type(actual) === t, "to be " + (/^[aeiou]/.test(t) ? "an " : "a ") + t);
      return a;
    },
    property(name, ...value) {
      const p = f.nested
        ? __path(actual, name)
        : actual !== null && actual !== undefined
          && (f.own ? Object.prototype.hasOwnProperty.call(Object(actual), name) : name in Object(actual))
          ? { found: true, value: actual[name] }
          : { found: false };
      const matches = (v) => (f.deep ? __eq(v, value[0]) : v === value[0]);
      const ok = p.found && (value.length === 0 || matches(p.value));
      check(ok, "to have property " + __show(name) + (value.length === 0 ? "" : " = " + __show(value[0])));
      // Like Chai, what follows is about the property's value: `.property("id").that.is.a("number")`.
      return !f.not && value.length === 0 ? expect(p.value) : a;
    },
    match(re) { check(typeof actual === "string" && re.test(actual), "to match " + String(re)); return a; },
    above(n) { check(actual > n, "to be above " + __show(n)); return a; },
    least(n) { check(actual >= n, "to be at least " + __show(n)); return a; },
    below(n) { check(actual < n, "to be below " + __show(n)); return a; },
    most(n) { check(actual <= n, "to be at most " + __show(n)); return a; },
    within(lo, hi) { check(actual >= lo && actual <= hi, "to be within " + lo + ".." + hi); return a; },
    oneOf(list) {
      const ok = Array.isArray(list) && list.some((x) => (f.deep ? __eq(x, actual) : x === actual));
      check(ok, "to be one of " + __show(list));
      return a;
    },
    keys(...names) {
      const want = names.length === 1 && Array.isArray(names[0]) ? names[0]
        : names.length === 1 && names[0] !== null && typeof names[0] === "object" ? Object.keys(names[0])
        : names;
      const have = actual !== null && typeof actual === "object" ? Object.keys(actual) : [];
      const ok = f.any
        ? want.some((k) => have.includes(String(k)))
        : (f.contains || want.length === have.length) && want.every((k) => have.includes(String(k)));
      const what = f.any ? "to have any of the keys " : f.contains ? "to include the keys " : "to have the keys ";
      check(ok, what + __show(want));
      return a;
    },
    instanceOf(c) { check(actual instanceof c, "to be an instance of " + (c && c.name)); return a; },
    closeTo(n, delta) { check(Math.abs(actual - n) <= delta, "to be close to " + n + " ± " + delta); return a; },
    satisfy(fn) { check(fn(actual), "to satisfy " + String(fn)); return a; },
    string(s) { check(typeof actual === "string" && actual.includes(s), "to contain " + __show(s)); return a; },
    members(list) {
      const same = (x, y) => (f.deep ? __eq(x, y) : x === y);
      const have = Array.isArray(actual) ? actual : [];
      const want = Array.isArray(list) ? list : [];
      let ok;
      if (f.ordered) {
        ok = (f.contains || have.length === want.length) && want.every((x, i) => i < have.length && same(have[i], x));
      } else {
        const left = [...have];
        ok = want.every((x) => {
          const i = left.findIndex((y) => same(y, x));
          return i >= 0 && left.splice(i, 1).length === 1;
        }) && (f.contains || left.length === 0);
      }
      const what = f.contains ? "to include the members " : "to have the same members as ";
      check(Array.isArray(actual) && ok, what + (f.ordered ? "in order " : "") + __show(want));
      return a;
    },
    // `lengthOf(n)` asserts, and `length.above(n)` and the like compare the length (R-TEST-19).
    lengthOf: (() => {
      const lengthOf = (n) => { check(__size(actual) === n, "to have length " + n); return a; };
      const compare = (what, ok) => (...n) => {
        check(ok(__size(actual), ...n), "to have length " + what + " " + n.map(__show).join(".."));
        return a;
      };
      const cmp = {
        above: compare("above", (s, n) => s > n),
        least: compare("at least", (s, n) => s >= n),
        below: compare("below", (s, n) => s < n),
        most: compare("at most", (s, n) => s <= n),
        within: compare("within", (s, lo, hi) => s >= lo && s <= hi),
      };
      Object.assign(lengthOf, cmp, {
        gt: cmp.above, greaterThan: cmp.above, gte: cmp.least, lt: cmp.below, lessThan: cmp.below, lte: cmp.most,
      });
      lengthOf.at = lengthOf;
      return lengthOf;
    })(),
  };
  const aliases = {
    equals: "equal", eq: "equal", eqls: "eql", an: "a", length: "lengthOf", matches: "match",
    gt: "above", greaterThan: "above", gte: "least", greaterThanOrEqual: "least", lt: "below", lessThan: "below", lte: "most",
    lessThanOrEqual: "most", key: "keys", instanceof: "instanceOf", approximately: "closeTo",
    haveOwnProperty: "ownProperty",
  };
  Object.assign(a, methods);
  a.ownProperty = (name, ...value) => with_({ own: true }).property(name, ...value);
  for (const [alias, name] of Object.entries(aliases)) a[alias] = a[name];
  // `include(x)` asserts, and what follows it is about a subset (R-TEST-19):
  // `include.members([…])`, `include.all.keys(…)`. Made when used, since the subset's
  // `expect` has an `include` of its own.
  const include = () => Object.setPrototypeOf(
    (item) => { check(__includes(actual, item, f.deep), "to include " + __show(item)); return a; },
    with_({ contains: true }),
  );
  for (const name of ["include", "includes", "contain", "contains"]) chain(name, include);
  return a;
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

    fn logs(run: &ScriptRun) -> Vec<String> {
        run.console
            .iter()
            .filter_map(ConsoleEntry::log_text)
            .collect()
    }

    fn test_run(script: &str, res: &Response) -> ScriptRun {
        let req = Request::new(HttpMethod::Get, "https://api.test/users?page=2");
        run(script, Phase::Test, &BTreeMap::new(), &req, Some(res))
    }

    /// R-TEST-9: logs keep their level, line and values; tests and the error that stopped
    /// the script show where they happened.
    #[test]
    fn the_console_shows_levels_lines_values_tests_and_errors() {
        use serde_json::json;
        let res = response(200, r#"{"id": 7}"#);
        let out = test_run(
            "console.log(\"id\", res.json().id, {a: [1, undefined]});\n\
             console.warn(\"ü😀\");\n\
             test(\"ok\", () => expect(1).toBe(1));\n\
             test(\"bad\", () => {\n  expect(1).to.equal(2);\n});\n\
             null.x;",
            &res,
        );
        assert_eq!(
            out.console,
            [
                ConsoleEntry::Log {
                    level: LogLevel::Log,
                    args: vec![
                        json!("id"),
                        json!(7),
                        json!({"a": [1, {"$type": "undefined"}]})
                    ],
                    line: Some(1),
                },
                ConsoleEntry::Log {
                    level: LogLevel::Warn,
                    args: vec![json!("ü😀")],
                    line: Some(2),
                },
                ConsoleEntry::Test {
                    name: "ok".into(),
                    passed: true,
                    message: None,
                    line: Some(3),
                },
                ConsoleEntry::Test {
                    name: "bad".into(),
                    passed: false,
                    message: Some("Expected 1 to equal 2".into()),
                    line: Some(5),
                },
                ConsoleEntry::Error {
                    message: "TypeError: cannot convert 'null' or 'undefined' to object".into(),
                    line: Some(7),
                },
            ]
        );
        assert_eq!(
            out.error.as_deref(),
            Some("TypeError: cannot convert 'null' or 'undefined' to object (line 7)")
        );
    }

    /// R-TEST-10: a breakpoint records the variables in scope each time it is passed;
    /// lines where one can't go get a note; the script runs on.
    #[test]
    fn breakpoints_record_the_variables_in_scope() {
        use serde_json::json;
        let res = response(200, r#"{"items": [{"id": 1}, {"id": 2}]}"#);
        let script = "const body = res.json();\n\
                      for (const item of body.items) {\n\
                        const twice = item.id * 2;\n\
                        vars.set(\"last\", String(twice));\n\
                      }\n\
                      const x =\n  5;";
        let req = Request::new(HttpMethod::Get, "https://api.test/");
        let lines: BTreeSet<u32> = [4, 7].into();
        let out = run_with_breakpoints(
            script,
            Phase::Test,
            &BTreeMap::new(),
            &req,
            Some(&res),
            ScriptInfo::default(),
            &lines,
        );
        assert_eq!(out.error, None);
        assert_eq!(out.vars["last"].as_deref(), Some("4"), "the script ran on");
        let hits: Vec<&ConsoleEntry> = out
            .console
            .iter()
            .filter(|e| matches!(e, ConsoleEntry::Break { .. }))
            .collect();
        assert_eq!(hits.len(), 2, "once per pass through the loop");
        let ConsoleEntry::Break { line, values } = hits[1] else {
            unreachable!()
        };
        assert_eq!(*line, 4);
        let get = |name: &str| {
            values
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, v)| v.clone())
        };
        assert_eq!(get("twice"), Some(json!(4)));
        assert_eq!(get("item"), Some(json!({"id": 2})));
        assert_eq!(get("body"), Some(json!({"items": [{"id": 1}, {"id": 2}]})));
        assert_eq!(get("x"), None, "not initialised yet");
        assert_eq!(get("res"), None, "the script API is not shown");
        assert!(matches!(
            &out.console[0],
            ConsoleEntry::Note { message } if message.starts_with("The breakpoint on line 7 was not set")
        ));
    }

    #[test]
    fn endless_logging_and_breakpoints_are_capped() {
        let res = response(200, "{}");
        let req = Request::new(HttpMethod::Get, "https://api.test/");
        let out = run_with_breakpoints(
            "for (let i = 0; i < 5000; i++) {\n  console.log(i);\n}",
            Phase::Test,
            &BTreeMap::new(),
            &req,
            Some(&res),
            ScriptInfo::default(),
            &[2].into(),
        );
        assert_eq!(out.console.len(), 1001, "1000 kept and one note");
        assert!(
            matches!(
                out.console.last(),
                Some(ConsoleEntry::Note { message }) if message.starts_with("4200 more")
            ),
            "5000 logs and 200 breakpoint hits, of which 1000 are kept"
        );
        let hits = out
            .console
            .iter()
            .filter(|e| matches!(e, ConsoleEntry::Break { .. }))
            .count();
        assert!(hits <= 200);
    }

    /// R-TEST-8: a Chai-style test, as written for Postman, runs and sets its variable.
    #[test]
    fn chai_style_tests_run() {
        let res = response(
            200,
            r#"{"data": {"accessToken": "t-ü😀", "expiresAt": "2026-10-21T00:00:00Z"}}"#,
        );
        let out = test_run(
            r#"
            test("responseData to include accessToken", function () {
              const response = res.json();
              expect(response).to.be.an('object');
              expect(typeof response.data.accessToken).toBe("string");
              expect(typeof response.data.expiresAt).toBe("string");

              vars.set('cAccessToken', response.data.accessToken);
            });
            "#,
            &res,
        );
        assert_eq!(out.error, None);
        assert!(out.tests[0].passed, "{:?}", out.tests[0].message);
        assert_eq!(out.vars["cAccessToken"].as_deref(), Some("t-ü😀"));
    }

    /// R-TEST-8: Chai's chains, flags and assertions, each passing and failing.
    #[test]
    fn chai_assertions_pass_and_fail_with_reasons() {
        use std::fmt::Write as _;

        let res = response(
            200,
            r#"{"id": 7, "name": "Ann", "tags": ["a", "b"], "user": {"name": "Ann", "roles": [{"r": 1}]}, "none": null}"#,
        );
        let passing = [
            "expect(b.id).to.equal(7)",
            "expect(b.id).to.not.equal(8)",
            "expect(b.id).not.to.equal(8)",
            "expect(b.user).to.deep.equal({name: 'Ann', roles: [{r: 1}]})",
            "expect(b.user).to.eql({name: 'Ann', roles: [{r: 1}]})",
            "expect(b.tags).to.be.an('array').that.includes('a')",
            "expect(b.name).to.be.a('string')",
            "expect(b.none).to.be.null",
            "expect(b.missing).to.be.undefined",
            "expect(b.id).to.exist",
            "expect(b.none).to.not.exist",
            "expect([]).to.be.empty",
            "expect(b.tags).to.not.be.empty",
            "expect(true).to.be.true",
            "expect(b.id).to.be.ok",
            "expect(b).to.have.property('id', 7)",
            "expect(b).to.have.property('user').that.has.property('name').that.equals('Ann')",
            "expect(b).to.have.nested.property('user.name', 'Ann')",
            "expect(b).to.not.have.property('missing')",
            "expect(b).to.include({name: 'Ann'})",
            "expect(b.user.roles).to.deep.include({r: 1})",
            "expect(b.name).to.include('nn')",
            "expect(b.tags).to.have.lengthOf(2)",
            "expect(b.tags).to.have.length(2)",
            "expect(b.name).to.match(/^A/)",
            "expect(b.id).to.be.above(5).and.below(10)",
            "expect(b.id).to.be.at.least(7)",
            "expect(b.id).to.be.at.most(7)",
            "expect(b.id).to.be.within(1, 10)",
            "expect(b.id).to.be.oneOf([1, 7])",
            "expect(b.user).to.have.all.keys('name', 'roles')",
            "expect(b.user).to.have.any.keys('name', 'other')",
            "expect(0.1 + 0.2).to.be.closeTo(0.3, 0.001)",
            "expect(b.id).to.satisfy((n) => n % 7 === 0)",
            "expect(res.status).to.eq(200)",
            "expect(res.status).toBe(200)",
        ];
        let failing = [
            ("expect(b.id).to.equal(8)", "Expected 7 to equal 8"),
            ("expect(b.id).to.not.equal(7)", "Expected 7 not to equal 7"),
            (
                "expect(b.name).to.be.a('number')",
                "Expected \"Ann\" to be a number",
            ),
            (
                "expect(b.tags).to.be.an('object')",
                "Expected [\"a\",\"b\"] to be an object",
            ),
            ("expect(b.none).to.exist", "Expected null to exist"),
            (
                "expect(b).to.have.property('missing')",
                "to have property \"missing\"",
            ),
            ("expect(b.tags).to.have.lengthOf(3)", "to have length 3"),
            ("expect(b.id).to.be.above(7)", "Expected 7 to be above 7"),
            (
                "expect(b.user).to.have.all.keys('name')",
                "to have the keys",
            ),
            (
                "expect(b.user).to.equal({name: 'Ann', roles: [{r: 1}]})",
                "to equal",
            ),
        ];
        let mut script = String::from("const b = res.json();\n");
        for (i, check) in passing.iter().enumerate() {
            writeln!(script, "test(\"pass {i}\", () => {{ {check}; }});")
                .expect("writing to a String");
        }
        for (i, (check, _)) in failing.iter().enumerate() {
            writeln!(script, "test(\"fail {i}\", () => {{ {check}; }});")
                .expect("writing to a String");
        }
        let out = test_run(&script, &res);
        assert_eq!(out.error, None);
        for (t, check) in out.tests.iter().zip(passing) {
            assert!(t.passed, "{check}: {:?}", t.message);
        }
        for (t, (check, message)) in out.tests[passing.len()..].iter().zip(failing) {
            assert!(!t.passed, "{check} should fail");
            let got = t.message.as_deref().unwrap_or_default();
            assert!(got.contains(message), "{check}: {got}");
        }
        assert_eq!(out.tests.len(), passing.len() + failing.len());
    }

    /// R-TEST-19: what Postman scripts use besides R-TEST-8: Chai's `members`, the
    /// length compared with `length.above(…)`, and `include.keys`, passing and failing.
    #[test]
    fn members_lengths_and_included_keys() {
        use std::fmt::Write as _;

        let res = response(
            200,
            r#"{"tags": ["a", "b", "ü😀"], "user": {"id": 1, "name": "Ann"}}"#,
        );
        let passing = [
            "expect(b.tags).to.have.members(['ü😀', 'a', 'b'])",
            "expect(b.tags).to.include.members(['b'])",
            "expect(b.tags).to.contain.members(['a', 'ü😀'])",
            "expect(b.tags).to.have.ordered.members(['a', 'b', 'ü😀'])",
            "expect(b.tags).to.include.ordered.members(['a', 'b'])",
            "expect([{x: 1}, {y: 2}]).to.include.deep.members([{y: 2}])",
            "expect([{x: 1}]).to.have.deep.members([{x: 1}])",
            "expect(b.tags).to.not.have.members(['a'])",
            "expect([]).to.have.members([])",
            "expect(b.tags).to.have.length.above(2)",
            "expect(b.tags).to.have.lengthOf.at.least(3)",
            "expect(b.tags).to.have.length.below(4)",
            "expect(b.tags).to.have.lengthOf.within(1, 3)",
            "expect(b.tags).to.not.have.length.above(3)",
            "expect('ü😀').to.have.length.above(2)",
            "expect(b.tags).to.have.length(3)",
            "expect(b.user).to.include.keys('name')",
            "expect(b.user).to.contain.all.keys('id', 'name')",
            "expect(b.tags).to.include('a')",
        ];
        let failing = [
            (
                "expect(b.tags).to.have.members(['a', 'b'])",
                "to have the same members as [\"a\",\"b\"]",
            ),
            (
                "expect(b.tags).to.include.members(['c'])",
                "to include the members [\"c\"]",
            ),
            (
                "expect(b.tags).to.have.ordered.members(['b', 'a', 'ü😀'])",
                "in order",
            ),
            (
                "expect([{x: 1}]).to.have.members([{x: 1}])",
                "to have the same members",
            ),
            (
                "expect('x').to.include.members(['x'])",
                "to include the members",
            ),
            (
                "expect(b.tags).to.have.length.above(3)",
                "to have length above 3",
            ),
            (
                "expect(b.tags).to.have.lengthOf.within(4, 9)",
                "to have length within 4..9",
            ),
            ("expect(b.user).to.include.keys('x')", "to include the keys"),
        ];
        let mut script = String::from("const b = res.json();\n");
        for (i, check) in passing.iter().enumerate() {
            writeln!(script, "test(\"pass {i}\", () => {{ {check}; }});")
                .expect("writing to a String");
        }
        for (i, (check, _)) in failing.iter().enumerate() {
            writeln!(script, "test(\"fail {i}\", () => {{ {check}; }});")
                .expect("writing to a String");
        }
        let out = test_run(&script, &res);
        assert_eq!(out.error, None);
        for (t, check) in out.tests.iter().zip(passing) {
            assert!(t.passed, "{check}: {:?}", t.message);
        }
        for (t, (check, message)) in out.tests[passing.len()..].iter().zip(failing) {
            assert!(!t.passed, "{check} should fail");
            let got = t.message.as_deref().unwrap_or_default();
            assert!(got.contains(message), "{check}: {got}");
        }
        assert_eq!(out.tests.len(), passing.len() + failing.len());
    }

    /// R-TEST-19: `test.skip` neither runs nor counts its test and says so in the console;
    /// `btoa`/`atob` are Base64 as in Postman; `vars.toObject()` lists the variables.
    #[test]
    fn skipped_tests_base64_and_all_variables() {
        let vars = BTreeMap::from([
            ("user".to_owned(), "Zoë 😀".to_owned()),
            ("gone".to_owned(), "x".to_owned()),
        ]);
        let out = run(
            r#"
            test.skip("later", () => { throw new Error("ran"); });
            test("base64", () => {
              expect(btoa("user:pass")).toBe("dXNlcjpwYXNz");
              expect(btoa("")).toBe("");
              expect(btoa("a")).toBe("YQ==");
              expect(btoa("ab")).toBe("YWI=");
              expect(btoa("Zoë")).toBe("Wm/r");
              expect(atob("dXNlcjpwYXNz")).toBe("user:pass");
              expect(atob("YQ==")).toBe("a");
              expect(atob("YW\nI= ")).toBe("ab");
              expect(atob(btoa("ÿ\u0000x"))).toBe("ÿ\u0000x");
              expect(() => btoa("😀")).toThrow;
            });
            test("not base64", () => { atob("a#b"); });
            test("wide", () => { btoa("😀"); });
            vars.unset("gone");
            vars.set("n", 1);
            vars.set("all", JSON.stringify(vars.toObject()));
            "#,
            Phase::PreRequest,
            &vars,
            &Request::new(HttpMethod::Get, "https://api.test"),
            None,
        );
        assert_eq!(out.error, None);
        let tests: Vec<(&str, bool)> = out
            .tests
            .iter()
            .map(|t| (t.name.as_str(), t.passed))
            .collect();
        assert_eq!(
            tests,
            [("base64", true), ("not base64", false), ("wide", false)],
            "{:?}",
            out.tests
        );
        assert!(
            out.tests[1]
                .message
                .as_deref()
                .unwrap_or_default()
                .contains("not Base64")
        );
        assert!(
            out.tests[2]
                .message
                .as_deref()
                .unwrap_or_default()
                .contains("U+00FF")
        );
        assert!(out.console.iter().any(
            |e| matches!(e, ConsoleEntry::Note { message } if message == "Skipped test: later")
        ));
        assert_eq!(
            out.vars["all"].as_deref(),
            Some(r#"{"user":"Zoë 😀","n":"1"}"#)
        );
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
        assert_eq!(logs(&out), ["t-1"]);
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
        assert_eq!(logs(&out), [r#"signed {"ok":true} false"#]);
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

    /// R-TEST-16: `info` says where the script runs; scripts that declare their own
    /// `info` or `workflow` still run.
    #[test]
    fn info_says_where_the_script_runs() {
        let mut req = Request::new(HttpMethod::Get, "https://api.test/");
        req.name = "Get ü😀".into();
        let pre = run_with_breakpoints(
            "vars.set('i', [info.event, info.requestName, info.iteration, info.iterationCount].join('|'));",
            Phase::PreRequest,
            &BTreeMap::new(),
            &req,
            None,
            ScriptInfo {
                iteration: 2,
                iterations: 5,
            },
            &BTreeSet::new(),
        );
        assert_eq!(pre.error, None);
        assert_eq!(pre.vars["i"].as_deref(), Some("prerequest|Get ü😀|2|5"));

        let own = run(
            "const info = 1; let workflow = 2; vars.set('x', String(info + workflow));",
            Phase::PreRequest,
            &BTreeMap::new(),
            &req,
            None,
        );
        assert_eq!(own.error, None);
        assert_eq!(own.vars["x"].as_deref(), Some("3"));
    }

    /// R-TEST-16: the response's cookies, by name; malformed `Set-Cookie` headers
    /// are left out.
    #[test]
    fn cookies_the_response_set() {
        let mut res = response(200, "");
        res.headers = vec![
            KeyValue::new("Set-Cookie", "session=a=b; Path=/"),
            KeyValue::new("set-cookie", "no-equals-sign"),
            KeyValue::new("SET-COOKIE", " name = Zoë ; Secure"),
            KeyValue::new("Set-Cookie", "=nameless"),
        ];
        let req = Request::new(HttpMethod::Get, "https://api.test/");
        let out = run(
            r#"test("cookies", () => {
  expect(res.cookie("session")).toBe("a=b");
  expect(res.cookie("name")).toBe("Zoë");
  expect(res.cookie("missing")).toBeUndefined();
  expect(res.cookie("toString")).toBeUndefined();
  expect(Object.keys(res.cookies)).toEqual(["name", "session"]);
});"#,
            Phase::Test,
            &BTreeMap::new(),
            &req,
            Some(&res),
        );
        assert_eq!(out.error, None);
        assert_eq!(out.failed_tests(), 0, "{:?}", out.tests);
    }

    /// R-TEST-16: `vars.replaceIn` fills in `{{variables}}` it knows, also ones set
    /// earlier in the script, and leaves the others.
    #[test]
    fn replace_in_fills_in_known_variables() {
        let vars = BTreeMap::from([("user".to_owned(), "Zoë 😀".to_owned())]);
        let req = Request::new(HttpMethod::Get, "https://api.test/");
        let out = run(
            "vars.set('n', 7);\n\
             vars.set('a', vars.replaceIn('{{ user }} has {{n}} of {{missing}} {{}}'));\n\
             vars.set('b', vars.replaceIn(42));",
            Phase::PreRequest,
            &vars,
            &req,
            None,
        );
        assert_eq!(out.error, None);
        assert_eq!(
            out.vars["a"].as_deref(),
            Some("Zoë 😀 has 7 of {{missing}} {{}}")
        );
        assert_eq!(out.vars["b"].as_deref(), Some("42"));
    }

    /// R-FLOW-6: the last of `workflow.next`/`stop` counts; a name is required.
    #[test]
    fn workflow_control_from_scripts() {
        let req = Request::new(HttpMethod::Get, "https://api.test/");
        let flow = |code: &str| run(code, Phase::PreRequest, &BTreeMap::new(), &req, None);
        assert_eq!(flow("").flow, None);
        assert_eq!(
            flow("workflow.next('Get user');").flow,
            Some(FlowControl::Next("Get user".into()))
        );
        assert_eq!(
            flow("workflow.next('A'); workflow.stop();").flow,
            Some(FlowControl::Stop)
        );
        let empty = flow("workflow.next('  ');");
        assert!(
            empty
                .error
                .is_some_and(|e| e.contains("workflow.next() needs the name of a step"))
        );
    }

    /// R-TEST-18: `xml2Json` reads an XML body as Postman's does, and says why when the
    /// text isn't XML.
    #[test]
    fn xml2json_reads_xml_bodies() {
        let res = response(
            200,
            r#"<users count="2"><user id="1">Ann ü😀</user><user id="2">Bo</user></users>"#,
        );
        let run = test_run(
            r#"
const users = xml2Json(res.body).users;
test("count", () => expect(users.$.count).toBe("2"));
test("names", () => expect(users.user.map((u) => u._)).toEqual(["Ann ü😀", "Bo"]));
vars.set("first", users.user[0].$.id);
const xml2JsonIsAFunction = typeof xml2Json === "function";
console.log(xml2JsonIsAFunction);
"#,
            &res,
        );
        assert_eq!(run.error, None);
        assert!(run.tests.iter().all(|t| t.passed), "{:?}", run.tests);
        assert_eq!(run.vars["first"].as_deref(), Some("1"));
        assert_eq!(logs(&run), ["true"]);

        let run = test_run("xml2Json(res.body);", &response(200, r#"{"id": 1}"#));
        let error = run.error.expect("JSON is not XML");
        assert!(
            error.ends_with(
                "xml2Json got text that is not XML. Invalid XML at line 1, column 1: \
                 there is text before the root element (line 1)"
            ),
            "{error}"
        );

        // A script may still declare its own `xml2Json`, as ported Postman scripts do.
        let run = test_run(
            "function xml2Json(t) { return 1; }\nvars.set(\"x\", xml2Json(\"\"));",
            &res,
        );
        assert_eq!(run.error, None);
        assert_eq!(run.vars["x"].as_deref(), Some("1"));
    }
}
