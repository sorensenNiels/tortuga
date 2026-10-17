//! Finds the projects in a folder or repository: "Add existing projects" (R-WS-3).

use std::path::{Path, PathBuf};

use tortuga_core::collection::MANIFEST_FILE;

/// How deep below the chosen folder projects are looked for.
const MAX_DEPTH: usize = 8;
/// Gives up after this many folders, so picking e.g. a home folder stays quick.
const MAX_FOLDERS: usize = 20_000;
/// Never worth searching, even where no `.gitignore` says so.
const ALWAYS_SKIPPED: [&str; 2] = ["node_modules", "target"];

/// Every project directory below `dir` (or `dir` itself), sorted. Hidden and
/// git-ignored folders are skipped, and a project's own subfolders are not searched.
pub fn find_projects(dir: &Path) -> Result<Vec<PathBuf>, String> {
    if !dir.is_dir() {
        return Err(format!("{} is not a folder", dir.display()));
    }
    let repo = gix::discover(dir).ok();
    let mut search = Search {
        ignored: repo.as_ref().and_then(Ignored::new),
        found: Vec::new(),
        visited: 0,
    };
    search.folder(dir, 0);
    search.found.sort();
    Ok(search.found)
}

struct Search<'r> {
    ignored: Option<Ignored<'r>>,
    found: Vec<PathBuf>,
    visited: usize,
}

impl Search<'_> {
    fn folder(&mut self, dir: &Path, depth: usize) {
        self.visited += 1;
        if dir.join(MANIFEST_FILE).is_file() {
            self.found.push(dir.to_owned());
            return;
        }
        if depth >= MAX_DEPTH || self.visited >= MAX_FOLDERS {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return; // unreadable folders are skipped, not an error
        };
        let mut subfolders: Vec<PathBuf> = entries
            .flatten()
            // `file_type` does not follow symlinks, so links can't make the search loop.
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .filter(|e| {
                let name = e.file_name();
                let name = name.to_string_lossy();
                !name.starts_with('.') && !ALWAYS_SKIPPED.contains(&name.as_ref())
            })
            .map(|e| e.path())
            .collect();
        subfolders.sort();
        for sub in subfolders {
            if self.ignored.as_mut().is_some_and(|i| i.is_ignored(&sub)) {
                continue;
            }
            self.folder(&sub, depth + 1);
        }
    }
}

/// `.gitignore` rules of the repository the search started in.
struct Ignored<'r> {
    workdir: PathBuf,
    stack: gix::AttributeStack<'r>,
}

impl<'r> Ignored<'r> {
    fn new(repo: &'r gix::Repository) -> Option<Self> {
        let workdir = std::fs::canonicalize(repo.workdir()?).ok()?;
        let index = repo.index_or_empty().ok()?;
        let stack = repo
            .excludes(
                &index,
                None,
                gix::worktree::stack::state::ignore::Source::default(),
            )
            .ok()?;
        Some(Self { workdir, stack })
    }

    fn is_ignored(&mut self, dir: &Path) -> bool {
        let Ok(dir) = std::fs::canonicalize(dir) else {
            return false;
        };
        let Ok(relative) = dir.strip_prefix(&self.workdir) else {
            return false;
        };
        self.stack
            .at_path(relative, Some(gix::index::entry::Mode::DIR))
            .is_ok_and(|p| p.is_excluded())
    }
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use super::*;

    fn project(dir: &Path) {
        std::fs::create_dir_all(dir).expect("mkdir");
        std::fs::write(dir.join(MANIFEST_FILE), "format = 1\nname = \"P\"\n").expect("write");
    }

    #[test]
    fn finds_projects_but_not_in_ignored_or_hidden_folders() {
        let dir = std::env::temp_dir().join(format!("tortuga-discover-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        let has_git = Command::new("git")
            .args(["init", "-q"])
            .current_dir(&dir)
            .output()
            .is_ok_and(|o| o.status.success());
        std::fs::write(dir.join(".gitignore"), "build/\n").expect("write");
        project(&dir.join("services/users/api"));
        project(&dir.join("services/orders/api"));
        project(&dir.join("services/orders/api/nested")); // inside a project: not searched
        project(&dir.join("node_modules/pkg"));
        project(&dir.join(".cache/p"));
        project(&dir.join("build/copy"));

        let found = find_projects(&dir).expect("search");
        let mut expected = vec![
            dir.join("services/orders/api"),
            dir.join("services/users/api"),
        ];
        if !has_git {
            eprintln!("git not installed; .gitignore not checked");
            expected.insert(0, dir.join("build/copy"));
        }
        assert_eq!(found, expected);
        assert_eq!(
            find_projects(&dir.join("services/users/api")).expect("search"),
            [dir.join("services/users/api")],
            "a project folder itself"
        );
        std::fs::remove_dir_all(dir).expect("clean up");
    }
}
