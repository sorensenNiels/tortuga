//! The selected request's response (R-HTTP-3): its filter (R-HTTP-13), trend (R-HIST-4)
//! and changes since the last one (R-HIST-5), and what the panel asks for.

use std::sync::Arc;

use egui::Ui;
use tortuga_core::settings::Feature;

use super::{Modal, Selection, Sent, TortugaUi};
use crate::components::{
    ChangesDialog, CookiesTab, Filtered, Outcome, PanelContext, ResponseAction, SaveExample, Trend,
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
        let cookies = self.cookies_tab(key);
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
            cookies,
        };
        match response_panel(ui, outcome, &mut self.response, cx) {
            Some(ResponseAction::SaveBody) => {
                if let Some(Outcome::Response(view)) = outcome {
                    save_body(
                        self.state.host.as_ref(),
                        &mut self.response.save_status,
                        &view.response,
                    );
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
                        between: t!("changes-previous-response"),
                        comparison: c,
                    }));
                }
            }
            Some(ResponseAction::ShowCookieJar) => {
                if let Some(k) = key {
                    self.show_cookies(k.root.clone());
                }
            }
            None => {}
        }
    }

    /// The Cookies tab is there only when the request sends and stores cookies: its own
    /// switch and the project's, workspace's or app's setting (R-CK-6, R-SET-1).
    fn cookies_tab(&self, key: &Selection) -> CookiesTab {
        let root = key.as_ref().map(|k| k.root.as_path());
        if !self.current.cookies || !self.state.resolved_settings(root).cookies.value {
            CookiesTab::Hidden
        } else if root.is_some() {
            CookiesTab::Project
        } else {
            CookiesTab::Scratch
        }
    }
}

/// "Save to file…": asks where, writes the body and says how it went in `status`.
fn save_body(
    host: &dyn crate::host::Host,
    status: &mut Option<String>,
    r: &tortuga_core::Response,
) {
    let title = t!("save-response-body");
    *status = match host.save_file(&title, &r.suggested_file_name(), &r.body) {
        Some(Ok(path)) => Some(t!("saved-to", path = path.display().to_string())),
        Some(Err(e)) => Some(t!("could-not-save", error = e)),
        None => status.take(),
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_host::TestHost;

    /// R-CK-6: the Cookies tab follows the request's switch and "Send and store cookies".
    #[test]
    fn the_cookies_tab_is_there_only_when_cookies_are_sent_and_stored() {
        let mut ui = TortugaUi::new(Box::new(TestHost::default()));
        assert_eq!(ui.cookies_tab(&None), CookiesTab::Scratch);
        ui.current.cookies = false;
        assert_eq!(ui.cookies_tab(&None), CookiesTab::Hidden);
        ui.current.cookies = true;
        ui.state.settings.client.send_cookies = false;
        assert_eq!(ui.cookies_tab(&None), CookiesTab::Hidden);
    }
}
