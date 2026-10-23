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
    ChangesDialog, ClearHistory, ConfirmDialog, HistoryAction, HistoryContext, HistoryRow, Outcome,
    ResponseView, TREND_LENGTH, TrendPoint, history_list,
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
    /// The entry chosen with "Select for comparing" (R-HIST-5): (project, id).
    selected_for_comparing: Option<(Option<PathBuf>, String)>,
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
                self.state.notify(t!("history-read-failed", error = e));
                Vec::new()
            });
            self.history.loaded.insert(key.clone(), list);
        }
        self.history.loaded.get(&key).map_or(&[], Vec::as_slice)
    }

    /// The last sends of the request `key`, oldest first, for its trend (R-HIST-4).
    pub(super) fn trend_points(&mut self, key: &Selection) -> Vec<TrendPoint> {
        if !self.state.host.keeps_history() {
            return Vec::new();
        }
        let path = key.as_ref().map(|k| k.path.as_path());
        let mut points: Vec<TrendPoint> = self
            .history_of(key.as_ref().map(|k| k.root.as_path()))
            .iter()
            .filter(|s| s.path.as_deref() == path)
            .take(TREND_LENGTH)
            .map(|s| TrendPoint {
                sent_at_ms: s.sent_at_ms,
                elapsed_ms: s.elapsed_ms,
                status: s.status,
            })
            .collect();
        points.reverse();
        points
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
        places.push((None, t!("scratch-request")));
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
            selected_for_comparing: self
                .history
                .selected_for_comparing
                .as_ref()
                .map(|(_, id)| id.as_str()),
        };
        if !self.state.settings.history.on() {
            crate::theme::hint(ui, t!("history-off"));
        }
        let mut filter = std::mem::take(&mut self.history.filter);
        let action = history_list(ui, &rows, &mut filter, cx);
        drop(rows);
        self.history.filter = filter;
        match action {
            Some(HistoryAction::Open { root, id }) => self.open_history(root.as_deref(), &id),
            Some(HistoryAction::CompareWithBefore { root, id }) => {
                match self.send_before(root.as_deref(), &id) {
                    Some(before) => self.compare_history(&(root.clone(), before), &(root, id)),
                    None => self.state.inform(t!("history-first-send")),
                }
            }
            Some(HistoryAction::SelectForComparing { root, id }) => {
                self.history.selected_for_comparing = Some((root, id));
                self.state.inform(t!("history-selected-for-compare"));
            }
            Some(HistoryAction::CompareWithSelected { root, id }) => {
                if let Some(selected) = self.history.selected_for_comparing.clone() {
                    self.compare_history(&selected, &(root, id));
                }
            }
            Some(HistoryAction::Clear(target)) => {
                let (title, message) = match &target {
                    ClearHistory::One(_, name) => (
                        t!("history-clear-title"),
                        t!("history-clear-message", name = name),
                    ),
                    ClearHistory::All => (
                        t!("history-clear-all-title"),
                        t!("history-clear-all-message"),
                    ),
                };
                let dialog = ConfirmDialog {
                    title,
                    message,
                    confirm_label: t!("clear"),
                };
                self.modal = Some(Modal::ClearHistory(target, dialog));
            }
            None => {}
        }
    }

    /// The id of the send of the same request before entry `id` that got a response
    /// (R-HIST-5).
    fn send_before(&mut self, root: Option<&Path>, id: &str) -> Option<String> {
        let list = self.history_of(root);
        let at = list.iter().position(|s| s.id == id)?;
        let path = list.get(at)?.path.clone();
        list.get(at + 1..)?
            .iter()
            .find(|s| s.path == path && s.status.is_some())
            .map(|s| s.id.clone())
    }

    /// Shows what changed between two history entries, the older one first (R-HIST-5).
    fn compare_history(&mut self, a: &(Option<PathBuf>, String), b: &(Option<PathBuf>, String)) {
        let load = |ui: &Self, (root, id): &(Option<PathBuf>, String)| {
            ui.state.host.history_entry(root.as_deref(), id)
        };
        let (a, b) = match (load(self, a), load(self, b)) {
            (Ok(a), Ok(b)) => (a, b),
            (Err(e), _) | (_, Err(e)) => {
                return self.state.notify(t!("history-open-failed", error = e));
            }
        };
        let (before, after) = if a.summary.sent_at_ms <= b.summary.sent_at_ms {
            (a, b)
        } else {
            (b, a)
        };
        let (Some(old), Some(new)) = (&before.response, &after.response) else {
            return self.state.inform(t!("history-compare-needs-response"));
        };
        let comparison = tortuga_core::changes::compare(&old.to_response(), &new.to_response());
        let offset = self.state.host.utc_offset_minutes();
        let now = now_ms();
        let label = |s: &Summary| {
            let mut text = format!(
                "{} {} · {}",
                day_label(s.sent_at_ms, now, offset),
                clock(s.sent_at_ms, offset),
                s.name
            );
            if let Some(env) = &s.environment {
                let _ = write!(text, " · {env}");
            }
            text
        };
        self.modal = Some(Modal::Changes(ChangesDialog {
            between: format!("{} → {}", label(&before.summary), label(&after.summary)),
            comparison: Arc::new(comparison),
        }));
    }

    /// Shows an entry: selects its request (or the scratch request, if it is gone) with
    /// that past response (R-HIST-2).
    fn open_history(&mut self, root: Option<&Path>, id: &str) {
        let entry = match self.state.host.history_entry(root, id) {
            Ok(entry) => entry,
            Err(e) => {
                self.state.notify(t!("history-open-failed", error = e));
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
            label.push_str(&t!("history-body-cut"));
        }
        let latest = self
            .sent
            .entry(key.clone())
            .or_default()
            .outcome
            .replace(outcome);
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
            self.sent.entry(past.key).or_default().outcome = past.latest;
        }
    }

    /// A send replaces a past response of the same request.
    pub(super) fn forget_past(&mut self, key: &Selection) {
        self.forget_past_if(|k| k == key);
    }

    /// Drops the past response shown for a request that `gone` says is gone.
    pub(super) fn forget_past_if(&mut self, gone: impl Fn(&Selection) -> bool) {
        if self.history.past.as_ref().is_some_and(|p| gone(&p.key)) {
            self.history.past = None;
        }
    }

    /// Moves the past response along with its request (R-COL-2).
    pub(super) fn remap_past(&mut self, moved: impl Fn(&Selection) -> Option<Selection>) {
        if let Some(past) = &mut self.history.past
            && let Some(to) = moved(&past.key)
        {
            past.key = to;
        }
    }

    /// Above a past response: when it was sent, "Restore request" and "Back to latest".
    pub(super) fn past_banner(&mut self, ui: &mut Ui, key: &Selection) {
        let Some(past) = self.history.past.as_ref().filter(|p| &p.key == key) else {
            return;
        };
        let p = theme::palette(ui);
        let label = t!("history-past-label", sent = past.label.as_str());
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
                        .button(t!("history-back-to-latest"))
                        .on_hover_text(t!("history-back-to-latest-hint"))
                        .clicked();
                    let restore = ui
                        .button(t!("history-restore"))
                        .on_hover_text(t!("history-restore-hint"))
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
        match result {
            Ok(()) => self.state.inform(t!("history-cleared")),
            Err(e) => self.state.notify(t!("history-clear-failed", error = e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// R-HIST-4: the request's last sends show as a trend next to its response, oldest
    /// first; other requests' sends don't count.
    #[test]
    fn a_request_shows_its_trend() {
        use egui_kittest::kittest::Queryable;
        use tortuga_core::history::Summary;

        let (mut ui, root, key) = crate::shell::tests::setup("trend");
        let summary = |i: u64, path: &Path, status: u16| Summary {
            id: format!("{i:04}"),
            sent_at_ms: 1_000 * i,
            path: Some(path.to_path_buf()),
            name: "Get user".into(),
            method: tortuga_core::HttpMethod::Get,
            url: "https://a.test/".into(),
            environment: None,
            status: Some(status),
            elapsed_ms: Some(10 * i),
            error: None,
        };
        let other = Path::new("other.toml");
        ui.history.loaded.insert(
            Some(root.clone()),
            vec![
                summary(4, &key.path, 500),
                summary(3, other, 200),
                summary(2, &key.path, 200),
                summary(1, &key.path, 200),
            ],
        );
        let points = ui.trend_points(&Some(key.clone()));
        assert_eq!(
            points.iter().map(|p| p.sent_at_ms).collect::<Vec<_>>(),
            [1_000, 2_000, 4_000],
            "oldest first, this request only"
        );
        let response = tortuga_core::Response {
            status: 200,
            status_text: "OK".into(),
            headers: Vec::new(),
            body: Vec::new(),
            elapsed: std::time::Duration::from_millis(3),
        };
        ui.sent.entry(Some(key)).or_default().outcome =
            Some(Outcome::Response(Arc::new(ResponseView::new(response))));
        {
            let mut h = egui_kittest::Harness::builder()
                .with_size(egui::vec2(1200.0, 900.0))
                .build_ui(|egui_ui| ui.ui(egui_ui));
            h.run();
            h.get_by_label("Last 3 sends: 10 ms to 40 ms, 1 failed");
        }
        std::fs::remove_dir_all(root).expect("clean up");
    }

    /// R-HIST-5: "Compare with the send before" finds the same request's previous send
    /// that got a response, and shows what changed, the older one first.
    #[test]
    fn a_send_is_compared_with_the_one_before() {
        let (mut ui, root, key) = crate::shell::tests::setup("compare-history");
        let request = ui.current.clone();
        let response = |total: &str| tortuga_core::Response {
            status: 200,
            status_text: "OK".into(),
            headers: vec![tortuga_core::KeyValue::new(
                "Content-Type",
                "application/json",
            )],
            body: format!(r#"{{"total": {total}}}"#).into_bytes(),
            elapsed: std::time::Duration::from_millis(5),
        };
        let entry = |seq: u64, path: &Path, result: Result<&tortuga_core::Response, String>| {
            Entry::new(
                tortuga_core::history::new_id(1_000 * seq, seq),
                1_000 * seq,
                Some(path.to_path_buf()),
                &request,
                None,
                result,
            )
        };
        let (old, new) = (response("9.5"), response("12"));
        ui.record_history(Some(&root), entry(1, &key.path, Ok(&old)));
        ui.record_history(Some(&root), entry(2, &key.path, Err("refused".into())));
        ui.record_history(Some(&root), entry(3, Path::new("other.toml"), Ok(&new)));
        ui.record_history(Some(&root), entry(4, &key.path, Ok(&new)));
        let latest = tortuga_core::history::new_id(4_000, 4);
        let before = ui.send_before(Some(&root), &latest).expect("one before");
        assert_eq!(
            before,
            tortuga_core::history::new_id(1_000, 1),
            "skips the error"
        );
        assert_eq!(ui.send_before(Some(&root), &before), None, "the first");

        ui.compare_history(&(Some(root.clone()), latest), &(Some(root.clone()), before));
        let Some(Modal::Changes(d)) = &ui.modal else {
            panic!("the comparison")
        };
        let tortuga_core::changes::BodyChanges::Json { changes, .. } = &d.comparison.body else {
            panic!("{:?}", d.comparison.body)
        };
        assert_eq!(
            (changes[0].before.as_deref(), changes[0].after.as_deref()),
            (Some("9.5"), Some("12")),
            "older first, whatever the order chosen"
        );
        std::fs::remove_dir_all(root).expect("clean up");
    }
}
