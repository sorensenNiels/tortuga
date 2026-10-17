//! Environments, servers and `{{variables}}` (R-VAR-1/2/4/5, R-SRV-1..4).

use std::collections::{BTreeMap, BTreeSet};

use crate::collection::Collection;
use crate::model::{KeyValue, Request, RequestBody};
use crate::{Error, Result};

/// Everything needed to turn a stored request into one that can be sent.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Context {
    /// Display name of the active environment, `None` if none is selected.
    pub environment: Option<String>,
    /// Server id → display name, for every server the collection defines.
    pub server_names: BTreeMap<String, String>,
    /// Server id → base URL in the active environment.
    pub server_urls: BTreeMap<String, String>,
    /// Effective variables: collection defaults overridden by the environment (R-VAR-4).
    pub variables: BTreeMap<String, String>,
}

impl Context {
    /// The context of `collection` with environment `environment_id` active.
    pub fn for_collection(collection: &Collection, environment_id: Option<&str>) -> Self {
        let env = environment_id.and_then(|id| collection.environment(id));
        let mut variables = collection.manifest.variables.clone();
        let mut server_urls = BTreeMap::new();
        if let Some(env) = env {
            // An empty value in the environment means "use the collection default" (R-VAR-7).
            variables.extend(
                env.variables
                    .iter()
                    .filter(|(_, v)| !v.is_empty())
                    .map(|(k, v)| (k.clone(), v.clone())),
            );
            server_urls.extend(
                env.servers
                    .iter()
                    .filter(|(_, v)| !v.trim().is_empty())
                    .map(|(k, v)| (k.clone(), v.clone())),
            );
        }
        Self {
            environment: env.map(|e| e.name.clone()),
            server_names: crate::collection::server_names(&collection.manifest),
            server_urls,
            variables,
        }
    }

    pub fn is_defined(&self, name: &str) -> bool {
        self.variables.contains_key(name)
    }
}

/// Resolves the server and all variables, giving a request with an absolute
/// URL that [`crate::Client::execute`] can send. Fails with every undefined
/// variable listed at once (R-VAR-5), or a missing server URL (R-SRV-4).
pub fn prepare(request: &Request, ctx: &Context) -> Result<Request> {
    let mut out = request.clone();
    if let Some(id) = &request.server {
        let name = ctx
            .server_names
            .get(id)
            .ok_or_else(|| Error::UnknownServer(id.clone()))?;
        let base = ctx
            .server_urls
            .get(id)
            .ok_or_else(|| Error::ServerUrlMissing {
                server: name.clone(),
                environment: ctx.environment.as_ref().map_or_else(
                    || "this collection (no environment selected)".to_owned(),
                    |e| format!("environment \"{e}\""),
                ),
            })?;
        out.url = join_base(base, &request.url);
        out.server = None;
    }

    let mut undefined = BTreeSet::new();
    out.url = substitute(&out.url, &ctx.variables, &mut undefined);
    substitute_all(&mut out.params, ctx, &mut undefined);
    substitute_all(&mut out.headers, ctx, &mut undefined);
    match &mut out.body {
        RequestBody::None => {}
        RequestBody::Text(t) | RequestBody::Json(t) => {
            *t = substitute(t, &ctx.variables, &mut undefined);
        }
        RequestBody::Form(fields) => substitute_all(fields, ctx, &mut undefined),
    }
    if undefined.is_empty() {
        Ok(out)
    } else {
        Err(Error::UndefinedVariables(undefined.into_iter().collect()))
    }
}

/// Undefined variables used anywhere in the request (for live warnings, R-VAR-5).
pub fn undefined_in(request: &Request, ctx: &Context) -> Vec<String> {
    let mut texts: Vec<&str> = vec![&request.url];
    let pairs = request
        .params
        .iter()
        .chain(&request.headers)
        .filter(|kv| kv.enabled);
    for kv in pairs {
        texts.extend([kv.name.as_str(), kv.value.as_str()]);
    }
    match &request.body {
        RequestBody::None => {}
        RequestBody::Text(t) | RequestBody::Json(t) => texts.push(t),
        RequestBody::Form(fields) => {
            for kv in fields.iter().filter(|kv| kv.enabled) {
                texts.extend([kv.name.as_str(), kv.value.as_str()]);
            }
        }
    }
    let set: BTreeSet<String> = texts
        .into_iter()
        .flat_map(names)
        .filter(|n| !ctx.is_defined(n))
        .map(str::to_owned)
        .collect();
    set.into_iter().collect()
}

/// The variable names used in `text`, in order of appearance.
pub fn names(text: &str) -> Vec<&str> {
    let mut found = Vec::new();
    scan(text, |part| {
        if let Part::Var(name) = part {
            found.push(name);
        }
    });
    found
}

/// Replaces `{{name}}` with its value. Unknown names are left as written and
/// added to `undefined`. `{{` without a valid name and `}}` is kept literally.
pub fn substitute(
    text: &str,
    vars: &BTreeMap<String, String>,
    undefined: &mut BTreeSet<String>,
) -> String {
    let mut out = String::with_capacity(text.len());
    scan(text, |part| match part {
        Part::Text(t) => out.push_str(t),
        Part::Var(name) => {
            if let Some(value) = vars.get(name) {
                out.push_str(value);
            } else {
                undefined.insert(name.to_owned());
                out.push_str("{{");
                out.push_str(name);
                out.push_str("}}");
            }
        }
    });
    out
}

fn substitute_all(pairs: &mut [KeyValue], ctx: &Context, undefined: &mut BTreeSet<String>) {
    for kv in pairs.iter_mut().filter(|kv| kv.enabled) {
        kv.name = substitute(&kv.name, &ctx.variables, undefined);
        kv.value = substitute(&kv.value, &ctx.variables, undefined);
    }
}

enum Part<'a> {
    Text(&'a str),
    Var(&'a str),
}

fn scan<'a>(text: &'a str, mut emit: impl FnMut(Part<'a>)) {
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else { break };
        let name = after[..end].trim();
        if name.is_empty() || name.contains(['{', '}']) {
            // Not a variable: keep the braces and continue after them.
            emit(Part::Text(&rest[..start + 2]));
            rest = after;
            continue;
        }
        emit(Part::Text(&rest[..start]));
        emit(Part::Var(name));
        rest = &after[end + 2..];
    }
    emit(Part::Text(rest));
}

/// `https://api.test/v1/` + `/users` → `https://api.test/v1/users`.
fn join_base(base: &str, path: &str) -> String {
    let base = base.trim().trim_end_matches('/');
    let path = path.trim();
    if path.is_empty() {
        base.to_owned()
    } else if path.starts_with('/') || path.starts_with('?') {
        format!("{base}{path}")
    } else {
        format!("{base}/{path}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::HttpMethod;

    fn ctx() -> Context {
        Context {
            environment: Some("Development".into()),
            server_names: BTreeMap::from([
                ("users".into(), "Users API".into()),
                ("auth".into(), "Auth".into()),
            ]),
            server_urls: BTreeMap::from([("users".into(), "http://localhost:{{port}}/".into())]),
            variables: BTreeMap::from([("port".into(), "8080".into()), ("id".into(), "42".into())]),
        }
    }

    #[test]
    fn substitutes_with_optional_spaces() {
        let mut undefined = BTreeSet::new();
        let vars = ctx().variables;
        assert_eq!(
            substitute("/users/{{id}}/{{ id }}", &vars, &mut undefined),
            "/users/42/42"
        );
        assert_eq!(
            substitute("{{}} {{a{b}} {{x", &vars, &mut undefined),
            "{{}} {{a{b}} {{x"
        );
        assert!(undefined.is_empty());
        assert_eq!(substitute("{{nope}}", &vars, &mut undefined), "{{nope}}");
        assert_eq!(undefined, BTreeSet::from(["nope".to_owned()]));
    }

    #[test]
    fn prepare_uses_server_base_url_and_variables() {
        let mut req = Request::new(HttpMethod::Post, "/users/{{id}}?x={{id}}");
        req.server = Some("users".into());
        req.headers.push(KeyValue::new("X-Id", "{{id}}"));
        req.headers.push(KeyValue {
            enabled: false,
            ..KeyValue::new("X-Off", "{{missing}}")
        });
        req.body = RequestBody::Json(r#"{"id": {{id}}}"#.into());

        let out = prepare(&req, &ctx()).expect("prepared");
        assert_eq!(out.server, None);
        assert_eq!(out.display_url(), "http://localhost:8080/users/42?x=42");
        assert_eq!(out.headers[0].value, "42");
        assert_eq!(out.body, RequestBody::Json(r#"{"id": 42}"#.into()));
    }

    #[test]
    fn prepare_reports_all_undefined_variables() {
        let mut req = Request::new(HttpMethod::Get, "https://a.test/{{b}}/{{a}}");
        req.headers
            .push(KeyValue::new("Authorization", "Bearer {{token}}"));
        let err = prepare(&req, &ctx()).expect_err("undefined");
        assert_eq!(err.to_string(), "undefined variables: a, b, token");
        assert_eq!(undefined_in(&req, &ctx()), ["a", "b", "token"]);
    }

    #[test]
    fn missing_server_url_names_server_and_environment() {
        let mut req = Request::new(HttpMethod::Get, "/login");
        req.server = Some("auth".into());
        assert_eq!(
            prepare(&req, &ctx()).expect_err("no url").to_string(),
            "server \"Auth\" has no URL in environment \"Development\""
        );

        req.server = Some("gone".into());
        assert!(matches!(prepare(&req, &ctx()), Err(Error::UnknownServer(id)) if id == "gone"));

        let no_env = Context {
            environment: None,
            server_urls: BTreeMap::new(),
            ..ctx()
        };
        req.server = Some("users".into());
        assert!(
            prepare(&req, &no_env)
                .expect_err("no env")
                .to_string()
                .contains("no environment selected")
        );
    }

    #[test]
    fn join_base_handles_slashes() {
        assert_eq!(
            join_base("http://a.test/v1/", "/users"),
            "http://a.test/v1/users"
        );
        assert_eq!(join_base("http://a.test", "users"), "http://a.test/users");
        assert_eq!(join_base("http://a.test/", ""), "http://a.test");
        assert_eq!(join_base("http://a.test", "?q=1"), "http://a.test?q=1");
    }
}
