//! The History view in the sidebar (R-HIST-2, R-HIST-3, D20).

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use egui::{Atom, AtomExt as _, Button, RichText, ScrollArea, TextEdit, Ui};
use tortuga_core::history::Summary;

use crate::{format, fuzzy, theme};

/// Most rows drawn; the filter finds older ones.
const MAX_ROWS: usize = 300;

/// One entry in the list.
#[derive(Debug, Clone, Copy)]
pub struct HistoryRow<'a> {
    /// The project (`None`: the scratch request).
    pub root: Option<&'a Path>,
    /// The project's name, or "Scratch request".
    pub place: &'a str,
    pub summary: &'a Summary,
}

/// Whose history to clear (R-HIST-3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClearHistory {
    One(Option<PathBuf>, String),
    All,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryAction {
    Open { root: Option<PathBuf>, id: String },
    Clear(ClearHistory),
}

/// What the list needs besides the rows.
#[derive(Debug, Clone, Copy)]
pub struct HistoryContext<'a> {
    pub now_ms: u64,
    pub offset_minutes: i32,
    /// The entry shown in the response panel, if any.
    pub open: Option<&'a str>,
    /// Places that have history: (project root, name).
    pub places: &'a [(Option<PathBuf>, String)],
}

/// The workspace's history, newest first, grouped by day, with a filter field.
/// `rows` must be sorted newest first.
pub fn history_list(
    ui: &mut Ui,
    rows: &[HistoryRow<'_>],
    filter: &mut String,
    cx: HistoryContext<'_>,
) -> Option<HistoryAction> {
    let mut action = None;
    egui::Sides::new().shrink_left().show(
        ui,
        |ui| {
            ui.add(
                TextEdit::singleline(filter)
                    .id_salt("history-filter")
                    .hint_text("Filter history")
                    .desired_width(ui.available_width()),
            )
            .on_hover_text("Search by name, URL, status, project or environment");
        },
        |ui| {
            if !cx.places.is_empty() {
                ui.menu_button("Clear", |ui| {
                    for (root, name) in cx.places {
                        if ui.button(format!("Clear {name}")).clicked() {
                            action = Some(HistoryAction::Clear(ClearHistory::One(
                                root.clone(),
                                name.clone(),
                            )));
                        }
                    }
                    ui.separator();
                    if ui.button("Clear all history").clicked() {
                        action = Some(HistoryAction::Clear(ClearHistory::All));
                    }
                })
                .response
                .on_hover_text("Remove history kept on this computer");
            }
        },
    );
    ui.add_space(8.0);

    let query = filter.trim();
    let shown: Vec<&HistoryRow<'_>> = rows
        .iter()
        .filter(|r| query.is_empty() || fuzzy::score(query, &search_text(r)).is_some())
        .take(MAX_ROWS)
        .collect();
    if shown.is_empty() {
        theme::hint(
            ui,
            if rows.is_empty() {
                "Requests you send appear here. History stays on this computer."
            } else {
                "Nothing in the history matches."
            },
        );
        return action;
    }

    ScrollArea::vertical()
        .id_salt("history")
        .auto_shrink(false)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            let mut day = String::new();
            for row in &shown {
                let label = format::day_label(row.summary.sent_at_ms, cx.now_ms, cx.offset_minutes);
                if label != day {
                    if !day.is_empty() {
                        ui.add_space(8.0);
                    }
                    theme::section_label(ui, &label);
                    day = label;
                }
                let selected = cx.open == Some(row.summary.id.as_str());
                if history_row(ui, row, selected, cx).clicked() {
                    action = Some(HistoryAction::Open {
                        root: row.root.map(Path::to_path_buf),
                        id: row.summary.id.clone(),
                    });
                }
            }
            if shown.len() == MAX_ROWS {
                theme::hint(ui, "Older entries: use the filter to find them.");
            }
        });
    action
}

fn search_text(row: &HistoryRow<'_>) -> String {
    let s = row.summary;
    format!(
        "{} {} {} {} {} {}",
        s.name,
        s.url,
        s.method.as_str(),
        s.status.map(|c| c.to_string()).unwrap_or_default(),
        row.place,
        s.environment.as_deref().unwrap_or_default()
    )
}

fn history_row(
    ui: &mut Ui,
    row: &HistoryRow<'_>,
    selected: bool,
    cx: HistoryContext<'_>,
) -> egui::Response {
    let p = theme::palette(ui);
    let s = row.summary;
    let method = s.method;
    let title = if s.name.is_empty() || row.root.is_none() {
        s.url.clone()
    } else {
        s.name.clone()
    };
    let (status, color) = match s.status {
        Some(code @ 200..=299) => (code.to_string(), p.ok),
        Some(code @ 300..=399) => (code.to_string(), p.redirect),
        Some(code) => (code.to_string(), p.fail),
        None => ("Error".to_owned(), p.fail),
    };
    let atoms = [
        Atom::from(
            RichText::new(format!("{:<5}", theme::method_short(method)))
                .monospace()
                .size(10.5)
                .color(theme::method_color(ui, method)),
        ),
        // The title gives way when the row is narrow, not the method.
        Atom::from(title).atom_shrink(true),
        Atom::grow(),
        Atom::from(RichText::new(status).small().color(color)),
        Atom::from(
            RichText::new(format::clock(s.sent_at_ms, cx.offset_minutes))
                .small()
                .color(p.muted),
        ),
    ];
    let atoms: egui::Atoms<'_> = atoms.into_iter().collect();
    let mut hover = format!("{} {}\n{}", method.as_str(), s.url, row.place);
    if let Some(env) = &s.environment {
        let _ = write!(hover, " · {env}");
    }
    if let Some(ms) = s.elapsed_ms {
        let _ = write!(hover, " · {ms} ms");
    }
    if let Some(e) = &s.error {
        hover.push('\n');
        hover.push_str(e);
    }
    ui.add(
        Button::selectable(selected, atoms)
            .frame_when_inactive(selected)
            .truncate()
            .min_size(egui::vec2(ui.available_width(), 26.0)),
    )
    .on_hover_text(hover)
}

#[cfg(test)]
mod tests {
    use egui_kittest::kittest::Queryable;
    use tortuga_core::HttpMethod;

    use super::*;

    fn summary(id: &str, name: &str, at: u64, status: Option<u16>) -> Summary {
        Summary {
            id: id.into(),
            sent_at_ms: at,
            path: Some("x.toml".into()),
            name: name.into(),
            method: HttpMethod::Get,
            url: format!("https://api.test/{name}"),
            environment: None,
            status,
            elapsed_ms: Some(5),
            error: None,
        }
    }

    /// R-HIST-2: grouped by day, filtered, and a click opens the entry.
    #[test]
    fn groups_filters_and_opens() {
        let day = 86_400_000;
        let now = 1_792_240_200_000;
        let summaries = [
            summary("3", "users", now, Some(200)),
            summary("2", "orders", now - day, Some(500)),
            summary("1", "health", now - 2 * day, None),
        ];
        let action = std::rc::Rc::new(std::cell::RefCell::new(None));
        let shared = action.clone();
        let filter = std::rc::Rc::new(std::cell::RefCell::new(String::new()));
        let shared_filter = filter.clone();
        let mut h = crate::test_host::harness(move |ui| {
            let rows: Vec<HistoryRow<'_>> = summaries
                .iter()
                .map(|s| HistoryRow {
                    root: Some(Path::new("/p")),
                    place: "Users API",
                    summary: s,
                })
                .collect();
            let cx = HistoryContext {
                now_ms: now,
                offset_minutes: 0,
                open: None,
                places: &[],
            };
            if let Some(a) = history_list(ui, &rows, &mut shared_filter.borrow_mut(), cx) {
                *shared.borrow_mut() = Some(a);
            }
        });
        h.run();
        h.get_by_label("TODAY");
        h.get_by_label("YESTERDAY");
        h.get_by_label_contains("orders").click();
        h.run();
        assert_eq!(
            *action.borrow(),
            Some(HistoryAction::Open {
                root: Some("/p".into()),
                id: "2".into()
            })
        );
        *filter.borrow_mut() = "500".into();
        h.run();
        assert!(h.query_by_label_contains("users").is_none(), "filtered out");
        h.get_by_label_contains("orders");
    }
}
