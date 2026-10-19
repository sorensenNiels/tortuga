//! What quick open and the command palette list, and running what was chosen
//! (R-COL-9, R-COL-10).

use std::path::{Path, PathBuf};

use tortuga_core::import::ImportKind;
use tortuga_core::settings::ImporterSettings;
use tortuga_core::{Item, Request};

use super::{Modal, Selection, TortugaUi, WorkspaceAction};
use crate::components::{
    DefaultsDialog, DialogResult, EnvironmentsDialog, Palette, PaletteItem, SettingsDialog,
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
    History,
    /// Workflows (R-FLOW-2, R-FLOW-3) and runtime variables (R-TEST-5).
    RunWorkflow(crate::state::WorkflowKey),
    NewWorkflow(PathBuf),
    RuntimeVars(Option<PathBuf>),
    /// The workspace's variables (R-WS-8).
    WorkspaceVariables,
    AddProjects,
    NewProject,
    Import(ImportKind),
}

/// Adding, creating and importing projects (R-WS-3, R-COL-15, R-COL-16, R-COL-17), with
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
        run("Import .http file…", Command::Import(ImportKind::Http))
            .keywords("http rest client jetbrains requests"),
    ]
    .into_iter()
    .filter(|item| match &item.value {
        PaletteChoice::Run(Command::Import(kind)) => importers.enabled(*kind),
        _ => true,
    })
    .collect()
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

    /// The command palette's commands; those that don't apply right now are left out.
    pub(super) fn command_items(&self) -> Vec<PaletteItem<PaletteChoice>> {
        let run =
            |label: &str, command: Command| PaletteItem::new(label, PaletteChoice::Run(command));
        let mut items = vec![
            run("Send request", Command::Send).hint("Ctrl+Enter"),
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
        let shown = self.state.workspace().environment.clone();
        for name in self.state.environment_names() {
            let current = shown.as_ref() == Some(&name);
            items.push(
                run(&format!("Environment: {name}"), Command::Environment(name))
                    .hint(if current { "current" } else { "" })
                    .keywords("switch"),
            );
        }
        for (i, ws) in self.state.local.workspaces.iter().enumerate() {
            if i != self.state.local.active_workspace {
                items.push(
                    run(&format!("Workspace: {}", ws.name), Command::Workspace(i))
                        .keywords("switch"),
                );
            }
        }
        for c in &self.state.collections {
            for w in &c.workflows {
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
            if !c.read_only {
                items.push(
                    run("New workflow…", Command::NewWorkflow(c.root.clone())).detail(c.name()),
                );
            }
            items.push(
                run(
                    "Environments, servers and variables…",
                    Command::Environments(c.root.clone()),
                )
                .detail(c.name()),
            );
            items.push(
                run(
                    "Project defaults…",
                    Command::ProjectDefaults(c.root.clone()),
                )
                .detail(c.name())
                .keywords("headers auth"),
            );
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
        items.push(
            run("Workspace variables…", Command::WorkspaceVariables)
                .detail(self.state.workspace().name.clone())
                .keywords("shared captured secret"),
        );
        items.push(run("Settings…", Command::Settings).keywords("preferences options"));
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

    fn run_command(&mut self, command: Command, ctx: &egui::Context) {
        match command {
            Command::Send => {
                if self.sending().is_none() {
                    self.send(ctx);
                }
            }
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
                self.modal = Some(Modal::Settings(SettingsDialog::new(&self.state.settings)));
            }
            Command::History => self.sidebar_view = super::SidebarView::History,
            Command::RunWorkflow(key) => self.run_workflow(&key, ctx),
            Command::NewWorkflow(root) => self.new_workflow_dialog(root),
            Command::RuntimeVars(root) => self.show_runtime_vars(root),
            Command::WorkspaceVariables => self.on_workspace(WorkspaceAction::Variables),
            Command::AddProjects => self.add_projects(),
            Command::NewProject => self.new_project(),
            Command::Import(kind) => self.import_file(kind),
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
                "Import Postman collection…"
            ]
        );
    }
}
