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

/// What `value` looks like, if it holds a credential anywhere in it.
pub fn credential_in(value: &str) -> Option<Credential> {
    if value.contains("-----BEGIN ") && value.contains("PRIVATE KEY-----") {
        return Some(Credential::PrivateKey);
    }
    for &(prefix, kind, min, extra) in PREFIXED {
        if value.match_indices(prefix).any(|(at, _)| {
            starts_word(value, at) && run_after(value, at + prefix.len(), extra) >= min
        }) {
            return Some(kind);
        }
    }
    if has_aws_key(value) {
        return Some(Credential::AwsAccessKey);
    }
    if has_jwt(value) {
        return Some(Credential::Jwt);
    }
    scheme_credential(value)
}

/// What an auth field holds: a typed-in Basic password or Bearer token counts whatever it
/// looks like; other fields only when [`credential_in`] flags them.
pub fn credential_in_auth(field: AuthField, value: &str) -> Option<Credential> {
    let typed = !without_variables(value).trim().is_empty();
    match field {
        AuthField::BasicPassword if typed => Some(Credential::TypedPassword),
        AuthField::BearerToken if typed => credential_in(value).or(Some(Credential::TypedToken)),
        AuthField::BasicPassword | AuthField::BearerToken | AuthField::Other => {
            credential_in(value)
        }
    }
}

/// Which auth field a value is in, for [`credential_in_auth`].
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

/// How many letters and digits (and characters of `extra`) follow from byte `at`.
fn run_after(value: &str, at: usize, extra: &str) -> usize {
    value.get(at..).map_or(0, |rest| {
        rest.chars()
            .take_while(|c| c.is_ascii_alphanumeric() || extra.contains(*c))
            .count()
    })
}

/// `AKIA` or `ASIA` and 16 capitals or digits, as a word of its own.
fn has_aws_key(value: &str) -> bool {
    ["AKIA", "ASIA"].iter().any(|prefix| {
        value.match_indices(prefix).any(|(at, _)| {
            let Some(rest) = value.get(at + prefix.len()..) else {
                return false;
            };
            let run = rest
                .chars()
                .take_while(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
                .count();
            starts_word(value, at) && run == 16
        })
    })
}

/// Three base64url parts joined by dots, the first starting `eyJ` (`{"` encoded).
fn has_jwt(value: &str) -> bool {
    value.match_indices("eyJ").any(|(at, _)| {
        if !starts_word(value, at) {
            return false;
        }
        let Some(rest) = value.get(at..) else {
            return false;
        };
        let token: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
            .collect();
        let parts: Vec<&str> = token.split('.').collect();
        parts.len() == 3
            && parts.iter().take(2).all(|p| p.len() >= 10)
            && parts.get(2).is_some_and(|p| !p.is_empty())
    })
}

/// `Bearer <token>` or `Basic <base64 of user:password>` written out.
fn scheme_credential(value: &str) -> Option<Credential> {
    let lower = value.to_ascii_lowercase();
    for (scheme, kind) in [
        ("bearer ", Credential::BearerToken),
        ("basic ", Credential::BasicCredentials),
    ] {
        for (at, _) in lower.match_indices(scheme) {
            if !starts_word(value, at) {
                continue;
            }
            let Some(rest) = value.get(at + scheme.len()..) else {
                continue;
            };
            let token: String = rest
                .trim_start()
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || "-._~+/=".contains(*c))
                .collect();
            let found = match kind {
                Credential::BearerToken => {
                    token.len() >= 16 && token.chars().any(|c| c.is_ascii_digit())
                }
                _ => base64::engine::general_purpose::STANDARD
                    .decode(&token)
                    .is_ok_and(|bytes| token.len() >= 8 && bytes.contains(&b':')),
            };
            if found {
                return Some(kind);
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
