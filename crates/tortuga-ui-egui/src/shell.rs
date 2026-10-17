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
    ConfirmDialog, CreateCollectionDialog, DefaultsDialog, DialogResult, EditorAction,
    EditorContext, EditorState, EnvironmentsDialog, MoveDialog, Outcome, ResponseAction,
    ResponseState, SaveToCollectionDialog, SettingsDialog, SidebarAction, SidebarState,
    request_editor, response_panel, sidebar,
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
    CreateCollection(CreateCollectionDialog),
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
}

impl TortugaUi {
    /// Loads settings and reopens the collections that were open last time.
    pub fn new(host: Box<dyn Host>) -> Self {
        let current = Request::default();
        let (finished_tx, finished_rx) = channel();
        Self {
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
        }
    }

    /// Draws the app into `ui` (the whole window) and handles what the user did.
    pub fn ui(&mut self, ui: &mut Ui) {
        if !self.styled {
            theme::install(ui.ctx());
            self.styled = true;
        }
        self.collect_finished();
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
            if let Some(root) = self.selection.as_ref().map(|k| k.root.clone()) {
                ui.add_space(12.0);
                ui.separator();
                ui.add_space(4.0);
                self.environment_picker(ui, &root);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(theme::ghost_button("⚙  Settings")).clicked() {
                    self.modal = Some(Modal::Settings(SettingsDialog::new(&self.state.settings)));
                }
            });
        });
    }

    /// Active environment of the selected request's collection (R-SRV-5).
    fn environment_picker(&mut self, ui: &mut Ui, root: &Path) {
        let Some(c) = self.state.collection(root) else {
            return;
        };
        let envs: Vec<(String, String)> = c
            .environments
            .iter()
            .map(|e| (e.id.clone(), e.name.clone()))
            .collect();
        let p = theme::palette(ui);
        ui.label(RichText::new("Environment").color(p.muted));
        let edit = if envs.is_empty() {
            ui.button("Add environments…").clicked()
        } else {
            let before = self.state.active_environment(root);
            let mut active = before.clone();
            let selected = envs
                .iter()
                .find(|(id, _)| Some(id) == active.as_ref())
                .map(|(_, name)| name.clone())
                .unwrap_or_default();
            ComboBox::from_id_salt("environment")
                .selected_text((RichText::new("⏺").size(9.0).color(p.ok), " ", selected))
                .show_ui(ui, |ui| {
                    for (id, name) in &envs {
                        ui.selectable_value(&mut active, Some(id.clone()), name);
                    }
                });
            if active != before
                && let Some(id) = active
            {
                self.state.set_active_environment(root, &id);
            }
            ui.add(theme::ghost_button("✏"))
                .on_hover_text("Edit environments, servers and variables")
                .clicked()
        };
        if edit {
            self.modal = Some(Modal::Environments(EnvironmentsDialog::new(
                &self.state,
                root.to_owned(),
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
                        .button("Save to collection…")
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
    }

    /// Drops state of deleted or closed items (`prefix` "" = the whole collection).
    fn forget(&mut self, root: &Path, prefix: &Path) {
        let gone = |k: &ItemKey| k.root == root && k.path.starts_with(prefix);
        self.drafts.retain(|k, _| !gone(k));
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
        let tx = self.finished_tx.clone();
        let repaint = egui_ctx.clone();
        let (task, abort) = abortable(async move {
            let result = client.execute(&prepared).await;
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
            self.outcomes.insert(f.key, Outcome::Cancelled);
        }
    }

    fn collect_finished(&mut self) {
        while let Ok(done) = self.finished_rx.try_recv() {
            if let Some(f) = self.in_flight.take_if(|f| f.id == done.id) {
                if f.key == self.selection {
                    self.response = ResponseState::default();
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
            self.remap(&key.root, &key.path, &new_path);
        }
    }

    fn revert(&mut self) {
        if let Some(key) = self.selection.clone() {
            self.drafts.remove(&key);
            self.load(self.state.saved_request(&key).unwrap_or_default());
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
            SidebarAction::Reload(root) => self.state.reload(&root),
            SidebarAction::Close(root) => {
                self.forget(&root, Path::new(""));
                self.state.close_collection(&root);
            }
            SidebarAction::OpenFolder => self.open_folder(),
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

    fn open_folder(&mut self) {
        let Some(dir) = self.state.host.pick_folder("Open or create a collection") else {
            return;
        };
        let manifest = dir.join(tortuga_core::collection::MANIFEST_FILE);
        if self.state.storage().exists(&manifest) {
            if let Err(e) = self.state.open_collection(&dir) {
                self.state.notify(e.to_string());
            }
        } else {
            self.modal = Some(Modal::CreateCollection(CreateCollectionDialog::new(dir)));
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
            Modal::CreateCollection(d) => match d.show(ctx) {
                DialogResult::Done(name) => {
                    if let Err(e) = self.state.create_collection(&d.dir, &name) {
                        self.state.notify(e.to_string());
                    }
                    false
                }
                result => result == DialogResult::Open,
            },
        };
        // A dialog may have opened another one (none do today); never overwrite it.
        if keep && self.modal.is_none() {
            self.modal = Some(modal);
        }
    }
}

impl TortugaUi {
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
