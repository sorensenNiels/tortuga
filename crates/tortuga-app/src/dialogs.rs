//! Native file dialogs.
//!
//! Blocking/modal on purpose: they are called from the UI thread, which is where
//! GTK and macOS require native dialogs to run. On Linux, rfd uses its GTK backend
//! (see Cargo.toml): the xdg-desktop-portal backend fails silently where no portal
//! runs (e.g. WSL).

use std::path::PathBuf;

/// Asks where to save a file. `None` if the user cancelled.
pub fn save_path(suggested_name: &str) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Save response body")
        .set_file_name(suggested_name)
        .save_file()
}

/// Asks for a folder. `None` if the user cancelled.
pub fn pick_folder(title: &str) -> Option<PathBuf> {
    rfd::FileDialog::new().set_title(title).pick_folder()
}
