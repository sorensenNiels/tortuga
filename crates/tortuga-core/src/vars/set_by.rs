//! The variables a project's captures and scripts set when they run (R-VAR-11, D79),
//! found in its files without running anything: what `tortuga-cli check` counts as
//! defined (R-CLI-5), and what the app lists next to the project's variables.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::collection::{Collection, Item};

/// Which part of a request or workflow step sets a variable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Setter {
    Capture,
    PreRequest,
    Tests,
}

/// What sets a variable.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Place {
    /// The request at this path, relative to the project.
    Request(PathBuf),
    /// A step of the workflow with this id.
    Workflow(String),
}

/// One place that sets a variable.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SetBy {
    pub place: Place,
    /// The request's or workflow's name.
    pub name: String,
    pub setter: Setter,
}

/// Every name the project's captures and scripts set, as written (`workspace.token`
/// stays `workspace.token`, R-WS-8), with each place that sets it, in tree order.
#[must_use]
pub fn set_by(collection: &Collection) -> BTreeMap<String, Vec<SetBy>> {
    let mut out: BTreeMap<String, Vec<SetBy>> = BTreeMap::new();
    let mut add = |names: Vec<String>, place: &Place, name: &str, setter: Setter| {
        for var in names {
            let by = SetBy {
                place: place.clone(),
                name: name.to_owned(),
                setter,
            };
            let places = out.entry(var).or_default();
            if !places.contains(&by) {
                places.push(by);
            }
        }
    };
    let mut requests = Vec::new();
    walk(&collection.items, &mut |item| {
        if let Item::Request(r) = item {
            requests.push(r);
        }
    });
    for r in requests {
        let place = Place::Request(r.path.clone());
        let name = &r.request.name;
        let scripts = &r.request.scripts;
        add(
            r.request.capture.keys().cloned().collect(),
            &place,
            name,
            Setter::Capture,
        );
        add(
            names_set_in(&scripts.pre_request),
            &place,
            name,
            Setter::PreRequest,
        );
        add(names_set_in(&scripts.tests), &place, name, Setter::Tests);
    }
    for w in &collection.workflows {
        let place = Place::Workflow(w.id.clone());
        for step in &w.steps {
            add(
                step.capture.keys().cloned().collect(),
                &place,
                &w.name,
                Setter::Capture,
            );
            add(names_set_in(&step.tests), &place, &w.name, Setter::Tests);
        }
    }
    out
}

/// The quoted names of `vars.set("name", …)` in `script`, as written. A name in a
/// variable (`vars.set(name, …)`) can't be known without running the script.
#[must_use]
pub fn names_set_in(script: &str) -> Vec<String> {
    let mut names = Vec::new();
    for (at, call) in script.match_indices("vars.set(") {
        let Some(rest) = script.get(at + call.len()..) else {
            continue;
        };
        let rest = rest.trim_start();
        let Some(quote) = rest
            .chars()
            .next()
            .filter(|c| matches!(c, '"' | '\'' | '`'))
        else {
            continue;
        };
        let Some(name) = rest
            .get(quote.len_utf8()..)
            .and_then(|r| r.split(quote).next())
            .filter(|n| !n.is_empty())
        else {
            continue;
        };
        names.push(name.to_owned());
    }
    names
}

fn walk<'a>(items: &'a [Item], f: &mut dyn FnMut(&'a Item)) {
    for item in items {
        f(item);
        if let Item::Folder(folder) = item {
            walk(&folder.items, f);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Request;
    use crate::storage::FsStorage;
    use crate::workflow::{Step, Workflow};

    #[test]
    fn quoted_names_in_vars_set_are_found() {
        // R-CLI-5, R-VAR-11
        let script = r#"vars.set("token", x); vars.set( 'ü😀', 1); vars.set(`a`, 2);
            vars.set("workspace.shared", 3); vars.set(name, 4); vars.set("#;
        assert_eq!(
            names_set_in(script),
            ["token", "ü😀", "a", "workspace.shared"]
        );
        assert_eq!(names_set_in(""), Vec::<String>::new());
    }

    /// R-VAR-11: captures and scripts of requests in folders and of workflow steps are
    /// found, each place once, with what sets the name there.
    #[test]
    fn names_are_found_with_every_place_that_sets_them() {
        let dir = tempfile::tempdir().expect("temp dir");
        let c = Collection::create(&FsStorage, dir.path(), "C").expect("create");
        let folder = c
            .new_folder(&FsStorage, std::path::Path::new(""), "Batches")
            .expect("folder");
        let mut create = Request {
            name: "Create batch".into(),
            ..Request::default()
        };
        create.scripts.pre_request = "vars.set('batchId', 1); vars.set('batchId', 2);".into();
        create.scripts.tests = "vars.set(\"workspace.token\", res.body);".into();
        create.capture.insert("batchId".into(), "$.id".into());
        let path = c
            .new_request(&FsStorage, &folder, &create)
            .expect("request");
        let c = Collection::open(&FsStorage, dir.path()).expect("open");
        let workflow = Workflow {
            id: "flow".into(),
            name: "Flow".into(),
            steps: vec![Step {
                request: crate::workflow::path_text(&path),
                capture: [("status".to_owned(), "status".to_owned())].into(),
                tests: "vars.set('batchId', 3);".into(),
                ..Step::default()
            }],
            ..Workflow::default()
        };
        let c = Collection {
            workflows: vec![workflow],
            ..c
        };

        let found = set_by(&c);

        let by = |place: Place, name: &str, setter| SetBy {
            place,
            name: name.into(),
            setter,
        };
        let request = Place::Request(path);
        let flow = Place::Workflow("flow".into());
        assert_eq!(
            found.get("batchId"),
            Some(&vec![
                by(request.clone(), "Create batch", Setter::Capture),
                by(request.clone(), "Create batch", Setter::PreRequest),
                by(flow.clone(), "Flow", Setter::Tests),
            ])
        );
        assert_eq!(
            found.get("workspace.token"),
            Some(&vec![by(request, "Create batch", Setter::Tests)])
        );
        assert_eq!(
            found.get("status"),
            Some(&vec![by(flow, "Flow", Setter::Capture)])
        );
        assert_eq!(found.len(), 3);
    }

    #[test]
    fn an_empty_project_sets_nothing() {
        let dir = tempfile::tempdir().expect("temp dir");
        let c = Collection::create(&FsStorage, dir.path(), "C").expect("create");
        assert!(set_by(&c).is_empty());
    }
}
