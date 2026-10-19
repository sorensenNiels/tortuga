use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;

use egui::{ComboBox, Context, Id, Modal, RichText, TextEdit, Ui};
use tortuga_core::import::ImportKind;
use tortuga_core::settings::{Appearance, HistorySettings, ImporterSettings, Theme};
use tortuga_core::{AppSettings, ClientSettings, CookieInfo};

use super::request_editor::parse_timeout;
use crate::state::{AppState, ItemKey};
use crate::theme;

/// What a dialog wants after this frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogResult<T> {
    Open,
    Close,
    Done(T),
}

/// Shows a modal dialog with a heading. Escape or a click outside closes it
/// when `dismissable` (dialogs with a lot of input are not, so nothing is lost by accident).
pub(super) fn dialog<T>(
    ctx: &Context,
    id: &str,
    title: &str,
    width: f32,
    dismissable: bool,
    content: impl FnOnce(&mut Ui) -> DialogResult<T>,
) -> DialogResult<T> {
    let frame = egui::Frame::window(&ctx.global_style()).inner_margin(egui::Margin::same(22));
    let response = Modal::new(Id::unique(id)).frame(frame).show(ctx, |ui| {
        ui.set_width(width.min(ctx.content_rect().width() - 48.0));
        ui.heading(title);
        ui.add_space(10.0);
        content(ui)
    });
    match response.inner {
        DialogResult::Open if dismissable && response.should_close() => DialogResult::Close,
        other => other,
    }
}

/// Cancel and confirm buttons, right-aligned. Returns (cancel clicked, confirm clicked).
fn actions(ui: &mut Ui, confirm: egui::Button<'_>, confirm_enabled: bool) -> (bool, bool) {
    ui.add_space(16.0);
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let confirm = ui.add_enabled(confirm_enabled, confirm).clicked();
        let cancel = ui.button("Cancel").clicked();
        (cancel, confirm)
    })
    .inner
}

fn finish<T>(cancel: bool, done: Option<T>) -> DialogResult<T> {
    match done {
        Some(value) => DialogResult::Done(value),
        None if cancel => DialogResult::Close,
        None => DialogResult::Open,
    }
}

/// A folder an item can go to: (display label, path relative to the root).
fn destinations(state: &AppState, root: &Path) -> Vec<(String, PathBuf)> {
    let Some(c) = state.collection(root) else {
        return Vec::new();
    };
    let mut out = vec![(format!("{} (top level)", c.name()), PathBuf::new())];
    for f in c.folders() {
        let depth = f.path.components().count();
        out.push((
            format!("{}{}", "    ".repeat(depth), f.name),
            f.path.clone(),
        ));
    }
    out
}

fn folder_picker(ui: &mut Ui, id: &str, choices: &[(String, PathBuf)], target: &mut PathBuf) {
    let selected = choices
        .iter()
        .find(|(_, p)| p == target)
        .map(|(label, _)| label.trim().to_owned())
        .unwrap_or_default();
    ComboBox::from_id_salt(id)
        .selected_text(selected)
        .width(ui.available_width())
        .show_ui(ui, |ui| {
            for (label, path) in choices {
                ui.selectable_value(target, path.clone(), label);
            }
        });
}

fn field_label(ui: &mut Ui, text: &str) {
    ui.add_space(8.0);
    let color = theme::palette(ui).muted;
    ui.label(theme::semibold(text).size(12.0).color(color));
}

// ---------------------------------------------------------------- confirm

#[derive(Debug, Clone)]
pub struct ConfirmDialog {
    pub title: String,
    pub message: String,
    pub confirm_label: String,
}

impl ConfirmDialog {
    pub fn show(&self, ctx: &Context) -> DialogResult<()> {
        dialog(ctx, "confirm", &self.title, 380.0, true, |ui| {
            ui.label(&self.message);
            let (cancel, confirm) =
                actions(ui, theme::danger_button(ui, &self.confirm_label), true);
            finish(cancel, confirm.then_some(()))
        })
    }
}

// ---------------------------------------------------------------- move

/// Pick a destination folder in the same project (R-COL-2).
#[derive(Debug, Clone)]
pub struct MoveDialog {
    pub item: ItemKey,
    target: PathBuf,
}

impl MoveDialog {
    pub fn new(item: ItemKey) -> Self {
        let target = current_parent(&item);
        Self { item, target }
    }

    pub fn show(&mut self, ctx: &Context, state: &AppState) -> DialogResult<PathBuf> {
        let item = &self.item;
        let choices: Vec<(String, PathBuf)> = destinations(state, &item.root)
            .into_iter()
            .filter(|(_, p)| !p.starts_with(&item.path) || p.as_os_str().is_empty())
            .collect();
        let name = state
            .collection(&item.root)
            .and_then(|c| c.find(&item.path).map(tortuga_core::Item::name))
            .unwrap_or_default();
        let unchanged = self.target == current_parent(item);
        let target = &mut self.target;
        dialog(
            ctx,
            "move",
            &format!("Move “{name}”"),
            380.0,
            true,
            |ui| {
                field_label(ui, "To folder");
                folder_picker(ui, "move-target", &choices, target);
                let (cancel, confirm) = actions(ui, theme::primary_button(ui, "Move"), !unchanged);
                finish(cancel, confirm.then(|| target.clone()))
            },
        )
    }
}

fn current_parent(item: &ItemKey) -> PathBuf {
    item.path.parent().map(PathBuf::from).unwrap_or_default()
}

// ---------------------------------------------------------------- save to project

/// Save the scratch request into a project (R-COL-7 → R-COL-1).
#[derive(Debug, Clone)]
pub struct SaveToCollectionDialog {
    root: Option<PathBuf>,
    folder: PathBuf,
    name: String,
}

impl SaveToCollectionDialog {
    pub fn new(state: &AppState, initial_name: &str) -> Self {
        Self {
            root: state
                .collections
                .iter()
                .find(|c| !c.read_only)
                .map(|c| c.root.clone()),
            folder: PathBuf::new(),
            name: if initial_name == "Untitled request" {
                String::new()
            } else {
                initial_name.to_owned()
            },
        }
    }

    /// Done with (folder to save in, request name).
    pub fn show(&mut self, ctx: &Context, state: &AppState) -> DialogResult<(ItemKey, String)> {
        let folders = self
            .root
            .as_ref()
            .map(|r| destinations(state, r))
            .unwrap_or_default();
        dialog(
            ctx,
            "save-to-collection",
            "Save request",
            420.0,
            true,
            |ui| {
                if state.collections.is_empty() {
                    ui.label("Add or create a project first (“+ Add” in the sidebar).");
                } else {
                    field_label(ui, "Name");
                    ui.add(
                        TextEdit::singleline(&mut self.name)
                            .hint_text("e.g. List users")
                            .desired_width(f32::INFINITY),
                    );
                    field_label(ui, "Project");
                    let selected = self
                        .root
                        .as_ref()
                        .and_then(|r| state.collection(r))
                        .map(|c| c.name().to_owned())
                        .unwrap_or_default();
                    let before = self.root.clone();
                    ComboBox::from_id_salt("save-collection")
                        .selected_text(selected)
                        .width(ui.available_width())
                        .show_ui(ui, |ui| {
                            for c in &state.collections {
                                ui.add_enabled_ui(!c.read_only, |ui| {
                                    ui.selectable_value(
                                        &mut self.root,
                                        Some(c.root.clone()),
                                        c.name(),
                                    );
                                });
                            }
                        });
                    if self.root != before {
                        self.folder = PathBuf::new();
                    }
                    field_label(ui, "Folder");
                    folder_picker(ui, "save-folder", &folders, &mut self.folder);
                }
                let name = self.name.trim();
                let ok = self.root.is_some() && !name.is_empty();
                let (cancel, confirm) = actions(ui, theme::primary_button(ui, "Save"), ok);
                let done = confirm
                    .then_some(self.root.as_ref())
                    .flatten()
                    .map(|r| (ItemKey::new(r, self.folder.clone()), name.to_owned()));
                finish(cancel, done)
            },
        )
    }
}

// ---------------------------------------------------------------- create project

/// The chosen folder is not a project yet: offer to create one there (R-WS-7).
#[derive(Debug, Clone)]
pub struct CreateProjectDialog {
    pub dir: PathBuf,
    name: String,
    /// The git repository the folder is already in, if any.
    repository: Option<PathBuf>,
    can_init_git: bool,
    init_git: bool,
}

impl CreateProjectDialog {
    pub fn new(dir: PathBuf, repository: Option<PathBuf>, can_init_git: bool) -> Self {
        let name = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        Self {
            dir,
            name,
            repository,
            can_init_git,
            init_git: false,
        }
    }

    /// Done with (project name, make the folder a git repository).
    pub fn show(&mut self, ctx: &Context) -> DialogResult<(String, bool)> {
        dialog(ctx, "create-project", "New project", 440.0, true, |ui| {
            theme::hint(
                ui,
                format!(
                    "Tortuga will add a tortuga.toml file to {}; requests are saved next to it.",
                    self.dir.display()
                ),
            );
            field_label(ui, "Project name");
            ui.add(TextEdit::singleline(&mut self.name).desired_width(f32::INFINITY));
            ui.add_space(8.0);
            match &self.repository {
                Some(repo) => theme::hint(
                    ui,
                    format!(
                        "Inside the git repository at {}: commit the project with your code.",
                        repo.display()
                    ),
                ),
                None if self.can_init_git => {
                    ui.checkbox(&mut self.init_git, "Make the folder a git repository")
                        .on_hover_text("So the project can be shared with git (git init)");
                }
                None => {}
            }
            let name = self.name.trim();
            let (cancel, confirm) =
                actions(ui, theme::primary_button(ui, "Create"), !name.is_empty());
            finish(cancel, confirm.then(|| (name.to_owned(), self.init_git)))
        })
    }
}

// ---------------------------------------------------------------- add existing projects

/// What the user chose in [`AddProjectsDialog`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddProjects {
    Add(Vec<PathBuf>),
    /// Nothing was found: create a project in the chosen folder instead.
    CreateHere(PathBuf),
}

/// Projects found in a folder or repository, as a checklist (R-WS-3).
#[derive(Debug)]
pub struct AddProjectsDialog {
    pub dir: PathBuf,
    results: Receiver<Result<Vec<PathBuf>, String>>,
    /// `None` while searching; then each project and whether it is ticked.
    found: Option<Result<Vec<(PathBuf, bool)>, String>>,
}

impl AddProjectsDialog {
    /// `results` delivers the search started for `dir`.
    pub fn new(dir: PathBuf, results: Receiver<Result<Vec<PathBuf>, String>>) -> Self {
        Self {
            dir,
            results,
            found: None,
        }
    }

    pub fn show(&mut self, ctx: &Context, state: &AppState) -> DialogResult<AddProjects> {
        if self.found.is_none()
            && let Ok(result) = self.results.try_recv()
        {
            // Everything starts ticked; projects already open stay ticked and disabled.
            self.found = Some(result.map(|paths| paths.into_iter().map(|p| (p, true)).collect()));
        }
        let dir = self.dir.clone();
        dialog(
            ctx,
            "add-projects",
            "Add existing projects",
            520.0,
            true,
            |ui| {
                let mut create_here = false;
                match &mut self.found {
                    None => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(format!("Looking for projects in {}…", dir.display()));
                        });
                    }
                    Some(Err(e)) => theme::error(ui, e.as_str()),
                    Some(Ok(found)) if found.is_empty() => {
                        ui.label(format!(
                            "No projects (folders with a tortuga.toml file) in {}.",
                            dir.display()
                        ));
                        ui.add_space(8.0);
                        create_here = ui.button("Create a project there…").clicked();
                    }
                    Some(Ok(found)) => {
                        theme::hint(
                            ui,
                            format!(
                                "Found in {}. Git-ignored folders were skipped.",
                                dir.display()
                            ),
                        );
                        ui.add_space(6.0);
                        egui::ScrollArea::vertical()
                            .max_height(320.0)
                            .show(ui, |ui| {
                                for (path, ticked) in found.iter_mut() {
                                    let open = state.collection(path).is_some();
                                    let label = match path.strip_prefix(&dir) {
                                        Ok(rel) if rel.as_os_str().is_empty() => ".".to_owned(),
                                        Ok(rel) => rel.display().to_string(),
                                        Err(_) => path.display().to_string(),
                                    };
                                    let label = if open {
                                        format!("{label}  (already in the workspace)")
                                    } else {
                                        label
                                    };
                                    ui.add_enabled(!open, egui::Checkbox::new(ticked, label));
                                }
                            });
                    }
                }
                let chosen: Vec<PathBuf> = match &self.found {
                    Some(Ok(found)) => found
                        .iter()
                        .filter(|(p, ticked)| *ticked && state.collection(p).is_none())
                        .map(|(p, _)| p.clone())
                        .collect(),
                    _ => Vec::new(),
                };
                let label = match chosen.len() {
                    1 => "Add 1 project".to_owned(),
                    n => format!("Add {n} projects"),
                };
                let (cancel, confirm) =
                    actions(ui, theme::primary_button(ui, &label), !chosen.is_empty());
                if create_here {
                    return DialogResult::Done(AddProjects::CreateHere(dir.clone()));
                }
                finish(cancel, confirm.then_some(AddProjects::Add(chosen)))
            },
        )
    }
}

// ---------------------------------------------------------------- copy as curl

/// The request uses secret values: copy them masked, or include them (R-HTTP-9, P6).
#[derive(Debug, Clone)]
pub struct CopyCurlDialog {
    command: tortuga_core::curl::CurlCommand,
}

impl CopyCurlDialog {
    pub fn new(command: tortuga_core::curl::CurlCommand) -> Self {
        Self { command }
    }

    /// Done with the text to copy.
    pub fn show(&self, ctx: &Context) -> DialogResult<String> {
        let names = self.command.secrets.join(", ");
        dialog(ctx, "copy-curl", "Copy as curl", 460.0, true, |ui| {
            ui.label(format!(
                "This request uses secret values ({names}). Copy it with them shown as <secret:name>, or include the real values?"
            ));
            ui.add_space(6.0);
            theme::hint(
                ui,
                "Commands you paste into chats, tickets or shell history may be kept for a long time.",
            );
            ui.add_space(16.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let masked = ui.add(theme::primary_button(ui, "Copy masked")).clicked();
                let full = ui.button("Include secret values").clicked();
                let cancel = ui.button("Cancel").clicked();
                let done = if masked {
                    self.command.masked.clone()
                } else if full {
                    Some(self.command.full.clone())
                } else {
                    None
                };
                finish(cancel, done)
            })
            .inner
        })
    }
}

// ---------------------------------------------------------------- name

/// Asks for a name, e.g. of a workspace.
#[derive(Debug, Clone)]
pub struct NameDialog {
    title: String,
    confirm_label: String,
    name: String,
    focused: bool,
}

impl NameDialog {
    pub fn new(title: &str, confirm_label: &str, name: &str) -> Self {
        Self {
            title: title.to_owned(),
            confirm_label: confirm_label.to_owned(),
            name: name.to_owned(),
            focused: false,
        }
    }

    pub fn show(&mut self, ctx: &Context) -> DialogResult<String> {
        dialog(ctx, "name", &self.title.clone(), 380.0, true, |ui| {
            field_label(ui, "Name");
            let field = ui.add(
                TextEdit::singleline(&mut self.name)
                    .id_salt("name-dialog")
                    .desired_width(f32::INFINITY),
            );
            if !self.focused {
                field.request_focus();
                self.focused = true;
            }
            let name = self.name.trim().to_owned();
            let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let (cancel, confirm) = actions(
                ui,
                theme::primary_button(ui, &self.confirm_label),
                !name.is_empty(),
            );
            finish(
                cancel,
                ((confirm || enter) && !name.is_empty()).then_some(name),
            )
        })
    }
}

// ---------------------------------------------------------------- settings

/// Global HTTP settings (R-HTTP-7, R-HTTP-8).
/// The tabs of the settings dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingsTab {
    Requests,
    Appearance,
    History,
    Importers,
}

#[derive(Debug, Clone)]
pub struct SettingsDialog {
    draft: AppSettings,
    tab: SettingsTab,
    timeout_text: String,
    /// Hosts not to verify, one per line (R-SET-3).
    hosts_text: String,
    /// History entries per project while history is on (R-SET-6).
    history_text: String,
    /// Shown when saving failed.
    pub error: Option<String>,
    /// "Add…" was clicked for a CA certificate; the shell asks for the file (R-HTTP-16).
    pub add_ca_requested: bool,
}

impl SettingsDialog {
    pub fn new(settings: &AppSettings) -> Self {
        Self {
            draft: settings.clone(),
            tab: SettingsTab::Requests,
            timeout_text: settings.client.default_timeout_ms.to_string(),
            hosts_text: settings.client.insecure_hosts.join("\n"),
            history_text: match settings.history.size {
                0 => tortuga_core::history::DEFAULT_SIZE,
                n => n,
            }
            .to_string(),
            error: None,
            add_ca_requested: false,
        }
    }

    /// Adds a CA certificate file chosen after "Add…" (R-HTTP-16).
    pub fn add_ca(&mut self, path: PathBuf) {
        let files = &mut self.draft.client.ca_certificates;
        if !files.contains(&path) {
            files.push(path);
        }
    }

    /// Done with the settings to apply. `can_pick_files`: offer "Add…" for CA certificates.
    pub fn show(&mut self, ctx: &Context, can_pick_files: bool) -> DialogResult<AppSettings> {
        dialog(ctx, "settings", "Settings", 460.0, true, |ui| {
            theme::tabs(
                ui,
                &mut self.tab,
                &[
                    (SettingsTab::Requests, "Requests", None),
                    (SettingsTab::Appearance, "Appearance", None),
                    (SettingsTab::History, "History", None),
                    (SettingsTab::Importers, "Importers", None),
                ],
            );
            match self.tab {
                SettingsTab::Requests => {
                    self.add_ca_requested |= request_settings(
                        ui,
                        &mut self.draft.client,
                        &mut self.timeout_text,
                        &mut self.hosts_text,
                        can_pick_files,
                    );
                }
                SettingsTab::Appearance => appearance_settings(ui, &mut self.draft.appearance),
                SettingsTab::History => {
                    history_settings(ui, &mut self.draft.history, &mut self.history_text);
                }
                SettingsTab::Importers => importer_settings(ui, &mut self.draft.importers),
            }
            if let Some(err) = &self.error {
                theme::error(ui, err);
            }
            let (cancel, confirm) = actions(ui, theme::primary_button(ui, "Save"), true);
            finish(cancel, confirm.then(|| self.draft.clone()))
        })
    }
}

/// HTTP settings (R-HTTP-7, R-HTTP-8, R-HTTP-16, R-SET-3). Returns whether "Add…" was
/// clicked for a CA certificate.
fn request_settings(
    ui: &mut Ui,
    d: &mut ClientSettings,
    timeout_text: &mut String,
    hosts_text: &mut String,
    can_pick_files: bool,
) -> bool {
    ui.checkbox(&mut d.follow_redirects, "Follow redirects");
    ui.checkbox(&mut d.send_cookies, "Send and store cookies");
    ui.checkbox(&mut d.verify_tls, "Verify TLS certificates");
    if d.verify_tls {
        insecure_hosts(ui, d, hosts_text);
    } else {
        theme::warning(
            ui,
            "Turning this off lets anyone between you and the server read and change your traffic. Only use it for local test servers.",
        );
    }
    ui.horizontal(|ui| {
        let label = ui.label("Default timeout (ms)");
        if ui
            .add(TextEdit::singleline(timeout_text).desired_width(120.0))
            .labelled_by(label.id)
            .changed()
            && let Some(ms) = parse_timeout(timeout_text)
        {
            d.default_timeout_ms = ms;
        }
    });
    theme::hint(
        ui,
        "Workspaces and projects can set their own timeout, redirects and cookies (their \
         “Defaults…”). Proxy settings are taken from the operating system.",
    );
    ui.add_space(10.0);
    ca_certificates(ui, &mut d.ca_certificates, can_pick_files)
}

/// Hosts whose certificates are not verified, on this machine only (R-SET-3).
fn insecure_hosts(ui: &mut Ui, d: &mut ClientSettings, text: &mut String) {
    ui.indent("insecure-hosts", |ui| {
        let label = ui.label("Don't verify certificates of these hosts:");
        let edit = TextEdit::multiline(text)
            .id_salt("insecure-hosts")
            .font(egui::TextStyle::Monospace)
            .hint_text("localhost\n*.dev.internal")
            .desired_rows(2)
            .desired_width(300.0);
        if ui.add(edit).labelled_by(label.id).changed() {
            d.insecure_hosts = text
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_owned)
                .collect();
        }
        theme::hint(
            ui,
            "One per line; *.example.test covers its subdomains. Only for test servers with \
             self-signed certificates. Kept on this computer: shared files can't turn \
             verification off.",
        );
        if !d.insecure_hosts.is_empty() {
            theme::warning(
                ui,
                "Traffic to these hosts could be read and changed by anyone in between.",
            );
        }
    });
}

/// Theme, text size and wrapping (R-SET-4, R-SET-5, R-SET-7).
fn appearance_settings(ui: &mut Ui, a: &mut Appearance) {
    egui::Grid::new("appearance")
        .num_columns(2)
        .spacing([16.0, 10.0])
        .show(ui, |ui| {
            ui.label("Theme");
            ui.horizontal(|ui| {
                ui.radio_value(&mut a.theme, Theme::System, "Like the system");
                ui.radio_value(&mut a.theme, Theme::Light, "Light");
                ui.radio_value(&mut a.theme, Theme::Dark, "Dark");
            });
            ui.end_row();
            let label = ui.label("Text size");
            let mut sizes = vec![80, 90, 100, 110, 125, 150, 175, 200];
            if !sizes.contains(&a.text_size) {
                sizes.push(a.text_size);
                sizes.sort_unstable();
            }
            ComboBox::from_id_salt("text-size")
                .selected_text(format!("{} %", a.text_size))
                .show_ui(ui, |ui| {
                    for size in sizes {
                        ui.selectable_value(&mut a.text_size, size, format!("{size} %"));
                    }
                })
                .response
                .labelled_by(label.id);
            ui.end_row();
        });
    theme::hint(
        ui,
        "Ctrl+= and Ctrl+- change the text size too; Ctrl+0 resets it.",
    );
    ui.add_space(8.0);
    ui.checkbox(&mut a.wrap_lines, "Wrap long lines in responses");
}

/// How many sent requests are kept per project, or none (R-SET-6).
fn history_settings(ui: &mut Ui, h: &mut HistorySettings, text: &mut String) {
    let mut on = h.on();
    if ui
        .checkbox(&mut on, "Keep a history of sent requests")
        .changed()
    {
        h.size = if on {
            text.trim()
                .parse()
                .ok()
                .filter(|n| *n > 0)
                .unwrap_or(tortuga_core::history::DEFAULT_SIZE)
        } else {
            0
        };
    }
    if on {
        ui.horizontal(|ui| {
            let label = ui.label("Entries per project");
            if ui
                .add(TextEdit::singleline(text).desired_width(80.0))
                .labelled_by(label.id)
                .changed()
                && let Some(n) = text.trim().parse().ok().filter(|n: &usize| *n > 0)
            {
                h.size = n;
            }
        });
        theme::hint(ui, "Lowering it removes the oldest entries when you save.");
    } else {
        theme::hint(
            ui,
            "Nothing more is recorded. History that is already there stays until you clear it \
             in the History view.",
        );
    }
}

/// Which importers "+ Add" and the command palette offer (R-COL-18).
fn importer_settings(ui: &mut Ui, importers: &mut ImporterSettings) {
    theme::hint(
        ui,
        "Turn off the importers you don't use; they disappear from “+ Add” and the command \
         palette. Projects you imported earlier are not affected.",
    );
    ui.add_space(6.0);
    for kind in ImportKind::ALL {
        let mut on = importers.enabled(kind);
        if ui.checkbox(&mut on, kind.importer_name()).changed() {
            importers.set_enabled(kind, on);
        }
    }
}

/// The runtime variables of a project, with "Clear all" (R-TEST-5).
#[derive(Debug, Clone)]
pub struct RuntimeVarsDialog {
    /// The project (`None`: the scratch request).
    pub root: Option<PathBuf>,
    title: String,
}

impl RuntimeVarsDialog {
    pub fn new(root: Option<PathBuf>, place: &str) -> Self {
        Self {
            root,
            title: format!("Runtime variables — {place}"),
        }
    }

    /// `Done` means "clear them all".
    pub fn show(
        &self,
        ctx: &Context,
        vars: Option<&std::collections::BTreeMap<String, String>>,
    ) -> DialogResult<()> {
        dialog(ctx, "runtime-vars", &self.title, 520.0, true, |ui| {
            theme::hint(
                ui,
                "Values captured from responses or set by scripts. They win over environment and \
                 project values, and are forgotten when Tortuga closes.",
            );
            ui.add_space(8.0);
            match vars {
                None => theme::hint(ui, "None yet."),
                Some(vars) => {
                    let p = theme::palette(ui);
                    egui::ScrollArea::vertical()
                        .max_height(320.0)
                        .show(ui, |ui| {
                            egui::Grid::new("runtime-vars")
                                .num_columns(2)
                                .spacing([16.0, 4.0])
                                .show(ui, |ui| {
                                    for (name, value) in vars {
                                        ui.label(RichText::new(name).monospace());
                                        let shown =
                                            crate::format::truncate(value, 200).0.to_owned();
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(shown).monospace().color(p.muted),
                                            )
                                            .truncate(),
                                        )
                                        .on_hover_text(value);
                                        ui.end_row();
                                    }
                                });
                        });
                }
            }
            let (close, clear) = actions(ui, theme::danger_button(ui, "Clear all"), vars.is_some());
            finish(close, clear.then_some(()))
        })
    }
}

/// What the user did in the cookie viewer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CookieAction {
    /// Remove this one; the dialog stays open.
    Remove(CookieInfo),
    /// Empty the jar; the dialog closes.
    ClearAll,
}

/// The cookies in a project's jar, each with a remove button, and "Clear all" (R-CK-5).
#[derive(Debug, Clone)]
pub struct CookiesDialog {
    pub root: PathBuf,
    title: String,
}

impl CookiesDialog {
    pub fn new(root: PathBuf, place: &str) -> Self {
        Self {
            root,
            title: format!("Cookies — {place}"),
        }
    }

    /// `now_ms` and `offset_minutes` format the expiry dates in local time.
    pub fn show(
        &self,
        ctx: &Context,
        cookies: &[CookieInfo],
        now_ms: u64,
        offset_minutes: i32,
    ) -> (DialogResult<()>, Option<CookieAction>) {
        let mut action = None;
        let result = dialog(ctx, "cookies", &self.title, 760.0, true, |ui| {
            theme::hint(
                ui,
                "Set by responses and sent with later matching requests. Kept on this computer \
                 only, never in the project.",
            );
            ui.add_space(8.0);
            if cookies.is_empty() {
                theme::hint(ui, "The jar is empty.");
            } else {
                let p = theme::palette(ui);
                egui::ScrollArea::vertical()
                    .max_height(360.0)
                    .show(ui, |ui| {
                        egui::Grid::new("cookies")
                            .num_columns(6)
                            .spacing([14.0, 6.0])
                            .striped(true)
                            .show(ui, |ui| {
                                for heading in
                                    ["", "Name", "Value", "Domain and path", "Expires", ""]
                                {
                                    theme::section_label(ui, heading);
                                }
                                ui.end_row();
                                for c in cookies {
                                    if ui
                                        .add(theme::icon_button(ui, "🗙"))
                                        .on_hover_text("Remove this cookie")
                                        .clicked()
                                    {
                                        action = Some(CookieAction::Remove(c.clone()));
                                    }
                                    ui.label(RichText::new(&c.name).monospace());
                                    let shown = crate::format::truncate(&c.value, 60).0.to_owned();
                                    // A truncated label in a Grid gets almost no width of its own.
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(200.0, ui.spacing().interact_size.y),
                                        egui::Layout::left_to_right(egui::Align::Center),
                                        |ui| {
                                            ui.set_width(200.0);
                                            ui.add(
                                                egui::Label::new(
                                                    RichText::new(shown).monospace().color(p.muted),
                                                )
                                                .truncate(),
                                            )
                                            .on_hover_text(&c.value);
                                        },
                                    );
                                    let place = cookie_place(c);
                                    ui.label(&place.0).on_hover_text(place.1);
                                    ui.label(cookie_expiry(c, now_ms, offset_minutes));
                                    ui.label(RichText::new(cookie_flags(c)).small().color(p.muted));
                                    ui.end_row();
                                }
                            });
                    });
            }
            let (close, clear) = actions(
                ui,
                theme::danger_button(ui, "Clear all"),
                !cookies.is_empty(),
            );
            if clear {
                action = Some(CookieAction::ClearAll);
            }
            finish(close, clear.then_some(()))
        });
        (result, action)
    }
}

/// "example.test/api" and what it means, for a cookie row.
fn cookie_place(c: &CookieInfo) -> (String, String) {
    let path = if c.path == "/" { "" } else { c.path.as_str() };
    let shown = if c.host_only {
        format!("{}{path}", c.domain)
    } else {
        format!("*.{}{path}", c.domain)
    };
    let hover = if c.host_only {
        format!("Sent to {} only, for paths under {}", c.domain, c.path)
    } else {
        format!(
            "Sent to {} and its subdomains, for paths under {}",
            c.domain, c.path
        )
    };
    (shown, hover)
}

fn cookie_expiry(c: &CookieInfo, now_ms: u64, offset_minutes: i32) -> String {
    match c.expires_ms {
        None => "Session".into(),
        Some(ms) => format!(
            "{} {}",
            crate::format::day_label(ms, now_ms, offset_minutes),
            crate::format::clock(ms, offset_minutes)
        ),
    }
}

fn cookie_flags(c: &CookieInfo) -> String {
    [(c.secure, "Secure"), (c.http_only, "HttpOnly")]
        .iter()
        .filter(|(on, _)| *on)
        .map(|(_, name)| *name)
        .collect::<Vec<_>>()
        .join(" · ")
}

/// CA certificate files trusted besides the system's (R-HTTP-16). Returns whether "Add…"
/// was clicked.
fn ca_certificates(ui: &mut Ui, files: &mut Vec<PathBuf>, can_pick_files: bool) -> bool {
    theme::section_label(ui, "CA certificates");
    let mut remove = None;
    for (i, path) in files.iter().enumerate() {
        ui.horizontal(|ui| {
            if ui
                .add(theme::icon_button(ui, "🗙"))
                .on_hover_text("Stop trusting it")
                .clicked()
            {
                remove = Some(i);
            }
            ui.add(egui::Label::new(path.display().to_string()).truncate())
                .on_hover_text(path.display().to_string());
        });
    }
    if let Some(i) = remove {
        files.remove(i);
    }
    let add = can_pick_files && ui.button("Add…").clicked();
    theme::hint(
        ui,
        "PEM files whose certificates are trusted besides the system's, e.g. your company's CA \
         for internal APIs. Kept on this computer only.",
    );
    add
}

#[cfg(test)]
mod tests {
    use egui_kittest::kittest::Queryable;

    use super::*;
    use crate::test_host::TestHost;

    /// R-CK-5: the cookie viewer lists cookies with where they go and when they expire;
    /// 🗙 removes one, "Clear all" empties the jar.
    #[test]
    fn cookie_viewer_lists_removes_and_clears() {
        let cookie = |name: &str, host_only, expires_ms| CookieInfo {
            domain: "example.test".into(),
            host_only,
            path: "/".into(),
            name: name.into(),
            value: format!("{name}-value"),
            expires_ms,
            secure: true,
            http_only: false,
        };
        let cookies = [
            cookie("session", true, None),
            cookie("theme", false, Some(4_733_510_400_000)),
        ];
        let dialog = CookiesDialog::new(PathBuf::from("/p"), "API");
        let mut actions = Vec::new();
        {
            let mut h = crate::test_host::harness(|ui| {
                let (_, action) = dialog.show(ui.ctx(), &cookies, 0, 0);
                actions.extend(action);
            });
            h.run();
            h.get_by_label("Cookies — API");
            h.get_by_label("session-value");
            h.get_by_label("example.test");
            h.get_by_label("*.example.test");
            h.get_by_label("Session");
            h.get_by_label("1 Jan 2120 00:00");
            h.query_all_by_label("🗙")
                .nth(1)
                .expect("second row")
                .click();
            h.run();
            h.get_by_label("Clear all").click();
            h.run();
        }
        assert_eq!(
            actions,
            [
                CookieAction::Remove(cookies[1].clone()),
                CookieAction::ClearAll
            ]
        );
    }

    /// R-HTTP-16: CA certificates are listed, added and removed in the settings.
    #[test]
    fn ca_certificates_in_settings() {
        let mut settings = AppSettings::default();
        settings.client.ca_certificates = vec![PathBuf::from("/etc/old-ca.pem")];
        let mut dialog = SettingsDialog::new(&settings);
        let mut result = None;
        {
            let mut h = crate::test_host::harness(|ui| {
                if let DialogResult::Done(r) = dialog.show(ui.ctx(), true) {
                    result = Some(r);
                }
            });
            h.run();
            h.get_by_label("/etc/old-ca.pem");
            h.get_by_label("Add…").click();
            h.run();
            h.get_by_label("🗙").click();
            h.run();
            h.get_by_label("Save").click();
            h.run();
        }
        assert!(dialog.add_ca_requested);
        assert_eq!(
            result.expect("saved").client.ca_certificates,
            Vec::<PathBuf>::new()
        );
        dialog.add_ca(PathBuf::from("/etc/company-ca.pem"));
        dialog.add_ca(PathBuf::from("/etc/company-ca.pem"));
        assert_eq!(
            dialog.draft.client.ca_certificates,
            [PathBuf::from("/etc/company-ca.pem")]
        );
    }

    /// R-SET-3..6: hosts not to verify, theme, wrapping and history are set in the
    /// settings dialog.
    #[test]
    fn requests_appearance_and_history_settings() {
        let mut dialog = SettingsDialog::new(&AppSettings::default());
        let mut result = None;
        {
            let mut h = crate::test_host::harness(|ui| {
                if let DialogResult::Done(r) = dialog.show(ui.ctx(), true) {
                    result = Some(r);
                }
            });
            h.run();
            h.get_by_label("Don't verify certificates of these hosts:");
            let field = h
                .get_all_by_role(egui::accesskit::Role::MultilineTextInput)
                .next()
                .expect("hosts field");
            field.focus();
            field.type_text("localhost\n *.dev.test ");
            h.run();
            h.get_by_label("Appearance").click();
            h.run();
            h.get_by_label("Dark").click();
            h.get_by_label("Wrap long lines in responses").click();
            h.run();
            h.get_by_label("History").click();
            h.run();
            h.get_by_label("Keep a history of sent requests").click();
            h.run();
            h.get_by_label("Save").click();
            h.run();
        }
        let saved = result.expect("saved");
        assert_eq!(saved.client.insecure_hosts, ["localhost", "*.dev.test"]);
        assert_eq!(saved.appearance.theme, Theme::Dark);
        assert!(saved.appearance.wrap_lines);
        assert_eq!(saved.history.size, 0, "history off");
    }

    /// R-COL-18: importers are turned off on the Importers tab.
    #[test]
    fn importers_are_turned_off_in_settings() {
        let mut dialog = SettingsDialog::new(&AppSettings::default());
        let mut result = None;
        {
            let mut h = crate::test_host::harness(|ui| {
                if let DialogResult::Done(r) = dialog.show(ui.ctx(), true) {
                    result = Some(r);
                }
            });
            h.run();
            h.get_by_label("Importers").click();
            h.run();
            h.get_by_label("Postman collections").click();
            h.run();
            h.get_by_label("Save").click();
            h.run();
        }
        let saved = result.expect("saved");
        assert!(!saved.importers.enabled(ImportKind::Postman));
        assert!(saved.importers.enabled(ImportKind::OpenApi));
        assert!(saved.importers.enabled(ImportKind::Http));
    }

    /// R-WS-3: found projects are a checklist; projects already open can't be added twice.
    #[test]
    fn found_projects_are_a_checklist() {
        let dir = std::env::temp_dir().join(format!("tortuga-addprojects-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let open = dir.join("users");
        tortuga_core::Collection::create(&tortuga_core::storage::FsStorage, &open, "Users")
            .expect("create");
        let state = AppState::new(Box::new(TestHost {
            open: vec![open.clone()],
            ..TestHost::default()
        }));
        let (tx, rx) = std::sync::mpsc::channel();
        let mut dialog = AddProjectsDialog::new(dir.clone(), rx);
        let mut result = None;
        {
            let mut h = crate::test_host::harness(|ui| {
                if let DialogResult::Done(r) = dialog.show(ui.ctx(), &state) {
                    result = Some(r);
                }
            });
            // `step`: the spinner keeps repainting while searching.
            h.step();
            assert!(h.query_by_label_contains("Looking for projects").is_some());
            tx.send(Ok(vec![
                dir.join("orders"),
                dir.join("payments/api"),
                open.clone(),
            ]))
            .expect("send");
            h.run();
            h.get_by_label("users  (already in the workspace)");
            h.get_by_label("orders").click(); // untick
            h.run();
            h.get_by_label("Add 1 project").click();
            h.run();
        }
        assert_eq!(
            result,
            Some(AddProjects::Add(vec![dir.join("payments/api")]))
        );
        std::fs::remove_dir_all(dir).expect("clean up");
    }

    /// R-WS-3: with nothing found, a project can be created there instead.
    #[test]
    fn nothing_found_offers_to_create_a_project() {
        let state = AppState::new(Box::new(TestHost::default()));
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(Ok(Vec::new())).expect("send");
        let dir = PathBuf::from("/somewhere");
        let mut dialog = AddProjectsDialog::new(dir.clone(), rx);
        let mut result = None;
        {
            let mut h = crate::test_host::harness(|ui| {
                if let DialogResult::Done(r) = dialog.show(ui.ctx(), &state) {
                    result = Some(r);
                }
            });
            h.run();
            h.get_by_label("Create a project there…").click();
            h.run();
        }
        assert_eq!(result, Some(AddProjects::CreateHere(dir)));
    }
}
