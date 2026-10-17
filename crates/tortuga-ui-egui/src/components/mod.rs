//! UI building blocks. Each is a plain function (or a dialog struct with `show`)
//! that draws into an [`egui::Ui`] and reports what the user asked for.

mod auth_editor;
mod body_editor;
mod defaults_dialog;
mod environments;
mod key_value_table;
mod modals;
mod request_editor;
mod response_panel;
mod sidebar;

pub use auth_editor::request_auth;
pub use body_editor::{BodyDrafts, body_editor};
pub use defaults_dialog::DefaultsDialog;
pub use environments::{EnvironmentsDialog, SavedConfig};
pub use key_value_table::key_value_table;
use modals::dialog;
pub use modals::{
    AddProjects, AddProjectsDialog, ConfirmDialog, CopyCurlDialog, CreateProjectDialog,
    DialogResult, MoveDialog, NameDialog, SaveToCollectionDialog, SettingsDialog,
};
pub use request_editor::{EditorAction, EditorContext, EditorState, request_editor};
pub use response_panel::{Outcome, ResponseAction, ResponseState, response_panel};
pub use sidebar::{SidebarAction, SidebarState, sidebar};
