//! Paste a `curl` command to fill a request (R-HTTP-11, D7, D19).
//!
//! Understands the commands people copy from API docs, logs and browsers ("Copy as
//! cURL (bash)"): POSIX shell quoting (`'…'`, `"…"`, `$'…'`), backslash line
//! continuations, and the options that describe a request. Options that only change
//! how curl itself behaves are skipped quietly; options Tortuga can't use are listed.

use crate::model::{
    Auth, FormPart, HttpMethod, KeyValue, Request, RequestBody, is_json_content_type,
    is_xml_content_type,
};
use crate::{query, vars};

/// A request read from a `curl` command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pasted {
    pub request: Request,
    /// Options that were not used, each with the reason, e.g. `-F (multipart bodies are not supported yet)`.
    pub ignored: Vec<String>,
}

/// Whether `text` looks like a `curl` command (and not a URL).
pub fn is_command(text: &str) -> bool {
    let mut words = text.split_whitespace();
    words.next().is_some_and(is_curl) && words.next().is_some()
}

/// `curl`, `curl.exe`, or a path to either.
fn is_curl(word: &str) -> bool {
    let program = word.rsplit(['/', '\\']).next().unwrap_or(word);
    matches!(program, "curl" | "curl.exe")
}

/// Reads a `curl` command. Fails if it can't be split into words or has no URL.
pub fn parse(text: &str) -> Result<Pasted, String> {
    let words = split_words(text)?;
    let mut words = words.into_iter();
    if !words.next().is_some_and(|first| is_curl(&first)) {
        return Err("this is not a curl command".into());
    }

    let mut b = Builder::default();
    let mut rest: Vec<String> = words.collect();
    rest.reverse(); // pop() takes the next word
    while let Some(word) = rest.pop() {
        if word == "--" {
            while let Some(url) = rest.pop() {
                b.url.get_or_insert(url);
            }
        } else if let Some(long) = word.strip_prefix("--") {
            b.long_option(long, &mut rest)?;
        } else if word.len() > 1 && word.starts_with('-') {
            b.short_options(&word[1..], &mut rest)?;
        } else if b.url.is_none() {
            b.url = Some(word);
        } else {
            b.ignore(&word, "a second URL");
        }
    }
    b.finish()
}

/// Uses one of the project's servers if the URL starts with its base URL in the active
/// environment; the URL then keeps only the path (R-HTTP-11, R-SRV-3).
pub fn use_server(request: &mut Request, ctx: &vars::Context) {
    if request.server.is_some() {
        return;
    }
    let mut undefined = std::collections::BTreeSet::new();
    // The longest base URL wins, so `/v2` beats the bare host.
    let best = ctx
        .server_urls
        .iter()
        .map(|(id, base)| {
            let base = vars::substitute(base, &ctx.variables, &mut undefined);
            (id, base.trim_end_matches('/').to_owned())
        })
        .filter(|(_, base)| !base.is_empty())
        .filter(|(_, base)| {
            request
                .url
                .strip_prefix(base.as_str())
                .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
        })
        .max_by_key(|(_, base)| base.len());
    if let Some((id, base)) = best {
        let path = &request.url[base.len()..];
        request.url = if path.is_empty() {
            "/".into()
        } else {
            path.to_owned()
        };
        request.server = Some(id.clone());
        request.sync_path_variables();
    }
}

#[derive(Debug, Default)]
struct Builder {
    url: Option<String>,
    method: Option<HttpMethod>,
    headers: Vec<KeyValue>,
    /// Pieces of `-d`/`--data…`, joined with `&` like curl does.
    data: Vec<String>,
    json: bool,
    /// `-F` parts (R-HTTP-15).
    form: Vec<FormPart>,
    get: bool,
    head: bool,
    user: Option<String>,
    timeout_ms: Option<u64>,
    ignored: Vec<String>,
}

/// Options that only change how curl runs or prints; nothing to fill in.
const QUIET: &[&str] = &[
    "compressed",
    "silent",
    "show-error",
    "verbose",
    "include",
    "location",
    "globoff",
    "http1.0",
    "http1.1",
    "http2",
    "http2-prior-knowledge",
    "http3",
    "no-progress-meter",
    "progress-bar",
    "fail",
    "fail-with-body",
    "no-buffer",
    "tr-encoding",
    "path-as-is",
    "ipv4",
    "ipv6",
];

/// Quiet short options (`-s`, `-S`, `-v`, `-i`, `-L`, `-g`, `-f`, `-N`, `-#`, `-4`, `-6`).
const QUIET_SHORT: &str = "sSviLgfN#46";

impl Builder {
    fn long_option(&mut self, name: &str, rest: &mut Vec<String>) -> Result<(), String> {
        let mut value = || rest.pop().ok_or_else(|| format!("--{name} needs a value"));
        match name {
            "request" => self.method(&value()?),
            "header" => self.header(&value()?),
            "data" | "data-ascii" | "data-binary" => {
                let v = value()?;
                self.data_from(&format!("--{name}"), v);
            }
            "data-raw" => self.data.push(value()?),
            "data-urlencode" => {
                let v = value()?;
                self.data_urlencode(&v);
            }
            "json" => {
                let v = value()?;
                self.json = true;
                self.data_from("--json", v);
            }
            "get" => self.get = true,
            "head" => self.head = true,
            "user" => self.user = Some(value()?),
            "user-agent" => self.headers.push(KeyValue::new("User-Agent", value()?)),
            "referer" => self.headers.push(KeyValue::new("Referer", value()?)),
            "cookie" => {
                let v = value()?;
                self.cookie(&v);
            }
            "max-time" => {
                let v = value()?;
                self.max_time(&v);
            }
            "url" => {
                let v = value()?;
                self.url.get_or_insert(v);
            }
            "form" => {
                let v = value()?;
                self.form_part(&v);
            }
            "form-string" => {
                let v = value()?;
                let (part_name, text) = v.split_once('=').unwrap_or((&v, ""));
                self.form.push(FormPart::text(part_name, text));
            }
            "insecure" => self.ignore(
                "--insecure",
                "certificate checks are a setting: Settings → Verify TLS certificates",
            ),
            "output" | "cookie-jar" | "proxy" | "cacert" | "cert" | "key" | "connect-timeout"
            | "retry" | "resolve" | "write-out" | "limit-rate" | "max-redirs" => {
                value()?;
                self.ignore(&format!("--{name}"), "not used by Tortuga");
            }
            _ if QUIET.contains(&name) => {}
            _ => self.ignore(&format!("--{name}"), "not known"),
        }
        Ok(())
    }

    /// `-sSL`, `-XPOST`, `-H 'A: b'`: letters may be combined; the first one that takes a
    /// value takes the rest of the word, or else the next word.
    fn short_options(&mut self, letters: &str, rest: &mut Vec<String>) -> Result<(), String> {
        for (i, c) in letters.char_indices() {
            let long = match c {
                'X' => "request",
                'H' => "header",
                'd' => "data",
                'u' => "user",
                'A' => "user-agent",
                'e' => "referer",
                'b' => "cookie",
                'm' => "max-time",
                'F' => "form",
                'o' => "output",
                'c' => "cookie-jar",
                'x' => "proxy",
                'E' => "cert",
                'w' => "write-out",
                'G' => {
                    self.get = true;
                    continue;
                }
                'I' => {
                    self.head = true;
                    continue;
                }
                'k' => {
                    self.long_option("insecure", rest)?;
                    continue;
                }
                _ if QUIET_SHORT.contains(c) => continue,
                _ => {
                    self.ignore(&format!("-{c}"), "not known");
                    continue;
                }
            };
            let attached = &letters[i + c.len_utf8()..];
            if !attached.is_empty() {
                rest.push(attached.to_owned());
            }
            return self.long_option(long, rest);
        }
        Ok(())
    }

    fn method(&mut self, name: &str) {
        match HttpMethod::ALL
            .into_iter()
            .find(|m| m.as_str().eq_ignore_ascii_case(name))
        {
            Some(m) => self.method = Some(m),
            None => self.ignore(&format!("-X {name}"), "method not supported"),
        }
    }

    fn header(&mut self, line: &str) {
        // `Name;` sends an empty header in curl; `Name:` removes one (nothing to do).
        if let Some(name) = line.strip_suffix(';')
            && !name.contains(':')
        {
            self.headers.push(KeyValue::new(name.trim(), ""));
            return;
        }
        let Some((name, value)) = line.split_once(':') else {
            self.ignore(&format!("-H {line}"), "not a header");
            return;
        };
        let (name, value) = (name.trim(), value.trim());
        // Worked out again when sending.
        if value.is_empty() || name.eq_ignore_ascii_case("content-length") {
            return;
        }
        self.headers.push(KeyValue::new(name, value));
    }

    /// `-d @file` reads a file; Tortuga can't, so it says so.
    fn data_from(&mut self, option: &str, value: String) {
        if let Some(file) = value.strip_prefix('@') {
            self.ignore(
                &format!("{option} @{file}"),
                "data from a file: paste the content into the body",
            );
        } else {
            // curl drops newlines from `-d`/`--data`, but people paste pretty JSON
            // with them and mean it as written.
            self.data.push(value);
        }
    }

    fn data_urlencode(&mut self, value: &str) {
        let encode = |s: &str| {
            query::encode_form(&[KeyValue::new(s, "")])
                .trim_end_matches('=')
                .to_owned()
        };
        // `name=value` encodes only the value; `=value` and `value` encode all of it.
        let piece = match value.split_once('=') {
            Some(("", v)) => encode(v),
            Some((name, v)) => format!("{name}={}", encode(v)),
            None => encode(value),
        };
        self.data.push(piece);
    }

    /// `-F name=value`, `-F name=@file` (or `<file`), each optionally with `;type=…`.
    fn form_part(&mut self, spec: &str) {
        let Some((name, rest)) = spec.split_once('=') else {
            self.ignore(&format!("-F {spec}"), "not name=value");
            return;
        };
        let (file, rest) = match rest.strip_prefix(['@', '<']) {
            Some(rest) => (true, rest),
            None => (false, rest),
        };
        // A quoted file name may contain `;`.
        let (value, options) = rest
            .strip_prefix('"')
            .and_then(|r| r.split_once('"'))
            .unwrap_or_else(|| rest.split_once(';').unwrap_or((rest, "")));
        let content_type = options
            .split(';')
            .find_map(|o| o.trim().strip_prefix("type="))
            .map(str::to_owned);
        let mut part = if file {
            FormPart::file(name, value)
        } else {
            FormPart::text(name, value)
        };
        part.content_type = content_type;
        self.form.push(part);
    }

    fn cookie(&mut self, value: &str) {
        if value.contains('=') {
            self.headers.push(KeyValue::new("Cookie", value));
        } else {
            self.ignore(&format!("-b {value}"), "cookies from a file");
        }
    }

    fn max_time(&mut self, seconds: &str) {
        match seconds.parse::<f64>() {
            Ok(s) if s > 0.0 && s.is_finite() => {
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "a positive, finite number of seconds"
                )]
                let ms = (s * 1000.0).round() as u64;
                self.timeout_ms = Some(ms.max(1));
            }
            _ => self.ignore(&format!("-m {seconds}"), "not a number of seconds"),
        }
    }

    fn ignore(&mut self, what: &str, why: &str) {
        self.ignored.push(format!("{what} ({why})"));
    }

    fn header_value(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|h| h.is_named(name))
            .map(|h| h.value.as_str())
    }

    fn remove_header(&mut self, name: &str) {
        self.headers.retain(|h| !h.is_named(name));
    }

    fn finish(mut self) -> Result<Pasted, String> {
        let Some(url) = self.url.take() else {
            return Err("the curl command has no URL".into());
        };
        let data = (!self.data.is_empty()).then(|| self.data.join("&"));

        let mut request = Request::new(HttpMethod::Get, &url);
        if !self.form.is_empty() {
            // Its Content-Type, with the boundary, is set when sending.
            self.remove_header("content-type");
            request.body = RequestBody::Multipart(std::mem::take(&mut self.form));
        } else if self.get {
            // `-G`: the data goes into the query string.
            if let Some(data) = &data {
                let (_, params) = query::split_url(&format!("?{data}"));
                request.params.extend(params);
            }
        } else if let Some(data) = data {
            request.body = self.body(data);
        }
        if self.json && self.header_value("accept").is_none() {
            self.headers
                .push(KeyValue::new("Accept", "application/json"));
        }
        request.method = self.method.unwrap_or(if self.head {
            HttpMethod::Head
        } else if request.body == RequestBody::None {
            HttpMethod::Get
        } else {
            HttpMethod::Post
        });
        request.auth = self.auth();
        request.headers = self.headers;
        request.timeout_ms = self.timeout_ms;
        request.sync_path_variables();
        Ok(Pasted {
            request,
            ignored: self.ignored,
        })
    }

    /// JSON, XML, form or text, from the Content-Type or the data itself. A Content-Type that
    /// Tortuga would set anyway is left out (R-HTTP-4).
    fn body(&mut self, data: String) -> RequestBody {
        let content_type = self
            .header_value("content-type")
            .map(str::to_ascii_lowercase);
        let looks_json = serde_json::from_str::<serde_json::Value>(&data)
            .is_ok_and(|v| v.is_object() || v.is_array());
        let body = match content_type.as_deref() {
            _ if self.json => RequestBody::Json(data),
            Some(ct) if is_json_content_type(ct) => RequestBody::Json(data),
            Some(ct) if is_xml_content_type(ct) => RequestBody::Xml(data),
            Some(ct) if ct.starts_with("application/x-www-form-urlencoded") => form(&data),
            Some(_) => RequestBody::Text(data),
            None if looks_json => RequestBody::Json(data),
            // curl sends `-d` as a form unless told otherwise.
            None => form(&data),
        };
        if let Some(default) = body.default_content_type()
            && content_type.as_deref() == Some(default)
        {
            self.remove_header("content-type");
        }
        body
    }

    /// `-u user:password`, or an `Authorization: Basic/Bearer` header, as the request's auth.
    fn auth(&mut self) -> Option<Auth> {
        if let Some(user) = self.user.take() {
            let (username, password) = user.split_once(':').unwrap_or((&user, ""));
            return Some(Auth::Basic {
                username: username.to_owned(),
                password: password.to_owned(),
            });
        }
        let value = self.header_value("authorization")?.to_owned();
        let (scheme, credentials) = value.split_once(' ')?;
        let credentials = credentials.trim();
        let auth = if scheme.eq_ignore_ascii_case("bearer") {
            Auth::Bearer {
                token: credentials.to_owned(),
            }
        } else if scheme.eq_ignore_ascii_case("basic") {
            use base64::Engine as _;
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(credentials)
                .ok()
                .and_then(|b| String::from_utf8(b).ok())?;
            let (username, password) = decoded.split_once(':')?;
            Auth::Basic {
                username: username.to_owned(),
                password: password.to_owned(),
            }
        } else {
            return None;
        };
        self.remove_header("authorization");
        Some(auth)
    }
}

/// `a=1&b=2` as form fields; anything else stays text.
fn form(data: &str) -> RequestBody {
    let is_form = !data.is_empty()
        && data.split('&').all(|pair| {
            let name = pair.split_once('=').map_or(pair, |(name, _)| name);
            !name.is_empty() && !name.contains(char::is_whitespace)
        });
    if !is_form {
        return RequestBody::Text(data.to_owned());
    }
    let fields = url::form_urlencoded::parse(data.as_bytes())
        .map(|(k, v)| KeyValue::new(k, v))
        .collect();
    RequestBody::Form(fields)
}

/// Splits a shell command line into words, like bash would.
fn split_words(text: &str) -> Result<Vec<String>, String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            ' ' | '\t' | '\n' | '\r' => {
                if in_word {
                    words.push(std::mem::take(&mut word));
                    in_word = false;
                }
            }
            '\\' => match chars.next() {
                // A line continuation, or a backslash at the very end.
                Some('\n') | None => {}
                Some('\r') => {
                    chars.next_if_eq(&'\n');
                }
                Some(next) => {
                    word.push(next);
                    in_word = true;
                }
            },
            '\'' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(ch) => word.push(ch),
                        None => return Err("a ' quote is not closed".into()),
                    }
                }
            }
            '"' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => match chars.next() {
                            Some(ch @ ('"' | '\\' | '$' | '`')) => word.push(ch),
                            Some('\n') => {}
                            Some(ch) => {
                                word.push('\\');
                                word.push(ch);
                            }
                            None => return Err("a \" quote is not closed".into()),
                        },
                        Some(ch) => word.push(ch),
                        None => return Err("a \" quote is not closed".into()),
                    }
                }
            }
            '$' if chars.peek() == Some(&'\'') => {
                chars.next();
                in_word = true;
                ansi_c_quoted(&mut chars, &mut word)?;
            }
            _ => {
                word.push(c);
                in_word = true;
            }
        }
    }
    if in_word {
        words.push(word);
    }
    Ok(words)
}

/// The rest of a `$'…'` word, with its backslash escapes.
fn ansi_c_quoted(
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
    word: &mut String,
) -> Result<(), String> {
    let unclosed = || "a $' quote is not closed".to_owned();
    loop {
        match chars.next().ok_or_else(unclosed)? {
            '\'' => return Ok(()),
            '\\' => match chars.next().ok_or_else(unclosed)? {
                'n' => word.push('\n'),
                't' => word.push('\t'),
                'r' => word.push('\r'),
                '0' => word.push('\0'),
                c @ ('x' | 'u' | 'U') => {
                    let max = match c {
                        'x' => 2,
                        'u' => 4,
                        _ => 8,
                    };
                    let mut hex = String::new();
                    while hex.len() < max
                        && let Some(d) = chars.next_if(char::is_ascii_hexdigit)
                    {
                        hex.push(d);
                    }
                    let ch = u32::from_str_radix(&hex, 16)
                        .ok()
                        .and_then(char::from_u32)
                        .ok_or_else(|| format!("\\{c}{hex} is not a character"))?;
                    word.push(ch);
                }
                other => word.push(other),
            },
            c => word.push(c),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn parsed(text: &str) -> Pasted {
        parse(text).expect("parses")
    }

    #[test]
    fn recognises_curl_commands() {
        assert!(is_command("curl https://a.test"));
        assert!(is_command("  /usr/bin/curl -X POST x"));
        assert!(is_command("curl.exe x"));
        assert!(!is_command("curl"));
        assert!(!is_command("https://curl.se"));
        assert!(!is_command("curly x"));
    }

    #[test]
    fn get_with_query_and_headers() {
        let p =
            parsed("curl 'https://api.test/users?page=2&q=a%20b' -H 'Accept: application/json'");
        let r = &p.request;
        assert_eq!(r.method, HttpMethod::Get);
        assert_eq!(r.url, "https://api.test/users");
        assert_eq!(
            r.params,
            [KeyValue::new("page", "2"), KeyValue::new("q", "a b")]
        );
        assert_eq!(r.headers, [KeyValue::new("Accept", "application/json")]);
        assert!(p.ignored.is_empty(), "{:?}", p.ignored);
    }

    #[test]
    fn data_makes_a_post_with_a_json_body() {
        let p = parsed(
            "curl -X POST https://api.test/users \\\n  -H 'Content-Type: application/json' \\\n  --data-raw '{\"name\": \"O'\\''Brien\"}'",
        );
        let r = &p.request;
        assert_eq!(r.method, HttpMethod::Post);
        assert_eq!(r.body, RequestBody::Json("{\"name\": \"O'Brien\"}".into()));
        assert!(r.headers.is_empty(), "Content-Type is set automatically");

        let p = parsed("curl https://api.test/u -d '[1, 2]'");
        assert_eq!(p.request.method, HttpMethod::Post);
        assert_eq!(p.request.body, RequestBody::Json("[1, 2]".into()));
    }

    #[test]
    fn form_data_and_urlencode() {
        let p = parsed(
            "curl https://api.test/login -d user=alice -d 'pass=a&b' --data-urlencode 'note=x y&z'",
        );
        assert_eq!(
            p.request.body,
            RequestBody::Form(vec![
                KeyValue::new("user", "alice"),
                KeyValue::new("pass", "a"),
                KeyValue::new("b", ""),
                KeyValue::new("note", "x y&z"),
            ])
        );
        let text = parsed("curl https://api.test/ -H 'Content-Type: text/plain' -d 'hello there'");
        assert_eq!(text.request.body, RequestBody::Text("hello there".into()));
        assert_eq!(
            text.request.headers,
            [KeyValue::new("Content-Type", "text/plain")]
        );
    }

    #[test]
    fn get_flag_moves_data_to_the_query() {
        let p = parsed("curl -G https://api.test/search -d q=rust -d page=2");
        assert_eq!(p.request.method, HttpMethod::Get);
        assert_eq!(p.request.body, RequestBody::None);
        assert_eq!(
            p.request.params,
            [KeyValue::new("q", "rust"), KeyValue::new("page", "2")]
        );
    }

    #[test]
    fn combined_short_options_and_attached_values() {
        let p = parsed("curl -sSL -XPUT -HAccept:text/html -m 2.5 https://api.test/x -k");
        let r = &p.request;
        assert_eq!(r.method, HttpMethod::Put);
        assert_eq!(r.headers, [KeyValue::new("Accept", "text/html")]);
        assert_eq!(r.timeout_ms, Some(2500));
        assert_eq!(p.ignored.len(), 1, "{:?}", p.ignored);
        assert!(p.ignored[0].starts_with("--insecure"));
    }

    #[test]
    fn auth_from_user_and_headers() {
        let basic = parsed("curl -u alice:s3cret https://api.test/");
        assert_eq!(
            basic.request.auth,
            Some(Auth::Basic {
                username: "alice".into(),
                password: "s3cret".into()
            })
        );
        let bearer = parsed("curl https://api.test/ -H 'Authorization: Bearer abc.def'");
        assert_eq!(
            bearer.request.auth,
            Some(Auth::Bearer {
                token: "abc.def".into()
            })
        );
        assert!(
            bearer.request.headers.is_empty(),
            "{:?}",
            bearer.request.headers
        );
        // `alice:pw` in base64
        let header = parsed("curl https://api.test/ -H 'authorization: Basic YWxpY2U6cHc='");
        assert_eq!(
            header.request.auth,
            Some(Auth::Basic {
                username: "alice".into(),
                password: "pw".into()
            })
        );
        let other = parsed("curl https://api.test/ -H 'Authorization: Digest x'");
        assert_eq!(other.request.auth, None);
        assert_eq!(other.request.headers.len(), 1);
    }

    /// What Chrome's "Copy as cURL (bash)" produces.
    #[test]
    fn browser_commands() {
        let text = r#"curl 'https://api.test/graphql' \
  -H 'accept: */*' \
  -H 'content-type: application/json' \
  -b 'session=abc; theme=dark' \
  -H $'x-note: it\'s \u00e9' \
  --data-raw $'{"query":"{ me { id } }","note":"line\\nnext"}' \
  --compressed"#;
        let p = parsed(text);
        let r = &p.request;
        assert_eq!(r.method, HttpMethod::Post);
        assert_eq!(
            r.headers,
            [
                KeyValue::new("accept", "*/*"),
                KeyValue::new("Cookie", "session=abc; theme=dark"),
                KeyValue::new("x-note", "it's é"),
            ]
        );
        assert_eq!(
            r.body,
            RequestBody::Json(r#"{"query":"{ me { id } }","note":"line\nnext"}"#.into())
        );
        assert!(p.ignored.is_empty(), "{:?}", p.ignored);
    }

    #[test]
    fn json_option_and_head() {
        let p = parsed(r#"curl --json '{"a":1}' https://api.test/"#);
        assert_eq!(p.request.method, HttpMethod::Post);
        assert_eq!(p.request.body, RequestBody::Json(r#"{"a":1}"#.into()));
        assert_eq!(
            p.request.headers,
            [KeyValue::new("Accept", "application/json")]
        );
        assert_eq!(
            parsed("curl -I https://api.test/").request.method,
            HttpMethod::Head
        );
    }

    #[test]
    fn unsupported_options_are_listed() {
        let p = parsed("curl -d @body.json -o out.txt --frobnicate https://api.test/");
        let names: Vec<&str> = p
            .ignored
            .iter()
            .map(|i| i.split(" (").next().unwrap_or_default())
            .collect();
        assert_eq!(names, ["--data @body.json", "--output", "--frobnicate"]);
    }

    /// R-HTTP-15: -F and --form-string make a multipart body.
    #[test]
    fn form_options_make_a_multipart_body() {
        let p = parsed(
            r#"curl https://api.test/upload -H 'Content-Type: multipart/form-data' -F 'avatar=@"/tmp/me; 1.png";type=image/png' -F title=Me --form-string 'raw=@kept'"#,
        );
        let mut avatar = FormPart::file("avatar", "/tmp/me; 1.png");
        avatar.content_type = Some("image/png".into());
        assert_eq!(p.request.method, HttpMethod::Post);
        assert_eq!(
            p.request.body,
            RequestBody::Multipart(vec![
                avatar,
                FormPart::text("title", "Me"),
                FormPart::text("raw", "@kept"),
            ])
        );
        assert!(p.request.headers.is_empty(), "{:?}", p.request.headers);
        assert!(p.ignored.is_empty(), "{:?}", p.ignored);
    }

    #[test]
    fn errors() {
        assert!(parse("curl -H 'Accept: x'").is_err(), "no URL");
        assert!(parse("curl 'https://a.test").is_err(), "unclosed quote");
        assert!(parse("wget https://a.test").is_err());
        assert!(parse("curl -X").is_err(), "missing value");
    }

    #[test]
    fn path_variables_and_double_quotes() {
        let p = parsed("curl \"https://api.test/users/:id?x=\\\"q\\\"\"");
        assert_eq!(p.request.url, "https://api.test/users/:id");
        assert_eq!(p.request.params, [KeyValue::new("x", "\"q\"")]);
        assert_eq!(p.request.path_vars.len(), 1);
    }

    /// R-HTTP-11: a known server's base URL becomes the server + path.
    #[test]
    fn known_server_base_urls_are_used() {
        let ctx = vars::Context {
            server_names: BTreeMap::from([
                ("api".into(), "API".into()),
                ("v2".into(), "V2".into()),
            ]),
            server_urls: BTreeMap::from([
                ("api".into(), "https://{{host}}/".into()),
                ("v2".into(), "https://api.test/v2".into()),
            ]),
            variables: BTreeMap::from([("host".into(), "api.test".into())]),
            ..vars::Context::default()
        };
        let mut r = parsed("curl https://api.test/users/:id?a=1").request;
        use_server(&mut r, &ctx);
        assert_eq!(r.server.as_deref(), Some("api"));
        assert_eq!(r.url, "/users/:id");
        assert_eq!(r.params, [KeyValue::new("a", "1")]);
        assert_eq!(r.path_vars.len(), 1);

        let mut r = parsed("curl https://api.test/v2/items").request;
        use_server(&mut r, &ctx);
        assert_eq!(
            (r.server.as_deref(), r.url.as_str()),
            (Some("v2"), "/items")
        );

        let mut r = parsed("curl https://api.test.evil/x").request;
        use_server(&mut r, &ctx);
        assert_eq!(r.server, None, "only whole host names match");
    }
}
