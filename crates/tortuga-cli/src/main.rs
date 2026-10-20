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

use tortuga_core::storage::FsStorage;
use tortuga_core::workflow::{self, Progress, RunOptions, RunResult, Workflow};
use tortuga_core::{Client, ClientSettings, Collection, data, report};

const USAGE: &str = "\
Usage: tortuga-cli run <project> [<workflow, folder or request>] [options]

Runs a workflow (by name or id), the requests of a folder, or one request of the
Tortuga project in <project>. Without a target, every request of the project runs.

Options:
  --env NAME            Use this environment (by name or id)
  --data FILE           Run once per row of a CSV or JSON data file
  --var NAME=VALUE      Set a variable (repeatable); wins over the project's values
  --workspace FILE      Use the variables, settings, headers and auth of this
                        .tortuga-workspace.toml; the project's own win over them
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
    cacerts: Vec<PathBuf>,
    report: Option<(ReportKind, PathBuf)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReportKind {
    Junit,
    Json,
}

enum Command {
    Run(Box<Args>),
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
    match words.next().as_deref() {
        None | Some("-h" | "--help" | "help") => return Ok(Command::Help),
        Some("-V" | "--version") => return Ok(Command::Version),
        Some("run") => {}
        Some(other) => return Err(format!("unknown command '{other}'")),
    }
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
            "--cacert" => args.cacerts.push(value("--cacert")?.into()),
            "--report" => {
                report_kind = Some(match value("--report")?.as_str() {
                    "junit" => ReportKind::Junit,
                    "json" => ReportKind::Json,
                    other => return Err(format!("unknown report '{other}' (junit or json)")),
                });
            }
            "--out" => out = Some(PathBuf::from(value("--out")?)),
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
fn read_workspace(path: &Path) -> Result<tortuga_core::workspace::WorkspaceFile, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    tortuga_core::workspace::read(&text, path).map_err(|e| e.to_string())
}

/// Runs what `args` asks for. `Ok(passed)`, or an error in the command or the project.
fn run(args: &Args) -> Result<bool, String> {
    let collection = Collection::open(&FsStorage, &args.project).map_err(|e| e.to_string())?;
    let workflow = find_target(&collection, args.target.as_deref())?;
    let environment = match &args.env {
        None => None,
        Some(name) => Some(
            collection
                .environments
                .iter()
                .find(|e| e.id.eq_ignore_ascii_case(name) || e.name.eq_ignore_ascii_case(name))
                .map(|e| e.id.clone())
                .ok_or_else(|| {
                    let names: Vec<&str> = collection
                        .environments
                        .iter()
                        .map(|e| e.name.as_str())
                        .collect();
                    format!(
                        "no environment '{name}' in the project (it has: {})",
                        names.join(", ")
                    )
                })?,
        ),
    };
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
    let workspace = match &args.workspace {
        None => tortuga_core::workspace::WorkspaceFile::default(),
        Some(path) => read_workspace(path)?,
    };
    let client = client(args)?;
    let options = RunOptions {
        environment,
        secrets: BTreeMap::new(),
        overrides: args.vars.clone(),
        workspace: workspace.variables.values,
        workspace_defaults: workspace.defaults,
        rows,
    };

    let env_name = options
        .environment
        .as_ref()
        .and_then(|id| collection.environment(id))
        .map_or_else(
            || "no environment".to_owned(),
            |e| format!("environment {}", e.name),
        );
    println!("{} — {} ({env_name})", collection.name(), workflow.name);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    let multi = options.rows.len() > 1;
    let result = runtime.block_on(workflow::run(
        &client,
        &collection,
        &workflow,
        &options,
        |p| {
            print_progress(p, &options, multi);
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

fn print_progress(p: Progress<'_>, options: &RunOptions, multi: bool) {
    match p {
        Progress::Started { iteration, step } => {
            if step == 0 && multi {
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
            "run api lifecycle --env Staging --var user=ann --var x=a=b --data rows.csv --insecure --report junit --out r.xml --workspace w.toml",
        ) else {
            panic!("run")
        };
        assert_eq!(a.project, PathBuf::from("api"));
        assert_eq!(a.target.as_deref(), Some("lifecycle"));
        assert_eq!(a.env.as_deref(), Some("Staging"));
        assert_eq!(a.vars["x"], "a=b");
        assert_eq!(a.workspace, Some(PathBuf::from("w.toml")));
        assert!(a.insecure);
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
