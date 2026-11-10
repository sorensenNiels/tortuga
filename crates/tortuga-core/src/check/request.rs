//! Credentials in one request's fields (R-CLI-6), and turning one into a secret variable
//! (R-VAR-10).

use super::credential::{AuthField, Found, find, find_in_auth};
use crate::model::{Auth, Request, RequestBody};

/// A field of a request that a value is flagged in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Url,
    Param(usize),
    PathVariable(usize),
    Header(usize),
    /// A text, JSON or XML body.
    Body,
    FormField(usize),
    /// A text part of a multipart body.
    Part(usize),
    AuthUsername,
    AuthPassword,
    AuthToken,
    /// The value of header auth.
    AuthValue,
}

/// A flagged value in a request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Flag {
    pub field: Field,
    /// As `check` names it: `header Authorization`, `auth password`.
    pub place: String,
    pub found: Found,
}

/// Every flagged value of `request`, in the order of its fields. Examples are not
/// scanned (D76).
pub fn flags(request: &Request) -> Vec<Flag> {
    let mut out = Vec::new();
    let mut add = |field: Field, place: String, found: Option<Found>| {
        if let Some(found) = found {
            out.push(Flag {
                field,
                place,
                found,
            });
        }
    };
    add(Field::Url, "url".into(), find(&request.url));
    for (i, p) in request.params.iter().enumerate() {
        add(Field::Param(i), format!("param {}", p.name), find(&p.value));
    }
    for (i, v) in request.path_vars.iter().enumerate() {
        add(
            Field::PathVariable(i),
            format!("path variable {}", v.name),
            find(&v.value),
        );
    }
    for (i, h) in request.headers.iter().enumerate() {
        add(
            Field::Header(i),
            format!("header {}", h.name),
            find(&h.value),
        );
    }
    match &request.body {
        RequestBody::None => {}
        RequestBody::Text(t) | RequestBody::Json(t) | RequestBody::Xml(t) => {
            add(Field::Body, "body".into(), find(t));
        }
        RequestBody::Form(fields) => {
            for (i, f) in fields.iter().enumerate() {
                add(
                    Field::FormField(i),
                    format!("form field {}", f.name),
                    find(&f.value),
                );
            }
        }
        RequestBody::Multipart(parts) => {
            for (i, p) in parts.iter().enumerate().filter(|(_, p)| !p.file) {
                add(Field::Part(i), format!("part {}", p.name), find(&p.value));
            }
        }
    }
    for (field, place, kind, value) in auth_fields(request.auth.as_ref()) {
        add(field, place.into(), find_in_auth(kind, value));
    }
    out
}

/// The auth fields that can hold a credential: (field, place, kind, value).
pub(super) fn auth_fields(auth: Option<&Auth>) -> Vec<(Field, &'static str, AuthField, &str)> {
    match auth {
        None | Some(Auth::None) => Vec::new(),
        Some(Auth::Basic { username, password }) => vec![
            (
                Field::AuthUsername,
                "auth username",
                AuthField::Other,
                username,
            ),
            (
                Field::AuthPassword,
                "auth password",
                AuthField::BasicPassword,
                password,
            ),
        ],
        Some(Auth::Bearer { token }) => vec![(
            Field::AuthToken,
            "auth token",
            AuthField::BearerToken,
            token,
        )],
        Some(Auth::Header { value, .. }) => {
            vec![(Field::AuthValue, "auth value", AuthField::Other, value)]
        }
    }
}

/// The name a new secret for `flag` gets before it is made unique: the field's own name
/// (a header's, a param's, `password`), else what the value looks like (`jwt`).
pub fn secret_name(request: &Request, flag: &Flag) -> String {
    let named = |name: &str| (!name.trim().is_empty()).then(|| name.to_owned());
    let own = match flag.field {
        Field::Param(i) => request.params.get(i).and_then(|p| named(&p.name)),
        Field::PathVariable(i) => request.path_vars.get(i).and_then(|v| named(&v.name)),
        Field::Header(i) => request.headers.get(i).and_then(|h| named(&h.name)),
        Field::FormField(i) => match &request.body {
            RequestBody::Form(fields) => fields.get(i).and_then(|f| named(&f.name)),
            _ => None,
        },
        Field::Part(i) => match &request.body {
            RequestBody::Multipart(parts) => parts.get(i).and_then(|p| named(&p.name)),
            _ => None,
        },
        Field::AuthUsername => Some("username".to_owned()),
        Field::AuthPassword => Some("password".to_owned()),
        Field::AuthToken => Some("token".to_owned()),
        Field::AuthValue => match &request.auth {
            Some(Auth::Header { name, .. }) => named(name),
            _ => None,
        },
        Field::Url | Field::Body => None,
    };
    crate::import::variable_name(&own.unwrap_or_else(|| flag.found.kind.short_name().to_owned()))
}

/// `base`, or `base_2`, `base_3`, … when it is `taken`, as the imports name secrets.
pub fn unique_name(base: &str, taken: impl Fn(&str) -> bool) -> String {
    (1..=usize::MAX)
        .map(|n| {
            if n == 1 {
                base.to_owned()
            } else {
                format!("{base}_{n}")
            }
        })
        .find(|name| !taken(name))
        .unwrap_or_else(|| base.to_owned())
}

/// Puts `{{name}}` where `flag`'s credential is in `request` and returns the credential,
/// or `None` when the field no longer holds it (the request changed since).
pub fn make_secret(request: &mut Request, flag: &Flag, name: &str) -> Option<String> {
    let value = value_mut(request, flag.field)?;
    let secret = value.get(flag.found.range.clone())?.to_owned();
    let still = match flag.field {
        Field::AuthUsername | Field::AuthPassword | Field::AuthToken | Field::AuthValue => {
            flags_in_auth(value, flag)
        }
        _ => find(value).as_ref() == Some(&flag.found),
    };
    if !still {
        return None;
    }
    value.replace_range(flag.found.range.clone(), &format!("{{{{{name}}}}}"));
    Some(secret)
}

/// Whether the auth field `value` still has `flag`'s credential where it was.
fn flags_in_auth(value: &str, flag: &Flag) -> bool {
    let kind = match flag.field {
        Field::AuthPassword => AuthField::BasicPassword,
        Field::AuthToken => AuthField::BearerToken,
        _ => AuthField::Other,
    };
    find_in_auth(kind, value).as_ref() == Some(&flag.found)
}

fn value_mut(request: &mut Request, field: Field) -> Option<&mut String> {
    match (field, &mut request.body, &mut request.auth) {
        (Field::Url, _, _) => Some(&mut request.url),
        (Field::Param(i), _, _) => request.params.get_mut(i).map(|p| &mut p.value),
        (Field::PathVariable(i), _, _) => request.path_vars.get_mut(i).map(|v| &mut v.value),
        (Field::Header(i), _, _) => request.headers.get_mut(i).map(|h| &mut h.value),
        (Field::Body, RequestBody::Text(t) | RequestBody::Json(t) | RequestBody::Xml(t), _) => {
            Some(t)
        }
        (Field::FormField(i), RequestBody::Form(fields), _) => {
            fields.get_mut(i).map(|f| &mut f.value)
        }
        (Field::Part(i), RequestBody::Multipart(parts), _) => {
            parts.get_mut(i).filter(|p| !p.file).map(|p| &mut p.value)
        }
        (Field::AuthUsername, _, Some(Auth::Basic { username, .. })) => Some(username),
        (Field::AuthPassword, _, Some(Auth::Basic { password, .. })) => Some(password),
        (Field::AuthToken, _, Some(Auth::Bearer { token })) => Some(token),
        (Field::AuthValue, _, Some(Auth::Header { value, .. })) => Some(value),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{HttpMethod, KeyValue};

    const GITHUB: &str = concat!("ghp_", "abcdefghijklmnopqrstuvwxyz0123");

    fn request() -> Request {
        let mut r = Request::new(HttpMethod::Post, format!("https://x.io/ü😀/{GITHUB}"));
        r.headers
            .push(KeyValue::new("Authorization", "Bearer abc123def456ghi789"));
        r.headers.push(KeyValue::new("X-Plain", "hello"));
        r.body = RequestBody::Json(format!(r#"{{"t": "{GITHUB}"}}"#));
        r.auth = Some(Auth::Basic {
            username: "ann".into(),
            password: "hunter2".into(),
        });
        r
    }

    #[test]
    fn every_flagged_field_is_listed_with_its_place() {
        // R-CLI-6, R-VAR-10
        let places: Vec<String> = flags(&request()).into_iter().map(|f| f.place).collect();
        assert_eq!(
            places,
            ["url", "header Authorization", "body", "auth password"]
        );
        assert_eq!(flags(&Request::new(HttpMethod::Get, "")), Vec::new());
    }

    #[test]
    fn a_credential_becomes_a_variable_in_its_field() {
        // R-VAR-10: only the token is replaced; the rest of the field stays.
        let mut r = request();
        let all = flags(&r);
        let names: Vec<String> = all.iter().map(|f| secret_name(&r, f)).collect();
        assert_eq!(
            names,
            ["github_token", "authorization", "github_token", "password"]
        );
        for (flag, name) in all.iter().zip(["gh", "authorization", "gh2", "password"]) {
            assert!(make_secret(&mut r, flag, name).is_some(), "{}", flag.place);
        }
        assert_eq!(r.url, "https://x.io/ü😀/{{gh}}");
        assert_eq!(r.headers[0].value, "Bearer {{authorization}}");
        assert_eq!(r.body, RequestBody::Json(r#"{"t": "{{gh2}}"}"#.into()));
        assert_eq!(
            r.auth,
            Some(Auth::Basic {
                username: "ann".into(),
                password: "{{password}}".into()
            })
        );
        assert_eq!(flags(&r), Vec::new());
    }

    #[test]
    fn a_stale_flag_changes_nothing() {
        // R-VAR-10: the field was edited after the warning was shown.
        let mut r = request();
        let flag = flags(&r).remove(1);
        r.headers[0].value = "Bearer {{t}}".into();
        assert_eq!(make_secret(&mut r, &flag, "x"), None);
        assert_eq!(r.headers[0].value, "Bearer {{t}}");
        r.headers.clear();
        assert_eq!(make_secret(&mut r, &flag, "x"), None);
    }

    #[test]
    fn names_are_made_unique_with_a_number() {
        // R-VAR-10
        assert_eq!(unique_name("token", |_| false), "token");
        assert_eq!(
            unique_name("token", |n| n == "token" || n == "token_2"),
            "token_3"
        );
    }

    #[test]
    fn the_secret_value_is_returned() {
        // R-VAR-10
        let mut r = request();
        let flag = flags(&r).remove(0);
        assert_eq!(make_secret(&mut r, &flag, "gh").as_deref(), Some(GITHUB));
    }
}
