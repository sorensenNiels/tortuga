use std::path::{Path, PathBuf};

use egui_kittest::kittest::Queryable;
use tortuga_core::storage::FsStorage;
use tortuga_core::{Collection, EnvironmentColor, Server};

use super::*;
use crate::state::tests::{project, temp};

/// Projects on disk, open in an `AppState`; each has a server "Users API" and the
/// environments given.
fn open(dir: &Path, projects: &[(&str, &[&str])]) -> (Vec<PathBuf>, AppState) {
    let mut roots = Vec::new();
    for (name, envs) in projects {
        let root = dir.join(name);
        project(&root, name, envs);
        let c = Collection::open(&FsStorage, &root).expect("open");
        let mut manifest = c.manifest.clone();
        manifest.servers = vec![Server {
            id: "users-api".into(),
            name: "Users API".into(),
        }];
        c.save_config(&FsStorage, &manifest, &c.environments)
            .expect("config");
        roots.push(root);
    }
    let state = AppState::new(Box::new(crate::test_host::TestHost {
        open: roots.clone(),
        ..crate::test_host::TestHost::default()
    }));
    (roots, state)
}

fn harness<'a>(
    dialog: &'a mut EnvironmentsDialog,
    state: &'a AppState,
    result: &'a std::cell::RefCell<Option<SavedEnvironments>>,
) -> egui_kittest::Harness<'a> {
    let mut installed = false;
    // The URL and value cells have no accessible name yet; these tests are about layout.
    egui_kittest::Harness::builder()
        .with_size(egui::vec2(1200.0, 900.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            if !installed {
                theme::install(ui.ctx());
                installed = true;
            }
            if let DialogResult::Done(saved) = dialog.show(ui.ctx(), state) {
                *result.borrow_mut() = Some(saved);
            }
        })
}

fn on_screen(h: &egui_kittest::Harness<'_>, value: &str) -> bool {
    h.query_all_by_value(value).next().is_some()
}

/// Clicks the header of a project's values (R-WS-10); the list on the left has its
/// name too.
fn toggle(h: &mut egui_kittest::Harness<'_>, project: &str) {
    h.query_all_by_label(project)
        .max_by(|a, b| a.rect().left().total_cmp(&b.rect().left()))
        .expect("project header")
        .click();
    h.run();
}

/// R-WS-9: one list for the workspace; each project has a column for every environment
/// in it, also ones it had no file for, in the list's order.
#[test]
fn every_project_follows_the_list() {
    let dir = temp("env-dialog-list");
    let (_, state) = open(&dir, &[("A", &["Dev", "Prod"]), ("B", &["prod"])]);
    let mut dialog = EnvironmentsDialog::for_workspace(&state);
    let names: Vec<&str> = dialog.list.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["Dev", "Prod"]);
    let b_cols: Vec<&str> = dialog.projects[1]
        .draft
        .environments
        .iter()
        .map(|e| e.name.as_str())
        .collect();
    assert_eq!(b_cols, ["Dev", "prod"]);

    dialog.add_environment();
    dialog.move_environment(2, 0);
    assert_eq!(dialog.projects[0].draft.environments.len(), 3);
    assert_eq!(
        dialog.projects[1].draft.environments[0].name,
        "Environment 3"
    );
    assert_eq!(dialog.shown, Shown::Environment(0), "follows the move");
    dialog.remove_environment(0);
    assert_eq!(dialog.projects[1].draft.environments.len(), 2);
    std::fs::remove_dir_all(dir).expect("clean up");
}

/// R-SRV-7, R-WS-9: only the chosen environment's values are on screen, for every
/// project; opened from a project, only that project's.
#[test]
fn one_environment_is_shown_with_every_project() {
    let dir = temp("env-dialog-shown");
    let (roots, state) = open(&dir, &[("A", &["Dev", "Staging"]), ("B", &[])]);
    let mut dialog = EnvironmentsDialog::for_workspace(&state);
    dialog.projects[0].draft.servers[0].urls = vec!["http://a-dev".into(), "http://a-stg".into()];
    dialog.projects[1].draft.servers[0].urls = vec!["http://b-dev".into(), String::new()];
    for p in &mut dialog.projects {
        p.expanded = true;
    }
    let result = std::cell::RefCell::new(None);
    {
        let mut h = harness(&mut dialog, &state, &result);
        h.run();
        assert!(on_screen(&h, "http://a-dev"));
        assert!(on_screen(&h, "http://b-dev"));
        assert!(!on_screen(&h, "http://a-stg"));
        h.get_by_label("Staging").click();
        h.run();
        assert!(on_screen(&h, "http://a-stg"));
        assert!(!on_screen(&h, "http://b-dev"));
    }

    let mut focused = EnvironmentsDialog::new(&state, roots[1].clone());
    for (p, draft) in focused.projects.iter_mut().zip(&dialog.projects) {
        p.draft = draft.draft.clone();
    }
    assert_eq!(focused.focus.as_deref(), Some(roots[1].as_path()));
    let mut h = harness(&mut focused, &state, &result);
    h.run();
    h.get_by_label("Environments — B");
    assert!(on_screen(&h, "http://b-dev"));
    assert!(!on_screen(&h, "http://a-dev"), "only B");
    h.get_by_label("Show every project and the shared values")
        .click();
    h.run();
    assert!(on_screen(&h, "http://b-dev"), "B stays open");
    assert!(!on_screen(&h, "http://a-dev"), "A starts folded");
    drop(h);
    std::fs::remove_dir_all(dir).expect("clean up");
}

/// R-WS-10: with several projects, each project's values start folded under its name
/// and unfold on a click, staying so when another environment is chosen; a single
/// project starts unfolded.
#[test]
fn several_projects_start_folded() {
    let dir = temp("env-dialog-folded");
    let (_, state) = open(&dir, &[("A ü😀", &["Dev", "Staging"]), ("B", &[])]);
    let mut dialog = EnvironmentsDialog::for_workspace(&state);
    dialog.projects[0].draft.servers[0].urls = vec!["http://a-dev".into(), "http://a-stg".into()];
    dialog.projects[1].draft.servers[0].urls = vec!["http://b-dev".into(), String::new()];
    dialog.projects[1].read_only = true;
    let result = std::cell::RefCell::new(None);
    {
        let mut h = harness(&mut dialog, &state, &result);
        h.run();
        assert!(!on_screen(&h, "http://a-dev"));
        assert!(!on_screen(&h, "http://b-dev"));
        h.get_by_label("B (read-only)");
        toggle(&mut h, "A ü😀");
        assert!(on_screen(&h, "http://a-dev"));
        assert!(!on_screen(&h, "http://b-dev"), "B stays folded");
        h.get_by_label("Staging").click();
        h.run();
        assert!(on_screen(&h, "http://a-stg"), "still unfolded");
        toggle(&mut h, "A ü😀");
        assert!(!on_screen(&h, "http://a-stg"));
    }
    std::fs::remove_dir_all(&dir).expect("clean up");

    let (_, state) = open(&dir, &[("C", &["Dev"])]);
    let mut single = EnvironmentsDialog::for_workspace(&state);
    single.projects[0].draft.servers[0].urls = vec!["http://c-dev".into()];
    let mut h = harness(&mut single, &state, &result);
    h.run();
    assert!(on_screen(&h, "http://c-dev"));
    drop(h);
    std::fs::remove_dir_all(dir).expect("clean up");
}

/// R-WS-9, R-SRV-8, R-SRV-9: colour, protection and a new name are the workspace's and
/// go into every project's file that has the environment; ids are kept; a project gets a
/// file only for environments it sets something in.
#[test]
fn saving_writes_the_list_and_each_project() {
    let dir = temp("env-dialog-save");
    let (roots, state) = open(&dir, &[("A", &["Dev", "Staging"]), ("B", &[])]);
    let mut dialog = EnvironmentsDialog::for_workspace(&state);
    dialog.shown = Shown::Environment(1);
    let result = std::cell::RefCell::new(None);
    {
        let mut h = harness(&mut dialog, &state, &result);
        h.run();
        h.get_by_label("Red").click();
        h.get_by_label("Protected").click();
        h.run();
        // The first text field is the environment's name.
        h.get_all_by_role(egui::accesskit::Role::TextInput)
            .next()
            .expect("name field")
            .click();
        h.run();
        h.get_all_by_role(egui::accesskit::Role::TextInput)
            .next()
            .expect("name field")
            .type_text(" ü😀");
        h.run();
    }
    dialog.projects[1].draft.servers[0].urls[0] = "http://b-dev".into();
    {
        let mut h = harness(&mut dialog, &state, &result);
        h.run();
        h.get_by_label("Save").click();
        h.run();
    }
    let saved = result.borrow_mut().take().expect("saved");
    assert_eq!(saved.list[1].name, "Staging ü😀");
    assert_eq!(saved.list[1].color, Some(EnvironmentColor::Red));
    assert!(saved.list[1].protected);
    let (root_a, a) = &saved.projects[0];
    assert_eq!(root_a, &roots[0]);
    assert_eq!(a.environments[1].id, "staging", "the file keeps its id");
    assert_eq!(a.environments[1].name, "Staging ü😀");
    assert!(a.environments[1].protected);
    assert_eq!(a.environments[1].color, Some(EnvironmentColor::Red));
    let b = &saved.projects[1].1;
    let b_files: Vec<&str> = b.environments.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(b_files, ["Dev"], "B sets nothing for Staging");
    assert_eq!(b.environments[0].servers["users-api"], "http://b-dev");
    std::fs::remove_dir_all(dir).expect("clean up");
}

/// R-WS-9: a project's empty fields show the workspace's values: server URLs by
/// server name whatever its case, variables, and secrets only as "workspace secret".
#[test]
fn empty_project_values_show_what_they_inherit() {
    let shared = EnvDraft {
        servers: vec![("users api".into(), "https://shared.dev".into())],
        variables: vec![
            draft::VarDraft {
                name: "tenant".into(),
                value: "acme ü😀".into(),
                secret: false,
            },
            draft::VarDraft {
                name: "token".into(),
                value: "s3cret".into(),
                secret: true,
            },
        ],
        ..EnvDraft::default()
    };
    let inherited = Inherited::of(&shared, Vec::new());
    assert_eq!(inherited.server("Users API"), Some("https://shared.dev"));
    assert_eq!(inherited.server("Orders"), None);
    let var = |name: &str| draft::VarRow {
        name: name.into(),
        default: "dflt".into(),
        ..draft::VarRow::default()
    };
    assert_eq!(inherited.variable_hint(&var("tenant")), "acme ü😀");
    assert_eq!(inherited.variable_hint(&var("token")), "workspace secret");
    assert_eq!(inherited.variable_hint(&var("other")), "dflt");
}

/// BUG-1: a project's server and variable names are not cut short.
#[test]
fn names_are_not_truncated() {
    let dir = temp("env-dialog-width");
    let (_, state) = open(&dir, &[("C", &["Dev"])]);
    let mut dialog = EnvironmentsDialog::for_workspace(&state);
    dialog.projects[0].draft.add_variable("token".into());
    dialog.shown = Shown::Project(0);
    let result = std::cell::RefCell::new(None);
    let mut h = harness(&mut dialog, &state, &result);
    h.run();
    let widest = |value: &str| {
        h.query_all_by_value(value)
            .map(|n| n.rect().width())
            .fold(0.0, f32::max)
    };
    let server = widest("Users API");
    assert!(server > 150.0, "server name field is only {server} wide");
    let variable = widest("token");
    assert!(
        variable > 150.0,
        "variable name field is only {variable} wide"
    );
    drop(h);
    std::fs::remove_dir_all(dir).expect("clean up");
}

#[test]
fn names_get_a_share_of_the_width_within_bounds() {
    // BUG-1
    assert!((views::name_width(600.0) - 240.0).abs() < f32::EPSILON);
    assert!((views::name_width(100.0) - 160.0).abs() < f32::EPSILON);
    assert!((views::name_width(5000.0) - 320.0).abs() < f32::EPSILON);
}

/// R-SRV-7: dragging the bottom-right corner makes the dialog bigger, on both sides.
#[test]
fn the_dialog_resizes_from_its_corner() {
    let dir = temp("env-dialog-resize");
    let (roots, state) = open(&dir, &[("C", &[])]);
    let mut dialog = EnvironmentsDialog::new(&state, roots[0].clone());
    let result = std::cell::RefCell::new(None);
    let mut h = harness(&mut dialog, &state, &result);
    h.run();
    let heading = h.get_by_label("Environments — C").rect();
    let left = heading.left();
    // The grip's centre: just outside the 860 × 560 body, in the frame's margin.
    let grip = egui::pos2(left + 860.0 + 11.0, heading.top() + 560.0 + 11.0);
    let pointer = |h: &mut egui_kittest::Harness<'_>, pos, pressed: Option<bool>| {
        h.input_mut().events.push(egui::Event::PointerMoved(pos));
        if let Some(pressed) = pressed {
            h.input_mut().events.push(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::default(),
            });
        }
        h.run();
    };
    pointer(&mut h, grip, None);
    pointer(&mut h, grip, Some(true));
    for step in 1..=4u8 {
        pointer(&mut h, grip + egui::vec2(10.0, 5.0) * f32::from(step), None);
    }
    pointer(&mut h, grip + egui::vec2(40.0, 20.0), Some(false));
    let moved = left - h.get_by_label("Environments — C").rect().left();
    assert!(
        (moved - 40.0).abs() < 1.0,
        "the left edge moved {moved} instead of 40"
    );
    drop(h);
    std::fs::remove_dir_all(dir).expect("clean up");
}

/// R-WS-9: a read-only project (newer format) keeps its files.
#[test]
fn read_only_projects_are_not_written() {
    let dir = temp("env-dialog-read-only");
    let (_, state) = open(&dir, &[("A", &["Dev"])]);
    let mut dialog = EnvironmentsDialog::for_workspace(&state);
    dialog.projects[0].read_only = true;
    let saved = dialog.to_saved(&state).expect("valid");
    assert_eq!(saved.projects, []);
    std::fs::remove_dir_all(dir).expect("clean up");
}

/// R-WS-12: a server only the workspace has shows among a project's servers; typing a
/// URL for it gives the project a server of that name, saved with the URL.
#[test]
fn a_workspace_server_shows_in_each_project_and_can_be_given_a_url() {
    let dir = temp("env-dialog-ws-server");
    let (_, state) = open(&dir, &[("C", &["Dev"])]);
    let mut dialog = EnvironmentsDialog::for_workspace(&state);
    dialog.list[0]
        .servers
        .push(("Payments ü😀".into(), "https://pay.dev".into()));
    let result = std::cell::RefCell::new(None);
    {
        let mut h = harness(&mut dialog, &state, &result);
        h.run();
        let row = h.get_by_label("Payments ü😀").rect();
        let url_field = |h: &egui_kittest::Harness<'_>, act: &dyn Fn(&egui_kittest::Node<'_>)| {
            let field = h
                .get_all_by_role(egui::accesskit::Role::TextInput)
                .find(|n| {
                    n.rect().left() > row.right() && n.rect().y_range().contains(row.center().y)
                })
                .expect("its URL field");
            act(&field);
        };
        url_field(&h, &|n| n.focus());
        h.run();
        url_field(&h, &|n| n.type_text("h"));
        h.run();
        // Now the project's own server: the field keeps focus for the rest.
        url_field(&h, &|n| n.type_text("ttp://pay.c"));
        h.run();
        assert!(on_screen(&h, "http://pay.c"), "all of it, focus kept");
        h.get_by_label("Save").click();
        h.run();
    }
    let saved = result.borrow_mut().take().expect("saved");
    let c = &saved.projects[0].1;
    let server = c
        .manifest
        .servers
        .iter()
        .find(|s| s.name == "Payments ü😀")
        .expect("added to the project");
    assert_eq!(c.environments[0].servers[&server.id], "http://pay.c");
    std::fs::remove_dir_all(dir).expect("clean up");
}

/// R-WS-12: a server added in one environment is listed in every other, and saving
/// gives it to every project, also without a URL anywhere yet.
#[test]
fn a_server_added_in_one_environment_is_in_all_and_in_every_project() {
    let dir = temp("env-dialog-shared-servers");
    let (_, state) = open(&dir, &[("A", &["Dev", "Staging"]), ("B ü😀", &[])]);
    let mut dialog = EnvironmentsDialog::for_workspace(&state);
    {
        let result = std::cell::RefCell::new(None);
        let mut h = harness(&mut dialog, &state, &result);
        h.run();
        // After the environment's name: no shared servers yet.
        h.get_all_by_role(egui::accesskit::Role::TextInput)
            .nth(1)
            .expect("the add-server field")
            .focus();
        h.run();
        h.get_all_by_role(egui::accesskit::Role::TextInput)
            .nth(1)
            .expect("the add-server field")
            .type_text("Payments ü😀");
        h.run();
        h.key_press(egui::Key::Enter);
        h.run();
    }
    assert_eq!(
        dialog.list[1].servers,
        [("Payments ü😀".to_owned(), String::new())],
        "Staging lists it too"
    );
    dialog.list[1].servers[0].1 = "https://pay.stg".into();

    let saved = dialog.to_saved(&state).expect("valid");
    assert!(saved.list[0].servers.is_empty(), "no URL in Dev");
    assert_eq!(saved.list[1].servers["Payments ü😀"], "https://pay.stg");
    for (_, project) in &saved.projects {
        assert!(
            project
                .manifest
                .servers
                .iter()
                .any(|s| s.is_named("payments ü😀")),
            "every project has it"
        );
    }
    std::fs::remove_dir_all(dir).expect("clean up");
}
