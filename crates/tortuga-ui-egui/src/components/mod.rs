//! UI building blocks. Each is a plain function (or a dialog struct with `show`)
//! that draws into an [`egui::Ui`] and reports what the user asked for.

mod body_editor;
mod environments;
mod key_value_table;
mod modals;
mod request_editor;
mod response_panel;
mod sidebar;

pub use body_editor::{BodyDrafts, body_editor};
pub use environments::EnvironmentsDialog;
pub use key_value_table::key_value_table;
use modals::dialog;
pub use modals::{
    ConfirmDialog, CreateCollectionDialog, DialogResult, MoveDialog, SaveToCollectionDialog,
    SettingsDialog,
};
pub use request_editor::{EditorAction, EditorContext, EditorState, request_editor};
pub use response_panel::{Outcome, ResponseAction, ResponseState, response_panel};
pub use sidebar::{SidebarAction, SidebarState, sidebar};
