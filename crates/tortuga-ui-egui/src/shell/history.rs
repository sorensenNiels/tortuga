//! Request history in the shell: recording what was sent, the History view in the
//! sidebar, and showing a past response (R-HIST-1..3, D20).

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use egui::{RichText, Ui};
use tortuga_core::Request;
use tortuga_core::history::{Entry, Summary, now_ms};

use super::{Modal, Selection, TortugaUi};
use crate::components::{
    ClearHistory, ConfirmDialog, HistoryAction, HistoryContext, HistoryRow, Outcome, ResponseView,
    history_list,
};
use crate::format::{clock, day_label, sentence};
use crate::state::ItemKey;
use crate::theme;

/// What the sidebar lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum SidebarView {
    #[default]
    Projects,
    History,
}

/// History as the UI knows it.
#[derive(Debug, Default)]
pub(super) struct HistoryState {
    /// Summaries per project (`None`: the scratch request), newest first; read from the
    /// host the first time they are needed.
    loaded: HashMap<Option<PathBuf>, Vec<Summary>>,
    filter: String,
    /// A past response shown instead of the latest one.
    pub(super) past: Option<Past>,
}

/// A response from history in the response panel.
#[derive(Debug)]
pub(super) struct Past {
    key: Selection,
    id: String,
    /// When and where it was sent, e.g. "Today 14:32 · Staging".
    label: String,
    request: Request,
    /// What the panel showed before, to go back to.
    latest: Option<Outcome>,
}

impl TortugaUi {
    /// The summaries of `root`, read from the host the first time.
    fn history_of(&mut self, root: Option<&Path>) -> &[Summary] {
        let key = root.map(Path::to_path_buf);
        if !self.history.loaded.contains_key(&key) {
            let list = self.state.host.history_list(root).unwrap_or_else(|e| {
                self.state
                    .notify(format!("Could not read the history: {e}"));
                Vec::new()
            });
            self.history.loaded.insert(key.clone(), list);
        }
        self.history.loaded.get(&key).map_or(&[], Vec::as_slice)
    }

    /// Keeps a finished send in the history (R-HIST-1), as many entries as the settings
    /// say; nothing when history is off (R-SET-6).
    pub(super) fn record_history(&mut self, root: Option<&Path>, entry: Entry) {
        let size = self.state.settings.history.size;
        if size == 0 {
            return;
        }
        let summary = entry.summary.clone();
        self.state.host.add_history(root, entry, size);
        if let Some(list) = self.history.loaded.get_mut(&root.map(Path::to_path_buf)) {
            list.insert(0, summary);
            list.truncate(size);
        }
    }

    /// Trims the history lists shown to the size in the settings (R-SET-6).
    pub(super) fn trim_loaded_history(&mut self) {
        let size = self.state.settings.history.size;
        if size > 0 {
            for list in self.history.loaded.values_mut() {
                list.truncate(size);
            }
        }
    }

    /// The History view: the workspace's projects and the scratch request (R-HIST-2).
    pub(super) fn history_panel(&mut self, ui: &mut Ui) {
        let mut places: Vec<(Option<PathBuf>, String)> = self
            .state
            .collections
            .iter()
            .map(|c| (Some(c.root.clone()), c.name().to_owned()))
            .collect();
        places.push((None, "Scratch request".to_owned()));
        for (root, _) in &places {
            self.history_of(root.as_deref());
        }
        let with_history: Vec<(Option<PathBuf>, String)> = places
            .iter()
            .filter(|(root, _)| self.history.loaded.get(root).is_some_and(|l| !l.is_empty()))
            .cloned()
            .collect();
        let mut rows: Vec<HistoryRow<'_>> = places
            .iter()
            .flat_map(|(root, name)| {
                self.history
                    .loaded
                    .get(root)
                    .into_iter()
                    .flatten()
                    .map(move |summary| HistoryRow {
                        root: root.as_deref(),
                        place: name,
                        summary,
                    })
            })
            .collect();
        rows.sort_by(|a, b| b.summary.id.cmp(&a.summary.id));
        let open = self
            .history
            .past
            .as_ref()
            .filter(|p| p.key == self.selection)
            .map(|p| p.id.clone());
        let cx = HistoryContext {
            now_ms: now_ms(),
            offset_minutes: self.state.host.utc_offset_minutes(),
            open: open.as_deref(),
            places: &with_history,
        };
        if !self.state.settings.history.on() {
            crate::theme::hint(
                ui,
                "History is off: new sends are not recorded (Settings → History).",
            );
        }
        let mut filter = std::mem::take(&mut self.history.filter);
        let action = history_list(ui, &rows, &mut filter, cx);
        drop(rows);
        self.history.filter = filter;
        match action {
            Some(HistoryAction::Open { root, id }) => self.open_history(root.as_deref(), &id),
            Some(HistoryAction::Clear(target)) => {
                let (title, message) = match &target {
                    ClearHistory::One(_, name) => (
                        "Clear history?".to_owned(),
                        format!("The history of {name} will be removed from this computer."),
                    ),
                    ClearHistory::All => (
                        "Clear all history?".to_owned(),
                        "All request history will be removed from this computer.".to_owned(),
                    ),
                };
                let dialog = ConfirmDialog {
                    title,
                    message,
                    confirm_label: "Clear".into(),
                };
                self.modal = Some(Modal::ClearHistory(target, dialog));
            }
            None => {}
        }
    }

    /// Shows an entry: selects its request (or the scratch request, if it is gone) with
    /// that past response (R-HIST-2).
    fn open_history(&mut self, root: Option<&Path>, id: &str) {
        let entry = match self.state.host.history_entry(root, id) {
            Ok(entry) => entry,
            Err(e) => {
                self.state
                    .notify(format!("Could not open the history entry: {e}"));
                return;
            }
        };
        let key = match (root, &entry.summary.path) {
            (Some(root), Some(path)) => {
                let key = ItemKey::new(root, path.clone());
                self.state.saved_request(&key).is_some().then_some(key)
            }
            _ => None,
        };
        self.back_to_latest();
        self.select(key.clone());
        let outcome = match (&entry.response, &entry.summary.error) {
            (Some(stored), _) => {
                Outcome::Response(Arc::new(ResponseView::new(stored.to_response())))
            }
            (None, Some(error)) => Outcome::Error(sentence(error)),
            (None, None) => Outcome::Cancelled,
        };
        let offset = self.state.host.utc_offset_minutes();
        let s = &entry.summary;
        let mut label = format!(
            "{} {}",
            day_label(s.sent_at_ms, now_ms(), offset),
            clock(s.sent_at_ms, offset)
        );
        if let Some(env) = &s.environment {
            let _ = write!(label, " · {env}");
        }
        if entry
            .response
            .as_ref()
            .is_some_and(tortuga_core::history::StoredResponse::truncated)
        {
            label.push_str(" · body cut to 1 MB");
        }
        let latest = self.outcomes.insert(key.clone(), outcome);
        self.response.reset();
        self.history.past = Some(Past {
            key,
            id: id.to_owned(),
            label,
            request: entry.request,
            latest,
        });
    }

    /// Puts the latest response back in place of a past one.
    pub(super) fn back_to_latest(&mut self) {
        if let Some(past) = self.history.past.take() {
            match past.latest {
                Some(latest) => drop(self.outcomes.insert(past.key, latest)),
                None => drop(self.outcomes.remove(&past.key)),
            }
        }
    }

    /// A send replaces a past response of the same request.
    pub(super) fn forget_past(&mut self, key: &Selection) {
        if self.history.past.as_ref().is_some_and(|p| &p.key == key) {
            self.history.past = None;
        }
    }

    /// Above a past response: when it was sent, "Restore request" and "Back to latest".
    pub(super) fn past_banner(&mut self, ui: &mut Ui, key: &Selection) {
        let Some(past) = self.history.past.as_ref().filter(|p| &p.key == key) else {
            return;
        };
        let p = theme::palette(ui);
        let label = format!("From history: sent {}", past.label);
        let (restore, back) = egui::Sides::new()
            .shrink_left()
            .show(
                ui,
                |ui| {
                    ui.label(RichText::new("⏺").size(9.0).color(p.redirect));
                    ui.label(RichText::new(label).color(p.redirect));
                },
                |ui| {
                    let back = ui
                        .button("Back to latest")
                        .on_hover_text("Show the latest response of this request")
                        .clicked();
                    let restore = ui
                        .button("Restore request")
                        .on_hover_text(
                            "Put this version of the request in the editor, as unsaved changes",
                        )
                        .clicked();
                    (restore, back)
                },
            )
            .1;
        ui.add_space(6.0);
        if restore {
            let request = past.request.clone();
            let response = std::mem::take(&mut self.response);
            self.load(request);
            self.response = response;
        }
        if back {
            self.back_to_latest();
        }
    }

    /// Removes history after the user confirmed it (R-HIST-3).
    pub(super) fn clear_history(&mut self, target: &ClearHistory) {
        let result = match target {
            ClearHistory::One(root, _) => {
                self.history.loaded.insert(root.clone(), Vec::new());
                self.state.host.clear_history(root.as_deref())
            }
            ClearHistory::All => {
                self.history.loaded.clear();
                self.state.host.clear_all_history()
            }
        };
        if let Err(e) = result {
            self.state
                .notify(format!("Could not clear the history: {e}"));
        }
    }
}
