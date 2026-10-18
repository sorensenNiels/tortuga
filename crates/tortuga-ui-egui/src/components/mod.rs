//! UI building blocks. Each is a plain function (or a dialog struct with `show`)
//! that draws into an [`egui::Ui`] and reports what the user asked for.

mod auth_editor;
mod body_editor;
mod defaults_dialog;
mod environments;
mod history_list;
mod import_dialog;
mod key_value_table;
mod modals;
mod palette;
mod request_editor;
mod response_panel;
mod sidebar;
mod tab_strip;
mod workflow_view;

pub use auth_editor::request_auth;
pub use body_editor::{BodyDrafts, body_editor};
pub use defaults_dialog::DefaultsDialog;
pub use environments::{EnvironmentsDialog, SavedConfig};
pub use history_list::{ClearHistory, HistoryAction, HistoryContext, HistoryRow, history_list};
pub use import_dialog::{ImportDialog, ImportTarget};
pub use key_value_table::key_value_table;
use modals::dialog;
pub use modals::{
    AddProjects, AddProjectsDialog, ConfirmDialog, CookieAction, CookiesDialog, CopyCurlDialog,
    CreateProjectDialog, DialogResult, MoveDialog, NameDialog, RuntimeVarsDialog,
    SaveToCollectionDialog, SettingsDialog,
};
pub use palette::{Palette, PaletteItem};
pub use request_editor::{EditorAction, EditorContext, EditorState, request_editor};
pub use response_panel::{
    Filtered, Outcome, PanelContext, ResponseAction, ResponseState, ResponseView, TestsView,
    TextBody, filter_body, response_panel, tests_details,
};
pub use sidebar::{Marks, SidebarAction, SidebarState, sidebar};
pub use tab_strip::{GroupChip, TabAction, TabGroup, TabStripState, TabView, tab_strip};
pub use workflow_view::{
    IterationView, RequestChoice, RunView, StepView, WorkflowAction, WorkflowContext,
    WorkflowEditorState, workflow_editor, workflow_results,
};
