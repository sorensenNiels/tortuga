//! The workflow editor and its run results (R-FLOW-2, R-FLOW-3, R-FLOW-4), also for a
//! run of a project's or folder's requests that is not saved (R-FLOW-7), with the
//! runner's options (R-FLOW-8, R-FLOW-9).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use egui::{ComboBox, DragValue, Label, RichText, ScrollArea, TextEdit, Ui};
use tortuga_core::workflow::{FlowEnd, Repeat, Start, Step, StepResult, Tries, Workflow};
use tortuga_core::{HttpMethod, KeyValue};

use super::{TestsView, VarHelp, key_value_table, tests_details};
use crate::theme;

mod history;
mod results;

pub use history::{KEPT_RUNS, RunChoice, RunRecord, runs_picker};
pub use results::workflow_results;

/// Most of a response body shown in the results.
const BODY_PREVIEW_BYTES: usize = 64 * 1024;

/// A request that can be added as a step.
#[derive(Debug, Clone)]
pub struct RequestChoice {
    pub path: PathBuf,
    pub method: HttpMethod,
    pub name: String,
    /// Its folder, e.g. "Users › Admin".
    pub place: String,
}

/// Editor state that is not part of the workflow file.
#[derive(Debug, Clone, Default)]
pub struct WorkflowEditorState {
    /// The step whose captures and tests are open.
    expanded: Option<usize>,
    /// Each step's captures as table rows.
    captures: Vec<Vec<KeyValue>>,
    /// The step of the last run shown in detail: (iteration, step).
    pub selected_result: Option<(usize, usize)>,
}

impl WorkflowEditorState {
    pub fn new(workflow: &Workflow) -> Self {
        let mut st = Self::default();
        st.sync(workflow);
        st
    }

    /// Rebuilds the capture rows from the workflow (after steps were added, moved or removed).
    fn sync(&mut self, workflow: &Workflow) {
        self.captures = workflow
            .steps
            .iter()
            .map(|s| s.capture.iter().map(|(k, v)| KeyValue::new(k, v)).collect())
            .collect();
    }
}

/// One step of a run, ready to show.
#[derive(Debug, Clone)]
pub struct StepView {
    /// Its place in the workflow's steps (a step may run more than once, R-FLOW-6).
    pub index: usize,
    pub name: String,
    pub failure: Option<String>,
    pub status: Option<(u16, String)>,
    pub elapsed_ms: Option<u64>,
    pub tests: TestsView,
    /// "POST https://…" as sent.
    pub sent: Option<String>,
    /// The start of the response body, as text.
    pub body: Option<String>,
    /// How often a repeating step was sent (R-FLOW-10).
    pub tries: Option<Tries>,
    /// The skip condition that held: not sent (R-FLOW-12).
    pub skipped_by: Option<String>,
}

impl StepView {
    pub fn new(result: &StepResult) -> Self {
        let o = &result.outcome;
        let response = o.response.as_ref().and_then(|r| r.as_ref().ok());
        let mut errors = o.errors.clone();
        if let Some(Err(e)) = &o.response {
            errors.push(e.clone());
        }
        Self {
            index: result.index,
            name: result.name.clone(),
            failure: if result.failed() { o.failure() } else { None },
            status: response.map(|r| (r.status, r.status_text.clone())),
            elapsed_ms: response.map(|r| u64::try_from(r.elapsed.as_millis()).unwrap_or(u64::MAX)),
            tests: TestsView {
                tests: o.tests.clone(),
                console: o.console.clone(),
                // Workflow results are not shown on script lines.
                ran: tortuga_core::Scripts::default(),
                errors,
                vars: o.vars.clone(),
            },
            sent: o.shown().map(|r| {
                format!(
                    "{} {}",
                    r.method.as_str(),
                    tortuga_core::query::join_url(&r.url, &r.params)
                )
            }),
            body: response.map(|r| {
                let head = &r.body[..r.body.len().min(BODY_PREVIEW_BYTES)];
                String::from_utf8_lossy(head).into_owned()
            }),
            tries: result.tries,
            skipped_by: result.skipped_by.clone(),
        }
    }
}

/// One pass of a run.
#[derive(Debug, Clone, Default)]
pub struct IterationView {
    pub row: Option<BTreeMap<String, String>>,
    pub steps: Vec<StepView>,
    pub skipped: usize,
    /// How a script ended it early (R-FLOW-6).
    pub ended: Option<FlowEnd>,
}

impl IterationView {
    fn failed(&self) -> bool {
        self.skipped > 0
            || self.ended == Some(FlowEnd::TooManySteps)
            || self.steps.iter().any(|s| s.failure.is_some())
    }
}

/// A run, while it runs and after.
#[derive(Debug, Clone, Default)]
pub struct RunView {
    pub iterations: Vec<IterationView>,
    pub running: bool,
    /// The step running now: (iteration, step, try from 1).
    pub current: Option<(usize, usize, u32)>,
    pub steps_per_iteration: usize,
    pub iterations_planned: usize,
    /// The iterations are a data file's rows (R-FLOW-4), not repeats (R-FLOW-8).
    pub by_rows: bool,
    pub cancelled: bool,
    /// Where it started (R-FLOW-13).
    pub start: Start,
    /// What it started from, per iteration (R-FLOW-13, D86).
    pub start_values: Vec<BTreeMap<String, String>>,
}

impl RunView {
    /// What a run that starts at step `k` begins with (R-FLOW-13, D86): per
    /// iteration of this run that reached step `k`, the values it started with and
    /// what the steps before `k` set, up to the first iteration that didn't reach it.
    pub fn values_at(&self, k: usize) -> Vec<BTreeMap<String, String>> {
        let mut out = Vec::new();
        for (n, iteration) in self.iterations.iter().enumerate() {
            let Some(at) = iteration.steps.iter().position(|s| s.index == k) else {
                break;
            };
            let mut values = self
                .start_values
                .get(n)
                .or_else(|| self.start_values.first())
                .cloned()
                .unwrap_or_default();
            for step in &iteration.steps[..at] {
                tortuga_core::runner::apply_changes(&mut values, &step.tests.vars);
            }
            out.push(values);
        }
        out
    }
}

impl RunView {
    fn counts(&self) -> (usize, usize, usize) {
        let steps = self.iterations.iter().flat_map(|i| &i.steps);
        steps.fold((0, 0, 0), |(p, f, s), step| {
            let failed = step.tests.tests.iter().filter(|t| !t.passed).count();
            (
                p + step.tests.tests.len() - failed,
                f + failed,
                s + usize::from(step.failure.is_some()),
            )
        })
    }

    fn passed(&self) -> bool {
        !self.cancelled && !self.iterations.iter().any(IterationView::failed)
    }
}

/// Where the workflow in the editor is kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowFile {
    /// In its file, as edited.
    Saved,
    /// In its file, with edits not saved yet.
    Edited,
    /// Nowhere: a run of a project's or folder's requests (R-FLOW-7).
    None,
}

/// What the editor needs besides the workflow.
#[derive(Debug, Clone, Copy)]
pub struct WorkflowContext<'a> {
    /// The project's requests, for "Add step".
    pub requests: &'a [RequestChoice],
    pub read_only: bool,
    pub file: WorkflowFile,
    pub running: bool,
    pub can_pick_files: bool,
    pub addons: EditorAddOns,
}

/// Add-ons that add to the workflow editor.
#[derive(Debug, Clone, Copy, Default)]
pub struct EditorAddOns {
    /// "Load test…" (R-ADD-7, R-FLOW-15).
    pub load_test: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkflowAction {
    Save,
    /// Keep a run that is not saved as a workflow file (R-FLOW-7).
    SaveAs,
    Revert,
    Run,
    Cancel,
    /// Pick a data file (R-FLOW-4).
    ChooseData,
    /// Go to a step's request.
    OpenRequest(PathBuf),
    /// Run from a step, or only that step (R-FLOW-13).
    RunFrom(Start),
    /// Open the load test of this workflow (R-FLOW-15).
    LoadTest,
}

/// Name, options and steps of a workflow (R-FLOW-2).
pub fn workflow_editor(
    ui: &mut Ui,
    workflow: &mut Workflow,
    st: &mut WorkflowEditorState,
    cx: WorkflowContext<'_>,
) -> Option<WorkflowAction> {
    if st.captures.len() != workflow.steps.len() {
        st.sync(workflow);
    }
    let mut action = header(ui, workflow, cx);
    ui.add_space(6.0);
    ui.horizontal_wrapped(|ui| {
        ui.checkbox(
            &mut workflow.continue_on_failure,
            t!("workflow-continue-on-failure"),
        );
        ui.add_space(16.0);
        let label = ui.label(t!("workflow-data-file"));
        let mut data = workflow.data.clone().unwrap_or_default();
        let edit = ui
            .add(
                TextEdit::singleline(&mut data)
                    .id_salt("workflow-data")
                    .hint_text(t!("workflow-data-none"))
                    .desired_width(220.0),
            )
            .labelled_by(label.id)
            .on_hover_text(t!("workflow-data-hint"));
        if edit.changed() {
            workflow.data = Some(data).filter(|d| !d.trim().is_empty());
        }
        if cx.can_pick_files && ui.small_button(t!("choose")).clicked() {
            action = Some(WorkflowAction::ChooseData);
        }
        if workflow.data.is_some() && ui.small_button(t!("clear")).clicked() {
            workflow.data = None;
        }
    });
    ui.add_space(4.0);
    run_options(ui, workflow);
    ui.add_space(8.0);
    theme::section_label(ui, &t!("workflow-steps"));
    ScrollArea::vertical()
        .id_salt("workflow-steps")
        .auto_shrink([false, true])
        .show(ui, |ui| {
            if workflow.steps.is_empty() {
                theme::hint(ui, t!("workflow-steps-empty"));
            }
            let mut change = None;
            for i in 0..workflow.steps.len() {
                if let Some(a) = step_row(ui, workflow, st, i, cx, &mut change) {
                    action = Some(a);
                }
            }
            apply_change(workflow, st, change);
            if !cx.read_only {
                add_step(ui, workflow, st, cx.requests);
            }
        });
    action
}

/// Moves and removals, applied after the rows are drawn.
#[derive(Debug, Clone, Copy)]
enum StepChange {
    Up(usize),
    Down(usize),
    Remove(usize),
}

fn apply_change(workflow: &mut Workflow, st: &mut WorkflowEditorState, change: Option<StepChange>) {
    let Some(change) = change else { return };
    match change {
        StepChange::Up(i) if i > 0 => workflow.steps.swap(i, i - 1),
        StepChange::Down(i) if i + 1 < workflow.steps.len() => workflow.steps.swap(i, i + 1),
        StepChange::Remove(i) => drop(workflow.steps.remove(i)),
        _ => return,
    }
    st.expanded = None;
    st.sync(workflow);
}

/// Iterations, the delay between steps (R-FLOW-8) and keeping variable values (R-FLOW-9).
fn run_options(ui: &mut Ui, workflow: &mut Workflow) {
    ui.horizontal_wrapped(|ui| {
        let label = ui.label(t!("workflow-iterations"));
        let mut iterations = workflow.iterations.max(1);
        let hint = if workflow.data.is_some() {
            t!("workflow-iterations-by-rows")
        } else {
            t!("workflow-iterations-hint")
        };
        let edit = ui
            .add_enabled(
                workflow.data.is_none(),
                DragValue::new(&mut iterations).range(1..=10_000),
            )
            .labelled_by(label.id)
            .on_hover_text(hint.clone())
            .on_disabled_hover_text(hint);
        if edit.changed() {
            workflow.iterations = iterations;
        }
        ui.add_space(16.0);
        let label = ui.label(t!("workflow-delay"));
        ui.add(
            DragValue::new(&mut workflow.delay_ms)
                .range(0..=600_000)
                .speed(10.0)
                .suffix(" ms"),
        )
        .labelled_by(label.id)
        .on_hover_text(t!("workflow-delay-hint"));
        ui.add_space(16.0);
        ui.checkbox(&mut workflow.keep_variables, t!("workflow-keep-variables"))
            .on_hover_text(t!("workflow-keep-variables-hint"));
    });
}

fn header(ui: &mut Ui, workflow: &mut Workflow, cx: WorkflowContext<'_>) -> Option<WorkflowAction> {
    let p = theme::palette(ui);
    let mut action = None;
    egui::Sides::new().height(46.0).shrink_left().show(
        ui,
        |ui| {
            ui.vertical(|ui| {
                let caption = match cx.file {
                    WorkflowFile::Saved | WorkflowFile::Edited => t!("workflow"),
                    WorkflowFile::None => t!("workflow-unsaved-run"),
                };
                ui.label(RichText::new(caption).small().color(p.muted));
                ui.horizontal(|ui| {
                    let title =
                        egui::FontId::new(18.0, egui::FontFamily::Name(theme::SEMIBOLD.into()));
                    ui.add_enabled(
                        !cx.read_only,
                        TextEdit::singleline(&mut workflow.name)
                            .id_salt("workflow-name")
                            .font(title)
                            .frame(egui::Frame::new())
                            .clip_text(false)
                            .desired_width(24.0),
                    )
                    .on_hover_text(t!("workflow-name"));
                    if cx.file == WorkflowFile::Edited {
                        theme::badge(ui, t!("unsaved"), p.redirect);
                    }
                });
            });
        },
        |ui| {
            if cx.running {
                if ui.add(theme::danger_button(ui, t!("cancel"))).clicked() {
                    action = Some(WorkflowAction::Cancel);
                }
            } else if ui
                .add_enabled(
                    !workflow.steps.is_empty(),
                    theme::primary_button(ui, t!("workflow-run")),
                )
                .on_hover_text(t!("workflow-run-hint"))
                .clicked()
            {
                action = Some(WorkflowAction::Run);
            }
            if cx.addons.load_test
                && ui
                    .add_enabled(
                        !workflow.steps.is_empty(),
                        egui::Button::new(t!("load-test-open")),
                    )
                    .on_hover_text(t!("load-test-open-hint"))
                    .clicked()
            {
                action = Some(WorkflowAction::LoadTest);
            }
            match cx.file {
                WorkflowFile::Saved | WorkflowFile::Edited => {
                    let edited = cx.file == WorkflowFile::Edited;
                    if ui
                        .add_enabled(edited && !cx.read_only, egui::Button::new(t!("save")))
                        .on_hover_text(t!("save-shortcut"))
                        .clicked()
                    {
                        action = Some(WorkflowAction::Save);
                    }
                    if edited && ui.button(t!("revert")).clicked() {
                        action = Some(WorkflowAction::Revert);
                    }
                }
                WorkflowFile::None => {
                    if ui
                        .add_enabled(!cx.read_only, egui::Button::new(t!("workflow-save-as")))
                        .on_hover_text(t!("workflow-save-as-hint"))
                        .clicked()
                    {
                        action = Some(WorkflowAction::SaveAs);
                    }
                }
            }
        },
    );
    action
}

/// One step: its request, summary and buttons; when open, its captures and tests.
fn step_row(
    ui: &mut Ui,
    workflow: &mut Workflow,
    st: &mut WorkflowEditorState,
    i: usize,
    cx: WorkflowContext<'_>,
    change: &mut Option<StepChange>,
) -> Option<WorkflowAction> {
    let p = theme::palette(ui);
    let mut action = None;
    // A copy to show; the step itself is edited below.
    let step = workflow.steps[i].clone();
    let request = cx.requests.iter().find(|r| r.path == step.request_path());
    let open = st.expanded == Some(i);
    ui.push_id(("step", i), |ui| {
        egui::Sides::new().shrink_left().truncate().show(
            ui,
            |ui| {
                // Fixed columns, so the names line up (labels drop trailing spaces).
                let number = ui.label(RichText::new(format!("{}.", i + 1)).color(p.muted));
                ui.add_space((20.0 - number.rect.width()).max(0.0));
                match request {
                    Some(r) => {
                        let tag = ui.label(
                            RichText::new(theme::method_short(r.method))
                                .monospace()
                                .size(10.5)
                                .color(theme::method_color(ui, r.method)),
                        );
                        ui.add_space((40.0 - tag.rect.width()).max(0.0));
                        ui.add(
                            Label::new(step.name.clone().unwrap_or_else(|| r.name.clone()))
                                .truncate(),
                        );
                    }
                    None => {
                        ui.label(
                            RichText::new(t!("workflow-step-missing", request = &step.request))
                                .color(p.fail),
                        );
                    }
                }
                let extras = step_extras(&step);
                if !extras.is_empty() {
                    ui.label(RichText::new(extras.join(" · ")).small().color(p.muted));
                }
            },
            |ui| {
                if !cx.read_only {
                    if ui
                        .add(theme::icon_button(ui, "🗙"))
                        .on_hover_text(t!("workflow-step-remove"))
                        .clicked()
                    {
                        *change = Some(StepChange::Remove(i));
                    }
                    if ui
                        .add(theme::icon_button(ui, "↓"))
                        .on_hover_text(t!("move-down"))
                        .clicked()
                    {
                        *change = Some(StepChange::Down(i));
                    }
                    if ui
                        .add(theme::icon_button(ui, "↑"))
                        .on_hover_text(t!("move-up"))
                        .clicked()
                    {
                        *change = Some(StepChange::Up(i));
                    }
                }
                if let Some(run) = run_menu(ui, i, cx.running) {
                    action = Some(run);
                }
                if request.is_some()
                    && ui
                        .small_button(t!("open"))
                        .on_hover_text(t!("workflow-step-open"))
                        .clicked()
                {
                    action = Some(WorkflowAction::OpenRequest(step.request_path()));
                }
                let label = if open {
                    t!("close")
                } else {
                    t!("workflow-step-checks")
                };
                if ui
                    .small_button(label)
                    .on_hover_text(t!("workflow-step-checks-hint"))
                    .clicked()
                {
                    st.expanded = if open { None } else { Some(i) };
                }
            },
        );
        if open {
            step_details(
                ui,
                &mut workflow.steps[i],
                &mut st.captures[i],
                request,
                cx.read_only,
            );
        }
    });
    action
}

/// "▶" on a step's row: run from it, or only it (R-FLOW-13).
fn run_menu(ui: &mut Ui, i: usize, running: bool) -> Option<WorkflowAction> {
    let mut action = None;
    ui.add_enabled_ui(!running, |ui| {
        ui.menu_button("▶", |ui| {
            if ui.button(t!("workflow-run-from-here")).clicked() {
                action = Some(WorkflowAction::RunFrom(Start::From(i)));
            }
            if ui.button(t!("workflow-run-this-step")).clicked() {
                action = Some(WorkflowAction::RunFrom(Start::Only(i)));
            }
        })
        .response
        .on_hover_text(t!("workflow-run-from-hint"));
    });
    action
}

/// What a step adds to its request, for its row: captures, tests, repeating.
fn step_extras(step: &Step) -> Vec<String> {
    let mut extras = Vec::new();
    if !step.capture.is_empty() {
        extras.push(t!("workflow-captures", count = step.capture.len()));
    }
    if !step.tests.trim().is_empty() {
        extras.push(t!("workflow-step-has-tests"));
    }
    if let Some(repeat) = step.repeat {
        extras.push(t!("workflow-step-repeats", count = repeat.times));
    }
    if step.always {
        extras.push(t!("workflow-step-always-runs"));
    }
    if !step.skip_when.trim().is_empty() {
        extras.push(t!("workflow-step-has-skip-when"));
    }
    extras
}

fn step_details(
    ui: &mut Ui,
    step: &mut Step,
    captures: &mut Vec<KeyValue>,
    request: Option<&RequestChoice>,
    read_only: bool,
) {
    egui::Frame::new()
        .inner_margin(egui::Margin {
            left: 28,
            right: 0,
            top: 4,
            bottom: 10,
        })
        .show(ui, |ui| {
            ui.add_enabled_ui(!read_only, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.label(t!("workflow-step-name"));
                    let mut name = step.name.clone().unwrap_or_default();
                    let hint = request.map_or("", |r| r.name.as_str());
                    if ui
                        .add(
                            TextEdit::singleline(&mut name)
                                .id_salt("step-name")
                                .hint_text(hint)
                                .desired_width(260.0),
                        )
                        .labelled_by(label.id)
                        .changed()
                    {
                        step.name = Some(name).filter(|n| !n.trim().is_empty());
                    }
                });
                ui.add_space(4.0);
                theme::section_label(ui, &t!("captures"));
                if key_value_table(
                    ui,
                    "step-captures",
                    captures,
                    &t!("variable"),
                    &VarHelp::default(),
                ) {
                    step.capture = captures
                        .iter()
                        .filter(|c| c.is_active())
                        .map(|c| (c.name.trim().to_owned(), c.value.trim().to_owned()))
                        .collect();
                }
                theme::hint(ui, t!("workflow-captures-hint"));
                ui.add_space(4.0);
                super::script_editor::script_heading(ui, &t!("tests"), &mut step.tests);
                // Line numbers, to find the lines the Console names; no breakpoints here.
                super::script_editor::script_editor(
                    ui,
                    "step-tests",
                    &mut step.tests,
                    4,
                    "test(\"created\", () => expect(res.status).toBe(201));",
                    None,
                    &std::collections::BTreeMap::new(),
                );
                theme::hint(ui, t!("workflow-step-tests-hint"));
                ui.add_space(4.0);
                repeat_options(ui, step);
                flow_options(ui, step);
            });
        });
}

/// "Repeat until it passes", with its tries and wait (R-FLOW-10).
fn repeat_options(ui: &mut Ui, step: &mut Step) {
    ui.horizontal_wrapped(|ui| {
        let mut on = step.repeat.is_some();
        if ui
            .checkbox(&mut on, t!("workflow-step-repeat"))
            .on_hover_text(t!("workflow-step-repeat-hint"))
            .changed()
        {
            step.repeat = on.then(Repeat::default);
        }
        let Some(repeat) = step.repeat.as_mut() else {
            return;
        };
        ui.add_space(12.0);
        let label = ui.label(t!("workflow-step-tries"));
        ui.add(DragValue::new(&mut repeat.times).range(2..=10_000))
            .labelled_by(label.id)
            .on_hover_text(t!("workflow-step-tries-hint"));
        ui.add_space(12.0);
        let label = ui.label(t!("workflow-step-wait"));
        ui.add(
            DragValue::new(&mut repeat.wait_ms)
                .range(0..=3_600_000)
                .speed(10.0)
                .suffix(" ms"),
        )
        .labelled_by(label.id)
        .on_hover_text(t!("workflow-step-wait-hint"));
    });
}

/// "Always run" (R-FLOW-11) and "Skip when" (R-FLOW-12).
fn flow_options(ui: &mut Ui, step: &mut Step) {
    ui.checkbox(&mut step.always, t!("workflow-step-always"))
        .on_hover_text(t!("workflow-step-always-hint"));
    ui.horizontal(|ui| {
        let label = ui.label(t!("workflow-step-skip-when"));
        ui.add(
            TextEdit::singleline(&mut step.skip_when)
                .id_salt("step-skip-when")
                .code_editor()
                .hint_text("vars.get(\"role\") !== \"admin\"")
                .desired_width(360.0),
        )
        .labelled_by(label.id)
        .on_hover_text(t!("workflow-step-skip-when-hint"));
    });
}

fn add_step(
    ui: &mut Ui,
    workflow: &mut Workflow,
    st: &mut WorkflowEditorState,
    requests: &[RequestChoice],
) {
    ui.add_space(6.0);
    let mut picked: Option<&Path> = None;
    ComboBox::from_id_salt("add-step")
        .selected_text(t!("workflow-add-step"))
        .width(320.0)
        .height(400.0)
        .show_ui(ui, |ui| {
            if requests.is_empty() {
                theme::hint(ui, t!("workflow-no-requests"));
            }
            for r in requests {
                let text = format!("{}  {}  {}", theme::method_short(r.method), r.name, r.place);
                if ui.selectable_label(false, text).clicked() {
                    picked = Some(&r.path);
                }
            }
        })
        .response
        .on_hover_text(t!("workflow-add-step-hint"));
    if let Some(path) = picked {
        workflow.steps.push(Step::new(path));
        st.sync(workflow);
    }
}

#[cfg(test)]
mod tests {
    use egui_kittest::kittest::Queryable;

    use super::*;

    fn requests() -> Vec<RequestChoice> {
        vec![
            RequestChoice {
                path: "auth/login.toml".into(),
                method: HttpMethod::Post,
                name: "Log in".into(),
                place: "Auth".into(),
            },
            RequestChoice {
                path: "users/get.toml".into(),
                method: HttpMethod::Get,
                name: "Get user".into(),
                place: "Users".into(),
            },
        ]
    }

    /// R-FLOW-2: steps are added, moved and removed; a missing request is marked.
    #[test]
    fn steps_are_added_moved_and_removed() {
        let mut wf = Workflow {
            name: "Flow".into(),
            steps: vec![
                Step::new(Path::new("auth/login.toml")),
                Step::new(Path::new("gone.toml")),
            ],
            ..Workflow::default()
        };
        let mut st = WorkflowEditorState::new(&wf);
        let requests = requests();
        {
            let mut h = crate::test_host::harness(|ui| {
                let cx = WorkflowContext {
                    requests: &requests,
                    read_only: false,
                    file: WorkflowFile::Edited,
                    running: false,
                    can_pick_files: false,
                    addons: EditorAddOns::default(),
                };
                workflow_editor(ui, &mut wf, &mut st, cx);
            });
            h.run();
            h.get_by_label_contains("gone.toml is not in the project");
            h.get_by_label("Add a request of this project as the next step")
                .click();
            h.run();
            h.get_by_label_contains("Get user").click();
            h.run();
            // Move the new step up, then remove the missing one.
            let ups: Vec<_> = h.query_all_by_label("↑").collect();
            ups[2].click();
            h.run();
            h.query_all_by_label("🗙").nth(2).expect("remove").click();
            h.run();
        }
        let paths: Vec<&str> = wf.steps.iter().map(|s| s.request.as_str()).collect();
        assert_eq!(paths, ["auth/login.toml", "users/get.toml"]);
    }

    /// R-FLOW-3: a running workflow can be cancelled.
    #[test]
    fn a_running_workflow_can_be_cancelled() {
        let mut wf = Workflow {
            name: "Flow".into(),
            steps: vec![Step::new(Path::new("auth/login.toml"))],
            ..Workflow::default()
        };
        let mut st = WorkflowEditorState::new(&wf);
        let requests = requests();
        let action = std::cell::Cell::new(None);
        let mut h = crate::test_host::harness(|ui| {
            let cx = WorkflowContext {
                requests: &requests,
                read_only: false,
                file: WorkflowFile::Saved,
                running: true,
                can_pick_files: false,
                addons: EditorAddOns::default(),
            };
            if let Some(a) = workflow_editor(ui, &mut wf, &mut st, cx) {
                action.set(Some(a));
            }
        });
        h.run();
        h.get_by_label("Cancel").click();
        h.run();
        drop(h);
        assert!(matches!(action.take(), Some(WorkflowAction::Cancel)));
    }

    #[test]
    fn results_show_steps_and_failures() {
        let tests = TestsView {
            tests: vec![tortuga_core::script::TestResult {
                name: "found".into(),
                passed: false,
                message: Some("Expected 404 to be 200".into()),
            }],
            ..TestsView::default()
        };
        let run = RunView {
            iterations: vec![IterationView {
                row: None,
                ended: None,
                steps: vec![StepView {
                    index: 0,
                    skipped_by: None,
                    name: "Get user".into(),
                    failure: Some("test “found” failed: Expected 404 to be 200".into()),
                    status: Some((404, "Not Found".into())),
                    elapsed_ms: Some(12),
                    tests,
                    sent: Some("GET https://api.test/users/bob".into()),
                    body: Some(String::new()),
                    tries: Some(Tries {
                        count: 7,
                        elapsed: std::time::Duration::from_millis(12_400),
                    }),
                }],
                skipped: 1,
            }],
            iterations_planned: 1,
            steps_per_iteration: 2,
            ..RunView::default()
        };
        let mut st = WorkflowEditorState::default();
        let mut h = crate::test_host::harness(|ui| workflow_results(ui, Some(&run), &mut st));
        h.run();
        h.get_by_label("Failed");
        h.get_by_label_contains("1 later step skipped");
        // R-FLOW-10: a step that repeated says how often it was sent.
        h.get_by_label_contains("after 7 tries (12.4 s)");
        h.get_by_label_contains("1. Get user").click();
        h.run();
        h.get_by_label("GET https://api.test/users/bob");
        h.get_by_label("Expected 404 to be 200");
    }

    /// R-FLOW-6: an iteration a script stopped passes and says so; one stopped by the
    /// step limit fails.
    #[test]
    fn results_say_how_scripts_ended_a_run() {
        for (end, verdict) in [
            (FlowEnd::Stopped, "Passed"),
            (FlowEnd::TooManySteps, "Failed"),
        ] {
            let run = RunView {
                iterations: vec![IterationView {
                    steps: vec![StepView {
                        index: 0,
                        skipped_by: None,
                        name: "Log in".into(),
                        failure: None,
                        status: Some((200, "OK".into())),
                        elapsed_ms: Some(3),
                        tests: TestsView::default(),
                        sent: None,
                        body: None,
                        tries: None,
                    }],
                    ended: Some(end),
                    ..IterationView::default()
                }],
                iterations_planned: 1,
                steps_per_iteration: 2,
                ..RunView::default()
            };
            let mut st = WorkflowEditorState::default();
            let mut h = crate::test_host::harness(|ui| workflow_results(ui, Some(&run), &mut st));
            h.run();
            h.get_by_label(verdict);
            h.get_by_label(&end.message());
        }
    }

    /// R-FLOW-10: "Repeat until it passes" turns on with 10 tries a second apart, and the
    /// step's row says so; turning it off removes it from the step.
    #[test]
    fn a_step_can_repeat_until_it_passes() {
        let mut step = Step::new(Path::new("jobs/status.toml"));
        let mut h = crate::test_host::harness(|ui| repeat_options(ui, &mut step));
        h.run();
        h.get_by_label("Repeat until it passes").click();
        h.run();
        h.get_by_label("Tries");
        drop(h);
        assert_eq!(step.repeat, Some(Repeat::default()));
        assert_eq!(step_extras(&step), ["up to 10 tries"]);
        let mut h = crate::test_host::harness(|ui| repeat_options(ui, &mut step));
        h.run();
        h.get_by_label("Repeat until it passes").click();
        h.run();
        drop(h);
        assert_eq!(step.repeat, None);
    }

    fn step_view(index: usize, vars: &[(&str, Option<&str>)]) -> StepView {
        StepView::new(&StepResult {
            index,
            name: format!("Step {index}"),
            request: "r.toml".into(),
            outcome: tortuga_core::runner::StepOutcome {
                vars: vars
                    .iter()
                    .map(|(k, v)| ((*k).to_owned(), v.map(str::to_owned)))
                    .collect(),
                ..Default::default()
            },
            tries: None,
            skipped_by: None,
        })
    }

    /// R-FLOW-13, D86: a run from step k starts, per iteration that reached k, from
    /// what that run started with and what the steps before k changed; iterations
    /// from the first that didn't reach k on are left out.
    #[test]
    fn a_run_from_a_step_starts_with_the_last_runs_values() {
        let run = RunView {
            start_values: vec![[("base".to_owned(), "ü😀".to_owned())].into()],
            iterations: vec![
                IterationView {
                    steps: vec![
                        step_view(0, &[("token", Some("t-1")), ("base", None)]),
                        step_view(1, &[("id", Some("7"))]),
                        step_view(2, &[("late", Some("x"))]),
                    ],
                    ..IterationView::default()
                },
                IterationView {
                    steps: vec![step_view(0, &[("token", Some("t-2"))])],
                    ..IterationView::default()
                },
            ],
            ..RunView::default()
        };
        let values = run.values_at(2);
        assert_eq!(values.len(), 1, "the second iteration never reached step 3");
        assert_eq!(
            values[0],
            BTreeMap::from([
                ("id".to_owned(), "7".to_owned()),
                ("token".to_owned(), "t-1".to_owned()),
            ])
        );
        assert_eq!(run.values_at(0).len(), 2);
        assert_eq!(run.values_at(0)[0]["base"], "ü😀");
        assert_eq!(run.values_at(9), Vec::<BTreeMap<String, String>>::new());
    }

    /// R-FLOW-11, R-FLOW-12: the step's options and what its row says about them.
    #[test]
    fn steps_can_always_run_and_be_skipped() {
        let mut step = Step::new(Path::new("users/delete.toml"));
        let mut h = crate::test_host::harness(|ui| flow_options(ui, &mut step));
        h.run();
        h.get_by_label("Always run").click();
        h.run();
        h.get_by_label("Skip when");
        drop(h);
        assert!(step.always);
        step.skip_when = "vars.get(\"keep\") === \"yes\"".into();
        assert_eq!(step_extras(&step), ["always runs", "skip condition"]);
    }

    /// R-FLOW-12, R-FLOW-13: a skipped step says why and passes; a run that started
    /// at a step says where and from what.
    #[test]
    fn results_show_skipped_steps_and_where_a_run_started() {
        let skipped = StepView::new(&StepResult {
            index: 1,
            name: "Purge".into(),
            request: "purge.toml".into(),
            outcome: tortuga_core::runner::StepOutcome::default(),
            tries: None,
            skipped_by: Some("vars.get(\"role\") !== \"admin\"".into()),
        });
        let run = RunView {
            iterations: vec![IterationView {
                steps: vec![skipped],
                ..IterationView::default()
            }],
            iterations_planned: 1,
            steps_per_iteration: 3,
            start: Start::From(1),
            start_values: vec![BTreeMap::new()],
            ..RunView::default()
        };
        let mut st = WorkflowEditorState::default();
        let mut h = crate::test_host::harness(|ui| workflow_results(ui, Some(&run), &mut st));
        h.run();
        h.get_by_label("Passed");
        h.get_by_label_contains("skipped: vars.get(\"role\") !== \"admin\"");
        h.get_by_label("Started at step 2 with the values the last run had there.");
    }

    /// R-FLOW-14: a kept run reads back as the results showed it, without bodies.
    #[test]
    fn kept_runs_read_back_as_they_were_shown() {
        let mut repeated = step_view(0, &[]);
        repeated.status = Some((202, "Accepted ü😀".into()));
        repeated.tries = Some(Tries {
            count: 3,
            elapsed: std::time::Duration::from_millis(2400),
        });
        repeated.body = Some("{\"big\": true}".into());
        let mut skipped = step_view(1, &[]);
        skipped.failure = None;
        skipped.skipped_by = Some("vars.get(\"x\")".into());
        let run = RunView {
            iterations: vec![IterationView {
                row: Some([("user".to_owned(), "ann".to_owned())].into()),
                steps: vec![repeated, skipped],
                skipped: 1,
                ended: Some(FlowEnd::Stopped),
            }],
            iterations_planned: 2,
            steps_per_iteration: 3,
            by_rows: true,
            ..RunView::default()
        };
        let record = RunRecord::new(&run, 1_700_000_000_000);
        let json = serde_json::to_string(&record).expect("json");
        let back: RunRecord = serde_json::from_str(&json).expect("read");
        assert_eq!(back, record);
        let view = back.view();
        let it = &view.iterations[0];
        assert_eq!((it.skipped, it.ended), (1, Some(FlowEnd::Stopped)));
        assert_eq!(it.row, run.iterations[0].row);
        assert_eq!(it.steps[0].status, run.iterations[0].steps[0].status);
        assert_eq!(it.steps[0].tries, run.iterations[0].steps[0].tries);
        assert_eq!(it.steps[0].body, None, "bodies are not kept");
        assert_eq!(it.steps[1].skipped_by.as_deref(), Some("vars.get(\"x\")"));
        assert!(view.by_rows && !view.running);
    }
}
