//! A project's mock server (R-MOCK-2, R-MOCK-4): whether it runs and on which port,
//! start and stop, and the log of what it received and what answered.

use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use egui::{Context, Grid, RichText, ScrollArea, TextEdit, Ui};
use tortuga_core::mock::{Hit, Outcome};

use super::DialogResult;
use super::modals::{dialog, finish};
use crate::theme;

/// What the user asked of the mock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MockAction {
    Start,
    Stop,
    /// Use this port from now on; a running mock restarts on it.
    SetPort(u16),
    ClearLog,
    /// Choose the "Mock" environment, so requests go to the mocks (D45).
    UseMockEnvironment,
}

/// What the dialog shows about the project's mock.
#[derive(Debug, Clone, Copy)]
pub struct MockView<'a> {
    /// The port it runs on; `None` when stopped.
    pub running: Option<u16>,
    /// The port it starts on.
    pub port: u16,
    /// Oldest first.
    pub hits: &'a [Hit],
    /// Requests already go to the mocks.
    pub in_use: bool,
    pub offset_minutes: i32,
}

#[derive(Debug, Clone)]
pub struct MockDialog {
    pub root: PathBuf,
    title: String,
    port_text: String,
}

impl MockDialog {
    pub fn new(root: PathBuf, project: &str, port: u16) -> Self {
        Self {
            root,
            title: format!("Mock server — {project}"),
            port_text: port.to_string(),
        }
    }

    pub fn show(
        &mut self,
        ctx: &Context,
        view: MockView<'_>,
    ) -> (DialogResult<()>, Option<MockAction>) {
        let mut action = None;
        let result = dialog(ctx, "mock", &self.title, 820.0, true, |ui| {
            theme::hint(
                ui,
                "Answers the project's requests with their examples (the Examples tab of a \
                 request), on this computer only.",
            );
            ui.add_space(8.0);
            if let Some(asked) = controls(ui, &mut self.port_text, view) {
                action = Some(asked);
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                theme::section_label(ui, "Received");
                if !view.hits.is_empty() && ui.small_button("Clear").clicked() {
                    action = Some(MockAction::ClearLog);
                }
            });
            log(ui, view.hits, view.offset_minutes);
            ui.add_space(12.0);
            let close = theme::button_row(ui, |ui| ui.button("Close").clicked());
            finish::<()>(close, None)
        });
        (result, action)
    }
}

/// Running or stopped, the port, and the buttons.
fn controls(ui: &mut Ui, port_text: &mut String, view: MockView<'_>) -> Option<MockAction> {
    let p = theme::palette(ui);
    let mut action = None;
    ui.horizontal(|ui| {
        match view.running {
            Some(port) => {
                theme::badge(ui, "Running", p.ok);
                ui.label(RichText::new(format!("http://127.0.0.1:{port}")).monospace());
            }
            None => {
                theme::badge(ui, "Stopped", p.muted);
            }
        }
        ui.add_space(12.0);
        let label = ui.label("Port");
        let edit = TextEdit::singleline(port_text).id_salt("mock-port");
        theme::sized_edit(ui, 60.0, edit).labelled_by(label.id);
        let typed = parse_port(port_text);
        if typed.is_none() {
            theme::warning(ui, "A port from 1 to 65535");
        }
        if let Some(port) = typed.filter(|p| *p != view.port) {
            let label = if view.running.is_some() {
                "Restart on this port"
            } else {
                "Use this port"
            };
            if ui.button(label).clicked() {
                action = Some(MockAction::SetPort(port));
            }
        }
        theme::button_row(ui, |ui| {
            if view.running.is_some() {
                if ui.add(theme::danger_button(ui, "Stop")).clicked() {
                    action = Some(MockAction::Stop);
                }
                if !view.in_use
                    && ui
                        .button("Send requests here")
                        .on_hover_text("Choose the “Mock” environment")
                        .clicked()
                {
                    action = Some(MockAction::UseMockEnvironment);
                }
            } else if ui.add(theme::primary_button(ui, "Start")).clicked() {
                action = Some(MockAction::Start);
            }
        });
    });
    action
}

/// The requests the mock received, newest first.
fn log(ui: &mut Ui, hits: &[Hit], offset_minutes: i32) {
    if hits.is_empty() {
        theme::hint(ui, "Nothing yet.");
        return;
    }
    let p = theme::palette(ui);
    ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
        Grid::new("mock-log")
            .num_columns(5)
            .spacing([14.0, 6.0])
            .striped(true)
            .show(ui, |ui| {
                for heading in ["Time", "Status", "Request", "Answered by", ""] {
                    theme::section_label(ui, heading);
                }
                ui.end_row();
                for hit in hits.iter().rev() {
                    let ms = hit
                        .at
                        .duration_since(UNIX_EPOCH)
                        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));
                    ui.label(
                        RichText::new(crate::format::clock_seconds(ms, offset_minutes))
                            .color(p.muted),
                    );
                    let color = if hit.status < 400 { p.ok } else { p.fail };
                    ui.label(theme::semibold(hit.status.to_string()).color(color));
                    let request = format!("{} {}", hit.method, hit.target);
                    let (shown, _) = crate::format::truncate(&request, 80);
                    ui.label(RichText::new(shown).monospace())
                        .on_hover_text(&request);
                    match &hit.outcome {
                        Outcome::Example {
                            request_name,
                            example,
                            ..
                        } => ui.label(format!("{request_name} › {example}")),
                        Outcome::NoMatch(reason) => {
                            ui.label(RichText::new(reason).color(p.warning_text))
                        }
                    };
                    ui.label("");
                    ui.end_row();
                }
            });
    });
}

fn parse_port(text: &str) -> Option<u16> {
    text.trim().parse().ok().filter(|p| *p > 0)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime};

    use egui_kittest::kittest::Queryable;

    use super::*;

    fn hit(status: u16, outcome: Outcome) -> Hit {
        Hit {
            at: SystemTime::UNIX_EPOCH + Duration::from_secs(3 * 3600 + 7),
            method: "GET".into(),
            target: "/users/ü😀?x=1".into(),
            status,
            outcome,
        }
    }

    fn show(
        view: MockView<'static>,
    ) -> (
        egui_kittest::Harness<'static>,
        std::rc::Rc<std::cell::Cell<Option<MockAction>>>,
    ) {
        let asked = std::rc::Rc::new(std::cell::Cell::new(None));
        let seen = std::rc::Rc::clone(&asked);
        let mut dialog = MockDialog::new(PathBuf::from("/p"), "Shop", view.port);
        let mut h = crate::test_host::harness(move |ui| {
            if let (_, Some(a)) = dialog.show(ui.ctx(), view) {
                seen.set(Some(a));
            }
        });
        h.run();
        (h, asked)
    }

    /// R-MOCK-4: the log shows each request with what answered it, newest first.
    #[test]
    fn the_log_shows_what_answered() {
        let hits: &'static [Hit] = Box::leak(Box::new([
            hit(
                200,
                Outcome::Example {
                    request: "get-user.toml".into(),
                    request_name: "Get user".into(),
                    example: "found".into(),
                },
            ),
            hit(
                404,
                Outcome::NoMatch("No request of “Shop” is GET /x".into()),
            ),
        ]));
        let (h, _) = show(MockView {
            running: Some(4010),
            port: 4010,
            hits,
            in_use: false,
            offset_minutes: 60,
        });
        h.get_by_label("http://127.0.0.1:4010");
        h.get_by_label("Get user › found");
        h.get_by_label("No request of “Shop” is GET /x");
        assert_eq!(h.query_all_by_label("04:00:07").count(), 2, "local time");
        h.get_by_label("Send requests here");
    }

    /// R-MOCK-2: a stopped mock offers Start; a new port is offered once it is valid.
    #[test]
    fn start_and_port() {
        let (mut h, asked) = show(MockView {
            running: None,
            port: 4010,
            hits: &[],
            in_use: false,
            offset_minutes: 0,
        });
        h.get_by_label("Nothing yet.");
        h.get_by_label("Start").click();
        h.run();
        assert_eq!(asked.get(), Some(MockAction::Start));
        let field = h
            .get_all_by_role(egui::accesskit::Role::TextInput)
            .next()
            .expect("port field");
        field.focus();
        field.type_text("1");
        h.run();
        h.get_by_label("Use this port").click();
        h.run();
        assert_eq!(asked.get(), Some(MockAction::SetPort(40101)));
    }

    #[test]
    fn ports() {
        assert_eq!(parse_port(" 8080 "), Some(8080));
        assert_eq!(parse_port("0"), None);
        assert_eq!(parse_port("70000"), None);
        assert_eq!(parse_port("ü😀"), None);
    }
}
