use std::path::PathBuf;
use std::time::Duration;

use thiserror::Error;

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// `text` with its first letter in upper case, to show as a sentence.
pub fn sentence(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// An error as a sentence for people.
pub fn message(error: &Error) -> String {
    sentence(&error.to_string())
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("invalid URL `{url}`: {source}")]
    InvalidUrl {
        url: String,
        #[source]
        source: url::ParseError,
    },

    #[error("invalid header `{name}`: {reason}")]
    InvalidHeader { name: String, reason: &'static str },

    #[error("request timed out after {}", format_duration(*.0))]
    Timeout(Duration),

    /// Connection, TLS, DNS, protocol errors. The message includes the full cause chain.
    #[error("{0}")]
    Transport(String),

    #[error("could not create HTTP client: {0}")]
    ClientSetup(String),

    /// The manual proxy's URL can't be used (R-HTTP-20). Never holds the URL itself.
    #[error("the proxy can't be used: {0}")]
    InvalidProxy(String),

    #[error("{}: {source}", .path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("{}: {message}", .path.display())]
    InvalidFile { path: PathBuf, message: String },

    #[error("could not start the mock server on port {port}: {source}")]
    MockServer {
        port: u16,
        #[source]
        source: std::io::Error,
    },

    #[error("{} is not a Tortuga project (no tortuga.toml)", .0.display())]
    NotACollection(PathBuf),

    #[error("{} already contains a Tortuga project", .0.display())]
    AlreadyACollection(PathBuf),

    #[error("{} no longer exists", .0.display())]
    NotFound(PathBuf),

    #[error(
        "this project was saved by a newer Tortuga (format {found}, this version understands {supported}); it is read-only"
    )]
    ReadOnly { found: u32, supported: u32 },

    #[error("{0}")]
    InvalidOperation(String),

    /// R-VAR-5
    #[error("undefined variable{}: {}", if .0.len() == 1 { "" } else { "s" }, .0.join(", "))]
    UndefinedVariables(Vec<String>),

    /// R-HTTP-10
    #[error("path variable{} without a value: {}", if .0.len() == 1 { "" } else { "s" }, .0.join(", "))]
    EmptyPathVariables(Vec<String>),

    /// R-SRV-4
    #[error("server `{0}` is not defined in this project")]
    UnknownServer(String),

    /// R-CK-3
    #[error("could not read the saved cookies: {0}")]
    InvalidCookies(String),

    /// R-CK-3
    #[error("could not save the cookies: {0}")]
    SaveCookies(String),

    /// R-COL-15
    #[error("could not import the OpenAPI description: {0}")]
    InvalidOpenApi(String),

    /// R-COL-16
    #[error("could not import the Postman collection: {0}")]
    InvalidPostman(String),

    /// R-COL-20
    #[error("could not import the Postman environment: {0}")]
    InvalidPostmanEnvironment(String),

    /// R-COL-17
    #[error("could not import the HTTP file: {0}")]
    InvalidHttpFile(String),

    /// R-COL-29
    #[error("could not import the HTTP environment file: {0}")]
    InvalidHttpEnvironment(String),

    /// R-COL-31
    #[error("could not import the Bruno collection: {0}")]
    InvalidBruno(String),

    /// R-COL-31
    #[error("could not import the Bruno environment: {0}")]
    InvalidBrunoEnvironment(String),

    /// R-COL-32
    #[error("could not import the Insomnia export: {0}")]
    InvalidInsomnia(String),

    /// R-VAR-3, R-LOC-2
    #[error("could not access secret values: {0}")]
    SecretStore(String),

    /// R-HTTP-16
    #[error("CA certificate {}: {reason}", .path.display())]
    CaCertificate { path: PathBuf, reason: String },

    /// R-HTTP-15
    #[error("could not upload the file {}: {reason}", .path.display())]
    UploadFile { path: PathBuf, reason: String },

    /// R-SRV-4
    #[error("server \"{server}\" has no URL in {environment}")]
    ServerUrlMissing { server: String, environment: String },
}

impl Error {
    pub(crate) fn transport(err: &reqwest::Error) -> Self {
        let mut causes = Vec::new();
        let mut source = std::error::Error::source(err);
        while let Some(cause) = source {
            causes.push(cause.to_string());
            source = cause.source();
        }
        Self::Transport(describe_transport(&err.to_string(), &causes))
    }
}

/// reqwest's top-level message is generic ("error sending request…"); the
/// useful part (DNS failure, refused, certificate…) is in the cause chain.
fn describe_transport(top: &str, causes: &[String]) -> String {
    let mut details = top.to_owned();
    for cause in causes {
        if !details.contains(cause.as_str()) {
            details.push_str(": ");
            details.push_str(cause);
        }
    }
    if details.to_ascii_lowercase().contains("certificate") {
        format!(
            "The server's TLS certificate is not trusted. If it is signed by your company's \
             CA, add that CA certificate in Settings; for a test server, add its host to \
             “Don't verify certificates of these hosts” there. ({details})"
        )
    } else {
        details
    }
}

fn format_duration(d: Duration) -> String {
    if d.as_millis() < 1000 {
        format!("{} ms", d.as_millis())
    } else {
        format!("{:.1} s", d.as_secs_f64())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sentence_capitalises_only_the_first_letter() {
        assert_eq!(sentence("the collection"), "The collection");
        assert_eq!(sentence("“x” uses"), "“x” uses");
        assert_eq!(sentence("ærø"), "Ærø");
        assert_eq!(sentence(""), "");
    }

    #[test]
    fn transport_message_includes_causes_once() {
        let causes = [
            "client error (Connect)".to_owned(),
            "connection refused".to_owned(),
        ];
        assert_eq!(
            describe_transport("error sending request (client error (Connect))", &causes),
            "error sending request (client error (Connect)): connection refused"
        );
    }

    #[test]
    fn certificate_errors_get_an_explanation() {
        let msg = describe_transport(
            "error sending request",
            &["invalid peer certificate: UnknownIssuer".into()],
        );
        assert!(
            msg.starts_with("The server's TLS certificate is not trusted."),
            "{msg}"
        );
        assert!(
            msg.ends_with("(error sending request: invalid peer certificate: UnknownIssuer)"),
            "{msg}"
        );
    }
}
