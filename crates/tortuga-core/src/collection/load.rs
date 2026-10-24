//! Reading a collection's items, environments and workflows from disk (R-COL-1, R-GIT-1).

use std::path::{Path, PathBuf};

use super::{
    Broken, ENVIRONMENTS_DIR, FOLDER_FILE, Folder, Item, MANIFEST_FILE, RequestItem, file_stem,
    read,
};
use crate::format::{self, Environment, FolderMeta};
use crate::storage::Storage;
use crate::workflow::{WORKFLOWS_DIR, Workflow};
use crate::{Error, Result};

pub(super) fn load_dir(storage: &dyn Storage, root: &Path, rel: &Path) -> Result<Vec<Item>> {
    let dir = root.join(rel);
    let entries = storage.list(&dir).map_err(|source| Error::Io {
        path: dir.clone(),
        source,
    })?;
    let at_root = rel.as_os_str().is_empty();
    let mut items = Vec::new();
    for entry in entries {
        let name = entry.name.as_str();
        if name.starts_with('.')
            || name == FOLDER_FILE
            || (at_root
                && (name == MANIFEST_FILE || name == ENVIRONMENTS_DIR || name == WORKFLOWS_DIR))
        {
            continue;
        }
        let path = rel.join(name);
        if entry.is_dir {
            items.extend(load_folder(storage, root, path)?.map(Item::Folder));
        } else if let Some(stem) = name.strip_suffix(".toml") {
            let item = match read(storage, &root.join(&path)).and_then(|text| {
                format::parse_request(&text, stem).map_err(|message| Error::InvalidFile {
                    path: path.clone(),
                    message,
                })
            }) {
                Ok((request, seq)) => Item::Request(RequestItem { path, seq, request }),
                Err(Error::InvalidFile { message, .. }) => Item::Broken(Broken {
                    path,
                    error: message,
                }),
                Err(e) => Item::Broken(Broken {
                    path,
                    error: e.to_string(),
                }),
            };
            items.push(item);
        }
    }
    sort_items(&mut items);
    Ok(items)
}

/// A directory as a folder. `None` for a directory with neither `_folder.toml` nor
/// requests (at any depth), e.g. files to upload kept in the project (R-HTTP-15).
fn load_folder(storage: &dyn Storage, root: &Path, path: PathBuf) -> Result<Option<Folder>> {
    let meta_path = root.join(&path).join(FOLDER_FILE);
    let has_meta = storage.exists(&meta_path);
    let (meta, error) = if has_meta {
        match read(storage, &meta_path).map(|t| format::parse_folder(&t)) {
            Ok(Ok(meta)) => (meta, None),
            Ok(Err(message)) => (
                FolderMeta::default(),
                Some(format!("{FOLDER_FILE}: {message}")),
            ),
            Err(e) => (FolderMeta::default(), Some(e.to_string())),
        }
    } else {
        (FolderMeta::default(), None)
    };
    let items = load_dir(storage, root, &path)?;
    if !has_meta && items.is_empty() {
        return Ok(None);
    }
    let defaults = meta.defaults();
    let name = meta
        .name
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| file_stem(&path));
    Ok(Some(Folder {
        path,
        name,
        seq: meta.seq,
        items,
        defaults,
        error,
    }))
}

pub(super) fn load_workflows(
    storage: &dyn Storage,
    root: &Path,
) -> Result<(Vec<Workflow>, Vec<Broken>)> {
    let dir = root.join(WORKFLOWS_DIR);
    if !storage.exists(&dir) {
        return Ok((Vec::new(), Vec::new()));
    }
    let entries = storage.list(&dir).map_err(|source| Error::Io {
        path: dir.clone(),
        source,
    })?;
    let (mut workflows, mut broken) = (Vec::new(), Vec::new());
    for entry in entries
        .into_iter()
        .filter(|e| !e.is_dir && !e.name.starts_with('.'))
    {
        let Some(id) = entry.name.strip_suffix(".toml") else {
            continue;
        };
        let rel = Path::new(WORKFLOWS_DIR).join(&entry.name);
        match read(storage, &root.join(&rel)).map(|t| format::parse_workflow(&t, id)) {
            Ok(Ok(w)) => workflows.push(w),
            Ok(Err(error)) => broken.push(Broken { path: rel, error }),
            Err(e) => broken.push(Broken {
                path: rel,
                error: e.to_string(),
            }),
        }
    }
    workflows.sort_by_key(|w| (w.name.to_lowercase(), w.id.clone()));
    Ok((workflows, broken))
}

pub(super) fn load_environments(
    storage: &dyn Storage,
    root: &Path,
) -> Result<(Vec<Environment>, Vec<Broken>)> {
    let dir = root.join(ENVIRONMENTS_DIR);
    if !storage.exists(&dir) {
        return Ok((Vec::new(), Vec::new()));
    }
    let entries = storage.list(&dir).map_err(|source| Error::Io {
        path: dir.clone(),
        source,
    })?;
    let (mut envs, mut broken) = (Vec::new(), Vec::new());
    for entry in entries
        .into_iter()
        .filter(|e| !e.is_dir && !e.name.starts_with('.'))
    {
        let Some(id) = entry.name.strip_suffix(".toml") else {
            continue;
        };
        let rel = Path::new(ENVIRONMENTS_DIR).join(&entry.name);
        match read(storage, &root.join(&rel)).map(|t| format::parse_environment(&t, id)) {
            Ok(Ok(env)) => envs.push(env),
            Ok(Err(error)) => broken.push(Broken { path: rel, error }),
            Err(e) => broken.push(Broken {
                path: rel,
                error: e.to_string(),
            }),
        }
    }
    envs.sort_by(|a, b| {
        (a.seq.unwrap_or(i64::MAX), a.name.to_lowercase())
            .cmp(&(b.seq.unwrap_or(i64::MAX), b.name.to_lowercase()))
    });
    Ok((envs, broken))
}

/// By position, then name; items without a position go last.
fn sort_items(items: &mut [Item]) {
    items.sort_by_cached_key(|i| (i.seq().unwrap_or(i64::MAX), i.name().to_lowercase()));
}
