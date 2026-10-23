//! Copies of requests and folders: next to the original (R-COL-2) and into another
//! project (R-COL-38).

use std::path::{Path, PathBuf};

use super::{Collection, FOLDER_FILE, Item, copy_dir, parent_of, write};
use crate::format::{self, FolderMeta};
use crate::model::Request;
use crate::storage::Storage;
use crate::{Error, Result};

impl Collection {
    /// Copies a request or folder (with everything in it) right after the original.
    pub fn duplicate(&self, storage: &dyn Storage, path: &Path) -> Result<PathBuf> {
        self.ensure_writable()?;
        let item = self
            .find(path)
            .ok_or_else(|| Error::NotFound(path.to_owned()))?;
        let parent = parent_of(path);
        let seq = self.seq_after(&parent, path);
        let name = format!("{} copy", item.name());
        self.write_copy(storage, &self.root, item, &parent, name, seq)
    }

    /// Copies a request or folder (with everything in it) of `source` into `parent` of
    /// this project, at the end, under its own name (R-COL-38). `source` may be another
    /// project, also a read-only one, and is left as it is.
    pub fn copy_from(
        &self,
        storage: &dyn Storage,
        source: &Collection,
        path: &Path,
        parent: &Path,
    ) -> Result<PathBuf> {
        self.ensure_writable()?;
        let item = source
            .find(path)
            .ok_or_else(|| Error::NotFound(path.to_owned()))?;
        if !parent.as_os_str().is_empty() && !matches!(self.find(parent), Some(Item::Folder(_))) {
            return Err(Error::NotFound(parent.to_owned()));
        }
        if source.root == self.root && parent.starts_with(path) {
            return Err(Error::InvalidOperation(
                "A folder cannot be copied into itself.".into(),
            ));
        }
        let seq = Some(self.next_seq(parent));
        self.write_copy(storage, &source.root, item, parent, item.name(), seq)
    }

    /// Writes `item` of the project at `from_root` into `parent` as `name`.
    fn write_copy(
        &self,
        storage: &dyn Storage,
        from_root: &Path,
        item: &Item,
        parent: &Path,
        name: String,
        seq: Option<i64>,
    ) -> Result<PathBuf> {
        match item {
            Item::Request(r) => {
                let copy = Request {
                    name,
                    ..r.request.clone()
                };
                let new_path = self.free_file_path(storage, parent, &copy.name, None);
                write(
                    storage,
                    &self.root.join(&new_path),
                    &format::write_request(&copy, seq),
                )?;
                Ok(new_path)
            }
            Item::Folder(folder) => {
                let new_path = self.free_dir_path(storage, parent, &name, None);
                copy_dir(
                    storage,
                    &from_root.join(&folder.path),
                    &self.root.join(&new_path),
                )?;
                let meta = FolderMeta::new(name, seq, &folder.defaults);
                write(
                    storage,
                    &self.root.join(&new_path).join(FOLDER_FILE),
                    &format::write_folder(&meta),
                )?;
                Ok(new_path)
            }
            Item::Broken(_) => Err(Error::InvalidOperation(
                "A file that cannot be read cannot be copied.".into(),
            )),
        }
    }
}
