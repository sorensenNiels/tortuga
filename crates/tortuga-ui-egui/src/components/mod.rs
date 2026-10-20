//! UI building blocks. Each is a plain function (or a dialog struct with `show`)
//! that draws into an [`egui::Ui`] and reports what the user asked for.

mod auth_editor;
mod body_editor;
mod console_view;
mod defaults_dialog;
mod environments;
mod history_list;
mod import_dialog;
mod key_value_table;
mod mock_dialog;
mod modals;
mod palette;
mod request_editor;
mod request_examples;
mod request_scripts;
mod response_panel;
mod script_editing;
mod script_editor;
mod sidebar;
mod tab_strip;
mod variable_row;
mod workflow_view;
mod workspace_vars;

pub use auth_editor::request_auth;
pub use body_editor::{BodyDrafts, body_editor};
pub use defaults_dialog::{DefaultsDialog, DefaultsTarget, EditedDefaults};
pub use environments::{EnvironmentsDialog, SavedConfig};
pub use history_list::{ClearHistory, HistoryAction, HistoryContext, HistoryRow, history_list};
pub use import_dialog::{ImportDialog, ImportTarget};
pub use key_value_table::key_value_table;
pub use mock_dialog::{MockAction, MockDialog, MockView};
pub use modals::{
    AddProjects, AddProjectsDialog, ConfirmDialog, CookieAction, CookiesDialog, CopyCurlDialog,
    CreateProjectDialog, DialogResult, MoveDialog, NameDialog, RuntimeVarsDialog,
    SaveToCollectionDialog, SettingsDialog,
};
use modals::{dialog, resizable_dialog};
pub use palette::{Palette, PaletteItem};
pub use request_editor::{EditorAction, EditorContext, EditorState, Rerun, request_editor};
pub use request_scripts::ScriptTab;
pub use response_panel::{
    Filtered, Outcome, PanelContext, ResponseAction, ResponseState, ResponseView, SaveExample,
    TestsView, TextBody, filter_body, response_panel, tests_details,
};
pub use script_editing::insert_block;
pub use sidebar::{Marks, SidebarAction, SidebarState, sidebar};
pub use tab_strip::{GroupChip, TabAction, TabGroup, TabStripState, TabView, tab_strip};
pub use workflow_view::{
    IterationView, RequestChoice, RunView, StepView, WorkflowAction, WorkflowContext,
    WorkflowEditorState, workflow_editor, workflow_results,
};
pub use workspace_vars::{WorkspaceVarsAction, WorkspaceVarsDialog};
