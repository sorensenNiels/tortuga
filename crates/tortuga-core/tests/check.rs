//! `check` on a project on disk (R-CLI-5, R-CLI-6).

use std::path::Path;

use tortuga_core::Collection;
use tortuga_core::check::credential::Credential;
use tortuga_core::check::{self, Finding, Kind};
use tortuga_core::storage::FsStorage;
use tortuga_core::workspace::WorkspaceFile;

const GITHUB: &str = concat!("ghp_", "abcdefghijklmnopqrstuvwxyz0123");

fn write(root: &Path, file: &str, text: &str) {
    let path = root.join(file);
    std::fs::create_dir_all(path.parent().expect("a file has a folder")).expect("dir");
    std::fs::write(path, text).expect("write");
}

fn findings(root: &Path) -> Vec<String> {
    check_with(root, &WorkspaceFile::default(), None)
        .iter()
        .map(ToString::to_string)
        .collect()
}

fn check_with(root: &Path, workspace: &WorkspaceFile, file: Option<&Path>) -> Vec<Finding> {
    let collection = Collection::open(&FsStorage, root).expect("project");
    check::check(&FsStorage, &collection, workspace, file)
}

/// A project with one of each mistake, and a clean request.
fn project(root: &Path) {
    write(
        root,
        "tortuga.toml",
        "format = 1\nname = \"P\"\nsecrets = [\"password\"]\n\n[variables]\nhost = \"x\"\n",
    );
    write(
        root,
        "environments/dev.toml",
        "name = \"Dev\"\n\n[variables]\nuser = \"ann\"\n",
    );
    write(root, "environments/prod.toml", "name = \"Prod\"\n");
    write(
        root,
        "users/get-ü😀.toml",
        "name = \"Get\"\nmethod = \"GET\"\nurl = \"https://{{host}}/{{user}}/{{id}}/{{$uuid}}/{{password}}/{{token}}/{{col}}\"\n\n\
         [capture]\nid = \"$.id\"\n",
    );
    write(
        root,
        "login.toml",
        &format!(
            "name = \"Log in\"\nmethod = \"POST\"\nurl = \"https://{{{{host}}}}\"\n\n\
             [[headers]]\nname = \"Authorization\"\nvalue = \"token {GITHUB}\"\n\n\
             [auth]\nkind = \"basic\"\nusername = \"ann\"\npassword = \"hunter2\"\n\n\
             [scripts]\ntests = 'vars.set(\"token\", res.json().t)'\n\n\
             [[examples]]\nname = \"ok\"\nbody = \"{GITHUB}\"\n"
        ),
    );
    write(root, "broken.toml", "name = ");
    write(
        root,
        "workflows/flow.toml",
        "name = \"Flow\"\ndata = \"rows.csv\"\n\n[[steps]]\nrequest = \"login.toml\"\n\n[[steps]]\nrequest = \"gone.toml\"\n",
    );
    write(root, "rows.csv", "col\n1\n");
}

#[test]
fn every_kind_of_mistake_is_found() {
    // R-CLI-5, R-CLI-6
    let dir = tempfile::tempdir().expect("tempdir");
    project(dir.path());
    let found = findings(dir.path());
    assert!(
        found.iter().any(|f| f.starts_with("broken.toml: ")),
        "{found:#?}"
    );
    assert!(
        found.contains(
            &"workflows/flow.toml: step 2: names gone.toml, which does not exist".to_owned()
        ),
        "{found:#?}"
    );
    assert!(
        found.contains(&"users/get-ü😀.toml: {{user}}: has no value in Prod".to_owned()),
        "{found:#?}"
    );
    assert!(
        found.contains(&"login.toml: header Authorization: looks like a GitHub token".to_owned()),
        "{found:#?}"
    );
    assert!(
        found
            .contains(&"login.toml: auth password: the Basic auth password is typed in".to_owned()),
        "{found:#?}"
    );
    // Captured, scripted, secret, dynamic, data and defined variables are fine, as are
    // examples and the username.
    assert_eq!(found.len(), 5, "{found:#?}");
    assert!(
        found.iter().all(|f| !f.contains(GITHUB)),
        "values never shown (P6)"
    );
}

#[test]
fn allowed_places_are_accepted_and_stale_entries_reported() {
    // R-CLI-6
    let dir = tempfile::tempdir().expect("tempdir");
    project(dir.path());
    let manifest = std::fs::read_to_string(dir.path().join("tortuga.toml")).expect("read");
    write(
        dir.path(),
        "tortuga.toml",
        &format!(
            "{}\n[check]\nallow = [\"login.toml: header Authorization\", \"old.toml: url\"]\n",
            manifest.replace("format = 1", "format = 3")
        ),
    );
    let found = findings(dir.path());
    assert!(!found.iter().any(|f| f.contains("GitHub")), "{found:#?}");
    assert!(
        found.contains(
            &"tortuga.toml: check.allow: allowed place not found: old.toml: url".to_owned()
        ),
        "{found:#?}"
    );
}

#[test]
fn a_clean_project_has_no_findings() {
    // R-CLI-5: also an empty project with no environments.
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), "tortuga.toml", "format = 1\nname = \"P\"\n");
    assert_eq!(findings(dir.path()), Vec::<String>::new());
    write(
        dir.path(),
        "a.toml",
        "name = \"A\"\nmethod = \"GET\"\nurl = \"{{base}}\"\n",
    );
    assert_eq!(findings(dir.path()), ["a.toml: {{base}}: has no value"]);
}

#[test]
fn the_workspace_gives_values_and_is_scanned() {
    // R-CLI-5, R-CLI-6: workspace variables and secrets count, its own values are checked.
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join("p");
    write(&root, "tortuga.toml", "format = 1\nname = \"P\"\n");
    write(
        &root,
        "a.toml",
        "name = \"A\"\nmethod = \"GET\"\nurl = \"{{base}}/{{key}}/{{stage}}\"\n",
    );
    let ws_file = dir.path().join("team.tortuga-workspace.toml");
    let text = format!(
        "format = 1\nname = \"Team\"\nsecrets = [\"key\"]\n\n[variables]\nbase = \"https://x\"\ntoken = \"{GITHUB}\"\n\n\
         [[projects]]\npath = \"p\"\n\n[[environments]]\nid = \"st\"\nname = \"Staging\"\n\n[environments.variables]\nstage = \"1\"\n"
    );
    std::fs::write(&ws_file, &text).expect("write");
    let workspace = tortuga_core::workspace::read(&text, &ws_file).expect("workspace");
    let found = check_with(&root, &workspace, Some(&ws_file));
    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(found[0].file, "../team.tortuga-workspace.toml");
    assert_eq!(found[0].place, "variable token");
    assert_eq!(found[0].kind, Kind::Credential(Credential::GitHubToken));
}

#[test]
fn a_thousand_requests_are_checked_quickly() {
    // R-CLI-5, R-NF-1: fits a pre-commit hook.
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), "tortuga.toml", "format = 1\nname = \"P\"\n");
    for env in ["a", "b", "c"] {
        write(
            dir.path(),
            &format!("environments/{env}.toml"),
            &format!("name = \"{env}\"\n"),
        );
    }
    for i in 0..1000 {
        write(
            dir.path(),
            &format!("r{i}.toml"),
            &format!(
                "name = \"R{i}\"\nmethod = \"POST\"\nurl = \"https://x/{{{{v{i}}}}}\"\n\n[body]\nkind = \"json\"\ncontent = '{{\"a\": \"{}\"}}'\n",
                "x".repeat(2000)
            ),
        );
    }
    let collection = Collection::open(&FsStorage, dir.path()).expect("project");
    let start = web_time::Instant::now();
    let found = check::check(&FsStorage, &collection, &WorkspaceFile::default(), None);
    let took = start.elapsed();
    assert_eq!(found.len(), 1000);
    assert!(took.as_millis() < 1000, "took {took:?}");
}
