//! Workspaces: named sets of projects (R-WS-1), the shareable workspace file (R-WS-5),
//! the variables every project of a workspace can use (R-WS-8), its environments
//! (R-WS-9), and the request settings, headers and auth its projects inherit (R-SET-1,
//! R-SET-2).
//!
//! A workspace itself is local data, kept by the app. "Save as file…" writes it as a
//! `<name>.tortuga-workspace.toml` that can be committed, e.g. to a "platform" repo:
//! each project by its path relative to the file, plus its git remote URL, so a new
//! teammate sees which repositories to clone, and the workspace variables. Only shared
//! data goes into the file (R-GIT-4): what is open or active stays on each machine, and
//! secret variables are listed by name only (R-VAR-3).
//!
//! ```toml
//! format = 2
//! name = "Acme platform"
//! secrets = ["api_key"]
//!
//! [variables]
//! tenant = "acme"
//!
//! [[projects]]
//! path = "../users-api"
//! remote = "git@github.com:acme/users-api.git"
//!
//! [[environments]]
//! id = "staging"
//! name = "Staging"
//! secrets = ["token"]
//!
//! [environments.servers]
//! Users = "https://users.staging.acme.test"
//!
//! [environments.variables]
//! tenant = "acme-staging"
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::format::{Environment, EnvironmentColor, Manifest};
use crate::model::{Auth, KeyValue};
use crate::secrets::SecretKey;
use crate::settings::RequestSettings;
use crate::{Error, Result, slug};

/// File name ending of a shareable workspace file.
pub const WORKSPACE_FILE_SUFFIX: &str = ".tortuga-workspace.toml";

/// Newest format version of workspace files this build reads. Files are written as
/// format 1 unless they hold variables or defaults (2) or environments (3): older builds
/// would drop those when they rewrite the file, so they must refuse it instead.
pub const WORKSPACE_FORMAT: u32 = 3;

/// Variables every project of a workspace can use (R-WS-8). They rank below the
/// project's own values, so a project can always override them.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkspaceVariables {
    /// Name → value, for variables that are not secret.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub values: BTreeMap<String, String>,
    /// Names of secret variables; their values stay on each machine (R-VAR-3).
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    pub secrets: BTreeSet<String>,
}

impl WorkspaceVariables {
    pub fn is_empty(&self) -> bool {
        self.values.is_empty() && self.secrets.is_empty()
    }

    /// The values requests use: the shared ones, plus this machine's secret values
    /// (`secrets`, by name). A secret without a local value is undefined.
    pub fn resolved(&self, secrets: &BTreeMap<String, String>) -> BTreeMap<String, String> {
        let mut out = self.values.clone();
        out.extend(
            secrets
                .iter()
                .filter(|(name, value)| self.secrets.contains(*name) && !value.is_empty())
                .map(|(k, v)| (k.clone(), v.clone())),
        );
        out
    }
}

/// What every project of a workspace inherits below its own (R-SET-1, R-SET-2):
/// request settings, headers and auth.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkspaceDefaults {
    #[serde(skip_serializing_if = "RequestSettings::is_empty")]
    pub settings: RequestSettings,
    /// Headers every request inherits unless its project, folders or itself name them.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<KeyValue>,
    /// Auth for requests whose folders and project set none.
    #[serde(skip_serializing_if = "Auth::is_none")]
    pub auth: Auth,
}

impl WorkspaceDefaults {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Where the secret value of workspace variable `name` is kept on this machine. `id` is
/// the workspace's local id, which stays the same when it is renamed.
pub fn secret_key(id: &str, name: &str) -> SecretKey {
    SecretKey::new(Path::new(&format!("workspace:{id}")), None, name)
}

/// An environment every project of the workspace has (R-WS-9, D62): its values, and the
/// base URLs of servers by name. A project's own environment of the same name wins over
/// it, value by value.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkspaceEnvironment {
    /// Stays the same when it is renamed; keys this machine's secret values.
    pub id: String,
    pub name: String,
    /// Its dot's colour (R-SRV-8), unless a project's own environment gives it one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<EnvironmentColor>,
    /// Ask before writes are sent to it (R-SRV-9), in every project.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub protected: bool,
    /// Server name → base URL, for the servers of that name in every project.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub servers: BTreeMap<String, String>,
    /// Name → value, for variables that are not secret.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub variables: BTreeMap<String, String>,
    /// Names of secret variables; their values stay on each machine (R-VAR-3).
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    pub secrets: BTreeSet<String>,
}

impl WorkspaceEnvironment {
    /// Whether this is the environment called `name` (R-WS-9).
    pub fn is_named(&self, name: &str) -> bool {
        crate::format::names_match(&self.name, name)
    }

    /// A free id for a new environment called `name`.
    pub fn new_id(name: &str, existing: &[Self]) -> String {
        slug::unique(&slug::slugify(name), |c| existing.iter().any(|e| e.id == c))
    }
}

/// The environment a project's requests use (R-WS-9): the workspace's `shared`, with its
/// server URLs given to the project's servers of the same name and this machine's
/// `secret_values` for it, overridden value by value by the project's `own` environment of
/// the same name. Its id is the project's environment's, or [`WORKSPACE_ENVIRONMENT_ID`].
/// `None` when there is neither.
pub fn effective_environment(
    shared: Option<&WorkspaceEnvironment>,
    secret_values: &BTreeMap<String, String>,
    own: Option<&Environment>,
    manifest: &Manifest,
) -> Option<Environment> {
    let Some(shared) = shared else {
        return own.cloned();
    };
    let mut env = Environment {
        id: WORKSPACE_ENVIRONMENT_ID.to_owned(),
        name: shared.name.clone(),
        color: shared.color,
        protected: shared.protected,
        ..Environment::default()
    };
    for (name, url) in &shared.servers {
        for server in manifest.servers.iter().filter(|s| s.is_named(name)) {
            env.servers.insert(server.id.clone(), url.clone());
        }
    }
    env.variables.clone_from(&shared.variables);
    env.variables.extend(
        secret_values
            .iter()
            .filter(|(name, value)| shared.secrets.contains(*name) && !value.is_empty())
            .map(|(k, v)| (k.clone(), v.clone())),
    );
    if let Some(own) = own {
        env.id.clone_from(&own.id);
        env.name.clone_from(&own.name);
        env.seq = own.seq;
        env.color = own.color.or(shared.color);
        env.protected = own.protected || shared.protected;
        // An empty value in the project means "inherit" (R-VAR-7), as in `vars::Context`.
        env.servers.extend(
            own.servers
                .iter()
                .filter(|(_, v)| !v.trim().is_empty())
                .map(|(k, v)| (k.clone(), v.clone())),
        );
        env.variables.extend(
            own.variables
                .iter()
                .filter(|(_, v)| !v.is_empty())
                .map(|(k, v)| (k.clone(), v.clone())),
        );
    }
    Some(env)
}

/// The id of a project's environment that only the workspace defines (R-WS-9). Never a
/// file stem, which are slugs.
pub const WORKSPACE_ENVIRONMENT_ID: &str = "@workspace";

/// Where this machine's secret value `name` of workspace environment `env_id` is kept.
pub fn environment_secret_key(id: &str, env_id: &str, name: &str) -> SecretKey {
    SecretKey::new(Path::new(&format!("workspace:{id}")), Some(env_id), name)
}

/// A project in a workspace: where it is, and where its repository can be cloned from.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectRef {
    /// Absolute path of the project directory (the one holding `tortuga.toml`).
    pub path: PathBuf,
    /// Remote URL of the project's git repository, without credentials.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote: Option<String>,
}

impl ProjectRef {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            remote: None,
        }
    }
}

/// The contents of a workspace file.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WorkspaceFile {
    pub name: String,
    /// Absolute paths, resolved against the file's directory.
    pub projects: Vec<ProjectRef>,
    pub variables: WorkspaceVariables,
    pub defaults: WorkspaceDefaults,
    pub environments: Vec<WorkspaceEnvironment>,
}

#[derive(Serialize, Deserialize)]
struct FileText {
    format: u32,
    name: String,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    secrets: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "RequestSettings::is_empty")]
    settings: RequestSettings,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    variables: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Auth::is_none")]
    auth: Auth,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    headers: Vec<KeyValue>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    projects: Vec<FileProject>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    environments: Vec<WorkspaceEnvironment>,
}

#[derive(Serialize, Deserialize)]
struct FileProject {
    /// Relative to the workspace file, with `/` separators on every OS.
    path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    remote: Option<String>,
}

/// A file name for a workspace called `name`, e.g. `acme-platform.tortuga-workspace.toml`.
pub fn file_name(name: &str) -> String {
    format!("{}{WORKSPACE_FILE_SUFFIX}", slug::slugify(name))
}

/// Writes `workspace` as the file that will be saved at `file` (its project paths are
/// absolute). Deterministic: the same workspace always gives the same text, so saving it
/// again never changes the file.
pub fn write(workspace: &WorkspaceFile, file: &Path) -> String {
    let dir = file.parent().unwrap_or(Path::new(""));
    let WorkspaceFile {
        name,
        projects,
        variables,
        defaults,
        environments,
    } = workspace;
    let format = if !environments.is_empty() {
        3
    } else if variables.is_empty() && defaults.is_empty() {
        1
    } else {
        2
    };
    let text = FileText {
        format,
        name: name.clone(),
        secrets: variables.secrets.clone(),
        settings: defaults.settings,
        variables: variables.values.clone(),
        auth: defaults.auth.clone(),
        headers: defaults.headers.clone(),
        environments: environments.clone(),
        projects: projects
            .iter()
            .map(|p| FileProject {
                path: to_slashes(&relative_path(dir, &p.path)),
                remote: p.remote.as_deref().map(without_credentials),
            })
            .collect(),
    };
    toml::to_string(&text).expect("workspace files always serialize to TOML")
}

/// Reads the workspace file at `file`; project paths come back absolute.
pub fn read(text: &str, file: &Path) -> Result<WorkspaceFile> {
    let invalid = |message: String| Error::InvalidFile {
        path: file.to_owned(),
        message,
    };
    let parsed: FileText = toml::from_str(text).map_err(|e| invalid(e.message().to_owned()))?;
    if parsed.format > WORKSPACE_FORMAT {
        return Err(invalid(format!(
            "saved by a newer Tortuga (format {}, this version understands {WORKSPACE_FORMAT})",
            parsed.format
        )));
    }
    let dir = file.parent().unwrap_or(Path::new(""));
    Ok(WorkspaceFile {
        name: parsed.name,
        projects: parsed
            .projects
            .into_iter()
            .map(|p| ProjectRef {
                path: normalize(&dir.join(&p.path)),
                remote: p.remote,
            })
            .collect(),
        variables: WorkspaceVariables {
            values: parsed.variables,
            secrets: parsed.secrets,
        },
        defaults: WorkspaceDefaults {
            settings: parsed.settings,
            headers: parsed.headers,
            auth: parsed.auth,
        },
        environments: parsed.environments,
    })
}

/// `to` relative to the directory `from`, worked out from the paths alone. Paths
/// with nothing in common (another Windows drive) stay absolute.
pub fn relative_path(from: &Path, to: &Path) -> PathBuf {
    let from = normalize(from);
    let to = normalize(to);
    let from: Vec<Component<'_>> = from.components().collect();
    let to_parts: Vec<Component<'_>> = to.components().collect();
    let common = from
        .iter()
        .zip(&to_parts)
        .take_while(|(a, b)| a == b)
        .count();
    if common == 0 {
        return to;
    }
    let mut out = PathBuf::new();
    for _ in common..from.len() {
        out.push("..");
    }
    for part in &to_parts[common..] {
        out.push(part.as_os_str());
    }
    if out.as_os_str().is_empty() {
        out.push(".");
    }
    out
}

/// Removes `.` and resolves `..` without touching the disk.
pub fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// A remote URL that is safe to share: passwords and tokens in it are dropped (P6).
/// The user name stays for SSH (`git@host:…` needs it) and goes for HTTP(S), where
/// it is often a token too.
pub fn without_credentials(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else {
        return url.to_owned(); // scp-like `git@host:path`: no password possible
    };
    let (authority, path) = rest.split_at(rest.find('/').unwrap_or(rest.len()));
    let Some((user_info, host)) = authority.rsplit_once('@') else {
        return url.to_owned();
    };
    let keep_user = scheme.eq_ignore_ascii_case("ssh");
    let user = user_info.split(':').next().unwrap_or_default();
    if keep_user && !user.is_empty() {
        format!("{scheme}://{user}@{host}{path}")
    } else {
        format!("{scheme}://{host}{path}")
    }
}

fn to_slashes(path: &Path) -> String {
    path.components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
        .replace("//", "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_written_relative_to_the_file() {
        let file = Path::new("/src/platform/acme.tortuga-workspace.toml");
        let projects = [
            ProjectRef {
                path: "/src/users-api".into(),
                remote: Some("https://bob:s3cret@github.com/acme/users-api.git".into()),
            },
            ProjectRef::new("/src/platform/api/orders"),
            ProjectRef::new("/src/platform"),
        ];
        let workspace = WorkspaceFile {
            name: "Acme".into(),
            projects: projects.to_vec(),
            ..WorkspaceFile::default()
        };
        let text = write(&workspace, file);
        assert_eq!(
            text,
            "format = 1\nname = \"Acme\"\n\n\
             [[projects]]\npath = \"../users-api\"\nremote = \"https://github.com/acme/users-api.git\"\n\n\
             [[projects]]\npath = \"api/orders\"\n\n\
             [[projects]]\npath = \".\"\n"
        );
        let back = read(&text, file).expect("read");
        assert_eq!(back.name, "Acme");
        let paths: Vec<&Path> = back.projects.iter().map(|p| p.path.as_path()).collect();
        assert_eq!(
            paths,
            [
                Path::new("/src/users-api"),
                Path::new("/src/platform/api/orders"),
                Path::new("/src/platform")
            ]
        );
        assert_eq!(
            back.projects[0].remote.as_deref(),
            Some("https://github.com/acme/users-api.git"),
            "no password in a shared file"
        );
        assert_eq!(write(&workspace, file), text, "deterministic");
    }

    #[test]
    fn newer_or_broken_files_are_errors() {
        let file = Path::new("/w/x.tortuga-workspace.toml");
        assert!(read("format = 4\nname = \"X\"\n", file).is_err());
        assert!(read("name = ", file).is_err());
        assert_eq!(
            read("format = 1\nname = \"X\"\n", file)
                .expect("empty")
                .projects,
            []
        );
    }

    /// R-WS-8: variables go into the file, secrets by name only, and only then is the
    /// file format 2 (older builds refuse it rather than drop the variables).
    #[test]
    fn variables_are_written_and_read() {
        let file = Path::new("/src/platform/acme.tortuga-workspace.toml");
        let variables = WorkspaceVariables {
            values: [("tenant".to_owned(), "acme".to_owned())].into(),
            secrets: ["api_key".to_owned()].into(),
        };
        let projects = [ProjectRef::new("/src/users-api")];
        let text = write(
            &WorkspaceFile {
                name: "Acme".into(),
                projects: projects.to_vec(),
                variables: variables.clone(),
                ..WorkspaceFile::default()
            },
            file,
        );
        assert_eq!(
            text,
            "format = 2\nname = \"Acme\"\nsecrets = [\"api_key\"]\n\n\
             [variables]\ntenant = \"acme\"\n\n\
             [[projects]]\npath = \"../users-api\"\n"
        );
        let back = read(&text, file).expect("read");
        assert_eq!(back.variables, variables);
        assert_eq!(back.projects, projects);
    }

    /// R-SET-1, R-SET-2: settings, auth and headers go into the file (format 2) and
    /// come back.
    #[test]
    fn defaults_are_written_and_read() {
        let file = Path::new("/src/platform/acme.tortuga-workspace.toml");
        let defaults = WorkspaceDefaults {
            settings: RequestSettings {
                timeout_ms: Some(5_000),
                ..RequestSettings::default()
            },
            headers: vec![KeyValue::new("X-Tenant", "{{tenant}}")],
            auth: Auth::Bearer {
                token: "{{token}}".into(),
            },
        };
        let text = write(
            &WorkspaceFile {
                name: "Acme".into(),
                defaults: defaults.clone(),
                ..WorkspaceFile::default()
            },
            file,
        );
        assert_eq!(
            text,
            "format = 2\nname = \"Acme\"\n\n[settings]\ntimeout_ms = 5000\n\n\
             [auth]\nkind = \"bearer\"\ntoken = \"{{token}}\"\n\n\
             [[headers]]\nname = \"X-Tenant\"\nvalue = \"{{tenant}}\"\n"
        );
        assert_eq!(read(&text, file).expect("read").defaults, defaults);
    }

    /// R-WS-8: secret values come from this machine; a secret without one is undefined.
    #[test]
    fn resolved_values_add_local_secrets() {
        let variables = WorkspaceVariables {
            values: [("tenant".to_owned(), "acme".to_owned())].into(),
            secrets: ["api_key".to_owned(), "unset".to_owned()].into(),
        };
        let local = BTreeMap::from([
            ("api_key".to_owned(), "k-1".to_owned()),
            ("unset".to_owned(), String::new()),
            ("not_listed".to_owned(), "x".to_owned()),
        ]);
        assert_eq!(
            variables.resolved(&local),
            BTreeMap::from([
                ("api_key".to_owned(), "k-1".to_owned()),
                ("tenant".to_owned(), "acme".to_owned()),
            ])
        );
        assert_eq!(
            secret_key("acme", "api_key").account(),
            "workspace:acme|*|api_key"
        );
    }

    #[test]
    fn relative_paths() {
        assert_eq!(
            relative_path(Path::new("/a/b"), Path::new("/a/c/d")),
            Path::new("../c/d")
        );
        assert_eq!(
            relative_path(Path::new("/a/b/"), Path::new("/a/b/./c")),
            Path::new("c")
        );
        assert_eq!(
            relative_path(Path::new("/a"), Path::new("/a")),
            Path::new(".")
        );
        assert_eq!(normalize(Path::new("/a/b/../c/./d")), Path::new("/a/c/d"));
    }

    #[test]
    fn credentials_are_removed_from_remotes() {
        assert_eq!(
            without_credentials("https://x-token:ghp_abc@github.com/a/b.git"),
            "https://github.com/a/b.git"
        );
        assert_eq!(
            without_credentials("ssh://git:pw@host:22/a.git"),
            "ssh://git@host:22/a.git"
        );
        assert_eq!(
            without_credentials("git@github.com:a/b.git"),
            "git@github.com:a/b.git"
        );
        assert_eq!(
            without_credentials("https://github.com/a/b.git"),
            "https://github.com/a/b.git"
        );
    }

    #[test]
    fn file_names_come_from_the_workspace_name() {
        assert_eq!(
            file_name("Acme Platform"),
            "acme-platform.tortuga-workspace.toml"
        );
        assert_eq!(file_name("!!"), "untitled.tortuga-workspace.toml");
    }

    fn staging() -> WorkspaceEnvironment {
        WorkspaceEnvironment {
            id: "staging".into(),
            name: "Staging".into(),
            color: Some(EnvironmentColor::Orange),
            servers: [("users".to_owned(), "https://users.staging.test".to_owned())].into(),
            variables: [
                ("tenant".to_owned(), "acme ü😀".to_owned()),
                ("region".to_owned(), "eu".to_owned()),
            ]
            .into(),
            secrets: ["token".to_owned()].into(),
            ..WorkspaceEnvironment::default()
        }
    }

    /// R-WS-9: environments go into the file (format 3, secrets by name only) and come back.
    #[test]
    fn environments_are_written_and_read() {
        let file = Path::new("/src/platform/acme.tortuga-workspace.toml");
        let workspace = WorkspaceFile {
            name: "Acme".into(),
            environments: vec![staging()],
            ..WorkspaceFile::default()
        };
        let text = write(&workspace, file);
        assert!(text.starts_with("format = 3\n"), "{text}");
        assert!(text.contains("[[environments]]"), "{text}");
        assert!(text.contains("secrets = [\"token\"]"), "{text}");
        assert_eq!(read(&text, file).expect("read"), workspace);
    }

    /// R-WS-9, D62: a project gets the workspace's environment, its servers by name and
    /// its secret values; its own environment of that name wins value by value.
    #[test]
    fn a_project_environment_wins_over_the_workspace_one() {
        let manifest = Manifest {
            servers: vec![
                crate::format::Server {
                    id: "u".into(),
                    name: "USERS".into(),
                },
                crate::format::Server {
                    id: "o".into(),
                    name: "Orders".into(),
                },
            ],
            ..Manifest::new("P")
        };
        let secrets: BTreeMap<String, String> = [
            ("token".to_owned(), "s3cret".to_owned()),
            ("not-secret".to_owned(), "ignored".to_owned()),
        ]
        .into();
        let shared = staging();
        let env = effective_environment(Some(&shared), &secrets, None, &manifest).expect("env");
        assert_eq!(env.id, WORKSPACE_ENVIRONMENT_ID);
        assert_eq!(env.name, "Staging");
        assert_eq!(
            env.servers["u"], "https://users.staging.test",
            "by name, any case"
        );
        assert!(!env.servers.contains_key("o"));
        assert_eq!(env.variables["token"], "s3cret");
        assert!(!env.variables.contains_key("not-secret"));
        assert_eq!(env.color, Some(EnvironmentColor::Orange));

        let own = Environment {
            id: "stg".into(),
            name: "staging".into(),
            protected: true,
            servers: [
                ("o".to_owned(), "https://orders.stg".to_owned()),
                ("u".to_owned(), " ".to_owned()),
            ]
            .into(),
            variables: [
                ("region".to_owned(), "us".to_owned()),
                ("tenant".to_owned(), String::new()),
            ]
            .into(),
            ..Environment::default()
        };
        let env =
            effective_environment(Some(&shared), &secrets, Some(&own), &manifest).expect("env");
        assert_eq!(env.id, "stg");
        assert_eq!(env.servers["o"], "https://orders.stg");
        assert_eq!(
            env.servers["u"], "https://users.staging.test",
            "empty inherits"
        );
        assert_eq!(env.variables["region"], "us", "the project wins");
        assert_eq!(env.variables["tenant"], "acme ü😀", "empty inherits");
        assert!(env.protected, "either protects it");
        assert_eq!(
            env.color,
            Some(EnvironmentColor::Orange),
            "the project gives none"
        );

        assert_eq!(
            effective_environment(None, &secrets, Some(&own), &manifest),
            Some(own.clone())
        );
        assert_eq!(effective_environment(None, &secrets, None, &manifest), None);
    }
}
