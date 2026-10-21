//! The sidebar's menus: "+ Add" (R-WS-3, R-COL-15..18), a project's menu and a
//! request's or folder's menu, with only the entries that work here (R-SET-13).

use std::path::PathBuf;

use egui::{RichText, Ui};
use tortuga_core::Collection;
use tortuga_core::collection::Direction;
use tortuga_core::import::ImportKind;
use tortuga_core::settings::ImporterSettings;

use super::{MenuOffers, Rename, SidebarAction, Tree};
use crate::components::{ENVIRONMENT_KINDS, environments_entry};
use crate::state::ItemKey;
use crate::theme;

/// The "+ Add" menu: new, existing and imported projects; only the importers that are
/// on (R-COL-18).
pub(super) fn add_menu(ui: &mut Ui, importers: &ImporterSettings) -> Option<SidebarAction> {
    let entries = [
        (
            "New project…",
            "Create a project in a folder of your choice",
            SidebarAction::NewProject,
        ),
        (
            "Add existing projects…",
            "Find the projects in a folder or git repository",
            SidebarAction::AddProjects,
        ),
        (
            "Import OpenAPI…",
            "Create a project from an OpenAPI 3 description (JSON or YAML)",
            SidebarAction::Import(ImportKind::OpenApi),
        ),
        (
            "Import Postman collection…",
            "Create a project from a Postman collection (v2.0 or v2.1 JSON)",
            SidebarAction::Import(ImportKind::Postman),
        ),
        (
            "Import Insomnia export…",
            "Create a project from an Insomnia export (v4 JSON or v5 YAML)",
            SidebarAction::Import(ImportKind::Insomnia),
        ),
        (
            "Import .http file…",
            "Create a project from a .http file (VS Code REST Client, JetBrains HTTP Client)",
            SidebarAction::Import(ImportKind::Http),
        ),
        (
            "Import Bruno collection…",
            "Create a project from a Bruno collection (the folder with bruno.json)",
            SidebarAction::Import(ImportKind::Bruno),
        ),
    ];
    let mut action = None;
    let shown = entries.into_iter().filter(|(_, _, entry)| match entry {
        SidebarAction::Import(kind) => importers.enabled(*kind),
        _ => true,
    });
    for (label, hover, entry) in shown {
        if ui.button(label).on_hover_text(hover).clicked() {
            action = Some(entry);
        }
    }
    action
}

/// What the user picked in a `…` or right-click menu.
pub(super) enum MenuChoice {
    Action(SidebarAction),
    Rename,
}

pub(super) fn apply(tree: &mut Tree<'_>, choice: Option<MenuChoice>, key: &ItemKey, name: &str) {
    match choice {
        Some(MenuChoice::Action(a)) => tree.actions.push(a),
        Some(MenuChoice::Rename) => {
            tree.st.rename = Some(Rename::new(key.clone(), name.to_owned()));
        }
        None => {}
    }
}

/// Actions for a collection.
pub(super) fn collection_menu(
    ui: &mut Ui,
    c: &Collection,
    cookie_count: usize,
    offers: MenuOffers,
    // Set when the project doesn't follow the workspace's environment (R-WS-6).
    environments: Option<&[(String, String)]>,
    // The port of its running mock server (R-MOCK-2).
    mock_port: Option<u16>,
) -> Option<MenuChoice> {
    let root = &c.root;
    let mut choice = None;
    let mut item = |ui: &mut Ui, label: String, pick: &dyn Fn() -> MenuChoice| {
        if ui.button(label).clicked() {
            choice = Some(pick());
        }
    };
    if !c.read_only {
        item(ui, "New request".into(), &|| {
            MenuChoice::Action(SidebarAction::NewRequest {
                root: root.clone(),
                parent: PathBuf::new(),
            })
        });
        item(ui, "New folder".into(), &|| {
            MenuChoice::Action(SidebarAction::NewFolder {
                root: root.clone(),
                parent: PathBuf::new(),
            })
        });
        item(ui, "Rename".into(), &|| MenuChoice::Rename);
    }
    let mut picked_environment = None;
    if let Some(envs) = environments {
        ui.menu_button("Use environment", |ui| {
            for (id, name) in envs {
                if ui.button(name).clicked() {
                    picked_environment = Some(id.clone());
                }
            }
        });
    }
    let environments = format!("Environments ({})…", c.environments.len());
    item(ui, environments, &|| {
        MenuChoice::Action(SidebarAction::EditEnvironments(root.clone()))
    });
    for kind in ENVIRONMENT_KINDS {
        if offers.environments(kind) && !c.read_only {
            item(ui, environments_entry(kind).into(), &|| {
                MenuChoice::Action(SidebarAction::ImportEnvironments(root.clone(), kind))
            });
        }
    }
    item(ui, "Defaults (headers, auth, requests)…".into(), &|| {
        MenuChoice::Action(SidebarAction::EditDefaults(root.clone()))
    });
    if offers.features.docs {
        item(ui, "Description…".into(), &|| {
            MenuChoice::Action(SidebarAction::ProjectDescription(root.clone()))
        });
    }
    if offers.features.workflows && !c.read_only {
        item(ui, "New workflow…".into(), &|| {
            MenuChoice::Action(SidebarAction::NewWorkflow(root.clone()))
        });
    }
    match mock_port {
        // Turned off (R-SET-13); one that is running can still be stopped.
        None if !offers.features.mock => {}
        None => item(ui, "Start mock server".into(), &|| {
            MenuChoice::Action(SidebarAction::StartMock(root.clone()))
        }),
        Some(port) => {
            item(ui, format!("Mock server (port {port})…"), &|| {
                MenuChoice::Action(SidebarAction::ShowMock(root.clone()))
            });
            item(ui, "Stop mock server".into(), &|| {
                MenuChoice::Action(SidebarAction::StopMock(root.clone()))
            });
        }
    }
    item(ui, "Runtime variables…".into(), &|| {
        MenuChoice::Action(SidebarAction::RuntimeVariables(root.clone()))
    });
    item(ui, format!("Cookies ({cookie_count})…"), &|| {
        MenuChoice::Action(SidebarAction::ShowCookies(root.clone()))
    });
    item(ui, "Reload from disk".into(), &|| {
        MenuChoice::Action(SidebarAction::Reload(root.clone()))
    });
    // R-GIT-13: commit, pull and push happen outside the app.
    if offers.open_folders {
        item(ui, "Open in file manager".into(), &|| {
            MenuChoice::Action(SidebarAction::OpenInFileManager(root.clone()))
        });
        item(ui, "Open in terminal".into(), &|| {
            MenuChoice::Action(SidebarAction::OpenInTerminal(root.clone()))
        });
    }
    item(ui, "Remove from workspace".into(), &|| {
        MenuChoice::Action(SidebarAction::Close(root.clone()))
    });
    if let Some(id) = picked_environment {
        return Some(MenuChoice::Action(SidebarAction::SetProjectEnvironment {
            root: root.clone(),
            id,
        }));
    }
    choice
}

/// Actions for a request or folder.
pub(super) fn item_menu(
    ui: &mut Ui,
    key: &ItemKey,
    is_folder: bool,
    docs: bool,
) -> Option<MenuChoice> {
    let act = |a: SidebarAction| Some(MenuChoice::Action(a));
    let mut choice = None;
    if is_folder {
        if ui.button("New request").clicked() {
            choice = act(SidebarAction::NewRequest {
                root: key.root.clone(),
                parent: key.path.clone(),
            });
        }
        if ui.button("New folder").clicked() {
            choice = act(SidebarAction::NewFolder {
                root: key.root.clone(),
                parent: key.path.clone(),
            });
        }
        if ui
            .button("Folder defaults…")
            .on_hover_text("Headers, auth and variables every request in this folder inherits")
            .clicked()
        {
            choice = act(SidebarAction::EditFolderDefaults(key.clone()));
        }
        if docs
            && ui
                .button("Description…")
                .on_hover_text("What this folder holds, in Markdown")
                .clicked()
        {
            choice = act(SidebarAction::FolderDescription(key.clone()));
        }
    }
    if ui.button("Rename").clicked() {
        choice = Some(MenuChoice::Rename);
    }
    if ui.button("Duplicate").clicked() {
        choice = act(SidebarAction::Duplicate(key.clone()));
    }
    if ui.button("Move up").clicked() {
        choice = act(SidebarAction::MoveBy(key.clone(), Direction::Up));
    }
    if ui.button("Move down").clicked() {
        choice = act(SidebarAction::MoveBy(key.clone(), Direction::Down));
    }
    if ui.button("Move to…").clicked() {
        choice = act(SidebarAction::MoveTo(key.clone()));
    }
    let danger = theme::palette(ui).fail;
    if ui.button(RichText::new("Delete…").color(danger)).clicked() {
        choice = act(SidebarAction::Delete(key.clone()));
    }
    choice
}

pub(super) fn menu_button(
    ui: &mut Ui,
    menu: impl FnOnce(&mut Ui) -> Option<MenuChoice>,
) -> Option<MenuChoice> {
    let (response, inner) =
        egui::containers::menu::MenuButton::from_button(theme::ghost_button("…")).ui(ui, menu);
    response.on_hover_text("Actions");
    inner.and_then(|i| i.inner)
}

pub(super) fn context_menu(
    row: &egui::Response,
    menu: impl FnOnce(&mut Ui) -> Option<MenuChoice>,
) -> Option<MenuChoice> {
    let mut choice = None;
    row.context_menu(|ui| choice = menu(ui));
    choice
}
