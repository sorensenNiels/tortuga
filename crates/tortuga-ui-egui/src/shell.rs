//! Top-level layout and the request workflow: selection, the request editor and
//! response area, saving. Each other area is a submodule with its own `impl TortugaUi`
//! block: unsaved edits (`drafts`), sending (`sending`), changes on disk (`disk`),
//! sidebar actions (`projects`), dialogs (`dialogs`), tabs, history, workflows, ….

mod appearance;
mod auto_save;
mod commands;
mod cookies;
mod dialogs;
mod disk;
mod drafts;
mod editor_actions;
mod fanout;
mod history;
mod import;
mod layout;
mod mock;
mod projects;
mod request_head;
mod response;
mod sending;
mod shortcuts;
pub(crate) use shortcuts::app_keys;
mod tabs;
mod toast;
mod updates;
mod workflows;
mod workspace_menu;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

use egui::{CentralPanel, Frame, Margin, Panel, RichText, Ui};
use futures_util::future::AbortHandle;
use tortuga_core::script::Breakpoints;
use tortuga_core::vars::Origin;
use tortuga_core::{Request, vars, workspace};

use crate::components::script_keys;
use crate::components::{
    AddProjectsDialog, ChangesDialog, ClearHistory, ConfirmDialog, CookiesDialog, CopyCurlDialog,
    CopyToProjectDialog, CreateProjectDialog, DefaultsDialog, EditorContext, EditorState,
    EnvironmentsDialog, Marks, MoveDialog, NameDialog, Outcome, Palette, Rerun, ResponseState,
    ResponseView, SaveToCollectionDialog, SettingsDialog, SidebarState, TabStripState, TestsView,
    TextBody, request_editor, sidebar,
};
use crate::format::sentence;
use crate::host::{Host, SavedDrafts};
use crate::state::{AppState, ItemKey};
use crate::theme;
use history::{HistoryState, SidebarView};
use layout::{Arrangement, Split};
use tabs::Tabs;
use workflows::{WorkflowEvent, Workflows};

use shortcuts::{
    CLOSE_TAB, COMMANDS, COMMANDS_TOO, ENVIRONMENT, FIND, FOCUS_URL, GO_TO_TAB, NEXT_TAB,
    PREVIOUS_TAB, QUICK_OPEN, SAVE, SEND, SEND_EVERYWHERE, SHORTCUTS, TAB_BEFORE, TOGGLE_SIDEBAR,
};

#[derive(Debug)]
enum Modal {
    Settings(Box<SettingsDialog>),
    Environments(EnvironmentsDialog),
    Defaults(DefaultsDialog),
    ConfirmDelete(ItemKey, ConfirmDialog),
    MoveTo(MoveDialog),
    CopyTo(CopyToProjectDialog),
    SaveToCollection(SaveToCollectionDialog),
    CreateProject(CreateProjectDialog),
    AddProjects(AddProjectsDialog),
    NewWorkspace(NameDialog),
    RenameWorkspace(NameDialog),
    RemoveWorkspace(ConfirmDialog),
    /// The workspace's variables (R-WS-8).
    WorkspaceVars(crate::components::WorkspaceVarsDialog),
    CopyCurl(CopyCurlDialog),
    /// Quick open and the command palette (R-COL-9, R-COL-10).
    Palette(Palette),
    ClearHistory(ClearHistory, ConfirmDialog),
    /// Workflows (R-FLOW-2) and runtime variables (R-TEST-5).
    NewWorkflow(PathBuf, NameDialog),
    RenameWorkflow(crate::state::WorkflowKey, NameDialog),
    DeleteWorkflow(crate::state::WorkflowKey, ConfirmDialog),
    /// Keep a project's run as a workflow file (R-FLOW-7).
    SaveRunAs(PathBuf, NameDialog),
    RuntimeVars(crate::components::RuntimeVarsDialog),
    /// The cookie viewer (R-CK-5).
    Cookies(CookiesDialog),
    /// A project's mock server: port and log (R-MOCK-2, R-MOCK-4).
    Mock(crate::components::MockDialog),
    /// Reviewing an OpenAPI or Postman import (R-COL-15, R-COL-16).
    Import(Box<import::OpenImport>),
    /// Reviewing Postman environments for a project (R-COL-20).
    EnvironmentImport(Box<import::OpenEnvironmentImport>),
    /// The keyboard shortcut sheet (R-COL-27).
    Shortcuts(crate::components::ShortcutsDialog),
    /// Asking before something goes to a protected environment (R-SRV-9).
    Protected(Guarded, ConfirmDialog),
    /// Closing a project's tabs while some have unsaved edits (R-COL-37).
    CloseProjectTabs(PathBuf, ConfirmDialog),
    /// What changed between two responses (R-HIST-5).
    Changes(ChangesDialog),
    /// The request sent to every environment (R-SRV-10); what came back is in
    /// `TortugaUi::fanout`.
    Fanout,
}

/// What waits for a yes before it goes to a protected environment (R-SRV-9).
#[derive(Debug, Clone, PartialEq, Eq)]
enum Guarded {
    Send,
    /// Sending to every environment, protected ones included (R-SRV-10).
    SendEverywhere,
    RunWorkflow(crate::state::WorkflowKey),
}

/// What the user picked in the workspace menu (R-WS-1, R-WS-5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WorkspaceAction {
    Switch(usize),
    New,
    Rename,
    /// The environments every project has (R-WS-9).
    Environments,
    Variables,
    Defaults,
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
    /// What the scripts and captures did (R-TEST-1).
    tests: Option<TestsView>,
    /// How the response differs from the request's previous one (R-HIST-5).
    comparison: Option<Arc<tortuga_core::changes::Comparison>>,
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
    /// The requests open in tabs (R-COL-13, R-COL-14).
    tabs: Tabs,
    tab_strip: TabStripState,
    editor: EditorState,
    sidebar: SidebarState,

    /// What each request's last send left to show.
    sent: HashMap<Selection, Sent>,
    response: ResponseState,
    in_flight: Option<InFlight>,
    next_send_id: u64,
    finished: Inbox<Finished>,
    /// Fonts and style are installed on the first frame.
    styled: bool,
    /// The theme and text size last given to egui (R-SET-4, R-SET-5).
    applied_appearance: Option<tortuga_core::settings::Appearance>,
    /// Requests with unsaved edits whose file changed on disk (R-GIT-11).
    conflicts: HashMap<ItemKey, DiskChange>,
    /// When a request was last copied as curl (egui time), for the "Copied" feedback.
    copied_at: Option<f64>,
    /// Unsaved edits as last written to local data (R-COL-8), and when that was checked.
    drafts_saved: SavedDrafts,
    drafts_checked_at: f64,
    /// When the user last typed or clicked (egui time), for auto-save (R-SET-11).
    last_input_at: f64,
    /// The divider between request and response (R-SET-12).
    split: Split,
    /// The request sent to every environment, and the answers (R-SRV-10).
    fanout: Option<fanout::Fanout>,
    fanout_results: fanout::FanoutInbox,
    /// Saved versions that restored edits started from, for projects not open yet:
    /// checked against the disk once their project opens (R-COL-8, R-GIT-11).
    unchecked_bases: HashMap<ItemKey, Option<Request>>,
    /// Recently opened requests, newest first, for quick open (R-COL-9).
    recent: Vec<ItemKey>,
    filter_job: Option<FilterJob>,
    filter_results: Inbox<FilterDone>,
    sidebar_view: SidebarView,
    history: HistoryState,
    /// Open workflows and their runs (R-FLOW-2, R-FLOW-3).
    workflows: Workflows,
    workflow_events: Inbox<WorkflowEvent>,
    /// Whether a new version is out (R-SET-15).
    updates: updates::Updates,
}

/// Results that background tasks send back to the UI thread, which reads them each frame.
#[derive(Debug)]
struct Inbox<T> {
    tx: Sender<T>,
    rx: Receiver<T>,
}

impl<T> Inbox<T> {
    fn new() -> Self {
        let (tx, rx) = channel();
        Self { tx, rx }
    }

    /// For a task to send its result with.
    fn sender(&self) -> Sender<T> {
        self.tx.clone()
    }

    /// The next result that arrived, if any.
    fn try_recv(&self) -> Option<T> {
        self.rx.try_recv().ok()
    }
}

/// What the app keeps for one request while it runs: what its last send left to show,
/// and how it is being debugged. One struct, so a rename or delete can't move or drop
/// one part and forget another.
#[derive(Debug, Default)]
struct Sent {
    outcome: Option<Outcome>,
    /// The JSON filter typed for it, kept while the app runs (R-HTTP-13).
    filter: String,
    /// What its scripts and captures did (R-TEST-1).
    tests: Option<TestsView>,
    /// Breakpoints in its scripts (R-TEST-10).
    breakpoints: Breakpoints,
    /// How its last response differs from the one before (R-HIST-5).
    comparison: Option<Arc<tortuga_core::changes::Comparison>>,
}

/// Moves every value whose key `moved` gives a new place to that place.
fn rekey<K: Eq + std::hash::Hash, V>(map: &mut HashMap<K, V>, moved: impl Fn(&K) -> Option<K>) {
    *map = std::mem::take(map)
        .into_iter()
        .map(|(k, v)| (moved(&k).unwrap_or(k), v))
        .collect();
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
        let mut ui = Self {
            state: AppState::new(host),
            modal: None,
            selection: None,
            editor: EditorState::new(&current),
            current,
            scratch: Request::default(),
            drafts: HashMap::new(),
            tabs: Tabs::default(),
            tab_strip: TabStripState::default(),
            sidebar: SidebarState::default(),
            sent: HashMap::new(),
            response: ResponseState::default(),
            in_flight: None,
            next_send_id: 0,
            finished: Inbox::new(),
            styled: false,
            applied_appearance: None,
            conflicts: HashMap::new(),
            copied_at: None,
            drafts_saved: SavedDrafts::default(),
            drafts_checked_at: 0.0,
            last_input_at: 0.0,
            split: Split::default(),
            fanout: None,
            fanout_results: Inbox::new(),
            unchecked_bases: HashMap::new(),
            recent: Vec::new(),
            filter_job: None,
            filter_results: Inbox::new(),
            sidebar_view: SidebarView::default(),
            history: HistoryState::default(),
            workflows: Workflows::default(),
            workflow_events: Inbox::new(),
            updates: updates::Updates::default(),
        };
        ui.restore_drafts();
        // Back where the user left off (R-WS-1, R-COL-13, R-COL-36).
        ui.restore_tabs();
        ui.sidebar.restore(ui.state.remembered_folded());
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
            self.check_for_updates_at_start(ui.ctx());
        }
        self.apply_appearance(ui.ctx());
        script_keys::use_settings(ui.ctx(), &self.state.settings.script_editor);
        self.collect_finished();
        self.collect_fanout();
        self.collect_workflow_events();
        self.collect_update_check();
        self.drop_missing_workflows();
        self.auto_save_now_and_then(ui.ctx());
        self.save_drafts_now_and_then(ui.input(|i| i.time));
        for root in self.state.take_changes() {
            self.on_disk_change(&root);
        }
        self.state.mocks.take_hits();
        // A script field's own keys win over the app's (R-TEST-17).
        if self.modal.is_none() && !script_keys::field_takes_keys(ui.ctx()) {
            self.shortcuts(ui);
        }

        let p = theme::palette(ui);
        let chrome = Frame::new()
            .fill(p.chrome)
            .inner_margin(Margin::symmetric(14, 8));
        Panel::top("topbar")
            .frame(chrome)
            .show(ui, |ui| self.topbar(ui));
        if !self.state.settings.client.verify_tls {
            let frame = theme::banner_frame(ui, false);
            Panel::top("tls-warning").frame(frame).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("⚠").color(p.warning_text));
                    ui.label(RichText::new(t!("tls-off-warning")).color(p.warning_text));
                    if ui.link(t!("tls-off-change")).clicked() {
                        self.modal = Some(Modal::Settings(Box::new(SettingsDialog::new(
                            &self.state.settings,
                        ))));
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
                            .on_hover_text(t!("dismiss"))
                            .clicked()
                        {
                            self.state.notice = None;
                        }
                    },
                );
            });
        }

        if !self.state.sidebar_hidden() {
            self.left_panel(ui);
        }
        self.tab_bar(ui);

        CentralPanel::default()
            .frame(
                Frame::new()
                    .fill(p.bg)
                    .inner_margin(Margin::symmetric(20, 16)),
            )
            .show(ui, |ui| {
                if self.workflows.open.is_some() {
                    self.workflow_page(ui);
                } else {
                    self.workspace(ui);
                }
            });

        self.show_modal(ui.ctx());
        self.toast(ui.ctx());
        self.sync_tabs();
        if let Some(folded) = self.sidebar.take_folded() {
            self.state.remember_folded(folded);
        }
    }

    /// The sidebar: projects, or the history (R-HIST-2).
    fn left_panel(&mut self, ui: &mut Ui) {
        let p = theme::palette(ui);
        let dirty = self.dirty_keys();
        let conflicts: HashSet<ItemKey> = self.conflicts.keys().cloned().collect();
        let dirty_workflows = self.dirty_workflows();
        let scratch_edited = *self.scratch_request() != Request::default();
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
                    let (projects, history) = (t!("sidebar-projects"), t!("history"));
                    theme::tabs(
                        ui,
                        &mut self.sidebar_view,
                        &[
                            (SidebarView::Projects, projects.as_str(), None),
                            (SidebarView::History, history.as_str(), None),
                        ],
                    );
                    ui.add_space(8.0);
                }
                match self.sidebar_view {
                    SidebarView::Projects => sidebar(
                        ui,
                        &self.state,
                        &mut self.sidebar,
                        self.selection
                            .as_ref()
                            .filter(|_| self.workflows.open.is_none()),
                        self.workflows.open.as_ref(),
                        Marks {
                            dirty: &dirty,
                            conflicts: &conflicts,
                            dirty_workflows: &dirty_workflows,
                            scratch_edited,
                        },
                    ),
                    SidebarView::History => {
                        self.history_panel(ui);
                        Vec::new()
                    }
                }
            })
            .inner;
        for action in actions {
            self.on_sidebar(action, ui.ctx());
        }
    }

    /// Keyboard shortcuts that work anywhere outside dialogs.
    fn shortcuts(&mut self, ui: &Ui) {
        // Ctrl+Shift+Enter first: Ctrl+Enter would match it too.
        let everywhere = ui.input_mut(|i| i.consume_shortcut(&SEND_EVERYWHERE));
        if everywhere && self.workflows.open.is_none() {
            self.send_everywhere(ui.ctx());
        }
        let (save, send) = ui.input_mut(|i| (i.consume_shortcut(&SAVE), i.consume_shortcut(&SEND)));
        // With a workflow open, they save and run the workflow (R-FLOW-2, R-FLOW-3).
        if let Some(key) = self.workflows.open.clone() {
            if save {
                self.save_open_workflow();
            }
            if send {
                self.run_workflow(&key, ui.ctx());
            }
        } else {
            if save {
                self.save();
            }
            if send && self.sending().is_none() {
                self.send(ui.ctx());
            }
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
        if ui.input_mut(|i| i.consume_shortcut(&TOGGLE_SIDEBAR)) {
            self.toggle_sidebar();
        }
        let (shortcuts, url, environment) = ui.input_mut(|i| {
            (
                i.consume_shortcut(&SHORTCUTS),
                i.consume_shortcut(&FOCUS_URL),
                i.consume_shortcut(&ENVIRONMENT),
            )
        });
        if shortcuts {
            self.show_shortcuts(ui.ctx());
        }
        if url && self.workflows.open.is_none() {
            self.editor.focus_url();
        }
        if environment {
            self.modal = Some(Modal::Palette(Palette::commands_matching(&t!(
                "cmd-environment",
                name = ""
            ))));
        }
        self.tab_shortcuts(ui);
    }

    /// The keyboard shortcut sheet (R-COL-27).
    fn show_shortcuts(&mut self, ctx: &egui::Context) {
        let mac = ctx.os() == egui::os::OperatingSystem::Mac;
        self.modal = Some(Modal::Shortcuts(crate::components::ShortcutsDialog::new(
            shortcuts::sheet(mac, &script_keys::current(ctx)),
        )));
    }

    /// Hides the sidebar, or shows it again (R-COL-26).
    fn toggle_sidebar(&mut self) {
        let hidden = self.state.sidebar_hidden();
        self.state.set_sidebar_hidden(!hidden);
    }

    /// Call when the app closes: saves automatically if that is on (R-SET-11), and keeps
    /// other unsaved edits for next time (R-COL-8).
    pub fn on_exit(&mut self) {
        self.auto_save();
        self.save_drafts();
    }

    // ---------------------------------------------------------------- layout

    fn topbar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let p = theme::palette(ui);
            let hint = if self.state.sidebar_hidden() {
                t!("sidebar-show-hint")
            } else {
                t!("sidebar-hide-hint")
            };
            if ui
                .add(theme::icon_button(ui, "☰"))
                .on_hover_text(hint)
                .clicked()
            {
                self.toggle_sidebar();
            }
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
                // Only the icon; the name is on hover and for screen readers.
                let settings = ui
                    .add(theme::ghost_button(RichText::new("⚙").size(16.0)))
                    .on_hover_text(t!("settings"));
                settings.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::accesskit::Role::Button, true, t!("settings"))
                });
                if settings.clicked() {
                    self.modal = Some(Modal::Settings(Box::new(SettingsDialog::new(
                        &self.state.settings,
                    ))));
                }
                self.update_link(ui);
                let go_to = (
                    t!("go-to-request"),
                    RichText::new("  Ctrl+P").small().color(p.muted),
                );
                if ui
                    .add(egui::Button::new(go_to))
                    .on_hover_text(t!("go-to-request-hint"))
                    .clicked()
                {
                    self.modal = Some(Modal::Palette(Palette::requests()));
                }
            });
        });
    }

    fn workspace(&mut self, ui: &mut Ui) {
        let key = self.selection.clone();
        let root = key.as_ref().map(|k| k.root.clone());
        let ctx = self.state.request_context(key.as_ref());
        let var_help = self.state.var_help(key.as_ref(), &self.current, &ctx);
        let current_dirty = key
            .as_ref()
            .is_some_and(|k| self.is_dirty(k, &self.current));
        let servers = self.server_names(root.as_deref());
        let server_base = self
            .current
            .server
            .as_ref()
            .and_then(|id| ctx.server_urls.get(id).cloned());
        let read_only = self.project_read_only(root.as_deref());

        self.request_head(ui, key.as_ref(), current_dirty, read_only);
        ui.add_space(4.0);

        let collection_auth = root.as_ref().map(|_| ctx.auth.clone().unwrap_or_default());
        let inherited_headers: Vec<(tortuga_core::KeyValue, Origin)> = ctx
            .inherited_headers_with_origins(&self.current)
            .map(|(h, origin)| (h.clone(), origin.clone()))
            .collect();
        let cookie_count = self.state.cookie_jar(root.as_deref()).len();
        let sending = self.sending() == Some(&key);
        let settings = self.state.resolved_settings(root.as_deref());
        let app = &self.state.settings.client;
        let unverified_host =
            vars::host(&self.current, &ctx).filter(|h| app.verify_tls && app.is_insecure_host(h));
        let preview = self
            .editor
            .shows_preview()
            .then(|| vars::preview(&self.current, &ctx).map_err(|e| sentence(&e.to_string())));
        let no_breakpoints = Breakpoints::default();
        let last = self.sent.get(&key);
        let breakpoints = last.map_or(&no_breakpoints, |s| &s.breakpoints);
        let last_run = last.and_then(|s| s.tests.as_ref());
        let rerun = rerun_for(sending, last);
        let arrangement = self.arrangement(ui.available_width());
        let view = self.editor_view(arrangement);
        let cx = EditorContext {
            servers: &servers,
            workspace_servers: &self.workspace_servers(root.as_deref()),
            server_base: server_base.as_deref(),
            vars: &var_help,
            sending,
            settings,
            unverified_host: unverified_host.as_deref(),
            collection_auth: collection_auth.as_ref(),
            inherited_headers: &inherited_headers,
            auth_origin: &ctx.auth_origin,
            cookie_count,
            in_collection: root.is_some(),
            preview: preview.as_ref(),
            can_pick_files: self.state.host.can_pick_folders(),
            breakpoints,
            last_run,
            rerun,
            view,
            features: &self.state.settings.features,
        };
        // R-SET-12: stacked, the request takes what its tab needs up to the divider's
        // height; side by side, each gets a column.
        match arrangement {
            Arrangement::Stacked => {
                let available = ui.available_height();
                let cap = self.request_cap(available);
                let request = ui.allocate_ui(egui::vec2(ui.available_width(), cap), |ui| {
                    request_editor(ui, &mut self.current, &mut self.editor, cx)
                });
                let used = request.response.rect.height();
                if let Some(action) = request.inner {
                    self.apply_editor_action(ui.ctx(), action, &key);
                }
                self.stacked_divider(ui, used, available);
                self.response_area(ui, &key, sending);
            }
            Arrangement::SideBySide => {
                let full = ui.available_rect_before_wrap();
                let width = self.request_width(full.width());
                let left = egui::Rect::from_min_size(full.min, egui::vec2(width, full.height()));
                let action = {
                    let mut left_ui = ui.new_child(egui::UiBuilder::new().max_rect(left));
                    left_ui.set_min_height(left.height());
                    request_editor(&mut left_ui, &mut self.current, &mut self.editor, cx)
                };
                if let Some(action) = action {
                    self.apply_editor_action(ui.ctx(), action, &key);
                }
                let divider = egui::Rect::from_min_size(
                    egui::pos2(left.right(), full.top()),
                    egui::vec2(layout::DIVIDER, full.height()),
                );
                self.side_divider(ui, divider, width, full.width());
                let right =
                    egui::Rect::from_min_max(egui::pos2(divider.right(), full.top()), full.max);
                let mut right_ui = ui.new_child(egui::UiBuilder::new().max_rect(right));
                self.response_area(&mut right_ui, &key, sending);
                ui.allocate_rect(full, egui::Sense::hover());
            }
        }
        if let Some(name) = var_help.open.take() {
            self.open_variable(key.as_ref(), &name);
        }
    }

    /// The project at `root` is open read-only (a newer file format, R-COL-7).
    fn project_read_only(&self, root: Option<&Path>) -> bool {
        root.and_then(|r| self.state.collection(r))
            .is_some_and(|c| c.read_only)
    }

    /// The servers of the project at `root`: (id, name).
    fn server_names(&self, root: Option<&Path>) -> Vec<(String, String)> {
        root.and_then(|r| self.state.collection(r))
            .map(|c| {
                c.manifest
                    .servers
                    .iter()
                    .map(|s| (s.id.clone(), s.name.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The workspace's servers the project at `root` lacks, to pick from (R-WS-12). None
    /// for the scratch request or a read-only project, which can't get them.
    fn workspace_servers(&self, root: Option<&Path>) -> Vec<String> {
        root.and_then(|r| self.state.collection(r))
            .filter(|c| !c.read_only)
            .map(|c| {
                workspace::servers_missing_from(&self.state.workspace().environments, &c.manifest)
            })
            .unwrap_or_default()
    }

    /// "Choose…" for a multipart file: inside the project the path is kept relative to
    /// it, so it works for teammates too (R-HTTP-15, D21).
    fn choose_upload(&mut self, root: Option<&Path>, index: usize) {
        let Some(path) = self.state.host.pick_any_file(&t!("choose-upload-file")) else {
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
        self.response.reset();
    }

    /// Opens a request in the preview tab, or switches to its tab (R-COL-13); in a tab
    /// that stays when the preview tab is turned off (R-SET-9).
    fn select(&mut self, new: Selection) {
        self.open_tab(new, !self.state.settings.editing.preview_tab);
    }

    /// Opens a request in a tab; `keep` makes it a tab that stays (R-COL-13).
    fn open_tab(&mut self, new: Selection, keep: bool) {
        self.keep_if_edited();
        self.tabs.open(&new, keep);
        self.select_and_remember(new, true);
    }

    /// Selects a request; `remember` keeps it as the workspace's selection (R-WS-1).
    fn select_and_remember(&mut self, new: Selection, remember: bool) {
        // Before anything moves: the edits are saved where they were made (R-SET-11).
        self.auto_save();
        self.workflows.open = None;
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
        // Every field that holds per-request state is listed here and in `forget`.
        self.retarget_workflows(root, old, new);
        self.tabs.remap(root, old, new);
        self.sidebar.remap(root, old, new);
        let moved = |k: &ItemKey| k.moved(root, old, new);
        let moved_selection = |k: &Selection| k.as_ref().and_then(moved).map(Some);
        rekey(&mut self.drafts, moved);
        rekey(&mut self.conflicts, moved);
        rekey(&mut self.unchecked_bases, moved);
        rekey(&mut self.sent, moved_selection);
        for k in &mut self.recent {
            if let Some(to) = moved(k) {
                *k = to;
            }
        }
        if let Some(f) = &mut self.in_flight
            && let Some(to) = moved_selection(&f.key)
        {
            f.key = to;
        }
        if let Some(to) = moved_selection(&self.selection) {
            self.selection = to;
        }
        self.remap_past(moved_selection);
    }

    /// Drops state of deleted or closed items (`prefix` "" = the whole collection).
    fn forget(&mut self, root: &Path, prefix: &Path) {
        let gone = |k: &ItemKey| k.root == root && k.path.starts_with(prefix);
        let gone_selection = |k: &Selection| k.as_ref().is_some_and(gone);
        self.drafts.retain(|k, _| !gone(k));
        self.conflicts.retain(|k, _| !gone(k));
        self.unchecked_bases.retain(|k, _| !gone(k));
        self.sent.retain(|k, _| !gone_selection(k));
        self.recent.retain(|k| !gone(k));
        self.tabs.retain(|k| !gone_selection(k));
        self.sidebar.forget(root, prefix);
        self.forget_past_if(gone_selection);
        if self.selection.as_ref().is_some_and(gone) {
            // Not `select`: the request is gone, so its edits are not kept.
            let next = self.tabs.fallback().unwrap_or_default();
            let request = match &next {
                None => self.scratch.clone(),
                Some(k) => self
                    .drafts
                    .get(k)
                    .cloned()
                    .or_else(|| self.state.saved_request(k))
                    .unwrap_or_default(),
            };
            self.state.remember_selection(next.as_ref());
            self.selection = next;
            self.load(request);
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
}

/// Whether the tests can run again on what `last` left (R-TEST-12).
fn rerun_for(sending: bool, last: Option<&Sent>) -> Rerun {
    if !sending && last.is_some_and(|s| matches!(s.outcome, Some(Outcome::Response(_)))) {
        Rerun::Ready
    } else {
        Rerun::Unavailable
    }
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
    use egui::{Key, Modifiers};
    use tortuga_core::storage::FsStorage;
    use tortuga_core::{Collection, Item};

    use super::*;
    use crate::components::TabAction;
    use crate::test_host::TestHost;

    pub(super) type Shared<T> = std::rc::Rc<std::cell::RefCell<T>>;

    /// A collection on disk with one request, opened in a fresh UI with that request selected.
    pub(super) fn setup(name: &str) -> (TortugaUi, PathBuf, ItemKey) {
        let (ui, root, key, _, _) = setup_shared(name);
        (ui, root, key)
    }

    /// Like [`setup`], plus the host's saved local state and unsaved edits, to start again.
    pub(super) fn setup_shared(
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
        let (local, drafts) = (
            std::rc::Rc::clone(&host.local),
            std::rc::Rc::clone(&host.drafts),
        );
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
            drafts: std::rc::Rc::clone(&drafts),
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

    /// The tabs in strip order, with whether each is the preview tab.
    fn strip(ui: &TortugaUi) -> Vec<(Selection, bool)> {
        ui.tabs.strip().map(|(k, p)| (k.clone(), p)).collect()
    }

    /// Adds a request to the project on disk.
    pub(super) fn add_request(ui: &mut TortugaUi, root: &Path, name: &str) -> ItemKey {
        let request = Request {
            name: name.into(),
            ..Request::default()
        };
        let path = ui
            .state
            .run(root, |c, s| c.new_request(s, Path::new(""), &request))
            .expect("request");
        ItemKey::new(root, path)
    }

    /// R-COL-13, D31: a single click previews, double click or an edit keeps the tab,
    /// sending does not; closing keeps edits; tabs come back after a restart.
    #[test]
    fn tabs_preview_keep_close_and_restore() {
        let (mut ui, root, first, local, drafts) = setup_shared("tabs");
        let second = add_request(&mut ui, &root, "List users");
        let first_tab = Some(first.clone());
        let second_tab = Some(second.clone());

        ui.select(second_tab.clone());
        assert_eq!(strip(&ui), [(second_tab.clone(), true)], "preview replaced");

        ui.open_tab(first_tab.clone(), true);
        ui.select(second_tab.clone());
        ui.send(&egui::Context::default());
        ui.sync_tabs();
        assert_eq!(
            strip(&ui),
            [(first_tab.clone(), false), (second_tab.clone(), true)],
            "sending does not keep the tab"
        );

        ui.current.set_display_url("https://a.test/edited");
        ui.sync_tabs();
        assert!(!ui.tabs.is_preview(&second_tab), "editing keeps the tab");

        ui.close_tabs(std::slice::from_ref(&second_tab));
        assert_eq!(ui.selection, first_tab, "back to the tab used before");
        assert!(ui.drafts.contains_key(&second), "closing keeps the edits");
        assert_eq!(strip(&ui), [(first_tab.clone(), false)]);

        ui.select(second_tab.clone());
        assert_eq!(ui.current.url, "https://a.test/edited");
        ui.sync_tabs();
        ui.on_exit();
        let again = TortugaUi::new(Box::new(TestHost {
            local,
            drafts,
            ..TestHost::default()
        }));
        assert_eq!(strip(&again), strip(&ui), "tabs restored");
        assert_eq!(again.selection, second_tab);
        std::fs::remove_dir_all(root).expect("clean up");
    }

    /// R-COL-36: a folded folder stays folded after a restart, follows a rename, and is
    /// forgotten when the folder is deleted.
    #[test]
    fn folded_folders_come_back_after_a_restart() {
        use egui_kittest::kittest::Queryable;

        let (mut ui, root, _, local, drafts) = setup_shared("folded");
        let folder = ui
            .state
            .run(&root, |c, s| c.new_folder(s, Path::new(""), "Users ü😀"))
            .expect("folder");
        let request = Request {
            name: "List users".into(),
            ..Request::default()
        };
        ui.state
            .run(&root, |c, s| c.new_request(s, &folder, &request))
            .expect("request");
        let saved_folded = || {
            local.borrow().as_ref().map_or_else(Vec::new, |l| {
                l.workspaces[l.active_workspace].folded.clone()
            })
        };
        {
            let mut harness = crate::test_host::harness(|egui_ui| ui.ui(egui_ui));
            harness.run();
            harness.get_by_label_contains("Users ü😀").click();
            harness.run();
            assert!(harness.query_by_label_contains("List users").is_none());
        }
        assert_eq!(saved_folded(), [(root.clone(), folder.clone())]);
        ui.on_exit();

        let mut again = TortugaUi::new(Box::new(TestHost {
            local: std::rc::Rc::clone(&local),
            drafts,
            ..TestHost::default()
        }));
        {
            let mut harness = crate::test_host::harness(|egui_ui| again.ui(egui_ui));
            harness.run();
            harness.get_by_label_contains("Users ü😀");
            assert!(
                harness.query_by_label_contains("List users").is_none(),
                "still folded"
            );
        }

        again.rename(&ItemKey::new(&root, folder.clone()), "People".into());
        crate::test_host::harness(|egui_ui| again.ui(egui_ui)).run();
        let renamed = saved_folded();
        assert_eq!(renamed.len(), 1);
        assert_ne!(renamed[0].1, folder, "follows the rename");

        let gone = ItemKey::new(&root, renamed[0].1.clone());
        again
            .state
            .run(&root, |c, s| c.delete(s, &gone.path))
            .expect("delete");
        again.forget(&gone.root, &gone.path);
        crate::test_host::harness(|egui_ui| again.ui(egui_ui)).run();
        assert!(saved_folded().is_empty(), "forgotten with the folder");
        std::fs::remove_dir_all(root).expect("clean up");
    }

    /// R-COL-37: closing a project's tabs asks first when one has unsaved edits, and
    /// the edits stay; with everything saved they close at once.
    #[test]
    fn closing_a_projects_tabs_warns_about_unsaved_edits() {
        let (mut ui, root, first) = setup("close-project-tabs");
        let second = add_request(&mut ui, &root, "List üsers 😀");
        ui.open_tab(Some(first.clone()), true);
        ui.open_tab(Some(second.clone()), true);
        ui.current.set_display_url("https://a.test/edited");
        ui.sync_tabs();

        ui.on_tab(TabAction::CloseGroup(root.clone()));
        assert!(
            matches!(ui.modal, Some(Modal::CloseProjectTabs(..))),
            "asks first"
        );
        assert_eq!(strip(&ui).len(), 2, "nothing closed before the answer");

        ui.modal = None;
        ui.close_project_tabs(&root);
        assert!(
            strip(&ui).iter().all(|(k, _)| k.is_none()),
            "the project's tabs are closed"
        );
        assert!(ui.drafts.contains_key(&second), "the edits stay");

        ui.drafts.remove(&second);
        ui.open_tab(Some(first.clone()), true);
        ui.on_tab(TabAction::CloseGroup(root.clone()));
        assert!(ui.modal.is_none(), "nothing unsaved: no question");
        assert!(strip(&ui).iter().all(|(k, _)| k.is_none()));
        std::fs::remove_dir_all(root).expect("clean up");
    }

    /// R-COL-13: Ctrl+Tab goes back to the tab shown before, and pressed again returns.
    #[test]
    fn ctrl_tab_goes_back_to_the_previous_tab() {
        let (mut ui, root, first) = setup("ctrl-tab");
        let second = add_request(&mut ui, &root, "List users");
        let third = add_request(&mut ui, &root, "Create user");
        ui.open_tab(Some(first.clone()), true);
        ui.open_tab(Some(second.clone()), true);
        ui.open_tab(Some(third.clone()), true);
        ui.select(Some(first.clone()));

        // The request shown after each frame.
        let shown: Shared<Selection> = Shared::default();
        let seen = std::rc::Rc::clone(&shown);
        let mut harness = crate::test_host::harness(move |egui_ui| {
            ui.ui(egui_ui);
            *seen.borrow_mut() = ui.selection.clone();
        });
        harness.run();
        harness.key_press_modifiers(egui::Modifiers::CTRL, egui::Key::Tab);
        harness.run();
        assert_eq!(*shown.borrow(), Some(third), "back to the previous tab");
        harness.key_press_modifiers(egui::Modifiers::CTRL, egui::Key::Tab);
        harness.run();
        assert_eq!(*shown.borrow(), Some(first), "and back again");
        drop(harness);
        std::fs::remove_dir_all(root).expect("clean up");
    }

    /// R-COL-13: tabs follow renames and close when their request is deleted.
    #[test]
    fn tabs_follow_renames_and_deletes() {
        let (mut ui, root, first) = setup("tabs-disk");
        let second = add_request(&mut ui, &root, "List users");
        ui.open_tab(Some(first.clone()), true);
        ui.open_tab(Some(second.clone()), true);

        ui.rename(&second, "All users".into());
        let renamed = ui.selection.clone();
        assert_ne!(renamed, Some(second.clone()));
        assert!(ui.tabs.contains(&renamed), "the tab follows the file");

        let renamed = renamed.expect("a request");
        std::fs::remove_file(root.join(&renamed.path)).expect("delete");
        ui.select(Some(first.clone()));
        ui.on_disk_change(&root);
        ui.sync_tabs();
        assert_eq!(
            strip(&ui),
            [(Some(first), false)],
            "deleted in git: tab closed"
        );
        std::fs::remove_dir_all(root).expect("clean up");
    }

    /// R-HTTP-13, R-TEST-1, R-TEST-10: a request's filter, test results and breakpoints
    /// follow a rename, and are gone with the request.
    #[test]
    fn last_send_follows_renames_and_deletes() {
        let (mut ui, root, first) = setup("sent-disk");
        let second = add_request(&mut ui, &root, "List users");
        ui.select(Some(second.clone()));
        ui.sent.insert(
            Some(second.clone()),
            Sent {
                filter: "$.id".into(),
                tests: Some(TestsView::default()),
                breakpoints: Breakpoints {
                    tests: [3].into(),
                    ..Breakpoints::default()
                },
                ..Sent::default()
            },
        );

        ui.rename(&second, "All users".into());
        let renamed = ui.selection.clone();
        assert_ne!(renamed, Some(second.clone()));
        let sent = ui.sent.get(&renamed).expect("moved with the request");
        assert_eq!(sent.filter, "$.id");
        assert!(sent.tests.is_some());
        assert_eq!(sent.breakpoints.tests, [3].into());
        assert!(!ui.sent.contains_key(&Some(second.clone())));

        let renamed = renamed.expect("a request");
        std::fs::remove_file(root.join(&renamed.path)).expect("delete");
        ui.select(Some(first));
        ui.on_disk_change(&root);
        assert!(!ui.sent.contains_key(&Some(renamed)), "nothing left of it");
        std::fs::remove_dir_all(root).expect("clean up");
    }

    /// R-TEST-12: "Re-run tests" runs the edited tests and captures on the last response,
    /// without sending; the pre-request script's console stays, variables apply.
    #[test]
    fn tests_run_again_on_the_last_response() {
        use tortuga_core::script::{ConsoleEntry, LogLevel, ScriptKind};

        use crate::components::EditorAction;

        let (mut ui, root, key) = setup("rerun");
        let selection = Some(key.clone());
        ui.select(selection.clone());
        ui.current.scripts.tests = "test(\"ok\", () => expect(res.status).toBe(500));".into();
        ui.current.capture.insert("user".into(), "$.name".into());
        let response = tortuga_core::Response {
            status: 200,
            status_text: "OK".into(),
            headers: vec![tortuga_core::KeyValue::new(
                "content-type",
                "application/json",
            )],
            body: r#"{"name": "ü😀"}"#.as_bytes().to_vec(),
            elapsed: std::time::Duration::from_millis(3),
            timing: None,
        };
        let pre_log = (
            ScriptKind::PreRequest,
            ConsoleEntry::Log {
                level: LogLevel::Log,
                args: vec![serde_json::json!("before")],
                line: Some(1),
            },
        );
        ui.sent.insert(
            selection.clone(),
            Sent {
                outcome: Some(Outcome::Response(Arc::new(ResponseView::new(response)))),
                tests: Some(TestsView {
                    console: vec![pre_log.clone()],
                    ..TestsView::default()
                }),
                ..Sent::default()
            },
        );

        ui.apply_editor_action(
            &egui::Context::default(),
            EditorAction::RerunTests,
            &selection,
        );
        let tests = ui.sent[&selection].tests.clone().expect("results");
        assert_eq!(tests.tests.len(), 1);
        assert!(!tests.tests[0].passed, "expects 500, got 200");
        assert_eq!(
            tests.console.first(),
            Some(&pre_log),
            "the pre-request console stays"
        );
        assert_eq!(tests.ran.tests, ui.current.scripts.tests);
        assert_eq!(
            ui.state
                .runtime_vars(Some(&root))
                .and_then(|v| v.get("user"))
                .map(String::as_str),
            Some("ü😀"),
            "the capture applied"
        );

        // Fixed and run again: no request was sent for any of it.
        ui.current.scripts.tests = "test(\"ok\", () => expect(res.status).toBe(200));".into();
        ui.apply_editor_action(
            &egui::Context::default(),
            EditorAction::RerunTests,
            &selection,
        );
        let tests = ui.sent[&selection].tests.clone().expect("results");
        assert!(tests.tests[0].passed);
        assert!(ui.in_flight.is_none());
        std::fs::remove_dir_all(root).expect("clean up");
    }

    /// R-MOCK-8: the response is checked against the request's examples, as a test.
    #[test]
    fn responses_are_checked_against_the_examples() {
        use crate::components::EditorAction;

        let (mut ui, root, key) = setup("contract");
        let selection = Some(key.clone());
        ui.select(selection.clone());
        ui.current.examples = vec![tortuga_core::mock::Example {
            body: r#"{"name": "x", "id": 1}"#.into(),
            ..tortuga_core::mock::Example::new("200 OK")
        }];
        let response = tortuga_core::Response {
            status: 200,
            status_text: "OK".into(),
            headers: vec![tortuga_core::KeyValue::new(
                "content-type",
                "application/json",
            )],
            body: r#"{"name": "ü😀"}"#.as_bytes().to_vec(),
            elapsed: std::time::Duration::from_millis(3),
            timing: None,
        };
        ui.sent.insert(
            selection.clone(),
            Sent {
                outcome: Some(Outcome::Response(Arc::new(ResponseView::new(response)))),
                ..Sent::default()
            },
        );
        ui.apply_editor_action(
            &egui::Context::default(),
            EditorAction::RerunTests,
            &selection,
        );
        let tests = ui.sent[&selection].tests.clone().expect("results");
        assert_eq!(tests.tests.len(), 1);
        assert_eq!(tests.tests[0].name, "Matches example “200 OK”");
        assert_eq!(tests.tests[0].message.as_deref(), Some("$.id is missing"));
        std::fs::remove_dir_all(root).expect("clean up");
    }

    /// R-WS-12: a server only the workspace has is offered to the project's requests;
    /// picking it adds it to the project and the request uses its URL from the workspace.
    #[test]
    fn a_workspace_server_picked_for_a_request_is_added_to_the_project() {
        use crate::components::EditorAction;

        let (mut ui, root, key) = setup("ws-server");
        ui.state.workspace_mut().environments =
            vec![tortuga_core::workspace::WorkspaceEnvironment {
                id: "staging".into(),
                name: "Staging".into(),
                servers: [("Payments ü😀".to_owned(), "https://pay.stg".to_owned())].into(),
                ..Default::default()
            }];
        assert_eq!(ui.workspace_servers(Some(&root)), ["Payments ü😀"]);
        assert!(
            ui.workspace_servers(None).is_empty(),
            "not for the scratch request"
        );

        ui.apply_editor_action(
            &egui::Context::default(),
            EditorAction::UseWorkspaceServer("Payments ü😀".into()),
            &Some(key.clone()),
        );
        let c = ui.state.collection(&root).expect("open");
        let server = c.manifest.servers.first().expect("added");
        assert_eq!(server.name, "Payments ü😀");
        assert_eq!(ui.current.server.as_ref(), Some(&server.id));
        assert!(
            ui.is_dirty(&key, &ui.current),
            "the request is not saved for the user"
        );
        assert!(ui.workspace_servers(Some(&root)).is_empty(), "listed once");
        let env = ui.state.effective_environment(&root).expect("env");
        assert_eq!(env.servers[&server.id], "https://pay.stg");
        std::fs::remove_dir_all(root).expect("clean up");
    }

    /// R-TEST-13: tests from the response go after the script with one blank line.
    #[test]
    fn tests_from_the_response_are_appended() {
        let (mut ui, root, key) = setup("add-test");
        ui.select(Some(key));
        ui.add_test("test(\"a\", () => {});");
        assert_eq!(ui.current.scripts.tests, "test(\"a\", () => {});");
        ui.add_test("test(\"b\", () => {});");
        ui.current.scripts.tests.push_str("\n\n\n");
        ui.add_test("test(\"c\", () => {});");
        assert_eq!(
            ui.current.scripts.tests,
            "test(\"a\", () => {});\n\ntest(\"b\", () => {});\n\ntest(\"c\", () => {});"
        );
        std::fs::remove_dir_all(root).expect("clean up");
    }

    /// R-COL-14: with tabs of two projects open, each project's tabs get a chip.
    #[test]
    fn tabs_are_grouped_by_project() {
        let (mut ui, root, first) = setup("groups");
        let other =
            std::env::temp_dir().join(format!("tortuga-disk-groups2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&other);
        Collection::create(&FsStorage, &other, "Orders API").expect("create");
        ui.state.open_collection(&other).expect("open");
        let order = add_request(&mut ui, &other, "Get order");

        let chips = |ui: &mut TortugaUi| {
            let mut chips = Vec::new();
            let mut harness = crate::test_host::harness(|egui_ui| {
                chips = ui
                    .tab_groups(egui_ui)
                    .iter()
                    .map(|g| g.chip.as_ref().map(|c| (c.name.clone(), g.tabs.len())))
                    .collect();
            });
            harness.run();
            drop(harness);
            chips
        };
        ui.open_tab(Some(first.clone()), true);
        assert_eq!(chips(&mut ui), [None], "one project: no chips");

        let second = add_request(&mut ui, &root, "List users");
        ui.open_tab(Some(order.clone()), true);
        ui.open_tab(Some(second), true);
        assert_eq!(
            chips(&mut ui),
            [Some(("C".into(), 2)), Some(("Orders API".into(), 1))],
            "grouped, in the order the projects were opened"
        );

        ui.on_tab(TabAction::CloseGroup(other.clone()));
        assert_eq!(chips(&mut ui), [None]);

        ui.open_tab(Some(order.clone()), false);
        assert_eq!(
            chips(&mut ui),
            [Some(("C".into(), 2)), Some(("Orders API".into(), 1))],
            "a preview of another project gets its own chip"
        );
        ui.open_tab(Some(order), true);
        let third = add_request(&mut ui, &other, "List orders");
        ui.open_tab(Some(third), false);
        assert_eq!(
            chips(&mut ui),
            [Some(("C".into(), 2)), Some(("Orders API".into(), 1)), None],
            "a preview right after its own group needs no chip"
        );
        std::fs::remove_dir_all(root).expect("clean up");
        std::fs::remove_dir_all(other).expect("clean up");
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

    /// R-COL-46: typing in the sidebar's filter box shows only the matching requests, also
    /// those out of view before, and clearing it brings the rest back.
    #[test]
    fn the_sidebar_filter_finds_requests_by_name() {
        use egui_kittest::kittest::Queryable;

        let root = std::env::temp_dir().join(format!("tortuga-filter-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        Collection::create(&FsStorage, &root, "Big").expect("create");
        for i in 0..300 {
            let mut request = Request::new(tortuga_core::HttpMethod::Get, "https://a.test/");
            request.name = format!("Request {i:03}");
            std::fs::write(
                root.join(format!("request-{i:03}.toml")),
                tortuga_core::format::write_request(&request, Some(i)),
            )
            .expect("write");
        }
        let mut ui = TortugaUi::new(Box::new(TestHost {
            open: vec![root.clone()],
            ..TestHost::default()
        }));
        let mut harness = crate::test_host::harness(move |egui_ui| ui.ui(egui_ui));
        harness.run();
        // The sidebar's filter box is the leftmost text field.
        let filter = harness
            .get_all_by_role(egui::accesskit::Role::TextInput)
            .min_by(|a, b| a.rect().left().total_cmp(&b.rect().left()))
            .expect("filter box");
        filter.focus();
        filter.type_text("request 299");
        harness.run();
        harness.get_by_label_contains("Request 299");
        assert!(harness.query_by_label_contains("Request 000").is_none());
        harness.key_press(Key::Escape);
        harness.run();
        harness.get_by_label_contains("Request 000");
        assert!(harness.query_by_label_contains("Request 299").is_none());
        std::fs::remove_dir_all(root).expect("clean up");
    }

    /// R-COL-47: with "{}" on, the filter finds a request by its saved body, and a body
    /// changed on disk is searched again, not the one from before.
    #[test]
    fn the_sidebar_filter_searches_request_bodies() {
        use egui_kittest::kittest::Queryable;

        let (ui, root, key) = setup("filter-bodies");
        let write_body = |body: &str| {
            let mut request = Request::new(tortuga_core::HttpMethod::Post, "https://a.test/");
            request.name = "Get user".into();
            request.body = tortuga_core::RequestBody::Json(body.into());
            std::fs::write(
                root.join(&key.path),
                tortuga_core::format::write_request(&request, Some(1)),
            )
            .expect("write");
        };
        write_body(r#"{"customerId": "Ärla-42"}"#);
        let ui = std::rc::Rc::new(std::cell::RefCell::new(ui));
        ui.borrow_mut().state.reload(&root);
        let shared = std::rc::Rc::clone(&ui);
        let mut harness = crate::test_host::harness(move |egui_ui| shared.borrow_mut().ui(egui_ui));
        harness.run();
        let filter = harness
            .get_all_by_role(egui::accesskit::Role::TextInput)
            .min_by(|a, b| a.rect().left().total_cmp(&b.rect().left()))
            .expect("filter box");
        filter.focus();
        filter.type_text("customerid ärla");
        harness.run();
        harness.get_by_label_contains("Nothing matches");
        harness.get_by_label("{}").click();
        harness.run();
        assert_eq!(
            harness.query_all_by_label_contains("Get user").count(),
            2,
            "tab and row"
        );

        write_body(r#"{"orderId": 7}"#);
        ui.borrow_mut().state.reload(&root);
        harness.run();
        harness.get_by_label_contains("Nothing matches");
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
        let shared = std::rc::Rc::clone(&ui);
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
                .toast
                .as_ref()
                .map(|t| t.text.as_str())
                .is_some_and(|n| n.contains("--insecure")),
            "{:?}",
            ui.state.toast
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
        let shared = std::rc::Rc::clone(&ui);
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

    /// R-WS-8: "Variables…" in the workspace menu adds a variable every project uses,
    /// and the dialog lists values captured as `workspace.<name>`.
    #[test]
    fn workspace_variables_dialog() {
        use egui_kittest::kittest::Queryable;

        let (ui, root, _) = setup("ws-vars-dialog");
        let ui = std::rc::Rc::new(std::cell::RefCell::new(ui));
        ui.borrow_mut().state.apply_runtime(
            Some(&root),
            &[("workspace.token".to_owned(), Some("t-1".to_owned()))].into(),
        );
        let shared = std::rc::Rc::clone(&ui);
        let mut harness = crate::test_host::harness(move |egui_ui| shared.borrow_mut().ui(egui_ui));
        harness.run();
        harness.get_by_label_contains("My workspace").click();
        harness.run();
        harness.get_by_label("Variables…").click();
        harness.run();
        harness.get_by_label_contains("Workspace variables — My workspace");
        harness.get_by_label("t-1");
        // The dialog's fields come after the editor's; its only one is "Add variable".
        let name = harness
            .get_all_by_role(egui::accesskit::Role::TextInput)
            .last()
            .expect("new variable field");
        name.focus();
        name.type_text("tenant");
        harness.run();
        harness.key_press(Key::Enter);
        harness.run();
        // Now: the row's name, its value, and "Add variable" again.
        let fields: Vec<_> = harness
            .get_all_by_role(egui::accesskit::Role::TextInput)
            .collect();
        assert_eq!(fields[fields.len() - 3].value().as_deref(), Some("tenant"));
        fields[fields.len() - 2].focus();
        fields[fields.len() - 2].type_text("acme");
        harness.run();
        // The request editor has a Save button too; the dialog's is drawn last.
        harness
            .query_all_by_label("Save")
            .last()
            .expect("dialog's Save")
            .click();
        harness.run();
        assert!(ui.borrow().modal.is_none(), "saved and closed");
        let ctx = ui.borrow().state.context(Some(&root));
        assert_eq!(
            ctx.variables.get("tenant").map(String::as_str),
            Some("acme")
        );
        drop(harness);
        std::fs::remove_dir_all(root).expect("clean up");
    }

    /// R-SET-1, R-SET-2: "Defaults…" in the workspace menu edits the workspace's
    /// headers, auth and request settings.
    #[test]
    fn workspace_defaults_from_the_menu() {
        use egui_kittest::kittest::Queryable;

        let (ui, root, _) = setup("ws-defaults-dialog");
        let ui = std::rc::Rc::new(std::cell::RefCell::new(ui));
        let shared = std::rc::Rc::clone(&ui);
        let mut harness = crate::test_host::harness(move |egui_ui| shared.borrow_mut().ui(egui_ui));
        harness.run();
        harness.get_by_label_contains("My workspace").click();
        harness.run();
        harness.get_by_label("Defaults…").click();
        harness.run();
        harness.get_by_label_contains("Workspace defaults — My workspace");
        harness.get_by_label("Requests").click();
        harness.run();
        harness.get_by_label("Timeout (ms)");
        assert!(matches!(
            &ui.borrow().modal,
            Some(Modal::Defaults(d)) if d.target == crate::components::DefaultsTarget::Workspace
        ));
        drop(harness);
        std::fs::remove_dir_all(root).expect("clean up");
    }

    /// R-VAR-9: a Ctrl+click on a variable in defaults opens where it can be defined:
    /// the project's environments, or the workspace's variables for the workspace's
    /// defaults; a folder's variable opens that folder's Variables.
    #[test]
    fn ctrl_click_in_defaults_opens_the_definition() {
        use crate::components::DefaultsTarget;

        let (mut ui, root, _) = setup("defaults-goto");
        let folder = ui
            .state
            .run(&root, |c, s| c.new_folder(s, Path::new(""), "Admin ü😀"))
            .expect("folder");
        let defaults = tortuga_core::format::FolderDefaults {
            variables: [("tenant".to_owned(), "acme".to_owned())].into(),
            ..Default::default()
        };
        ui.state
            .run(&root, |c, s| c.save_folder_defaults(s, &folder, &defaults))
            .expect("defaults");

        ui.open_default_variable(&DefaultsTarget::Project(root.clone()), "nowhere");
        assert!(matches!(ui.modal.take(), Some(Modal::Environments(_))));

        ui.open_default_variable(&DefaultsTarget::Workspace, "nowhere");
        assert!(matches!(ui.modal.take(), Some(Modal::WorkspaceVars(_))));

        ui.open_default_variable(
            &DefaultsTarget::Folder(root.clone(), folder.clone()),
            "tenant",
        );
        assert!(matches!(
            ui.modal.take(),
            Some(Modal::Defaults(d)) if d.target == DefaultsTarget::Folder(root.clone(), folder)
        ));
        std::fs::remove_dir_all(root).expect("clean up");
    }

    /// R-SET-6: with history off nothing is recorded; a smaller size trims what is kept.
    #[test]
    fn history_size_and_off() {
        use tortuga_core::history::{Entry, new_id};

        let (mut ui, root, key) = setup("history-size");
        let entry = |ui: &TortugaUi, n: u64| {
            Entry::new(
                new_id(n, 1),
                n,
                Some(key.path.clone()),
                &ui.current,
                None,
                Err("down".to_owned()),
            )
        };
        let count = |ui: &TortugaUi| ui.state.host.history_list(Some(&root)).expect("list").len();
        let mut settings = ui.state.settings.clone();
        settings.history.size = 0;
        ui.state.apply_settings(&settings).expect("off");
        let e = entry(&ui, 1);
        ui.record_history(Some(&root), e);
        assert_eq!(count(&ui), 0, "nothing recorded");

        settings.history.size = 200;
        ui.state.apply_settings(&settings).expect("on");
        for n in 2..6 {
            let e = entry(&ui, n);
            ui.record_history(Some(&root), e);
        }
        assert_eq!(count(&ui), 4);
        settings.history.size = 2;
        ui.state.apply_settings(&settings).expect("smaller");
        ui.trim_loaded_history();
        assert_eq!(count(&ui), 2, "trimmed");
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
        let shared = std::rc::Rc::clone(&ui);
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
                timing: None,
            };
            let entry = Entry::new(
                tortuga_core::history::new_id(1_000, 1),
                1_000,
                Some(key.path.clone()),
                &ui.current,
                None,
                Ok(&response),
            );
            ui.finished
                .sender()
                .send(Finished {
                    id,
                    outcome: Outcome::from_result(Ok(response)),
                    entry: Some(entry),
                    tests: None,
                    comparison: None,
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

    /// R-COL-22: saving the defaults dialog writes a project's and a folder's
    /// description to their files and keeps their other defaults.
    #[test]
    fn descriptions_are_saved_in_the_files() {
        use crate::components::{DefaultsTarget, EditedDefaults};

        let dir = tempfile::tempdir().expect("dir");
        let root = dir.path().to_path_buf();
        let col = Collection::create(&FsStorage, &root, "C").expect("create");
        let folder = col
            .new_folder(&FsStorage, Path::new(""), "Admin")
            .expect("folder");
        let mut ui = TortugaUi::new(Box::new(TestHost {
            open: vec![root.clone()],
            ..TestHost::default()
        }));
        let headers = vec![tortuga_core::KeyValue::new("X-Area", "admin")];
        for target in [
            DefaultsTarget::Project(root.clone()),
            DefaultsTarget::Folder(root.clone(), folder.clone()),
        ] {
            let saved = ui.save_defaults(
                &target,
                EditedDefaults {
                    description: "# Shop ü😀\n\nThe *orders* API.\n".into(),
                    headers: headers.clone(),
                    ..EditedDefaults::default()
                },
            );
            assert!(saved);
        }
        let c = ui.state.collection(&root).expect("project");
        assert_eq!(c.manifest.description, "# Shop ü😀\n\nThe *orders* API.\n");
        assert_eq!(c.manifest.headers, headers);
        let Some(tortuga_core::Item::Folder(f)) = c.find(&folder) else {
            panic!("the folder");
        };
        assert_eq!(f.defaults.description, "# Shop ü😀\n\nThe *orders* API.\n");
        assert_eq!(f.defaults.headers, headers);
        let file = std::fs::read_to_string(root.join(&folder).join("_folder.toml")).expect("file");
        assert!(file.contains("description = \"\"\"\n# Shop ü😀"), "{file}");
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

    /// R-COL-12: dragging a row onto the top of another puts it before that one;
    /// dropping it on a folder moves it into the folder.
    #[test]
    fn drag_and_drop_in_the_tree() {
        use egui_kittest::kittest::Queryable;

        let root = std::env::temp_dir().join(format!("tortuga-dnd-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let col = Collection::create(&FsStorage, &root, "Shop").expect("create");
        for name in ["Alpha", "Beta"] {
            let mut r = Request::new(tortuga_core::HttpMethod::Get, "https://a.test/");
            r.name = name.into();
            Collection::open(&FsStorage, &root)
                .expect("open")
                .new_request(&FsStorage, Path::new(""), &r)
                .expect("request");
        }
        drop(col);
        Collection::open(&FsStorage, &root)
            .expect("open")
            .new_folder(&FsStorage, Path::new(""), "Orders")
            .expect("folder");
        let ui = std::rc::Rc::new(std::cell::RefCell::new(TortugaUi::new(Box::new(
            TestHost {
                open: vec![root.clone()],
                ..TestHost::default()
            },
        ))));
        let shared = std::rc::Rc::clone(&ui);
        let mut harness = crate::test_host::harness(move |egui_ui| shared.borrow_mut().ui(egui_ui));
        harness.run();

        let drag = |harness: &mut egui_kittest::Harness<'_>, from: &str, to: &str, frac: f32| {
            let start = harness.get_by_label_contains(from).rect().center();
            let target = harness.get_by_label_contains(to).rect();
            let end = egui::pos2(target.center().x, target.top() + target.height() * frac);
            harness.event(egui::Event::PointerMoved(start));
            harness.event(egui::Event::PointerButton {
                pos: start,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            });
            harness.step();
            for i in 1..=5 {
                #[expect(clippy::cast_precision_loss, reason = "a few steps")]
                let t = i as f32 / 5.0;
                harness.event(egui::Event::PointerMoved(start + (end - start) * t));
                harness.step();
            }
            harness.event(egui::Event::PointerButton {
                pos: end,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            });
            harness.run();
        };
        let order = || {
            let c = Collection::open(&FsStorage, &root).expect("open");
            c.items.iter().map(Item::name).collect::<Vec<_>>()
        };
        assert_eq!(order(), ["Alpha", "Beta", "Orders"]);
        drag(&mut harness, "Beta", "Alpha", 0.2);
        assert_eq!(order(), ["Beta", "Alpha", "Orders"]);
        drag(&mut harness, "Alpha", "Orders", 0.7);
        assert_eq!(order(), ["Beta", "Orders"]);
        let c = Collection::open(&FsStorage, &root).expect("open");
        assert_eq!(
            c.children(Path::new("orders"))
                .iter()
                .map(Item::name)
                .collect::<Vec<_>>(),
            ["Alpha"]
        );
        drop(harness);
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
