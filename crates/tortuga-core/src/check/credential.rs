//! Values that look like credentials (R-CLI-6, D76): hand-written prefix and shape
//! checks, no regex. A `{{variable}}` is never one, since the scan needs the token's own
//! characters right after its prefix.

use base64::Engine as _;

/// What a flagged value looks like. Named in findings instead of the value (P6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Credential {
    Jwt,
    BearerToken,
    BasicCredentials,
    AwsAccessKey,
    GitHubToken,
    StripeKey,
    SlackToken,
    PrivateKey,
    /// A Basic auth password typed in rather than a `{{variable}}`.
    TypedPassword,
    /// A Bearer auth token typed in rather than a `{{variable}}`.
    TypedToken,
}

impl Credential {
    /// A variable name for a secret of this kind found where nothing names it.
    pub fn short_name(self) -> &'static str {
        match self {
            Self::Jwt => "jwt",
            Self::BearerToken | Self::TypedToken => "token",
            Self::BasicCredentials => "basic_credentials",
            Self::AwsAccessKey => "aws_access_key_id",
            Self::GitHubToken => "github_token",
            Self::StripeKey => "stripe_key",
            Self::SlackToken => "slack_token",
            Self::PrivateKey => "private_key",
            Self::TypedPassword => "password",
        }
    }

    /// The finding's message, e.g. "looks like a GitHub token".
    pub fn describe(self) -> &'static str {
        match self {
            Self::Jwt => "looks like a JWT",
            Self::BearerToken => "looks like a Bearer token",
            Self::BasicCredentials => "looks like Basic credentials",
            Self::AwsAccessKey => "looks like an AWS access key",
            Self::GitHubToken => "looks like a GitHub token",
            Self::StripeKey => "looks like a Stripe live key",
            Self::SlackToken => "looks like a Slack token",
            Self::PrivateKey => "looks like a private key",
            Self::TypedPassword => "the Basic auth password is typed in",
            Self::TypedToken => "the Bearer token is typed in",
        }
    }
}

/// Prefixes of tokens whose rest is letters and digits (and `_`/`-` where `extra` says),
/// with the shortest rest that counts.
const PREFIXED: &[(&str, Credential, usize, &str)] = &[
    ("ghp_", Credential::GitHubToken, 20, ""),
    ("gho_", Credential::GitHubToken, 20, ""),
    ("ghu_", Credential::GitHubToken, 20, ""),
    ("ghs_", Credential::GitHubToken, 20, ""),
    ("ghr_", Credential::GitHubToken, 20, ""),
    ("github_pat_", Credential::GitHubToken, 20, "_"),
    ("sk_live_", Credential::StripeKey, 10, ""),
    ("rk_live_", Credential::StripeKey, 10, ""),
    ("xoxb-", Credential::SlackToken, 10, "-"),
    ("xoxp-", Credential::SlackToken, 10, "-"),
    ("xoxa-", Credential::SlackToken, 10, "-"),
    ("xoxr-", Credential::SlackToken, 10, "-"),
    ("xoxs-", Credential::SlackToken, 10, "-"),
];

/// A credential found in a value: what it looks like and its bytes in the value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub kind: Credential,
    /// The token alone (after `Bearer `, say), so it can be swapped for a `{{variable}}`.
    pub range: std::ops::Range<usize>,
}

/// What `value` looks like, if it holds a credential anywhere in it.
pub fn credential_in(value: &str) -> Option<Credential> {
    find(value).map(|f| f.kind)
}

/// The first credential in `value`, with where it is.
pub fn find(value: &str) -> Option<Found> {
    private_key(value)
        .or_else(|| prefixed(value))
        .or_else(|| aws_key(value))
        .or_else(|| jwt(value))
        .or_else(|| scheme_credential(value))
}

/// What an auth field holds: a typed-in Basic password or Bearer token counts whatever it
/// looks like (the whole field); other fields only when [`find`] flags them.
pub fn find_in_auth(field: AuthField, value: &str) -> Option<Found> {
    let typed = !without_variables(value).trim().is_empty();
    let whole = |kind| Found {
        kind,
        range: 0..value.len(),
    };
    match field {
        AuthField::BasicPassword if typed => Some(whole(Credential::TypedPassword)),
        AuthField::BearerToken if typed => find(value).or(Some(whole(Credential::TypedToken))),
        AuthField::BasicPassword | AuthField::BearerToken | AuthField::Other => find(value),
    }
}

/// As [`find_in_auth`], without where.
pub fn credential_in_auth(field: AuthField, value: &str) -> Option<Credential> {
    find_in_auth(field, value).map(|f| f.kind)
}

/// Which auth field a value is in, for [`find_in_auth`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthField {
    BasicPassword,
    BearerToken,
    Other,
}

/// `value` with every `{{variable}}` taken out.
fn without_variables(value: &str) -> String {
    let mut out = String::new();
    let mut at = 0;
    for (range, _) in crate::vars::spans(value) {
        if let Some(text) = value.get(at..range.start) {
            out.push_str(text);
        }
        at = range.end;
    }
    if let Some(rest) = value.get(at..) {
        out.push_str(rest);
    }
    out
}

/// Whether the byte at `at` starts a word: nothing, or no letter, digit or `_`, before it.
fn starts_word(value: &str, at: usize) -> bool {
    value
        .get(..at)
        .and_then(|before| before.chars().next_back())
        .is_none_or(|c| !c.is_ascii_alphanumeric() && c != '_')
}

/// How many bytes from `at` are ASCII characters `keep` accepts.
fn run_after(value: &str, at: usize, keep: impl Fn(char) -> bool) -> usize {
    value.get(at..).map_or(0, |rest| {
        rest.chars()
            .take_while(|c| c.is_ascii() && keep(*c))
            .count()
    })
}

/// `-----BEGIN … PRIVATE KEY-----` up to the end of its `-----END … PRIVATE KEY-----`
/// line, or of the value.
fn private_key(value: &str) -> Option<Found> {
    let start = value.find("-----BEGIN ")?;
    let rest = value.get(start..)?;
    rest.find("PRIVATE KEY-----")?;
    let end = rest
        .find("-----END ")
        .and_then(|e| {
            let tail = rest.get(e..)?;
            let close = tail.find("PRIVATE KEY-----")? + "PRIVATE KEY-----".len();
            Some(start + e + close)
        })
        .unwrap_or(value.len());
    Some(Found {
        kind: Credential::PrivateKey,
        range: start..end,
    })
}

fn prefixed(value: &str) -> Option<Found> {
    PREFIXED.iter().find_map(|&(prefix, kind, min, extra)| {
        value.match_indices(prefix).find_map(|(at, _)| {
            let run = run_after(value, at + prefix.len(), |c| {
                c.is_ascii_alphanumeric() || extra.contains(c)
            });
            (starts_word(value, at) && run >= min).then(|| Found {
                kind,
                range: at..at + prefix.len() + run,
            })
        })
    })
}

/// `AKIA` or `ASIA` and 16 capitals or digits, as a word of its own.
fn aws_key(value: &str) -> Option<Found> {
    ["AKIA", "ASIA"].iter().find_map(|prefix| {
        value.match_indices(prefix).find_map(|(at, _)| {
            let run = run_after(value, at + prefix.len(), |c| {
                c.is_ascii_uppercase() || c.is_ascii_digit()
            });
            (starts_word(value, at) && run == 16).then(|| Found {
                kind: Credential::AwsAccessKey,
                range: at..at + prefix.len() + run,
            })
        })
    })
}

/// Three base64url parts joined by dots, the first starting `eyJ` (`{"` encoded).
fn jwt(value: &str) -> Option<Found> {
    value.match_indices("eyJ").find_map(|(at, _)| {
        let len = run_after(value, at, |c| {
            c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')
        });
        let token = value.get(at..at + len)?;
        let parts: Vec<&str> = token.split('.').collect();
        let is_jwt = starts_word(value, at)
            && parts.len() == 3
            && parts.iter().take(2).all(|p| p.len() >= 10)
            && parts.get(2).is_some_and(|p| !p.is_empty());
        is_jwt.then(|| Found {
            kind: Credential::Jwt,
            range: at..at + len,
        })
    })
}

/// `Bearer <token>` or `Basic <base64 of user:password>` written out; the range is the
/// token.
fn scheme_credential(value: &str) -> Option<Found> {
    // ASCII lower-casing keeps every byte where it was.
    let lower = value.to_ascii_lowercase();
    for (scheme, kind) in [
        ("bearer ", Credential::BearerToken),
        ("basic ", Credential::BasicCredentials),
    ] {
        for (at, _) in lower.match_indices(scheme) {
            if !starts_word(value, at) {
                continue;
            }
            let after = at + scheme.len();
            let Some(rest) = value.get(after..) else {
                continue;
            };
            let start = after + (rest.len() - rest.trim_start().len());
            let len = run_after(value, start, |c| {
                c.is_ascii_alphanumeric() || "-._~+/=".contains(c)
            });
            let Some(token) = value.get(start..start + len) else {
                continue;
            };
            let found = match kind {
                Credential::BearerToken => {
                    token.len() >= 16 && token.chars().any(|c| c.is_ascii_digit())
                }
                _ => base64::engine::general_purpose::STANDARD
                    .decode(token)
                    .is_ok_and(|bytes| token.len() >= 8 && bytes.contains(&b':')),
            };
            if found {
                return Some(Found {
                    kind,
                    range: start..start + len,
                });
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const JWT: &str = concat!(
        "eyJhbGciOiJIUzI1NiJ9",
        ".eyJzdWIiOiIxMjM0NTY3ODkwIn0",
        ".dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U"
    );

    #[test]
    fn credentials_are_recognised_by_their_shape() {
        // R-CLI-6
        let cases = [
            (JWT, Credential::Jwt),
            ("Bearer abc123def456ghi789", Credential::BearerToken),
            ("basic dXNlcjpwYXNzd29yZA==", Credential::BasicCredentials),
            (
                concat!("AKIA", "IOSFODNN7EXAMPLE"),
                Credential::AwsAccessKey,
            ),
            (
                concat!("ghp_", "abcdefghijklmnopqrstuvwxyz0123"),
                Credential::GitHubToken,
            ),
            (
                concat!("github_pat_", "11ABCDEFG_abcdefghijklmnop"),
                Credential::GitHubToken,
            ),
            (
                concat!("sk_", "live_4eC39HqLyjWDarjtT1zdp7dc"),
                Credential::StripeKey,
            ),
            (
                concat!("xoxb", "-1234567890-abcdef"),
                Credential::SlackToken,
            ),
            (
                concat!(
                    "-----BEGIN RSA PRIVATE ",
                    "KEY-----\nMIIE\n-----END RSA PRIVATE KEY-----"
                ),
                Credential::PrivateKey,
            ),
        ];
        for (value, kind) in cases {
            assert_eq!(credential_in(value), Some(kind), "{value}");
        }
    }

    #[test]
    fn credentials_inside_other_text_are_found() {
        // R-CLI-6: in a JSON body, a URL or after non-ASCII text.
        assert_eq!(
            credential_in(&format!(r#"{{"token": "{JWT}"}}"#)),
            Some(Credential::Jwt)
        );
        assert_eq!(
            credential_in(concat!("https://x.io/?key=AKIA", "IOSFODNN7EXAMPLE&y=1")),
            Some(Credential::AwsAccessKey)
        );
        assert_eq!(
            credential_in(concat!("ü😀 ghp_", "abcdefghijklmnopqrstuvwxyz0123")),
            Some(Credential::GitHubToken)
        );
    }

    #[test]
    fn variables_and_ordinary_text_are_not_credentials() {
        // R-CLI-6: only {{variables}}, prose, short or partial tokens, and empty text.
        for value in [
            "",
            "{{token}}",
            "Bearer {{token}}",
            "ghp_{{github_token}}",
            "Basic {{basic}}",
            "basic authentication is supported",
            "Bearer authentication",
            "ghp_short",
            concat!("XAKIA", "IOSFODNN7EXAMPLE"),
            concat!("AKIA", "IOSFODNN7EXAMPLE1"),
            "eyJhbGciOiJIUzI1NiJ9",
            concat!("my_sk_", "live_4eC39HqLyjWDarjtT1zdp7dc"),
            "ü😀",
            &"a".repeat(100_000),
        ] {
            assert_eq!(credential_in(value), None, "{value}");
        }
    }

    #[test]
    fn the_range_is_the_token_alone() {
        // R-VAR-10: so only the token becomes a {{variable}}.
        let value = "ü😀 Bearer   abc123def456ghi789, more";
        let found = find(value).expect("found");
        assert_eq!(value.get(found.range), Some("abc123def456ghi789"));
        let value = format!("x={JWT}&y");
        assert_eq!(value.get(find(&value).expect("jwt").range), Some(JWT));
        let pem = concat!(
            "a\n-----BEGIN PRIVATE ",
            "KEY-----\nMIIE\n-----END PRIVATE KEY-----\nb"
        );
        assert_eq!(
            pem.get(find(pem).expect("pem").range),
            Some(concat!(
                "-----BEGIN PRIVATE ",
                "KEY-----\nMIIE\n-----END PRIVATE KEY-----"
            ))
        );
        let whole = find_in_auth(AuthField::BasicPassword, "hünter2").expect("typed");
        assert_eq!(whole.range, 0.."hünter2".len());
    }

    #[test]
    fn typed_in_auth_secrets_are_flagged_and_variables_are_not() {
        // R-CLI-6
        assert_eq!(
            credential_in_auth(AuthField::BasicPassword, "hunter2"),
            Some(Credential::TypedPassword)
        );
        assert_eq!(
            credential_in_auth(AuthField::BasicPassword, "{{password}}"),
            None
        );
        assert_eq!(credential_in_auth(AuthField::BasicPassword, " "), None);
        assert_eq!(
            credential_in_auth(AuthField::BearerToken, "abc"),
            Some(Credential::TypedToken)
        );
        assert_eq!(
            credential_in_auth(AuthField::BearerToken, JWT),
            Some(Credential::Jwt)
        );
        assert_eq!(
            credential_in_auth(AuthField::BearerToken, "{{token}}"),
            None
        );
        assert_eq!(credential_in_auth(AuthField::Other, "alice"), None);
    }
}
