//! `tortuga-cli list`: a project's requests, folders, workflows and environments as JSON,
//! for editor extensions (R-VSC-1).

use tortuga_core::Collection;
use tortuga_core::workspace::WorkspaceFile;

use crate::Args;

/// The listing of `args.project` (and the environments of `args.workspace`).
pub fn list(args: &Args) -> Result<String, String> {
    let collection = Collection::open(&tortuga_core::storage::FsStorage, &args.project)
        .map_err(|e| e.to_string())?;
    let workspace = match &args.workspace {
        None => WorkspaceFile::default(),
        Some(path) => crate::read_workspace(path)?,
    };
    Ok(tortuga_core::listing::to_json(&collection, &workspace))
}
