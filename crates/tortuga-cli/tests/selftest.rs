//! Tortuga tests itself with Tortuga (R-MOCK-7): every workflow of every project in
//! `selftest/tortuga-selftest.tortuga-workspace.toml` runs against its project's mock.
//! `make selftest` runs only this; `cargo test` and CI run it with everything else. A
//! failing workflow's output is in the failure message.

use std::path::{Path, PathBuf};
use std::process::Command;

use tortuga_core::Collection;
use tortuga_core::storage::FsStorage;

fn workspace_file() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../selftest/tortuga-selftest.tortuga-workspace.toml")
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
    assert!(runs >= 3, "found {runs} workflows");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
