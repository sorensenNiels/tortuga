//! Top-level layout and the request workflow: selection, unsaved drafts,
//! sending, saving, and the dialogs.

mod commands;
mod history;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

use egui::{
    CentralPanel, ComboBox, Frame, Key, KeyboardShortcut, Margin, Modifiers, Panel, RichText,
    TextEdit, Ui,
};
use futures_util::future::{AbortHandle, abortable};
use tortuga_core::{Item, Request, vars};

use crate::components::{
    AddProjects, AddProjectsDialog, ClearHistory, ConfirmDialog, CopyCurlDialog,
    CreateProjectDialog, DefaultsDialog, DialogResult, EditorAction, EditorContext, EditorState,
    EnvironmentsDialog, Filtered, MoveDialog, NameDialog, Outcome, Palette, PanelContext,
    ResponseAction, ResponseState, ResponseView, SaveToCollectionDialog, SettingsDialog,
    SidebarAction, SidebarState, TextBody, request_editor, response_panel, sidebar,
};
use crate::format::sentence;
use crate::host::{DraftConflict, Host, SavedDraft, SavedDrafts};
use crate::state::{AppState, ItemKey};
use crate::theme;
use history::{HistoryState, SidebarView};

const SAVE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::S);
const SEND: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Enter);
const FIND: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::F);
const QUICK_OPEN: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::P);
const COMMANDS: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::K);
const COMMANDS_TOO: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::P);

#[derive(Debug)]
enum Modal {
    Settings(SettingsDialog),
    Environments(EnvironmentsDialog),
    Defaults(DefaultsDialog),
    ConfirmDelete(ItemKey, ConfirmDialog),
    MoveTo(MoveDialog),
    SaveToCollection(SaveToCollectionDialog),
    CreateProject(CreateProjectDialog),
    AddProjects(AddProjectsDialog),
    NewWorkspace(NameDialog),
    RenameWorkspace(NameDialog),
    RemoveWorkspace(ConfirmDialog),
    CopyCurl(CopyCurlDialog),
    /// Quick open and the command palette (R-COL-9, R-COL-10).
    Palette(Palette),
    ClearHistory(ClearHistory, ConfirmDialog),
}

/// What the user picked in the workspace menu (R-WS-1, R-WS-5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WorkspaceAction {
    Switch(usize),
    New,
    Rename,
    SaveAsFile,
    OpenFile,
    Remove,
}

/// `None` = the scratch request.
type Selection = Option<ItemKey>;

#[derive(Debug)]
struct InFlight {
    key: Selection,
    id: u64,
    abort: AbortHandle,
}

/// A send that finished in the background.
#[derive(Debug)]
struct Finished {
    id: u64,
    outcome: Outcome,
    /// What to keep in the history (R-HIST-1); `None` if it was never sent.
    entry: Option<tortuga_core::history::Entry>,
}

/// The JSON filter being applied to the shown response (R-HTTP-13).
#[derive(Debug)]
struct FilterJob {
    view: Arc<ResponseView>,
    expr: String,
    id: u64,
    /// `None` while it is being worked out.
    result: Option<Result<TextBody, String>>,
    abort: AbortHandle,
}

#[derive(Debug)]
struct FilterDone {
    id: u64,
    result: Result<TextBody, String>,
}

/// The whole Tortuga user interface. Create it once, then call [`TortugaUi::ui`] every frame.
#[derive(Debug)]
pub struct TortugaUi {
    state: AppState,
    modal: Option<Modal>,

    // The request being edited, and edits of other requests not saved yet (R-COL-6).
    selection: Selection,
    current: Request,
    scratch: Request,
    drafts: HashMap<ItemKey, Request>,
    editor: EditorState,
    sidebar: SidebarState,

    outcomes: HashMap<Selection, Outcome>,
    response: ResponseState,
    in_flight: Option<InFlight>,
    next_send_id: u64,
    finished_tx: Sender<Finished>,
    finished_rx: Receiver<Finished>,
    /// Fonts and style are installed on the first frame.
    styled: bool,
    /// Requests with unsaved edits whose file changed on disk (R-GIT-11).
    conflicts: HashMap<ItemKey, DiskChange>,
    /// When a request was last copied as curl (egui time), for the "Copied" feedback.
    copied_at: Option<f64>,
    /// Unsaved edits as last written to local data (R-COL-8), and when that was checked.
    drafts_saved: SavedDrafts,
    drafts_checked_at: f64,
    /// Saved versions that restored edits started from, for projects not open yet:
    /// checked against the disk once their project opens (R-COL-8, R-GIT-11).
    unchecked_bases: HashMap<ItemKey, Option<Request>>,
    /// Recently opened requests, newest first, for quick open (R-COL-9).
    recent: Vec<ItemKey>,
    /// The JSON filter typed for each request, kept while the app runs (R-HTTP-13).
    filters: HashMap<Selection, String>,
    filter_job: Option<FilterJob>,
    filter_tx: Sender<FilterDone>,
    filter_rx: Receiver<FilterDone>,
    sidebar_view: SidebarView,
    history: HistoryState,
}

/// How often unsaved edits are written to local data, at most (R-COL-8).
const DRAFTS_SAVE_SECONDS: f64 = 1.0;

/// What happened on disk to a request that has unsaved edits (R-GIT-11).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DiskChange {
    Changed,
    Deleted,
}

/// The user's answer in the conflict banner (D9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConflictChoice {
    KeepMine,
    TakeTheirs,
}

impl TortugaUi {
    /// Loads settings and reopens the collections that were open last time.
    pub fn new(host: Box<dyn Host>) -> Self {
        let current = Request::default();
        let (finished_tx, finished_rx) = channel();
        let (filter_tx, filter_rx) = channel();
        let mut ui = Self {
            state: AppState::new(host),
            modal: None,
            selection: None,
            editor: EditorState::new(&current),
            current,
            scratch: Request::default(),
            drafts: HashMap::new(),
            sidebar: SidebarState::default(),
            outcomes: HashMap::new(),
            response: ResponseState::default(),
            in_flight: None,
            next_send_id: 0,
            finished_tx,
            finished_rx,
            styled: false,
            conflicts: HashMap::new(),
            copied_at: None,
            drafts_saved: SavedDrafts::default(),
            drafts_checked_at: 0.0,
            unchecked_bases: HashMap::new(),
            recent: Vec::new(),
            filters: HashMap::new(),
            filter_job: None,
            filter_tx,
            filter_rx,
            sidebar_view: SidebarView::default(),
            history: HistoryState::default(),
        };
        ui.restore_drafts();
        // Back where the user left off (R-WS-1).
        let remembered = ui.state.remembered_selection();
        ui.select(remembered);
        ui
    }

    /// Draws the app into `ui` (the whole window) and handles what the user did.
    pub fn ui(&mut self, ui: &mut Ui) {
        if !self.styled {
            theme::install(ui.ctx());
            self.state.set_repaint(ui.ctx());
            self.styled = true;
        }
        self.collect_finished();
        self.save_drafts_now_and_then(ui.input(|i| i.time));
        for root in self.state.take_changes() {
            self.on_disk_change(&root);
        }
        if self.modal.is_none() {
            self.shortcuts(ui);
        }

        let p = theme::palette(ui);
        let chrome = Frame::new()
            .fill(p.chrome)
            .inner_margin(Margin::symmetric(14, 8));
        Panel::top("topbar")
            .frame(chrome)
            .show(ui, |ui| self.topbar(ui));
        if !self.state.settings.verify_tls {
            let frame = theme::banner_frame(ui, false);
            Panel::top("tls-warning").frame(frame).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("⚠").color(p.warning_text));
                    ui.label(
                        RichText::new(
                            "TLS certificate verification is off. Responses could come from an impostor server.",
                        )
                        .color(p.warning_text),
                    );
                    if ui.link("Change in settings").clicked() {
                        self.modal = Some(Modal::Settings(SettingsDialog::new(&self.state.settings)));
                    }
                });
            });
        }
        if let Some(message) = self.state.notice.clone() {
            let frame = theme::banner_frame(ui, true);
            Panel::top("notice").frame(frame).show(ui, |ui| {
                egui::Sides::new().shrink_left().show(
                    ui,
                    |ui| {
                        ui.add(egui::Label::new(RichText::new(message).color(p.fail)).wrap());
                    },
                    |ui| {
                        if ui
                            .add(theme::icon_button(ui, "🗙"))
                            .on_hover_text("Dismiss")
                            .clicked()
                        {
                            self.state.notice = None;
                        }
                    },
                );
            });
        }

        self.left_panel(ui);

        CentralPanel::default()
            .frame(
                Frame::new()
                    .fill(p.bg)
                    .inner_margin(Margin::symmetric(20, 16)),
            )
            .show(ui, |ui| self.workspace(ui));

        self.show_modal(ui.ctx());
    }

    /// The sidebar: projects, or the history (R-HIST-2).
    fn left_panel(&mut self, ui: &mut Ui) {
        let p = theme::palette(ui);
        let dirty = self.dirty_keys();
        let conflicts: HashSet<ItemKey> = self.conflicts.keys().cloned().collect();
        let actions = Panel::left("sidebar")
            .resizable(true)
            .default_size(270.0)
            .size_range(200.0..=520.0)
            .frame(
                Frame::new()
                    .fill(p.chrome)
                    .inner_margin(Margin::symmetric(10, 12)),
            )
            .show(ui, |ui| {
                if self.state.host.keeps_history() {
                    theme::tabs(
                        ui,
                        &mut self.sidebar_view,
                        &[
                            (SidebarView::Projects, "Projects", None),
                            (SidebarView::History, "History", None),
                        ],
                    );
                    ui.add_space(8.0);
                }
                match self.sidebar_view {
                    SidebarView::Projects => sidebar(
                        ui,
                        &self.state,
                        &mut self.sidebar,
                        self.selection.as_ref(),
                        &dirty,
                        &conflicts,
                    ),
                    SidebarView::History => {
                        self.history_panel(ui);
                        Vec::new()
                    }
                }
            })
            .inner;
        for action in actions {
            self.on_sidebar(action);
        }
    }

    /// Keyboard shortcuts that work anywhere outside dialogs.
    fn shortcuts(&mut self, ui: &mut Ui) {
        let (save, send) = ui.input_mut(|i| (i.consume_shortcut(&SAVE), i.consume_shortcut(&SEND)));
        if save {
            self.save();
        }
        if send && self.sending().is_none() {
            self.send(ui.ctx());
        }
        // Ctrl+Shift+P first: Ctrl+P would match it too.
        let (commands, quick_open) = ui.input_mut(|i| {
            (
                i.consume_shortcut(&COMMANDS_TOO) || i.consume_shortcut(&COMMANDS),
                i.consume_shortcut(&QUICK_OPEN),
            )
        });
        if commands {
            self.modal = Some(Modal::Palette(Palette::commands()));
        } else if quick_open {
            self.modal = Some(Modal::Palette(Palette::requests()));
        }
        if ui.input_mut(|i| i.consume_shortcut(&FIND)) {
            self.response.focus_search = true;
        }
    }

    /// Call when the app closes: keeps unsaved edits for next time (R-COL-8).
    pub fn on_exit(&mut self) {
        self.save_drafts();
    }

    // ---------------------------------------------------------------- unsaved edits (R-COL-8)

    /// Unsaved edits as they should be kept now.
    fn drafts_snapshot(&self) -> SavedDrafts {
        let mut edited = self.drafts.clone();
        if let Some(k) = &self.selection {
            if self.is_dirty(k, &self.current) {
                edited.insert(k.clone(), self.current.clone());
            } else {
                edited.remove(k);
            }
        }
        let mut drafts: Vec<SavedDraft> = edited
            .into_iter()
            .map(|(key, request)| {
                let base = if self.state.collection(&key.root).is_some() {
                    self.state.saved_request(&key)
                } else {
                    self.unchecked_bases.get(&key).cloned().flatten()
                };
                let conflict = self.conflicts.get(&key).map(|c| match c {
                    DiskChange::Changed => DraftConflict::Changed,
                    DiskChange::Deleted => DraftConflict::Deleted,
                });
                SavedDraft {
                    root: key.root,
                    path: key.path,
                    request,
                    base,
                    conflict,
                }
            })
            .collect();
        // A stable order, so an unchanged set is never written again.
        drafts.sort_by(|a, b| (&a.root, &a.path).cmp(&(&b.root, &b.path)));
        let scratch = if self.selection.is_none() {
            &self.current
        } else {
            &self.scratch
        };
        SavedDrafts {
            scratch: (*scratch != Request::default()).then(|| scratch.clone()),
            drafts,
        }
    }

    /// Keeps unsaved edits for next time, checking at most every [`DRAFTS_SAVE_SECONDS`].
    fn save_drafts_now_and_then(&mut self, now: f64) {
        if now - self.drafts_checked_at >= DRAFTS_SAVE_SECONDS {
            self.drafts_checked_at = now;
            self.save_drafts();
        }
    }

    /// Writes unsaved edits to local data if they changed since the last write.
    fn save_drafts(&mut self) {
        let snapshot = self.drafts_snapshot();
        if snapshot == self.drafts_saved {
            return;
        }
        if let Err(e) = self.state.host.save_drafts(&snapshot) {
            self.state.notify(format!(
                "Unsaved edits could not be kept for next time: {e}"
            ));
        }
        self.drafts_saved = snapshot;
    }

    /// Brings back the unsaved edits of the last session.
    fn restore_drafts(&mut self) {
        let saved = match self.state.host.load_drafts() {
            Ok(saved) => saved,
            Err(e) => {
                self.state.notify(format!(
                    "Could not read the unsaved edits of last time: {e}"
                ));
                return;
            }
        };
        self.scratch = saved.scratch.clone().unwrap_or_default();
        self.load(self.scratch.clone());
        for draft in &saved.drafts {
            let key = ItemKey::new(&draft.root, draft.path.clone());
            self.drafts.insert(key.clone(), draft.request.clone());
            let change = match draft.conflict {
                Some(DraftConflict::Changed) => Some(DiskChange::Changed),
                Some(DraftConflict::Deleted) => Some(DiskChange::Deleted),
                None => None,
            };
            match change {
                Some(change) => {
                    self.conflicts.insert(key, change);
                }
                None => {
                    self.unchecked_bases.insert(key, draft.base.clone());
                }
            }
        }
        self.drafts_saved = saved;
        self.check_restored_drafts();
    }

    /// Compares restored edits with the disk once their project is open: a file that
    /// changed while the app was closed gets the conflict banner (R-GIT-11); edits
    /// that now equal the file are dropped.
    fn check_restored_drafts(&mut self) {
        let open: Vec<ItemKey> = self
            .unchecked_bases
            .keys()
            .filter(|k| self.state.collection(&k.root).is_some())
            .cloned()
            .collect();
        for key in open {
            let Some(base) = self.unchecked_bases.remove(&key) else {
                continue;
            };
            let saved = self.state.saved_request(&key);
            if saved != base {
                let change = if saved.is_some() {
                    DiskChange::Changed
                } else {
                    DiskChange::Deleted
                };
                self.conflicts.insert(key, change);
            } else if saved.as_ref() == self.drafts.get(&key) {
                self.drafts.remove(&key);
            }
        }
    }

    // ---------------------------------------------------------------- layout

    fn topbar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let p = theme::palette(ui);
            ui.label(theme::semibold("🐢").size(18.0).color(p.accent));
            ui.label(theme::semibold("Tortuga").size(15.0));
            ui.add_space(12.0);
            ui.separator();
            ui.add_space(4.0);
            if let Some(action) = self.workspace_menu(ui) {
                self.on_workspace(action);
            }
            if !self.state.collections.is_empty() {
                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);
                self.environment_picker(ui);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(theme::ghost_button("⚙  Settings")).clicked() {
                    self.modal = Some(Modal::Settings(SettingsDialog::new(&self.state.settings)));
                }
                let go_to = (
                    "Go to request…",
                    RichText::new("  Ctrl+P").small().color(p.muted),
                );
                if ui
                    .add(egui::Button::new(go_to))
                    .on_hover_text("Quick open (Ctrl+P). Ctrl+K lists every command.")
                    .clicked()
                {
                    self.modal = Some(Modal::Palette(Palette::requests()));
                }
            });
        });
    }

    /// The workspace switcher and what can be done with workspaces (R-WS-1, R-WS-5).
    fn workspace_menu(&self, ui: &mut Ui) -> Option<WorkspaceAction> {
        let local = &self.state.local;
        let current = self.state.workspace();
        let button =
            egui::Button::new((theme::semibold(&current.name), "  ⏷")).frame_when_inactive(false);
        let (response, inner) =
            egui::containers::menu::MenuButton::from_button(button).ui(ui, |ui| {
                let mut action = None;
                theme::section_label(ui, "Workspaces");
                for (i, ws) in local.workspaces.iter().enumerate() {
                    let mut button = ui.add(egui::Button::selectable(
                        i == local.active_workspace,
                        &ws.name,
                    ));
                    if let Some(file) = &ws.file {
                        button = button.on_hover_text(format!("Saved in {}", file.display()));
                    }
                    if button.clicked() {
                        action = Some(WorkspaceAction::Switch(i));
                    }
                }
                ui.separator();
                if ui.button("New workspace…").clicked() {
                    action = Some(WorkspaceAction::New);
                }
                if ui.button("Rename…").clicked() {
                    action = Some(WorkspaceAction::Rename);
                }
                if self.state.host.can_save_files() {
                    let hover = match &current.file {
                        Some(file) => format!(
                            "Kept in sync with {}. Choose another file to move it.",
                            file.display()
                        ),
                        None => {
                            "Write the workspace to a file you can commit, so teammates can open it"
                                .into()
                        }
                    };
                    if ui.button("Save as file…").on_hover_text(hover).clicked() {
                        action = Some(WorkspaceAction::SaveAsFile);
                    }
                    if ui
                        .button("Open workspace file…")
                        .on_hover_text("Open a workspace a teammate saved as a file")
                        .clicked()
                    {
                        action = Some(WorkspaceAction::OpenFile);
                    }
                }
                let danger = theme::palette(ui).fail;
                if ui
                    .add_enabled(
                        local.workspaces.len() > 1,
                        egui::Button::new(RichText::new("Remove workspace…").color(danger)),
                    )
                    .on_disabled_hover_text("There is always at least one workspace")
                    .clicked()
                {
                    action = Some(WorkspaceAction::Remove);
                }
                action
            });
        response.on_hover_text("Switch workspace");
        inner.and_then(|i| i.inner)
    }

    fn on_workspace(&mut self, action: WorkspaceAction) {
        match action {
            WorkspaceAction::Switch(index) => self.switch_workspace(|state| {
                state.switch_workspace(index);
            }),
            WorkspaceAction::New => {
                self.modal = Some(Modal::NewWorkspace(NameDialog::new(
                    "New workspace",
                    "Create",
                    "",
                )));
            }
            WorkspaceAction::Rename => {
                let name = self.state.workspace().name.clone();
                self.modal = Some(Modal::RenameWorkspace(NameDialog::new(
                    "Rename workspace",
                    "Rename",
                    &name,
                )));
            }
            WorkspaceAction::SaveAsFile => {
                let name = tortuga_core::workspace::file_name(&self.state.workspace().name);
                if let Some(file) = self.state.host.pick_save_path("Save workspace as", &name)
                    && let Err(e) = self.state.save_workspace_as(&file)
                {
                    self.state.notify(e);
                }
            }
            WorkspaceAction::OpenFile => {
                if let Some(file) = self.state.host.pick_file(
                    "Open a workspace file",
                    tortuga_core::workspace::WORKSPACE_FILE_SUFFIX,
                ) {
                    let mut result = Ok(());
                    self.switch_workspace(|state| result = state.open_workspace_file(&file));
                    if let Err(e) = result {
                        self.state.notify(e);
                    }
                }
            }
            WorkspaceAction::Remove => {
                let name = self.state.workspace().name.clone();
                self.modal = Some(Modal::RemoveWorkspace(ConfirmDialog {
                    title: "Remove workspace?".into(),
                    message: format!(
                        "“{name}” is removed from this machine. Its projects and files stay where they are."
                    ),
                    confirm_label: "Remove".into(),
                }));
            }
        }
    }

    /// Runs `change` (which may switch workspace), keeping unsaved edits of the old
    /// workspace for this session, then selects the new workspace's remembered request.
    fn switch_workspace(&mut self, change: impl FnOnce(&mut AppState)) {
        self.select_and_remember(None, false);
        change(&mut self.state);
        self.check_restored_drafts();
        let remembered = self.state.remembered_selection();
        self.select_and_remember(remembered, false);
    }

    /// One environment for the whole workspace (R-WS-6, R-SRV-5): every project with an
    /// environment of that name uses it.
    fn environment_picker(&mut self, ui: &mut Ui) {
        let names = self.state.environment_names();
        // The project whose environments ✏ edits: the selected request's, else the first.
        let root = self
            .selection
            .as_ref()
            .map(|k| k.root.clone())
            .or_else(|| self.state.collections.first().map(|c| c.root.clone()));
        let Some(root) = root else {
            return;
        };
        let p = theme::palette(ui);
        ui.label(RichText::new("Environment").color(p.muted));
        let edit = if names.is_empty() {
            ui.button("Add environments…").clicked()
        } else {
            let shown = self.state.workspace().environment.clone().or_else(|| {
                let c = self.state.collection(&root)?;
                let id = self.state.active_environment(&root)?;
                c.environment(&id).map(|e| e.name.clone())
            });
            let mut picked = None;
            ComboBox::from_id_salt("environment")
                .selected_text((
                    RichText::new("⏺").size(9.0).color(p.ok),
                    " ",
                    shown.clone().unwrap_or_default(),
                ))
                .show_ui(ui, |ui| {
                    for name in &names {
                        if ui
                            .selectable_label(shown.as_ref() == Some(name), name)
                            .clicked()
                        {
                            picked = Some(name.clone());
                        }
                    }
                })
                .response
                .on_hover_text("Switches every project in the workspace that has an environment with this name");
            if let Some(name) = picked {
                self.state.set_workspace_environment(&name);
            }
            ui.add(theme::ghost_button("✏"))
                .on_hover_text("Edit environments, servers and variables of this project")
                .clicked()
        };
        if edit {
            self.modal = Some(Modal::Environments(EnvironmentsDialog::new(
                &self.state,
                root,
            )));
        }
    }

    fn workspace(&mut self, ui: &mut Ui) {
        let key = self.selection.clone();
        let root = key.as_ref().map(|k| k.root.clone());
        let ctx = self.state.request_context(key.as_ref());
        let undefined = vars::undefined_in(&self.current, &ctx);
        let current_dirty = key
            .as_ref()
            .is_some_and(|k| self.is_dirty(k, &self.current));
        let servers: Vec<(String, String)> = root
            .as_ref()
            .and_then(|r| self.state.collection(r))
            .map(|c| {
                c.manifest
                    .servers
                    .iter()
                    .map(|s| (s.id.clone(), s.name.clone()))
                    .collect()
            })
            .unwrap_or_default();
        let server_base = self
            .current
            .server
            .as_ref()
            .and_then(|id| ctx.server_urls.get(id).cloned());
        let read_only = root
            .as_ref()
            .and_then(|r| self.state.collection(r))
            .is_some_and(|c| c.read_only);

        self.request_head(ui, key.as_ref(), current_dirty, read_only);
        ui.add_space(4.0);

        let collection_auth = root.as_ref().map(|_| ctx.auth.clone());
        let inherited_headers: Vec<(tortuga_core::KeyValue, String)> = ctx
            .inherited_headers_with_origins(&self.current)
            .map(|(h, origin)| (h.clone(), origin.to_owned()))
            .collect();
        let cookie_count = self.state.cookie_jar(root.as_deref()).len();
        let sending = self.sending() == Some(&key);
        let default_timeout_ms = self.state.settings.default_timeout_ms;
        let preview = self
            .editor
            .shows_preview()
            .then(|| vars::preview(&self.current, &ctx).map_err(|e| sentence(&e.to_string())));
        let action = Panel::top("request")
            .frame(Frame::new().inner_margin(Margin {
                bottom: 12,
                ..Margin::ZERO
            }))
            .resizable(true)
            .default_size(300.0)
            .size_range(120.0..=f32::INFINITY)
            .show(ui, |ui| {
                // Fill the panel: egui remembers a panel's size from what its content used,
                // so a short tab would otherwise shrink the request area for good.
                ui.set_min_height(ui.available_height());
                request_editor(
                    ui,
                    &mut self.current,
                    &mut self.editor,
                    EditorContext {
                        servers: &servers,
                        server_base: server_base.as_deref(),
                        undefined: &undefined,
                        sending,
                        default_timeout_ms,
                        collection_auth: collection_auth.as_ref(),
                        inherited_headers: &inherited_headers,
                        auth_origin: &ctx.auth_origin,
                        cookie_count,
                        in_collection: root.is_some(),
                        preview: preview.as_ref(),
                        can_pick_files: self.state.host.can_pick_folders(),
                    },
                )
            })
            .inner;
        match action {
            Some(EditorAction::Send) => self.send(ui.ctx()),
            Some(EditorAction::Cancel) => self.cancel(),
            Some(EditorAction::PasteCurl(text)) => self.paste_curl(&text),
            Some(EditorAction::ChooseFile(index)) => self.choose_upload(root.as_deref(), index),
            Some(EditorAction::EditCollectionDefaults) => {
                if let Some(root) = root {
                    self.modal = Some(Modal::Defaults(
                        DefaultsDialog::new(&self.state, root).on_auth(),
                    ));
                }
            }
            None => {}
        }

        ui.add_space(14.0);
        self.response_area(ui, &key, sending);
    }

    /// The response of the selected request, with its JSON filter and "Save to file…".
    fn response_area(&mut self, ui: &mut Ui, key: &Selection, sending: bool) {
        if !sending {
            self.past_banner(ui, key);
        }
        let can_save = self.state.host.can_save_files();
        let view = match self.outcomes.get(key) {
            Some(Outcome::Response(view)) => Some(view.clone()),
            _ => None,
        };
        let expr = self
            .filters
            .get(key)
            .map(|f| f.trim().to_owned())
            .unwrap_or_default();
        self.update_filter(view.as_ref(), &expr, ui.ctx());
        let filtered = match self.filter_job.as_ref().map(|j| &j.result) {
            None => Filtered::Off,
            Some(None) => Filtered::Pending,
            Some(Some(Ok(body))) => Filtered::Done(body),
            Some(Some(Err(message))) => Filtered::Failed(message),
        };
        let outcome = self.outcomes.get(key);
        let cx = PanelContext {
            sending,
            can_save,
            filter: self.filters.entry(key.clone()).or_default(),
            filtered,
        };
        if let Some(ResponseAction::SaveBody) = response_panel(ui, outcome, &mut self.response, cx)
            && let Some(Outcome::Response(view)) = outcome
        {
            let r = &view.response;
            self.response.save_status =
                match self.state.host.save_file(&r.suggested_file_name(), &r.body) {
                    Some(Ok(path)) => Some(format!("Saved to {}", path.display())),
                    Some(Err(e)) => Some(format!("Could not save: {e}")),
                    None => self.response.save_status.take(),
                };
        }
    }

    /// Request name, unsaved marker and save/revert (R-COL-6).
    fn request_head(
        &mut self,
        ui: &mut Ui,
        key: Option<&ItemKey>,
        current_dirty: bool,
        read_only: bool,
    ) {
        let p = theme::palette(ui);
        let Some(k) = key else {
            egui::Sides::new().height(46.0).show(
                ui,
                |ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Scratch request").small().color(p.muted));
                        ui.label(theme::semibold("Untitled request").size(18.0));
                    });
                },
                |ui| {
                    if ui
                        .button("Save to project…")
                        .on_hover_text("Save (Ctrl+S)")
                        .clicked()
                    {
                        self.save();
                    }
                    if copy_curl_button(ui, self.copied_at) {
                        self.copy_as_curl(ui.ctx());
                    }
                },
            );
            ui.add_space(8.0);
            return;
        };
        let crumb = breadcrumb(&self.state, k);
        let copied_at = self.copied_at;
        let ((), (save, revert, copy)) = egui::Sides::new().height(46.0).shrink_left().show(
            ui,
            |ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new(crumb).small().color(p.muted));
                    ui.horizontal(|ui| {
                        // Looks like a title; becomes a field on hover/focus.
                        let title =
                            egui::FontId::new(18.0, egui::FontFamily::Name(theme::SEMIBOLD.into()));
                        ui.add_enabled(
                            !read_only,
                            TextEdit::singleline(&mut self.current.name)
                                .id_salt("request-name")
                                .font(title)
                                .frame(Frame::new())
                                // Grows with the name, so the badge sits right after it.
                                .clip_text(false)
                                .desired_width(24.0),
                        )
                        .on_hover_text("Request name");
                        if current_dirty {
                            theme::badge(ui, "Unsaved", p.redirect)
                                .on_hover_text("This request has changes that are not saved");
                        }
                    });
                });
            },
            |ui| {
                let save = ui
                    .add_enabled(
                        current_dirty && !read_only,
                        theme::primary_button(ui, "Save"),
                    )
                    .on_hover_text("Save (Ctrl+S)")
                    .clicked();
                let revert = current_dirty && ui.button("Revert").clicked();
                let copy = copy_curl_button(ui, copied_at);
                (save, revert, copy)
            },
        );
        if copy {
            self.copy_as_curl(ui.ctx());
        }
        if save {
            self.save();
        }
        if revert {
            self.revert();
        }
        ui.add_space(8.0);
        let key = k.clone();
        self.conflict_banner(ui, &key);
    }

    /// Copies the request as curl; asks first if it contains secret values (R-HTTP-9).
    fn copy_as_curl(&mut self, egui_ctx: &egui::Context) {
        let ctx = self.state.request_context(self.selection.as_ref());
        match tortuga_core::curl::command(&self.current, &ctx, &self.state.settings) {
            Err(e) => {
                self.copied_at = None;
                self.state.notify(format!("Could not copy as curl: {e}"));
            }
            Ok(command) if command.masked.is_none() => {
                egui_ctx.copy_text(command.full);
                self.copied_at = Some(egui_ctx.input(|i| i.time));
            }
            Ok(command) => self.modal = Some(Modal::CopyCurl(CopyCurlDialog::new(command))),
        }
    }

    /// Fills the current request from a pasted `curl` command, as unsaved edits
    /// (R-HTTP-11, D19). Its name and cookie setting stay.
    fn paste_curl(&mut self, text: &str) {
        use tortuga_core::curl::import;

        let pasted = match import::parse(text) {
            Ok(pasted) => pasted,
            Err(e) => {
                self.state
                    .notify(format!("Could not read the curl command: {e}"));
                return;
            }
        };
        let mut request = pasted.request;
        import::use_server(
            &mut request,
            &self.state.request_context(self.selection.as_ref()),
        );
        request.name.clone_from(&self.current.name);
        request.cookies = self.current.cookies;
        let response = std::mem::take(&mut self.response);
        self.load(request);
        self.response = response;
        if !pasted.ignored.is_empty() {
            self.state.notify(format!(
                "Filled in from the curl command. Not used: {}.",
                pasted.ignored.join("; ")
            ));
        }
    }

    /// "Choose…" for a multipart file: inside the project the path is kept relative to
    /// it, so it works for teammates too (R-HTTP-15, D21).
    fn choose_upload(&mut self, root: Option<&Path>, index: usize) {
        let Some(path) = self.state.host.pick_any_file("Choose a file to upload") else {
            return;
        };
        let value = upload_path(&path, root);
        self.editor.set_file(&mut self.current, index, value);
    }

    // ---------------------------------------------------------------- requests

    fn is_dirty(&self, key: &ItemKey, request: &Request) -> bool {
        self.state.saved_request(key).as_ref() != Some(request)
    }

    fn dirty_keys(&self) -> HashSet<ItemKey> {
        let mut dirty: HashSet<ItemKey> = self.drafts.keys().cloned().collect();
        if let Some(k) = &self.selection
            && self.is_dirty(k, &self.current)
        {
            dirty.insert(k.clone());
        }
        dirty
    }

    /// Replaces the edited request from outside the editor.
    fn load(&mut self, request: Request) {
        self.editor = EditorState::new(&request);
        self.current = request;
        self.response = ResponseState::default();
    }

    fn select(&mut self, new: Selection) {
        self.select_and_remember(new, true);
    }

    /// Selects a request; `remember` keeps it as the workspace's selection (R-WS-1).
    fn select_and_remember(&mut self, new: Selection, remember: bool) {
        if remember {
            self.state.remember_selection(new.as_ref());
        }
        if let Some(k) = &new {
            self.remember_recent(k);
        }
        if self.selection == new {
            return;
        }
        let cur = std::mem::take(&mut self.current);
        match &self.selection {
            None => self.scratch = cur,
            Some(k) if self.is_dirty(k, &cur) => drop(self.drafts.insert(k.clone(), cur)),
            Some(k) => drop(self.drafts.remove(k)),
        }
        let next = match &new {
            None => self.scratch.clone(),
            Some(k) => self
                .drafts
                .get(k)
                .cloned()
                .or_else(|| self.state.saved_request(k))
                .unwrap_or_default(),
        };
        self.selection = new;
        self.load(next);
    }

    /// Keys follow files and folders that were renamed or moved.
    fn remap(&mut self, root: &Path, old: &Path, new: &Path) {
        let moved: Vec<_> = self
            .drafts
            .keys()
            .filter_map(|k| Some((k.clone(), k.moved(root, old, new)?)))
            .collect();
        for (from, to) in moved {
            if let Some(v) = self.drafts.remove(&from) {
                self.drafts.insert(to, v);
            }
        }
        let moved: Vec<_> = self
            .outcomes
            .keys()
            .flatten()
            .filter_map(|k| Some((k.clone(), k.moved(root, old, new)?)))
            .collect();
        for (from, to) in moved {
            if let Some(v) = self.outcomes.remove(&Some(from)) {
                self.outcomes.insert(Some(to), v);
            }
        }
        if let Some(f) = &mut self.in_flight
            && let Some(to) = f.key.as_ref().and_then(|k| k.moved(root, old, new))
        {
            f.key = Some(to);
        }
        if let Some(to) = self
            .selection
            .as_ref()
            .and_then(|k| k.moved(root, old, new))
        {
            self.selection = Some(to);
        }
        let moved: Vec<_> = self
            .conflicts
            .keys()
            .filter_map(|k| Some((k.clone(), k.moved(root, old, new)?)))
            .collect();
        for (from, to) in moved {
            if let Some(v) = self.conflicts.remove(&from) {
                self.conflicts.insert(to, v);
            }
        }
    }

    /// Drops state of deleted or closed items (`prefix` "" = the whole collection).
    fn forget(&mut self, root: &Path, prefix: &Path) {
        let gone = |k: &ItemKey| k.root == root && k.path.starts_with(prefix);
        self.drafts.retain(|k, _| !gone(k));
        self.conflicts.retain(|k, _| !gone(k));
        self.outcomes.retain(|k, _| !k.as_ref().is_some_and(gone));
        if self.selection.as_ref().is_some_and(gone) {
            self.selection = None;
            self.load(self.scratch.clone());
        }
    }

    /// The selection whose request is being sent, if any.
    fn sending(&self) -> Option<&Selection> {
        self.in_flight.as_ref().map(|f| &f.key)
    }

    fn send(&mut self, egui_ctx: &egui::Context) {
        if let Some(f) = self.in_flight.take() {
            f.abort.abort();
        }
        let key = self.selection.clone();
        let ctx = self.state.request_context(key.as_ref());
        self.response = ResponseState::default();
        self.forget_past(&key);
        let prepared = match vars::prepare(&self.current, &ctx) {
            Ok(prepared) => prepared,
            Err(e) => {
                self.outcomes
                    .insert(key, Outcome::Error(sentence(&e.to_string())));
                return;
            }
        };
        self.next_send_id += 1;
        let id = self.next_send_id;
        let client = self.state.client.clone();
        let jar = self
            .state
            .cookie_jar(key.as_ref().map(|k| k.root.as_path()));
        let tx = self.finished_tx.clone();
        let repaint = egui_ctx.clone();
        // For the history (R-HIST-1): the request as in the editor, not as resolved.
        let sent_at = tortuga_core::history::now_ms();
        let entry_id = tortuga_core::history::new_id(sent_at, id);
        let path = key.as_ref().map(|k| k.path.clone());
        let request = self.current.clone();
        let environment = ctx.environment.clone();
        let (task, abort) = abortable(async move {
            let result = client.execute_with_cookies(&prepared, Some(&jar)).await;
            // Off the UI thread: copying up to 1 MB of body for the history.
            let entry = tortuga_core::history::Entry::new(
                entry_id,
                sent_at,
                path,
                &request,
                environment,
                result.as_ref().map_err(|e| sentence(&e.to_string())),
            );
            // Prepared here, off the UI thread: pretty-printing big bodies takes a while (R-NF-2).
            let outcome = Outcome::from_result(result);
            // The receiver only goes away with the app.
            let _ = tx.send(Finished {
                id,
                outcome,
                entry: Some(entry),
            });
            repaint.request_repaint();
        });
        self.state.host.spawn(Box::pin(async move {
            // `Err` means it was cancelled (R-HTTP-7); dropping the future closed the connection.
            let _ = task.await;
        }));
        self.in_flight = Some(InFlight { key, id, abort });
    }

    fn cancel(&mut self) {
        if let Some(f) = self.in_flight.take() {
            f.abort.abort();
            // A redirect may already have set cookies.
            self.state
                .save_cookies(f.key.as_ref().map(|k| k.root.as_path()));
            self.outcomes.insert(f.key, Outcome::Cancelled);
        }
    }

    /// Filters the shown JSON response in the background whenever the response or the
    /// filter changes (R-HTTP-13). A result that arrives for an older filter is dropped.
    fn update_filter(&mut self, view: Option<&Arc<ResponseView>>, expr: &str, ctx: &egui::Context) {
        while let Ok(done) = self.filter_rx.try_recv() {
            if let Some(job) = self.filter_job.as_mut().filter(|j| j.id == done.id) {
                job.result = Some(done.result);
            }
        }
        let Some(view) = view.filter(|_| !expr.is_empty()) else {
            if let Some(job) = self.filter_job.take() {
                job.abort.abort();
            }
            return;
        };
        if self
            .filter_job
            .as_ref()
            .is_some_and(|j| Arc::ptr_eq(&j.view, view) && j.expr == expr)
        {
            return;
        }
        if let Some(job) = self.filter_job.take() {
            job.abort.abort();
        }
        self.next_send_id += 1;
        let id = self.next_send_id;
        let (task_view, task_expr) = (view.clone(), expr.to_owned());
        let tx = self.filter_tx.clone();
        let repaint = ctx.clone();
        let (task, abort) = abortable(async move {
            let result = crate::components::filter_body(&task_view, &task_expr);
            // The receiver only goes away with the app.
            let _ = tx.send(FilterDone { id, result });
            repaint.request_repaint();
        });
        self.state.host.spawn(Box::pin(async move {
            let _ = task.await;
        }));
        self.filter_job = Some(FilterJob {
            view: view.clone(),
            expr: expr.to_owned(),
            id,
            result: None,
            abort,
        });
    }

    fn collect_finished(&mut self) {
        while let Ok(done) = self.finished_rx.try_recv() {
            if let Some(f) = self.in_flight.take_if(|f| f.id == done.id) {
                if f.key == self.selection {
                    self.response = ResponseState::default();
                }
                self.state
                    .save_cookies(f.key.as_ref().map(|k| k.root.as_path()));
                if let Some(entry) = done.entry {
                    self.record_history(f.key.as_ref().map(|k| k.root.as_path()), entry);
                }
                self.outcomes.insert(f.key, done.outcome);
            }
        }
    }

    fn save(&mut self) {
        let Some(key) = self.selection.clone() else {
            self.modal = Some(Modal::SaveToCollection(SaveToCollectionDialog::new(
                &self.state,
                &self.current.name,
            )));
            return;
        };
        let request = self.current.clone();
        if let Some(new_path) = self
            .state
            .run(&key.root, |c, s| c.save_request(s, &key.path, &request))
        {
            self.drafts.remove(&key);
            // Saving answers a pending conflict with "keep mine".
            self.conflicts.remove(&key);
            self.remap(&key.root, &key.path, &new_path);
        }
    }

    fn revert(&mut self) {
        if let Some(key) = self.selection.clone() {
            self.drafts.remove(&key);
            self.load(self.state.saved_request(&key).unwrap_or_default());
        }
    }

    // ---------------------------------------------------------------- changes on disk (M6)

    /// A collection changed on disk (git pull, branch switch, another editor): reload
    /// it and bring open requests up to date (R-GIT-10). Requests with unsaved edits
    /// are not overwritten; the user chooses in a banner (R-GIT-11).
    fn on_disk_change(&mut self, root: &Path) {
        let in_root = |k: &ItemKey| k.root == root;
        let mut watched: Vec<ItemKey> =
            self.drafts.keys().filter(|k| in_root(k)).cloned().collect();
        if let Some(k) = self.selection.as_ref().filter(|k| in_root(k))
            && !watched.contains(k)
        {
            watched.push(k.clone());
        }
        let before: Vec<(ItemKey, Option<Request>, bool)> = watched
            .into_iter()
            .map(|k| {
                let saved = self.state.saved_request(&k);
                let dirty = self.drafts.contains_key(&k)
                    || (self.selection.as_ref() == Some(&k)
                        && saved.as_ref() != Some(&self.current));
                (k, saved, dirty)
            })
            .collect();

        self.state.reload(root);
        self.state.refresh_git(root);

        for (key, old, dirty) in before {
            let new = self.state.saved_request(&key);
            if new == old {
                continue;
            }
            if dirty {
                let change = if new.is_some() {
                    DiskChange::Changed
                } else {
                    DiskChange::Deleted
                };
                self.conflicts.insert(key, change);
            } else if self.selection.as_ref() == Some(&key) {
                match new {
                    Some(request) => self.load(request),
                    None => self.forget(&key.root, &key.path),
                }
            }
        }
    }

    /// Answers the conflict banner of the selected request (R-GIT-11, D9).
    fn resolve_conflict(&mut self, key: &ItemKey, choice: ConflictChoice) {
        let Some(change) = self.conflicts.remove(key) else {
            return;
        };
        match (change, choice) {
            // Keep the edits; saving will write them over the new version.
            (DiskChange::Changed, ConflictChoice::KeepMine) => {}
            (DiskChange::Changed, ConflictChoice::TakeTheirs) => {
                self.drafts.remove(key);
                if self.selection.as_ref() == Some(key) {
                    self.load(self.state.saved_request(key).unwrap_or_default());
                }
            }
            // The file is gone: restore it, with the edits, where it was.
            (DiskChange::Deleted, ConflictChoice::KeepMine) => {
                let request = if self.selection.as_ref() == Some(key) {
                    self.current.clone()
                } else {
                    self.drafts.get(key).cloned().unwrap_or_default()
                };
                let parent = key.path.parent().map(Path::to_path_buf).unwrap_or_default();
                if let Some(path) = self
                    .state
                    .run(&key.root, |c, s| c.new_request(s, &parent, &request))
                {
                    self.drafts.remove(key);
                    let restored = ItemKey::new(&key.root, path);
                    if self.selection.as_ref() == Some(key) {
                        self.selection = Some(restored);
                        self.current = request;
                    }
                }
            }
            (DiskChange::Deleted, ConflictChoice::TakeTheirs) => self.forget(&key.root, &key.path),
        }
    }

    /// The banner shown above a request with a pending conflict.
    fn conflict_banner(&mut self, ui: &mut Ui, key: &ItemKey) {
        let Some(change) = self.conflicts.get(key).copied() else {
            return;
        };
        let p = theme::palette(ui);
        let (message, keep, take) = match change {
            DiskChange::Changed => (
                "This request changed on disk (for example after git pull) while you had unsaved edits.",
                "Keep mine",
                "Take theirs",
            ),
            DiskChange::Deleted => (
                "This request was deleted on disk while you had unsaved edits.",
                "Restore with my edits",
                "Discard my edits",
            ),
        };
        let mut choice = None;
        theme::banner_frame(ui, false)
            .corner_radius(egui::CornerRadius::same(8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("⚠").color(p.warning_text));
                    ui.label(RichText::new(message).color(p.warning_text));
                    if ui.button(keep).clicked() {
                        choice = Some(ConflictChoice::KeepMine);
                    }
                    if ui.button(take).clicked() {
                        choice = Some(ConflictChoice::TakeTheirs);
                    }
                });
            });
        ui.add_space(8.0);
        if let Some(choice) = choice {
            self.resolve_conflict(key, choice);
        }
    }

    // ---------------------------------------------------------------- sidebar

    fn on_sidebar(&mut self, action: SidebarAction) {
        match action {
            SidebarAction::Select(key) => self.select(key),
            SidebarAction::NewRequest { root, parent } => {
                let request = Request {
                    name: "New request".into(),
                    ..Request::default()
                };
                if let Some(path) = self
                    .state
                    .run(&root, |c, s| c.new_request(s, &parent, &request))
                {
                    self.select(Some(ItemKey::new(&root, path)));
                }
            }
            SidebarAction::NewFolder { root, parent } => {
                if let Some(path) = self
                    .state
                    .run(&root, |c, s| c.new_folder(s, &parent, "New folder"))
                {
                    let key = ItemKey::new(&root, path);
                    let name = self
                        .state
                        .collection(&root)
                        .and_then(|c| c.find(&key.path).map(Item::name))
                        .unwrap_or_default();
                    self.sidebar.start_rename(key, name);
                }
            }
            SidebarAction::Rename { key, name } => self.rename(&key, name),
            SidebarAction::RenameCollection { root, name } => {
                self.state.run(&root, |c, s| {
                    let manifest = tortuga_core::Manifest {
                        name: name.clone(),
                        ..c.manifest.clone()
                    };
                    c.save_config(s, &manifest, &c.environments)
                });
            }
            SidebarAction::Duplicate(key) => {
                if let Some(path) = self.state.run(&key.root, |c, s| c.duplicate(s, &key.path))
                    && path.extension().is_some_and(|e| e == "toml")
                {
                    self.select(Some(ItemKey::new(&key.root, path)));
                }
            }
            SidebarAction::MoveBy(key, direction) => {
                self.state
                    .run(&key.root, |c, s| c.move_by(s, &key.path, direction));
            }
            SidebarAction::MoveTo(key) => self.modal = Some(Modal::MoveTo(MoveDialog::new(key))),
            SidebarAction::Delete(key) => {
                let dialog = self.confirm_delete(&key);
                self.modal = Some(Modal::ConfirmDelete(key, dialog));
            }
            SidebarAction::ShowError(message) => self.state.notify(message),
            SidebarAction::EditEnvironments(root) => {
                self.modal = Some(Modal::Environments(EnvironmentsDialog::new(
                    &self.state,
                    root,
                )));
            }
            SidebarAction::EditDefaults(root) => {
                self.modal = Some(Modal::Defaults(DefaultsDialog::new(&self.state, root)));
            }
            SidebarAction::EditFolderDefaults(key) => {
                self.modal = Some(Modal::Defaults(DefaultsDialog::for_folder(
                    &self.state,
                    key.root,
                    key.path,
                )));
            }
            SidebarAction::Reload(root) => self.on_disk_change(&root),
            SidebarAction::OpenInFileManager(root) => {
                if let Err(e) = self.state.host.open_in_file_manager(&root) {
                    self.state.notify(e);
                }
            }
            SidebarAction::OpenInTerminal(root) => {
                if let Err(e) = self.state.host.open_in_terminal(&root) {
                    self.state.notify(e);
                }
            }
            SidebarAction::ClearCookies(root) => self.state.clear_cookies(&root),
            SidebarAction::Close(root) => {
                self.forget(&root, Path::new(""));
                self.state.close_collection(&root);
            }
            SidebarAction::SetProjectEnvironment { root, id } => {
                self.state.set_project_environment(&root, &id);
            }
            SidebarAction::NewProject => self.new_project(),
            SidebarAction::AddProjects => self.add_projects(),
            SidebarAction::LocateMissing(path) => self.locate_missing(&path),
            SidebarAction::RemoveMissing(path) => self.state.remove_missing(&path),
        }
    }

    fn rename(&mut self, key: &ItemKey, name: String) {
        let is_folder = self
            .state
            .collection(&key.root)
            .is_some_and(|c| matches!(c.find(&key.path), Some(Item::Folder(_))));
        let new_path = if is_folder {
            self.state
                .run(&key.root, |c, s| c.rename_folder(s, &key.path, &name))
        } else {
            let Some(saved) = self.state.saved_request(key) else {
                return;
            };
            let renamed = Request {
                name: name.clone(),
                ..saved
            };
            self.state
                .run(&key.root, |c, s| c.save_request(s, &key.path, &renamed))
        };
        let Some(new_path) = new_path else {
            return;
        };
        if !is_folder {
            if let Some(d) = self.drafts.get_mut(key) {
                d.name.clone_from(&name);
            }
            if self.selection.as_ref() == Some(key) {
                self.current.name = name;
            }
        }
        self.remap(&key.root, &key.path, &new_path);
    }

    fn confirm_delete(&self, key: &ItemKey) -> ConfirmDialog {
        let item = self
            .state
            .collection(&key.root)
            .and_then(|c| c.find(&key.path));
        let name = item.map(Item::name).unwrap_or_default();
        let message = if matches!(item, Some(Item::Folder(_))) {
            format!("“{name}” and everything in it will be deleted from disk.")
        } else {
            format!("“{name}” will be deleted from disk.")
        };
        ConfirmDialog {
            title: "Delete?".into(),
            message,
            confirm_label: "Delete".into(),
        }
    }

    /// R-WS-7: pick (or make) a folder, then name the project there.
    fn new_project(&mut self) {
        let Some(dir) = self
            .state
            .host
            .pick_folder("Choose or create a folder for the new project")
        else {
            return;
        };
        self.create_project_in(dir);
    }

    fn create_project_in(&mut self, dir: PathBuf) {
        let manifest = dir.join(tortuga_core::collection::MANIFEST_FILE);
        if self.state.storage().exists(&manifest) {
            // Already a project: just add it.
            if let Err(e) = self.state.open_collection(&dir) {
                self.state.notify(e.to_string());
            }
            return;
        }
        let repository = self.state.host.repository_of(&dir);
        let can_init = self.state.host.can_init_git();
        self.modal = Some(Modal::CreateProject(CreateProjectDialog::new(
            dir, repository, can_init,
        )));
    }

    /// R-WS-3: pick a folder or repository and list the projects in it.
    fn add_projects(&mut self) {
        let Some(dir) = self
            .state
            .host
            .pick_folder("Add the projects in a folder or repository")
        else {
            return;
        };
        let (tx, rx) = channel();
        let wake = self.state.waker();
        self.state.host.find_projects(
            &dir,
            Box::new(move |result| {
                let _ = tx.send(result);
                wake();
            }),
        );
        self.modal = Some(Modal::AddProjects(AddProjectsDialog::new(dir, rx)));
    }

    /// R-WS-5: the user points at the folder where a missing project is now.
    fn locate_missing(&mut self, old: &Path) {
        let name = old
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let Some(dir) = self.state.host.pick_folder(&format!("Locate “{name}”")) else {
            return;
        };
        if !self
            .state
            .storage()
            .exists(&dir.join(tortuga_core::collection::MANIFEST_FILE))
        {
            self.state.notify(format!(
                "{} is not a Tortuga project (it has no tortuga.toml).",
                dir.display()
            ));
            return;
        }
        if let Err(e) = self.state.locate_missing(old, &dir) {
            self.state.notify(e.to_string());
        }
    }

    // ---------------------------------------------------------------- dialogs

    fn show_modal(&mut self, ctx: &egui::Context) {
        let Some(mut modal) = self.modal.take() else {
            return;
        };
        let keep = match &mut modal {
            Modal::Settings(d) => match d.show(ctx) {
                DialogResult::Done(settings) => match self.state.apply_settings(&settings) {
                    Ok(()) => false,
                    Err(e) => {
                        d.error = Some(e);
                        true
                    }
                },
                result => result == DialogResult::Open,
            },
            Modal::Environments(d) => match d.show(ctx, &self.state) {
                DialogResult::Done(saved) => !self.save_environments(d, saved),
                result => result == DialogResult::Open,
            },
            Modal::Defaults(d) => match d.show(ctx, &self.state) {
                DialogResult::Done(defaults) => {
                    !self.save_defaults(&d.root.clone(), d.folder.clone().as_deref(), &defaults)
                }
                result => result == DialogResult::Open,
            },
            Modal::ConfirmDelete(key, d) => match d.show(ctx) {
                DialogResult::Done(()) => {
                    if self
                        .state
                        .run(&key.root, |c, s| c.delete(s, &key.path))
                        .is_some()
                    {
                        self.forget(&key.root, &key.path);
                    }
                    false
                }
                result => result == DialogResult::Open,
            },
            Modal::MoveTo(d) => match d.show(ctx, &self.state) {
                DialogResult::Done(dest) => {
                    let key = d.item.clone();
                    if let Some(new_path) = self
                        .state
                        .run(&key.root, |c, s| c.move_to(s, &key.path, &dest))
                    {
                        self.remap(&key.root, &key.path, &new_path);
                    }
                    false
                }
                result => result == DialogResult::Open,
            },
            Modal::SaveToCollection(d) => match d.show(ctx, &self.state) {
                DialogResult::Done((folder, name)) => {
                    let request = Request {
                        name,
                        ..self.current.clone()
                    };
                    if let Some(path) = self.state.run(&folder.root, |c, s| {
                        c.new_request(s, &folder.path, &request)
                    }) {
                        // The scratch request starts over; the saved copy is selected.
                        self.current = Request::default();
                        self.select(Some(ItemKey::new(&folder.root, path)));
                    }
                    false
                }
                result => result == DialogResult::Open,
            },
            Modal::CopyCurl(d) => match d.show(ctx) {
                DialogResult::Done(text) => {
                    ctx.copy_text(text);
                    self.copied_at = Some(ctx.input(|i| i.time));
                    false
                }
                result => result == DialogResult::Open,
            },
            Modal::Palette(palette) => self.show_palette(ctx, palette),
            Modal::ClearHistory(target, d) => match d.show(ctx) {
                DialogResult::Done(()) => {
                    self.clear_history(target);
                    false
                }
                result => result == DialogResult::Open,
            },
            Modal::CreateProject(_)
            | Modal::AddProjects(_)
            | Modal::NewWorkspace(_)
            | Modal::RenameWorkspace(_)
            | Modal::RemoveWorkspace(_) => self.show_workspace_modal(ctx, &mut modal),
        };
        // A dialog may have opened another one ("Create a project there…"); never overwrite it.
        if keep && self.modal.is_none() {
            self.modal = Some(modal);
        }
    }
}

impl TortugaUi {
    /// Dialogs about projects and workspaces (R-WS-1, 3, 5, 7). Returns whether it stays open.
    fn show_workspace_modal(&mut self, ctx: &egui::Context, modal: &mut Modal) -> bool {
        match modal {
            Modal::CreateProject(d) => match d.show(ctx) {
                DialogResult::Done((name, init_git)) => {
                    if init_git && let Err(e) = self.state.host.init_git(&d.dir) {
                        self.state.notify(e);
                    }
                    if let Err(e) = self.state.create_collection(&d.dir, &name) {
                        self.state.notify(e.to_string());
                    }
                    false
                }
                result => result == DialogResult::Open,
            },
            Modal::AddProjects(d) => match d.show(ctx, &self.state) {
                DialogResult::Done(AddProjects::Add(paths)) => {
                    for path in paths {
                        if let Err(e) = self.state.open_collection(&path) {
                            self.state.notify(e.to_string());
                        }
                    }
                    false
                }
                DialogResult::Done(AddProjects::CreateHere(dir)) => {
                    self.create_project_in(dir);
                    false
                }
                result => result == DialogResult::Open,
            },
            Modal::NewWorkspace(d) => match d.show(ctx) {
                DialogResult::Done(name) => {
                    self.switch_workspace(|state| state.new_workspace(&name));
                    false
                }
                result => result == DialogResult::Open,
            },
            Modal::RenameWorkspace(d) => match d.show(ctx) {
                DialogResult::Done(name) => {
                    self.state.rename_workspace(&name);
                    false
                }
                result => result == DialogResult::Open,
            },
            Modal::RemoveWorkspace(d) => match d.show(ctx) {
                DialogResult::Done(()) => {
                    self.switch_workspace(AppState::delete_workspace);
                    false
                }
                result => result == DialogResult::Open,
            },
            _ => false,
        }
    }

    /// Saves the environments dialog. Secrets first: if the credential store fails,
    /// nothing changes on disk and the dialog stays open with the error. Returns success.
    /// Saves project defaults (R-COL-4) or folder defaults (R-COL-11). Returns whether it worked.
    fn save_defaults(
        &mut self,
        root: &Path,
        folder: Option<&Path>,
        defaults: &tortuga_core::format::FolderDefaults,
    ) -> bool {
        self.state
            .run(root, |c, s| {
                if let Some(folder) = folder {
                    return c.save_folder_defaults(s, folder, defaults);
                }
                let manifest = tortuga_core::Manifest {
                    headers: defaults.headers.clone(),
                    auth: defaults.auth.clone(),
                    ..c.manifest.clone()
                };
                c.save_config(s, &manifest, &c.environments)
            })
            .is_some()
    }

    fn save_environments(
        &mut self,
        d: &mut EnvironmentsDialog,
        saved: crate::components::SavedConfig,
    ) -> bool {
        if let Err(e) = self.state.save_secrets(&d.root, saved.secrets) {
            d.error = Some(e);
            return false;
        }
        self.state
            .run(&d.root, |c, s| {
                c.save_config(s, &saved.manifest, &saved.environments)
            })
            .is_some()
    }
}

/// "Copy as curl" (R-HTTP-9); shows "Copied" for a moment after `copied_at`. Returns whether it was clicked.
fn copy_curl_button(ui: &mut Ui, copied_at: Option<f64>) -> bool {
    const FEEDBACK_SECONDS: f64 = 2.0;
    let now = ui.input(|i| i.time);
    let copied = copied_at.is_some_and(|t| now - t < FEEDBACK_SECONDS);
    if copied {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_secs_f64(FEEDBACK_SECONDS));
    }
    let label = if copied { "Copied" } else { "Copy as curl" };
    ui.add(theme::ghost_button(label))
        .on_hover_text("Copy this request as a curl command, with variables filled in")
        .clicked()
}

/// How a chosen file is saved in the request (D21): relative to the project folder
/// (with `/`, the same on every OS) when inside it, else the full path.
fn upload_path(path: &Path, root: Option<&Path>) -> String {
    match root.and_then(|r| path.strip_prefix(r).ok()) {
        Some(relative) => relative
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/"),
        None => path.to_string_lossy().into_owned(),
    }
}

fn breadcrumb(state: &AppState, key: &ItemKey) -> String {
    let Some(c) = state.collection(&key.root) else {
        return String::new();
    };
    let mut parts = vec![c.name().to_owned()];
    let mut path = PathBuf::new();
    if let Some(parent) = key.path.parent() {
        for comp in parent.components() {
            path.push(comp);
            if let Some(item) = c.find(&path) {
                parts.push(item.name());
            }
        }
    }
    parts.join(" / ") + " /"
}

#[cfg(test)]
mod tests {
    use tortuga_core::Collection;
    use tortuga_core::storage::FsStorage;

    use super::*;
    use crate::test_host::TestHost;

    type Shared<T> = std::rc::Rc<std::cell::RefCell<T>>;

    /// A collection on disk with one request, opened in a fresh UI with that request selected.
    fn setup(name: &str) -> (TortugaUi, PathBuf, ItemKey) {
        let (ui, root, key, _, _) = setup_shared(name);
        (ui, root, key)
    }

    /// Like [`setup`], plus the host's saved local state and unsaved edits, to start again.
    fn setup_shared(
        name: &str,
    ) -> (
        TortugaUi,
        PathBuf,
        ItemKey,
        Shared<Option<crate::host::LocalState>>,
        Shared<SavedDrafts>,
    ) {
        let root = std::env::temp_dir().join(format!("tortuga-disk-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let collection = Collection::create(&FsStorage, &root, "C").expect("create");
        let mut request = Request::new(tortuga_core::HttpMethod::Get, "https://a.test/old");
        request.name = "Get user".into();
        let path = collection
            .new_request(&FsStorage, Path::new(""), &request)
            .expect("request");
        let host = TestHost {
            open: vec![root.clone()],
            ..TestHost::default()
        };
        let (local, drafts) = (host.local.clone(), host.drafts.clone());
        let mut ui = TortugaUi::new(Box::new(host));
        let key = ItemKey::new(&root, path);
        ui.select(Some(key.clone()));
        (ui, root, key, local, drafts)
    }

    /// Simulates a teammate's change arriving through `git pull`.
    fn change_on_disk(root: &Path, key: &ItemKey, url: &str) {
        let mut request = Request::new(tortuga_core::HttpMethod::Get, url);
        request.name = "Get user".into();
        std::fs::write(
            root.join(&key.path),
            tortuga_core::format::write_request(&request, Some(100)),
        )
        .expect("write");
    }

    /// R-GIT-10: without unsaved edits, the open request shows the new version.
    #[test]
    fn requests_without_edits_follow_the_disk() {
        let (mut ui, root, key) = setup("follow");
        change_on_disk(&root, &key, "https://a.test/new");
        ui.on_disk_change(&root);
        assert_eq!(ui.current.url, "https://a.test/new");
        assert!(ui.conflicts.is_empty());
        std::fs::remove_dir_all(root).expect("clean up");
    }

    /// R-GIT-11: with unsaved edits, the user chooses: take theirs, or keep mine and save.
    #[test]
    fn unsaved_edits_are_not_overwritten() {
        let (mut ui, root, key) = setup("conflict");
        ui.current.set_display_url("https://a.test/mine");
        change_on_disk(&root, &key, "https://a.test/theirs");
        ui.on_disk_change(&root);
        assert_eq!(ui.current.url, "https://a.test/mine", "edits kept");
        assert_eq!(ui.conflicts.get(&key), Some(&DiskChange::Changed));

        ui.resolve_conflict(&key, ConflictChoice::TakeTheirs);
        assert_eq!(ui.current.url, "https://a.test/theirs");
        assert!(ui.conflicts.is_empty());

        ui.current.set_display_url("https://a.test/mine-again");
        change_on_disk(&root, &key, "https://a.test/theirs-again");
        ui.on_disk_change(&root);
        ui.resolve_conflict(&key, ConflictChoice::KeepMine);
        ui.save();
        let on_disk = std::fs::read_to_string(root.join(&key.path)).expect("read");
        assert!(on_disk.contains("mine-again"), "{on_disk}");
        std::fs::remove_dir_all(root).expect("clean up");
    }

    /// R-COL-8: unsaved edits (and the scratch request) are back after a restart.
    #[test]
    fn unsaved_edits_survive_a_restart() {
        let (mut ui, root, key, local, drafts) = setup_shared("restart");
        ui.current.set_display_url("https://a.test/unsaved");
        ui.select(None);
        ui.current.set_display_url("https://scratch.test/");
        ui.select(Some(key.clone()));
        ui.on_exit();

        let mut again = TortugaUi::new(Box::new(TestHost {
            local,
            drafts: drafts.clone(),
            ..TestHost::default()
        }));
        assert_eq!(again.selection, Some(key.clone()), "selection restored");
        assert_eq!(again.current.url, "https://a.test/unsaved");
        assert!(again.dirty_keys().contains(&key), "still marked unsaved");
        assert!(again.conflicts.is_empty());
        again.select(None);
        assert_eq!(again.current.url, "https://scratch.test/");

        // Saved: nothing is kept for next time any more.
        again.select(Some(key.clone()));
        again.save();
        again.on_exit();
        assert_eq!(drafts.borrow().drafts.len(), 0);
        std::fs::remove_dir_all(root).expect("clean up");
    }

    /// R-COL-8 + R-GIT-11: a file that changed while the app was closed gets the banner.
    #[test]
    fn edits_of_a_file_changed_while_closed_conflict() {
        let (mut ui, root, key, local, drafts) = setup_shared("closed");
        ui.current.set_display_url("https://a.test/mine");
        ui.on_exit();
        change_on_disk(&root, &key, "https://a.test/theirs");

        let again = TortugaUi::new(Box::new(TestHost {
            local,
            drafts,
            ..TestHost::default()
        }));
        assert_eq!(again.current.url, "https://a.test/mine", "edits kept");
        assert_eq!(again.conflicts.get(&key), Some(&DiskChange::Changed));
        std::fs::remove_dir_all(root).expect("clean up");
    }

    /// R-HTTP-9: without secrets the command is copied right away; with secrets the
    /// user chooses first.
    #[test]
    fn copy_as_curl_asks_only_when_secrets_are_used() {
        let (mut ui, root, _) = setup("curl");
        let egui_ctx = egui::Context::default();
        ui.copy_as_curl(&egui_ctx);
        assert!(ui.copied_at.is_some(), "copied without asking");
        assert!(ui.modal.is_none());

        ui.current.headers = vec![tortuga_core::KeyValue::new("X-Key", "{{api_key}}")];
        let mut secrets = tortuga_core::secrets::SecretValues::default();
        secrets
            .values
            .insert((None, "api_key".into()), "k-123".into());
        ui.state.run(&root, |c, s| {
            let manifest = tortuga_core::Manifest {
                secrets: vec!["api_key".into()],
                ..c.manifest.clone()
            };
            c.save_config(s, &manifest, &c.environments)
        });
        ui.state.save_secrets(&root, secrets).expect("secrets");
        ui.copy_as_curl(&egui_ctx);
        assert!(matches!(ui.modal, Some(Modal::CopyCurl(_))), "asks first");
        std::fs::remove_dir_all(root).expect("clean up");
    }

    /// R-WS-1: switching workspaces keeps unsaved edits for the session and brings
    /// back each workspace's selected request.
    #[test]
    fn switching_workspaces_keeps_edits_and_selection() {
        let (mut ui, root, key) = setup("switch");
        ui.current.set_display_url("https://a.test/unsaved");
        ui.switch_workspace(|state| state.new_workspace("Other"));
        assert_eq!(
            ui.selection, None,
            "the new workspace starts on the scratch request"
        );
        assert_eq!(ui.state.collections.len(), 0);

        ui.switch_workspace(|state| state.switch_workspace(0));
        assert_eq!(ui.selection, Some(key));
        assert_eq!(ui.current.url, "https://a.test/unsaved", "edits kept");
        std::fs::remove_dir_all(root).expect("clean up");
    }

    /// R-WS-1, R-WS-4, R-WS-5: the top bar names the workspace; the sidebar lists its
    /// projects and the ones that are missing on this machine.
    #[test]
    fn sidebar_shows_projects_and_missing_ones() {
        use egui_kittest::kittest::Queryable;

        let (_, root, _) = setup("sidebar");
        let gone = root.with_file_name("tortuga-not-cloned-yet");
        let mut ui = TortugaUi::new(Box::new(TestHost {
            open: vec![root.clone(), gone],
            ..TestHost::default()
        }));
        let mut harness = crate::test_host::harness(move |egui_ui| ui.ui(egui_ui));
        harness.run();
        harness.get_by_label_contains("My workspace");
        harness.get_by_label_contains("tortuga-not-cloned-yet");
        std::fs::remove_dir_all(root).expect("clean up");
    }

    /// R-NF-1: only the rows in view are built, so big projects cost little per frame.
    #[test]
    fn big_projects_build_only_the_rows_in_view() {
        use egui_kittest::kittest::Queryable;

        let root = std::env::temp_dir().join(format!("tortuga-big-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let collection = Collection::create(&FsStorage, &root, "Big").expect("create");
        for i in 0..300 {
            let mut request = Request::new(tortuga_core::HttpMethod::Get, "https://a.test/");
            request.name = format!("Request {i:03}");
            std::fs::write(
                root.join(format!("request-{i:03}.toml")),
                tortuga_core::format::write_request(&request, Some(i)),
            )
            .expect("write");
        }
        drop(collection);
        let mut ui = TortugaUi::new(Box::new(TestHost {
            open: vec![root.clone()],
            ..TestHost::default()
        }));
        let mut harness = crate::test_host::harness(move |egui_ui| ui.ui(egui_ui));
        harness.run();
        harness.get_by_label_contains("Request 000");
        assert!(
            harness.query_by_label_contains("Request 299").is_none(),
            "rows out of view are not built"
        );
        std::fs::remove_dir_all(root).expect("clean up");
    }

    /// R-HTTP-11, D19: pasting a curl command into the URL field fills the request as unsaved edits.
    #[test]
    fn pasting_curl_fills_the_request() {
        let (ui, root, key) = setup("paste-curl");
        let ui = std::rc::Rc::new(std::cell::RefCell::new(ui));
        let shared = ui.clone();
        let mut harness = crate::test_host::harness(move |egui_ui| shared.borrow_mut().ui(egui_ui));
        harness.run();
        // The URL field has focus when a request is opened.
        harness.event(egui::Event::Paste(
            "curl -X POST https://api.test/users -H 'X-Trace: 1' --json '{\"a\":1}' -k".into(),
        ));
        harness.run();
        drop(harness);
        let ui = ui.borrow();
        assert_eq!(ui.current.method, tortuga_core::HttpMethod::Post);
        assert_eq!(ui.current.url, "https://api.test/users");
        assert_eq!(ui.current.name, "Get user", "the name stays");
        assert!(ui.is_dirty(&key, &ui.current), "unsaved edits");
        assert!(
            ui.state
                .notice
                .as_deref()
                .is_some_and(|n| n.contains("--insecure")),
            "{:?}",
            ui.state.notice
        );
        std::fs::remove_dir_all(root).expect("clean up");
    }

    /// R-COL-9, R-COL-10: Ctrl+P finds a request by name; Ctrl+K runs a command.
    #[test]
    fn quick_open_and_commands() {
        use egui_kittest::kittest::Queryable;

        let (mut ui, root, key) = setup("quick-open");
        ui.select(None);
        let ui = std::rc::Rc::new(std::cell::RefCell::new(ui));
        let shared = ui.clone();
        let mut harness = crate::test_host::harness(move |egui_ui| shared.borrow_mut().ui(egui_ui));
        harness.run();
        harness.key_press_modifiers(Modifiers::COMMAND, Key::P);
        harness.run();
        // The palette's field has focus.
        harness.event(egui::Event::Text("get usr".into()));
        harness.run();
        harness.key_press(Key::Enter);
        harness.run();
        assert_eq!(ui.borrow().selection, Some(key.clone()));
        assert!(ui.borrow().modal.is_none(), "closed after opening");

        harness.key_press_modifiers(Modifiers::COMMAND, Key::K);
        harness.run();
        harness.get_by_label_contains("Quick open: go to a request");
        // The palette's field has focus.
        harness.event(egui::Event::Text("settings".into()));
        harness.run();
        harness.key_press(Key::Enter);
        harness.run();
        assert!(matches!(ui.borrow().modal, Some(Modal::Settings(_))));
        drop(harness);
        std::fs::remove_dir_all(root).expect("clean up");
    }

    /// R-HIST-1..3: a finished send is kept; opening it shows that response and can
    /// restore that version of the request; history can be cleared.
    #[test]
    fn history_records_opens_restores_and_clears() {
        use egui_kittest::kittest::Queryable;
        use tortuga_core::history::Entry;

        let (ui, root, key) = setup("history");
        let ui = std::rc::Rc::new(std::cell::RefCell::new(ui));
        let shared = ui.clone();
        let mut harness = crate::test_host::harness(move |egui_ui| shared.borrow_mut().ui(egui_ui));
        harness.run();

        // Send "v1", and let it finish with a 200.
        let sent_url = "https://a.test/v1";
        {
            let mut ui = ui.borrow_mut();
            ui.current.set_display_url(sent_url);
            let ctx = harness.ctx.clone();
            ui.send(&ctx);
            let id = ui.in_flight.as_ref().expect("in flight").id;
            let response = tortuga_core::Response {
                status: 201,
                status_text: "Created".into(),
                headers: Vec::new(),
                body: b"first".to_vec(),
                elapsed: std::time::Duration::from_millis(3),
            };
            let entry = Entry::new(
                tortuga_core::history::new_id(1_000, 1),
                1_000,
                Some(key.path.clone()),
                &ui.current,
                None,
                Ok(&response),
            );
            ui.finished_tx
                .send(Finished {
                    id,
                    outcome: Outcome::from_result(Ok(response)),
                    entry: Some(entry),
                })
                .expect("send");
            // Then edit the request further.
            ui.current.set_display_url("https://a.test/v2");
            ui.sidebar_view = SidebarView::History;
        }
        harness.run();
        harness
            .get(
                egui_kittest::kittest::by()
                    .label_contains("201")
                    .role(egui::accesskit::Role::Button),
            )
            .click();
        harness.run();
        {
            let ui = ui.borrow();
            assert_eq!(ui.selection, Some(key.clone()));
            assert!(ui.history.past.is_some(), "a past response is shown");
        }
        harness.get_by_label_contains("From history");
        harness.get_by_label("Restore request").click();
        harness.run();
        assert_eq!(
            ui.borrow().current.url,
            sent_url,
            "that version is back, unsaved"
        );

        ui.borrow_mut()
            .clear_history(&ClearHistory::One(Some(root.clone()), "C".into()));
        harness.run();
        harness.get_by_label_contains("Requests you send appear here");
        drop(harness);
        std::fs::remove_dir_all(root).expect("clean up");
    }

    /// R-COL-11: a request in a folder uses the folder's variables and headers.
    #[test]
    fn requests_inherit_folder_defaults() {
        use egui_kittest::kittest::Queryable;
        use tortuga_core::format::FolderDefaults;

        let root =
            std::env::temp_dir().join(format!("tortuga-folder-defaults-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let col = Collection::create(&FsStorage, &root, "C").expect("create");
        let folder = col
            .new_folder(&FsStorage, Path::new(""), "Admin")
            .expect("folder");
        let col = Collection::open(&FsStorage, &root).expect("open");
        col.save_folder_defaults(
            &FsStorage,
            &folder,
            &FolderDefaults {
                headers: vec![tortuga_core::KeyValue::new("X-Area", "admin")],
                variables: [("tenant".to_owned(), "acme".to_owned())].into(),
                ..FolderDefaults::default()
            },
        )
        .expect("defaults");
        let col = Collection::open(&FsStorage, &root).expect("open");
        let mut request = Request::new(tortuga_core::HttpMethod::Get, "https://{{tenant}}.test/");
        request.name = "Users".into();
        let path = col
            .new_request(&FsStorage, &folder, &request)
            .expect("request");
        let mut ui = TortugaUi::new(Box::new(TestHost {
            open: vec![root.clone()],
            ..TestHost::default()
        }));
        let key = ItemKey::new(&root, path);
        ui.select(Some(key.clone()));
        let ctx = ui.state.request_context(Some(&key));
        assert!(
            vars::undefined_in(&ui.current, &ctx).is_empty(),
            "{{{{tenant}}}} comes from the folder"
        );

        let mut harness = crate::test_host::harness(move |egui_ui| ui.ui(egui_ui));
        harness.run();
        assert!(harness.query_by_label_contains("Undefined:").is_none());
        harness.get_by_label_contains("Headers").click();
        harness.run();
        harness.get_by_label("FROM FOLDER “ADMIN”");
        drop(harness);
        std::fs::remove_dir_all(root).expect("clean up");
    }

    /// D21: files inside the project are saved relative to it.
    #[test]
    fn upload_paths_are_relative_inside_the_project() {
        let root = Path::new("/repo/api");
        assert_eq!(
            upload_path(&root.join("fixtures").join("me.png"), Some(root)),
            "fixtures/me.png"
        );
        assert_eq!(
            upload_path(Path::new("/home/me/x.bin"), Some(root)),
            "/home/me/x.bin"
        );
        assert_eq!(
            upload_path(Path::new("/home/me/x.bin"), None),
            "/home/me/x.bin"
        );
    }

    /// R-GIT-11: a request deleted on disk while edited can be restored with the edits.
    #[test]
    fn deleted_request_with_edits_can_be_restored() {
        let (mut ui, root, key) = setup("deleted");
        ui.current.set_display_url("https://a.test/mine");
        std::fs::remove_file(root.join(&key.path)).expect("delete");
        ui.on_disk_change(&root);
        assert_eq!(ui.conflicts.get(&key), Some(&DiskChange::Deleted));

        ui.resolve_conflict(&key, ConflictChoice::KeepMine);
        let restored = ui.selection.clone().expect("still selected");
        let saved = ui.state.saved_request(&restored).expect("restored on disk");
        assert_eq!(saved.url, "https://a.test/mine");
        std::fs::remove_dir_all(root).expect("clean up");
    }
}
