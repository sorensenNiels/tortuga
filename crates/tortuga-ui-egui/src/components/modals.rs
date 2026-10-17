use std::path::{Path, PathBuf};

use egui::{ComboBox, Context, Id, Modal, TextEdit, Ui};
use tortuga_core::ClientSettings;

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

/// Pick a destination folder in the same collection (R-COL-2).
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

// ---------------------------------------------------------------- save to collection

/// Save the scratch request into a collection (R-COL-7 → R-COL-1).
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
                    ui.label("Open or create a collection first (“Open…” in the sidebar).");
                } else {
                    field_label(ui, "Name");
                    ui.add(
                        TextEdit::singleline(&mut self.name)
                            .hint_text("e.g. List users")
                            .desired_width(f32::INFINITY),
                    );
                    field_label(ui, "Collection");
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

// ---------------------------------------------------------------- create collection

/// The chosen folder is not a collection yet: offer to create one there.
#[derive(Debug, Clone)]
pub struct CreateCollectionDialog {
    pub dir: PathBuf,
    name: String,
}

impl CreateCollectionDialog {
    pub fn new(dir: PathBuf) -> Self {
        let name = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        Self { dir, name }
    }

    /// Done with the collection name.
    pub fn show(&mut self, ctx: &Context) -> DialogResult<String> {
        dialog(
            ctx,
            "create-collection",
            "Create a collection",
            420.0,
            true,
            |ui| {
                theme::hint(
                    ui,
                    format!(
                        "{} is not a collection yet. Tortuga will add a tortuga.toml file there; requests are saved next to it.",
                        self.dir.display()
                    ),
                );
                field_label(ui, "Collection name");
                ui.add(TextEdit::singleline(&mut self.name).desired_width(f32::INFINITY));
                let name = self.name.trim();
                let (cancel, confirm) =
                    actions(ui, theme::primary_button(ui, "Create"), !name.is_empty());
                finish(cancel, confirm.then(|| name.to_owned()))
            },
        )
    }
}

// ---------------------------------------------------------------- settings

/// Global HTTP settings (R-HTTP-7, R-HTTP-8).
#[derive(Debug, Clone)]
pub struct SettingsDialog {
    draft: ClientSettings,
    timeout_text: String,
    /// Shown when saving failed.
    pub error: Option<String>,
}

impl SettingsDialog {
    pub fn new(settings: &ClientSettings) -> Self {
        Self {
            draft: settings.clone(),
            timeout_text: settings.default_timeout_ms.to_string(),
            error: None,
        }
    }

    /// Done with the settings to apply.
    pub fn show(&mut self, ctx: &Context) -> DialogResult<ClientSettings> {
        dialog(ctx, "settings", "Settings", 420.0, true, |ui| {
            let d = &mut self.draft;
            ui.checkbox(&mut d.follow_redirects, "Follow redirects");
            ui.checkbox(&mut d.verify_tls, "Verify TLS certificates");
            if !d.verify_tls {
                theme::warning(
                    ui,
                    "Turning this off lets anyone between you and the server read and change your traffic. Only use it for local test servers.",
                );
            }
            ui.horizontal(|ui| {
                ui.label("Default timeout (ms)");
                if ui
                    .add(TextEdit::singleline(&mut self.timeout_text).desired_width(120.0))
                    .changed()
                    && let Some(ms) = parse_timeout(&self.timeout_text)
                {
                    d.default_timeout_ms = ms;
                }
            });
            theme::hint(ui, "Proxy settings are taken from the operating system.");
            if let Some(err) = &self.error {
                theme::error(ui, err);
            }
            let (cancel, confirm) = actions(ui, theme::primary_button(ui, "Save"), true);
            finish(cancel, confirm.then(|| self.draft.clone()))
        })
    }
}
