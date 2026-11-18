//! Running a workflow (R-FLOW-3): the steps of each iteration in order, as scripts
//! direct them (R-FLOW-6), with repeats (R-FLOW-10), always-run steps (R-FLOW-11),
//! skip conditions (R-FLOW-12) and runs that start at a step (R-FLOW-13).

use std::collections::BTreeMap;
use std::time::Duration;

use web_time::Instant;

use super::{
    FlowEnd, Iteration, MAX_STEPS, Progress, Repeat, RunOptions, RunResult, Start, Step,
    StepResult, Tries, Workflow, step_names,
};
use crate::collection::Collection;
use crate::cookies::CookieJar;
use crate::runner::{self, Extra, StepOutcome};
use crate::script::{self, FlowControl, ScriptInfo};
use crate::{Client, vars};

/// What every step of a run shares.
struct Run<'a> {
    client: &'a Client,
    collection: &'a Collection,
    workflow: &'a Workflow,
    options: &'a RunOptions,
    names: Vec<String>,
    iterations: usize,
    /// Values carried across iterations when the workflow keeps them (R-FLOW-9).
    kept: BTreeMap<String, String>,
}

/// One iteration while it runs.
struct IterationRun<'a> {
    n: usize,
    row: Option<&'a BTreeMap<String, String>>,
    jar: CookieJar,
    /// What steps captured and scripts set in this iteration.
    captured: BTreeMap<String, String>,
}

/// How an iteration ended before its last step.
#[derive(Clone, Copy, PartialEq, Eq)]
enum EarlyEnd {
    /// A failed step; the steps after it are skipped (R-FLOW-3).
    Failed,
    /// `workflow.stop()` (R-FLOW-6).
    Stopped,
}

/// Runs `workflow` (R-FLOW-3, R-FLOW-4): once per data row, else `iterations` times
/// (R-FLOW-8); each iteration starts with no captured values and an empty cookie jar,
/// apart from the variables a workflow keeps (R-FLOW-9) and the values a run that
/// starts at a step is given (R-FLOW-13); a failed step ends the iteration unless the
/// workflow continues after failures, and always-run steps still run (R-FLOW-11).
/// `delay_ms` passes between steps. A step that repeats is sent until a try passes
/// (R-FLOW-10); a step whose skip condition holds is not sent (R-FLOW-12).
pub async fn run(
    client: &Client,
    collection: &Collection,
    workflow: &Workflow,
    options: &RunOptions,
    mut progress: impl FnMut(Progress<'_>) + Send,
) -> RunResult {
    let rows: Vec<Option<&BTreeMap<String, String>>> = if options.rows.is_empty() {
        vec![None; workflow.iterations.max(1) as usize]
    } else {
        options.rows.iter().map(Some).collect()
    };
    let mut run = Run {
        client,
        collection,
        workflow,
        options,
        names: step_names(collection, workflow),
        iterations: rows.len(),
        kept: if workflow.keep_variables {
            options.runtime.clone()
        } else {
            BTreeMap::new()
        },
    };
    let mut result = RunResult::default();
    for (n, row) in rows.into_iter().enumerate() {
        let iteration = run.iteration(n, row, &mut progress).await;
        result.iterations.push(iteration);
    }
    result
}

impl Run<'_> {
    async fn iteration(
        &mut self,
        n: usize,
        row: Option<&BTreeMap<String, String>>,
        progress: &mut (impl FnMut(Progress<'_>) + Send),
    ) -> Iteration {
        let (first, only_one) = match self.options.start {
            Start::First => (0, false),
            Start::From(i) => (i, false),
            Start::Only(i) => (i, true),
        };
        let mut it = IterationRun {
            n,
            row,
            jar: CookieJar::default(),
            captured: self.options.start_values(n),
        };
        let mut iteration = Iteration {
            row: row.cloned(),
            ..Iteration::default()
        };
        let mut i = first;
        let mut early = None;
        while i < self.workflow.steps.len() {
            if iteration.steps.len() == MAX_STEPS {
                iteration.ended = Some(FlowEnd::TooManySteps);
                break;
            }
            self.wait_between_steps(n, &iteration).await;
            let (done, next) = self.step(&mut it, i, progress).await;
            let failed = done.failed();
            iteration.steps.push(done);
            if only_one {
                break;
            }
            if failed && !self.workflow.continue_on_failure {
                early = Some((i, EarlyEnd::Failed));
                break;
            }
            let Some(next) = next else {
                if !failed {
                    iteration.ended = Some(FlowEnd::Stopped);
                }
                early = Some((i, EarlyEnd::Stopped));
                break;
            };
            i = next;
        }
        if let Some((at, end)) = early {
            self.finish_early(&mut it, &mut iteration, at, end, progress)
                .await;
        }
        iteration
    }

    /// After an iteration ended at step `at`: the always-run steps after it run
    /// (R-FLOW-11); after a failure, the others are counted as skipped (R-FLOW-3).
    async fn finish_early(
        &mut self,
        it: &mut IterationRun<'_>,
        iteration: &mut Iteration,
        at: usize,
        end: EarlyEnd,
        progress: &mut (impl FnMut(Progress<'_>) + Send),
    ) {
        for j in at + 1..self.workflow.steps.len() {
            if self.workflow.steps[j].always {
                self.wait_between_steps(it.n, iteration).await;
                // Their jumps and stops change nothing: the iteration has ended.
                let (done, _) = self.step(it, j, progress).await;
                iteration.steps.push(done);
            } else if end == EarlyEnd::Failed {
                iteration.skipped.push(j);
            }
        }
    }

    /// `delay_ms` between steps, also between iterations, not before the first
    /// (R-FLOW-8).
    async fn wait_between_steps(&self, n: usize, iteration: &Iteration) {
        let delay = Duration::from_millis(self.workflow.delay_ms);
        if !delay.is_zero() && (n, iteration.steps.len()) != (0, 0) {
            pause(delay).await;
        }
    }

    /// Runs step `i`: skipped when its condition holds (R-FLOW-12), else sent once or
    /// until a try passes (R-FLOW-10). Returns its result and the step to run next.
    async fn step(
        &mut self,
        it: &mut IterationRun<'_>,
        i: usize,
        progress: &mut (impl FnMut(Progress<'_>) + Send),
    ) -> (StepResult, Option<usize>) {
        let workflow = self.workflow;
        let step = &workflow.steps[i];
        let info = ScriptInfo {
            iteration: it.n,
            iterations: self.iterations,
        };
        let mut result = StepResult {
            index: i,
            name: self.names[i].clone(),
            request: step.request.clone(),
            outcome: StepOutcome::default(),
            tries: None,
            skipped_by: None,
        };
        let condition = step.skip_when.trim();
        if !condition.is_empty() {
            progress(Progress::Started {
                iteration: it.n,
                step: i,
                attempt: 1,
            });
            let variables = self.variables(it);
            match self.skip(step, condition, variables, info) {
                Ok(true) => {
                    result.skipped_by = Some(condition.to_owned());
                    progress(Progress::Finished {
                        iteration: it.n,
                        step: &result,
                    });
                    return (result, Some(i + 1));
                }
                Ok(false) => {}
                Err(e) => {
                    result.outcome.errors.push(format!("Skip when: {e}"));
                    progress(Progress::Finished {
                        iteration: it.n,
                        step: &result,
                    });
                    return (result, Some(i + 1));
                }
            }
        }
        let (outcome, tries) = self.send(it, i, step, info, progress).await;
        result.outcome = outcome;
        result.tries = tries;
        let next = next_step(&mut result.outcome, &self.names, i);
        progress(Progress::Finished {
            iteration: it.n,
            step: &result,
        });
        (result, next)
    }

    /// Sends step `i`, again until a try passes when it repeats (R-FLOW-10). Every
    /// try's variable changes are applied and kept in the outcome.
    async fn send(
        &mut self,
        it: &mut IterationRun<'_>,
        i: usize,
        step: &Step,
        info: ScriptInfo,
        progress: &mut (impl FnMut(Progress<'_>) + Send),
    ) -> (StepOutcome, Option<Tries>) {
        let mut changes = BTreeMap::new();
        let started = Instant::now();
        let repeat = step.repeat.filter(|r| r.times > 1);
        let mut attempt = 1;
        let mut outcome = loop {
            progress(Progress::Started {
                iteration: it.n,
                step: i,
                attempt,
            });
            let at = StepRun {
                variables: self.variables(it),
                jar: &it.jar,
                info,
            };
            let mut outcome = run_one(self.client, self.collection, step, self.options, at).await;
            runner::apply_changes(&mut it.captured, &outcome.vars);
            if self.workflow.keep_variables {
                runner::apply_changes(&mut self.kept, &outcome.vars);
            }
            changes.extend(outcome.vars.clone());
            if !another_try(repeat, attempt, &mut outcome) {
                break outcome;
            }
            attempt += 1;
            if let Some(wait) = repeat.map(|r| r.wait_ms).filter(|ms| *ms > 0) {
                pause(Duration::from_millis(wait)).await;
            }
        };
        // Every try's changes, so a run that keeps variables loses none (R-FLOW-9).
        outcome.vars = changes;
        let tries = repeat.map(|_| Tries {
            count: attempt,
            elapsed: started.elapsed(),
        });
        (outcome, tries)
    }

    /// A step's variables on top of the project's: values kept from earlier
    /// iterations (R-FLOW-9) below the data row; this iteration's own above it.
    fn variables(&self, it: &IterationRun<'_>) -> BTreeMap<String, String> {
        let mut variables = self.kept.clone();
        variables.extend(self.options.overrides.clone());
        variables.extend(it.row.cloned().unwrap_or_default());
        variables.extend(it.captured.clone());
        variables
    }

    /// Whether `step`'s skip condition holds (R-FLOW-12), with the variables the
    /// step would get.
    fn skip(
        &self,
        step: &Step,
        condition: &str,
        variables: BTreeMap<String, String>,
        info: ScriptInfo,
    ) -> Result<bool, String> {
        let path = step.request_path();
        let Some(item) = self.collection.request(&path) else {
            return Err(format!(
                "The request {} is not in the project.",
                step.request
            ));
        };
        let ctx = step_context(self.collection, &path, self.options, variables);
        script::condition(condition, &ctx.variables, &item.request, info)
    }
}

/// Whether a repeating step is sent again after a try with `outcome` (R-FLOW-10): not
/// once a try passed or a script chose what runs next. When the last try fails, its
/// outcome says that no try passed.
fn another_try(repeat: Option<Repeat>, attempt: u32, outcome: &mut StepOutcome) -> bool {
    let Some(repeat) = repeat else {
        return false;
    };
    if !outcome.failed() || outcome.flow.is_some() {
        return false;
    }
    if attempt >= repeat.times {
        outcome
            .errors
            .push(format!("No try passed in {} tries.", repeat.times));
        return false;
    }
    true
}

/// Waits between steps (R-FLOW-8) and between a repeating step's tries (R-FLOW-10).
#[cfg(not(target_arch = "wasm32"))]
pub(super) async fn pause(delay: Duration) {
    tokio::time::sleep(delay).await;
}

/// The web version has no timer in the engine yet (D6): no wait.
#[cfg(target_arch = "wasm32")]
pub(super) fn pause(_: Duration) -> std::future::Ready<()> {
    std::future::ready(())
}

/// Where a step runs: the variables it gets on top of the project's (overrides, the
/// data row, captured values), the iteration's cookie jar, and the iteration.
struct StepRun<'a> {
    variables: BTreeMap<String, String>,
    jar: &'a CookieJar,
    info: ScriptInfo,
}

/// The variables, secrets and defaults a step of the request at `path` runs with.
fn step_context(
    collection: &Collection,
    path: &std::path::Path,
    options: &RunOptions,
    variables: BTreeMap<String, String>,
) -> vars::Context {
    let mut ctx =
        vars::Context::for_request(collection, options.environment.as_deref(), Some(path))
            .with_workspace_defaults(&options.workspace_defaults)
            .with_secrets(options.secrets.clone())
            .with_workspace(&options.workspace, &options.secret_names);
    ctx.secrets.extend(options.secret_names.iter().cloned());
    ctx.variables.extend(variables);
    ctx
}

/// Runs one step.
async fn run_one(
    client: &Client,
    collection: &Collection,
    step: &Step,
    options: &RunOptions,
    at: StepRun<'_>,
) -> StepOutcome {
    let path = step.request_path();
    let Some(item) = collection.request(&path) else {
        return StepOutcome {
            errors: vec![format!(
                "The request {} is not in the project.",
                step.request
            )],
            ..StepOutcome::default()
        };
    };
    let ctx = step_context(collection, &path, options, at.variables);
    let extra = Extra {
        capture: Some(&step.capture),
        tests: Some(step.tests.as_str()),
        breakpoints: None,
        check_examples: options.check_examples,
        info: at.info,
    };
    runner::run_step(client, Some(at.jar), &item.request, &ctx, extra).await
}

/// The index of the step to run after step `i`, by what its scripts asked (R-FLOW-6);
/// `None` ends the iteration. A step name the workflow doesn't have is an error of the
/// step.
fn next_step(outcome: &mut StepOutcome, names: &[String], i: usize) -> Option<usize> {
    let Some(flow) = &outcome.flow else {
        return Some(i + 1);
    };
    let name = match flow {
        FlowControl::Stop => return None,
        FlowControl::Next(name) => name,
    };
    let found = names.iter().position(|n| n == name);
    if found.is_none() {
        outcome.errors.push(format!(
            "workflow.next(\"{name}\"): the workflow has no step called “{name}”."
        ));
    }
    found
}
