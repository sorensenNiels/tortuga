//! The sidebar's menus: "+ Add" (R-WS-3, R-COL-15..18), a project's menu and a
//! request's or folder's menu, with only the entries that work here (R-SET-13).

use std::path::PathBuf;

use egui::{AtomExt as _, RichText, Ui};
use tortuga_core::collection::Direction;
use tortuga_core::import::ImportKind;
use tortuga_core::settings::ImporterSettings;
use tortuga_core::{Collection, Item};

use super::{MenuOffers, Rename, ShownFeatures, SidebarAction, Tree};
use crate::components::{ENVIRONMENT_KINDS, environments_entry};
use crate::state::ItemKey;
use crate::theme;

/// The "+ Add" menu: new and existing projects, and under "More" the importers that are
/// on (R-COL-18, R-COL-42).
pub(super) fn add_menu(ui: &mut Ui, importers: &ImporterSettings) -> Option<SidebarAction> {
    let mut action = None;
    let mut button = |ui: &mut Ui, label: String, hover: String, entry: SidebarAction| {
        if ui.button(label).on_hover_text(hover).clicked() {
            action = Some(entry);
        }
    };
    button(
        ui,
        t!("cmd-new-project"),
        t!("add-new-project-hint"),
        SidebarAction::NewProject,
    );
    button(
        ui,
        t!("cmd-add-projects"),
        t!("add-existing-hint"),
        SidebarAction::AddProjects,
    );
    let imports: Vec<_> = [
        (
            ImportKind::OpenApi,
            t!("cmd-import-openapi"),
            t!("add-openapi-hint"),
        ),
        (
            ImportKind::Postman,
            t!("cmd-import-postman"),
            t!("add-postman-hint"),
        ),
        (
            ImportKind::Insomnia,
            t!("cmd-import-insomnia"),
            t!("add-insomnia-hint"),
        ),
        (ImportKind::Http, t!("cmd-import-http"), t!("add-http-hint")),
        (
            ImportKind::Bruno,
            t!("cmd-import-bruno"),
            t!("add-bruno-hint"),
        ),
    ]
    .into_iter()
    .filter(|(kind, _, _)| importers.enabled(*kind))
    .collect();
    if !imports.is_empty() {
        ui.menu_button(t!("menu-more"), |ui| {
            for (kind, label, hover) in imports {
                button(ui, label, hover, SidebarAction::Import(kind));
            }
        });
    }
    action
}

/// What the user picked in a `…` or right-click menu.
pub(super) enum MenuChoice {
    Action(SidebarAction),
    Rename,
    /// Fold or unfold every folder below the row (R-COL-43).
    Fold(Fold),
    /// Sort the items below the row (R-COL-44).
    Sort,
}

/// "Collapse all" or "Expand all" below a project or folder (R-COL-43).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Fold {
    Collapse,
    Expand,
}

/// Applies a menu choice on the row `key`; `below` is what the row holds and `groups` are
/// its other foldable rows (the Workflows group), for [`MenuChoice::Fold`].
pub(super) fn apply(
    tree: &mut Tree<'_>,
    choice: Option<MenuChoice>,
    key: &ItemKey,
    name: &str,
    below: &[Item],
    groups: &[ItemKey],
) {
    match choice {
        Some(MenuChoice::Action(a)) => tree.actions.push(a),
        Some(MenuChoice::Rename) => {
            tree.st.rename = Some(Rename::new(key.clone(), name.to_owned()));
        }
        Some(MenuChoice::Fold(fold)) => tree.st.fold_below(key, below, groups, fold),
        Some(MenuChoice::Sort) => tree.actions.push(SidebarAction::Sort(key.clone())),
        None => {}
    }
}

/// "Collapse all" and "Expand all" when there are folders to fold (R-COL-43), and "Sort A→Z"
/// when there is something to sort (R-COL-44).
fn arrange_entries(ui: &mut Ui, foldable: bool, sortable: bool) -> Option<MenuChoice> {
    let mut choice = None;
    if foldable {
        if entry(ui, "−", &t!("sidebar-collapse-all"))
            .on_hover_text(t!("menu-collapse-all-hint"))
            .clicked()
        {
            choice = Some(MenuChoice::Fold(Fold::Collapse));
        }
        if entry(ui, "+", &t!("sidebar-expand-all"))
            .on_hover_text(t!("menu-expand-all-hint"))
            .clicked()
        {
            choice = Some(MenuChoice::Fold(Fold::Expand));
        }
    }
    if sortable
        && entry(ui, "↓", &t!("menu-sort"))
            .on_hover_text(t!("menu-sort-hint"))
            .clicked()
    {
        choice = Some(MenuChoice::Sort);
    }
    choice
}

/// A submenu of the `…` menus, with its icon in the icon column (R-COL-45).
fn submenu(
    ui: &mut Ui,
    glyph: &str,
    label: String,
    add: impl FnOnce(&mut Ui) -> Option<MenuChoice>,
) -> Option<MenuChoice> {
    ui.menu_button((icon(ui, RichText::new(glyph)), label), add)
        .inner
        .flatten()
}

/// Actions for a collection: the frequent ones first, the rest in submenus by theme (R-COL-45).
pub(super) fn collection_menu(
    ui: &mut Ui,
    c: &Collection,
    cookie_count: usize,
    offers: MenuOffers,
    // The port of its running mock server (R-MOCK-2).
    mock_port: Option<u16>,
) -> Option<MenuChoice> {
    let root = &c.root;
    let writable = !c.read_only;
    let act = |a: SidebarAction| Some(MenuChoice::Action(a));
    let mut choice = None;
    let mut pick = |found: Option<MenuChoice>| {
        if found.is_some() {
            choice = found;
        }
    };

    if writable && entry(ui, "🗋", &t!("cmd-new-request")).clicked() {
        pick(act(SidebarAction::NewRequest {
            root: root.clone(),
            parent: PathBuf::new(),
        }));
    }
    if writable && entry(ui, "🗀", &t!("menu-new-folder")).clicked() {
        pick(act(SidebarAction::NewFolder {
            root: root.clone(),
            parent: PathBuf::new(),
        }));
    }
    if offers.features.workflows && writable && entry(ui, "🔀", &t!("cmd-new-workflow")).clicked()
    {
        pick(act(SidebarAction::NewWorkflow(root.clone())));
    }
    if offers.features.workflows
        && entry(ui, "⏵", &t!("run-requests"))
            .on_hover_text(t!("run-requests-hint"))
            .clicked()
    {
        pick(act(SidebarAction::RunRequests(
            root.clone(),
            PathBuf::new(),
        )));
    }
    ui.separator();

    if writable && entry(ui, "✏", &t!("rename")).clicked() {
        pick(Some(MenuChoice::Rename));
    }
    if entry(ui, "🌐", &t!("workspace-environments")).clicked() {
        pick(act(SidebarAction::EditEnvironments(root.clone())));
    }
    if entry(ui, "⚙", &t!("menu-project-defaults")).clicked() {
        pick(act(SidebarAction::EditDefaults(root.clone())));
    }
    if offers.features.docs && entry(ui, "📖", &t!("menu-description")).clicked() {
        pick(act(SidebarAction::ProjectDescription(root.clone())));
    }
    ui.separator();

    let foldable = has_folders(&c.items) || (offers.features.workflows && !c.workflows.is_empty());
    let sortable = writable && (c.items.len() > 1 || has_folders(&c.items));
    if foldable || sortable {
        pick(submenu(ui, "☰", t!("menu-arrange"), |ui| {
            arrange_entries(ui, foldable, sortable)
        }));
    }
    if offers.features.mock || mock_port.is_some() {
        pick(submenu(ui, "🎭", t!("menu-mock"), |ui| {
            mock_entries(ui, root, mock_port)
        }));
    }
    pick(submenu(ui, "🏷", t!("menu-runtime"), |ui| {
        runtime_entries(ui, root, cookie_count)
    }));
    let imports: Vec<_> = ENVIRONMENT_KINDS
        .into_iter()
        .filter(|kind| offers.environments(*kind) && writable)
        .collect();
    if !imports.is_empty() {
        pick(submenu(ui, "📥", t!("menu-import"), |ui| {
            import_entries(ui, root, &imports)
        }));
    }
    pick(submenu(ui, "📂", t!("menu-files"), |ui| {
        file_entries(ui, root, offers)
    }));
    ui.separator();

    if entry(ui, "🗙", &t!("project-remove-from-workspace")).clicked() {
        pick(act(SidebarAction::Close(root.clone())));
    }
    choice
}

/// Environments to import from other tools (R-COL-20, R-COL-29, R-COL-31).
fn import_entries(ui: &mut Ui, root: &std::path::Path, kinds: &[ImportKind]) -> Option<MenuChoice> {
    let mut choice = None;
    for &kind in kinds {
        if entry(ui, "📥", &environments_entry(kind)).clicked() {
            choice = Some(MenuChoice::Action(SidebarAction::ImportEnvironments(
                root.to_owned(),
                kind,
            )));
        }
    }
    choice
}

/// The project's runtime variables and cookies (R-TEST-5, R-CK-3).
fn runtime_entries(ui: &mut Ui, root: &std::path::Path, cookie_count: usize) -> Option<MenuChoice> {
    let mut choice = None;
    if entry(ui, "🏷", &t!("cmd-runtime-variables")).clicked() {
        choice = Some(MenuChoice::Action(SidebarAction::RuntimeVariables(
            root.to_owned(),
        )));
    }
    if entry(ui, "🍪", &t!("menu-cookies", count = cookie_count)).clicked() {
        choice = Some(MenuChoice::Action(SidebarAction::ShowCookies(
            root.to_owned(),
        )));
    }
    choice
}

/// The project's folder outside the app, and reading it again (R-GIT-13: commit, pull and
/// push happen outside the app).
fn file_entries(ui: &mut Ui, root: &std::path::Path, offers: MenuOffers) -> Option<MenuChoice> {
    let mut choice = None;
    if offers.open_folders {
        if entry(ui, "📂", &t!("menu-file-manager")).clicked() {
            choice = Some(MenuChoice::Action(SidebarAction::OpenInFileManager(
                root.to_owned(),
            )));
        }
        if entry(ui, "⌨", &t!("menu-terminal")).clicked() {
            choice = Some(MenuChoice::Action(SidebarAction::OpenInTerminal(
                root.to_owned(),
            )));
        }
    }
    if entry(ui, "🔄", &t!("menu-reload")).clicked() {
        choice = Some(MenuChoice::Action(SidebarAction::Reload(root.to_owned())));
    }
    choice
}

/// Start the mock server, or show and stop the one that runs (R-MOCK-2, R-MOCK-4).
fn mock_entries(ui: &mut Ui, root: &std::path::Path, mock_port: Option<u16>) -> Option<MenuChoice> {
    let act = |a: SidebarAction| Some(MenuChoice::Action(a));
    match mock_port {
        None => entry(ui, "▶", &t!("cmd-start-mock"))
            .clicked()
            .then(|| MenuChoice::Action(SidebarAction::StartMock(root.to_owned()))),
        Some(port) => {
            let mut choice = None;
            if entry(ui, "🎭", &t!("menu-mock-server", port = port)).clicked() {
                choice = act(SidebarAction::ShowMock(root.to_owned()));
            }
            if entry(ui, "⏹", &t!("cmd-stop-mock")).clicked() {
                choice = act(SidebarAction::StopMock(root.to_owned()));
            }
            choice
        }
    }
}

/// Actions for a request or folder.
pub(super) fn item_menu(
    ui: &mut Ui,
    key: &ItemKey,
    // The folder's items; `None` for a request.
    folder: Option<&[Item]>,
    features: ShownFeatures,
) -> Option<MenuChoice> {
    let is_folder = folder.is_some();
    let act = |a: SidebarAction| Some(MenuChoice::Action(a));
    let mut choice = None;
    if is_folder
        && features.workflows
        && entry(ui, "⏵", &t!("run-requests"))
            .on_hover_text(t!("run-requests-hint"))
            .clicked()
    {
        choice = act(SidebarAction::RunRequests(
            key.root.clone(),
            key.path.clone(),
        ));
    }
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
        if features.docs
            && entry(ui, "📖", &t!("menu-description"))
                .on_hover_text(t!("menu-folder-description-hint"))
                .clicked()
        {
            choice = act(SidebarAction::FolderDescription(key.clone()));
        }
    }
    if let Some(below) = folder {
        let foldable = has_folders(below);
        let sortable = below.len() > 1 || foldable;
        if foldable || sortable {
            let arranged = submenu(ui, "☰", t!("menu-arrange"), |ui| {
                arrange_entries(ui, foldable, sortable)
            });
            if let Some(found) = arranged {
                choice = Some(found);
            }
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

fn has_folders(items: &[Item]) -> bool {
    items.iter().any(|i| matches!(i, Item::Folder(_)))
}

/// Width of the icon column in the `…` menus, so the labels line up.
const ICON_WIDTH: f32 = 18.0;

/// A menu entry with its icon left of the label.
fn entry(ui: &mut Ui, icon: &str, label: &str) -> egui::Response {
    entry_text(ui, RichText::new(icon), RichText::new(label))
}

fn entry_text(ui: &mut Ui, icon_text: RichText, label: RichText) -> egui::Response {
    ui.button((icon(ui, icon_text), label))
}

/// An entry's icon, in a column of fixed width.
fn icon(ui: &Ui, icon: RichText) -> egui::Atom<'static> {
    let height = ui.text_style_height(&egui::TextStyle::Button);
    egui::Atom::from(icon)
        .atom_size(egui::vec2(ICON_WIDTH, height))
        .atom_align(egui::Align2::CENTER_CENTER)
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
