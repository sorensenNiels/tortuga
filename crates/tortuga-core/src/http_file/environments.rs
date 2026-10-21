//! Reading the environment files of the HTTP clients (R-COL-29), for
//! [`crate::import::import_environments`]:
//! - the JetBrains HTTP Client's `http-client.env.json`, and its private twin
//!   `http-client.private.env.json`, whose values are all secret (D35);
//! - the VS Code REST Client's `rest-client.environmentVariables` setting, in a
//!   `settings.json` or a `.code-workspace` file (comments allowed, as VS Code allows).
//!
//! Both map environment names to their variables. Values in `$shared` go into every
//! environment, below the environment's own; REST Client's `{{$shared name}}` is
//! filled in. Settings that are not variables (JetBrains' SSL and auth objects) and
//! values read from the machine (`{{$processEnv …}}`, `{{$dotenv …}}`) are left out
//! with a warning.

use serde_json::{Map, Value};

use crate::import::{EnvValue, ImportedEnvironment};
use crate::{Error, Result, postman};

/// REST Client's setting in VS Code's `settings.json`.
const REST_CLIENT: &str = "rest-client.environmentVariables";
/// The environment whose values every other one gets.
const SHARED: &str = "$shared";

/// Whether `file_name` is a JetBrains HTTP Client environment file
/// (`http-client.env.json`, `http-client.private.env.json`).
pub fn is_environment_file_name(file_name: &str) -> bool {
    let name = file_name.to_ascii_lowercase();
    name.starts_with("http-client") && name.ends_with(".env.json")
}

/// Whether the file `file_name` with JSON `doc` is an environment file of the HTTP
/// clients.
pub(crate) fn looks_like_environments(file_name: &str, doc: &Value) -> bool {
    is_environment_file_name(file_name) || rest_client(doc).is_some()
}

/// The REST Client setting in a `settings.json` or in a `.code-workspace` file's
/// `settings`.
fn rest_client(doc: &Value) -> Option<&Value> {
    doc.get(REST_CLIENT)
        .or_else(|| doc.get("settings")?.get(REST_CLIENT))
}

/// Reads an environment file of the HTTP clients named `file_name` (R-COL-29): an
/// environment per name in it, in file order.
pub fn parse_environments(text: &str, file_name: &str) -> Result<Vec<ImportedEnvironment>> {
    let text = without_comments(text.trim_start_matches('\u{feff}'));
    let doc: Value =
        serde_json::from_str(&text).map_err(|e| invalid(format!("it is not valid JSON: {e}")))?;
    if postman::looks_like_environment(&doc) {
        return Err(invalid(
            "this is a Postman environment: import it with “Import Postman environments…” \
             in a project's menu",
        ));
    }
    let (environments, private) = match rest_client(&doc) {
        Some(setting) => (setting, false),
        None => (&doc, file_name.to_ascii_lowercase().contains(".private.")),
    };
    let Some(environments) = environments
        .as_object()
        .filter(|envs| envs.values().all(Value::is_object))
    else {
        return Err(invalid(
            "this is not an environment file of the HTTP Client or REST Client: it should \
             map environment names to their variables, as http-client.env.json does",
        ));
    };
    let shared = environments.get(SHARED).and_then(Value::as_object);
    let out: Vec<ImportedEnvironment> = environments
        .iter()
        .filter(|(name, _)| !name.starts_with('$') && !name.trim().is_empty())
        .filter_map(|(name, values)| {
            Some(environment(
                name.trim(),
                values.as_object()?,
                shared,
                private,
            ))
        })
        .collect();
    if out.is_empty() {
        return Err(invalid(format!(
            "it has no environments{}",
            if shared.is_some() {
                ", only $shared values"
            } else {
                ""
            }
        )));
    }
    Ok(out)
}

fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidHttpEnvironment(message.into())
}

/// One environment: the shared values it doesn't set, then its own.
fn environment(
    name: &str,
    own: &Map<String, Value>,
    shared: Option<&Map<String, Value>>,
    secret: bool,
) -> ImportedEnvironment {
    let mut env = ImportedEnvironment {
        name: name.to_owned(),
        values: Vec::new(),
        warnings: Vec::new(),
    };
    let inherited = shared
        .into_iter()
        .flatten()
        .filter(|(key, _)| !own.contains_key(*key));
    for (key, value) in inherited.chain(own) {
        let key = key.trim();
        if key.is_empty() {
            continue;
        }
        let value = match value {
            Value::Null => String::new(),
            Value::String(s) => s.clone(),
            Value::Bool(_) | Value::Number(_) => value.to_string(),
            Value::Array(_) | Value::Object(_) => {
                env.warnings.push(format!(
                    "“{name}”: {key} was left out: it is a setting of the HTTP Client (such as \
                     SSL or auth), not a variable."
                ));
                continue;
            }
        };
        let value = fill_in_shared(&value, shared);
        let value = match unsupported(&value) {
            Some(read) => {
                env.warnings.push(format!(
                    "“{name}”: {key} reads {{{{{read}}}}}, which Tortuga does not fill in, so it \
                     was left empty: set it under Environments…."
                ));
                String::new()
            }
            None => value,
        };
        env.values.push(EnvValue {
            key: key.to_owned(),
            value,
            secret,
        });
    }
    env
}

/// REST Client's `{{$shared name}}` replaced by the shared value.
fn fill_in_shared(value: &str, shared: Option<&Map<String, Value>>) -> String {
    let mut out = value.to_owned();
    for name in crate::vars::names(value) {
        let Some(key) = name.strip_prefix("$shared ") else {
            continue;
        };
        if let Some(Value::String(text)) = shared.and_then(|s| s.get(key.trim())) {
            out = out.replace(&format!("{{{{{name}}}}}"), text);
        }
    }
    out
}

/// A `{{$…}}` in `value` that reads something Tortuga can't (`$processEnv`, `$dotenv`,
/// `$aadToken`, …): not one of its dynamic variables (R-VAR-8).
fn unsupported(value: &str) -> Option<&str> {
    crate::vars::names(value)
        .into_iter()
        .find(|n| n.starts_with('$') && !crate::vars::dynamic::is_dynamic(n))
}

/// JSON with comments (`//`, `/* */`) and trailing commas, as VS Code's settings allow,
/// as plain JSON.
fn without_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut in_string = false;
    while let Some(c) = chars.next() {
        if in_string {
            out.push(c);
            match c {
                '\\' => {
                    if let Some(escaped) = chars.next() {
                        out.push(escaped);
                    }
                }
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match (c, chars.peek()) {
            ('"', _) => {
                in_string = true;
                out.push(c);
            }
            ('/', Some('/')) => {
                for next in chars.by_ref() {
                    if next == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            ('/', Some('*')) => {
                chars.next();
                let mut last = ' ';
                for next in chars.by_ref() {
                    if last == '*' && next == '/' {
                        break;
                    }
                    last = next;
                }
                out.push(' ');
            }
            ('}' | ']', _) => {
                // A trailing comma before it.
                if let Some(i) = out.rfind(|c: char| !c.is_whitespace())
                    && out[i..].starts_with(',')
                {
                    out.remove(i);
                }
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const JETBRAINS: &str = r#"{
      "$shared": { "version": "v1", "limit": 10 },
      "dev": {
        "baseUrl": "http://localhost:3000",
        "user": "ann-ü😀",
        "limit": 5,
        "SSLConfiguration": { "clientCertificate": "cert.pem" }
      },
      "prod": { "baseUrl": "https://shop.test", "debug": false, "empty": null }
    }"#;

    fn values(env: &ImportedEnvironment) -> Vec<(&str, &str)> {
        env.values
            .iter()
            .map(|v| (v.key.as_str(), v.value.as_str()))
            .collect()
    }

    /// R-COL-29: an environment per name in file order, `$shared` values below each
    /// environment's own, settings objects left out with a warning.
    #[test]
    fn reads_a_jetbrains_environment_file() {
        let envs = parse_environments(JETBRAINS, "http-client.env.json").expect("parse");
        let names: Vec<_> = envs.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["dev", "prod"]);
        assert_eq!(
            values(&envs[0]),
            [
                ("version", "v1"),
                ("baseUrl", "http://localhost:3000"),
                ("user", "ann-ü😀"),
                ("limit", "5"),
            ]
        );
        assert_eq!(
            values(&envs[1]),
            [
                ("version", "v1"),
                ("limit", "10"),
                ("baseUrl", "https://shop.test"),
                ("debug", "false"),
                ("empty", ""),
            ]
        );
        assert!(envs.iter().flat_map(|e| &e.values).all(|v| !v.secret));
        assert_eq!(envs[0].warnings.len(), 1);
        assert!(envs[0].warnings[0].contains("SSLConfiguration was left out"));
        assert_eq!(envs[1].warnings, Vec::<String>::new());
    }

    /// R-COL-29, D35: every value of the private file is secret, and Debug hides them.
    #[test]
    fn values_of_the_private_file_are_secret() {
        let text = r#"{"dev": {"token": "s3cret"}, "prod": {"token": "p-1"}}"#;
        let envs = parse_environments(text, "http-client.private.env.json").expect("parse");
        assert!(envs.iter().flat_map(|e| &e.values).all(|v| v.secret));
        assert!(!format!("{envs:?}").contains("s3cret"));
        let public = parse_environments(text, "http-client.env.json").expect("parse");
        assert!(public.iter().flat_map(|e| &e.values).all(|v| !v.secret));
    }

    /// R-COL-29: REST Client's setting in `settings.json` (with comments and trailing
    /// commas) or a `.code-workspace` file; `{{$shared …}}` is filled in, values read
    /// from the machine are left empty with a warning.
    #[test]
    fn reads_rest_client_settings() {
        let settings = r#"{
          // Editor settings first.
          "editor.fontSize": 14, /* and a block comment, with "quotes" */
          "rest-client.environmentVariables": {
            "$shared": { "version": "v2", "prefix": "api" },
            "local": {
              "host": "localhost:8080",
              "path": "/{{$shared prefix}}/{{$shared version}}",
              "url": "http://example.test//not-a-comment",
              "home": "{{$processEnv HOME}}",
              "id": "{{$guid}}",
            },
            "production": {},
          },
        }"#;
        let envs = parse_environments(settings, "settings.json").expect("parse");
        let names: Vec<_> = envs.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["local", "production"]);
        assert_eq!(
            values(&envs[0]),
            [
                ("version", "v2"),
                ("prefix", "api"),
                ("host", "localhost:8080"),
                ("path", "/api/v2"),
                ("url", "http://example.test//not-a-comment"),
                ("home", ""),
                ("id", "{{$guid}}"),
            ]
        );
        assert_eq!(values(&envs[1]), [("version", "v2"), ("prefix", "api")]);
        assert_eq!(
            envs[0].warnings,
            [
                "“local”: home reads {{$processEnv HOME}}, which Tortuga does not fill in, so it \
              was left empty: set it under Environments…."
            ]
        );

        let workspace = r#"{"folders": [], "settings": {
            "rest-client.environmentVariables": {"dev": {"a": "1"}}}}"#;
        let envs = parse_environments(workspace, "shop.code-workspace").expect("workspace");
        assert_eq!(values(&envs[0]), [("a", "1")]);
        let doc: Value = serde_json::from_str(workspace).expect("json");
        assert!(looks_like_environments("shop.code-workspace", &doc));
    }

    #[test]
    fn refuses_what_is_not_an_environment_file() {
        let message = |text: &str| {
            crate::error::message(
                &parse_environments(text, "http-client.env.json").expect_err(text),
            )
        };
        assert!(message("{").contains("not valid JSON"));
        assert!(message(r#"{"dev": "x"}"#).contains("map environment names"));
        assert!(message("[]").contains("map environment names"));
        assert!(message("{}").ends_with("it has no environments"));
        assert!(message(r#"{"$shared": {"a": 1}}"#).contains("only $shared values"));
        let postman =
            r#"{"name": "Staging", "values": [], "_postman_variable_scope": "environment"}"#;
        assert!(message(postman).contains("Import Postman environments"));
    }

    #[test]
    fn environment_file_names() {
        assert!(is_environment_file_name("http-client.env.json"));
        assert!(is_environment_file_name("HTTP-Client.private.env.json"));
        assert!(!is_environment_file_name("client.env.json"));
        assert!(!is_environment_file_name("http-client.json"));
    }

    #[test]
    fn comments_and_trailing_commas_are_removed() {
        assert_eq!(without_comments("[1, 2, ]"), "[1, 2 ]");
        assert_eq!(
            without_comments("{\"a\": \"/* x */\"} // end"),
            "{\"a\": \"/* x */\"} "
        );
        assert_eq!(without_comments(r#"{"a": "\"//",}"#), r#"{"a": "\"//"}"#);
        assert_eq!(without_comments("/* unclosed"), " ");
        assert_eq!(without_comments("\"ü😀\" /* ü */"), "\"ü😀\"  ");
    }
}
