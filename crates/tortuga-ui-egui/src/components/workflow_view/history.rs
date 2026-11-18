//! The kept runs of a workflow (R-FLOW-14): what each finished run showed, without
//! bodies, so a failing run can be compared with one that passed.

use std::collections::BTreeMap;
use std::time::Duration;

use egui::{ComboBox, Ui};
use serde::{Deserialize, Serialize};
use tortuga_core::script::TestResult;
use tortuga_core::workflow::{FlowEnd, Tries};

use super::{IterationView, RunView, StepView, TestsView};

/// How many runs of a workflow are kept.
pub const KEPT_RUNS: usize = 10;

/// A finished run, as kept on this computer (R-FLOW-14).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunRecord {
    /// When it finished, in milliseconds since 1970 (UTC).
    pub finished_ms: u64,
    #[serde(default)]
    pub by_rows: bool,
    #[serde(default)]
    pub iterations_planned: usize,
    #[serde(default)]
    pub steps_per_iteration: usize,
    #[serde(default)]
    pub iterations: Vec<IterationRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IterationRecord {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row: Option<BTreeMap<String, String>>,
    #[serde(default)]
    pub skipped: usize,
    /// [`FlowEnd::id`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended: Option<String>,
    #[serde(default)]
    pub steps: Vec<StepRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepRecord {
    pub index: usize,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<(u16, String)>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elapsed_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tests: Vec<TestResult>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
    /// (count, milliseconds) of a repeating step (R-FLOW-10).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tries: Option<(u32, u64)>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skipped_by: Option<String>,
}

impl RunRecord {
    /// What `run` shows, finished at `finished_ms`.
    pub fn new(run: &RunView, finished_ms: u64) -> Self {
        Self {
            finished_ms,
            by_rows: run.by_rows,
            iterations_planned: run.iterations_planned,
            steps_per_iteration: run.steps_per_iteration,
            iterations: run
                .iterations
                .iter()
                .map(|it| IterationRecord {
                    row: it.row.clone(),
                    skipped: it.skipped,
                    ended: it.ended.map(|e| e.id().to_owned()),
                    steps: it.steps.iter().map(StepRecord::new).collect(),
                })
                .collect(),
        }
    }

    /// The run as the results show it: no request sent, no body, no console.
    pub fn view(&self) -> RunView {
        RunView {
            by_rows: self.by_rows,
            iterations_planned: self.iterations_planned,
            steps_per_iteration: self.steps_per_iteration,
            iterations: self
                .iterations
                .iter()
                .map(|it| IterationView {
                    row: it.row.clone(),
                    skipped: it.skipped,
                    ended: it.ended.as_deref().and_then(FlowEnd::from_id),
                    steps: it.steps.iter().map(StepRecord::view).collect(),
                })
                .collect(),
            ..RunView::default()
        }
    }

    fn passed(&self) -> bool {
        self.view().passed()
    }

    /// (tests passed, tests failed).
    fn tests(&self) -> (usize, usize) {
        let tests = self
            .iterations
            .iter()
            .flat_map(|i| &i.steps)
            .flat_map(|s| &s.tests);
        tests.fold(
            (0, 0),
            |(p, f), t| if t.passed { (p + 1, f) } else { (p, f + 1) },
        )
    }
}

impl StepRecord {
    fn new(step: &StepView) -> Self {
        Self {
            index: step.index,
            name: step.name.clone(),
            failure: step.failure.clone(),
            status: step.status.clone(),
            elapsed_ms: step.elapsed_ms,
            tests: step.tests.tests.clone(),
            errors: step.tests.errors.clone(),
            tries: step.tries.map(|t| {
                let ms = u64::try_from(t.elapsed.as_millis()).unwrap_or(u64::MAX);
                (t.count, ms)
            }),
            skipped_by: step.skipped_by.clone(),
        }
    }

    fn view(&self) -> StepView {
        StepView {
            index: self.index,
            name: self.name.clone(),
            failure: self.failure.clone(),
            status: self.status.clone(),
            elapsed_ms: self.elapsed_ms,
            tests: TestsView {
                tests: self.tests.clone(),
                errors: self.errors.clone(),
                ..TestsView::default()
            },
            sent: None,
            body: None,
            tries: self.tries.map(|(count, ms)| Tries {
                count,
                elapsed: Duration::from_millis(ms),
            }),
            skipped_by: self.skipped_by.clone(),
        }
    }
}

/// Which run the results show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunChoice {
    /// The run of this session (going on or last finished).
    Current,
    /// A kept run, by its place in the list (newest first).
    Kept(usize),
}

/// The **Runs** list above the results (R-FLOW-14). Returns the run chosen.
pub fn runs_picker(
    ui: &mut Ui,
    runs: &[RunRecord],
    shown: RunChoice,
    has_current: bool,
    utc_offset_minutes: i32,
) -> Option<RunChoice> {
    if runs.is_empty() {
        return None;
    }
    let label = |run: &RunRecord| {
        let (passed, failed) = run.tests();
        let verdict = if run.passed() {
            t!("passed")
        } else {
            t!("failed")
        };
        format!(
            "{} · {verdict} · {} · {}",
            crate::format::clock_seconds(run.finished_ms, utc_offset_minutes),
            t!("workflow-tests-passed", count = passed),
            t!("workflow-tests-failed", count = failed)
        )
    };
    let selected = match shown {
        RunChoice::Kept(i) => runs
            .get(i)
            .map_or_else(|| t!("workflow-run-current"), label),
        RunChoice::Current => t!("workflow-run-current"),
    };
    let mut chosen = None;
    ui.horizontal(|ui| {
        let caption = ui.label(t!("workflow-runs"));
        ComboBox::from_id_salt("workflow-runs")
            .selected_text(selected)
            .width(360.0)
            .show_ui(ui, |ui| {
                if has_current
                    && ui
                        .selectable_label(shown == RunChoice::Current, t!("workflow-run-current"))
                        .clicked()
                {
                    chosen = Some(RunChoice::Current);
                }
                for (i, run) in runs.iter().enumerate() {
                    if ui
                        .selectable_label(shown == RunChoice::Kept(i), label(run))
                        .clicked()
                    {
                        chosen = Some(RunChoice::Kept(i));
                    }
                }
            })
            .response
            .labelled_by(caption.id)
            .on_hover_text(t!("workflow-runs-hint"));
    });
    chosen
}
