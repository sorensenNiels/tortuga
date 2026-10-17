//! File access for collections, behind a trait so other targets (e.g. web)
//! can provide their own. Paths are absolute.

use std::io;
use std::path::Path;

/// One directory entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub is_dir: bool,
}

pub trait Storage {
    fn read_to_string(&self, path: &Path) -> io::Result<String>;
    /// Writes the whole file, creating parent directories. Must not leave a
    /// half-written file behind if it fails.
    fn write(&self, path: &Path, contents: &str) -> io::Result<()>;
    /// Entries of a directory, sorted by name.
    fn list(&self, dir: &Path) -> io::Result<Vec<Entry>>;
    fn exists(&self, path: &Path) -> bool;
    fn create_dir_all(&self, path: &Path) -> io::Result<()>;
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
    fn remove_file(&self, path: &Path) -> io::Result<()>;
    fn remove_dir_all(&self, path: &Path) -> io::Result<()>;
}

/// The local file system.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone, Copy, Default)]
pub struct FsStorage;

#[cfg(not(target_arch = "wasm32"))]
impl Storage for FsStorage {
    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        std::fs::read_to_string(path)
    }

    fn write(&self, path: &Path, contents: &str) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        // Write to a temporary sibling, then rename over the target (atomic on
        // all supported OSes), so a crash never leaves a truncated file.
        let tmp = temp_sibling(path);
        std::fs::write(&tmp, contents)?;
        std::fs::rename(&tmp, path).inspect_err(|_| {
            let _ = std::fs::remove_file(&tmp);
        })
    }

    fn list(&self, dir: &Path) -> io::Result<Vec<Entry>> {
        let mut entries = Vec::new();
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            entries.push(Entry {
                name: entry.file_name().to_string_lossy().into_owned(),
                is_dir: entry.file_type()?.is_dir(),
            });
        }
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(entries)
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        std::fs::create_dir_all(path)
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        if let Some(dir) = to.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::rename(from, to)
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        std::fs::remove_file(path)
    }

    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        std::fs::remove_dir_all(path)
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn temp_sibling(path: &Path) -> std::path::PathBuf {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    path.with_file_name(format!(".{name}.tortuga-tmp"))
}

/// Storage for targets without a file system (the web build, for now).
#[derive(Debug, Clone, Copy, Default)]
pub struct UnsupportedStorage;

impl Storage for UnsupportedStorage {
    fn read_to_string(&self, _: &Path) -> io::Result<String> {
        Err(unsupported())
    }
    fn write(&self, _: &Path, _: &str) -> io::Result<()> {
        Err(unsupported())
    }
    fn list(&self, _: &Path) -> io::Result<Vec<Entry>> {
        Err(unsupported())
    }
    fn exists(&self, _: &Path) -> bool {
        false
    }
    fn create_dir_all(&self, _: &Path) -> io::Result<()> {
        Err(unsupported())
    }
    fn rename(&self, _: &Path, _: &Path) -> io::Result<()> {
        Err(unsupported())
    }
    fn remove_file(&self, _: &Path) -> io::Result<()> {
        Err(unsupported())
    }
    fn remove_dir_all(&self, _: &Path) -> io::Result<()> {
        Err(unsupported())
    }
}

fn unsupported() -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        "projects are not available in this build yet",
    )
}
