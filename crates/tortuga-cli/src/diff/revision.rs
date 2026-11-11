//! A project's files at a git revision, read-only, as a `Storage` (R-CLI-7, D77), so
//! `Collection::open` reads a revision as it reads the working tree.

use std::io;
use std::path::{Path, PathBuf};

use tortuga_core::storage::{Entry, Storage};

/// The tree of a commit, with `root` the repository's working tree it stands for.
pub struct TreeStorage<'r> {
    tree: gix::Tree<'r>,
    root: PathBuf,
}

impl<'r> TreeStorage<'r> {
    pub fn new(tree: gix::Tree<'r>, root: &Path) -> Self {
        Self {
            tree,
            root: root.to_owned(),
        }
    }

    /// The entry at `path`, an absolute path in the working tree; `None` for the root.
    fn entry(&self, path: &Path) -> io::Result<Option<gix::object::tree::Entry<'r>>> {
        let relative = path.strip_prefix(&self.root).map_err(|_| not_found(path))?;
        if relative.as_os_str().is_empty() {
            return Ok(None);
        }
        self.tree
            .lookup_entry_by_path(relative)
            .map_err(io::Error::other)?
            .map(Some)
            .ok_or_else(|| not_found(path))
    }
}

fn not_found(path: &Path) -> io::Error {
    io::Error::new(
        io::ErrorKind::NotFound,
        format!("{} is not in this revision", path.display()),
    )
}

fn read_only() -> io::Error {
    io::Error::new(
        io::ErrorKind::ReadOnlyFilesystem,
        "a git revision is read-only",
    )
}

impl Storage for TreeStorage<'_> {
    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        let entry = self.entry(path)?.ok_or_else(|| not_found(path))?;
        let object = entry.object().map_err(io::Error::other)?;
        String::from_utf8(object.detach().data)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    fn list(&self, dir: &Path) -> io::Result<Vec<Entry>> {
        let tree = match self.entry(dir)? {
            None => self.tree.clone(),
            Some(entry) => entry
                .object()
                .and_then(gix::Object::try_into_tree)
                .map_err(io::Error::other)?,
        };
        let mut entries = tree
            .iter()
            .map(|e| {
                e.map(|e| Entry {
                    name: e.filename().to_string(),
                    is_dir: e.mode().is_tree(),
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(io::Error::other)?;
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(entries)
    }

    fn exists(&self, path: &Path) -> bool {
        self.entry(path).is_ok()
    }

    fn write(&self, _: &Path, _: &str) -> io::Result<()> {
        Err(read_only())
    }

    fn create_dir_all(&self, _: &Path) -> io::Result<()> {
        Err(read_only())
    }

    fn rename(&self, _: &Path, _: &Path) -> io::Result<()> {
        Err(read_only())
    }

    fn remove_file(&self, _: &Path) -> io::Result<()> {
        Err(read_only())
    }

    fn remove_dir_all(&self, _: &Path) -> io::Result<()> {
        Err(read_only())
    }
}

/// The tree of revision `rev` (`main`, `HEAD~2`, a commit id).
pub fn tree<'r>(repo: &'r gix::Repository, rev: &str) -> Result<gix::Tree<'r>, String> {
    let id = repo
        .rev_parse_single(rev)
        .map_err(|e| format!("{rev}: {e}"))?;
    id.object()
        .and_then(gix::Object::peel_to_tree)
        .map_err(|e| format!("{rev}: {e}"))
}

/// The tree of the best common ancestor of `a` and `b`, as a pull request compares.
pub fn merge_base<'r>(
    repo: &'r gix::Repository,
    a: &str,
    b: &str,
) -> Result<gix::Tree<'r>, String> {
    let id = |rev: &str| {
        repo.rev_parse_single(rev)
            .map(gix::Id::detach)
            .map_err(|e| format!("{rev}: {e}"))
    };
    let base = repo
        .merge_base(id(a)?, id(b)?)
        .map_err(|e| format!("{a}...{b}: {e}"))?
        .ok_or_else(|| format!("{a} and {b} have no commit in common"))?;
    tree(repo, &base.to_string())
}
