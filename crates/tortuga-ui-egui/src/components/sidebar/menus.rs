//! The sidebar's menus: "+ Add" (R-WS-3, R-COL-15..18), a project's menu and a
//! request's or folder's menu, with only the entries that work here (R-SET-13).

use std::path::PathBuf;

use egui::{AtomExt as _, RichText, Ui};
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
            t!("cmd-new-project"),
            t!("add-new-project-hint"),
            SidebarAction::NewProject,
        ),
        (
            t!("cmd-add-projects"),
            t!("add-existing-hint"),
            SidebarAction::AddProjects,
        ),
        (
            t!("cmd-import-openapi"),
            t!("add-openapi-hint"),
            SidebarAction::Import(ImportKind::OpenApi),
        ),
        (
            t!("cmd-import-postman"),
            t!("add-postman-hint"),
            SidebarAction::Import(ImportKind::Postman),
        ),
        (
            t!("cmd-import-insomnia"),
            t!("add-insomnia-hint"),
            SidebarAction::Import(ImportKind::Insomnia),
        ),
        (
            t!("cmd-import-http"),
            t!("add-http-hint"),
            SidebarAction::Import(ImportKind::Http),
        ),
        (
            t!("cmd-import-bruno"),
            t!("add-bruno-hint"),
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
    // The port of its running mock server (R-MOCK-2).
    mock_port: Option<u16>,
) -> Option<MenuChoice> {
    let root = &c.root;
    let mut choice = None;
    let mut item = |ui: &mut Ui, icon: &str, label: String, pick: &dyn Fn() -> MenuChoice| {
        if entry(ui, icon, &label).clicked() {
            choice = Some(pick());
        }
    };
    if !c.read_only {
        item(ui, "🗋", t!("cmd-new-request"), &|| {
            MenuChoice::Action(SidebarAction::NewRequest {
                root: root.clone(),
                parent: PathBuf::new(),
            })
        });
        item(ui, "🗀", t!("menu-new-folder"), &|| {
            MenuChoice::Action(SidebarAction::NewFolder {
                root: root.clone(),
                parent: PathBuf::new(),
            })
        });
        item(ui, "✏", t!("rename"), &|| MenuChoice::Rename);
    }
    item(ui, "🌐", t!("workspace-environments"), &|| {
        MenuChoice::Action(SidebarAction::EditEnvironments(root.clone()))
    });
    for kind in ENVIRONMENT_KINDS {
        if offers.environments(kind) && !c.read_only {
            item(ui, "📥", environments_entry(kind), &|| {
                MenuChoice::Action(SidebarAction::ImportEnvironments(root.clone(), kind))
            });
        }
    }
    item(ui, "⚙", t!("menu-project-defaults"), &|| {
        MenuChoice::Action(SidebarAction::EditDefaults(root.clone()))
    });
    if offers.features.docs {
        item(ui, "📖", t!("menu-description"), &|| {
            MenuChoice::Action(SidebarAction::ProjectDescription(root.clone()))
        });
    }
    if offers.features.workflows && !c.read_only {
        item(ui, "🔀", t!("cmd-new-workflow"), &|| {
            MenuChoice::Action(SidebarAction::NewWorkflow(root.clone()))
        });
    }
    match mock_port {
        // Turned off (R-SET-13); one that is running can still be stopped.
        None if !offers.features.mock => {}
        None => item(ui, "▶", t!("cmd-start-mock"), &|| {
            MenuChoice::Action(SidebarAction::StartMock(root.clone()))
        }),
        Some(port) => {
            item(ui, "🎭", t!("menu-mock-server", port = port), &|| {
                MenuChoice::Action(SidebarAction::ShowMock(root.clone()))
            });
            item(ui, "⏹", t!("cmd-stop-mock"), &|| {
                MenuChoice::Action(SidebarAction::StopMock(root.clone()))
            });
        }
    }
    item(ui, "🏷", t!("cmd-runtime-variables"), &|| {
        MenuChoice::Action(SidebarAction::RuntimeVariables(root.clone()))
    });
    item(
        ui,
        "🍪",
        t!("menu-cookies", count = cookie_count),
        &|| MenuChoice::Action(SidebarAction::ShowCookies(root.clone())),
    );
    item(ui, "🔄", t!("menu-reload"), &|| {
        MenuChoice::Action(SidebarAction::Reload(root.clone()))
    });
    // R-GIT-13: commit, pull and push happen outside the app.
    if offers.open_folders {
        item(ui, "📂", t!("menu-file-manager"), &|| {
            MenuChoice::Action(SidebarAction::OpenInFileManager(root.clone()))
        });
        item(ui, "⌨", t!("menu-terminal"), &|| {
            MenuChoice::Action(SidebarAction::OpenInTerminal(root.clone()))
        });
    }
    item(ui, "🗙", t!("project-remove-from-workspace"), &|| {
        MenuChoice::Action(SidebarAction::Close(root.clone()))
    });
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
        if entry(ui, "🗋", &t!("cmd-new-request")).clicked() {
            choice = act(SidebarAction::NewRequest {
                root: key.root.clone(),
                parent: key.path.clone(),
            });
        }
        if entry(ui, "🗀", &t!("menu-new-folder")).clicked() {
            choice = act(SidebarAction::NewFolder {
                root: key.root.clone(),
                parent: key.path.clone(),
            });
        }
        if entry(ui, "⚙", &t!("menu-folder-defaults"))
            .on_hover_text(t!("menu-folder-defaults-hint"))
            .clicked()
        {
            choice = act(SidebarAction::EditFolderDefaults(key.clone()));
        }
        if docs
            && entry(ui, "📖", &t!("menu-description"))
                .on_hover_text(t!("menu-folder-description-hint"))
                .clicked()
        {
            choice = act(SidebarAction::FolderDescription(key.clone()));
        }
    }
    if entry(ui, "✏", &t!("rename")).clicked() {
        choice = Some(MenuChoice::Rename);
    }
    if entry(ui, "🗐", &t!("menu-duplicate")).clicked() {
        choice = act(SidebarAction::Duplicate(key.clone()));
    }
    if entry(ui, "⬆", &t!("move-up")).clicked() {
        choice = act(SidebarAction::MoveBy(key.clone(), Direction::Up));
    }
    if entry(ui, "⬇", &t!("move-down")).clicked() {
        choice = act(SidebarAction::MoveBy(key.clone(), Direction::Down));
    }
    if entry(ui, "↗", &t!("menu-move-to")).clicked() {
        choice = act(SidebarAction::MoveTo(key.clone()));
    }
    if entry(ui, "📋", &t!("menu-copy-to-project"))
        .on_hover_text(t!("menu-copy-to-project-hint"))
        .clicked()
    {
        choice = act(SidebarAction::CopyTo(key.clone()));
    }
    let danger = theme::palette(ui).fail;
    let delete = entry_text(
        ui,
        RichText::new("🗑").color(danger),
        RichText::new(t!("delete-ellipsis")).color(danger),
    );
    if delete.clicked() {
        choice = act(SidebarAction::Delete(key.clone()));
    }
    choice
}

/// Width of the icon column in the `…` menus, so the labels line up.
const ICON_WIDTH: f32 = 18.0;

/// A menu entry with its icon left of the label.
fn entry(ui: &mut Ui, icon: &str, label: &str) -> egui::Response {
    entry_text(ui, RichText::new(icon), RichText::new(label))
}

fn entry_text(ui: &mut Ui, icon: RichText, label: RichText) -> egui::Response {
    let height = ui.text_style_height(&egui::TextStyle::Button);
    let icon = egui::Atom::from(icon)
        .atom_size(egui::vec2(ICON_WIDTH, height))
        .atom_align(egui::Align2::CENTER_CENTER);
    ui.button((icon, label))
}

pub(super) fn menu_button(
    ui: &mut Ui,
    menu: impl FnOnce(&mut Ui) -> Option<MenuChoice>,
) -> Option<MenuChoice> {
    let (response, inner) =
        egui::containers::menu::MenuButton::from_button(theme::ghost_button("…")).ui(ui, menu);
    response.on_hover_text(t!("menu-actions"));
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
