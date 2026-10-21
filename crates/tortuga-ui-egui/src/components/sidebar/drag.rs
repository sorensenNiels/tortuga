//! Moving requests and folders by dragging them in the tree (R-COL-12).

use std::path::{Path, PathBuf};

use egui::Ui;

use super::{SidebarAction, Tree};
use crate::state::ItemKey;
use crate::theme;

/// What is being dragged in the tree (R-COL-12).
#[derive(Debug, Clone)]
pub(super) struct TreeDrag {
    key: ItemKey,
    name: String,
}

/// Makes a row draggable, and a place to drop rows of the same project: on the upper
/// part before it, on the lower part after it, and on most of a folder into it.
pub(super) fn drag_and_drop(
    ui: &Ui,
    row: &egui::Response,
    key: &ItemKey,
    name: &str,
    is_folder: bool,
    next: Option<&Path>,
    tree: &mut Tree<'_>,
) {
    row.dnd_set_drag_payload(TreeDrag {
        key: key.clone(),
        name: name.to_owned(),
    });
    let Some(drag) = row.dnd_hover_payload::<TreeDrag>() else {
        return;
    };
    let Some(pointer) = ui.ctx().pointer_latest_pos() else {
        return;
    };
    if drag.key.root != key.root || drag.key == *key {
        return;
    }
    let rect = row.rect;
    let parent = key.path.parent().map(Path::to_path_buf).unwrap_or_default();
    let top = pointer.y < rect.top() + rect.height() * if is_folder { 0.3 } else { 0.5 };
    let (into, before, mark) = if top {
        (parent, Some(key.path.clone()), Mark::Line(rect.top()))
    } else if is_folder {
        (key.path.clone(), None, Mark::Box)
    } else {
        (
            parent,
            next.map(Path::to_path_buf),
            Mark::Line(rect.bottom()),
        )
    };
    if into.starts_with(&drag.key.path) {
        return; // a folder into itself
    }
    mark.paint(ui, rect);
    if row.dnd_release_payload::<TreeDrag>().is_some() {
        tree.actions.push(SidebarAction::Place {
            key: drag.key.clone(),
            parent: into,
            before,
        });
    }
}

/// Dropping a row on its project's row puts it at the end of the top level.
pub(super) fn drop_on_project(ui: &Ui, row: &egui::Response, root: &Path, tree: &mut Tree<'_>) {
    let Some(drag) = row.dnd_hover_payload::<TreeDrag>() else {
        return;
    };
    if drag.key.root != root {
        return;
    }
    Mark::Box.paint(ui, row.rect);
    if row.dnd_release_payload::<TreeDrag>().is_some() {
        tree.actions.push(SidebarAction::Place {
            key: drag.key.clone(),
            parent: PathBuf::new(),
            before: None,
        });
    }
}

/// Where a dragged row will land.
#[derive(Debug, Clone, Copy)]
pub(super) enum Mark {
    /// Between rows, at this height.
    Line(f32),
    /// Into the folder or project of this row.
    Box,
}

impl Mark {
    fn paint(self, ui: &Ui, rect: egui::Rect) {
        let stroke = egui::Stroke::new(2.0, theme::palette(ui).accent);
        match self {
            Self::Line(y) => {
                ui.painter().hline(rect.x_range(), y, stroke);
            }
            Self::Box => {
                ui.painter()
                    .rect_stroke(rect, 4.0, stroke, egui::StrokeKind::Inside);
            }
        }
    }
}

/// The name of what is being dragged, next to the pointer.
pub(super) fn drag_label(ui: &Ui) {
    let Some(drag) = egui::DragAndDrop::payload::<TreeDrag>(ui.ctx()) else {
        return;
    };
    let Some(pos) = ui.ctx().pointer_latest_pos() else {
        return;
    };
    ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    egui::Area::new(egui::Id::unique("tree-drag"))
        .order(egui::Order::Tooltip)
        .fixed_pos(pos + egui::vec2(14.0, 6.0))
        .interactable(false)
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| ui.label(&drag.name));
        });
}
