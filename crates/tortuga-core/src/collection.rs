//! Collections on disk (R-COL-1..6, R-GIT-1..8).
//!
//! A collection is a directory with a `tortuga.toml` manifest. Folders are
//! directories, requests are one `.toml` file each, environments live in
//! `environments/`. Every mutating method writes to storage; callers then
//! re-open the collection to see the result (simple and always in sync with disk).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::format::{self, Environment, FORMAT_VERSION, FolderDefaults, FolderMeta, Manifest};
use crate::model::Request;
use crate::slug;
use crate::storage::Storage;
use crate::workflow::{WORKFLOWS_DIR, Workflow};
use crate::{Error, Result};

pub const MANIFEST_FILE: &str = "tortuga.toml";
pub const FOLDER_FILE: &str = "_folder.toml";
pub const ENVIRONMENTS_DIR: &str = "environments";

/// Gap between positions, so an item can usually be placed between two
/// others by changing only its own file (R-GIT-6).
const SEQ_STEP: i64 = 100;

#[derive(Debug, Clone, PartialEq)]
pub struct Collection {
    /// Absolute path of the collection directory.
    pub root: PathBuf,
    pub manifest: Manifest,
    /// Written by a newer Tortuga: shown, but never modified (R-GIT-7).
    pub read_only: bool,
    pub items: Vec<Item>,
    pub environments: Vec<Environment>,
    /// Environment files that could not be read (R-GIT-8).
    pub broken_environments: Vec<Broken>,
    /// Workflows in `workflows/`, by name (R-FLOW-1).
    pub workflows: Vec<Workflow>,
    /// Workflow files that could not be read.
    pub broken_workflows: Vec<Broken>,
}

/// An entry in the collection tree. Paths are relative to the collection root.
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Folder(Folder),
    Request(RequestItem),
    /// A request file that could not be read; the rest still loads (R-GIT-8).
    Broken(Broken),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Folder {
    pub path: PathBuf,
    pub name: String,
    pub seq: Option<i64>,
    pub items: Vec<Item>,
    /// Headers, auth and variables its requests inherit (R-COL-11).
    pub defaults: FolderDefaults,
    /// Set when `_folder.toml` could not be read.
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RequestItem {
    pub path: PathBuf,
    pub seq: Option<i64>,
    pub request: Request,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Broken {
    pub path: PathBuf,
    pub error: String,
}

impl Item {
    pub fn path(&self) -> &Path {
        match self {
            Self::Folder(f) => &f.path,
            Self::Request(r) => &r.path,
            Self::Broken(b) => &b.path,
        }
    }

    pub fn name(&self) -> String {
        match self {
            Self::Folder(f) => f.name.clone(),
            Self::Request(r) => r.request.name.clone(),
            Self::Broken(b) => file_stem(&b.path),
        }
    }

    pub fn seq(&self) -> Option<i64> {
        match self {
            Self::Folder(f) => f.seq,
            Self::Request(r) => r.seq,
            Self::Broken(_) => None,
        }
    }
}

/// A request or folder to add with [`Collection::add_items`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NewItem {
    Request(Box<Request>),
    Folder { name: String, items: Vec<NewItem> },
}

/// Direction for [`Collection::move_by`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
}

impl Collection {
    /// Turns `root` (existing or not) into a new, empty collection.
    pub fn create(storage: &dyn Storage, root: &Path, name: &str) -> Result<Self> {
        let manifest_path = root.join(MANIFEST_FILE);
        if storage.exists(&manifest_path) {
            return Err(Error::AlreadyACollection(root.to_owned()));
        }
        let manifest = Manifest::new(name.trim());
        write(storage, &manifest_path, &format::write_manifest(&manifest))?;
        Self::open(storage, root)
    }

    pub fn open(storage: &dyn Storage, root: &Path) -> Result<Self> {
        let manifest_path = root.join(MANIFEST_FILE);
        if !storage.exists(&manifest_path) {
            return Err(Error::NotACollection(root.to_owned()));
        }
        let manifest =
            format::parse_manifest(&read(storage, &manifest_path)?).map_err(|message| {
                Error::InvalidFile {
                    path: manifest_path,
                    message,
                }
            })?;
        let read_only = manifest.format > FORMAT_VERSION;
        let items = load_dir(storage, root, Path::new(""))?;
        let (environments, broken_environments) = load_environments(storage, root)?;
        let (workflows, broken_workflows) = load_workflows(storage, root)?;
        Ok(Self {
            root: root.to_owned(),
            manifest,
            read_only,
            items,
            environments,
            broken_environments,
            workflows,
            broken_workflows,
        })
    }

    pub fn name(&self) -> &str {
        &self.manifest.name
    }

    /// Finds an item anywhere in the tree.
    pub fn find(&self, path: &Path) -> Option<&Item> {
        find_in(&self.items, path)
    }

    pub fn request(&self, path: &Path) -> Option<&RequestItem> {
        match self.find(path)? {
            Item::Request(r) => Some(r),
            _ => None,
        }
    }

    /// Items directly inside `parent` (`""` for the root).
    pub fn children(&self, parent: &Path) -> &[Item] {
        if parent.as_os_str().is_empty() {
            return &self.items;
        }
        match self.find(parent) {
            Some(Item::Folder(f)) => &f.items,
            _ => &[],
        }
    }

    /// All folders, depth-first, for "move to" pickers.
    pub fn folders(&self) -> Vec<&Folder> {
        fn walk<'a>(items: &'a [Item], out: &mut Vec<&'a Folder>) {
            for item in items {
                if let Item::Folder(f) = item {
                    out.push(f);
                    walk(&f.items, out);
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.items, &mut out);
        out
    }

    /// Adds a request at the end of `parent`. Returns its path.
    pub fn new_request(
        &self,
        storage: &dyn Storage,
        parent: &Path,
        request: &Request,
    ) -> Result<PathBuf> {
        self.ensure_writable()?;
        let path = self.free_file_path(storage, parent, &request.name, None);
        let seq = self.next_seq(parent);
        write(
            storage,
            &self.root.join(&path),
            &format::write_request(request, Some(seq)),
        )?;
        Ok(path)
    }

    /// Adds requests and folders of requests at the end of `parent` in one go, e.g. from
    /// an import (R-COL-15). The collection must be opened again to see them.
    pub fn add_items(&self, storage: &dyn Storage, parent: &Path, items: &[NewItem]) -> Result<()> {
        self.ensure_writable()?;
        self.write_new_items(storage, parent, self.next_seq(parent), items)
    }

    fn write_new_items(
        &self,
        storage: &dyn Storage,
        parent: &Path,
        first_seq: i64,
        items: &[NewItem],
    ) -> Result<()> {
        let mut seq = first_seq;
        for item in items {
            match item {
                NewItem::Request(request) => {
                    let path = self.free_file_path(storage, parent, &request.name, None);
                    write(
                        storage,
                        &self.root.join(&path),
                        &format::write_request(request, Some(seq)),
                    )?;
                }
                NewItem::Folder { name, items } => {
                    let path = self.free_dir_path(storage, parent, name, None);
                    let meta = FolderMeta::new(
                        name.trim().to_owned(),
                        Some(seq),
                        &FolderDefaults::default(),
                    );
                    write(
                        storage,
                        &self.root.join(&path).join(FOLDER_FILE),
                        &format::write_folder(&meta),
                    )?;
                    self.write_new_items(storage, &path, SEQ_STEP, items)?;
                }
            }
            seq += SEQ_STEP;
        }
        Ok(())
    }

    /// Adds a folder at the end of `parent`. Returns its path.
    pub fn new_folder(&self, storage: &dyn Storage, parent: &Path, name: &str) -> Result<PathBuf> {
        self.ensure_writable()?;
        let path = self.free_dir_path(storage, parent, name, None);
        let meta = FolderMeta::new(
            name.trim().to_owned(),
            Some(self.next_seq(parent)),
            &FolderDefaults::default(),
        );
        write(
            storage,
            &self.root.join(&path).join(FOLDER_FILE),
            &format::write_folder(&meta),
        )?;
        Ok(path)
    }

    /// Saves an edited request. Renaming it renames the file (R-GIT-5); an
    /// unchanged request leaves the file untouched (R-GIT-3). Returns the new path.
    pub fn save_request(
        &self,
        storage: &dyn Storage,
        path: &Path,
        request: &Request,
    ) -> Result<PathBuf> {
        self.ensure_writable()?;
        let seq = self.request(path).and_then(|r| r.seq);
        let parent = parent_of(path);
        let new_path = if slug::slugify(&request.name) == file_stem(path) {
            path.to_owned()
        } else {
            self.free_file_path(storage, &parent, &request.name, Some(path))
        };
        write_if_changed(
            storage,
            &self.root.join(&new_path),
            &format::write_request(request, seq),
        )?;
        if new_path != path {
            remove_file(storage, &self.root.join(path))?;
        }
        Ok(new_path)
    }

    /// Renames a folder (directory and display name). Returns the new path.
    pub fn rename_folder(&self, storage: &dyn Storage, path: &Path, name: &str) -> Result<PathBuf> {
        self.ensure_writable()?;
        let Some(Item::Folder(folder)) = self.find(path) else {
            return Err(Error::NotFound(path.to_owned()));
        };
        let parent = parent_of(path);
        let new_path = if slug::slugify(name) == file_stem(path) {
            path.to_owned()
        } else {
            let new_path = self.free_dir_path(storage, &parent, name, Some(path));
            rename(storage, &self.root.join(path), &self.root.join(&new_path))?;
            new_path
        };
        let meta = FolderMeta::new(name.trim().to_owned(), folder.seq, &folder.defaults);
        write_if_changed(
            storage,
            &self.root.join(&new_path).join(FOLDER_FILE),
            &format::write_folder(&meta),
        )?;
        Ok(new_path)
    }

    /// Saves the headers, auth and variables a folder's requests inherit (R-COL-11).
    pub fn save_folder_defaults(
        &self,
        storage: &dyn Storage,
        path: &Path,
        defaults: &FolderDefaults,
    ) -> Result<()> {
        self.ensure_writable()?;
        let Some(Item::Folder(folder)) = self.find(path) else {
            return Err(Error::NotFound(path.to_owned()));
        };
        let meta = FolderMeta::new(folder.name.clone(), folder.seq, defaults);
        write_if_changed(
            storage,
            &self.root.join(path).join(FOLDER_FILE),
            &format::write_folder(&meta),
        )
    }

    pub fn workflow(&self, id: &str) -> Option<&Workflow> {
        self.workflows.iter().find(|w| w.id == id)
    }

    /// Adds an empty workflow called `name` (R-FLOW-2). Returns its id.
    pub fn new_workflow(&self, storage: &dyn Storage, name: &str) -> Result<String> {
        self.ensure_writable()?;
        let id = slug::unique(&slug::slugify(name), |c| {
            self.workflows.iter().any(|w| w.id == c)
                || storage.exists(&self.root.join(WORKFLOWS_DIR).join(format!("{c}.toml")))
        });
        let workflow = Workflow {
            id: id.clone(),
            name: name.trim().to_owned(),
            ..Workflow::default()
        };
        self.save_workflow(storage, &workflow)?;
        Ok(id)
    }

    /// Writes a workflow to `workflows/<id>.toml` (unchanged content leaves the file alone).
    pub fn save_workflow(&self, storage: &dyn Storage, workflow: &Workflow) -> Result<()> {
        self.ensure_writable()?;
        if workflow.id.is_empty() || workflow.id.contains(['/', '\\', '.']) {
            return Err(Error::InvalidOperation(format!(
                "not a workflow id: {}",
                workflow.id
            )));
        }
        let path = self
            .root
            .join(WORKFLOWS_DIR)
            .join(format!("{}.toml", workflow.id));
        write_if_changed(storage, &path, &format::write_workflow(workflow))
    }

    pub fn delete_workflow(&self, storage: &dyn Storage, id: &str) -> Result<()> {
        self.ensure_writable()?;
        let path = self.root.join(WORKFLOWS_DIR).join(format!("{id}.toml"));
        remove_file(storage, &path)
    }

    /// Points workflow steps at a request or folder that moved from `old` to `new`
    /// (R-FLOW-1).
    pub fn retarget_workflows(&self, storage: &dyn Storage, old: &Path, new: &Path) -> Result<()> {
        for workflow in &self.workflows {
            let mut moved = workflow.clone();
            if moved.retarget(old, new) {
                self.save_workflow(storage, &moved)?;
            }
        }
        Ok(())
    }

    /// The folders `path` is in, outermost first (R-COL-11).
    pub fn folders_of(&self, path: &Path) -> Vec<&Folder> {
        let mut out = Vec::new();
        let mut items = &self.items;
        for dir in path
            .parent()
            .into_iter()
            .flat_map(Path::ancestors)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
        {
            if dir.as_os_str().is_empty() {
                continue;
            }
            let Some(folder) = items.iter().find_map(|i| match i {
                Item::Folder(f) if f.path == dir => Some(f),
                _ => None,
            }) else {
                break;
            };
            out.push(folder);
            items = &folder.items;
        }
        out
    }

    /// Copies a request or folder (with everything in it) right after the original.
    pub fn duplicate(&self, storage: &dyn Storage, path: &Path) -> Result<PathBuf> {
        self.ensure_writable()?;
        let item = self
            .find(path)
            .ok_or_else(|| Error::NotFound(path.to_owned()))?;
        let parent = parent_of(path);
        let seq = self.seq_after(&parent, path);
        let name = format!("{} copy", item.name());
        match item {
            Item::Request(r) => {
                let copy = Request {
                    name,
                    ..r.request.clone()
                };
                let new_path = self.free_file_path(storage, &parent, &copy.name, None);
                write(
                    storage,
                    &self.root.join(&new_path),
                    &format::write_request(&copy, seq),
                )?;
                Ok(new_path)
            }
            Item::Folder(folder) => {
                let new_path = self.free_dir_path(storage, &parent, &name, None);
                copy_dir(storage, &self.root.join(path), &self.root.join(&new_path))?;
                let meta = FolderMeta::new(name, seq, &folder.defaults);
                write(
                    storage,
                    &self.root.join(&new_path).join(FOLDER_FILE),
                    &format::write_folder(&meta),
                )?;
                Ok(new_path)
            }
            Item::Broken(_) => Err(Error::InvalidOperation(
                "A file that cannot be read cannot be duplicated.".into(),
            )),
        }
    }

    /// Deletes a request, a broken file, or a folder with everything in it.
    pub fn delete(&self, storage: &dyn Storage, path: &Path) -> Result<()> {
        self.ensure_writable()?;
        match self
            .find(path)
            .ok_or_else(|| Error::NotFound(path.to_owned()))?
        {
            Item::Folder(_) => storage
                .remove_dir_all(&self.root.join(path))
                .map_err(|source| Error::Io {
                    path: self.root.join(path),
                    source,
                }),
            _ => remove_file(storage, &self.root.join(path)),
        }
    }

    /// Moves an item into another folder (`""` = collection root), at the end.
    pub fn move_to(
        &self,
        storage: &dyn Storage,
        path: &Path,
        new_parent: &Path,
    ) -> Result<PathBuf> {
        self.ensure_writable()?;
        let item = self
            .find(path)
            .ok_or_else(|| Error::NotFound(path.to_owned()))?;
        if new_parent.starts_with(path) {
            return Err(Error::InvalidOperation(
                "A folder cannot be moved into itself.".into(),
            ));
        }
        if parent_of(path) == new_parent {
            return Ok(path.to_owned());
        }
        if !new_parent.as_os_str().is_empty()
            && !matches!(self.find(new_parent), Some(Item::Folder(_)))
        {
            return Err(Error::NotFound(new_parent.to_owned()));
        }
        let seq = self.next_seq(new_parent);
        let new_path = match item {
            Item::Folder(_) => self.free_dir_path(storage, new_parent, &file_stem(path), None),
            _ => self.free_file_path(storage, new_parent, &file_stem(path), None),
        };
        rename(storage, &self.root.join(path), &self.root.join(&new_path))?;
        self.write_seq(storage, &new_path, item, seq)?;
        Ok(new_path)
    }

    /// Puts an item in `parent` just before `before` (`None`: at the end), moving it
    /// there first if it is in another folder: what a drag and drop does (R-COL-12).
    /// Usually writes one file; renumbers the folder's items only when there is no room
    /// between the neighbours (R-GIT-6). Returns the item's new path.
    pub fn place(
        &self,
        storage: &dyn Storage,
        path: &Path,
        parent: &Path,
        before: Option<&Path>,
    ) -> Result<PathBuf> {
        self.ensure_writable()?;
        let item = self
            .find(path)
            .ok_or_else(|| Error::NotFound(path.to_owned()))?;
        if parent.starts_with(path) {
            return Err(Error::InvalidOperation(
                "A folder cannot be moved into itself.".into(),
            ));
        }
        if !parent.as_os_str().is_empty() && !matches!(self.find(parent), Some(Item::Folder(_))) {
            return Err(Error::NotFound(parent.to_owned()));
        }
        if before == Some(path) {
            return Ok(path.to_owned());
        }
        let siblings: Vec<&Item> = self
            .children(parent)
            .iter()
            .filter(|i| !matches!(i, Item::Broken(_)) && i.path() != path)
            .collect();
        let index = match before {
            None => siblings.len(),
            Some(b) => siblings
                .iter()
                .position(|i| i.path() == b)
                .ok_or_else(|| Error::NotFound(b.to_owned()))?,
        };

        let new_path = if parent_of(path) == parent {
            path.to_owned()
        } else {
            let new_path = match item {
                Item::Folder(_) => self.free_dir_path(storage, parent, &file_stem(path), None),
                _ => self.free_file_path(storage, parent, &file_stem(path), None),
            };
            rename(storage, &self.root.join(path), &self.root.join(&new_path))?;
            new_path
        };

        let seqs: Vec<Option<i64>> = siblings.iter().map(|i| i.seq()).collect();
        let ordered = seqs.iter().all(Option::is_some) && seqs.windows(2).all(|w| w[0] < w[1]);
        let prev = index.checked_sub(1).and_then(|i| seqs[i]);
        let next = seqs.get(index).copied().flatten();
        let free = match (prev, next) {
            _ if !ordered => None,
            (None, None) => Some(SEQ_STEP),
            (Some(p), None) => Some(p + SEQ_STEP),
            (None, Some(n)) => Some(n - SEQ_STEP),
            (Some(p), Some(n)) if n - p > 1 => Some(p + (n - p) / 2),
            _ => None,
        };
        if let Some(seq) = free {
            self.write_seq(storage, &new_path, item, seq)?;
        } else {
            let mut order: Vec<(&Path, &Item)> = siblings.iter().map(|i| (i.path(), *i)).collect();
            order.insert(index, (&new_path, item));
            for (n, (at, it)) in (1..).zip(order) {
                self.write_seq(storage, at, it, n * SEQ_STEP)?;
            }
        }
        Ok(new_path)
    }

    /// Moves an item one place up or down among its siblings (R-COL-3).
    /// Usually rewrites two files; renumbers the siblings only if positions are missing or equal.
    pub fn move_by(&self, storage: &dyn Storage, path: &Path, direction: Direction) -> Result<()> {
        self.ensure_writable()?;
        let parent = parent_of(path);
        let siblings: Vec<&Item> = self
            .children(&parent)
            .iter()
            .filter(|i| !matches!(i, Item::Broken(_)))
            .collect();
        let Some(index) = siblings.iter().position(|i| i.path() == path) else {
            return Err(Error::NotFound(path.to_owned()));
        };
        let other = match direction {
            Direction::Up if index > 0 => index - 1,
            Direction::Down if index + 1 < siblings.len() => index + 1,
            _ => return Ok(()),
        };
        let seqs: Vec<Option<i64>> = siblings.iter().map(|i| i.seq()).collect();
        let distinct = seqs.iter().all(Option::is_some) && seqs.windows(2).all(|w| w[0] < w[1]);
        if distinct {
            let (a, b) = (siblings[index], siblings[other]);
            self.write_seq(storage, a.path(), a, b.seq().unwrap_or_default())?;
            self.write_seq(storage, b.path(), b, a.seq().unwrap_or_default())?;
        } else {
            let mut order: Vec<&Item> = siblings.clone();
            order.swap(index, other);
            for (n, item) in (1..).zip(order) {
                self.write_seq(storage, item.path(), item, n * SEQ_STEP)?;
            }
        }
        Ok(())
    }

    /// Writes the manifest and environment files (R-VAR-2, R-SRV-1/2). Files
    /// whose content did not change are left untouched; environments no
    /// longer in `environments` are deleted.
    pub fn save_config(
        &self,
        storage: &dyn Storage,
        manifest: &Manifest,
        environments: &[Environment],
    ) -> Result<()> {
        self.ensure_writable()?;
        write_if_changed(
            storage,
            &self.root.join(MANIFEST_FILE),
            &format::write_manifest(manifest),
        )?;
        let dir = self.root.join(ENVIRONMENTS_DIR);
        for env in environments {
            write_if_changed(
                storage,
                &dir.join(format!("{}.toml", env.id)),
                &format::write_environment(env),
            )?;
        }
        for old in &self.environments {
            if !environments.iter().any(|e| e.id == old.id) {
                remove_file(storage, &dir.join(format!("{}.toml", old.id)))?;
            }
        }
        Ok(())
    }

    /// A file stem for a new environment called `name`, unique among `existing`.
    pub fn new_environment_id(name: &str, existing: &[Environment]) -> String {
        slug::unique(&slug::slugify(name), |c| existing.iter().any(|e| e.id == c))
    }

    /// A stable id for a new server called `name`, unique among `existing`.
    pub fn new_server_id(name: &str, existing: &[format::Server]) -> String {
        slug::unique(&slug::slugify(name), |c| existing.iter().any(|s| s.id == c))
    }

    pub fn environment(&self, id: &str) -> Option<&Environment> {
        self.environments.iter().find(|e| e.id == id)
    }

    // ------------------------------------------------------------ helpers

    fn ensure_writable(&self) -> Result<()> {
        if self.read_only {
            Err(Error::ReadOnly {
                found: self.manifest.format,
                supported: FORMAT_VERSION,
            })
        } else {
            Ok(())
        }
    }

    fn next_seq(&self, parent: &Path) -> i64 {
        self.children(parent)
            .iter()
            .filter_map(Item::seq)
            .max()
            .unwrap_or(0)
            + SEQ_STEP
    }

    /// A position between `path` and the item after it, if there is room.
    fn seq_after(&self, parent: &Path, path: &Path) -> Option<i64> {
        let siblings = self.children(parent);
        let index = siblings.iter().position(|i| i.path() == path)?;
        let here = siblings[index].seq()?;
        match siblings.get(index + 1).and_then(Item::seq) {
            Some(next) if next - here > 1 => Some(here + (next - here) / 2),
            Some(_) => None,
            None => Some(here + SEQ_STEP),
        }
    }

    fn write_seq(&self, storage: &dyn Storage, path: &Path, item: &Item, seq: i64) -> Result<()> {
        match item {
            Item::Request(r) => write_if_changed(
                storage,
                &self.root.join(path),
                &format::write_request(&r.request, Some(seq)),
            ),
            Item::Folder(f) => {
                let meta = FolderMeta::new(f.name.clone(), Some(seq), &f.defaults);
                write_if_changed(
                    storage,
                    &self.root.join(path).join(FOLDER_FILE),
                    &format::write_folder(&meta),
                )
            }
            Item::Broken(_) => Ok(()),
        }
    }

    /// `parent/<slug>.toml`, not clashing with other files or folders.
    /// `own` is the item being renamed, which may keep its current name.
    fn free_file_path(
        &self,
        storage: &dyn Storage,
        parent: &Path,
        name: &str,
        own: Option<&Path>,
    ) -> PathBuf {
        let stem = slug::unique(&slug::slugify(name), |c| {
            let file = parent.join(format!("{c}.toml"));
            own != Some(file.as_path())
                && (storage.exists(&self.root.join(&file))
                    || storage.exists(&self.root.join(parent.join(c))))
        });
        parent.join(format!("{stem}.toml"))
    }

    fn free_dir_path(
        &self,
        storage: &dyn Storage,
        parent: &Path,
        name: &str,
        own: Option<&Path>,
    ) -> PathBuf {
        let reserved = parent.as_os_str().is_empty();
        let stem = slug::unique(&slug::slugify(name), |c| {
            let dir = parent.join(c);
            (reserved && (c == ENVIRONMENTS_DIR || c == WORKFLOWS_DIR))
                || (own != Some(dir.as_path())
                    && (storage.exists(&self.root.join(&dir))
                        || storage.exists(&self.root.join(parent.join(format!("{c}.toml"))))))
        });
        parent.join(stem)
    }
}

fn find_in<'a>(items: &'a [Item], path: &Path) -> Option<&'a Item> {
    for item in items {
        if item.path() == path {
            return Some(item);
        }
        if let Item::Folder(f) = item
            && path.starts_with(&f.path)
            && let Some(found) = find_in(&f.items, path)
        {
            return Some(found);
        }
    }
    None
}

fn load_dir(storage: &dyn Storage, root: &Path, rel: &Path) -> Result<Vec<Item>> {
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

fn load_workflows(storage: &dyn Storage, root: &Path) -> Result<(Vec<Workflow>, Vec<Broken>)> {
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

fn load_environments(
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

fn copy_dir(storage: &dyn Storage, from: &Path, to: &Path) -> Result<()> {
    storage.create_dir_all(to).map_err(|source| Error::Io {
        path: to.to_owned(),
        source,
    })?;
    for entry in storage.list(from).map_err(|source| Error::Io {
        path: from.to_owned(),
        source,
    })? {
        let (src, dst) = (from.join(&entry.name), to.join(&entry.name));
        if entry.is_dir {
            copy_dir(storage, &src, &dst)?;
        } else {
            write(storage, &dst, &read(storage, &src)?)?;
        }
    }
    Ok(())
}

fn parent_of(path: &Path) -> PathBuf {
    path.parent().map(Path::to_owned).unwrap_or_default()
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn read(storage: &dyn Storage, path: &Path) -> Result<String> {
    storage.read_to_string(path).map_err(|source| Error::Io {
        path: path.to_owned(),
        source,
    })
}

fn write(storage: &dyn Storage, path: &Path, contents: &str) -> Result<()> {
    storage.write(path, contents).map_err(|source| Error::Io {
        path: path.to_owned(),
        source,
    })
}

/// Skips the write when the file already has exactly this content (R-GIT-3).
fn write_if_changed(storage: &dyn Storage, path: &Path, contents: &str) -> Result<()> {
    if storage
        .read_to_string(path)
        .is_ok_and(|current| current == contents)
    {
        return Ok(());
    }
    write(storage, path, contents)
}

fn remove_file(storage: &dyn Storage, path: &Path) -> Result<()> {
    storage.remove_file(path).map_err(|source| Error::Io {
        path: path.to_owned(),
        source,
    })
}

fn rename(storage: &dyn Storage, from: &Path, to: &Path) -> Result<()> {
    storage.rename(from, to).map_err(|source| Error::Io {
        path: from.to_owned(),
        source,
    })
}

/// Environment lookup helpers shared by the UI and request preparation.
pub fn server_names(manifest: &Manifest) -> BTreeMap<String, String> {
    manifest
        .servers
        .iter()
        .map(|s| (s.id.clone(), s.name.clone()))
        .collect()
}
