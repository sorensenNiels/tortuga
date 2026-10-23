//! "Send to every environment" (R-SRV-10): one row per environment with its status,
//! time and size, and how its response's shape differs from the first one's.

use std::sync::Arc;

use egui::{Context, RichText, ScrollArea, Ui};
use tortuga_core::changes::Change;
use tortuga_core::{EnvironmentColor, Response};

use super::{DialogResult, resizable_dialog};
use crate::{format, theme};

/// One environment the request went to.
#[derive(Debug, Clone)]
pub struct FanoutRow {
    pub environment: String,
    pub color: Option<EnvironmentColor>,
    pub result: FanoutResult,
    /// How the shape differs from the first environment's response (empty for it).
    pub shape: Vec<Change>,
}

#[derive(Debug, Clone)]
pub enum FanoutResult {
    Sending,
    Done(Arc<Response>),
    Failed(String),
}

/// Shows the rows for `request` (e.g. "GET Get order"); `Close` when closed.
pub fn fanout_dialog(ctx: &Context, request: &str, rows: &[FanoutRow]) -> DialogResult<()> {
    resizable_dialog(
        ctx,
        "fanout",
        &t!("fanout-title"),
        egui::vec2(760.0, 460.0),
        egui::vec2(480.0, 260.0),
        |ui| {
            theme::hint(
                ui,
                format!(
                    "{request}, sent to each environment without its scripts, captures or \
                     history. Shapes are compared with the first response: keys and types, \
                     not values."
                ),
            );
            ui.add_space(8.0);
            let height = (ui.available_height() - 40.0).max(100.0);
            ScrollArea::vertical()
                .max_height(height)
                .auto_shrink([false, false])
                .show(ui, |ui| table(ui, rows));
            ui.add_space(8.0);
            if theme::button_row(ui, |ui| ui.button(t!("close")).clicked()) {
                DialogResult::Close
            } else {
                DialogResult::Open
            }
        },
    )
}

fn table(ui: &mut Ui, rows: &[FanoutRow]) {
    let p = theme::palette(ui);
    let baseline = rows
        .iter()
        .find(|r| matches!(r.result, FanoutResult::Done(_)))
        .map(|r| r.environment.as_str());
    egui::Grid::new("fanout")
        .num_columns(4)
        .spacing([18.0, 10.0])
        .striped(true)
        .show(ui, |ui| {
            for row in rows {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("⏺").color(theme::environment_color(ui, row.color)));
                    ui.label(theme::semibold(&row.environment));
                });
                match &row.result {
                    FanoutResult::Sending => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(RichText::new(t!("sending")).color(p.muted));
                        });
                        ui.label("");
                        ui.label("");
                    }
                    FanoutResult::Failed(why) => {
                        ui.label(RichText::new(t!("failed")).color(p.fail));
                        ui.add(egui::Label::new(RichText::new(why).color(p.fail)).wrap());
                        ui.label("");
                    }
                    FanoutResult::Done(response) => {
                        let color = match response.status {
                            200..=299 => p.ok,
                            300..=399 => p.redirect,
                            _ => p.fail,
                        };
                        theme::badge(
                            ui,
                            format!("{} {}", response.status, response.status_text),
                            color,
                        );
                        ui.label(
                            RichText::new(format!(
                                "{} · {}",
                                format::elapsed(response.elapsed),
                                format::size(response.body.len())
                            ))
                            .color(p.muted),
                        );
                        shape(ui, row, baseline);
                    }
                }
                ui.end_row();
            }
        });
}

/// How this row's shape differs from the baseline environment's.
fn shape(ui: &mut Ui, row: &FanoutRow, baseline: Option<&str>) {
    let p = theme::palette(ui);
    if baseline == Some(row.environment.as_str()) {
        ui.label(RichText::new(t!("fanout-baseline")).color(p.muted));
        return;
    }
    if row.shape.is_empty() {
        ui.label(RichText::new(t!("fanout-same-shape")).color(p.muted));
        return;
    }
    ui.vertical(|ui| {
        for c in &row.shape {
            let (text, color) = match (&c.before, &c.after) {
                (None, Some(t)) => (format!("+ {} ({t})", c.at), p.ok),
                (Some(t), None) => (format!("− {} ({t})", c.at), p.fail),
                (Some(a), Some(b)) => (format!("~ {} {a} → {b}", c.at), p.redirect),
                (None, None) => (c.at.clone(), p.muted),
            };
            ui.label(RichText::new(text).monospace().color(color));
        }
    });
}
