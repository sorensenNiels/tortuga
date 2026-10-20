//! Tortuga tests itself with Tortuga (R-MOCK-7): every workflow of every project in
//! `selftest/tortuga-selftest.tortuga-workspace.toml` runs against its project's mock,
//! with each response checked against its examples (R-MOCK-8). The contract project's
//! workflow also runs against `selftest/contract-built`, an implementation that drifted.
//! `make selftest` runs only this; `cargo test` and CI run it with everything else. A
//! failing workflow's output is in the failure message.

use std::io::{BufRead as _, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use tortuga_core::Collection;
use tortuga_core::storage::FsStorage;

fn selftest_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../selftest")
}

fn workspace_file() -> PathBuf {
    selftest_dir().join("tortuga-selftest.tortuga-workspace.toml")
}

#[test]
fn every_selftest_workflow_passes_against_the_mock() {
    let file = workspace_file();
    let text = std::fs::read_to_string(&file).expect("the self-test workspace file");
    let workspace = tortuga_core::workspace::read(&text, &file).expect("a workspace file");
    assert!(
        !workspace.projects.is_empty(),
        "the workspace lists projects"
    );

    let mut failures = Vec::new();
    let mut runs = 0;
    for project in &workspace.projects {
        let collection = Collection::open(&FsStorage, &project.path)
            .unwrap_or_else(|e| panic!("{}: {e}", project.path.display()));
        assert!(
            collection.broken_environments.is_empty() && collection.broken_workflows.is_empty(),
            "{}: every file reads",
            collection.name()
        );
        assert!(
            !collection.workflows.is_empty(),
            "{} has a workflow to run",
            collection.name()
        );
        for workflow in &collection.workflows {
            runs += 1;
            let out = Command::new(env!("CARGO_BIN_EXE_tortuga-cli"))
                .arg("run")
                .arg(&project.path)
                .arg(&workflow.id)
                .arg("--mock")
                .arg("--check-examples")
                .arg("--workspace")
                .arg(&file)
                .output()
                .expect("run tortuga-cli");
            let stdout = String::from_utf8_lossy(&out.stdout);
            if !out.status.success() {
                failures.push(format!(
                    "{} / {}:\n{stdout}{}",
                    collection.name(),
                    workflow.name,
                    String::from_utf8_lossy(&out.stderr)
                ));
            }
        }
    }
    assert!(runs >= 4, "found {runs} workflows");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// R-MOCK-8: the design in `selftest/contract`, sent to the mock of
/// `selftest/contract-built` as if it were the real API, reports where they differ.
#[test]
fn the_contract_check_reports_where_the_built_api_drifted_from_the_design() {
    let mut built = Command::new(env!("CARGO_BIN_EXE_tortuga-cli"))
        .arg("mock")
        .arg(selftest_dir().join("contract-built"))
        .args(["--port", "0"])
        .stdout(Stdio::piped())
        .spawn()
        .expect("start the built API's mock");
    let mut lines = BufReader::new(built.stdout.take().expect("stdout")).lines();
    let first = lines.next().expect("a line").expect("utf-8");
    let port = first
        .split("http://127.0.0.1:")
        .nth(1)
        .and_then(|rest| rest.split(' ').next())
        .unwrap_or_else(|| panic!("no port in “{first}”"))
        .to_owned();

    let out = Command::new(env!("CARGO_BIN_EXE_tortuga-cli"))
        .arg("run")
        .arg(selftest_dir().join("contract"))
        .args(["Order lifecycle", "--env", "built", "--check-examples"])
        .args(["--var", &format!("built_port={port}")])
        .arg("--workspace")
        .arg(workspace_file())
        .output()
        .expect("run tortuga-cli");
    built.kill().expect("stop the mock");
    // Reaping it; its exit status after the kill says nothing.
    let _ = built.wait();

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{stdout}");
    for expected in [
        // Other values, the same shape.
        "ok   Matches example “orders”",
        "FAIL Matches example “open order”: $.id is a string, the example has a number; \
         $.status is missing; $.lines[0].qty is a string, the example has a number; \
         $.state is not in the example",
        "FAIL Matches an example: Status 200 is not one of the examples' (201)",
    ] {
        assert!(
            stdout.contains(expected),
            "expected “{expected}” in:\n{stdout}"
        );
    }
}
