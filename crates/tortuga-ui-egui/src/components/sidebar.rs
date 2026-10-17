use std::collections::HashSet;
use std::path::{Path, PathBuf};

use egui::text::{CCursor, CCursorRange};
use egui::{Atom, Button, Key, RichText, ScrollArea, Sides, TextEdit, Ui};
use tortuga_core::collection::Direction;
use tortuga_core::git::{FileStatus, GitStatus};
use tortuga_core::workspace::ProjectRef;
use tortuga_core::{Collection, Item};

use crate::state::{AppState, ItemKey};
use crate::theme;

/// Something the user asked for in the sidebar; handled by the shell.
#[derive(Debug, Clone, PartialEq)]
pub enum SidebarAction {
    /// `None` = the scratch request.
    Select(Option<ItemKey>),
    NewRequest {
        root: PathBuf,
        parent: PathBuf,
    },
    NewFolder {
        root: PathBuf,
        parent: PathBuf,
    },
    Rename {
        key: ItemKey,
        name: String,
    },
    RenameCollection {
        root: PathBuf,
        name: String,
    },
    Duplicate(ItemKey),
    MoveBy(ItemKey, Direction),
    MoveTo(ItemKey),
    Delete(ItemKey),
    ShowError(String),
    EditEnvironments(PathBuf),
    /// Default headers and auth of a collection (R-COL-4).
    EditDefaults(PathBuf),
    /// Empty the collection's cookie jar (R-CK-3).
    ClearCookies(PathBuf),
    /// Open the collection folder outside the app (R-GIT-13).
    OpenInFileManager(PathBuf),
    OpenInTerminal(PathBuf),
    Reload(PathBuf),
    /// Remove the project from the workspace; its files stay (R-WS-1).
    Close(PathBuf),
    /// The environment a project uses when it has none named like the workspace's (R-WS-6).
    SetProjectEnvironment {
        root: PathBuf,
        id: String,
    },
    /// Create a project in a folder (R-WS-7).
    NewProject,
    /// Find projects in a folder or repository and add them (R-WS-3).
    AddProjects,
    /// A missing project was moved: pick its new folder (R-WS-5).
    LocateMissing(PathBuf),
    RemoveMissing(PathBuf),
}

/// Height of a tree row.
const ROW_HEIGHT: f32 = 28.0;

/// Sidebar state that survives between frames.
#[derive(Debug, Default)]
pub struct SidebarState {
    /// Collapsed folders; a collection's own row uses the key with an empty path.
    collapsed: HashSet<ItemKey>,
    rename: Option<Rename>,
}

impl SidebarState {
    /// Starts inline renaming of an item, e.g. a folder that was just created.
    pub fn start_rename(&mut self, key: ItemKey, current_name: String) {
        self.rename = Some(Rename::new(key, current_name));
    }
}

/// Inline name editor: Enter or leaving the field commits, Escape cancels.
#[derive(Debug)]
struct Rename {
    /// For a collection, the path is empty.
    key: ItemKey,
    text: String,
    original: String,
    focused: bool,
}

impl Rename {
    fn new(key: ItemKey, name: String) -> Self {
        Self {
            key,
            text: name.clone(),
            original: name,
            focused: false,
        }
    }
}

struct Tree<'a> {
    st: &'a mut SidebarState,
    selected: Option<&'a ItemKey>,
    dirty: &'a HashSet<ItemKey>,
    /// Requests whose file changed on disk while they had unsaved edits (R-GIT-11).
    conflicts: &'a HashSet<ItemKey>,
    /// Git status of the collection being drawn (R-GIT-12).
    git: Option<&'a GitStatus>,
    /// Show the branch on the project row (not when a repository row above shows it, R-WS-4).
    show_branch: bool,
    /// Set when the project doesn't have the workspace's environment (R-WS-6).
    env_note: Option<EnvNote>,
    can_open_folders: bool,
    actions: Vec<SidebarAction>,
}

/// The projects of the workspace and their requests (R-COL-1, R-COL-2, R-COL-3, R-WS-4).
pub fn sidebar(
    ui: &mut Ui,
    state: &AppState,
    st: &mut SidebarState,
    selected: Option<&ItemKey>,
    dirty: &HashSet<ItemKey>,
    conflicts: &HashSet<ItemKey>,
) -> Vec<SidebarAction> {
    let mut tree = Tree {
        st,
        selected,
        dirty,
        conflicts,
        git: None,
        show_branch: true,
        env_note: None,
        can_open_folders: state.host.can_open_folders(),
        actions: Vec::new(),
    };

    let full = egui::vec2(ui.available_width(), ROW_HEIGHT);
    if ui
        .add(
            Button::selectable(
                selected.is_none(),
                (Atom::from("✏"), Atom::from("Scratch request")),
            )
            .min_size(full)
            .frame_when_inactive(selected.is_none()),
        )
        .on_hover_text("A request that is not saved anywhere, for quick one-off calls")
        .clicked()
    {
        tree.actions.push(SidebarAction::Select(None));
    }
    ui.add_space(14.0);
    egui::Sides::new().show(
        ui,
        |ui| theme::section_label(ui, "Projects"),
        |ui| {
            if state.host.can_pick_folders() {
                let (response, inner) =
                    egui::containers::menu::MenuButton::from_button(theme::ghost_button("+ Add"))
                        .ui(ui, |ui| {
                            let mut action = None;
                            if ui
                                .button("New project…")
                                .on_hover_text("Create a project in a folder of your choice")
                                .clicked()
                            {
                                action = Some(SidebarAction::NewProject);
                            }
                            if ui
                                .button("Add existing projects…")
                                .on_hover_text("Find the projects in a folder or git repository")
                                .clicked()
                            {
                                action = Some(SidebarAction::AddProjects);
                            }
                            action
                        });
                response.on_hover_text("Add a project to this workspace");
                tree.actions.extend(inner.and_then(|i| i.inner));
            }
        },
    );

    ScrollArea::vertical()
        .id_salt("sidebar")
        .auto_shrink(false)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            for (repository, members) in state.project_groups() {
                let shared = repository.as_ref().filter(|_| members.len() > 1);
                let depth = usize::from(shared.is_some());
                let collapsed = match shared {
                    Some(repo) => {
                        let git = state.git.get(&members[0].root);
                        ui.push_id(repo, |ui| repository_row(ui, repo, git, &members, &mut tree))
                            .inner
                    }
                    None => false,
                };
                if collapsed {
                    continue;
                }
                for c in members {
                    ui.push_id(&c.root, |ui| {
                        tree.git = state.git.get(&c.root);
                        tree.show_branch = shared.is_none();
                        tree.env_note = environment_note(state, c);
                        collection(ui, c, state.cookie_jar(Some(&c.root)).len(), depth, &mut tree);
                    });
                }
            }
            for (project, reason) in &state.missing {
                ui.push_id(&project.path, |ui| missing_row(ui, project, reason, &mut tree));
            }
            if state.collections.is_empty() && state.missing.is_empty() {
                ui.add_space(4.0);
                theme::hint(
                    ui,
                    "Add a project to start. Projects are plain files you can commit to git, next to your code.",
                );
            }
        });

    tree.actions
}

fn collection(ui: &mut Ui, c: &Collection, cookie_count: usize, depth: usize, tree: &mut Tree<'_>) {
    let root = c.root.clone();
    let key = ItemKey::new(&root, "");
    let collapsed = tree.st.collapsed.contains(&key);
    let read_only = c.read_only;

    let can_open = tree.can_open_folders;
    let environments = tree.env_note.as_ref().map(|n| n.environments.clone());
    let menu =
        |ui: &mut Ui| collection_menu(ui, c, cookie_count, can_open, environments.as_deref());
    if is_renaming(tree, &key) {
        if let Some(name) = rename_row(ui, depth, tree, &key) {
            tree.actions.push(SidebarAction::RenameCollection {
                root: root.clone(),
                name,
            });
        }
    } else {
        let mut atoms = vec![
            Atom::from(twisty(collapsed)),
            Atom::from(theme::semibold(c.name())),
        ];
        if read_only {
            atoms.push(Atom::from(RichText::new("read-only").small().weak()));
        }
        // R-GIT-12: the current branch (shown on the repository row when projects share one).
        let branch = tree
            .git
            .and_then(|g| g.branch.clone())
            .filter(|_| tree.show_branch);
        if let Some(branch) = &branch {
            atoms.push(Atom::from(
                RichText::new(branch)
                    .small()
                    .color(theme::palette(ui).muted),
            ));
        }
        atoms.push(Atom::grow());
        // R-WS-6: this project doesn't have the workspace's environment.
        if let Some(note) = &tree.env_note {
            atoms.push(Atom::from(
                RichText::new(format!("⚠ {}", note.used))
                    .small()
                    .color(theme::palette(ui).redirect),
            ));
        }
        let (row, from_button) = tree_row(ui, depth, false, atoms, |ui| menu_button(ui, menu));
        let mut hover = root.display().to_string();
        if let Some(branch) = branch {
            hover = format!("{hover}\nGit branch: {branch}");
        }
        if let Some(note) = &tree.env_note {
            hover = format!(
                "{hover}\nNo “{}” environment in this project: it uses “{}”. Change it in the menu.",
                note.wanted, note.used
            );
        }
        let row = row.on_hover_text(hover);
        if row.clicked() {
            toggle(&mut tree.st.collapsed, &key);
        }
        let choice = from_button.or_else(|| context_menu(&row, menu));
        apply(tree, choice, &key, c.name());
    }

    if !collapsed {
        items(
            ui,
            &root,
            Path::new(""),
            &c.items,
            depth + 1,
            read_only,
            tree,
        );
        for b in &c.broken_environments {
            let label = Atom::from(format!("⚠ {}", b.path.display()));
            let (row, ()) = tree_row(ui, depth + 1, false, [label, Atom::grow()], |_| {});
            if row.clicked() {
                tree.actions.push(SidebarAction::ShowError(format!(
                    "{}: {}",
                    b.path.display(),
                    b.error
                )));
            }
        }
    }
}

/// A project without an environment named like the workspace's (R-WS-6).
struct EnvNote {
    /// The workspace's environment.
    wanted: String,
    /// The one this project uses instead.
    used: String,
    /// The project's environments, as (id, name).
    environments: Vec<(String, String)>,
}

fn environment_note(state: &AppState, c: &Collection) -> Option<EnvNote> {
    if state.follows_workspace_environment(&c.root) {
        return None;
    }
    let wanted = state.workspace().environment.clone()?;
    let used = state
        .active_environment(&c.root)
        .and_then(|id| c.environment(&id).map(|e| e.name.clone()))
        .unwrap_or_default();
    let envs = c
        .environments
        .iter()
        .map(|e| (e.id.clone(), e.name.clone()))
        .collect();
    Some(EnvNote {
        wanted,
        used,
        environments: envs,
    })
}

/// The row above projects that share a git repository: its name, branch and state,
/// shown once (R-WS-4). Returns whether it is collapsed.
fn repository_row(
    ui: &mut Ui,
    repo: &Path,
    git: Option<&GitStatus>,
    members: &[&Collection],
    tree: &mut Tree<'_>,
) -> bool {
    let key = ItemKey::new(repo, ".repository");
    let collapsed = tree.st.collapsed.contains(&key);
    let p = theme::palette(ui);
    let name = repo.file_name().map_or_else(
        || repo.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    let mut atoms = vec![
        Atom::from(twisty(collapsed)),
        Atom::from(theme::semibold(name).color(p.muted)),
    ];
    let branch = git.and_then(|g| g.branch.clone());
    if let Some(branch) = &branch {
        atoms.push(Atom::from(RichText::new(branch).small().color(p.muted)));
    }
    atoms.push(Atom::grow());
    let (row, ()) = tree_row(ui, 0, false, atoms, |_| {});
    let mut hover = format!(
        "Git repository {}\n{} projects in this workspace",
        repo.display(),
        members.len()
    );
    if let Some(branch) = branch {
        hover = format!("{hover}\nGit branch: {branch}");
    }
    if row.on_hover_text(hover).clicked() {
        toggle(&mut tree.st.collapsed, &key);
    }
    collapsed
}

/// A project of the workspace that isn't on this machine (moved, or not cloned yet), R-WS-5.
fn missing_row(ui: &mut Ui, project: &ProjectRef, reason: &str, tree: &mut Tree<'_>) {
    let p = theme::palette(ui);
    let name = project.path.file_name().map_or_else(
        || project.path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    let atoms = [
        Atom::from(RichText::new("⚠").color(p.muted)),
        Atom::from(RichText::new(name).color(p.muted)),
        Atom::from(RichText::new("missing").small().color(p.muted)),
        Atom::grow(),
    ];
    let path = project.path.clone();
    let remote = project.remote.clone();
    let menu = |ui: &mut Ui| {
        let mut choice = None;
        if ui
            .button("Locate…")
            .on_hover_text("Pick the folder where the project is now")
            .clicked()
        {
            choice = Some(MenuChoice::Action(SidebarAction::LocateMissing(
                path.clone(),
            )));
        }
        if let Some(remote) = &remote
            && ui
                .button("Copy clone URL")
                .on_hover_text(remote.as_str())
                .clicked()
        {
            ui.ctx().copy_text(remote.clone());
        }
        if ui.button("Remove from workspace").clicked() {
            choice = Some(MenuChoice::Action(SidebarAction::RemoveMissing(
                path.clone(),
            )));
        }
        choice
    };
    let (row, from_button) = tree_row(ui, 0, false, atoms, |ui| menu_button(ui, menu));
    let mut hover = format!("{}\n{reason}", project.path.display());
    if let Some(remote) = &project.remote {
        hover = format!("{hover}\nClone it from {remote}, then use Locate…");
    }
    let row = row.on_hover_text(hover);
    let choice = from_button.or_else(|| context_menu(&row, menu));
    if let Some(MenuChoice::Action(action)) = choice {
        tree.actions.push(action);
    }
}

fn items(
    ui: &mut Ui,
    root: &std::path::Path,
    folder: &Path,
    list: &[Item],
    depth: usize,
    read_only: bool,
    tree: &mut Tree<'_>,
) {
    for item in list {
        ui.push_id(item.path(), |ui| {
            tree_item(ui, root, item, depth, read_only, tree);
        });
    }
    // Requests deleted in git are still shown where they were (R-GIT-12, D10).
    let deleted = tree
        .git
        .map(|g| g.deleted_requests(folder))
        .unwrap_or_default();
    for path in deleted
        .iter()
        .filter(|p| !list.iter().any(|i| i.path() == p.as_path()))
    {
        ui.push_id(path, |ui| deleted_row(ui, path, depth));
    }
}

/// A request whose file was deleted (in git terms): greyed and struck through.
fn deleted_row(ui: &mut Ui, path: &Path, depth: usize) {
    let p = theme::palette(ui);
    let name = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let atoms = [
        Atom::from(RichText::new("     ").monospace().size(10.5)),
        Atom::from(RichText::new(name).strikethrough().color(p.muted)),
        Atom::grow(),
        git_marker(ui, FileStatus::Deleted),
    ];
    let (row, ()) = tree_row(ui, depth, false, atoms, |_| {});
    row.on_hover_text(
        "Deleted since the last commit. Commit the deletion, or restore the file with git.",
    );
}

/// The one-letter git marker of a row (R-GIT-12).
fn git_marker(ui: &Ui, status: FileStatus) -> Atom<'static> {
    let p = theme::palette(ui);
    let (letter, color) = match status {
        FileStatus::Modified => ("M", p.redirect),
        FileStatus::New => ("N", p.ok),
        FileStatus::Deleted => ("D", p.fail),
        FileStatus::Conflicted => ("C", p.purple),
    };
    Atom::from(theme::semibold(letter).size(11.0).color(color))
}

fn git_description(status: FileStatus) -> &'static str {
    match status {
        FileStatus::Modified => "Git: modified since the last commit",
        FileStatus::New => "Git: new, not committed yet",
        FileStatus::Deleted => "Git: deleted",
        FileStatus::Conflicted => "Git: merge conflict — fix it, then commit",
    }
}

fn tree_item(
    ui: &mut Ui,
    root: &std::path::Path,
    item: &Item,
    depth: usize,
    read_only: bool,
    tree: &mut Tree<'_>,
) {
    let key = ItemKey::new(root, item.path());

    if let Item::Broken(b) = item {
        broken_row(ui, b, key, depth, read_only, tree);
        return;
    }

    let is_folder = matches!(item, Item::Folder(_));
    let collapsed = tree.st.collapsed.contains(&key);
    let name = item.name();

    if is_renaming(tree, &key) {
        if let Some(new_name) = rename_row(ui, depth, tree, &key) {
            tree.actions.push(SidebarAction::Rename {
                key: key.clone(),
                name: new_name,
            });
        }
    } else if !skip_hidden_row(ui) {
        let p = theme::palette(ui);
        let mut atoms = vec![match item {
            Item::Request(r) => {
                let m = r.request.method;
                let tag = format!("{:<5}", theme::method_short(m));
                Atom::from(
                    RichText::new(tag)
                        .monospace()
                        .size(10.5)
                        .color(theme::method_color(ui, m)),
                )
            }
            _ => Atom::from(twisty(collapsed)),
        }];
        atoms.push(Atom::from(name.clone()));
        atoms.push(Atom::grow());
        let folder_error = match item {
            Item::Folder(f) => f.error.as_ref(),
            _ => None,
        };
        if folder_error.is_some() {
            atoms.push(Atom::from(RichText::new("⚠").color(p.redirect)));
        }
        let dirty = tree.dirty.contains(&key);
        if dirty {
            atoms.push(Atom::from(RichText::new("⏺").size(9.0).color(p.accent)));
        }
        let conflict = tree.conflicts.contains(&key);
        if conflict {
            atoms.push(Atom::from(RichText::new("⚠").color(p.redirect)));
        }
        let git_status = tree.git.and_then(|g| {
            if is_folder {
                g.folder(&key.path)
            } else {
                g.file(&key.path)
            }
        });
        if let Some(status) = git_status {
            atoms.push(git_marker(ui, status));
        }
        let selected = tree.selected == Some(&key);
        let menu = |ui: &mut Ui| item_menu(ui, &key, is_folder);
        let (mut row, from_button) = tree_row(ui, depth, selected, atoms, |ui| {
            if read_only {
                None
            } else {
                menu_button(ui, menu)
            }
        });
        let mut hover: Vec<&str> = Vec::new();
        if let Some(err) = folder_error {
            hover.push(err);
        }
        if conflict {
            hover.push("Changed on disk while you had unsaved edits: open it to choose");
        } else if dirty {
            hover.push("Unsaved changes");
        }
        if let Some(status) = git_status {
            hover.push(git_description(status));
        }
        if !hover.is_empty() {
            row = row.on_hover_text(hover.join("\n"));
        }
        if row.clicked() {
            if is_folder {
                toggle(&mut tree.st.collapsed, &key);
            } else {
                tree.actions.push(SidebarAction::Select(Some(key.clone())));
            }
        }
        let from_context = if read_only {
            None
        } else {
            context_menu(&row, menu)
        };
        apply(tree, from_button.or(from_context), &key, &name);
    }

    if let Item::Folder(f) = item
        && !collapsed
    {
        items(ui, root, &key.path, &f.items, depth + 1, read_only, tree);
    }
}

/// What the user picked in a `…` or right-click menu.
enum MenuChoice {
    Action(SidebarAction),
    Rename,
}

fn apply(tree: &mut Tree<'_>, choice: Option<MenuChoice>, key: &ItemKey, name: &str) {
    match choice {
        Some(MenuChoice::Action(a)) => tree.actions.push(a),
        Some(MenuChoice::Rename) => {
            tree.st.rename = Some(Rename::new(key.clone(), name.to_owned()));
        }
        None => {}
    }
}

/// Actions for a collection.
fn collection_menu(
    ui: &mut Ui,
    c: &Collection,
    cookie_count: usize,
    can_open_folders: bool,
    // Set when the project doesn't follow the workspace's environment (R-WS-6).
    environments: Option<&[(String, String)]>,
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
    item(ui, "Defaults (headers, auth)…".into(), &|| {
        MenuChoice::Action(SidebarAction::EditDefaults(root.clone()))
    });
    let clear = ui
        .add_enabled(
            cookie_count > 0,
            egui::Button::new(format!("Clear cookies ({cookie_count})")),
        )
        .on_hover_text("Cookies are kept on this machine, never in the project");
    let clear_clicked = clear.clicked();
    item(ui, "Reload from disk".into(), &|| {
        MenuChoice::Action(SidebarAction::Reload(root.clone()))
    });
    // R-GIT-13: commit, pull and push happen outside the app.
    if can_open_folders {
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
    if clear_clicked {
        return Some(MenuChoice::Action(SidebarAction::ClearCookies(
            root.clone(),
        )));
    }
    choice
}

/// A request file that could not be read (R-GIT-8), e.g. with merge-conflict markers.
fn broken_row(
    ui: &mut Ui,
    b: &tortuga_core::collection::Broken,
    key: ItemKey,
    depth: usize,
    read_only: bool,
    tree: &mut Tree<'_>,
) {
    let file = b.path.file_name().unwrap_or_default().to_string_lossy();
    let mut atoms = vec![Atom::from(format!("⚠ {file}")), Atom::grow()];
    if let Some(status) = tree.git.and_then(|g| g.file(&key.path)) {
        atoms.push(git_marker(ui, status));
    }
    let (row, delete) = tree_row(ui, depth, false, atoms, |ui| {
        !read_only
            && ui
                .add(theme::icon_button(ui, "🗙"))
                .on_hover_text("Delete this file")
                .clicked()
    });
    if row.on_hover_text(&b.error).clicked() {
        tree.actions.push(SidebarAction::ShowError(format!(
            "Could not read {}: {}. Fix the file in an editor, then use “Reload from disk”.",
            b.path.display(),
            b.error
        )));
    }
    if delete {
        tree.actions.push(SidebarAction::Delete(key));
    }
}

/// Actions for a request or folder.
fn item_menu(ui: &mut Ui, key: &ItemKey, is_folder: bool) -> Option<MenuChoice> {
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

fn menu_button(
    ui: &mut Ui,
    menu: impl FnOnce(&mut Ui) -> Option<MenuChoice>,
) -> Option<MenuChoice> {
    let (response, inner) =
        egui::containers::menu::MenuButton::from_button(theme::ghost_button("…")).ui(ui, menu);
    response.on_hover_text("Actions");
    inner.and_then(|i| i.inner)
}

fn context_menu(
    row: &egui::Response,
    menu: impl FnOnce(&mut Ui) -> Option<MenuChoice>,
) -> Option<MenuChoice> {
    let mut choice = None;
    row.context_menu(|ui| choice = menu(ui));
    choice
}

fn twisty(collapsed: bool) -> &'static str {
    if collapsed { "⏵" } else { "⏷" }
}

fn indent(depth: usize) -> f32 {
    #[expect(clippy::cast_precision_loss, reason = "tree depth is small")]
    let depth = depth as f32;
    depth * 14.0
}

/// One tree row: the label fills the width, `trailing` (menu or delete button) sits
/// at the right edge. Trailing is laid out first, so the row never overflows the sidebar.
fn tree_row<'a, T>(
    ui: &mut Ui,
    depth: usize,
    selected: bool,
    atoms: impl IntoIterator<Item = Atom<'a>>,
    trailing: impl FnOnce(&mut Ui) -> T,
) -> (egui::Response, T) {
    let atoms: egui::Atoms<'a> = atoms.into_iter().collect();
    // Row actions only show on the hovered or selected row (or while a menu is open),
    // but stay clickable, so the tree stays calm.
    let row_rect = egui::Rect::from_min_size(
        ui.cursor().min,
        egui::vec2(ui.available_width(), ROW_HEIGHT),
    );
    let show_trailing =
        selected || ui.rect_contains_pointer(row_rect) || egui::Popup::is_any_open(ui.ctx());
    Sides::new().shrink_left().truncate().show(
        ui,
        |ui| {
            ui.add_space(indent(depth));
            ui.add(
                Button::selectable(selected, atoms)
                    .frame_when_inactive(selected)
                    .truncate()
                    .min_size(egui::vec2(ui.available_width(), ROW_HEIGHT)),
            )
        },
        |ui| {
            if !show_trailing {
                ui.visuals_mut().override_text_color = Some(egui::Color32::TRANSPARENT);
            }
            trailing(ui)
        },
    )
}

/// A row scrolled out of view only takes up its space, so a project with thousands of
/// requests costs next to nothing per frame (R-NF-1, P2). Returns whether the row was skipped.
fn skip_hidden_row(ui: &mut Ui) -> bool {
    let size = egui::vec2(ui.available_width(), ROW_HEIGHT);
    if ui.is_rect_visible(egui::Rect::from_min_size(ui.cursor().min, size)) {
        return false;
    }
    ui.allocate_space(size);
    true
}

fn toggle(set: &mut HashSet<ItemKey>, key: &ItemKey) {
    if !set.remove(key) {
        set.insert(key.clone());
    }
}

fn is_renaming(tree: &Tree<'_>, key: &ItemKey) -> bool {
    tree.st.rename.as_ref().is_some_and(|r| r.key == *key)
}

/// The inline rename field for `key`. Returns the new name once the user commits a real change.
fn rename_row(ui: &mut Ui, depth: usize, tree: &mut Tree<'_>, key: &ItemKey) -> Option<String> {
    let rename = tree.st.rename.as_mut().filter(|r| r.key == *key)?;
    let mut out = ui
        .horizontal(|ui| {
            ui.add_space(indent(depth));
            TextEdit::singleline(&mut rename.text)
                .id_salt("rename")
                .desired_width(ui.available_width())
                .show(ui)
        })
        .inner;
    if !rename.focused {
        // Select the current name so typing replaces it.
        out.response.request_focus();
        let end = rename.text.chars().count();
        out.state
            .cursor
            .set_char_range(Some(CCursorRange::two(CCursor::new(0), CCursor::new(end))));
        out.state.store(ui.ctx(), out.response.id);
        rename.focused = true;
        return None;
    }
    if !out.response.lost_focus() {
        return None;
    }
    let cancelled = ui.input(|i| i.key_pressed(Key::Escape));
    let name = rename.text.trim().to_owned();
    let changed = !cancelled && !name.is_empty() && name != rename.original;
    tree.st.rename = None;
    changed.then_some(name)
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use super::*;

    /// R-NF-1: a skipped row takes exactly the space of a drawn one, so the scroll bar doesn't jump.
    #[test]
    fn hidden_rows_take_the_space_of_drawn_rows() {
        let drawn = Rc::new(Cell::new(0.0));
        let skipped = Rc::new(Cell::new(0.0));
        let (d, s) = (drawn.clone(), skipped.clone());
        let mut harness = crate::test_host::harness(move |ui| {
            let key = ItemKey::new(Path::new("/p"), "get-user.toml");
            let top = ui.cursor().min.y;
            let atoms = [
                Atom::from(RichText::new("GET  ").monospace().size(10.5)),
                Atom::from("Get user"),
                Atom::grow(),
            ];
            tree_row(ui, 2, false, atoms, |ui| {
                menu_button(ui, |ui: &mut Ui| item_menu(ui, &key, false))
            });
            d.set(ui.cursor().min.y - top);

            ui.scope(|ui| {
                ui.set_clip_rect(egui::Rect::NOTHING);
                let top = ui.cursor().min.y;
                assert!(skip_hidden_row(ui));
                s.set(ui.cursor().min.y - top);
            });
        });
        harness.run();
        assert!(drawn.get() > 0.0);
        assert!(
            (drawn.get() - skipped.get()).abs() < 0.01,
            "{drawn:?} vs {skipped:?}"
        );
    }
}
