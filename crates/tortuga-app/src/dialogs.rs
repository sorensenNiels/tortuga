//! Native file dialogs.
//!
//! Blocking/modal on purpose: they are called from the UI thread, which is where
//! GTK and macOS require native dialogs to run. On Linux, rfd uses its GTK backend
//! (see Cargo.toml): the xdg-desktop-portal backend fails silently where no portal
//! runs (e.g. WSL).

use std::path::PathBuf;

/// Asks where to save a file. `None` if the user cancelled.
pub fn save_path(title: &str, suggested_name: &str) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title(title)
        .set_file_name(suggested_name)
        .save_file()
}

/// Asks for an existing file whose name ends with `suffix` (e.g. `.tortuga-workspace.toml`).
pub fn pick_file(title: &str, suffix: &str) -> Option<PathBuf> {
    // Filters match the last extension only.
    let extension = suffix.rsplit('.').next().unwrap_or_default();
    rfd::FileDialog::new()
        .set_title(title)
        .add_filter(suffix.trim_start_matches('.'), &[extension])
        .pick_file()
}

/// Asks for any file. `None` if the user cancelled.
pub fn pick_any_file(title: &str) -> Option<PathBuf> {
    rfd::FileDialog::new().set_title(title).pick_file()
}

/// Asks for one or more files with one of `extensions` (any file when empty). Empty if
/// the user cancelled.
pub fn pick_any_files(title: &str, extensions: &[&str]) -> Vec<PathBuf> {
    let mut dialog = rfd::FileDialog::new().set_title(title);
    if !extensions.is_empty() {
        dialog = dialog.add_filter(extensions.join(", "), extensions);
    }
    dialog.pick_files().unwrap_or_default()
}

/// Asks for a folder. `None` if the user cancelled.
pub fn pick_folder(title: &str) -> Option<PathBuf> {
    rfd::FileDialog::new().set_title(title).pick_folder()
}
