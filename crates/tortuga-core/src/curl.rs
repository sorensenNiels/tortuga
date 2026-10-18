//! Copy a request as a `curl` command (R-HTTP-9).
//!
//! The command is written from the request as it would be sent: variables, path
//! variables, server base URL, project headers and auth all resolved (see
//! [`vars::prepare`]). Secret values are only included when asked for; otherwise
//! each one is replaced by `<secret:name>` (P6).
//!
//! The output uses POSIX shell quoting (bash, zsh, Git Bash, WSL).

pub mod import;

use crate::model::{HttpMethod, Request, RequestBody};
use crate::{ClientSettings, Result, query, vars};

/// A request as `curl` commands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurlCommand {
    /// With every value, secrets included.
    pub full: String,
    /// With secret values replaced by `<secret:name>`; `None` if the request uses no secrets.
    pub masked: Option<String>,
    /// Names of the secret variables the request uses.
    pub secrets: Vec<String>,
}

/// The `curl` command for `request` in `ctx`, sent with `settings`.
pub fn command(
    request: &Request,
    ctx: &vars::Context,
    settings: &ClientSettings,
) -> Result<CurlCommand> {
    let full = write(&vars::prepare(request, ctx)?, settings);

    // The same request with every secret value replaced by a placeholder.
    let mut secrets: Vec<String> = ctx
        .secrets
        .iter()
        .filter(|name| ctx.variables.get(*name).is_some_and(|v| !v.is_empty()))
        .cloned()
        .collect();
    let masked = write(&vars::prepare(request, &ctx.masked())?, settings);
    if masked == full {
        return Ok(CurlCommand {
            full,
            masked: None,
            secrets: Vec::new(),
        });
    }
    // Only the secrets that actually appear in the request.
    secrets.retain(|name| masked.contains(&format!("<secret:{name}>")));
    Ok(CurlCommand {
        full,
        masked: Some(masked),
        secrets,
    })
}

/// Writes a prepared request (see [`vars::prepare`]) as a `curl` command.
pub fn write(request: &Request, settings: &ClientSettings) -> String {
    let mut parts: Vec<String> = Vec::new();
    let url = query::join_url(&request.url, &request.params);
    let has_body = !matches!(request.body, RequestBody::None);
    match request.method {
        HttpMethod::Get if !has_body => parts.push(format!("curl {}", quote(&url))),
        // `-X HEAD` would wait for a body that never comes.
        HttpMethod::Head => parts.push(format!("curl --head {}", quote(&url))),
        method => parts.push(format!("curl -X {} {}", method.as_str(), quote(&url))),
    }
    if settings.follow_redirects {
        parts.push("-L".into());
    }
    if !settings.verify_tls {
        parts.push("-k".into());
    }
    // curl sets a multipart Content-Type itself, with the boundary (like sending does).
    let multipart = matches!(request.body, RequestBody::Multipart(_));
    for header in request.headers.iter().filter(|h| h.is_active()) {
        if multipart && header.name.trim().eq_ignore_ascii_case("content-type") {
            continue;
        }
        if header.name.eq_ignore_ascii_case("authorization")
            && let Some(user) = basic_credentials(&header.value)
        {
            // Readable (and maskable) instead of base64.
            parts.push(format!("-u {}", quote(&user)));
        } else {
            parts.push(format!(
                "-H {}",
                quote(&format!("{}: {}", header.name, header.value))
            ));
        }
    }
    if !multipart
        && !request.has_header("content-type")
        && let Some(content_type) = request.body.default_content_type()
    {
        parts.push(format!(
            "-H {}",
            quote(&format!("Content-Type: {content_type}"))
        ));
    }
    match &request.body {
        RequestBody::None => {}
        RequestBody::Text(text) | RequestBody::Json(text) => {
            parts.push(format!("--data-raw {}", quote(text)));
        }
        RequestBody::Form(fields) => {
            parts.push(format!("--data-raw {}", quote(&query::encode_form(fields))));
        }
        RequestBody::Multipart(form) => {
            for p in form.iter().filter(|p| p.is_active()) {
                parts.push(form_option(p));
            }
        }
    }
    if let Some(timeout) = request.timeout() {
        parts.push(format!("--max-time {}", timeout.as_secs_f64()));
    }
    parts.join(" \\\n  ")
}

/// One multipart part as a curl option (R-HTTP-15): files as `-F name=@path`, text as
/// `--form-string` (which never reads files), or `-F` when it has a content type.
fn form_option(part: &crate::model::FormPart) -> String {
    let name = part.name.trim();
    let content_type = part
        .content_type
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty());
    let typed = |value: String| match content_type {
        Some(ct) => format!("{value};type={ct}"),
        None => value,
    };
    if part.file {
        // Paths with `;`, `,` or quotes need curl's own double quotes.
        let path = if part.value.contains([';', ',', '"']) {
            format!(
                "\"{}\"",
                part.value.replace('\\', "\\\\").replace('"', "\\\"")
            )
        } else {
            part.value.clone()
        };
        format!("-F {}", quote(&typed(format!("{name}=@{path}"))))
    } else if content_type.is_some() {
        format!("-F {}", quote(&typed(format!("{name}={}", part.value))))
    } else {
        format!("--form-string {}", quote(&format!("{name}={}", part.value)))
    }
}

/// `user:password` from a `Basic …` header value, if it decodes cleanly.
pub fn basic_credentials(value: &str) -> Option<String> {
    use base64::Engine as _;

    let encoded = value
        .strip_prefix("Basic ")
        .or_else(|| value.strip_prefix("basic "))?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded.trim())
        .ok()?;
    String::from_utf8(bytes).ok().filter(|s| s.contains(':'))
}

/// Single-quotes `text` for a POSIX shell.
fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::model::{Auth, KeyValue};

    fn settings() -> ClientSettings {
        ClientSettings {
            follow_redirects: false,
            ..ClientSettings::default()
        }
    }

    #[test]
    fn get_requests_are_short() {
        let r = Request::new(HttpMethod::Get, "https://api.test/users?page=2");
        assert_eq!(
            write(&r, &settings()),
            "curl 'https://api.test/users?page=2'"
        );
    }

    #[test]
    fn method_headers_body_and_settings() {
        let mut r = Request::new(HttpMethod::Post, "https://api.test/users");
        r.headers = vec![
            KeyValue::new("Accept", "application/json"),
            KeyValue {
                enabled: false,
                ..KeyValue::new("X-Off", "1")
            },
        ];
        r.body = RequestBody::Json("{\"name\": \"O'Brien\"}".into());
        r.timeout_ms = Some(2500);
        let s = ClientSettings {
            follow_redirects: true,
            verify_tls: false,
            ..ClientSettings::default()
        };
        assert_eq!(
            write(&r, &s),
            "curl -X POST 'https://api.test/users' \\\n  -L \\\n  -k \\\n  \
             -H 'Accept: application/json' \\\n  -H 'Content-Type: application/json' \\\n  \
             --data-raw '{\"name\": \"O'\\''Brien\"}' \\\n  --max-time 2.5"
        );
    }

    #[test]
    fn forms_and_head() {
        let mut r = Request::new(HttpMethod::Put, "https://api.test/f");
        r.body = RequestBody::Form(vec![KeyValue::new("a b", "c&d")]);
        let text = write(&r, &settings());
        assert!(
            text.contains("-H 'Content-Type: application/x-www-form-urlencoded'"),
            "{text}"
        );
        assert!(text.contains("--data-raw 'a+b=c%26d'"), "{text}");
        let head = Request::new(HttpMethod::Head, "https://api.test/");
        assert_eq!(write(&head, &settings()), "curl --head 'https://api.test/'");
    }

    /// R-HTTP-9: variables resolved; secret values masked unless asked for.
    #[test]
    fn secrets_are_masked_in_the_masked_command() {
        let mut ctx = vars::Context {
            variables: BTreeMap::from([("host".into(), "api.test".into())]),
            secrets: ["password".into(), "token".into()].into(),
            ..vars::Context::default()
        }
        .with_secrets(BTreeMap::from([
            ("password".into(), "s3cret".into()),
            ("token".into(), "unused-here".into()),
        ]));
        let mut r = Request::new(HttpMethod::Get, "https://{{host}}/me");
        r.auth = Some(Auth::Basic {
            username: "alice".into(),
            password: "{{password}}".into(),
        });
        let curl = command(&r, &ctx, &settings()).expect("curl");
        assert_eq!(
            curl.full,
            "curl 'https://api.test/me' \\\n  -u 'alice:s3cret'"
        );
        assert_eq!(
            curl.masked.as_deref(),
            Some("curl 'https://api.test/me' \\\n  -u 'alice:<secret:password>'")
        );
        assert_eq!(curl.secrets, ["password"], "only secrets the request uses");

        ctx.secrets.clear();
        r.auth = Some(Auth::None);
        let plain = command(&r, &ctx, &settings()).expect("curl");
        assert_eq!(plain.masked, None);
    }

    /// R-HTTP-15: text parts as --form-string, files as -F name=@path; no Content-Type header.
    #[test]
    fn multipart_parts() {
        use crate::model::FormPart;

        let mut r = Request::new(HttpMethod::Post, "https://api.test/upload");
        r.headers = vec![KeyValue::new("Content-Type", "multipart/form-data")];
        let mut avatar = FormPart::file("avatar", "/tmp/me; 1.png");
        avatar.content_type = Some("image/png".into());
        let mut off = FormPart::text("off", "x");
        off.enabled = false;
        r.body = RequestBody::Multipart(vec![FormPart::text("note", "@not a file"), avatar, off]);
        assert_eq!(
            write(&r, &settings()),
            "curl -X POST 'https://api.test/upload' \\\n  --form-string 'note=@not a file' \\\n  \
             -F 'avatar=@\"/tmp/me; 1.png\";type=image/png'"
        );
    }
}
