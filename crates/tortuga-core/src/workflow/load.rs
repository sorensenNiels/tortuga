//! Load tests (R-FLOW-15, D87): a workflow run by many virtual users at once, each
//! repeating its steps, with response times per step. Responses are not kept.

use std::sync::Mutex;
use std::time::Duration;

use futures_util::future::join_all;
use web_time::Instant;

use super::execute::pause;
use super::{RunOptions, RunResult, Workflow, run};
use crate::Client;
use crate::collection::Collection;

/// Most virtual users at once.
pub const MAX_USERS: u32 = 100;

/// How a load test runs (R-FLOW-15).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadOptions {
    /// Virtual users at once, 1 to [`MAX_USERS`].
    pub users: u32,
    pub limit: LoadLimit,
    /// Until every user runs; users start evenly spread over it.
    pub ramp_up: Duration,
}

/// When a load test ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadLimit {
    /// Users start no new iteration after this long.
    Duration(Duration),
    /// Each user runs the steps this often.
    Iterations(u32),
}

/// What a load test measured so far, or in all.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LoadReport {
    /// Per step of the workflow, in its order.
    pub steps: Vec<StepStats>,
    /// Iterations finished, by all users together.
    pub iterations: usize,
    /// From the start until the last iteration finished.
    pub elapsed: Duration,
}

/// The sends of one step (R-FLOW-15).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StepStats {
    pub name: String,
    pub sends: usize,
    /// Sends that failed as a step does (R-FLOW-3).
    pub failures: usize,
    /// Response times in milliseconds, of the sends that got a response.
    times: Vec<u64>,
}

impl StepStats {
    /// The median response time, in milliseconds.
    pub fn median_ms(&self) -> Option<u64> {
        self.percentile(50)
    }

    /// The time 95 % of the responses took at most, in milliseconds.
    pub fn p95_ms(&self) -> Option<u64> {
        self.percentile(95)
    }

    pub fn max_ms(&self) -> Option<u64> {
        self.times.iter().copied().max()
    }

    /// Nearest-rank percentile `p` (1 to 100) of the response times.
    fn percentile(&self, p: usize) -> Option<u64> {
        let mut sorted = self.times.clone();
        sorted.sort_unstable();
        let rank = (sorted.len() * p).div_ceil(100).max(1);
        sorted.get(rank - 1).copied()
    }
}

impl LoadReport {
    fn new(workflow: &Workflow, collection: &Collection) -> Self {
        Self {
            steps: super::step_names(collection, workflow)
                .into_iter()
                .map(|name| StepStats {
                    name,
                    ..StepStats::default()
                })
                .collect(),
            ..Self::default()
        }
    }

    /// All sends, and those that failed.
    pub fn sends(&self) -> (usize, usize) {
        self.steps
            .iter()
            .fold((0, 0), |(s, f), step| (s + step.sends, f + step.failures))
    }

    /// Sends per second over the elapsed time.
    pub fn sends_per_second(&self) -> f64 {
        let secs = self.elapsed.as_secs_f64();
        if secs <= 0.0 {
            return 0.0;
        }
        #[expect(
            clippy::cast_precision_loss,
            reason = "a count of sends is far below 2^52"
        )]
        let sends = self.sends().0 as f64;
        sends / secs
    }

    /// Adds what one iteration of a user sent.
    fn add(&mut self, result: &RunResult) {
        self.iterations += result.iterations.len();
        for step in result.iterations.iter().flat_map(|i| &i.steps) {
            if step.skipped_by.is_some() {
                continue;
            }
            let Some(stats) = self.steps.get_mut(step.index) else {
                continue;
            };
            stats.sends += 1;
            stats.failures += usize::from(step.failed());
            if let Some(Ok(response)) = &step.outcome.response {
                stats
                    .times
                    .push(u64::try_from(response.elapsed.as_millis()).unwrap_or(u64::MAX));
            }
        }
    }
}

/// Runs `workflow` as a load test (R-FLOW-15): `load.users` users at once, each
/// running its steps again and again as one iteration of [`run`] (the data file's
/// rows taken in turn, its own cookies and values), until `load.limit`. `progress`
/// gets the report after every iteration of every user.
pub async fn load(
    client: &Client,
    collection: &Collection,
    workflow: &Workflow,
    options: &RunOptions,
    load: LoadOptions,
    progress: impl FnMut(&LoadReport) + Send,
) -> LoadReport {
    let users = load.users.clamp(1, MAX_USERS);
    let started = Instant::now();
    let report = Mutex::new(LoadReport::new(workflow, collection));
    let progress = Mutex::new(progress);
    // One iteration per run; the rows decide what each one gets.
    let once = Workflow {
        iterations: 1,
        ..workflow.clone()
    };
    let user = |u: u32| {
        let (report, progress, once) = (&report, &progress, &once);
        async move {
            let delay = load.ramp_up.mul_f64(f64::from(u) / f64::from(users));
            if !delay.is_zero() {
                pause(delay).await;
            }
            let mut n: usize = 0;
            loop {
                let more = match load.limit {
                    LoadLimit::Duration(d) => started.elapsed() < d,
                    LoadLimit::Iterations(max) => n < max as usize,
                };
                if !more {
                    break;
                }
                let row = options
                    .rows
                    .get((n * users as usize + u as usize) % options.rows.len().max(1));
                let this = RunOptions {
                    rows: row.cloned().into_iter().collect(),
                    ..options.clone()
                };
                let result = run(client, collection, once, &this, |_| {}).await;
                n += 1;
                let snapshot = {
                    let mut report = lock(report);
                    report.add(&result);
                    report.elapsed = started.elapsed();
                    report.clone()
                };
                (lock(progress))(&snapshot);
            }
        }
    };
    join_all((0..users).map(user)).await;
    let mut report = report
        .into_inner()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    report.elapsed = started.elapsed();
    report
}

/// The guard of `mutex`, also after a panic elsewhere (the data stays usable).
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats(times: &[u64]) -> StepStats {
        StepStats {
            times: times.to_vec(),
            ..StepStats::default()
        }
    }

    /// R-FLOW-15: median, p95 and max by nearest rank; none without responses.
    #[test]
    fn percentiles_by_nearest_rank() {
        let s = stats(&(1..=100).rev().collect::<Vec<_>>());
        assert_eq!(
            (s.median_ms(), s.p95_ms(), s.max_ms()),
            (Some(50), Some(95), Some(100))
        );
        let one = stats(&[7]);
        assert_eq!((one.median_ms(), one.p95_ms()), (Some(7), Some(7)));
        let none = stats(&[]);
        assert_eq!((none.median_ms(), none.max_ms()), (None, None));
    }
}
