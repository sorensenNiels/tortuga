//! The workflow editor and its run results (R-FLOW-2, R-FLOW-3, R-FLOW-4).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use egui::{ComboBox, Label, RichText, ScrollArea, TextEdit, Ui};
use tortuga_core::workflow::{FlowEnd, Step, StepResult, Workflow};
use tortuga_core::{HttpMethod, KeyValue};

use super::{TestsView, key_value_table, tests_details};
use crate::theme;

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
    pub name: String,
    pub failure: Option<String>,
    pub status: Option<(u16, String)>,
    pub elapsed_ms: Option<u64>,
    pub tests: TestsView,
    /// "POST https://…" as sent.
    pub sent: Option<String>,
    /// The start of the response body, as text.
    pub body: Option<String>,
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
            name: result.name.clone(),
            failure: o.failure(),
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
            sent: o.sent.as_ref().map(|r| {
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
    /// The step running now: (iteration, step).
    pub current: Option<(usize, usize)>,
    pub steps_per_iteration: usize,
    pub iterations_planned: usize,
    pub cancelled: bool,
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

/// What the editor needs besides the workflow.
#[derive(Debug, Clone, Copy)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent facts the editor shows"
)]
pub struct WorkflowContext<'a> {
    /// The project's requests, for "Add step".
    pub requests: &'a [RequestChoice],
    pub read_only: bool,
    /// The workflow has unsaved changes.
    pub dirty: bool,
    pub running: bool,
    pub can_pick_files: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkflowAction {
    Save,
    Revert,
    Run,
    Cancel,
    /// Pick a data file (R-FLOW-4).
    ChooseData,
    /// Go to a step's request.
    OpenRequest(PathBuf),
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
        ui.checkbox(&mut workflow.continue_on_failure, "Continue after a failed step");
        ui.add_space(16.0);
        let label = ui.label("Data file");
        let mut data = workflow.data.clone().unwrap_or_default();
        let edit = ui
            .add(
                TextEdit::singleline(&mut data)
                    .id_salt("workflow-data")
                    .hint_text("none: one run")
                    .desired_width(220.0),
            )
            .labelled_by(label.id)
            .on_hover_text("A CSV or JSON file, relative to the project: one run per row, its values as variables");
        if edit.changed() {
            workflow.data = Some(data).filter(|d| !d.trim().is_empty());
        }
        if cx.can_pick_files && ui.small_button("Choose…").clicked() {
            action = Some(WorkflowAction::ChooseData);
        }
        if workflow.data.is_some() && ui.small_button("Clear").clicked() {
            workflow.data = None;
        }
    });
    ui.add_space(8.0);
    theme::section_label(ui, "Steps");
    ScrollArea::vertical()
        .id_salt("workflow-steps")
        .auto_shrink([false, true])
        .show(ui, |ui| {
            if workflow.steps.is_empty() {
                theme::hint(ui, "Add the requests to run, in order. Values captured by a step can be used by the next ones as {{variables}}.");
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

fn header(ui: &mut Ui, workflow: &mut Workflow, cx: WorkflowContext<'_>) -> Option<WorkflowAction> {
    let p = theme::palette(ui);
    let mut action = None;
    egui::Sides::new().height(46.0).shrink_left().show(
        ui,
        |ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new("Workflow").small().color(p.muted));
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
                    .on_hover_text("Workflow name");
                    if cx.dirty {
                        theme::badge(ui, "Unsaved", p.redirect);
                    }
                });
            });
        },
        |ui| {
            if cx.running {
                if ui.add(theme::danger_button(ui, "Cancel")).clicked() {
                    action = Some(WorkflowAction::Cancel);
                }
            } else if ui
                .add_enabled(!workflow.steps.is_empty(), theme::primary_button(ui, "Run"))
                .on_hover_text(
                    "Run the steps in order with the active environment (unsaved changes included)",
                )
                .clicked()
            {
                action = Some(WorkflowAction::Run);
            }
            if ui
                .add_enabled(cx.dirty && !cx.read_only, egui::Button::new("Save"))
                .on_hover_text("Save (Ctrl+S)")
                .clicked()
            {
                action = Some(WorkflowAction::Save);
            }
            if cx.dirty && ui.button("Revert").clicked() {
                action = Some(WorkflowAction::Revert);
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
                            RichText::new(format!("⚠ {} is not in the project", step.request))
                                .color(p.fail),
                        );
                    }
                }
                let mut extras = Vec::new();
                if !step.capture.is_empty() {
                    extras.push(crate::format::count(step.capture.len(), "capture"));
                }
                if !step.tests.trim().is_empty() {
                    extras.push("tests".to_owned());
                }
                if !extras.is_empty() {
                    ui.label(RichText::new(extras.join(" · ")).small().color(p.muted));
                }
            },
            |ui| {
                if !cx.read_only {
                    if ui
                        .add(theme::icon_button(ui, "🗙"))
                        .on_hover_text("Remove this step")
                        .clicked()
                    {
                        *change = Some(StepChange::Remove(i));
                    }
                    if ui
                        .add(theme::icon_button(ui, "↓"))
                        .on_hover_text("Move down")
                        .clicked()
                    {
                        *change = Some(StepChange::Down(i));
                    }
                    if ui
                        .add(theme::icon_button(ui, "↑"))
                        .on_hover_text("Move up")
                        .clicked()
                    {
                        *change = Some(StepChange::Up(i));
                    }
                }
                if request.is_some()
                    && ui
                        .small_button("Open")
                        .on_hover_text("Go to the request")
                        .clicked()
                {
                    action = Some(WorkflowAction::OpenRequest(step.request_path()));
                }
                let label = if open { "Close" } else { "Checks…" };
                if ui
                    .small_button(label)
                    .on_hover_text("Captures and tests for this step, on top of the request's own")
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
                    let label = ui.label("Step name");
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
                theme::section_label(ui, "Captures");
                if key_value_table(ui, "step-captures", captures, "Variable", &[]) {
                    step.capture = captures
                        .iter()
                        .filter(|c| c.is_active())
                        .map(|c| (c.name.trim().to_owned(), c.value.trim().to_owned()))
                        .collect();
                }
                theme::hint(
                    ui,
                    "$.json.path, xpath://xml/path, header:Name or status; available as {{variable}} in later \
                     steps.",
                );
                ui.add_space(4.0);
                super::script_editor::script_heading(ui, "Tests", &mut step.tests);
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
                theme::hint(ui, "Runs after the request's own tests.");
            });
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
        .selected_text("+ Add step…")
        .width(320.0)
        .height(400.0)
        .show_ui(ui, |ui| {
            if requests.is_empty() {
                theme::hint(ui, "This project has no requests yet.");
            }
            for r in requests {
                let text = format!("{}  {}  {}", theme::method_short(r.method), r.name, r.place);
                if ui.selectable_label(false, text).clicked() {
                    picked = Some(&r.path);
                }
            }
        })
        .response
        .on_hover_text("Add a request of this project as the next step");
    if let Some(path) = picked {
        workflow.steps.push(Step::new(path));
        st.sync(workflow);
    }
}

/// The last run: per iteration and step, with details of the chosen step (R-FLOW-3).
pub fn workflow_results(ui: &mut Ui, run: Option<&RunView>, st: &mut WorkflowEditorState) {
    let p = theme::palette(ui);
    let Some(run) = run else {
        ui.vertical_centered(|ui| {
            ui.add_space(30.0);
            ui.label(RichText::new("⏵").size(28.0).color(p.border));
            ui.label(theme::semibold("Not run yet").color(p.muted));
            theme::hint(ui, "Run the workflow to see each step's result here.");
        });
        return;
    };
    summary(ui, run);
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
                        ui.label(theme::semibold(format!("Row {}", n + 1)));
                        ui.add(Label::new(RichText::new(row).small().color(p.muted)).truncate());
                    });
                }
                for (i, step) in iteration.steps.iter().enumerate() {
                    if step_result_row(ui, step, i, st.selected_result == Some((n, i)), multi) {
                        st.selected_result = Some((n, i));
                    }
                }
                if iteration.skipped > 0 {
                    theme::hint(
                        ui,
                        format!(
                            "{} skipped after the failure.",
                            crate::format::count(iteration.skipped, "later step")
                        ),
                    );
                }
                match iteration.ended {
                    Some(end @ FlowEnd::Stopped) => theme::hint(ui, end.message()),
                    Some(end @ FlowEnd::TooManySteps) => theme::error(ui, end.message()),
                    None => {}
                }
            }
            if let Some((n, i)) = run.current {
                ui.horizontal(|ui| {
                    ui.spinner();
                    let of = run.steps_per_iteration;
                    let label = if multi {
                        format!("Row {}, step {} of {of} …", n + 1, i + 1)
                    } else {
                        format!("Step {} of {of} …", i + 1)
                    };
                    ui.label(RichText::new(label).color(p.muted));
                });
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
            ui.label(theme::semibold("Running…"));
        } else if run.cancelled {
            theme::badge(ui, "Cancelled", p.muted);
        } else if run.passed() {
            theme::badge(ui, "Passed", p.ok);
        } else {
            theme::badge(ui, "Failed", p.fail);
        }
        let mut parts = vec![
            format!("{} passed", crate::format::count(passed, "test")),
            format!("{failed} failed"),
        ];
        if failed_steps > 0 {
            parts.push(format!(
                "{} failed",
                crate::format::count(failed_steps, "step")
            ));
        }
        if run.iterations_planned > 1 {
            parts.push(format!(
                "{} of {} rows",
                run.iterations.len(),
                run.iterations_planned
            ));
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
    if !step.tests.tests.is_empty() {
        let passed = step.tests.tests.iter().filter(|t| t.passed).count();
        detail.push(format!("tests {passed}/{}", step.tests.tests.len()));
    }
    let (glyph, color) = if step.failure.is_some() {
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
                theme::section_label(ui, "Console");
                super::console_view::console_view(ui, &step.tests.console);
            }
            if let Some(body) = &step.body {
                ui.add_space(8.0);
                theme::section_label(ui, "Response body");
                if body.is_empty() {
                    theme::hint(ui, "No body.");
                } else {
                    ui.add(Label::new(RichText::new(body).monospace()).wrap());
                }
            }
        });
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
                    dirty: true,
                    running: false,
                    can_pick_files: false,
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
                dirty: false,
                running: true,
                can_pick_files: false,
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
                    name: "Get user".into(),
                    failure: Some("test “found” failed: Expected 404 to be 200".into()),
                    status: Some((404, "Not Found".into())),
                    elapsed_ms: Some(12),
                    tests,
                    sent: Some("GET https://api.test/users/bob".into()),
                    body: Some(String::new()),
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
                        name: "Log in".into(),
                        failure: None,
                        status: Some((200, "OK".into())),
                        elapsed_ms: Some(3),
                        tests: TestsView::default(),
                        sent: None,
                        body: None,
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
}
