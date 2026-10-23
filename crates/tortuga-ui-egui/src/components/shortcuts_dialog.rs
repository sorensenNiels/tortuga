//! The keyboard shortcut sheet (F1, R-COL-27): every shortcut by where it works, as the
//! shell's table of shortcuts lists them.

use egui::{Context, RichText};

use super::{DialogResult, dialog, fitting_scroll_area};
use crate::theme;

/// One line of the sheet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcutRow {
    /// Where it works, the sheet's heading for it, e.g. "Tabs".
    pub group: String,
    /// The keys as this OS writes them, e.g. "Ctrl+S" or "Cmd+S".
    pub keys: String,
    pub does: String,
}

#[derive(Debug, Clone)]
pub struct ShortcutsDialog {
    rows: Vec<ShortcutRow>,
    /// The list's height last frame (see `fitting_scroll_area`).
    height: f32,
}

impl ShortcutsDialog {
    /// `rows` grouped as they come: a heading where the group changes.
    pub fn new(rows: Vec<ShortcutRow>) -> Self {
        Self { rows, height: 0.0 }
    }

    pub fn show(&mut self, ctx: &Context) -> DialogResult<()> {
        let rows = &self.rows;
        let height = &mut self.height;
        dialog(
            ctx,
            "shortcuts",
            &t!("shortcuts-title"),
            520.0,
            true,
            |ui| {
                fitting_scroll_area(ui, "shortcuts", 200.0, height, |ui| {
                    // One grid, so the keys line up across the groups.
                    egui::Grid::new("shortcuts")
                        .num_columns(2)
                        .spacing([24.0, 6.0])
                        .show(ui, |ui| {
                            let mut group = "";
                            for row in rows {
                                if row.group != group {
                                    // An empty row before every heading but the first.
                                    if !group.is_empty() {
                                        ui.end_row();
                                    }
                                    group = &row.group;
                                    theme::section_label(ui, group);
                                    ui.end_row();
                                }
                                ui.label(RichText::new(&row.keys).monospace());
                                ui.label(&row.does);
                                ui.end_row();
                            }
                        });
                });
                ui.add_space(12.0);
                let close = theme::button_row(ui, |ui| ui.button(t!("close")).clicked());
                if close {
                    DialogResult::Close
                } else {
                    DialogResult::Open
                }
            },
        )
    }
}
