//! Workspaces: named sets of projects (R-WS-1), the shareable workspace file (R-WS-5),
//! the variables every project of a workspace can use (R-WS-8), its environments
//! (R-WS-9), and the request settings, headers and auth its projects inherit (R-SET-1,
//! R-SET-2).
//!
//! A workspace itself is local data, kept by the app. "Save as file…" writes it as a
//! `<name>.tortuga-workspace.toml` that can be committed, e.g. to a "platform" repo.
//! A project in the file's own repository, or without a git remote, is listed by its
//! path relative to the file. A project in another repository is listed by that
//! repository's remote URL and its folder in it (R-WS-11), so it is found wherever a
//! teammate cloned it, and a new teammate sees what to clone. Only shared
//! data goes into the file (R-GIT-4): what is open or active stays on each machine, and
//! secret variables are listed by name only (R-VAR-3).
//!
//! ```toml
//! format = 4
//! name = "Acme platform"
//! secrets = ["api_key"]
//!
//! [variables]
//! tenant = "acme"
//!
//! [[projects]]
//! path = "gateway"
//! remote = "git@github.com:acme/platform.git"
//!
//! [[projects]]
//! remote = "git@github.com:acme/users-api.git"
//! folder = "api"
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

use crate::collection::MANIFEST_FILE;
use crate::format::{Environment, EnvironmentColor, Manifest};
use crate::model::{Auth, KeyValue};
use crate::secrets::SecretKey;
use crate::settings::RequestSettings;
use crate::storage::Storage;
use crate::{Error, Result, slug};

/// File name ending of a shareable workspace file.
pub const WORKSPACE_FILE_SUFFIX: &str = ".tortuga-workspace.toml";

/// Newest format version of workspace files this build reads. Files are written as
/// format 1 unless they hold variables or defaults (2), environments (3) or projects
/// listed by remote (4): older builds would drop or misread those, so they must refuse
/// the file instead.
pub const WORKSPACE_FORMAT: u32 = 4;

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

/// The names of servers the workspace's `environments` give a base URL to that `manifest`
/// has none of, each once whatever its case (R-WS-12): every project lists them.
pub fn servers_missing_from(
    environments: &[WorkspaceEnvironment],
    manifest: &Manifest,
) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for name in environments.iter().flat_map(|e| e.servers.keys()) {
        let known = names.iter().any(|n| crate::format::names_match(n, name))
            || manifest.servers.iter().any(|s| s.is_named(name));
        if !known {
            names.push(name.clone());
        }
    }
    names
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
    /// The project's folder in its git repository, with `/` separators (empty for the
    /// top folder); known with `remote`. With them a project in another repository
    /// than the workspace file is shared by remote (R-WS-11).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
    /// The path as the workspace file lists it, for a project read from one. It is
    /// written back as it was, so locating the project on this machine doesn't change
    /// the shared file (R-WS-11).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shared_path: Option<String>,
}

impl ProjectRef {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            ..Self::default()
        }
    }

    /// The top folder of the project's repository, when its folder in it is known.
    pub fn repository(&self) -> Option<PathBuf> {
        let folder = self.folder.as_deref()?;
        let depth = folder.split('/').filter(|part| !part.is_empty()).count();
        self.path.ancestors().nth(depth).map(Path::to_path_buf)
    }

    /// Listed by remote in a workspace file at `file` (R-WS-11): in another repository
    /// than the file, with a remote and its folder in it known.
    fn shared_by_remote(&self, file: &Path) -> bool {
        self.remote.is_some()
            && self
                .repository()
                .is_some_and(|repo| !normalize(file).starts_with(normalize(&repo)))
    }

    /// Lists the same project of the same repository as `other`, both shared by remote.
    pub fn same_remote_project(&self, other: &Self) -> bool {
        match (&self.remote, &other.remote, &self.folder, &other.folder) {
            (Some(a), Some(b), Some(f), Some(g)) => f == g && same_repository(a, b),
            _ => false,
        }
    }
}

/// The folder of the project at `project` in the repository at `repository`, with `/`
/// separators (empty for its top folder); `None` when it isn't in it.
pub fn folder_in(repository: &Path, project: &Path) -> Option<String> {
    normalize(project)
        .strip_prefix(normalize(repository))
        .ok()
        .map(to_slashes)
}

/// Where the project `project`, listed by remote, is on this machine (R-WS-11): its
/// folder in the first clone of the same remote among the repositories of `known` that
/// has the project.
pub fn locate(
    storage: &dyn Storage,
    project: &ProjectRef,
    known: &[ProjectRef],
) -> Option<PathBuf> {
    let remote = project.remote.as_deref()?;
    let folder = project.folder.as_deref()?;
    known
        .iter()
        .filter(|k| {
            k.remote
                .as_deref()
                .is_some_and(|r| same_repository(r, remote))
        })
        .filter_map(ProjectRef::repository)
        .map(|repo| normalize(&repo.join(folder)))
        .find(|path| storage.exists(&path.join(MANIFEST_FILE)))
}

/// Two remote URLs name the same repository, however they are written: SSH or HTTPS,
/// with or without user, port, `.git` or a trailing `/`, in any case.
pub fn same_repository(a: &str, b: &str) -> bool {
    repository_key(a) == repository_key(b)
}

fn repository_key(url: &str) -> String {
    let url = url.trim().to_lowercase();
    let (authority, path) = match url.split_once("://") {
        Some((_, rest)) => rest.split_at(rest.find('/').unwrap_or(rest.len())),
        // scp-like `git@host:path`, or a local path
        None => url.split_once(':').unwrap_or(("", url.as_str())),
    };
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let host = host.split(':').next().unwrap_or_default();
    let path = path.trim_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    format!("{host}/{path}")
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
    /// Relative to the workspace file, with `/` separators on every OS; none for a
    /// project listed by remote and folder (R-WS-11).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    remote: Option<String>,
    /// The project's folder in the repository, for a project listed by remote; none
    /// for its top folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    folder: Option<String>,
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
    let by_remote = projects.iter().any(|p| p.shared_by_remote(file));
    let format = if by_remote {
        4
    } else if !environments.is_empty() {
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
            .map(|p| {
                let remote = p.remote.as_deref().map(without_credentials);
                if p.shared_by_remote(file) {
                    let folder = p.folder.clone().filter(|f| !f.is_empty());
                    return FileProject {
                        path: None,
                        remote,
                        folder,
                    };
                }
                let path = p
                    .shared_path
                    .clone()
                    .unwrap_or_else(|| to_slashes(&relative_path(dir, &p.path)));
                FileProject {
                    path: Some(path),
                    remote,
                    folder: None,
                }
            })
            .collect(),
    };
    toml::to_string(&text).expect("workspace files always serialize to TOML")
}

/// Reads the workspace file at `file`; project paths come back absolute. A project
/// listed by remote comes back where a clone next to the file's folder would have it,
/// until [`locate`] finds it on this machine.
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
    let projects = parsed
        .projects
        .into_iter()
        .map(|p| match (p.path, p.remote) {
            (Some(path), remote) => Ok(ProjectRef {
                path: normalize(&dir.join(&path)),
                remote,
                folder: None,
                shared_path: Some(path),
            }),
            (None, Some(remote)) => {
                let folder = p.folder.unwrap_or_default();
                let clone = dir.join("..").join(repository_name(&remote)).join(&folder);
                Ok(ProjectRef {
                    path: normalize(&clone),
                    remote: Some(remote),
                    folder: Some(folder),
                    shared_path: None,
                })
            }
            (None, None) => Err(invalid("a project has neither a path nor a remote".into())),
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(WorkspaceFile {
        name: parsed.name,
        projects,
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

/// The folder `git clone` makes for `remote`: its last part without `.git`.
fn repository_name(remote: &str) -> &str {
    let last = remote
        .trim_end_matches('/')
        .rsplit(['/', ':', '\\'])
        .next()
        .unwrap_or(remote);
    last.strip_suffix(".git").unwrap_or(last)
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
                ..ProjectRef::default()
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

    /// R-WS-11: a project in another repository is listed by remote and folder, so the
    /// file doesn't depend on where anyone cloned it; projects in the file's own
    /// repository, or without a folder known, keep their path.
    #[test]
    fn projects_in_other_repositories_are_listed_by_remote() {
        let file = Path::new("/src/platform/acme.tortuga-workspace.toml");
        let workspace = WorkspaceFile {
            name: "Acme".into(),
            projects: vec![
                ProjectRef {
                    path: "/src/platform/gateway".into(),
                    remote: Some("git@github.com:acme/platform.git".into()),
                    folder: Some("gateway".into()),
                    shared_path: None,
                },
                ProjectRef {
                    path: "/home/ann/code/users-api/api".into(),
                    remote: Some("https://bob:s3cret@github.com/acme/users-api.git".into()),
                    folder: Some("api".into()),
                    shared_path: None,
                },
                ProjectRef {
                    path: "/elsewhere/billing".into(),
                    remote: Some("git@github.com:acme/billing.git".into()),
                    folder: Some(String::new()),
                    shared_path: None,
                },
                ProjectRef::new("/src/scratch-ü😀"),
            ],
            ..WorkspaceFile::default()
        };
        let text = write(&workspace, file);
        assert_eq!(
            text,
            "format = 4\nname = \"Acme\"\n\n\
             [[projects]]\npath = \"gateway\"\nremote = \"git@github.com:acme/platform.git\"\n\n\
             [[projects]]\nremote = \"https://github.com/acme/users-api.git\"\nfolder = \"api\"\n\n\
             [[projects]]\nremote = \"git@github.com:acme/billing.git\"\n\n\
             [[projects]]\npath = \"../scratch-ü😀\"\n"
        );
        let back = read(&text, file).expect("read");
        let paths: Vec<&Path> = back.projects.iter().map(|p| p.path.as_path()).collect();
        assert_eq!(
            paths,
            [
                Path::new("/src/platform/gateway"),
                Path::new("/src/users-api/api"),
                Path::new("/src/billing"),
                Path::new("/src/scratch-ü😀"),
            ],
            "until located, a clone next to the file's folder is assumed"
        );
        assert_eq!(back.projects[1].folder.as_deref(), Some("api"));
        assert_eq!(back.projects[2].folder.as_deref(), Some(""));
        assert_eq!(back.projects[0].shared_path.as_deref(), Some("gateway"));
        assert_eq!(write(&back, file), text, "read and written back unchanged");
        assert!(read("format = 4\nname = \"X\"\n[[projects]]\n", file).is_err());
    }

    /// R-WS-11: a project located somewhere else keeps the path the file lists.
    #[test]
    fn a_located_project_keeps_the_shared_path() {
        let file = Path::new("/src/platform/acme.tortuga-workspace.toml");
        let text = "format = 1\nname = \"Acme\"\n\n[[projects]]\npath = \"../users\"\n";
        let mut back = read(text, file).expect("read");
        back.projects[0].path = "/somewhere/else/users".into();
        assert_eq!(write(&back, file), text);
    }

    #[test]
    fn remotes_of_one_repository_match_however_written() {
        let same = [
            "git@github.com:acme/users-api.git",
            "https://github.com/acme/users-api",
            "https://bob@GitHub.com/acme/users-api.git/",
            "ssh://git@github.com:22/acme/users-api.git",
        ];
        for url in same {
            assert!(same_repository(same[0], url), "{url}");
        }
        assert!(!same_repository(same[0], "git@github.com:acme/orders.git"));
        assert!(!same_repository(
            same[0],
            "git@gitlab.com:acme/users-api.git"
        ));
        assert!(same_repository("/srv/git/ü😀.git", "/srv/git/ü😀"));
        assert_eq!(
            repository_name("git@github.com:acme/users-api.git"),
            "users-api"
        );
        assert_eq!(repository_name("https://example.test/ü😀/"), "ü😀");
        assert_eq!(repository_name("plain"), "plain");
    }

    #[test]
    fn folders_in_a_repository() {
        assert_eq!(
            folder_in(Path::new("/r"), Path::new("/r/a/b")).as_deref(),
            Some("a/b")
        );
        assert_eq!(
            folder_in(Path::new("/r"), Path::new("/r")).as_deref(),
            Some("")
        );
        assert_eq!(folder_in(Path::new("/r"), Path::new("/other")), None);
        let project = ProjectRef {
            path: "/r/a/b".into(),
            folder: Some("a/b".into()),
            ..ProjectRef::default()
        };
        assert_eq!(project.repository(), Some(PathBuf::from("/r")));
        assert_eq!(ProjectRef::new("/r").repository(), None);
    }

    /// R-WS-11: a project listed by remote is found in a clone this machine knows.
    #[test]
    fn projects_are_located_in_known_clones() {
        let dir = std::env::temp_dir().join(format!("tortuga-ws-locate-{}", std::process::id()));
        let clone = dir.join("code/users-api");
        std::fs::create_dir_all(clone.join("api")).expect("mkdir");
        std::fs::write(clone.join("api").join(MANIFEST_FILE), "").expect("manifest");
        let wanted = ProjectRef {
            path: "/guess/users-api/api".into(),
            remote: Some("https://github.com/acme/users-api.git".into()),
            folder: Some("api".into()),
            shared_path: None,
        };
        let gone = ProjectRef {
            path: dir.join("old/users-api"),
            remote: Some("git@github.com:acme/users-api.git".into()),
            folder: Some(String::new()),
            shared_path: None,
        };
        let known = ProjectRef {
            path: clone.join("docs"),
            remote: Some("git@github.com:acme/users-api.git".into()),
            folder: Some("docs".into()),
            shared_path: None,
        };
        let other = ProjectRef {
            path: dir.join("orders"),
            remote: Some("git@github.com:acme/orders.git".into()),
            folder: Some(String::new()),
            shared_path: None,
        };
        let storage = crate::storage::FsStorage;
        assert_eq!(
            locate(&storage, &wanted, &[other.clone(), gone.clone(), known]),
            Some(clone.join("api")),
            "a clone without the project is skipped"
        );
        assert_eq!(locate(&storage, &wanted, &[other, gone]), None);
        std::fs::remove_dir_all(dir).expect("clean up");
    }

    #[test]
    fn newer_or_broken_files_are_errors() {
        let file = Path::new("/w/x.tortuga-workspace.toml");
        assert!(read("format = 5\nname = \"X\"\n", file).is_err());
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
        assert_eq!(back.projects[0].path, projects[0].path);
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

    /// R-WS-12: servers only the workspace has are listed for a project once, whatever
    /// their case; ones the project has under any case are not.
    #[test]
    fn servers_only_the_workspace_has_are_missing_from_the_project() {
        let manifest = Manifest {
            servers: vec![crate::format::Server {
                id: "u".into(),
                name: "USERS".into(),
            }],
            ..Manifest::new("P")
        };
        let mut dev = staging();
        dev.servers = [
            ("Payments ü😀".to_owned(), "https://pay.dev.test".to_owned()),
            (" users ".to_owned(), "https://users.dev.test".to_owned()),
        ]
        .into();
        let mut prod = staging();
        prod.servers = [("payments ü😀".to_owned(), "https://pay.test".to_owned())].into();
        assert_eq!(
            servers_missing_from(&[staging(), dev, prod], &manifest),
            ["Payments ü😀"]
        );
        assert_eq!(servers_missing_from(&[], &manifest), Vec::<String>::new());
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
