//! The Load test add-on (R-ADD-7, R-FLOW-15, D87): a workflow run by many virtual users
//! at once, with each step's response times, in a dialog. The shell starts the test,
//! since it holds the workflow's draft and run options; this module keeps the dialog
//! and what the running test reports.

use std::sync::mpsc::Sender;
use std::time::Duration;

use egui::{DragValue, Grid, RichText, Ui};
use futures_util::future::AbortHandle;
use tortuga_core::workflow::{LoadLimit, LoadOptions, LoadReport, MAX_USERS};

use crate::components::{DialogResult, dialog};
use crate::shell::Inbox;
use crate::state::{AppState, WorkflowKey};
use crate::theme;

/// The add-on's state while it is on.
#[derive(Debug)]
pub struct LoadTests {
    dialog: Option<LoadDialog>,
    updates: Inbox<Update>,
    next_id: u64,
}

impl Default for LoadTests {
    fn default() -> Self {
        Self {
            dialog: None,
            updates: Inbox::new(),
            next_id: 0,
        }
    }
}

/// What a running test sends: its report so far, or in all.
#[derive(Debug)]
pub(crate) struct Update {
    id: u64,
    report: LoadReport,
    done: bool,
}

impl Update {
    pub(crate) fn new(id: u64, report: LoadReport, done: bool) -> Self {
        Self { id, report, done }
    }
}

/// When the test ends, as chosen in the dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Limit {
    Duration,
    Iterations,
}

#[derive(Debug)]
struct LoadDialog {
    key: WorkflowKey,
    name: String,
    users: u32,
    limit: Limit,
    seconds: u64,
    iterations: u32,
    ramp_up_s: u64,
    /// Start was clicked; the shell starts it.
    wants_start: bool,
    run: Option<Running>,
}

#[derive(Debug)]
struct Running {
    id: u64,
    abort: AbortHandle,
    report: LoadReport,
    done: bool,
    cancelled: bool,
}

impl LoadDialog {
    fn options(&self) -> LoadOptions {
        LoadOptions {
            users: self.users.clamp(1, MAX_USERS),
            limit: match self.limit {
                Limit::Duration => LoadLimit::Duration(Duration::from_secs(self.seconds.max(1))),
                Limit::Iterations => LoadLimit::Iterations(self.iterations.max(1)),
            },
            ramp_up: Duration::from_secs(self.ramp_up_s),
        }
    }

    fn running(&self) -> bool {
        self.run.as_ref().is_some_and(|r| !r.done && !r.cancelled)
    }

    fn cancel(&mut self) {
        if let Some(run) = self.run.as_mut().filter(|r| !r.done) {
            run.abort.abort();
            run.cancelled = true;
        }
    }
}

impl LoadTests {
    /// Opens the dialog for the workflow `key`, keeping its choices when it is the
    /// same workflow.
    pub(crate) fn open(&mut self, key: WorkflowKey, name: String) {
        if self.dialog.as_ref().is_some_and(|d| d.key == key) {
            return;
        }
        if let Some(old) = &mut self.dialog {
            old.cancel();
        }
        self.dialog = Some(LoadDialog {
            key,
            name,
            users: 10,
            limit: Limit::Duration,
            seconds: 30,
            iterations: 10,
            ramp_up_s: 0,
            wants_start: false,
            run: None,
        });
    }

    /// The test Start asked for: which workflow, and how.
    pub(crate) fn take_start(&mut self) -> Option<(WorkflowKey, LoadOptions)> {
        let d = self.dialog.as_mut().filter(|d| d.wants_start)?;
        d.wants_start = false;
        Some((d.key.clone(), d.options()))
    }

    /// The shell started the test: the id and sender its task reports with.
    pub(crate) fn started(&mut self, abort: AbortHandle) -> (u64, Sender<Update>) {
        self.next_id += 1;
        let id = self.next_id;
        if let Some(d) = &mut self.dialog {
            d.run = Some(Running {
                id,
                abort,
                report: LoadReport::default(),
                done: false,
                cancelled: false,
            });
        }
        (id, self.updates.sender())
    }

    /// Stops a running test, when the add-on is turned off (R-ADD-2).
    pub(crate) fn stop(&mut self) {
        if let Some(d) = &mut self.dialog {
            d.cancel();
        }
    }

    /// Takes in what running tests reported; once a frame.
    pub(crate) fn take_updates(&mut self) {
        while let Some(update) = self.updates.try_recv() {
            let run = self
                .dialog
                .as_mut()
                .and_then(|d| d.run.as_mut())
                .filter(|r| r.id == update.id && !r.cancelled);
            if let Some(run) = run {
                run.report = update.report;
                run.done |= update.done;
            }
        }
    }
}

/// Shows the dialog. Returns whether it stays open; closing it stops a running test.
pub(crate) fn dialog_shown(ctx: &egui::Context, state: &mut AppState) -> bool {
    let Some(tests) = state.addons.load_test.as_mut() else {
        return false;
    };
    let Some(mut d) = tests.dialog.take() else {
        return false;
    };
    let title = t!("load-test-title", name = d.name.as_str());
    let result = dialog(ctx, "load-test", &title, 620.0, false, |ui| {
        content(ui, &mut d)
    });
    let open = matches!(result, DialogResult::Open);
    if !open {
        d.cancel();
    }
    if open && let Some(tests) = state.addons.load_test.as_mut() {
        tests.dialog = Some(d);
    }
    if open && d_running(state) {
        // The table changes as reports come in.
        ctx.request_repaint_after(Duration::from_millis(250));
    }
    open
}

fn d_running(state: &AppState) -> bool {
    state
        .addons
        .load_test
        .as_ref()
        .and_then(|t| t.dialog.as_ref())
        .is_some_and(LoadDialog::running)
}

fn content(ui: &mut Ui, d: &mut LoadDialog) -> DialogResult<()> {
    let running = d.running();
    ui.add_enabled_ui(!running, |ui| settings(ui, d));
    ui.add_space(12.0);
    if let Some(run) = &d.run {
        report(ui, run);
        ui.add_space(12.0);
    }
    let mut result = DialogResult::Open;
    ui.horizontal(|ui| {
        if running {
            if ui.add(theme::danger_button(ui, t!("cancel"))).clicked() {
                d.cancel();
            }
        } else if ui
            .add(theme::primary_button(ui, t!("load-test-start")))
            .clicked()
        {
            d.wants_start = true;
        }
        if ui.button(t!("close")).clicked() {
            result = DialogResult::Close;
        }
    });
    result
}

fn settings(ui: &mut Ui, d: &mut LoadDialog) {
    Grid::new("load-test-settings")
        .num_columns(2)
        .spacing([12.0, 8.0])
        .show(ui, |ui| {
            let label = ui.label(t!("load-test-users"));
            ui.add(DragValue::new(&mut d.users).range(1..=MAX_USERS))
                .labelled_by(label.id)
                .on_hover_text(t!("load-test-users-hint"));
            ui.end_row();
            ui.label(t!("load-test-limit"));
            ui.horizontal(|ui| {
                let by_time =
                    ui.radio_value(&mut d.limit, Limit::Duration, t!("load-test-duration"));
                ui.add_enabled(
                    d.limit == Limit::Duration,
                    DragValue::new(&mut d.seconds).range(1..=3600).suffix(" s"),
                )
                .labelled_by(by_time.id);
                ui.add_space(12.0);
                let by_count =
                    ui.radio_value(&mut d.limit, Limit::Iterations, t!("load-test-iterations"));
                ui.add_enabled(
                    d.limit == Limit::Iterations,
                    DragValue::new(&mut d.iterations).range(1..=100_000),
                )
                .labelled_by(by_count.id);
            });
            ui.end_row();
            let label = ui.label(t!("load-test-ramp-up"));
            ui.add(
                DragValue::new(&mut d.ramp_up_s)
                    .range(0..=3600)
                    .suffix(" s"),
            )
            .labelled_by(label.id)
            .on_hover_text(t!("load-test-ramp-up-hint"));
            ui.end_row();
        });
    theme::hint(ui, t!("load-test-hint"));
}

fn report(ui: &mut Ui, run: &Running) {
    let p = theme::palette(ui);
    let ms = |v: Option<u64>| v.map_or_else(|| "—".to_owned(), |v| format!("{v} ms"));
    Grid::new("load-test-report")
        .num_columns(6)
        .striped(true)
        .spacing([18.0, 6.0])
        .show(ui, |ui| {
            for heading in [
                t!("load-test-step"),
                t!("load-test-sends"),
                t!("load-test-failed"),
                t!("load-test-median"),
                "p95".to_owned(),
                t!("load-test-max"),
            ] {
                ui.label(theme::semibold(heading));
            }
            ui.end_row();
            for s in &run.report.steps {
                ui.label(&s.name);
                ui.label(s.sends.to_string());
                let failed = RichText::new(s.failures.to_string());
                ui.label(if s.failures > 0 {
                    failed.color(p.fail)
                } else {
                    failed
                });
                ui.label(ms(s.median_ms()));
                ui.label(ms(s.p95_ms()));
                ui.label(ms(s.max_ms()));
                ui.end_row();
            }
        });
    ui.add_space(6.0);
    let (sends, failed) = run.report.sends();
    let state = if run.cancelled {
        t!("cancelled")
    } else if run.done {
        t!("load-test-done")
    } else {
        t!("running")
    };
    ui.horizontal(|ui| {
        if !run.done && !run.cancelled {
            ui.spinner();
        }
        ui.label(t!(
            "load-test-summary",
            state = state,
            sends = sends,
            failed = failed,
            seconds = crate::i18n::decimal(run.report.elapsed.as_secs_f64(), 1),
            rate = crate::i18n::decimal(run.report.sends_per_second(), 1)
        ));
    });
}

#[cfg(test)]
mod tests {
    use egui_kittest::kittest::Queryable as _;

    use super::*;

    fn key() -> WorkflowKey {
        WorkflowKey {
            root: "/p".into(),
            id: "buy".into(),
        }
    }

    /// R-FLOW-15: Start hands the shell the test once; reports of the running test
    /// fill the table, and a cancelled test takes no more.
    #[test]
    fn a_test_starts_once_and_takes_its_reports() {
        let mut tests = LoadTests::default();
        tests.open(key(), "Buy ü😀".into());
        assert!(tests.take_start().is_none());
        tests.dialog.as_mut().expect("open").wants_start = true;
        let (k, options) = tests.take_start().expect("start");
        assert_eq!((k, options.users), (key(), 10));
        assert_eq!(options.limit, LoadLimit::Duration(Duration::from_secs(30)));
        assert!(tests.take_start().is_none(), "once");

        let (abort, _) = AbortHandle::new_pair();
        let (id, tx) = tests.started(abort);
        let report = LoadReport {
            iterations: 3,
            ..LoadReport::default()
        };
        tx.send(Update::new(id, report.clone(), false))
            .expect("send");
        tx.send(Update::new(id + 7, LoadReport::default(), true))
            .expect("send");
        tests.take_updates();
        let run = tests
            .dialog
            .as_ref()
            .and_then(|d| d.run.as_ref())
            .expect("run");
        assert_eq!((run.report.iterations, run.done), (3, false));

        tests.dialog.as_mut().expect("open").cancel();
        tx.send(Update::new(id, LoadReport::default(), true))
            .expect("send");
        tests.take_updates();
        let run = tests
            .dialog
            .as_ref()
            .and_then(|d| d.run.as_ref())
            .expect("run");
        assert_eq!((run.report.iterations, run.cancelled), (3, true));
    }

    /// R-FLOW-15: the dialog's choices and the table.
    #[test]
    fn the_dialog_shows_its_settings_and_the_report() {
        let mut tests = LoadTests::default();
        tests.open(key(), "Buy".into());
        let mut d = tests.dialog.take().expect("open");
        let (abort, _) = AbortHandle::new_pair();
        d.run = Some(Running {
            id: 1,
            abort,
            report: LoadReport::default(),
            done: true,
            cancelled: false,
        });
        let mut h = crate::test_host::harness(|ui| {
            content(ui, &mut d);
        });
        h.run();
        for label in [
            "Virtual users",
            "Ramp-up",
            "Start",
            "Step",
            "Sends",
            "Median",
        ] {
            h.get_by_label(label);
        }
        h.get_by_label_contains("0 sends");
    }
}
