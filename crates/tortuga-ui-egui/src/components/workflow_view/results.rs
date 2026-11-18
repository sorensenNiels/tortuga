//! The results of a workflow run (R-FLOW-3): per iteration and step, with repeats
//! (R-FLOW-10), skipped steps (R-FLOW-12) and where the run started (R-FLOW-13).

use egui::{Label, RichText, ScrollArea, Ui};
use tortuga_core::workflow::{FlowEnd, Start, Tries};

use super::{RunView, StepView, WorkflowEditorState, tests_details};
use crate::theme;

/// The last run: per iteration and step, with details of the chosen step (R-FLOW-3).
pub fn workflow_results(ui: &mut Ui, run: Option<&RunView>, st: &mut WorkflowEditorState) {
    let p = theme::palette(ui);
    let Some(run) = run else {
        ui.vertical_centered(|ui| {
            ui.add_space(30.0);
            ui.label(RichText::new("⏵").size(28.0).color(p.border));
            ui.label(theme::semibold(t!("workflow-not-run")).color(p.muted));
            theme::hint(ui, t!("workflow-not-run-hint"));
        });
        return;
    };
    summary(ui, run);
    if let Some(note) = start_note(run) {
        theme::hint(ui, note);
    }
    ui.add_space(6.0);
    let multi = run.iterations_planned > 1;
    // The list takes the whole page until a step is chosen; then it shares it.
    let has_chosen = st
        .selected_result
        .is_some_and(|(n, i)| run.iterations.get(n).is_some_and(|it| i < it.steps.len()));
    let share = if has_chosen { 0.45 } else { 1.0 };
    ScrollArea::vertical()
        .id_salt("workflow-results")
        .max_height(ui.available_height() * share)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            for (n, iteration) in run.iterations.iter().enumerate() {
                if multi {
                    let failed = iteration.failed();
                    let row = iteration
                        .row
                        .as_ref()
                        .map(|r| {
                            r.iter()
                                .map(|(k, v)| format!("{k}={v}"))
                                .collect::<Vec<_>>()
                                .join(", ")
                        })
                        .unwrap_or_default();
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        mark(ui, !failed);
                        let label = if run.by_rows {
                            t!("workflow-row", row = n + 1)
                        } else {
                            t!("workflow-iteration", n = n + 1)
                        };
                        ui.label(theme::semibold(label));
                        ui.add(Label::new(RichText::new(row).small().color(p.muted)).truncate());
                    });
                }
                for (i, step) in iteration.steps.iter().enumerate() {
                    if step_result_row(ui, step, i, st.selected_result == Some((n, i)), multi) {
                        st.selected_result = Some((n, i));
                    }
                }
                if iteration.skipped > 0 {
                    theme::hint(ui, t!("workflow-skipped", count = iteration.skipped));
                }
                match iteration.ended {
                    Some(end @ FlowEnd::Stopped) => theme::hint(ui, end.message()),
                    Some(end @ FlowEnd::TooManySteps) => theme::error(ui, end.message()),
                    None => {}
                }
            }
            if let Some(current) = run.current {
                running_line(ui, run, current, multi);
            }
        });
    let chosen = st
        .selected_result
        .and_then(|(n, i)| run.iterations.get(n)?.steps.get(i));
    if let Some(step) = chosen {
        ui.separator();
        step_detail(ui, step);
    }
}

fn summary(ui: &mut Ui, run: &RunView) {
    let p = theme::palette(ui);
    let (passed, failed, failed_steps) = run.counts();
    ui.horizontal_wrapped(|ui| {
        if run.running {
            ui.spinner();
            ui.label(theme::semibold(t!("running")));
        } else if run.cancelled {
            theme::badge(ui, t!("cancelled"), p.muted);
        } else if run.passed() {
            theme::badge(ui, t!("passed"), p.ok);
        } else {
            theme::badge(ui, t!("failed"), p.fail);
        }
        let mut parts = vec![
            t!("workflow-tests-passed", count = passed),
            t!("workflow-tests-failed", count = failed),
        ];
        if failed_steps > 0 {
            parts.push(t!("workflow-steps-failed", count = failed_steps));
        }
        if run.iterations_planned > 1 {
            let done = run.iterations.len();
            let planned = run.iterations_planned;
            parts.push(if run.by_rows {
                t!("workflow-rows-done", done = done, planned = planned)
            } else {
                t!("workflow-iterations-done", done = done, planned = planned)
            });
        }
        ui.label(RichText::new(parts.join(" · ")).color(p.muted));
    });
}

fn mark(ui: &mut Ui, ok: bool) {
    let p = theme::palette(ui);
    if ok {
        ui.label(RichText::new("✓").color(p.ok));
    } else {
        ui.label(RichText::new("✗").color(p.fail));
    }
}

/// The step running now (R-FLOW-3), and its try when it repeats (R-FLOW-10).
fn running_line(ui: &mut Ui, run: &RunView, (n, i, attempt): (usize, usize, u32), multi: bool) {
    let p = theme::palette(ui);
    ui.horizontal(|ui| {
        ui.spinner();
        let of = run.steps_per_iteration;
        let label = if multi && run.by_rows {
            t!(
                "workflow-running-row-step",
                row = n + 1,
                step = i + 1,
                of = of
            )
        } else if multi {
            t!(
                "workflow-running-iteration-step",
                n = n + 1,
                step = i + 1,
                of = of
            )
        } else {
            t!("workflow-running-step", step = i + 1, of = of)
        };
        let label = if attempt > 1 {
            format!("{label} {}", t!("workflow-running-try", attempt = attempt))
        } else {
            label
        };
        ui.label(RichText::new(label).color(p.muted));
    });
}

/// Where a run that did not start at the first step started, and from what values
/// (R-FLOW-13).
fn start_note(run: &RunView) -> Option<String> {
    let (at, only) = match run.start {
        Start::First => return None,
        Start::From(i) => (i, false),
        Start::Only(i) => (i, true),
    };
    let place = if only {
        t!("workflow-ran-only-step", step = at + 1)
    } else {
        t!("workflow-started-at-step", step = at + 1)
    };
    let values = if run.start_values.is_empty() {
        t!("workflow-started-clean")
    } else {
        t!("workflow-started-with-last-run")
    };
    Some(format!("{place} {values}"))
}

/// A step's line in the results. Returns whether it was clicked.
fn step_result_row(ui: &mut Ui, step: &StepView, i: usize, selected: bool, indent: bool) -> bool {
    let p = theme::palette(ui);
    let mut detail = Vec::new();
    if let Some((code, text)) = &step.status {
        detail.push(format!("{code} {text}"));
    }
    if let Some(ms) = step.elapsed_ms {
        detail.push(format!("{ms} ms"));
    }
    if let Some(tries) = step.tries {
        detail.push(tries_text(tries));
    }
    if let Some(condition) = &step.skipped_by {
        detail.push(t!("workflow-step-skipped", condition = condition.as_str()));
    }
    if !step.tests.tests.is_empty() {
        let passed = step.tests.tests.iter().filter(|t| t.passed).count();
        detail.push(t!(
            "workflow-step-tests",
            passed = passed,
            total = step.tests.tests.len()
        ));
    }
    let (glyph, color) = if step.skipped_by.is_some() {
        ("−", p.muted)
    } else if step.failure.is_some() {
        ("✗", p.fail)
    } else {
        ("✓", p.ok)
    };
    let atoms = [
        egui::Atom::from(RichText::new(glyph).color(color)),
        egui::Atom::from(format!("{}. {}", i + 1, step.name)),
        egui::Atom::grow(),
        egui::Atom::from(RichText::new(detail.join(" · ")).small().color(p.muted)),
    ];
    let atoms: egui::Atoms<'_> = atoms.into_iter().collect();
    let response = ui
        .horizontal(|ui| {
            if indent {
                ui.add_space(18.0);
            }
            ui.add(
                egui::Button::selectable(selected, atoms)
                    .frame_when_inactive(selected)
                    .truncate()
                    .min_size(egui::vec2(ui.available_width(), 26.0)),
            )
        })
        .inner;
    if let Some(why) = &step.failure {
        ui.horizontal(|ui| {
            ui.add_space(if indent { 46.0 } else { 28.0 });
            ui.add(Label::new(RichText::new(why).small().color(p.fail)).truncate());
        });
    }
    response.clicked()
}

fn step_detail(ui: &mut Ui, step: &StepView) {
    let p = theme::palette(ui);
    ScrollArea::vertical()
        .id_salt("workflow-step-detail")
        .auto_shrink(false)
        .show(ui, |ui| {
            ui.label(theme::semibold(&step.name));
            if let Some(sent) = &step.sent {
                ui.add(Label::new(RichText::new(sent).monospace().color(p.muted)).wrap());
            }
            ui.add_space(4.0);
            tests_details(ui, &step.tests);
            if !step.tests.console.is_empty() {
                ui.add_space(8.0);
                theme::section_label(ui, &t!("console"));
                super::super::console_view::console_view(ui, &step.tests.console);
            }
            if let Some(body) = &step.body {
                ui.add_space(8.0);
                theme::section_label(ui, &t!("response-body"));
                if body.is_empty() {
                    theme::hint(ui, t!("no-body"));
                } else {
                    ui.add(Label::new(RichText::new(body).monospace()).wrap());
                }
            }
        });
}

/// "after 7 tries (12.4 s)" (R-FLOW-10).
pub(super) fn tries_text(tries: Tries) -> String {
    t!(
        "workflow-step-tries-done",
        count = tries.count,
        seconds = crate::i18n::decimal(tries.elapsed.as_secs_f64(), 1)
    )
}
