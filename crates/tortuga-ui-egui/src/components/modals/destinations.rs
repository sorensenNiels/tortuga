//! Dialogs that pick where an item goes: a folder of its own project (move), or a
//! project and folder (save the scratch request, copy into another project).

use std::path::{Path, PathBuf};

use egui::{ComboBox, Context, TextEdit, Ui};

use super::{DialogResult, actions, dialog, field_label, finish};
use crate::state::{AppState, ItemKey};
use crate::theme;

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

fn folder_picker(
    ui: &mut Ui,
    id: &str,
    label: &str,
    choices: &[(String, PathBuf)],
    target: &mut PathBuf,
) {
    let label = field_label(ui, label);
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
        })
        .response
        .labelled_by(label);
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
                folder_picker(ui, "move-target", "To folder", &choices, target);
                let (cancel, confirm) = actions(ui, theme::primary_button(ui, "Move"), !unchanged);
                finish(cancel, confirm.then(|| target.clone()))
            },
        )
    }
}

fn current_parent(item: &ItemKey) -> PathBuf {
    item.path.parent().map(PathBuf::from).unwrap_or_default()
}

// ---------------------------------------------------------------- project and folder

/// A writable project and a folder in it, picked with two drop-downs.
#[derive(Debug, Clone)]
struct Destination {
    root: Option<PathBuf>,
    folder: PathBuf,
}

impl Destination {
    /// The first writable project, preferring one that isn't `avoid`.
    fn new(state: &AppState, avoid: Option<&Path>) -> Self {
        let writable = || state.collections.iter().filter(|c| !c.read_only);
        let root = writable()
            .find(|c| Some(c.root.as_path()) != avoid)
            .or_else(|| writable().next())
            .map(|c| c.root.clone());
        Self {
            root,
            folder: PathBuf::new(),
        }
    }

    fn show(&mut self, ui: &mut Ui, id: &str, state: &AppState) {
        let label = field_label(ui, "Project");
        let selected = self
            .root
            .as_ref()
            .and_then(|r| state.collection(r))
            .map(|c| c.name().to_owned())
            .unwrap_or_default();
        let before = self.root.clone();
        ComboBox::from_id_salt((id, "project"))
            .selected_text(selected)
            .width(ui.available_width())
            .show_ui(ui, |ui| {
                for c in &state.collections {
                    ui.add_enabled_ui(!c.read_only, |ui| {
                        ui.selectable_value(&mut self.root, Some(c.root.clone()), c.name());
                    });
                }
            })
            .response
            .labelled_by(label);
        if self.root != before {
            self.folder = PathBuf::new();
        }
        let folders = self
            .root
            .as_ref()
            .map(|r| destinations(state, r))
            .unwrap_or_default();
        folder_picker(
            ui,
            &format!("{id}-folder"),
            "Folder",
            &folders,
            &mut self.folder,
        );
    }

    fn key(&self) -> Option<ItemKey> {
        self.root
            .as_ref()
            .map(|r| ItemKey::new(r, self.folder.clone()))
    }
}

// ---------------------------------------------------------------- save to project

/// Save the scratch request into a project (R-COL-7 → R-COL-1).
#[derive(Debug, Clone)]
pub struct SaveToCollectionDialog {
    to: Destination,
    name: String,
}

impl SaveToCollectionDialog {
    pub fn new(state: &AppState, initial_name: &str) -> Self {
        Self {
            to: Destination::new(state, None),
            name: if initial_name == "Untitled request" {
                String::new()
            } else {
                initial_name.to_owned()
            },
        }
    }

    /// Done with (folder to save in, request name).
    pub fn show(&mut self, ctx: &Context, state: &AppState) -> DialogResult<(ItemKey, String)> {
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
                    self.to.show(ui, "save", state);
                }
                let name = self.name.trim();
                let ok = self.to.root.is_some() && !name.is_empty();
                let (cancel, confirm) = actions(ui, theme::primary_button(ui, "Save"), ok);
                let done = confirm
                    .then(|| self.to.key())
                    .flatten()
                    .map(|key| (key, name.to_owned()));
                finish(cancel, done)
            },
        )
    }
}

// ---------------------------------------------------------------- copy to project

/// Copy a request or folder into a folder of another project, or of its own (R-COL-38).
#[derive(Debug, Clone)]
pub struct CopyToProjectDialog {
    pub item: ItemKey,
    to: Destination,
}

impl CopyToProjectDialog {
    pub fn new(state: &AppState, item: ItemKey) -> Self {
        let to = Destination::new(state, Some(&item.root));
        Self { item, to }
    }

    /// Done with the folder to copy into.
    pub fn show(&mut self, ctx: &Context, state: &AppState) -> DialogResult<ItemKey> {
        let item = &self.item;
        let name = state
            .collection(&item.root)
            .and_then(|c| c.find(&item.path).map(tortuga_core::Item::name))
            .unwrap_or_default();
        let to = &mut self.to;
        dialog(
            ctx,
            "copy-to-project",
            &format!("Copy “{name}” to project"),
            420.0,
            true,
            |ui| {
                if to.root.is_none() {
                    ui.label("There is no project that can be changed to copy into.");
                } else {
                    to.show(ui, "copy", state);
                }
                let into_itself =
                    to.root.as_ref() == Some(&item.root) && to.folder.starts_with(&item.path);
                let ok = to.root.is_some() && !into_itself;
                let (cancel, confirm) = actions(ui, theme::primary_button(ui, "Copy"), ok);
                finish(cancel, confirm.then(|| to.key()).flatten())
            },
        )
    }
}
