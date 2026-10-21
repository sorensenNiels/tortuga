//! Workflows (R-FLOW-1..4, D4, D29): an ordered list of requests of a project, each
//! with optional extra captures and tests, run one after the other with the values
//! captured along the way. Stored as TOML files in the project's `workflows/` folder.
//! Scripts may change the order with `workflow.next(name)` and `workflow.stop()`
//! (R-FLOW-6, D50).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::collection::{Collection, Item};
use crate::cookies::CookieJar;
use crate::runner::{self, Extra, StepOutcome};
use crate::script::{FlowControl, ScriptInfo};
use crate::{Client, vars};

/// Steps one iteration may run, so a `workflow.next(…)` loop that never ends stops
/// (R-FLOW-6).
#[cfg(not(test))]
pub const MAX_STEPS: usize = 1000;
/// Fewer in this crate's tests, so the loop test sends few requests.
#[cfg(test)]
pub const MAX_STEPS: usize = 20;

/// Folder of workflow files in a project (D29).
pub const WORKFLOWS_DIR: &str = "workflows";

/// A workflow file (R-FLOW-1).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Workflow {
    /// The file stem; given once, so renaming keeps the file.
    #[serde(skip)]
    pub id: String,
    pub name: String,
    /// Run the next steps after a failed one (R-FLOW-3).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub continue_on_failure: bool,
    /// A CSV or JSON data file, relative to the project, to run once per row (R-FLOW-4).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<Step>,
}

/// One step: a request of the project, with optional extra captures and tests.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Step {
    /// Path of the request file in the project, with `/`.
    pub request: String,
    /// Shown instead of the request's name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Variable name → where to take it from in the response (R-TEST-4).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub capture: BTreeMap<String, String>,
    /// A test script for this step, after the request's own.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tests: String,
}

impl Step {
    pub fn new(request: &Path) -> Self {
        Self {
            request: path_text(request),
            ..Self::default()
        }
    }

    pub fn request_path(&self) -> PathBuf {
        PathBuf::from(&self.request)
    }
}

/// A request path as written in a workflow file: `/`-separated on every OS.
pub fn path_text(path: &Path) -> String {
    path.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

impl Workflow {
    /// A workflow that runs every request in `folder` (`""`: the whole project), in tree
    /// order, for the command-line runner (R-CLI-1).
    pub fn from_folder(collection: &Collection, folder: &Path) -> Self {
        fn collect(items: &[Item], out: &mut Vec<Step>) {
            for item in items {
                match item {
                    Item::Request(r) => out.push(Step::new(&r.path)),
                    Item::Folder(f) => collect(&f.items, out),
                    Item::Broken(_) => {}
                }
            }
        }
        let mut steps = Vec::new();
        collect(collection.children(folder), &mut steps);
        let name = if folder.as_os_str().is_empty() {
            collection.name().to_owned()
        } else {
            collection
                .find(folder)
                .map_or_else(|| path_text(folder), Item::name)
        };
        Self {
            name,
            continue_on_failure: true,
            steps,
            ..Self::default()
        }
    }

    /// A workflow of one request (R-CLI-1).
    pub fn single(collection: &Collection, request: &Path) -> Self {
        Self {
            name: collection
                .find(request)
                .map_or_else(|| path_text(request), Item::name),
            steps: vec![Step::new(request)],
            ..Self::default()
        }
    }

    /// Points steps at a request or folder that moved from `old` to `new`. Returns
    /// whether anything changed.
    pub fn retarget(&mut self, old: &Path, new: &Path) -> bool {
        let mut changed = false;
        for step in &mut self.steps {
            if let Ok(rest) = step.request_path().strip_prefix(old) {
                step.request = path_text(&new.join(rest));
                changed = true;
            }
        }
        changed
    }
}

/// What a run starts from (R-FLOW-3, R-FLOW-4, R-CLI-2).
#[derive(Debug, Clone, Default)]
pub struct RunOptions {
    /// The active environment's id.
    pub environment: Option<String>,
    /// This machine's secret values for that environment.
    pub secrets: BTreeMap<String, String>,
    /// Values given for this run (`--var`, `TORTUGA_VAR_*`); they win over the project's.
    pub overrides: BTreeMap<String, String>,
    /// The workspace's variables (R-WS-8); every value of the project wins over them.
    pub workspace: BTreeMap<String, String>,
    /// The workspace's settings, headers and auth (R-SET-1, R-SET-2), below the project's.
    pub workspace_defaults: crate::workspace::WorkspaceDefaults,
    /// One iteration per row; none means one iteration without a row.
    pub rows: Vec<BTreeMap<String, String>>,
    /// Compare each response with its request's examples (R-MOCK-8, `--check-examples`).
    pub check_examples: bool,
}

/// A step that ran (or could not).
#[derive(Debug, Clone)]
pub struct StepResult {
    /// Its place in the workflow's steps (a step may run more than once, R-FLOW-6).
    pub index: usize,
    /// The step's name, or its request's.
    pub name: String,
    pub request: String,
    pub outcome: StepOutcome,
}

/// One pass through the steps.
#[derive(Debug, Clone, Default)]
pub struct Iteration {
    /// The data row, for data-driven runs.
    pub row: Option<BTreeMap<String, String>>,
    pub steps: Vec<StepResult>,
    /// Steps not run because an earlier one failed: the ones after it.
    pub skipped: usize,
    /// How a script ended the iteration early (R-FLOW-6).
    pub ended: Option<FlowEnd>,
}

/// Why an iteration ended before its last step without a failure (R-FLOW-6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlowEnd {
    /// A script called `workflow.stop()`.
    Stopped,
    /// [`MAX_STEPS`] steps ran: a `workflow.next(…)` loop that never ends. A failure.
    TooManySteps,
}

impl FlowEnd {
    /// What happened, as a sentence for the user.
    pub fn message(self) -> String {
        match self {
            Self::Stopped => "A script ended this iteration with workflow.stop().".to_owned(),
            Self::TooManySteps => format!(
                "Stopped after {MAX_STEPS} steps: workflow.next(…) keeps going round (a \
                 loop that never ends?)."
            ),
        }
    }

    /// For the JSON report: `stopped` or `too_many_steps`.
    pub fn id(self) -> &'static str {
        match self {
            Self::Stopped => "stopped",
            Self::TooManySteps => "too_many_steps",
        }
    }
}

impl Iteration {
    pub fn failed(&self) -> bool {
        self.skipped > 0
            || self.ended == Some(FlowEnd::TooManySteps)
            || self.steps.iter().any(|s| s.outcome.failed())
    }
}

/// The whole run.
#[derive(Debug, Clone, Default)]
pub struct RunResult {
    pub iterations: Vec<Iteration>,
}

impl RunResult {
    pub fn passed(&self) -> bool {
        !self.iterations.iter().any(Iteration::failed)
    }

    /// (tests passed, tests failed, steps failed).
    pub fn counts(&self) -> (usize, usize, usize) {
        let steps = self.iterations.iter().flat_map(|i| &i.steps);
        steps.fold((0, 0, 0), |(p, f, s), step| {
            let failed = step.outcome.failed_tests();
            (
                p + step.outcome.tests.len() - failed,
                f + failed,
                s + usize::from(step.outcome.failed()),
            )
        })
    }
}

/// Progress while a workflow runs, for live result views.
#[derive(Debug, Clone, Copy)]
pub enum Progress<'a> {
    Started {
        iteration: usize,
        step: usize,
    },
    Finished {
        iteration: usize,
        step: &'a StepResult,
    },
}

/// Runs `workflow` (R-FLOW-3, R-FLOW-4): each iteration starts with no captured values
/// and an empty cookie jar; a failed step ends the iteration unless the workflow
/// continues after failures.
pub async fn run(
    client: &Client,
    collection: &Collection,
    workflow: &Workflow,
    options: &RunOptions,
    mut progress: impl FnMut(Progress<'_>) + Send,
) -> RunResult {
    let rows: Vec<Option<&BTreeMap<String, String>>> = if options.rows.is_empty() {
        vec![None]
    } else {
        options.rows.iter().map(Some).collect()
    };
    let names = step_names(collection, workflow);
    let iterations = rows.len();
    let mut result = RunResult::default();
    for (n, row) in rows.into_iter().enumerate() {
        let jar = CookieJar::default();
        let mut captured: BTreeMap<String, String> = BTreeMap::new();
        let mut iteration = Iteration {
            row: row.cloned(),
            ..Iteration::default()
        };
        let mut i = 0;
        while let Some(step) = workflow.steps.get(i) {
            if iteration.steps.len() == MAX_STEPS {
                iteration.ended = Some(FlowEnd::TooManySteps);
                break;
            }
            progress(Progress::Started {
                iteration: n,
                step: i,
            });
            let mut variables = options.overrides.clone();
            variables.extend(row.cloned().unwrap_or_default());
            variables.extend(captured.clone());
            let at = StepRun {
                variables,
                jar: &jar,
                info: ScriptInfo {
                    iteration: n,
                    iterations,
                },
            };
            let mut outcome = run_one(client, collection, step, options, at).await;
            runner::apply_changes(&mut captured, &outcome.vars);
            let next = next_step(&mut outcome, &names, i);
            let done = StepResult {
                index: i,
                name: names[i].clone(),
                request: step.request.clone(),
                outcome,
            };
            progress(Progress::Finished {
                iteration: n,
                step: &done,
            });
            let failed = done.outcome.failed();
            iteration.steps.push(done);
            if failed && !workflow.continue_on_failure {
                iteration.skipped = workflow.steps.len() - i - 1;
                break;
            }
            let Some(next) = next else {
                if !failed {
                    iteration.ended = Some(FlowEnd::Stopped);
                }
                break;
            };
            i = next;
        }
        result.iterations.push(iteration);
    }
    result
}

/// Where a step runs: the variables it gets on top of the project's (overrides, the
/// data row, captured values), the iteration's cookie jar, and the iteration.
struct StepRun<'a> {
    variables: BTreeMap<String, String>,
    jar: &'a CookieJar,
    info: ScriptInfo,
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
    let mut ctx =
        vars::Context::for_request(collection, options.environment.as_deref(), Some(&path))
            .with_workspace_defaults(&options.workspace_defaults)
            .with_secrets(options.secrets.clone())
            .with_workspace(&options.workspace, &std::collections::BTreeSet::new());
    ctx.variables.extend(at.variables);
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

/// What each step is called in a run and by `workflow.next(…)`: its own name, else its
/// request's, else the request's path.
fn step_names(collection: &Collection, workflow: &Workflow) -> Vec<String> {
    workflow
        .steps
        .iter()
        .map(|step| {
            step.name.clone().unwrap_or_else(|| {
                collection
                    .request(&step.request_path())
                    .map_or_else(|| step.request.clone(), |r| r.request.name.clone())
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workflow_file_round_trips() {
        let mut capture = BTreeMap::new();
        capture.insert("token".to_owned(), "$.access_token".to_owned());
        let wf = Workflow {
            id: String::new(),
            name: "User lifecycle".into(),
            continue_on_failure: true,
            data: Some("data/users.csv".into()),
            steps: vec![
                Step {
                    request: "auth/login.toml".into(),
                    capture,
                    ..Step::default()
                },
                Step {
                    request: "users/create-user.toml".into(),
                    name: Some("Create".into()),
                    tests: "test(\"created\", () => expect(res.status).toBe(201));\n".into(),
                    ..Step::default()
                },
            ],
        };
        let text = crate::format::write_workflow(&wf);
        assert_eq!(
            text,
            r#"name = "User lifecycle"
continue_on_failure = true
data = "data/users.csv"

[[steps]]
request = "auth/login.toml"

[steps.capture]
token = "$.access_token"

[[steps]]
request = "users/create-user.toml"
name = "Create"
tests = """
test("created", () => expect(res.status).toBe(201));
"""
"#
        );
        let back = crate::format::parse_workflow(&text, "lifecycle").expect("parse");
        assert_eq!(back.id, "lifecycle");
        assert_eq!(
            Workflow {
                id: String::new(),
                ..back
            },
            wf
        );
    }

    #[test]
    fn steps_follow_moved_requests() {
        let mut wf = Workflow {
            steps: vec![
                Step::new(Path::new("users/get.toml")),
                Step::new(Path::new("users/admin/list.toml")),
                Step::new(Path::new("orders/get.toml")),
            ],
            ..Workflow::default()
        };
        assert!(wf.retarget(Path::new("users"), Path::new("people")));
        let paths: Vec<&str> = wf.steps.iter().map(|s| s.request.as_str()).collect();
        assert_eq!(
            paths,
            [
                "people/get.toml",
                "people/admin/list.toml",
                "orders/get.toml"
            ]
        );
        assert!(!wf.retarget(Path::new("nothing"), Path::new("x")));
    }

    /// R-FLOW-6: a `workflow.next(…)` loop that never ends stops after `MAX_STEPS`
    /// steps and fails.
    #[tokio::test]
    async fn an_endless_loop_is_stopped() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .respond_with(wiremock::ResponseTemplate::new(200))
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().expect("temp dir");
        let storage = crate::storage::FsStorage;
        let col = Collection::create(&storage, dir.path(), "Loop").expect("project");
        let mut ping = crate::Request::new(crate::HttpMethod::Get, server.uri());
        ping.name = "Ping".into();
        col.new_request(&storage, Path::new(""), &ping)
            .expect("request");
        let col = Collection::open(&storage, dir.path()).expect("open");
        let wf = Workflow {
            steps: vec![Step {
                tests: "workflow.next(\"Ping\");".into(),
                ..Step::new(Path::new("ping.toml"))
            }],
            ..Workflow::default()
        };
        let result = run(
            &Client::default(),
            &col,
            &wf,
            &RunOptions::default(),
            |_| {},
        )
        .await;
        let iteration = &result.iterations[0];
        assert_eq!(iteration.steps.len(), MAX_STEPS);
        assert!(iteration.steps.iter().all(|s| !s.outcome.failed()));
        assert_eq!(iteration.ended, Some(FlowEnd::TooManySteps));
        assert!(iteration.failed());
        assert!(FlowEnd::TooManySteps.message().contains("20 steps"));
    }
}
