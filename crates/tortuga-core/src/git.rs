//! Git status of a collection, as shown in the tree (R-GIT-12).
//!
//! Only plain data lives here, so the engine stays free of git libraries and
//! compiles for wasm. The desktop app reads the repository itself (R-GIT-14)
//! and fills a [`GitStatus`].

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::collection::{ENVIRONMENTS_DIR, FOLDER_FILE, MANIFEST_FILE};

/// What git says about one file, compared with the last commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FileStatus {
    /// Changed since the last commit (staged or not).
    Modified,
    /// Not in the last commit: untracked or newly added.
    New,
    /// In the last commit, gone now (deleted on disk or staged for deletion).
    Deleted,
    /// Unresolved merge conflict.
    Conflicted,
}

impl FileStatus {
    /// Which status wins when a folder holds files in several states.
    fn weight(self) -> u8 {
        match self {
            Self::Conflicted => 3,
            Self::Modified => 2,
            Self::New | Self::Deleted => 1,
        }
    }
}

/// The git state of one collection.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GitStatus {
    /// Current branch, or a short commit id when detached; `None` before the first commit.
    pub branch: Option<String>,
    /// Changed files, by path relative to the collection root.
    pub files: BTreeMap<PathBuf, FileStatus>,
}

impl GitStatus {
    /// Status of a request file (or `_folder.toml`).
    pub fn file(&self, path: &Path) -> Option<FileStatus> {
        self.files.get(path).copied()
    }

    /// Status shown on a folder: the most important status of anything inside it.
    pub fn folder(&self, folder: &Path) -> Option<FileStatus> {
        self.files
            .iter()
            .filter(|(path, _)| path.starts_with(folder) && path.as_path() != folder)
            .map(|(_, status)| *status)
            .max_by_key(|s| s.weight())
    }

    /// Request files deleted directly inside `folder` ("" for the top level), so the
    /// tree can still show them (D10).
    pub fn deleted_requests(&self, folder: &Path) -> Vec<PathBuf> {
        self.files
            .iter()
            .filter(|(path, status)| {
                **status == FileStatus::Deleted
                    && path.parent() == Some(folder)
                    && is_request_file(path)
            })
            .map(|(path, _)| path.clone())
            .collect()
    }

    /// True if anything in the collection differs from the last commit.
    pub fn is_dirty(&self) -> bool {
        !self.files.is_empty()
    }
}

/// A `*.toml` request file (not the manifest, folder metadata or an environment).
fn is_request_file(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    let in_environments = path
        .components()
        .next()
        .is_some_and(|c| c.as_os_str() == ENVIRONMENTS_DIR);
    path.extension().is_some_and(|e| e == "toml")
        && name != FOLDER_FILE
        && name != MANIFEST_FILE
        && !in_environments
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status() -> GitStatus {
        GitStatus {
            branch: Some("main".into()),
            files: [
                ("users/list.toml", FileStatus::Modified),
                ("users/gone.toml", FileStatus::Deleted),
                ("users/admin/new.toml", FileStatus::New),
                ("orders/a.toml", FileStatus::Conflicted),
                ("orders/b.toml", FileStatus::Modified),
                ("environments/dev.toml", FileStatus::Deleted),
                ("old.toml", FileStatus::Deleted),
            ]
            .into_iter()
            .map(|(p, s)| (PathBuf::from(p), s))
            .collect(),
        }
    }

    #[test]
    fn folders_show_the_most_important_status_inside() {
        let s = status();
        assert_eq!(s.folder(Path::new("users")), Some(FileStatus::Modified));
        assert_eq!(s.folder(Path::new("users/admin")), Some(FileStatus::New));
        assert_eq!(s.folder(Path::new("orders")), Some(FileStatus::Conflicted));
        assert_eq!(s.folder(Path::new("empty")), None);
        assert_eq!(
            s.file(Path::new("users/list.toml")),
            Some(FileStatus::Modified)
        );
    }

    #[test]
    fn deleted_requests_are_listed_per_folder() {
        let s = status();
        assert_eq!(
            s.deleted_requests(Path::new("users")),
            [PathBuf::from("users/gone.toml")]
        );
        assert_eq!(
            s.deleted_requests(Path::new("")),
            [PathBuf::from("old.toml")]
        );
        assert_eq!(
            s.deleted_requests(Path::new("environments")),
            Vec::<PathBuf>::new(),
            "environment files are not requests"
        );
    }
}
