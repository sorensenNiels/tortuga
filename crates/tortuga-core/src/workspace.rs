//! Workspaces: named sets of projects (R-WS-1), and the shareable workspace file (R-WS-5).
//!
//! A workspace itself is local data, kept by the app. "Save as file…" writes it as a
//! `<name>.tortuga-workspace.toml` that can be committed, e.g. to a "platform" repo:
//! each project by its path relative to the file, plus its git remote URL, so a new
//! teammate sees which repositories to clone. Only shared data goes into the file
//! (R-GIT-4): what is open or active stays on each machine.
//!
//! ```toml
//! format = 1
//! name = "Acme platform"
//!
//! [[projects]]
//! path = "../users-api"
//! remote = "git@github.com:acme/users-api.git"
//! ```

use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{Error, Result, slug};

/// File name ending of a shareable workspace file.
pub const WORKSPACE_FILE_SUFFIX: &str = ".tortuga-workspace.toml";

/// Format version of workspace files written by this build.
pub const WORKSPACE_FORMAT: u32 = 1;

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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceFile {
    pub name: String,
    /// Absolute paths, resolved against the file's directory.
    pub projects: Vec<ProjectRef>,
}

#[derive(Serialize, Deserialize)]
struct FileText {
    format: u32,
    name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    projects: Vec<FileProject>,
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

/// Writes the workspace file that will be saved at `file`. Deterministic: the same
/// workspace always gives the same text, so saving it again never changes the file.
pub fn write(name: &str, projects: &[ProjectRef], file: &Path) -> String {
    let dir = file.parent().unwrap_or(Path::new(""));
    let text = FileText {
        format: WORKSPACE_FORMAT,
        name: name.to_owned(),
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
        let text = write("Acme", &projects, file);
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
        assert_eq!(write("Acme", &projects, file), text, "deterministic");
    }

    #[test]
    fn newer_or_broken_files_are_errors() {
        let file = Path::new("/w/x.tortuga-workspace.toml");
        assert!(read("format = 2\nname = \"X\"\n", file).is_err());
        assert!(read("name = ", file).is_err());
        assert_eq!(
            read("format = 1\nname = \"X\"\n", file)
                .expect("empty")
                .projects,
            []
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
}
