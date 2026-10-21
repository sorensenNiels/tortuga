//! The selected request's response (R-HTTP-3): its filter (R-HTTP-13), trend (R-HIST-4)
//! and changes since the last one (R-HIST-5), and what the panel asks for.

use std::sync::Arc;

use egui::Ui;
use tortuga_core::settings::Feature;

use super::{Modal, Selection, Sent, TortugaUi};
use crate::components::{
    ChangesDialog, Filtered, Outcome, PanelContext, ResponseAction, SaveExample, Trend,
    response_panel,
};

impl TortugaUi {
    /// The response of the selected request, with its JSON filter and "Save to file…".
    pub(super) fn response_area(&mut self, ui: &mut Ui, key: &Selection, sending: bool) {
        if !sending {
            self.past_banner(ui, key);
        }
        let can_save = self.state.host.can_save_files();
        let example = if self.state.settings.features.enabled(Feature::Examples)
            && key
                .as_ref()
                .and_then(|k| self.state.collection(&k.root))
                .is_some_and(|c| !c.read_only)
        {
            SaveExample::Offered
        } else {
            SaveExample::Hidden
        };
        let sent = self.sent.get(key);
        let view = match sent.and_then(|s| s.outcome.as_ref()) {
            Some(Outcome::Response(view)) => Some(Arc::clone(view)),
            _ => None,
        };
        let expr = sent.map(|s| s.filter.trim().to_owned()).unwrap_or_default();
        let points = self.trend_points(key);
        self.update_filter(view.as_ref(), &expr, ui.ctx());
        let filtered = match self.filter_job.as_ref().map(|j| &j.result) {
            None => Filtered::Off,
            Some(None) => Filtered::Pending,
            Some(Some(Ok(body))) => Filtered::Done(body),
            Some(Some(Err(message))) => Filtered::Failed(message),
        };
        let trend = Trend {
            points: &points,
            now_ms: tortuga_core::history::now_ms(),
            offset_minutes: self.state.host.utc_offset_minutes(),
        };
        let Sent {
            outcome,
            filter,
            tests,
            breakpoints: _,
            comparison,
        } = self.sent.entry(key.clone()).or_default();
        let outcome = outcome.as_ref();
        let cx = PanelContext {
            sending,
            can_save,
            filter,
            filtered,
            tests: tests.as_ref(),
            wrap: self.state.settings.appearance.wrap_lines,
            example,
            trend,
            changes: comparison.as_deref(),
        };
        match response_panel(ui, outcome, &mut self.response, cx) {
            Some(ResponseAction::SaveBody) => {
                if let Some(Outcome::Response(view)) = outcome {
                    let r = &view.response;
                    self.response.save_status =
                        match self.state.host.save_file(&r.suggested_file_name(), &r.body) {
                            Some(Ok(path)) => Some(format!("Saved to {}", path.display())),
                            Some(Err(e)) => Some(format!("Could not save: {e}")),
                            None => self.response.save_status.take(),
                        };
                }
            }
            Some(ResponseAction::SaveAsExample) => {
                if let Some(Outcome::Response(view)) = outcome {
                    let r = &view.response;
                    let base = format!("{} {}", r.status, r.status_text);
                    let examples = &mut self.current.examples;
                    let name = tortuga_core::import::unique_name(base.trim(), |n| {
                        examples.iter().any(|e| e.name == n)
                    });
                    examples.push(tortuga_core::mock::Example::from_response(name, r));
                    self.editor.open_example(examples.len() - 1);
                }
            }
            Some(ResponseAction::SetWrap(wrap)) => {
                self.state
                    .update_settings(|s| s.appearance.wrap_lines = wrap);
            }
            Some(ResponseAction::AddTest(code)) => self.add_test(&code),
            Some(ResponseAction::AddCapture { name, from }) => self.add_capture(&name, &from),
            Some(ResponseAction::ShowChanges) => {
                if let Some(c) = self.sent.get(key).and_then(|s| s.comparison.clone()) {
                    self.modal = Some(Modal::Changes(ChangesDialog {
                        between: "The previous response → this one".into(),
                        comparison: c,
                    }));
                }
            }
            None => {}
        }
    }
}
