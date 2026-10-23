//! What quick open and the command palette list, and running what was chosen
//! (R-COL-9, R-COL-10).

use std::path::{Path, PathBuf};

use tortuga_core::import::ImportKind;
use tortuga_core::settings::{Feature, ImporterSettings};
use tortuga_core::{Item, Request};

use super::{Modal, Selection, TortugaUi, WorkspaceAction};
use crate::components::{
    DefaultsDialog, DialogResult, ENVIRONMENT_KINDS, EnvironmentsDialog, Palette, PaletteItem,
    SettingsDialog, environments_entry,
};
use crate::state::ItemKey;

/// How many recently opened requests quick open puts first.
const RECENT: usize = 20;

/// An entry chosen in the palette.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum PaletteChoice {
    Open(Selection),
    Run(Command),
}

/// The actions the command palette offers (R-COL-10).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Command {
    Send,
    /// The shown request to each of its project's environments (R-SRV-10).
    SendEverywhere,
    Save,
    Revert,
    CopyCurl,
    QuickOpen,
    /// A new request next to the selected one (or at the top of the first project).
    NewRequest,
    Environment(String),
    Workspace(usize),
    Environments(PathBuf),
    ProjectDefaults(PathBuf),
    Settings,
    /// Look for a new version now and show the answer on Settings → About (R-SET-15).
    CheckForUpdates,
    History,
    /// Hide or show the sidebar (R-COL-26).
    ToggleSidebar,
    /// The keyboard shortcut sheet (R-COL-27).
    Shortcuts,
    /// Workflows (R-FLOW-2, R-FLOW-3) and runtime variables (R-TEST-5).
    RunWorkflow(crate::state::WorkflowKey),
    NewWorkflow(PathBuf),
    RuntimeVars(Option<PathBuf>),
    /// The workspace's variables (R-WS-8).
    WorkspaceVariables,
    /// The workspace's headers, auth and request settings (R-SET-1, R-SET-2).
    WorkspaceDefaults,
    /// The environments every project has (R-WS-9).
    WorkspaceEnvironments,
    AddProjects,
    NewProject,
    Import(ImportKind),
    /// Postman (R-COL-20) or HTTP Client (R-COL-29) environments into a project.
    ImportEnvironments(PathBuf, ImportKind),
    /// A project's mock server (R-MOCK-2, R-MOCK-4).
    StartMock(PathBuf),
    StopMock(PathBuf),
    ShowMock(PathBuf),
    /// Send requests to the running mocks (D45).
    MockEnvironment,
}

/// Adding, creating and importing projects (R-WS-3, R-COL-15, R-COL-16, R-COL-17,
/// R-COL-31, R-COL-32), with
/// only the importers that are on (R-COL-18).
fn project_commands(importers: &ImporterSettings) -> Vec<PaletteItem<PaletteChoice>> {
    let run = |label: &str, command: Command| PaletteItem::new(label, PaletteChoice::Run(command));
    [
        run("Add existing projects…", Command::AddProjects),
        run("New project…", Command::NewProject),
        run("Import OpenAPI…", Command::Import(ImportKind::OpenApi))
            .keywords("swagger spec yaml json"),
        run(
            "Import Postman collection…",
            Command::Import(ImportKind::Postman),
        )
        .keywords("postman collection json"),
        run(
            "Import Insomnia export…",
            Command::Import(ImportKind::Insomnia),
        )
        .keywords("insomnia kong export yaml json"),
        run("Import .http file…", Command::Import(ImportKind::Http))
            .keywords("http rest client jetbrains requests"),
        run(
            "Import Bruno collection…",
            Command::Import(ImportKind::Bruno),
        )
        .keywords("bruno bru collection folder"),
    ]
    .into_iter()
    .filter(|item| match &item.value {
        PaletteChoice::Run(Command::Import(kind)) => importers.enabled(*kind),
        _ => true,
    })
    .collect()
}

/// The workspace's environments (R-WS-9), variables (R-WS-8) and defaults (R-SET-1, R-SET-2).
fn workspace_commands(workspace: &str) -> [PaletteItem<PaletteChoice>; 3] {
    let run = |label: &str, command: Command| PaletteItem::new(label, PaletteChoice::Run(command));
    [
        run("Workspace environments…", Command::WorkspaceEnvironments)
            .detail(workspace)
            .keywords("staging production servers urls"),
        run("Workspace variables…", Command::WorkspaceVariables)
            .detail(workspace)
            .keywords("shared captured secret"),
        run("Workspace defaults…", Command::WorkspaceDefaults)
            .detail(workspace)
            .keywords("headers auth timeout redirects cookies settings"),
    ]
}

impl TortugaUi {
    /// Remembers an opened request for quick open (R-COL-9).
    pub(super) fn remember_recent(&mut self, key: &ItemKey) {
        self.recent.retain(|k| k != key);
        self.recent.insert(0, key.clone());
        self.recent.truncate(RECENT);
    }

    /// Quick open: the scratch request, then recent requests, then every request in tree order.
    pub(super) fn request_items(&self) -> Vec<PaletteItem<PaletteChoice>> {
        let mut all = Vec::new();
        for c in &self.state.collections {
            collect(&c.items, &c.root, c.name(), "", &mut all);
        }
        let rank = |k: &ItemKey| {
            self.recent
                .iter()
                .position(|r| r == k)
                .unwrap_or(usize::MAX)
        };
        // Stable: the tree order stays among requests that weren't opened.
        all.sort_by_key(|(key, _)| rank(key));
        let mut items = vec![
            PaletteItem::new("Scratch request", PaletteChoice::Open(None))
                .detail("Not saved anywhere"),
        ];
        items.extend(all.into_iter().map(|(key, item)| item.value_for(key)));
        items
    }

    /// Switching the workspace's environment (R-WS-6), and to the mocks while one runs
    /// (R-MOCK-4).
    fn environment_items(&self) -> Vec<PaletteItem<PaletteChoice>> {
        let run =
            |label: &str, command: Command| PaletteItem::new(label, PaletteChoice::Run(command));
        let in_mock = self.state.mocks.in_use();
        let shown = self.state.chosen_environment_name();
        let mut items: Vec<_> = self
            .state
            .environment_names()
            .into_iter()
            .map(|name| {
                let current = !in_mock && shown.as_ref() == Some(&name);
                run(&format!("Environment: {name}"), Command::Environment(name))
                    .hint(if current { "current" } else { "" })
                    .keywords("switch")
            })
            .collect();
        if !self.state.mocks.ports().is_empty() {
            items.push(
                run("Environment: Mock", Command::MockEnvironment)
                    .hint(if in_mock { "current" } else { "" })
                    .keywords("switch mock server"),
            );
        }
        items
    }

    /// A project's mock server commands (R-MOCK-2, R-MOCK-4).
    fn mock_items(&self, c: &tortuga_core::Collection) -> Vec<PaletteItem<PaletteChoice>> {
        let run =
            |label: &str, command: Command| PaletteItem::new(label, PaletteChoice::Run(command));
        let root = &c.root;
        let items = if self.state.mocks.port(root).is_some() {
            vec![
                run("Mock server…", Command::ShowMock(root.clone())),
                run("Stop mock server", Command::StopMock(root.clone())),
            ]
        } else if self.state.settings.features.enabled(Feature::Examples) {
            vec![run("Start mock server", Command::StartMock(root.clone()))]
        } else {
            // Turned off (R-SET-13); one that is running can still be stopped.
            Vec::new()
        };
        items
            .into_iter()
            .map(|item| item.detail(c.name()).keywords("mock examples"))
            .collect()
    }

    /// The project the palette's commands are for (R-COL-25): the open workflow's, else
    /// the selected request's, else the first one.
    fn current_project(&self) -> Option<&Path> {
        self.workflows
            .open
            .as_ref()
            .map(|w| w.root.as_path())
            .or_else(|| self.selection.as_ref().map(|k| k.root.as_path()))
            .or_else(|| self.state.collections.first().map(|c| c.root.as_path()))
    }

    /// The commands for project `c`: its workflows, environments, defaults and mock.
    fn project_items(&self, c: &tortuga_core::Collection) -> Vec<PaletteItem<PaletteChoice>> {
        let run =
            |label: &str, command: Command| PaletteItem::new(label, PaletteChoice::Run(command));
        let workflows = self.state.settings.features.enabled(Feature::Workflows);
        let mut items = Vec::new();
        for w in c.workflows.iter().filter(|_| workflows) {
            let key = crate::state::WorkflowKey {
                root: c.root.clone(),
                id: w.id.clone(),
            };
            items.push(
                run(
                    &format!("Run workflow: {}", w.name),
                    Command::RunWorkflow(key),
                )
                .detail(c.name())
                .keywords("test"),
            );
        }
        if workflows && !c.read_only {
            items.push(run("New workflow…", Command::NewWorkflow(c.root.clone())).detail(c.name()));
        }
        items.push(
            run(
                "Environments, servers and variables…",
                Command::Environments(c.root.clone()),
            )
            .detail(c.name()),
        );
        for kind in ENVIRONMENT_KINDS {
            if !c.read_only
                && self.state.settings.importers.enabled(kind)
                && self.state.host.can_pick_folders()
            {
                items.push(
                    run(
                        environments_entry(kind),
                        Command::ImportEnvironments(c.root.clone(), kind),
                    )
                    .detail(c.name())
                    .keywords(match kind {
                        ImportKind::Postman => "postman environment json",
                        ImportKind::Http => "http client rest env json",
                        ImportKind::Bruno => "bruno environment bru",
                        ImportKind::OpenApi | ImportKind::Insomnia => "",
                    }),
                );
            }
        }
        items.push(
            run(
                "Project defaults…",
                Command::ProjectDefaults(c.root.clone()),
            )
            .detail(c.name())
            .keywords("headers auth timeout redirects cookies settings"),
        );
        items.extend(self.mock_items(c));
        items
    }

    /// The command palette's commands; those that don't apply right now are left out.
    pub(super) fn command_items(&self) -> Vec<PaletteItem<PaletteChoice>> {
        let run =
            |label: &str, command: Command| PaletteItem::new(label, PaletteChoice::Run(command));
        let mut items = vec![
            run("Send request", Command::Send).hint("Ctrl+Enter"),
            run("Send to every environment", Command::SendEverywhere)
                .hint("Ctrl+Shift+Enter")
                .keywords("compare environments all"),
            run("Quick open: go to a request", Command::QuickOpen).hint("Ctrl+P"),
        ];
        let key = self.selection.clone();
        let dirty = key
            .as_ref()
            .is_some_and(|k| self.is_dirty(k, &self.current));
        if key.is_none() || dirty {
            items.push(run("Save", Command::Save).hint("Ctrl+S"));
        }
        if dirty {
            items.push(run("Revert changes", Command::Revert));
        }
        items.push(run("Copy as curl", Command::CopyCurl));
        if !self.state.collections.is_empty() {
            items.push(run("New request", Command::NewRequest));
        }
        items.extend(self.environment_items());
        for (i, ws) in self.state.local.workspaces.iter().enumerate() {
            if i != self.state.local.active_workspace {
                items.push(
                    run(&format!("Workspace: {}", ws.name), Command::Workspace(i))
                        .keywords("switch"),
                );
            }
        }
        let current = self.current_project();
        for c in &self.state.collections {
            // Other projects' commands only when the text names the project (R-COL-25).
            let elsewhere = Some(c.root.as_path()) != current;
            items.extend(self.project_items(c).into_iter().map(|mut item| {
                item.elsewhere = elsewhere;
                item
            }));
        }
        let root = self.selection.as_ref().map(|k| k.root.clone());
        let place = root
            .as_ref()
            .and_then(|r| self.state.collection(r))
            .map_or_else(|| "Scratch request".to_owned(), |c| c.name().to_owned());
        items.push(
            run("Runtime variables…", Command::RuntimeVars(root))
                .detail(place)
                .keywords("captured clear"),
        );
        items.extend(workspace_commands(&self.state.workspace().name));
        items.push(run("Settings…", Command::Settings).keywords("preferences options"));
        if self.state.host.checks_for_updates() {
            items.push(
                run("Check for updates", Command::CheckForUpdates)
                    .keywords("new version release upgrade about"),
            );
        }
        let sidebar = if self.state.sidebar_hidden() {
            "Show the sidebar"
        } else {
            "Hide the sidebar"
        };
        items.push(
            run(sidebar, Command::ToggleSidebar)
                .hint("Ctrl+B")
                .keywords("projects panel toggle"),
        );
        items.push(
            run("Keyboard shortcuts", Command::Shortcuts)
                .hint("F1")
                .keywords("keys help"),
        );
        if self.state.host.keeps_history() {
            items.push(run("Show history", Command::History).keywords("sent past responses"));
        }
        if self.state.host.can_pick_folders() {
            items.extend(project_commands(&self.state.settings.importers));
        }
        items
    }

    /// Shows quick open or the command palette. Returns whether it stays open.
    pub(super) fn show_palette(&mut self, ctx: &egui::Context, palette: &mut Palette) -> bool {
        let items = if palette.shows_commands() {
            self.command_items()
        } else {
            self.request_items()
        };
        match palette.show(ctx, &items) {
            DialogResult::Done(choice) => {
                self.on_palette(choice, ctx);
                false
            }
            result => result == DialogResult::Open,
        }
    }

    fn on_palette(&mut self, choice: PaletteChoice, ctx: &egui::Context) {
        match choice {
            PaletteChoice::Open(selection) => self.select(selection),
            PaletteChoice::Run(command) => self.run_command(command, ctx),
        }
    }

    pub(super) fn run_command(&mut self, command: Command, ctx: &egui::Context) {
        match command {
            Command::Send => {
                if self.sending().is_none() {
                    self.send(ctx);
                }
            }
            Command::SendEverywhere => self.send_everywhere(ctx),
            Command::Save => self.save(),
            Command::Revert => self.revert(),
            Command::CopyCurl => self.copy_as_curl(ctx),
            Command::QuickOpen => {
                self.modal = Some(Modal::Palette(Palette::requests()));
            }
            Command::NewRequest => {
                let (root, parent) = match &self.selection {
                    Some(k) => (
                        k.root.clone(),
                        k.path.parent().map(Path::to_path_buf).unwrap_or_default(),
                    ),
                    None => match self.state.collections.first() {
                        Some(c) => (c.root.clone(), PathBuf::new()),
                        None => return,
                    },
                };
                self.on_sidebar(
                    crate::components::SidebarAction::NewRequest { root, parent },
                    ctx,
                );
            }
            Command::Environment(name) => self.state.set_workspace_environment(&name),
            Command::MockEnvironment => self.state.use_mock_environment(),
            Command::StartMock(root) => self.start_mock(&root),
            Command::StopMock(root) => self.stop_mock(&root),
            Command::ShowMock(root) => self.show_mock(root),
            Command::Workspace(i) => self.on_workspace(WorkspaceAction::Switch(i)),
            Command::Environments(root) => {
                self.modal = Some(Modal::Environments(EnvironmentsDialog::new(
                    &self.state,
                    root,
                )));
            }
            Command::ProjectDefaults(root) => {
                self.modal = Some(Modal::Defaults(DefaultsDialog::new(&self.state, root)));
            }
            Command::Settings => {
                self.modal = Some(Modal::Settings(Box::new(SettingsDialog::new(
                    &self.state.settings,
                ))));
            }
            Command::CheckForUpdates => {
                self.check_for_updates_now(ctx);
                self.modal = Some(Modal::Settings(Box::new(
                    SettingsDialog::new(&self.state.settings).on_about_tab(),
                )));
            }
            Command::History => {
                self.state.set_sidebar_hidden(false);
                self.sidebar_view = super::SidebarView::History;
            }
            Command::ToggleSidebar => self.toggle_sidebar(),
            Command::Shortcuts => self.show_shortcuts(ctx),
            Command::RunWorkflow(key) => self.run_workflow(&key, ctx),
            Command::NewWorkflow(root) => self.new_workflow_dialog(root),
            Command::RuntimeVars(root) => self.show_runtime_vars(root),
            Command::WorkspaceVariables => self.on_workspace(WorkspaceAction::Variables),
            Command::WorkspaceDefaults => self.on_workspace(WorkspaceAction::Defaults),
            Command::WorkspaceEnvironments => self.on_workspace(WorkspaceAction::Environments),
            Command::AddProjects => self.add_projects(),
            Command::NewProject => self.new_project(),
            Command::Import(kind) => self.import_file(kind),
            Command::ImportEnvironments(root, kind) => self.import_environments(root, kind),
        }
    }
}

/// A request found in the tree, for quick open.
struct Found {
    label: String,
    detail: String,
    request: Request,
}

impl Found {
    fn value_for(self, key: ItemKey) -> PaletteItem<PaletteChoice> {
        let url = self.request.display_url();
        PaletteItem::new(self.label, PaletteChoice::Open(Some(key)))
            .method(self.request.method)
            .detail(self.detail)
            .keywords(url)
    }
}

/// Every request below `items`, with "Project › Folder › …" as detail.
fn collect(items: &[Item], root: &Path, trail: &str, sub: &str, out: &mut Vec<(ItemKey, Found)>) {
    let place = if sub.is_empty() {
        trail.to_owned()
    } else {
        format!("{trail} › {sub}")
    };
    for item in items {
        match item {
            Item::Request(r) => out.push((
                ItemKey::new(root, r.path.clone()),
                Found {
                    label: r.request.name.clone(),
                    detail: place.clone(),
                    request: r.request.clone(),
                },
            )),
            Item::Folder(f) => {
                let sub = if sub.is_empty() {
                    f.name.clone()
                } else {
                    format!("{sub} › {}", f.name)
                };
                collect(&f.items, root, trail, &sub, out);
            }
            Item::Broken(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// R-COL-18: importers that are off are not in the command palette.
    #[test]
    fn palette_lists_only_importers_that_are_on() {
        let mut importers = ImporterSettings::default();
        importers.set_enabled(ImportKind::Http, false);
        let labels: Vec<String> = project_commands(&importers)
            .into_iter()
            .map(|i| i.label)
            .collect();
        assert_eq!(
            labels,
            [
                "Add existing projects…",
                "New project…",
                "Import OpenAPI…",
                "Import Postman collection…",
                "Import Insomnia export…",
                "Import Bruno collection…"
            ]
        );
    }

    /// R-SET-13: features that are off leave the palette, the sidebar and the editor's
    /// tabs, except where a request already uses them.
    #[test]
    fn features_that_are_off_are_hidden() {
        use egui_kittest::kittest::Queryable;

        let (mut ui, root, key) = crate::shell::tests::setup("features-off");
        ui.create_workflow(&root, "Smoke");
        ui.select(Some(key.clone()));
        let labels = |ui: &TortugaUi| -> Vec<String> {
            ui.command_items().into_iter().map(|i| i.label).collect()
        };
        let has = |labels: &[String], label: &str| labels.iter().any(|l| l == label);
        let before = labels(&ui);
        assert!(has(&before, "Run workflow: Smoke") && has(&before, "Start mock server"));

        ui.state.update_settings(|s| {
            for feature in [Feature::Workflows, Feature::Examples, Feature::Scripts] {
                s.features.set_enabled(feature, false);
            }
        });
        let after = labels(&ui);
        for gone in ["Run workflow: Smoke", "New workflow…", "Start mock server"] {
            assert!(!has(&after, gone), "{gone} is hidden");
        }
        {
            ui.styled = false;
            let mut h = egui_kittest::Harness::builder()
                .with_size(egui::vec2(1200.0, 900.0))
                .build_ui(|egui_ui| ui.ui(egui_ui));
            h.run();
            assert!(h.query_by_label("Scripts").is_none(), "no Scripts tab");
            assert!(h.query_by_label("Examples").is_none(), "no Examples tab");
            assert!(
                h.query_by_label_contains("Workflows").is_none(),
                "no workflows"
            );
            h.get_by_label("Docs");
        }

        ui.current.scripts.tests = "test(\"ok\", () => {});".into();
        ui.styled = false;
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(1200.0, 900.0))
            .build_ui(|egui_ui| ui.ui(egui_ui));
        h.run();
        h.get_by_label("Scripts (⏺)");
        drop(h);
        std::fs::remove_dir_all(root).expect("clean up");
    }
}
