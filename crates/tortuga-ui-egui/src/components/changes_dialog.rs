//! "What changed" between two responses (R-HIST-5): the status, time and size, the
//! headers, and the body's changes by path.

use std::sync::Arc;

use egui::{Context, RichText, ScrollArea, Ui};
use tortuga_core::changes::{BodyChanges, Change, Comparison};

use super::{DialogResult, resizable_dialog};
use crate::{format, theme};

/// Longest value shown for one change; the full ones are in the responses.
const VALUE_BYTES: usize = 200;

#[derive(Debug, Clone)]
pub struct ChangesDialog {
    /// Which two responses, e.g. "Previous send → this one".
    pub between: String,
    pub comparison: Arc<Comparison>,
}

impl ChangesDialog {
    pub fn show(&self, ctx: &Context) -> DialogResult<()> {
        resizable_dialog(
            ctx,
            "changes",
            &t!("changes-title"),
            egui::vec2(720.0, 520.0),
            egui::vec2(480.0, 300.0),
            |ui| {
                theme::hint(ui, &self.between);
                ui.add_space(6.0);
                let close = ui.available_height() - 40.0;
                ScrollArea::vertical()
                    .max_height(close.max(100.0))
                    .auto_shrink([false, false])
                    .show(ui, |ui| comparison(ui, &self.comparison));
                ui.add_space(8.0);
                if theme::button_row(ui, |ui| ui.button(t!("close")).clicked()) {
                    DialogResult::Close
                } else {
                    DialogResult::Open
                }
            },
        )
    }
}

fn comparison(ui: &mut Ui, c: &Comparison) {
    let p = theme::palette(ui);
    if c.count() == 0 {
        ui.label(t!("changes-nothing"));
    }
    if let Some((before, after)) = c.status {
        ui.label(
            RichText::new(t!("changes-status", before = before, after = after)).color(p.redirect),
        );
    }
    let ms = |ms: u64| format::elapsed(std::time::Duration::from_millis(ms));
    ui.label(
        RichText::new(t!(
            "changes-time-size",
            time_before = ms(c.elapsed_ms.0),
            time_after = ms(c.elapsed_ms.1),
            size_before = format::size(c.size.0),
            size_after = format::size(c.size.1)
        ))
        .color(p.muted),
    );
    if !c.headers.is_empty() {
        ui.add_space(8.0);
        theme::section_label(ui, &t!("changes-headers", count = c.headers.len()));
        changes(ui, &c.headers);
    }
    ui.add_space(8.0);
    match &c.body {
        BodyChanges::Same => theme::section_label(ui, &t!("changes-body-same")),
        BodyChanges::Json {
            changes: list,
            more,
        } => {
            theme::section_label(ui, &t!("changes-body", count = list.len() + more));
            changes(ui, list);
            if *more > 0 {
                theme::hint(ui, t!("changes-more", count = *more));
            }
        }
        BodyChanges::Lines { differ, first } => {
            theme::section_label(ui, &t!("body"));
            ui.label(t!("changes-lines-differ", count = *differ, first = *first));
        }
    }
}

/// One line per change: + added, − removed, ~ changed.
fn changes(ui: &mut Ui, list: &[Change]) {
    let p = theme::palette(ui);
    let short = |v: &str| {
        let (shown, cut) = format::truncate(v, VALUE_BYTES);
        if cut {
            format!("{shown}…")
        } else {
            shown.to_owned()
        }
    };
    egui::Grid::new(ui.next_auto_id())
        .num_columns(2)
        .spacing([16.0, 4.0])
        .show(ui, |ui| {
            for change in list {
                let (mark, color, text) = match (&change.before, &change.after) {
                    (None, Some(after)) => ("+", p.ok, short(after)),
                    (Some(before), None) => ("−", p.fail, short(before)),
                    (Some(before), Some(after)) => (
                        "~",
                        p.redirect,
                        format!("{} → {}", short(before), short(after)),
                    ),
                    (None, None) => ("~", p.muted, String::new()),
                };
                ui.label(
                    RichText::new(format!("{mark} {}", change.at))
                        .monospace()
                        .color(color),
                );
                ui.add(egui::Label::new(RichText::new(text).monospace()).wrap());
                ui.end_row();
            }
        });
}

#[cfg(test)]
mod tests {
    use egui_kittest::kittest::Queryable;
    use tortuga_core::changes::compare;

    use super::*;

    fn response(status: u16, body: &str) -> tortuga_core::Response {
        tortuga_core::Response {
            status,
            status_text: String::new(),
            headers: vec![tortuga_core::KeyValue::new(
                "Content-Type",
                "application/json",
            )],
            body: body.as_bytes().to_vec(),
            elapsed: std::time::Duration::from_millis(5),
        }
    }

    /// R-HIST-5: the dialog lists the status and each body change by path.
    #[test]
    fn the_dialog_lists_the_changes() {
        let c = compare(
            &response(200, r#"{"total": 9.5, "gone": 1}"#),
            &response(500, r#"{"total": 12, "new": "ü😀"}"#),
        );
        let dialog = ChangesDialog {
            between: "Previous send → this one".into(),
            comparison: Arc::new(c),
        };
        let mut h = crate::test_host::harness(|ui| {
            dialog.show(ui.ctx());
        });
        h.run();
        h.get_by_label("Status 200 → 500");
        h.get_by_label("~ $.total");
        h.get_by_label("9.5 → 12");
        h.get_by_label("− $.gone");
        h.get_by_label("+ $.new");
        h.get_by_label("\"ü😀\"");
    }
}
