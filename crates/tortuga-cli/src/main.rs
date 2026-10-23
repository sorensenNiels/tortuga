//! `tortuga-cli`: runs a workflow, a folder or a request of a Tortuga project from a
//! terminal or CI, with the same files the app uses (R-CLI-1..3, D5).

#![expect(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "a CLI reports to the terminal"
)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

mod mock;

use tortuga_core::storage::FsStorage;
use tortuga_core::workflow::{self, Progress, RunOptions, RunResult, Workflow};
use tortuga_core::workspace::WorkspaceFile;
use tortuga_core::{Client, ClientSettings, Collection, Environment, data, report};

const USAGE: &str = "\
Usage: tortuga-cli run <project> [<workflow, folder or request>] [options]
       tortuga-cli mock <project> [--port N] [--env NAME]

run:  Runs a workflow (by name or id), the requests of a folder, or one request of
      the Tortuga project in <project>. Without a target, every request runs.
mock: Serves the example responses saved with the project's requests on
      http://127.0.0.1:<port> (default 4010) until stopped with Ctrl+C.

Options:
  --env NAME            Use this environment (by name or id): the project's, or with
                        --workspace the workspace's, the project's values winning
  --data FILE           Run once per row of a CSV or JSON data file
  --var NAME=VALUE      Set a variable (repeatable); wins over the project's values
  --workspace FILE      Use the variables, environments, settings, headers and auth
                        of this .tortuga-workspace.toml; the project's own win
  --mock                Run against the project's mock: start it on a free port and
                        send every request that has a server there
  --port N              The mock's port
  --check-examples      Also check each response against its request's examples:
                        status, Content-Type and JSON shape (a difference fails)
  --allow-protected     Run steps that may change data (not GET, HEAD or OPTIONS)
                        against an environment marked protected
  --insecure            Do not verify TLS certificates
  --cacert FILE         Also trust the CA certificates in this PEM file (repeatable)
  --report junit|json   Write a report ...
  --out FILE            ... to this file
  -h, --help            Show this help
  -V, --version         Show the version

Variables can also come from environment variables named TORTUGA_VAR_<name>, e.g.
TORTUGA_VAR_token=... for {{token}}: the way to pass secrets in CI.

Exit status: 0 when everything passed, 1 when a test or step failed, 2 for errors
in the command or the project.";

/// Stack for the thread that runs the workflow (the Linux/macOS main-thread default).
const RUN_STACK: usize = 8 * 1024 * 1024;

/// What the command line asks for.
#[derive(Debug, Default, PartialEq, Eq)]
struct Args {
    project: PathBuf,
    target: Option<String>,
    env: Option<String>,
    data: Option<PathBuf>,
    vars: BTreeMap<String, String>,
    workspace: Option<PathBuf>,
    insecure: bool,
    /// Whether steps that write may run against a protected environment (R-SRV-9).
    protected: ProtectedRuns,
    cacerts: Vec<PathBuf>,
    report: Option<(ReportKind, PathBuf)>,
    /// Run against the project's mock (R-MOCK-5).
    mock: bool,
    port: Option<u16>,
    /// Compare responses with the requests' examples (R-MOCK-8).
    check_examples: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReportKind {
    Junit,
    Json,
}

enum Command {
    Run(Box<Args>),
    /// Serve the project's mock (R-MOCK-5).
    Mock(Box<Args>),
    Help,
    Version,
}

fn main() -> ExitCode {
    let command = match parse(std::env::args().skip(1).collect()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("tortuga-cli: {e}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    match command {
        Command::Help => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Command::Version => {
            println!("tortuga-cli {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Command::Mock(args) => match mock::serve(&args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("tortuga-cli: {e}");
                ExitCode::from(2)
            }
        },
        Command::Run(args) => {
            let mut args = *args;
            args.vars = env_vars(std::env::vars())
                .into_iter()
                .chain(args.vars)
                .collect();
            // Scripts run in the JS engine, which needs more stack than the 1 MB main
            // thread Windows gives (debug builds overflow it), so run on a bigger one.
            let outcome = std::thread::Builder::new()
                .name("run".into())
                .stack_size(RUN_STACK)
                .spawn(move || run(&args))
                .map_err(|e| e.to_string())
                .and_then(|h| h.join().map_err(|_| "the run panicked".to_owned())?);
            match outcome {
                Ok(true) => ExitCode::SUCCESS,
                Ok(false) => ExitCode::from(1),
                Err(e) => {
                    eprintln!("tortuga-cli: {e}");
                    ExitCode::from(2)
                }
            }
        }
    }
}

fn parse(input: Vec<String>) -> Result<Command, String> {
    let mut words = input.into_iter();
    let serve_mock = match words.next().as_deref() {
        None | Some("-h" | "--help" | "help") => return Ok(Command::Help),
        Some("-V" | "--version") => return Ok(Command::Version),
        Some("run") => false,
        Some("mock") => true,
        Some(other) => return Err(format!("unknown command '{other}'")),
    };
    let mut args = Args::default();
    let mut positional = Vec::new();
    let mut report_kind = None;
    let mut out = None;
    while let Some(arg) = words.next() {
        let mut value = |name: &str| words.next().ok_or_else(|| format!("{name} needs a value"));
        match arg.as_str() {
            "--env" => args.env = Some(value("--env")?),
            "--data" => args.data = Some(value("--data")?.into()),
            "--var" => {
                let v = value("--var")?;
                let (name, val) = v
                    .split_once('=')
                    .ok_or_else(|| format!("--var needs NAME=VALUE, not '{v}'"))?;
                args.vars.insert(name.trim().to_owned(), val.to_owned());
            }
            "--workspace" => args.workspace = Some(value("--workspace")?.into()),
            "--insecure" | "-k" => args.insecure = true,
            "--allow-protected" => args.protected = ProtectedRuns::Allowed,
            "--cacert" => args.cacerts.push(value("--cacert")?.into()),
            "--report" => {
                report_kind = Some(match value("--report")?.as_str() {
                    "junit" => ReportKind::Junit,
                    "json" => ReportKind::Json,
                    other => return Err(format!("unknown report '{other}' (junit or json)")),
                });
            }
            "--out" => out = Some(PathBuf::from(value("--out")?)),
            "--mock" => args.mock = true,
            "--check-examples" => args.check_examples = true,
            "--port" => {
                let v = value("--port")?;
                args.port = Some(
                    v.parse()
                        .map_err(|_| format!("--port needs a port number, not '{v}'"))?,
                );
            }
            "-h" | "--help" => return Ok(Command::Help),
            s if s.starts_with('-') => return Err(format!("unknown option '{s}'")),
            _ => positional.push(arg),
        }
    }
    args.report = match (report_kind, out) {
        (Some(kind), Some(out)) => Some((kind, out)),
        (None, None) => None,
        (Some(_), None) => return Err("--report needs --out FILE".into()),
        (None, Some(_)) => return Err("--out needs --report junit or json".into()),
    };
    let mut positional = positional.into_iter();
    args.project = positional
        .next()
        .ok_or("which project? Give its folder (the one with tortuga.toml)")?
        .into();
    args.target = positional.next();
    if let Some(extra) = positional.next() {
        return Err(format!("unexpected '{extra}'"));
    }
    if serve_mock {
        if let Some(target) = args.target {
            return Err(format!("mock serves the whole project; remove '{target}'"));
        }
        return Ok(Command::Mock(Box::new(args)));
    }
    Ok(Command::Run(Box::new(args)))
}

/// `TORTUGA_VAR_<name>=value` environment variables (R-CLI-2, D30).
fn env_vars(env: impl Iterator<Item = (String, String)>) -> BTreeMap<String, String> {
    env.filter_map(|(k, v)| {
        k.strip_prefix("TORTUGA_VAR_")
            .filter(|n| !n.is_empty())
            .map(|n| (n.to_owned(), v))
    })
    .collect()
}

/// A workspace file: its shared variables (R-WS-8), settings, headers and auth
/// (R-SET-1, R-SET-2). Secret variables have no value in the file; pass them with
/// `--var` or `TORTUGA_VAR_<name>`.
fn read_workspace(path: &Path) -> Result<WorkspaceFile, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    tortuga_core::workspace::read(&text, path).map_err(|e| e.to_string())
}

/// Whether a run may change data in a protected environment (R-SRV-9).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum ProtectedRuns {
    #[default]
    Refused,
    /// `--allow-protected`.
    Allowed,
}

/// Refuses a run whose steps may change data against a protected environment, unless
/// `--allow-protected` says so (R-SRV-9).
fn check_protected(
    collection: &Collection,
    workflow: &Workflow,
    environment: Option<&str>,
    protected: ProtectedRuns,
) -> Result<(), String> {
    let Some(env) = environment.and_then(|id| collection.environment(id)) else {
        return Ok(());
    };
    let writes = workflow.writes(collection);
    if !env.protected || protected == ProtectedRuns::Allowed || writes.is_empty() {
        return Ok(());
    }
    Err(format!(
        "environment {} is protected and this run may change its data ({}); add \
         --allow-protected to run it anyway",
        env.name,
        writes.join(", ")
    ))
}

/// The environment called `name` (by name or id), if one is asked for: the project's, or
/// the workspace's with the project's of that name over it (R-WS-9).
fn environment(
    collection: &Collection,
    workspace: &WorkspaceFile,
    name: Option<&str>,
) -> Result<Option<Environment>, String> {
    let Some(name) = name else { return Ok(None) };
    let own = collection
        .environments
        .iter()
        .find(|e| e.id.eq_ignore_ascii_case(name) || e.is_named(name));
    let shared = workspace.environments.iter().find(|e| e.is_named(name));
    // Secret values come from `--var` and `TORTUGA_VAR_<name>`, which win anyway.
    tortuga_core::workspace::effective_environment(
        shared,
        &BTreeMap::new(),
        own,
        &collection.manifest,
    )
    .map(Some)
    .ok_or_else(|| {
        let names: Vec<&str> = workspace
            .environments
            .iter()
            .map(|e| e.name.as_str())
            .chain(collection.environments.iter().map(|e| e.name.as_str()))
            .collect();
        format!(
            "no environment '{name}' in the project or workspace (there are: {})",
            names.join(", ")
        )
    })
}

/// The project, with the environment `--env` asks for in it, and its id; and the
/// workspace file `--workspace` names (R-WS-9).
fn open_project(args: &Args) -> Result<(Collection, Option<String>, WorkspaceFile), String> {
    let collection = Collection::open(&FsStorage, &args.project).map_err(|e| e.to_string())?;
    let workspace = match &args.workspace {
        None => WorkspaceFile::default(),
        Some(path) => read_workspace(path)?,
    };
    Ok(
        match environment(&collection, &workspace, args.env.as_deref())? {
            Some(env) => {
                let id = env.id.clone();
                (collection.with_environment(env), Some(id), workspace)
            }
            None => (collection, None, workspace),
        },
    )
}

/// Runs what `args` asks for. `Ok(passed)`, or an error in the command or the project.
fn run(args: &Args) -> Result<bool, String> {
    let (collection, environment, workspace) = open_project(args)?;
    let workflow = find_target(&collection, args.target.as_deref())?;
    if !args.mock {
        check_protected(
            &collection,
            &workflow,
            environment.as_deref(),
            args.protected,
        )?;
    }
    let data_file = args
        .data
        .clone()
        .or_else(|| workflow.data.as_ref().map(|d| args.project.join(d)));
    let rows = match &data_file {
        None => Vec::new(),
        Some(path) => {
            let text =
                std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
            data::parse(&path.to_string_lossy(), &text)
                .map_err(|e| format!("{}: {e}", path.display()))?
        }
    };
    let client = client(args)?;
    let mut options = RunOptions {
        environment,
        secrets: BTreeMap::new(),
        overrides: args.vars.clone(),
        workspace: workspace.variables.values,
        workspace_defaults: workspace.defaults,
        rows,
        check_examples: args.check_examples,
    };

    let env_name = options
        .environment
        .as_ref()
        .and_then(|id| collection.environment(id))
        .map_or_else(
            || "no environment".to_owned(),
            |e| format!("environment {}", e.name),
        );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    // Requests go to the mock through its environment (R-MOCK-4, R-MOCK-5).
    let (_mock, collection) = if args.mock {
        let (running, with_mock) = runtime.block_on(mock::start_for_run(
            collection,
            options.environment.as_deref(),
            args.port.unwrap_or(0),
        ))?;
        println!(
            "{} — {} ({env_name}, mock on {})",
            with_mock.name(),
            workflow.name,
            running.base_url()
        );
        options.environment = Some(tortuga_core::mock::ENVIRONMENT_ID.to_owned());
        (Some(running), with_mock)
    } else {
        println!("{} — {} ({env_name})", collection.name(), workflow.name);
        (None, collection)
    };
    let multi = options.rows.len() > 1;
    let mut row_shown = None;
    let result = runtime.block_on(workflow::run(
        &client,
        &collection,
        &workflow,
        &options,
        |p| {
            print_progress(p, &options, multi, &mut row_shown);
        },
    ));
    print_summary(&workflow, &result);
    if let Some((kind, out)) = &args.report {
        let text = match kind {
            ReportKind::Junit => report::junit(&workflow, &result),
            ReportKind::Json => report::json(&workflow, &result),
        };
        std::fs::write(out, text).map_err(|e| format!("{}: {e}", out.display()))?;
        println!("Report written to {}", out.display());
    }
    Ok(result.passed())
}

fn client(args: &Args) -> Result<Client, String> {
    let settings = ClientSettings {
        verify_tls: !args.insecure,
        ..ClientSettings::default()
    };
    let mut files = Vec::new();
    for path in &args.cacerts {
        let pem = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        files.push((path.clone(), pem));
    }
    let (client, problems) =
        Client::with_ca_certificates(settings, &files).map_err(|e| e.to_string())?;
    if let Some(problem) = problems.first() {
        return Err(problem.to_string());
    }
    Ok(
        client.with_file_reader(tortuga_core::FileReader(std::sync::Arc::new(|p: &Path| {
            std::fs::read(p)
        }))),
    )
}

/// The workflow to run: a workflow by id or name, a folder, a request, or (none) the project.
fn find_target(collection: &Collection, target: Option<&str>) -> Result<Workflow, String> {
    let Some(target) = target.map(str::trim).filter(|t| !t.is_empty() && *t != ".") else {
        return Ok(Workflow::from_folder(collection, Path::new("")));
    };
    let stem = target
        .strip_prefix("workflows/")
        .unwrap_or(target)
        .trim_end_matches(".toml");
    if let Some(w) = collection
        .workflows
        .iter()
        .find(|w| w.id == stem || w.name.eq_ignore_ascii_case(target))
    {
        return Ok(w.clone());
    }
    let path = Path::new(target.trim_end_matches('/'));
    if matches!(collection.find(path), Some(tortuga_core::Item::Folder(_))) {
        return Ok(Workflow::from_folder(collection, path));
    }
    let request = if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("toml"))
    {
        path.to_path_buf()
    } else {
        PathBuf::from(format!("{target}.toml"))
    };
    if collection.request(&request).is_some() {
        return Ok(Workflow::single(collection, &request));
    }
    let names: Vec<&str> = collection
        .workflows
        .iter()
        .map(|w| w.name.as_str())
        .collect();
    Err(format!(
        "'{target}' is not a workflow, folder or request of the project (workflows: {})",
        if names.is_empty() {
            "none".to_owned()
        } else {
            names.join(", ")
        }
    ))
}

/// `row_shown`: the iteration whose row was printed last. Steps may run out of order
/// (R-FLOW-6), so a row starts when the iteration changes, not at step 0.
fn print_progress(
    p: Progress<'_>,
    options: &RunOptions,
    multi: bool,
    row_shown: &mut Option<usize>,
) {
    match p {
        Progress::Started { iteration, .. } => {
            if multi && *row_shown != Some(iteration) {
                *row_shown = Some(iteration);
                let row = options.rows.get(iteration).map(|r| {
                    r.iter()
                        .map(|(k, v)| format!("{k}={v}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                });
                println!("\nRow {}: {}", iteration + 1, row.unwrap_or_default());
            }
        }
        Progress::Finished { step, .. } => {
            let o = &step.outcome;
            let detail = match &o.response {
                Some(Ok(r)) => format!(
                    "{} {}, {} ms",
                    r.status,
                    r.status_text,
                    r.elapsed.as_millis()
                ),
                Some(Err(_)) | None => "no response".to_owned(),
            };
            println!(
                "  {} {} ({detail})",
                if o.failed() { "FAIL" } else { "ok  " },
                step.name
            );
            for t in &o.tests {
                match &t.message {
                    None => println!("         ok   {}", t.name),
                    Some(m) => println!("         FAIL {}: {m}", t.name),
                }
            }
            let response_error = o.response.as_ref().and_then(|r| r.as_ref().err());
            for e in o.errors.iter().chain(response_error) {
                println!("         {e}");
            }
            for line in o.console.iter().filter_map(|(_, e)| e.log_text()) {
                println!("         log: {line}");
            }
        }
    }
}

/// "1 step", "2 steps".
fn count(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("{n} {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

fn print_summary(workflow: &Workflow, result: &RunResult) {
    let (passed, failed, failed_steps) = result.counts();
    let skipped: usize = result.iterations.iter().map(|i| i.skipped).sum();
    let mut parts = vec![
        format!("{} passed", count(passed, "test")),
        format!("{failed} failed"),
    ];
    if failed_steps > 0 {
        parts.push(format!("{} failed", count(failed_steps, "step")));
    }
    if skipped > 0 {
        parts.push(format!("{} skipped", count(skipped, "step")));
    }
    // How scripts ended iterations (R-FLOW-6).
    for (n, iteration) in result.iterations.iter().enumerate() {
        if let Some(end) = iteration.ended {
            if result.iterations.len() > 1 {
                println!("Row {}: {}", n + 1, end.message());
            } else {
                println!("{}", end.message());
            }
        }
    }
    let verdict = if result.passed() { "PASSED" } else { "FAILED" };
    println!("\n{verdict}: {} — {}", workflow.name, parts.join(", "));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &str) -> Result<Command, String> {
        parse(s.split_whitespace().map(str::to_owned).collect())
    }

    #[test]
    fn parses_options() {
        let Ok(Command::Run(a)) = args(
            "run api lifecycle --env Staging --var user=ann --var x=a=b --data rows.csv --insecure --report junit --out r.xml --workspace w.toml --check-examples",
        ) else {
            panic!("run")
        };
        assert_eq!(a.project, PathBuf::from("api"));
        assert_eq!(a.target.as_deref(), Some("lifecycle"));
        assert_eq!(a.env.as_deref(), Some("Staging"));
        assert_eq!(a.vars["x"], "a=b");
        assert_eq!(a.workspace, Some(PathBuf::from("w.toml")));
        assert!(a.insecure);
        assert!(a.check_examples);
        assert_eq!(a.report, Some((ReportKind::Junit, PathBuf::from("r.xml"))));
        assert!(matches!(args("--help"), Ok(Command::Help)));
        assert!(args("run").is_err(), "no project");
        assert!(args("run api --frob").is_err());
        assert!(args("run api --report junit").is_err(), "no --out");
        assert!(args("run api --var novalue").is_err());
        assert!(args("deploy").is_err());
    }

    #[test]
    fn variables_from_the_environment() {
        let env = [
            ("TORTUGA_VAR_token".to_owned(), "s3cret".to_owned()),
            ("PATH".to_owned(), "/bin".to_owned()),
            ("TORTUGA_VAR_".to_owned(), "x".to_owned()),
        ];
        assert_eq!(
            env_vars(env.into_iter()),
            BTreeMap::from([("token".to_owned(), "s3cret".to_owned())])
        );
    }
}
