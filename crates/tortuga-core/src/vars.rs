//! Environments, servers, `{{variables}}`, collection defaults and auth
//! (R-VAR-1..5, R-SRV-1..4, R-COL-4, R-HTTP-5), and dynamic variables such as
//! `{{$guid}}` (R-VAR-8, [`dynamic`]).

use std::collections::{BTreeMap, BTreeSet};

use crate::collection::Collection;
use crate::http::ClientSettings;
use crate::model::{Auth, KeyValue, Request, RequestBody};
use crate::settings::RequestSettings;
use crate::workspace::WorkspaceDefaults;
use crate::{Error, Result, query};

pub mod dynamic;

/// Everything needed to turn a stored request into one that can be sent.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Context {
    /// Display name of the active environment, `None` if none is selected.
    pub environment: Option<String>,
    /// Server id → display name, for every server the collection defines.
    pub server_names: BTreeMap<String, String>,
    /// Server id → base URL in the active environment.
    pub server_urls: BTreeMap<String, String>,
    /// Effective variables: collection defaults, overridden by the environment,
    /// overridden by local secret values (R-VAR-4).
    pub variables: BTreeMap<String, String>,
    /// Names of the collection's secret variables (R-VAR-3).
    pub secrets: BTreeSet<String>,
    /// Headers every request inherits (R-COL-4), from the project and the request's
    /// folders (R-COL-11).
    pub headers: Vec<KeyValue>,
    /// Where each of `headers` comes from, e.g. "the project" or "folder “Users”".
    pub header_origins: Vec<String>,
    /// Auth for requests that don't set their own (R-HTTP-5); `None` when no level sets
    /// any, so none is sent. `Some(Auth::None)` is a level saying "no auth".
    pub auth: Option<Auth>,
    /// Where `auth` comes from.
    pub auth_origin: String,
    /// The project folder, which relative paths of files to upload start from (R-HTTP-15, D21).
    pub base_dir: Option<std::path::PathBuf>,
    /// How requests are sent: the project's settings, then the workspace's, then the
    /// app's, as far as they have been added (R-SET-1). What is still unset falls
    /// back to the client's settings.
    pub settings: RequestSettings,
}

impl Context {
    /// The context of `collection` with environment `environment_id` active.
    pub fn for_collection(collection: &Collection, environment_id: Option<&str>) -> Self {
        Self::for_request(collection, environment_id, None)
    }

    /// The context of the request at `path` in `collection` (R-COL-11): what its folders
    /// define comes between the project and the request.
    pub fn for_request(
        collection: &Collection,
        environment_id: Option<&str>,
        path: Option<&std::path::Path>,
    ) -> Self {
        let env = environment_id.and_then(|id| collection.environment(id));
        Self::for_request_in(collection, env, path)
    }

    /// As [`Context::for_request`], with an environment that need not be one of the
    /// project's files, such as the mock's (R-MOCK-4).
    pub fn for_request_in(
        collection: &Collection,
        env: Option<&crate::format::Environment>,
        path: Option<&std::path::Path>,
    ) -> Self {
        let folders = path.map(|p| collection.folders_of(p)).unwrap_or_default();
        let mut variables = collection.manifest.variables.clone();
        // Nearest folder last, so it wins; an empty value means "inherit".
        for folder in &folders {
            variables.extend(
                folder
                    .defaults
                    .variables
                    .iter()
                    .filter(|(_, v)| !v.is_empty())
                    .map(|(k, v)| (k.clone(), v.clone())),
            );
        }
        let mut server_urls = BTreeMap::new();
        if let Some(env) = env {
            // An empty value in the environment means "use the default" (R-VAR-7).
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

        // Each level's headers, unless a deeper level names the same header.
        let levels: Vec<(String, &[KeyValue])> = std::iter::once((
            "the project".to_owned(),
            collection.manifest.headers.as_slice(),
        ))
        .chain(folders.iter().map(|f| {
            (
                format!("folder “{}”", f.name),
                f.defaults.headers.as_slice(),
            )
        }))
        .collect();
        let mut headers = Vec::new();
        let mut header_origins = Vec::new();
        for (i, (origin, level)) in levels.iter().enumerate() {
            for h in *level {
                let replaced = levels[i + 1..]
                    .iter()
                    .any(|(_, deeper)| deeper.iter().any(|d| same_name(d, h)));
                if !replaced {
                    headers.push(h.clone());
                    header_origins.push(origin.clone());
                }
            }
        }
        // The nearest level that says anything wins, also when it says "no auth".
        let (auth, auth_origin) = folders
            .iter()
            .rev()
            .find_map(|f| {
                let auth = f.defaults.auth.clone()?;
                Some((Some(auth), format!("folder “{}”", f.name)))
            })
            .unwrap_or_else(|| (collection.manifest.auth.clone(), "the project".to_owned()));
        Self {
            environment: env.map(|e| e.name.clone()),
            server_names: crate::collection::server_names(&collection.manifest),
            server_urls,
            variables,
            secrets: collection.manifest.secrets.iter().cloned().collect(),
            headers,
            header_origins,
            auth,
            auth_origin,
            base_dir: Some(collection.root.clone()),
            settings: collection.manifest.settings,
        }
    }

    /// Adds what the workspace gives every project below its own (R-SET-1, R-SET-2):
    /// settings the project leaves unset, headers no level below names, and auth when
    /// neither the folders nor the project set any.
    #[must_use]
    pub fn with_workspace_defaults(mut self, defaults: &WorkspaceDefaults) -> Self {
        self.settings = self.settings.or(defaults.settings);
        let inherited: Vec<KeyValue> = defaults
            .headers
            .iter()
            .filter(|h| !self.headers.iter().any(|own| same_name(own, h)))
            .cloned()
            .collect();
        let origins = vec!["the workspace".to_owned(); inherited.len()];
        self.headers.splice(0..0, inherited);
        self.header_origins.splice(0..0, origins);
        if self.auth.is_none() && !defaults.auth.is_none() {
            self.auth = Some(defaults.auth.clone());
            "the workspace".clone_into(&mut self.auth_origin);
        }
        self
    }

    /// Fills what no workspace or project sets from the app's settings, the last
    /// level (R-SET-1).
    #[must_use]
    pub fn with_app_settings(mut self, app: &ClientSettings) -> Self {
        self.settings = self.settings.or(RequestSettings::from_app(app));
        self
    }

    /// Adds this machine's secret values; they win over everything else (R-VAR-4).
    #[must_use]
    pub fn with_secrets(mut self, values: BTreeMap<String, String>) -> Self {
        self.variables.extend(values);
        self
    }

    /// Adds the workspace's variables (R-WS-8). They rank below every value the project
    /// defines, so they only fill in names that are still undefined. Names in `secrets`
    /// that are filled in this way are masked like the project's secrets (P6).
    #[must_use]
    pub fn with_workspace(
        mut self,
        values: &BTreeMap<String, String>,
        secrets: &BTreeSet<String>,
    ) -> Self {
        for (name, value) in values {
            if !self.variables.contains_key(name) {
                self.variables.insert(name.clone(), value.clone());
                if secrets.contains(name) {
                    self.secrets.insert(name.clone());
                }
            }
        }
        self
    }

    /// The auth `request` uses: its own, or what it inherits.
    pub fn auth_for<'a>(&'a self, request: &'a Request) -> &'a Auth {
        static NO_AUTH: Auth = Auth::None;
        request
            .auth
            .as_ref()
            .or(self.auth.as_ref())
            .unwrap_or(&NO_AUTH)
    }

    /// Collection headers `request` inherits: those it doesn't name itself.
    pub fn inherited_headers<'a>(
        &'a self,
        request: &'a Request,
    ) -> impl Iterator<Item = &'a KeyValue> {
        self.inherited_headers_with_origins(request).map(|(h, _)| h)
    }

    /// Like [`Self::inherited_headers`], with where each one comes from.
    pub fn inherited_headers_with_origins<'a>(
        &'a self,
        request: &'a Request,
    ) -> impl Iterator<Item = (&'a KeyValue, &'a str)> {
        self.headers
            .iter()
            .enumerate()
            .filter(|(_, h)| h.is_active() && !request.headers.iter().any(|own| same_name(own, h)))
            .map(|(i, h)| {
                (
                    h,
                    self.header_origins
                        .get(i)
                        .map_or("the project", String::as_str),
                )
            })
    }

    /// Whether `{{name}}` gets a value: a variable, or a dynamic variable (R-VAR-8).
    pub fn is_defined(&self, name: &str) -> bool {
        self.variables.contains_key(name) || dynamic::is_dynamic(name)
    }

    /// This context with every secret value replaced by `<secret:name>` (P6).
    #[must_use]
    pub fn masked(&self) -> Self {
        let mut out = self.clone();
        for name in &self.secrets {
            if let Some(value) = out.variables.get_mut(name)
                && !value.is_empty()
            {
                *value = format!("<secret:{name}>");
            }
        }
        out
    }
}

/// The request as it will be sent, to show it (R-HTTP-14): like [`prepare`], with the
/// Content-Type that sending adds, secret values shown as `<secret:name>`, and
/// undefined and dynamic variables left as `{{name}}` (a dynamic one gets a new value
/// on every send, R-VAR-8). Cookies from the jar are not included.
pub fn preview(request: &Request, ctx: &Context) -> Result<Request> {
    let mut ctx = ctx.masked();
    for name in names_in(request, &ctx) {
        let placeholder = format!("{{{{{name}}}}}");
        ctx.variables.entry(name).or_insert(placeholder);
    }
    let mut out = prepare(request, &ctx)?;
    if matches!(out.body, RequestBody::Multipart(_)) {
        // Always set when sending, with the boundary (R-HTTP-15).
        out.headers.retain(|h| !h.is_named("content-type"));
        out.headers.push(KeyValue::new(
            "Content-Type",
            "multipart/form-data; boundary=… (set when sending)",
        ));
    } else if !out.has_header("content-type")
        && let Some(content_type) = out.body.default_content_type()
    {
        out.headers
            .push(KeyValue::new("Content-Type", content_type));
    }
    Ok(out)
}

/// The full path of a file to upload: relative paths start at the project folder (D21).
fn file_path(path: &str, ctx: &Context) -> Result<String> {
    let path = path.trim();
    if path.is_empty() {
        return Err(Error::UploadFile {
            path: std::path::PathBuf::new(),
            reason: "no file chosen".into(),
        });
    }
    let as_path = std::path::Path::new(path);
    if as_path.is_absolute() {
        return Ok(path.to_owned());
    }
    match &ctx.base_dir {
        Some(dir) => Ok(dir.join(as_path).to_string_lossy().into_owned()),
        None => Err(Error::UploadFile {
            path: as_path.to_owned(),
            reason: "a relative path needs a project; the scratch request needs the full path"
                .into(),
        }),
    }
}

fn same_name(a: &KeyValue, b: &KeyValue) -> bool {
    a.is_named(&b.name)
}

/// Resolves the server and all variables, giving a request with an absolute
/// URL that [`crate::Client::execute`] can send. Fails with every undefined
/// variable listed at once (R-VAR-5), or a missing server URL (R-SRV-4).
pub fn prepare(request: &Request, ctx: &Context) -> Result<Request> {
    let mut out = request.clone();
    let mut undefined = BTreeSet::new();
    // The request's own timeout and "no cookies" win over the settings (R-SET-1).
    out.timeout_ms = request.timeout_ms.or(ctx.settings.timeout_ms);
    out.follow_redirects = ctx.settings.follow_redirects;
    out.cookies = request.cookies && ctx.settings.cookies.unwrap_or(true);

    // Path variables first, on the request's own URL (R-HTTP-10): their values may
    // use {{variables}}, and a server's base URL may contain `host:port`.
    let path_values: Vec<KeyValue> = request
        .path_vars
        .iter()
        .map(|v| {
            KeyValue::new(
                &v.name,
                substitute(&v.value, &ctx.variables, &mut undefined),
            )
        })
        .collect();
    let url = query::fill_path_variables(&request.url, &path_values)
        .map_err(Error::EmptyPathVariables)?;
    out.path_vars = Vec::new();
    out.url = url;

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
                    || "this project (no environment selected)".to_owned(),
                    |e| format!("environment \"{e}\""),
                ),
            })?;
        out.url = join_base(base, &out.url);
        out.server = None;
    }

    // Collection headers first, then the request's own (R-COL-4).
    out.headers = ctx
        .inherited_headers(request)
        .cloned()
        .chain(request.headers.iter().cloned())
        .collect();
    let mut auth = ctx.auth_for(request).clone();
    out.auth = None;

    out.url = substitute(&out.url, &ctx.variables, &mut undefined);
    substitute_all(&mut out.params, ctx, &mut undefined);
    substitute_all(&mut out.headers, ctx, &mut undefined);
    match &mut out.body {
        RequestBody::None => {}
        RequestBody::Text(t) | RequestBody::Json(t) | RequestBody::Xml(t) => {
            *t = substitute(t, &ctx.variables, &mut undefined);
        }
        RequestBody::Form(fields) => substitute_all(fields, ctx, &mut undefined),
        RequestBody::Multipart(parts) => {
            for part in parts.iter_mut() {
                part.name = substitute(&part.name, &ctx.variables, &mut undefined);
                part.value = substitute(&part.value, &ctx.variables, &mut undefined);
                if let Some(ct) = &mut part.content_type {
                    *ct = substitute(ct, &ctx.variables, &mut undefined);
                }
                if part.file {
                    part.value = file_path(&part.value, ctx)?;
                }
            }
        }
    }
    for field in auth.fields_mut() {
        *field = substitute(field, &ctx.variables, &mut undefined);
    }
    // A hand-written Authorization header wins (like Content-Type, R-HTTP-4).
    if !out.has_header("authorization")
        && let Some(value) = auth.header_value()
    {
        out.headers.push(KeyValue::new("Authorization", value));
    }
    if undefined.is_empty() {
        Ok(out)
    } else {
        Err(Error::UndefinedVariables(undefined.into_iter().collect()))
    }
}

/// Undefined variables used anywhere in the request (for live warnings, R-VAR-5).
pub fn undefined_in(request: &Request, ctx: &Context) -> Vec<String> {
    names_in(request, ctx)
        .into_iter()
        .filter(|n| !ctx.is_defined(n))
        .collect()
}

/// Every variable name used anywhere in the request, inherited headers and auth
/// included, sorted.
pub fn names_in(request: &Request, ctx: &Context) -> Vec<String> {
    let mut texts: Vec<&str> = vec![&request.url];
    texts.extend(ctx.auth_for(request).fields());
    texts.extend(request.path_vars.iter().map(|v| v.value.as_str()));
    let pairs = request
        .params
        .iter()
        .chain(&request.headers)
        .chain(ctx.inherited_headers(request))
        .filter(|kv| kv.enabled);
    for kv in pairs {
        texts.extend([kv.name.as_str(), kv.value.as_str()]);
    }
    match &request.body {
        RequestBody::None => {}
        RequestBody::Text(t) | RequestBody::Json(t) | RequestBody::Xml(t) => texts.push(t),
        RequestBody::Form(fields) => {
            for kv in fields.iter().filter(|kv| kv.enabled) {
                texts.extend([kv.name.as_str(), kv.value.as_str()]);
            }
        }
        RequestBody::Multipart(parts) => {
            for part in parts.iter().filter(|p| p.enabled) {
                texts.extend([part.name.as_str(), part.value.as_str()]);
                texts.extend(part.content_type.as_deref());
            }
        }
    }
    let set: BTreeSet<String> = texts
        .into_iter()
        .flat_map(names)
        .map(str::to_owned)
        .collect();
    set.into_iter().collect()
}

/// Where each `{{name}}` is in `text`: the byte range of the whole `{{…}}` and the
/// name, in order of appearance (R-VAR-9).
pub fn spans(text: &str) -> Vec<(std::ops::Range<usize>, &str)> {
    let mut found = Vec::new();
    let mut at = 0;
    scan(text, |part| match part {
        Part::Text(t) => at += t.len(),
        Part::Var(name) => {
            // `scan` gives the trimmed name; the braces and spaces are around it.
            let end = text
                .get(at..)
                .and_then(|rest| rest.find("}}"))
                .map_or(text.len(), |i| at + i + 2);
            found.push((at..end, name));
            at = end;
        }
    });
    found
}

/// Where a variable's value comes from (R-VAR-9), from the level that wins down.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// Captured or set by a script while the app runs (R-TEST-5).
    Runtime,
    /// Captured or set as `workspace.<name>` (R-WS-8).
    WorkspaceRuntime,
    /// This machine's value of a secret variable (R-VAR-3).
    Secret,
    /// The environment of that name (R-VAR-2).
    Environment(String),
    /// The folder of that name, at `path` in the project (R-COL-11).
    Folder {
        name: String,
        path: std::path::PathBuf,
    },
    /// The project's own value (`tortuga.toml`).
    Project,
    /// The workspace's variables (R-WS-8).
    Workspace,
    /// A dynamic variable: a new value at every use (R-VAR-8).
    Dynamic,
}

impl Source {
    /// "the environment “Staging”", to show after "From".
    pub fn describe(&self) -> String {
        match self {
            Self::Runtime => "a capture or script, kept while the app runs".to_owned(),
            Self::WorkspaceRuntime => {
                "a capture or script for the workspace, kept while the app runs".to_owned()
            }
            Self::Secret => "this computer's secret value".to_owned(),
            Self::Environment(name) => format!("the environment “{name}”"),
            Self::Folder { name, .. } => format!("the folder “{name}”"),
            Self::Project => "the project".to_owned(),
            Self::Workspace => "the workspace".to_owned(),
            Self::Dynamic => "a dynamic variable: a new value every time".to_owned(),
        }
    }
}

/// Which of the project's levels gives `name` its value for the request at `path` with
/// environment `env`: the environment, else the nearest folder, else the project; by
/// the same rules as [`Context::for_request_in`] (an empty value inherits).
pub fn project_source(
    collection: &Collection,
    env: Option<&crate::format::Environment>,
    path: Option<&std::path::Path>,
    name: &str,
) -> Option<Source> {
    let set = |vars: &BTreeMap<String, String>| vars.get(name).is_some_and(|v| !v.is_empty());
    if let Some(env) = env.filter(|e| set(&e.variables)) {
        return Some(Source::Environment(env.name.clone()));
    }
    let folders = path.map(|p| collection.folders_of(p)).unwrap_or_default();
    if let Some(folder) = folders.iter().rev().find(|f| set(&f.defaults.variables)) {
        return Some(Source::Folder {
            name: folder.name.clone(),
            path: folder.path.clone(),
        });
    }
    collection
        .manifest
        .variables
        .contains_key(name)
        .then_some(Source::Project)
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

/// Replaces `{{name}}` with its value, or a dynamic variable with a new value (R-VAR-8;
/// a variable of that name wins). Unknown names are left as written and added to
/// `undefined`. `{{` without a valid name and `}}` is kept literally.
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
            } else if let Some(value) = dynamic::value(name) {
                out.push_str(&value);
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
/// The host `request` is sent to in `ctx`, as far as it can be worked out from its server
/// and URL: `None` while the URL is incomplete (R-SET-3).
pub fn host(request: &Request, ctx: &Context) -> Option<String> {
    let url = match &request.server {
        Some(id) => join_base(ctx.server_urls.get(id)?, &request.url),
        None => request.url.clone(),
    };
    let url = substitute(&url, &ctx.variables, &mut BTreeSet::new());
    let parsed = url::Url::parse(url.trim()).ok()?;
    parsed.host_str().map(str::to_owned)
}

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
            ..Context::default()
        }
    }

    /// R-VAR-8: dynamic variables get a new value at every place; a variable of the
    /// same name wins; names Tortuga doesn't know stay undefined.
    #[test]
    fn dynamic_variables_are_filled_in() {
        let mut undefined = BTreeSet::new();
        let text = "{{$guid}} {{ $guid }} {{$randomInt 7 8}} {{$randomFirstName}} ü😀";
        let out = substitute(text, &BTreeMap::new(), &mut undefined);
        let words: Vec<&str> = out.split(' ').collect();
        assert_eq!(words[0].len(), 36);
        assert_ne!(words[0], words[1], "a new value each time");
        assert_eq!(words[2], "7");
        assert_eq!(words[3], "{{$randomFirstName}}");
        assert_eq!(undefined, BTreeSet::from(["$randomFirstName".to_owned()]));
        let own = BTreeMap::from([("$guid".to_owned(), "mine".to_owned())]);
        assert_eq!(substitute("{{$guid}}", &own, &mut BTreeSet::new()), "mine");

        let mut request = Request::new(crate::model::HttpMethod::Post, "https://a.test/{{$uuid}}");
        request.headers.push(KeyValue::new("X-Id", "{{$guid}}"));
        request.body =
            RequestBody::Json(r#"{"at": "{{$isoTimestamp}}", "who": "{{$nobody}}"}"#.into());
        let ctx = Context::default();
        assert_eq!(undefined_in(&request, &ctx), ["$nobody"]);
        // The Preview shows them as written: they change on every send.
        request.body = RequestBody::Json(r#"{"at": "{{$isoTimestamp}}"}"#.into());
        let preview = preview(&request, &ctx).expect("preview");
        assert_eq!(preview.url, "https://a.test/{{$uuid}}");
        assert_eq!(preview.header("x-id"), Some("{{$guid}}"));
        let sent = prepare(&request, &ctx).expect("prepare");
        assert!(!sent.url.contains("{{"), "{}", sent.url);
        assert_eq!(sent.header("x-id").map(str::len), Some(36));
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
    fn collection_headers_are_inherited_unless_the_request_names_them() {
        let ctx = Context {
            headers: vec![
                KeyValue::new("Accept", "application/json"),
                KeyValue::new("X-Team", "{{id}}"),
                KeyValue::new("X-Trace", "on"),
            ],
            ..ctx()
        };
        let mut req = Request::new(HttpMethod::Get, "https://a.test");
        req.headers.push(KeyValue::new("accept", "text/plain"));
        // A disabled header with the same name opts this request out of the default.
        req.headers.push(KeyValue {
            enabled: false,
            ..KeyValue::new("X-Trace", "")
        });
        let out = prepare(&req, &ctx).expect("prepared");
        let sent: Vec<(&str, &str)> = out
            .headers
            .iter()
            .filter(|h| h.is_active())
            .map(|h| (h.name.as_str(), h.value.as_str()))
            .collect();
        assert_eq!(sent, [("X-Team", "42"), ("accept", "text/plain")]);
    }

    #[test]
    fn auth_is_inherited_overridden_and_substituted() {
        let ctx = Context {
            auth: Some(Auth::Bearer {
                token: "{{id}}".into(),
            }),
            ..ctx()
        };
        let auth_header = |req: &Request| {
            prepare(req, &ctx)
                .expect("prepared")
                .headers
                .iter()
                .find(|h| h.name.eq_ignore_ascii_case("authorization"))
                .map(|h| h.value.clone())
        };
        let mut req = Request::new(HttpMethod::Get, "https://a.test");
        assert_eq!(auth_header(&req).as_deref(), Some("Bearer 42"), "inherited");

        req.auth = Some(Auth::None);
        assert_eq!(auth_header(&req), None, "explicitly none");

        req.auth = Some(Auth::Basic {
            username: "u".into(),
            password: "{{missing}}".into(),
        });
        assert_eq!(undefined_in(&req, &ctx), ["missing"]);
        assert!(prepare(&req, &ctx).is_err());

        req.auth = None;
        req.headers.push(KeyValue::new("Authorization", "Custom x"));
        assert_eq!(
            auth_header(&req).as_deref(),
            Some("Custom x"),
            "manual header wins"
        );
    }

    #[test]
    fn secret_values_win_over_environment_and_collection() {
        let ctx = ctx().with_secrets(BTreeMap::from([("id".into(), "secret-7".into())]));
        let req = Request::new(HttpMethod::Get, "https://a.test/{{id}}");
        assert_eq!(
            prepare(&req, &ctx).expect("prepared").url,
            "https://a.test/secret-7"
        );
    }

    /// R-WS-8: workspace variables only fill names the project leaves undefined.
    #[test]
    fn workspace_values_rank_below_the_project() {
        let workspace = BTreeMap::from([
            ("id".to_owned(), "from-workspace".to_owned()),
            ("tenant".to_owned(), "acme".to_owned()),
        ]);
        let secrets = BTreeSet::from(["tenant".to_owned(), "id".to_owned()]);
        let ctx = ctx().with_workspace(&workspace, &secrets);
        let req = Request::new(HttpMethod::Get, "https://a.test/{{tenant}}/{{id}}");
        assert_eq!(
            prepare(&req, &ctx).expect("prepared").url,
            "https://a.test/acme/42",
            "the project's id wins"
        );
        assert_eq!(undefined_in(&req, &ctx), Vec::<String>::new());
        assert_eq!(
            preview(&req, &ctx).expect("preview").url,
            "https://a.test/<secret:tenant>/42",
            "a workspace secret is masked; the project's own id is not"
        );
    }

    /// R-SET-1: the project's settings, then the workspace's, then the app's; the
    /// request's own timeout and "no cookies" win.
    #[test]
    fn settings_are_applied_when_prepared() {
        let mut ctx = ctx();
        ctx.settings.timeout_ms = Some(5_000);
        let workspace = WorkspaceDefaults {
            settings: RequestSettings {
                timeout_ms: Some(9_000),
                cookies: Some(false),
                ..RequestSettings::default()
            },
            ..WorkspaceDefaults::default()
        };
        let app = ClientSettings {
            follow_redirects: false,
            ..ClientSettings::default()
        };
        let ctx = ctx
            .with_workspace_defaults(&workspace)
            .with_app_settings(&app);
        let mut req = Request::new(HttpMethod::Get, "https://a.test/");
        let out = prepare(&req, &ctx).expect("prepared");
        assert_eq!(out.timeout_ms, Some(5_000), "the project's");
        assert_eq!(out.follow_redirects, Some(false), "the app's");
        assert!(!out.cookies, "the workspace's");
        req.timeout_ms = Some(100);
        assert_eq!(prepare(&req, &ctx).expect("prepared").timeout_ms, Some(100));
    }

    /// R-SET-2: workspace headers and auth come below the project's.
    #[test]
    fn workspace_headers_and_auth() {
        let workspace = WorkspaceDefaults {
            headers: vec![
                KeyValue::new("X-Tenant", "acme"),
                KeyValue::new("Accept", "text/plain"),
            ],
            auth: Auth::Bearer { token: "ws".into() },
            ..WorkspaceDefaults::default()
        };
        let mut ctx = ctx();
        ctx.headers = vec![KeyValue::new("accept", "application/json")];
        ctx.header_origins = vec!["the project".into()];
        let ctx = ctx.with_workspace_defaults(&workspace);
        let req = Request::new(HttpMethod::Get, "https://a.test/");
        let inherited: Vec<(String, String)> = ctx
            .inherited_headers_with_origins(&req)
            .map(|(h, o)| (format!("{}: {}", h.name, h.value), o.to_owned()))
            .collect();
        assert_eq!(
            inherited,
            [
                ("X-Tenant: acme".to_owned(), "the workspace".to_owned()),
                (
                    "accept: application/json".to_owned(),
                    "the project".to_owned()
                ),
            ]
        );
        assert_eq!(ctx.auth_origin, "the workspace");
        let out = prepare(&req, &ctx).expect("prepared");
        assert!(
            out.headers
                .contains(&KeyValue::new("Authorization", "Bearer ws"))
        );

        // The project's own auth wins.
        let mut own = super::tests::ctx();
        own.auth = Some(Auth::Bearer {
            token: "project".into(),
        });
        own.auth_origin = "the project".into();
        assert_eq!(
            own.with_workspace_defaults(&workspace).auth_origin,
            "the project"
        );

        // "No auth" in the project keeps the workspace's out.
        let mut none = super::tests::ctx();
        none.auth = Some(Auth::None);
        none.auth_origin = "the project".into();
        let none = none.with_workspace_defaults(&workspace);
        assert_eq!(none.auth_origin, "the project");
        let out = prepare(&req, &none).expect("prepared");
        assert!(!out.has_header("authorization"), "{:?}", out.headers);
    }

    #[test]
    fn path_variables_are_filled_before_the_server_base() {
        let mut req = Request::new(HttpMethod::Get, "/users/:user/orders/:order");
        req.server = Some("users".into());
        req.path_vars[0].value = "{{id}}".into();
        req.path_vars[1].value = "a/b".into();
        let out = prepare(&req, &ctx()).expect("prepared");
        assert_eq!(out.url, "http://localhost:8080/users/42/orders/a%2Fb");
        assert_eq!(out.path_vars, Vec::<KeyValue>::new());

        req.path_vars[1].value.clear();
        assert_eq!(
            prepare(&req, &ctx()).expect_err("empty").to_string(),
            "path variable without a value: order"
        );
    }

    /// R-SET-3: the host comes from the server's base URL and variables.
    #[test]
    fn host_of_a_request() {
        let ctx = ctx();
        let mut req = Request::new(HttpMethod::Get, "/users/{{id}}");
        assert_eq!(host(&req, &ctx), None, "no server");
        req.server = Some("users".into());
        assert_eq!(host(&req, &ctx).as_deref(), Some("localhost"));
        req.server = Some("auth".into());
        assert_eq!(host(&req, &ctx), None, "no base URL in this environment");
        let req = Request::new(HttpMethod::Get, "https://Api.{{id}}.test:8443/x");
        assert_eq!(host(&req, &ctx).as_deref(), Some("api.42.test"));
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

    /// R-HTTP-14: resolved, secrets masked, undefined left as typed, Content-Type added.
    #[test]
    fn preview_masks_secrets_and_keeps_undefined_names() {
        let ctx = Context {
            variables: BTreeMap::from([
                ("host".into(), "api.test".into()),
                ("token".into(), "s3cret".into()),
            ]),
            secrets: ["token".into()].into(),
            auth: Some(Auth::Bearer {
                token: "{{token}}".into(),
            }),
            ..Context::default()
        };
        let mut r = Request::new(crate::HttpMethod::Post, "https://{{host}}/u/{{missing}}");
        r.body = RequestBody::Json("{}".into());
        let shown = preview(&r, &ctx).expect("preview");
        assert_eq!(shown.url, "https://api.test/u/{{missing}}");
        assert_eq!(
            shown.headers,
            [
                KeyValue::new("Authorization", "Bearer <secret:token>"),
                KeyValue::new("Content-Type", "application/json"),
            ]
        );
        assert!(
            prepare(&r, &ctx).is_err(),
            "sending still stops on undefined variables"
        );
    }

    /// R-HTTP-15, D21: relative upload paths start at the project folder.
    #[test]
    fn upload_paths_are_resolved() {
        use crate::model::FormPart;

        let ctx = Context {
            base_dir: Some("/repo/api".into()),
            variables: BTreeMap::from([("dir".into(), "fixtures".into())]),
            ..Context::default()
        };
        let mut r = Request::new(HttpMethod::Post, "https://a.test/");
        r.body = RequestBody::Multipart(vec![
            FormPart::file("a", "{{dir}}/me.png"),
            FormPart::file("b", "/abs/x.bin"),
            FormPart::text("c", "{{dir}}"),
        ]);
        let RequestBody::Multipart(parts) = prepare(&r, &ctx).expect("prepare").body else {
            panic!("multipart")
        };
        let values: Vec<&str> = parts.iter().map(|p| p.value.as_str()).collect();
        let expected = std::path::Path::new("/repo/api").join("fixtures/me.png");
        assert_eq!(
            values,
            [expected.to_str().expect("utf-8"), "/abs/x.bin", "fixtures"]
        );

        let scratch = Context::default();
        let err = prepare(&r, &scratch).expect_err("relative path without a project");
        assert!(err.to_string().contains("full path"), "{err}");
    }

    /// R-VAR-9: each `{{name}}` with its place, spaces inside the braces included,
    /// after non-ASCII text, and not text that only looks like one.
    #[test]
    fn variables_are_found_with_their_place() {
        let text = "ü😀/{{ base }}/x{{id}}{{}}{{ open";
        let found = spans(text);
        let parts: Vec<(&str, &str)> = found
            .iter()
            .map(|(r, name)| (&text[r.clone()], *name))
            .collect();
        assert_eq!(parts, [("{{ base }}", "base"), ("{{id}}", "id")]);
        assert_eq!(spans(""), []);
    }

    /// R-VAR-9: the environment wins, then the nearest folder, then the project; an
    /// empty value inherits.
    #[test]
    fn the_source_of_a_value() {
        use crate::storage::FsStorage;
        let dir = tempfile::tempdir().expect("dir");
        let c = Collection::create(&FsStorage, dir.path(), "C").expect("create");
        let mut manifest = c.manifest.clone();
        manifest.variables = [
            ("a".to_owned(), "p".to_owned()),
            ("b".to_owned(), "p".to_owned()),
            ("c".to_owned(), "p".to_owned()),
        ]
        .into();
        c.save_config(&FsStorage, &manifest, &[]).expect("config");
        let folder = c
            .new_folder(&FsStorage, std::path::Path::new(""), "Users ü")
            .expect("folder");
        let c = Collection::open(&FsStorage, dir.path()).expect("open");
        let defaults = crate::format::FolderDefaults {
            variables: [
                ("b".to_owned(), "f".to_owned()),
                ("c".to_owned(), String::new()),
            ]
            .into(),
            ..crate::format::FolderDefaults::default()
        };
        c.save_folder_defaults(&FsStorage, &folder, &defaults)
            .expect("defaults");
        let c = Collection::open(&FsStorage, dir.path()).expect("open");
        let env = crate::format::Environment {
            name: "Staging".into(),
            variables: [
                ("a".to_owned(), "e".to_owned()),
                ("b".to_owned(), String::new()),
            ]
            .into(),
            ..crate::format::Environment::default()
        };
        let request = folder.join("get.toml");
        let at = |name| project_source(&c, Some(&env), Some(&request), name);
        assert_eq!(at("a"), Some(Source::Environment("Staging".into())));
        assert!(matches!(at("b"), Some(Source::Folder { name, .. }) if name == "Users ü"));
        assert_eq!(
            at("c"),
            Some(Source::Project),
            "an empty folder value inherits"
        );
        assert_eq!(at("nope"), None);
        assert_eq!(dynamic::describe("$guid"), Some("a random UUID"));
        assert!(
            dynamic::SUGGESTED
                .iter()
                .all(|(n, _)| dynamic::is_dynamic(n))
        );
    }
}
