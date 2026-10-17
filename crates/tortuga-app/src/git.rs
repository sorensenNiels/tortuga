//! Reads git status with gitoxide, without a `git` executable (R-GIT-12, R-GIT-14, D8).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use gix::bstr::BString;
use tortuga_core::git::{FileStatus, GitStatus};

/// Git status of the collection at `root`; `Ok(None)` if it is not inside a git repository.
pub fn status(root: &Path) -> Result<Option<GitStatus>, String> {
    let Ok(repo) = gix::discover(root) else {
        return Ok(None);
    };
    let Some(workdir) = repo.workdir() else {
        return Ok(None); // a bare repository has no files to compare
    };
    let workdir = canonical(workdir);
    let root = canonical(root);
    let Ok(prefix) = root.strip_prefix(&workdir) else {
        return Ok(None);
    };

    let branch = match repo.head_name() {
        Ok(Some(name)) => Some(name.shorten().to_string()),
        _ => repo
            .head_id()
            .ok()
            .map(|id| id.to_hex_with_len(7).to_string()),
    };

    // Only look inside the collection, which keeps big repositories fast.
    let patterns: Vec<BString> = if prefix.as_os_str().is_empty() {
        Vec::new()
    } else {
        let prefix = gix::path::into_bstr(prefix).map_err(|e| e.to_string())?;
        vec![format!("{}/", gix::path::to_unix_separators_on_windows(prefix)).into()]
    };
    let items = repo
        .status(gix::progress::Discard)
        .map_err(|e| e.to_string())?
        .untracked_files(gix::status::UntrackedFiles::Files)
        .index_worktree_rewrites(None)
        .tree_index_track_renames(gix::status::tree_index::TrackRenames::Disabled)
        .into_iter(patterns)
        .map_err(|e| e.to_string())?;

    let mut files: BTreeMap<PathBuf, FileStatus> = BTreeMap::new();
    for item in items {
        let item = item.map_err(|e| e.to_string())?;
        let Some(status) = classify(&item) else {
            continue;
        };
        let Ok(repo_path) = gix::path::from_bstr(item.location()) else {
            continue; // not representable as a path on this OS
        };
        let Ok(path) = repo_path.strip_prefix(prefix) else {
            continue;
        };
        files
            .entry(path.to_owned())
            .and_modify(|current| *current = merge(*current, status))
            .or_insert(status);
    }
    Ok(Some(GitStatus {
        branch,
        files,
        repository: workdir.clone(),
        remote: remote_url(&repo),
    }))
}

/// URL of the repository's default remote (usually `origin`), without credentials.
fn remote_url(repo: &gix::Repository) -> Option<String> {
    let remote = repo
        .find_default_remote(gix::remote::Direction::Fetch)?
        .ok()?;
    let url = remote.url(gix::remote::Direction::Fetch)?.to_bstring();
    Some(tortuga_core::workspace::without_credentials(
        &url.to_string(),
    ))
}

/// Top folder of the git repository `dir` is in, if any.
pub fn repository_of(dir: &Path) -> Option<PathBuf> {
    gix::discover(dir)
        .ok()
        .and_then(|repo| repo.workdir().map(canonical))
}

/// Makes `dir` a new git repository (R-WS-7).
pub fn init(dir: &Path) -> Result<(), String> {
    gix::init(dir).map(drop).map_err(|e| {
        format!(
            "Could not create a git repository in {}: {e}",
            dir.display()
        )
    })
}

fn classify(item: &gix::status::Item) -> Option<FileStatus> {
    use gix::diff::index::ChangeRef;
    use gix::status::index_worktree::iter::Summary;

    Some(match item {
        gix::status::Item::TreeIndex(change) => match change {
            ChangeRef::Addition { .. } | ChangeRef::Rewrite { .. } => FileStatus::New,
            ChangeRef::Deletion { .. } => FileStatus::Deleted,
            ChangeRef::Modification { .. } => FileStatus::Modified,
        },
        gix::status::Item::IndexWorktree(change) => match change.summary()? {
            Summary::Conflict => FileStatus::Conflicted,
            Summary::Removed => FileStatus::Deleted,
            Summary::Added | Summary::IntentToAdd | Summary::Renamed | Summary::Copied => {
                FileStatus::New
            }
            Summary::Modified | Summary::TypeChange => FileStatus::Modified,
        },
    })
}

/// A file reported twice (staged and unstaged): a conflict wins, then what
/// happened since the last commit (new or deleted) over a later edit.
fn merge(a: FileStatus, b: FileStatus) -> FileStatus {
    use FileStatus::{Conflicted, Deleted, Modified, New};
    match (a, b) {
        (Conflicted, _) | (_, Conflicted) => Conflicted,
        (New, _) | (_, New) => New,
        (Deleted, _) | (_, Deleted) => Deleted,
        (Modified, Modified) => Modified,
    }
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_owned())
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use super::*;

    /// Runs `git` to set up a test repository; tests are skipped where git is not installed.
    fn git(dir: &Path, args: &[&str]) -> bool {
        Command::new("git")
            .args([
                "-c",
                "user.name=T",
                "-c",
                "user.email=t@t",
                "-c",
                "init.defaultBranch=main",
            ])
            .args(args)
            .current_dir(dir)
            .output()
            .is_ok_and(|o| o.status.success())
    }

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(path, text).expect("write");
    }

    #[test]
    fn reports_changes_inside_the_collection_only() {
        let dir = std::env::temp_dir().join(format!("tortuga-git-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        if !git(&dir, &["init", "-q"]) {
            eprintln!("git not installed; skipping");
            return;
        }
        let coll = dir.join("api");
        write(&coll.join("tortuga.toml"), "format = 1\nname = \"A\"\n");
        write(&coll.join("users/list.toml"), "url = \"/a\"\n");
        write(&coll.join("users/gone.toml"), "url = \"/b\"\n");
        write(&dir.join("README.md"), "outside the collection\n");
        assert!(git(&dir, &["add", "-A"]) && git(&dir, &["commit", "-q", "-m", "init"]));

        write(&coll.join("users/list.toml"), "url = \"/changed\"\n");
        std::fs::remove_file(coll.join("users/gone.toml")).expect("rm");
        write(&coll.join("users/new.toml"), "url = \"/c\"\n");
        write(
            &dir.join("README.md"),
            "changed, but not in the collection\n",
        );

        assert!(git(
            &dir,
            &[
                "remote",
                "add",
                "origin",
                "https://bob:tok@example.test/acme/api.git"
            ]
        ));
        let status = status(&coll).expect("status").expect("in a repo");
        assert_eq!(status.branch.as_deref(), Some("main"));
        assert_eq!(status.repository, canonical(&dir));
        assert_eq!(
            status.remote.as_deref(),
            Some("https://example.test/acme/api.git"),
            "credentials are not passed on"
        );
        assert_eq!(repository_of(&coll), Some(canonical(&dir)));
        let files: Vec<(&str, FileStatus)> = status
            .files
            .iter()
            .map(|(p, s)| (p.to_str().expect("utf8"), *s))
            .collect();
        assert_eq!(
            files,
            [
                ("users/gone.toml", FileStatus::Deleted),
                ("users/list.toml", FileStatus::Modified),
                ("users/new.toml", FileStatus::New),
            ]
        );

        assert_eq!(status_outside_repo(), None);
        std::fs::remove_dir_all(dir).expect("clean up");
    }

    #[test]
    fn reports_merge_conflicts() {
        let dir = std::env::temp_dir().join(format!("tortuga-git-conflict-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        if !git(&dir, &["init", "-q"]) {
            eprintln!("git not installed; skipping");
            return;
        }
        let file = dir.join("list.toml");
        write(&file, "url = \"/base\"\n");
        assert!(git(&dir, &["add", "-A"]) && git(&dir, &["commit", "-q", "-m", "base"]));
        assert!(git(&dir, &["checkout", "-q", "-b", "other"]));
        write(&file, "url = \"/theirs\"\n");
        assert!(git(&dir, &["commit", "-q", "-am", "theirs"]));
        assert!(git(&dir, &["checkout", "-q", "main"]));
        write(&file, "url = \"/mine\"\n");
        assert!(git(&dir, &["commit", "-q", "-am", "mine"]));
        assert!(!git(&dir, &["merge", "-q", "other"]), "the merge conflicts");

        let status = status(&dir).expect("status").expect("in a repo");
        assert_eq!(
            status.file(Path::new("list.toml")),
            Some(FileStatus::Conflicted)
        );
        std::fs::remove_dir_all(dir).expect("clean up");
    }

    fn status_outside_repo() -> Option<GitStatus> {
        // The temp dir root itself is (normally) not inside a repository.
        let dir = std::env::temp_dir().join(format!("tortuga-nogit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let result = status(&dir).expect("status");
        std::fs::remove_dir_all(dir).expect("clean up");
        result
    }

    #[test]
    fn init_makes_a_repository() {
        let dir = std::env::temp_dir().join(format!("tortuga-git-init-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        init(&dir).expect("init");
        assert_eq!(repository_of(&dir), Some(canonical(&dir)));
        std::fs::remove_dir_all(dir).expect("clean up");
    }

    #[test]
    fn staged_and_unstaged_states_merge() {
        use FileStatus::{Conflicted, Deleted, Modified, New};
        assert_eq!(merge(New, Modified), New);
        assert_eq!(merge(Modified, Conflicted), Conflicted);
        assert_eq!(merge(Deleted, Modified), Deleted);
    }
}
