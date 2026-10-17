//! Top-level layout and the request workflow: selection, unsaved drafts,
//! sending, saving, and the dialogs.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};

use egui::{
    CentralPanel, ComboBox, Frame, Key, KeyboardShortcut, Margin, Modifiers, Panel, RichText,
    TextEdit, Ui,
};
use futures_util::future::{AbortHandle, abortable};
use tortuga_core::{Item, Request, vars};

use crate::components::{
    AddProjects, AddProjectsDialog, ConfirmDialog, CreateProjectDialog, DefaultsDialog,
    DialogResult, EditorAction, EditorContext, EditorState, EnvironmentsDialog, MoveDialog,
    NameDialog, Outcome, ResponseAction, ResponseState, SaveToCollectionDialog, SettingsDialog,
    SidebarAction, SidebarState, request_editor, response_panel, sidebar,
};
use crate::format::sentence;
use crate::host::Host;
use crate::state::{AppState, ItemKey};
use crate::theme;

const SAVE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::S);
const SEND: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Enter);

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
}

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
        };
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
        for root in self.state.take_changes() {
            self.on_disk_change(&root);
        }
        if self.modal.is_none() {
            let (save, send) =
                ui.input_mut(|i| (i.consume_shortcut(&SAVE), i.consume_shortcut(&SEND)));
            if save {
                self.save();
            }
            if send && self.sending().is_none() {
                self.send(ui.ctx());
            }
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
                sidebar(
                    ui,
                    &self.state,
                    &mut self.sidebar,
                    self.selection.as_ref(),
                    &dirty,
                    &conflicts,
                )
            })
            .inner;
        for action in actions {
            self.on_sidebar(action);
        }

        CentralPanel::default()
            .frame(
                Frame::new()
                    .fill(p.bg)
                    .inner_margin(Margin::symmetric(20, 16)),
            )
            .show(ui, |ui| self.workspace(ui));

        self.show_modal(ui.ctx());
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
        let ctx = self.state.context(root.as_deref());
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
        let inherited_headers: Vec<tortuga_core::KeyValue> =
            ctx.inherited_headers(&self.current).cloned().collect();
        let cookie_count = self.state.cookie_jar(root.as_deref()).len();
        let sending = self.sending() == Some(&key);
        let default_timeout_ms = self.state.settings.default_timeout_ms;
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
                        cookie_count,
                        in_collection: root.is_some(),
                    },
                )
            })
            .inner;
        match action {
            Some(EditorAction::Send) => self.send(ui.ctx()),
            Some(EditorAction::Cancel) => self.cancel(),
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
        let can_save = self.state.host.can_save_files();
        let outcome = self.outcomes.get(&key);
        if let Some(ResponseAction::SaveBody) =
            response_panel(ui, outcome, sending, &mut self.response, can_save)
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
                },
            );
            ui.add_space(8.0);
            return;
        };
        let crumb = breadcrumb(&self.state, k);
        let ((), (save, revert)) = egui::Sides::new().height(46.0).shrink_left().show(
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
                (save, revert)
            },
        );
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
        let ctx = self.state.context(key.as_ref().map(|k| k.root.as_path()));
        self.response = ResponseState::default();
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
        let (task, abort) = abortable(async move {
            let result = client.execute_with_cookies(&prepared, Some(&jar)).await;
            // Prepared here, off the UI thread: pretty-printing big bodies takes a while (R-NF-2).
            let outcome = Outcome::from_result(result);
            // The receiver only goes away with the app.
            let _ = tx.send(Finished { id, outcome });
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

    fn collect_finished(&mut self) {
        while let Ok(done) = self.finished_rx.try_recv() {
            if let Some(f) = self.in_flight.take_if(|f| f.id == done.id) {
                if f.key == self.selection {
                    self.response = ResponseState::default();
                }
                self.state
                    .save_cookies(f.key.as_ref().map(|k| k.root.as_path()));
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
                DialogResult::Done((headers, auth)) => self
                    .state
                    .run(&d.root, |c, s| {
                        let manifest = tortuga_core::Manifest {
                            headers: headers.clone(),
                            auth: auth.clone(),
                            ..c.manifest.clone()
                        };
                        c.save_config(s, &manifest, &c.environments)
                    })
                    .is_none(),
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

    /// A collection on disk with one request, opened in a fresh UI with that request selected.
    fn setup(name: &str) -> (TortugaUi, PathBuf, ItemKey) {
        let root = std::env::temp_dir().join(format!("tortuga-disk-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let collection = Collection::create(&FsStorage, &root, "C").expect("create");
        let mut request = Request::new(tortuga_core::HttpMethod::Get, "https://a.test/old");
        request.name = "Get user".into();
        let path = collection
            .new_request(&FsStorage, Path::new(""), &request)
            .expect("request");
        let mut ui = TortugaUi::new(Box::new(TestHost {
            open: vec![root.clone()],
            ..TestHost::default()
        }));
        let key = ItemKey::new(&root, path);
        ui.select(Some(key.clone()));
        (ui, root, key)
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
