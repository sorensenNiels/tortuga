//! Importing a `.http` file, as written for the VS Code REST Client or the JetBrains
//! HTTP Client, as a new project (R-COL-17, D7, D34, D37).
//!
//! The file becomes:
//! - a request per `###` block, in file order, named by the text after `###`, else a
//!   `# @name`, else the comment just above the request line;
//! - `@name = value` file variables as project variables; one that starts request URLs
//!   (`{{baseUrl}}/users`) as a server with its value as the base URL of a "Default"
//!   environment (D36);
//! - `Authorization` headers as auth with secret variables (D35): values written in the
//!   file go to [`Import::secret_values`], never into the project. When every request
//!   has the same auth, it becomes the project's;
//! - REST Client request variables (`{{login.response.body.$.token}}`) as a capture in
//!   the `login` request and a plain variable (R-TEST-4);
//! - JSON, form and text bodies; GraphQL requests (`X-Request-Type: GraphQL`) as JSON;
//! - response handler and pre-request scripts (`> {% … %}`, `< {% … %}`) as comments
//!   to port by hand (D28).
//!
//! What can't be imported as written is reported in [`Import::warnings`].
//! [`crate::import::create_project`] writes the result.

use std::collections::BTreeMap;

use crate::collection::NewItem;
use crate::format::{self, Manifest};
use crate::import::{Collected, Import, ImportKind, short_name, variable_name, variables_in};
use crate::model::{
    Auth, HttpMethod, KeyValue, Request, RequestBody, Scripts, is_json_content_type,
    is_xml_content_type,
};
use crate::{Error, Result, curl};

/// Reads a `.http` file into a project named after `file_name` (R-COL-17).
pub fn parse(text: &str, file_name: &str) -> Result<Import> {
    let text = text.trim_start_matches('\u{feff}').replace("\r\n", "\n");
    let blocks = blocks(&text);
    if !blocks.iter().any(|b| b.request_line.is_some()) {
        return Err(Error::InvalidHttpFile(
            "it has no requests: a request is a line such as `GET https://example.com/users`, \
             with `###` between requests"
                .into(),
        ));
    }
    Ok(Importer::default().run(&blocks, file_name))
}

/// Whether `text` reads as a `.http` file: its first line that is not blank, a comment,
/// a `###` separator or a variable is a request line.
pub(crate) fn looks_like_http(text: &str) -> bool {
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !is_comment(l) && !l.starts_with("###") && variable(l).is_none())
        .is_some_and(|line| {
            let first = line.split_whitespace().next().unwrap_or_default();
            line.starts_with("http://")
                || line.starts_with("https://")
                || (first.chars().all(|c| c.is_ascii_uppercase())
                    && first.parse::<HttpMethod>().is_ok()
                    && line.len() > first.len())
        })
}

/// One `###` block, split into its parts.
#[derive(Debug, Default)]
struct Block<'a> {
    /// The text after `###`.
    title: Option<&'a str>,
    /// `# @name`: what other requests call it in `{{name.response…}}`.
    id: Option<&'a str>,
    /// The last comment above the request line.
    comment: Option<&'a str>,
    variables: Vec<(&'a str, &'a str)>,
    no_cookies: bool,
    request_line: Option<String>,
    headers: Vec<(&'a str, &'a str)>,
    body: Vec<&'a str>,
    pre_request: Vec<String>,
    handler: Vec<String>,
    /// Script files (`> handler.js`) and body files (`< body.json`), left out.
    files: Vec<&'a str>,
}

fn is_comment(line: &str) -> bool {
    line.starts_with('#') || line.starts_with("//")
}

/// `@name = value`.
fn variable(line: &str) -> Option<(&str, &str)> {
    let (name, value) = line.strip_prefix('@')?.split_once('=')?;
    let name = name.trim();
    (!name.is_empty()
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || "_-.".contains(c)))
    .then(|| (name, value.trim()))
}

/// The directive of a `# @directive value` comment.
fn directive(line: &str) -> Option<(&str, &str)> {
    let text = line
        .strip_prefix("//")
        .or_else(|| line.strip_prefix('#'))?
        .trim();
    let text = text.strip_prefix('@')?;
    let (name, value) = text
        .split_once(|c: char| c.is_whitespace() || c == '=')
        .unwrap_or((text, ""));
    Some((name, value.trim().trim_start_matches('=').trim()))
}

/// Splits the file at `###` lines and reads each block's lines.
fn blocks(text: &str) -> Vec<Block<'_>> {
    let mut out = vec![Block::default()];
    let mut lines = text.lines().peekable();
    // 0 = before the request line, 1 = headers, 2 = body.
    let mut part = 0;
    while let Some(raw) = lines.next() {
        let line = raw.trim();
        if let Some(title) = line.strip_prefix("###") {
            let title = title.trim();
            out.push(Block {
                title: (!title.is_empty()).then_some(title),
                ..Block::default()
            });
            part = 0;
            continue;
        }
        let Some(block) = out.last_mut() else {
            continue;
        };
        // Scripts: `< {% … %}` before the request, `> {% … %}` after it.
        if let Some(rest) = line
            .strip_prefix("> {%")
            .or_else(|| line.strip_prefix("< {%"))
        {
            let mut code = rest.to_owned();
            while !code.contains("%}") {
                let Some(next) = lines.next() else { break };
                code.push('\n');
                code.push_str(next);
            }
            let code = dedent(code.split("%}").next().unwrap_or_default());
            if line.starts_with('<') && part == 0 {
                block.pre_request.push(code);
            } else {
                block.handler.push(code);
            }
            continue;
        }
        if part == 2 {
            if let Some(file) = line.strip_prefix("> ") {
                block.files.push(file.trim());
            } else if line.starts_with(">>") {
                // Saving the response to a file: nothing to keep.
            } else {
                block.body.push(raw);
            }
            continue;
        }
        if line.is_empty() {
            if part == 1 {
                part = 2;
            }
            continue;
        }
        if part == 0 {
            if let Some((name, value)) = directive(line) {
                match name {
                    "name" if !value.is_empty() => block.id = Some(value),
                    "no-cookie-jar" => block.no_cookies = true,
                    _ => {}
                }
            } else if is_comment(line) {
                let comment = line.trim_start_matches(['#', '/']).trim();
                if !comment.is_empty() {
                    block.comment = Some(comment);
                }
            } else if let Some(var) = variable(line) {
                block.variables.push(var);
            } else if is_request_line(line) {
                block.request_line = Some(line.to_owned());
                part = 1;
            }
            continue;
        }
        // Headers, and query lines (`?a=1`, `&b=2`) right after the request line.
        if is_comment(line) {
            continue;
        }
        if (line.starts_with('?') || line.starts_with('&')) && block.headers.is_empty() {
            if let Some(request_line) = &mut block.request_line {
                request_line.push_str(line);
            }
            continue;
        }
        match line.split_once(':') {
            Some((name, value))
                if !name.trim().is_empty() && !name.contains(char::is_whitespace) =>
            {
                block.headers.push((name.trim(), value.trim()));
            }
            // A body without the blank line before it.
            _ => {
                part = 2;
                block.body.push(raw);
            }
        }
    }
    out
}

#[derive(Default)]
struct Importer {
    c: Collected,
}

impl Importer {
    fn run(mut self, blocks: &[Block<'_>], file_name: &str) -> Import {
        // Variables apply to the whole file, wherever they are defined.
        for block in blocks {
            for (name, value) in &block.variables {
                self.c
                    .variables
                    .insert((*name).to_owned(), (*value).to_owned());
            }
        }
        let mut requests: Vec<(Option<&str>, Request)> = Vec::new();
        for block in blocks {
            if let Some(request) = self.request(block) {
                requests.push((block.id, request));
            }
        }
        self.request_variables(&mut requests);

        let mut manifest = Manifest::new(project_name(file_name));
        // The same auth on every request is the project's.
        let first = requests.first().and_then(|(_, r)| r.auth.clone());
        if let Some(auth) = first.filter(|a| !a.is_none())
            && requests.iter().all(|(_, r)| r.auth.as_ref() == Some(&auth))
        {
            for (_, request) in &mut requests {
                request.auth = None;
            }
            manifest.auth = auth;
        }
        let items = requests
            .into_iter()
            .map(|(_, r)| NewItem::Request(Box::new(r)))
            .collect();
        self.c.finish(ImportKind::Http, manifest, items)
    }

    fn request(&mut self, block: &Block<'_>) -> Option<Request> {
        let line = block.request_line.as_deref()?;
        let (method, url) = request_line(line);
        let display = block
            .title
            .or(block.id)
            .or(block.comment)
            .map_or_else(|| format!("{method} {url}"), str::to_owned);
        let name = short_name(display.trim_end_matches('.'));
        let Ok(method) = method.parse::<HttpMethod>() else {
            self.c.warn(format!(
                "“{name}” was left out: Tortuga does not send {method} requests."
            ));
            return None;
        };

        let mut headers: Vec<KeyValue> = Vec::new();
        let mut auth = None;
        let mut graphql = false;
        let mut host = None;
        for (header, value) in &block.headers {
            match header.to_ascii_lowercase().as_str() {
                "authorization" => auth = Some(self.authorization(value, &name)),
                "x-request-type" if value.eq_ignore_ascii_case("graphql") => graphql = true,
                "host" if url.starts_with('/') => host = Some(*value),
                lower if is_key_header(lower) => headers.push(KeyValue::new(
                    *header,
                    self.c.credential(value, &variable_name(header)),
                )),
                _ => headers.push(KeyValue::new(*header, *value)),
            }
        }
        // `GET /path` with a `Host` header.
        let url = match host {
            Some(host) if host.ends_with(":443") => format!("https://{host}{url}"),
            Some(host) => format!("http://{host}{url}"),
            None => url.to_owned(),
        };

        let mut request = Request::new(method, &url);
        request.name = name;
        if let Some((server, rest)) = self.c.server_for(&request.url) {
            request.server = Some(server);
            request.url = rest;
        }
        request.sync_path_variables();
        request.cookies = !block.no_cookies;
        let header_list: Vec<KeyValue> = headers.clone();
        request.headers = headers;
        request.body = self.body(block, &header_list, graphql, &request.name);
        if let Some(auth) = auth {
            match auth {
                Ok(auth) => request.auth = Some(auth),
                Err(header) => request.headers.push(header),
            }
        }
        request.scripts = Scripts {
            pre_request: self.scripts(&block.pre_request),
            tests: self.scripts(&block.handler),
        };
        for file in &block.files {
            self.c.warn(format!(
                "“{}” runs the script file {file} after the response, which was left out.",
                request.name
            ));
        }
        Some(request)
    }

    fn scripts(&mut self, scripts: &[String]) -> String {
        let code = scripts.join("\n\n");
        if code.trim().is_empty() {
            String::new()
        } else {
            self.c.commented_script(&code, ImportKind::Http)
        }
    }

    /// Bearer and basic auth with secret variables (D35); another scheme stays an
    /// `Authorization` header (the `Err`), its value a secret variable.
    fn authorization(&mut self, value: &str, request: &str) -> std::result::Result<Auth, KeyValue> {
        let (scheme, rest) = value.split_once(' ').unwrap_or((value, ""));
        let rest = rest.trim();
        match scheme.to_ascii_lowercase().as_str() {
            "bearer" => {
                return Ok(Auth::Bearer {
                    token: self.c.credential(rest, "token"),
                });
            }
            "basic" => {
                // `Basic user:password`, `Basic user password` (REST Client) or base64.
                let pair = rest
                    .split_once(' ')
                    .map(|(u, p)| (u.to_owned(), p.trim().to_owned()))
                    .or_else(|| {
                        (!variables_in(rest).is_empty() || rest.contains(':'))
                            .then(|| rest.split_once(':'))
                            .flatten()
                            .map(|(u, p)| (u.to_owned(), p.to_owned()))
                    })
                    .or_else(|| {
                        curl::basic_credentials(value).and_then(|decoded| {
                            decoded
                                .split_once(':')
                                .map(|(u, p)| (u.to_owned(), p.to_owned()))
                        })
                    });
                if let Some((username, password)) = pair {
                    return Ok(Auth::Basic {
                        username: self.c.credential(&username, "username"),
                        password: self.c.credential(&password, "password"),
                    });
                }
            }
            "digest" | "aws" => self.c.warn(format!(
                "“{request}” uses {scheme} auth, which Tortuga does not support: its \
                 Authorization header was kept as it is."
            )),
            _ => {}
        }
        Err(KeyValue::new(
            "Authorization",
            self.c.credential(value, "authorization"),
        ))
    }

    fn body(
        &mut self,
        block: &Block<'_>,
        headers: &[KeyValue],
        graphql: bool,
        request: &str,
    ) -> RequestBody {
        let text = block.body.join("\n").trim_end().to_owned();
        let text = text.trim_start_matches('\n');
        if text.trim().is_empty() {
            return RequestBody::None;
        }
        if let Some(file) = text.trim().strip_prefix('<')
            && !text.trim().contains('\n')
        {
            self.c.warn(format!(
                "The body of “{request}” is read from the file{}, which Tortuga can't send as \
                 a whole body yet, so it was left out.",
                file.trim_start_matches('@')
                    .split_whitespace()
                    .last()
                    .map_or_else(String::new, |f| format!(" {f}"))
            ));
            return RequestBody::None;
        }
        let content_type = headers
            .iter()
            .find(|h| h.is_named("content-type"))
            .map(|h| h.value.to_ascii_lowercase())
            .unwrap_or_default();
        if graphql {
            let (query, variables) = text.split_once("\n\n").unwrap_or((text, ""));
            let variables = serde_json::from_str(variables.trim())
                .unwrap_or_else(|_| serde_json::Value::Object(serde_json::Map::new()));
            let json = serde_json::json!({ "query": query.trim(), "variables": variables });
            return RequestBody::Json(serde_json::to_string_pretty(&json).unwrap_or_default());
        }
        if is_json_content_type(&content_type)
            || (content_type.is_empty() && text.trim_start().starts_with(['{', '[']))
        {
            return RequestBody::Json(text.to_owned());
        }
        if is_xml_content_type(&content_type) {
            return RequestBody::Xml(text.to_owned());
        }
        if content_type.starts_with("application/x-www-form-urlencoded") {
            let joined: String = text.lines().map(str::trim).collect();
            return RequestBody::Form(
                joined
                    .split('&')
                    .filter(|pair| !pair.is_empty())
                    .map(|pair| {
                        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
                        KeyValue::new(form_decode(name), form_decode(value))
                    })
                    .collect(),
            );
        }
        if content_type.starts_with("multipart/") && text.contains("\n<") {
            self.c.warn(format!(
                "The multipart body of “{request}” was kept as text, without the files it \
                 reads (< path): rebuild it as Multipart in the Body tab."
            ));
        }
        RequestBody::Text(text.to_owned())
    }

    /// REST Client request variables (`{{login.response.body.$.token}}`) become a
    /// capture in `login` and a plain variable, `{{login_token}}` (R-TEST-4).
    fn request_variables(&mut self, requests: &mut [(Option<&str>, Request)]) {
        let mut replace: BTreeMap<String, String> = BTreeMap::new();
        let mut captures: Vec<(usize, String, String)> = Vec::new();
        for (_, request) in requests.iter() {
            let text = format::write_request(request, None);
            for name in variables_in(&text) {
                let Some((id, rest)) = name.split_once(".response.") else {
                    continue;
                };
                if replace.contains_key(&name) {
                    continue;
                }
                let target = requests.iter().position(|(i, _)| *i == Some(id));
                let source = if let Some(path) = rest.strip_prefix("body.") {
                    path.starts_with('$').then(|| path.to_owned())
                } else {
                    rest.strip_prefix("headers.").map(|h| format!("header:{h}"))
                };
                let (Some(target), Some(source)) = (target, source) else {
                    self.c.warn(format!(
                        "“{}” uses {{{{{name}}}}}, which Tortuga can't fill in: capture the value \
                         into a variable in that request instead (Scripts tab).",
                        request.name
                    ));
                    continue;
                };
                let last = source
                    .rsplit(['.', ':'])
                    .next()
                    .filter(|s| !s.is_empty() && *s != "$")
                    .unwrap_or("value");
                let variable = variable_name(&format!("{id}_{last}"));
                replace.insert(name.clone(), variable.clone());
                captures.push((target, variable, source));
            }
        }
        for (target, variable, source) in captures {
            requests[target].1.capture.insert(variable, source);
        }
        if replace.is_empty() {
            return;
        }
        // A captured value is never written anywhere, so it needn't be secret.
        for from in replace.keys() {
            self.c.secrets.remove(from);
        }
        for (_, request) in requests.iter_mut() {
            let mut text = format::write_request(request, None);
            for (from, to) in &replace {
                text = text.replace(&format!("{{{{{from}}}}}"), &format!("{{{{{to}}}}}"));
            }
            if let Ok((changed, _)) = format::parse_request(&text, &request.name) {
                *request = changed;
            }
        }
    }
}

/// `code` without blank lines around it and without the indentation all lines share.
fn dedent(code: &str) -> String {
    let lines: Vec<&str> = code.lines().skip_while(|l| l.trim().is_empty()).collect();
    let indent = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);
    let lines: Vec<&str> = lines
        .iter()
        .map(|l| l.get(indent..).unwrap_or_else(|| l.trim_start()))
        .collect();
    lines.join("\n").trim_end().to_owned()
}

/// `METHOD URL`, or a URL alone.
fn is_request_line(line: &str) -> bool {
    let url = |text: &str| {
        ["http://", "https://", "{{", "/"]
            .iter()
            .any(|p| text.starts_with(p))
    };
    url(line)
        || line
            .split_once(char::is_whitespace)
            .is_some_and(|(method, rest)| {
                method.chars().all(|c| c.is_ascii_alphabetic()) && !rest.trim().is_empty()
            })
}

/// `METHOD URL [HTTP/1.1]`, or just the URL for a GET.
fn request_line(line: &str) -> (String, &str) {
    let line = match line.rsplit_once(' ') {
        Some((rest, version)) if version.starts_with("HTTP/") => rest.trim(),
        _ => line,
    };
    match line.split_once(char::is_whitespace) {
        Some((method, url))
            if method.chars().all(|c| c.is_ascii_alphabetic())
                && !method.eq_ignore_ascii_case("http")
                && !method.eq_ignore_ascii_case("https") =>
        {
            (method.to_ascii_uppercase(), url.trim())
        }
        _ => ("GET".to_owned(), line),
    }
}

/// Headers that carry a key or token, whose values become secret variables (D35).
fn is_key_header(lower: &str) -> bool {
    ["api-key", "apikey", "api_key", "token", "secret"]
        .iter()
        .any(|k| lower.contains(k))
}

fn form_decode(text: &str) -> String {
    percent_encoding::percent_decode_str(&text.replace('+', " "))
        .decode_utf8_lossy()
        .into_owned()
}

/// The file name without its extension, readable: `pokemon-api.http` → "Pokemon api".
fn project_name(file_name: &str) -> String {
    let stem = std::path::Path::new(file_name)
        .file_stem()
        .map(|s| {
            s.to_string_lossy()
                .trim_start_matches('.')
                .replace(['-', '_'], " ")
        })
        .unwrap_or_default();
    let mut chars = stem.trim().chars();
    chars.next().map_or_else(
        || "Imported requests".to_owned(),
        |first| first.to_uppercase().chain(chars).collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::{requests, script_note};

    /// The file from the request (R-COL-17).
    const POKEMON: &str = r#"# Pokémon, types and generations (the read API, Kysely)
# Start the app with "yarn start" and import some Pokémon first (imports.http).

@baseUrl = http://localhost:3000

### The first page of Pokémon, by Pokédex number
GET {{baseUrl}}/pokemon

### Filter by type and sort by stat total
GET {{baseUrl}}/pokemon?type=fire&sort=total&limit=5

### One Pokémon by name
GET {{baseUrl}}/pokemon/pikachu

### 404: not imported
GET {{baseUrl}}/pokemon/9999
"#;

    /// REST Client and JetBrains features in one file.
    const RICH: &str = r#"@host = api.shop.test
@token = s3cret-token

###
# Log in
# @name login
POST https://{{host}}/login HTTP/1.1
Content-Type: application/json

{
  "user": "ann",
  "password": "hunter2"
}

> {%
  client.test("ok", function () {
    client.assert(response.status === 200);
  });
%}

### Orders of the user
GET https://{{host}}/orders
  ?limit=10
  &page=2
Authorization: Bearer {{login.response.body.$.token}}
X-Request-Id: {{login.response.headers.X-Request-Id}}
# a header comment
Accept: application/json

### Form login
POST https://{{host}}/form
Authorization: Basic ann:hunter2
Content-Type: application/x-www-form-urlencoded

name=Ann+Lee&city=K%C3%B8ge
&debug=1

### Search
POST https://{{host}}/graphql
X-Request-Type: GraphQL
X-API-Key: key-123

query ($n: Int) { users(first: $n) { id } }

{"n": 2}

### Upload
# @no-cookie-jar
PUT https://{{host}}/upload
Content-Type: application/octet-stream

< ./avatar.png

### Old
GET /legacy?x=1
Host: legacy.test:443
Authorization: Digest ann hunter2

### Trace it
TRACE https://{{host}}/

### Uuid
POST https://{{host}}/ids
X-Id: {{$uuid}}
"#;

    fn find<'a>(import: &'a Import, name: &str) -> &'a Request {
        requests(&import.items)
            .into_iter()
            .find(|r| r.name == name)
            .unwrap_or_else(|| panic!("no request {name}"))
    }

    /// R-COL-17: the file from the request: a request per block, named by its `###`
    /// title, and `{{baseUrl}}` as a server (D36).
    #[test]
    fn requests_and_base_url() {
        let import = parse(POKEMON, "pokemon.http").expect("parse");
        assert_eq!(import.kind, ImportKind::Http);
        assert_eq!(import.manifest.name, "Pokemon");
        assert_eq!(import.requests, 4);
        let names: Vec<&str> = requests(&import.items)
            .iter()
            .map(|r| r.name.as_str())
            .collect();
        assert_eq!(
            names,
            [
                "The first page of Pokémon, by Pokédex number",
                "Filter by type and sort by stat total",
                "One Pokémon by name",
                "404: not imported"
            ]
        );
        let filter = find(&import, "Filter by type and sort by stat total");
        assert_eq!(filter.server.as_deref(), Some("baseurl"));
        assert_eq!(filter.url, "/pokemon");
        assert_eq!(
            filter.params,
            [
                KeyValue::new("type", "fire"),
                KeyValue::new("sort", "total"),
                KeyValue::new("limit", "5")
            ]
        );
        assert_eq!(
            import.environments[0].servers["baseurl"],
            "http://localhost:3000"
        );
        assert!(
            import.manifest.variables.is_empty(),
            "baseUrl is the server"
        );
        assert_eq!(import.manifest.auth, Auth::None);
        assert_eq!(import.warnings, Vec::<String>::new());
    }

    /// R-COL-17: names, query lines, auth from headers with secrets (D35), request
    /// variables as captures, bodies, scripts and directives.
    #[test]
    fn rest_client_and_jetbrains_features() {
        let import = parse(RICH, "shop.http").expect("parse");
        assert_eq!(import.requests, 7, "TRACE is left out");

        let login = find(&import, "login");
        assert_eq!(login.url, "https://{{host}}/login");
        assert_eq!(
            login.body,
            RequestBody::Json("{\n  \"user\": \"ann\",\n  \"password\": \"hunter2\"\n}".into())
        );
        assert!(
            login
                .scripts
                .tests
                .starts_with(&script_note(ImportKind::Http))
        );
        assert!(
            login
                .scripts
                .tests
                .contains("// client.test(\"ok\", function () {\n//   client.assert")
        );
        assert_eq!(
            login.capture,
            BTreeMap::from([
                ("login_token".to_owned(), "$.token".to_owned()),
                (
                    "login_x_request_id".to_owned(),
                    "header:X-Request-Id".to_owned()
                ),
            ])
        );

        let orders = find(&import, "Orders of the user");
        assert_eq!(
            orders.params,
            [KeyValue::new("limit", "10"), KeyValue::new("page", "2")]
        );
        assert_eq!(
            orders.auth,
            Some(Auth::Bearer {
                token: "{{login_token}}".into()
            })
        );
        assert_eq!(
            orders.headers,
            [
                KeyValue::new("X-Request-Id", "{{login_x_request_id}}"),
                KeyValue::new("Accept", "application/json")
            ]
        );

        let form = find(&import, "Form login");
        assert_eq!(
            form.auth,
            Some(Auth::Basic {
                username: "{{username}}".into(),
                password: "{{password}}".into()
            })
        );
        assert_eq!(
            form.body,
            RequestBody::Form(vec![
                KeyValue::new("name", "Ann Lee"),
                KeyValue::new("city", "Køge"),
                KeyValue::new("debug", "1"),
            ])
        );

        let search = find(&import, "Search");
        let RequestBody::Json(json) = &search.body else {
            panic!("graphql as json");
        };
        let json: serde_json::Value = serde_json::from_str(json).expect("json");
        assert_eq!(json["query"], "query ($n: Int) { users(first: $n) { id } }");
        assert_eq!(json["variables"]["n"], 2);
        assert_eq!(
            search.headers,
            [KeyValue::new("X-API-Key", "{{x_api_key}}")]
        );

        let upload = find(&import, "Upload");
        assert!(!upload.cookies);
        assert_eq!(upload.body, RequestBody::None);

        let old = find(&import, "Old");
        assert_eq!(old.url, "https://legacy.test:443/legacy");
        assert_eq!(old.params, [KeyValue::new("x", "1")]);
        assert_eq!(
            old.headers,
            [KeyValue::new("Authorization", "{{authorization}}")]
        );

        // Credentials are secret; values found in the file go to the secret store.
        assert_eq!(
            import.manifest.secrets,
            ["authorization", "password", "username", "x_api_key"]
        );
        assert_eq!(
            import.secret_values,
            BTreeMap::from([
                ("authorization".to_owned(), "Digest ann hunter2".to_owned()),
                ("password".to_owned(), "hunter2".to_owned()),
                ("username".to_owned(), "ann".to_owned()),
                ("x_api_key".to_owned(), "key-123".to_owned()),
            ])
        );
        // `@token` is not used for auth, so it stays a plain variable.
        assert_eq!(import.manifest.variables["token"], "s3cret-token");
        assert_eq!(import.manifest.variables["host"], "api.shop.test");
    }

    #[test]
    fn warnings() {
        let warnings = parse(RICH, "shop.http").expect("parse").warnings;
        let expect = [
            "“Trace it” was left out: Tortuga does not send TRACE requests.",
            "The body of “Upload” is read from the file ./avatar.png",
            "“Old” uses Digest auth",
            "variables such as {{$uuid}}",
            "1 script was kept as a comment: the HTTP Client's client API",
        ];
        for text in expect {
            assert!(
                warnings.iter().any(|w| w.contains(text)),
                "missing {text:?} in {warnings:#?}"
            );
        }
        assert_eq!(warnings.len(), expect.len(), "{warnings:#?}");
    }

    /// The same auth on every request is the project's; a request variable of a
    /// request that doesn't exist is a warning.
    #[test]
    fn shared_auth_and_unknown_requests() {
        let text = "### A\nGET https://a.test/a\nAuthorization: Bearer abc\n\n\
                    ### B\nGET https://a.test/b\nAuthorization: Bearer abc\n\
                    X-Id: {{nobody.response.body.$.id}}\n";
        let import = parse(text, "a.http").expect("parse");
        assert_eq!(
            import.manifest.auth,
            Auth::Bearer {
                token: "{{token}}".into()
            }
        );
        assert!(requests(&import.items).iter().all(|r| r.auth.is_none()));
        assert_eq!(import.secret_values["token"], "abc");
        assert!(import.warnings[0].contains("uses {{nobody.response.body.$.id}}"));
    }

    #[test]
    fn errors_and_detection() {
        let message =
            |text: &str| crate::error::message(&parse(text, "x.http").expect_err("refused"));
        assert!(message("# only a comment\n@a = 1\n").contains("no requests"));
        assert!(looks_like_http("# c\n@a = 1\n### x\nGET https://a.test\n"));
        assert!(looks_like_http("https://a.test/ping\n"));
        assert!(!looks_like_http("openapi: 3.0.0\n"));
        assert!(!looks_like_http("GET\n"));
        assert!(!looks_like_http("Get on with it\n"));
        assert!(is_request_line("{{base}}/x") && is_request_line("get /x"));
        assert!(!is_request_line("hello: 1") && !is_request_line("GET"));
    }

    #[test]
    fn helpers() {
        assert_eq!(dedent("\n  a {\n    b\n  }\n  "), "a {\n  b\n}");
        assert_eq!(
            request_line("GET https://a.test HTTP/1.1"),
            ("GET".into(), "https://a.test")
        );
        assert_eq!(
            request_line("https://a.test"),
            ("GET".into(), "https://a.test")
        );
        assert_eq!(request_line("delete /x"), ("DELETE".into(), "/x"));
        assert_eq!(directive("# @name login"), Some(("name", "login")));
        assert_eq!(directive("// @name=login"), Some(("name", "login")));
        assert_eq!(
            variable("@base_url = http://x"),
            Some(("base_url", "http://x"))
        );
        assert_eq!(variable("@ not a var"), None);
        assert_eq!(project_name("pokemon-api.http"), "Pokemon api");
        assert_eq!(project_name(""), "Imported requests");
    }
}
