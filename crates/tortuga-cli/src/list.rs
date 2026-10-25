//! `tortuga-cli list`: a project's requests, folders, workflows and environments as JSON,
//! for editor extensions (R-VSC-1), so they never re-implement how a project is read.

use serde::Serialize;
use tortuga_core::workspace::WorkspaceFile;
use tortuga_core::{Collection, Item};

use crate::Args;

#[derive(Serialize)]
struct Project<'a> {
    name: &'a str,
    items: Vec<Entry<'a>>,
    workflows: Vec<Named<'a>>,
    /// The project's, then the workspace's (with `--workspace`) not already among them.
    environments: Vec<Named<'a>>,
}

/// Paths are relative to the project, with `/`, as `tortuga-cli run` takes them.
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum Entry<'a> {
    Folder {
        name: &'a str,
        path: String,
        items: Vec<Entry<'a>>,
    },
    Request {
        name: &'a str,
        path: String,
        method: &'static str,
    },
    Broken {
        path: String,
        error: &'a str,
    },
}

#[derive(Serialize)]
struct Named<'a> {
    id: &'a str,
    name: &'a str,
}

/// The listing of `args.project` (and the environments of `args.workspace`).
pub fn list(args: &Args) -> Result<String, String> {
    let collection = Collection::open(&tortuga_core::storage::FsStorage, &args.project)
        .map_err(|e| e.to_string())?;
    let workspace = match &args.workspace {
        None => WorkspaceFile::default(),
        Some(path) => crate::read_workspace(path)?,
    };
    Ok(to_json(&collection, &workspace))
}

fn to_json(collection: &Collection, workspace: &WorkspaceFile) -> String {
    let mut environments: Vec<Named<'_>> = collection
        .environments
        .iter()
        .map(|e| Named {
            id: &e.id,
            name: &e.name,
        })
        .collect();
    for e in &workspace.environments {
        if !environments
            .iter()
            .any(|n| n.id == e.id || n.name == e.name)
        {
            environments.push(Named {
                id: &e.id,
                name: &e.name,
            });
        }
    }
    let project = Project {
        name: collection.name(),
        items: entries(&collection.items),
        workflows: collection
            .workflows
            .iter()
            .map(|w| Named {
                id: &w.id,
                name: &w.name,
            })
            .collect(),
        environments,
    };
    serde_json::to_string_pretty(&project).unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}"))
}

fn entries(items: &[Item]) -> Vec<Entry<'_>> {
    items
        .iter()
        .map(|item| {
            let path = item.path().to_string_lossy().replace('\\', "/");
            match item {
                Item::Folder(f) => Entry::Folder {
                    name: &f.name,
                    path,
                    items: entries(&f.items),
                },
                Item::Request(r) => Entry::Request {
                    name: &r.request.name,
                    path,
                    method: r.request.method.as_str(),
                },
                Item::Broken(b) => Entry::Broken {
                    path,
                    error: &b.error,
                },
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use tortuga_core::storage::FsStorage;
    use tortuga_core::{HttpMethod, Request};

    use super::*;

    /// R-VSC-1: folders nest, paths are relative with `/`, names keep non-ASCII text.
    #[test]
    fn lists_folders_requests_workflows_and_environments() {
        let dir = tempfile::tempdir().expect("dir");
        let col = Collection::create(&FsStorage, dir.path(), "Shop ü😀").expect("create");
        let folder = col
            .new_folder(&FsStorage, Path::new(""), "Users")
            .expect("folder");
        let mut request = Request::new(HttpMethod::Post, "https://a.test/users");
        request.name = "Create user ü😀".into();
        col.new_request(&FsStorage, &folder, &request)
            .expect("request");
        std::fs::create_dir_all(dir.path().join("workflows")).expect("dir");
        std::fs::write(
            dir.path().join("workflows/smoke.toml"),
            "name = \"Smoke\"\n",
        )
        .expect("wf");
        std::fs::create_dir_all(dir.path().join("environments")).expect("dir");
        std::fs::write(dir.path().join("environments/dev.toml"), "name = \"Dev\"\n").expect("env");
        let col = Collection::open(&FsStorage, dir.path()).expect("open");

        let json: serde_json::Value =
            serde_json::from_str(&to_json(&col, &WorkspaceFile::default())).expect("json");

        assert_eq!(json["name"], "Shop ü😀");
        assert_eq!(json["items"][0]["kind"], "folder");
        assert_eq!(json["items"][0]["path"], "users");
        let req = &json["items"][0]["items"][0];
        assert_eq!(req["kind"], "request");
        assert_eq!(req["name"], "Create user ü😀");
        assert_eq!(req["method"], "POST");
        assert!(
            req["path"]
                .as_str()
                .is_some_and(|p| p.starts_with("users/"))
        );
        assert_eq!(json["workflows"][0]["id"], "smoke");
        assert_eq!(json["workflows"][0]["name"], "Smoke");
        assert_eq!(json["environments"][0]["name"], "Dev");
    }

    #[test]
    fn an_empty_project_lists_nothing() {
        let dir = tempfile::tempdir().expect("dir");
        let col = Collection::create(&FsStorage, dir.path(), "Empty").expect("create");

        let json: serde_json::Value =
            serde_json::from_str(&to_json(&col, &WorkspaceFile::default())).expect("json");

        assert_eq!(json["items"], serde_json::json!([]));
        assert_eq!(json["workflows"], serde_json::json!([]));
        assert_eq!(json["environments"], serde_json::json!([]));
    }
}
