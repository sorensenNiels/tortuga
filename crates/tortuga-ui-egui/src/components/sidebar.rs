mod drag;
mod menus;
mod problems;

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use egui::text::{CCursor, CCursorRange};
use egui::{Atom, AtomExt as _, Button, Key, RichText, ScrollArea, Sides, TextEdit, Ui};
use tortuga_core::collection::Direction;
use tortuga_core::git::{FileStatus, GitStatus};
use tortuga_core::import::ImportKind;
use tortuga_core::settings::Feature;
use tortuga_core::{Collection, Item};

use crate::components::ENVIRONMENT_KINDS;
use crate::state::{AppState, ItemKey, WorkflowKey};
use crate::theme;
use drag::{drag_and_drop, drag_label, drop_on_project};
use menus::{MenuChoice, add_menu, apply, collection_menu, context_menu, item_menu, menu_button};
use problems::{broken_row, missing_row};

/// Something the user asked for in the sidebar; handled by the shell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SidebarAction {
    /// Open in the preview tab (R-COL-13); `None` = the scratch request.
    Select(Option<ItemKey>),
    /// Open in a tab that stays (double click, R-COL-13).
    Keep(Option<ItemKey>),
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
    /// Default headers, auth and variables of a folder (R-COL-11).
    EditFolderDefaults(ItemKey),
    /// A project's description (R-COL-22).
    ProjectDescription(PathBuf),
    /// A folder's description (R-COL-22).
    FolderDescription(ItemKey),
    /// Workflows (R-FLOW-2, R-FLOW-3).
    NewWorkflow(PathBuf),
    SelectWorkflow(WorkflowKey),
    RunWorkflow(WorkflowKey),
    RenameWorkflow(WorkflowKey),
    DeleteWorkflow(WorkflowKey),
    /// Show (and clear) the project's runtime variables (R-TEST-5).
    RuntimeVariables(PathBuf),
    /// Drag and drop: put the item in `parent` before `before` (`None`: at the end) (R-COL-12).
    Place {
        key: ItemKey,
        parent: PathBuf,
        before: Option<PathBuf>,
    },
    /// Show the project's cookie jar, to remove cookies or clear it (R-CK-3, R-CK-5).
    ShowCookies(PathBuf),
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
    /// Create a project from an OpenAPI description (R-COL-15), a Postman collection
    /// (R-COL-16) or a `.http` file (R-COL-17).
    Import(ImportKind),
    /// Import Postman (R-COL-20) or HTTP Client (R-COL-29) environments into a project.
    ImportEnvironments(PathBuf, ImportKind),
    /// Start the project's mock server, or show it when it runs (R-MOCK-2).
    StartMock(PathBuf),
    StopMock(PathBuf),
    /// The mock server dialog: port and log (R-MOCK-4).
    ShowMock(PathBuf),
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

/// Entries a project's menu shows only when they work here.
#[derive(Debug, Clone, Copy, Default)]
struct MenuOffers {
    /// "Open in file manager" and "Open in terminal" (R-GIT-13).
    open_folders: bool,
    /// For each of [`ENVIRONMENT_KINDS`]: its importer is on (R-COL-18) and files can be
    /// picked, so projects offer to import its environments (R-COL-20, R-COL-29, R-COL-31).
    environment_imports: [bool; ENVIRONMENT_KINDS.len()],
    /// Which features are on (R-SET-13).
    features: ShownFeatures,
}

impl MenuOffers {
    /// Whether projects offer to import environments of `kind`.
    fn environments(self, kind: ImportKind) -> bool {
        ENVIRONMENT_KINDS
            .iter()
            .position(|k| *k == kind)
            .is_some_and(|i| self.environment_imports[i])
    }
}

/// The features a project's menu and rows show (R-SET-13).
#[derive(Debug, Clone, Copy, Default)]
struct ShownFeatures {
    workflows: bool,
    mock: bool,
    docs: bool,
}

struct Tree<'a> {
    st: &'a mut SidebarState,
    selected: Option<&'a ItemKey>,
    selected_workflow: Option<&'a WorkflowKey>,
    dirty: &'a HashSet<ItemKey>,
    /// Requests whose file changed on disk while they had unsaved edits (R-GIT-11).
    conflicts: &'a HashSet<ItemKey>,
    dirty_workflows: &'a HashSet<WorkflowKey>,
    /// Git status of the collection being drawn (R-GIT-12).
    git: Option<&'a GitStatus>,
    /// Show the branch on the project row (not when a repository row above shows it, R-WS-4).
    show_branch: bool,
    /// Set when the project doesn't have the workspace's environment (R-WS-6).
    env_note: Option<EnvNote>,
    /// The port of the project's running mock server (R-MOCK-2).
    mock_port: Option<u16>,
    offers: MenuOffers,
    /// Projects show their tab-group colour when there are several (R-COL-14).
    color_dots: bool,
    actions: Vec<SidebarAction>,
}

/// What the rows of the tree are marked with.
#[derive(Debug, Clone, Copy)]
pub struct Marks<'a> {
    /// Requests with unsaved edits.
    pub dirty: &'a HashSet<ItemKey>,
    /// Requests whose file changed on disk while they had unsaved edits (R-GIT-11).
    pub conflicts: &'a HashSet<ItemKey>,
    /// Workflows with unsaved edits (R-FLOW-5).
    pub dirty_workflows: &'a HashSet<WorkflowKey>,
}

/// The projects of the workspace and their requests (R-COL-1, R-COL-2, R-COL-3, R-WS-4).
pub fn sidebar(
    ui: &mut Ui,
    state: &AppState,
    st: &mut SidebarState,
    selected: Option<&ItemKey>,
    selected_workflow: Option<&WorkflowKey>,
    marks: Marks<'_>,
) -> Vec<SidebarAction> {
    let scratch_selected = selected.is_none() && selected_workflow.is_none();
    let mut tree = Tree {
        st,
        selected,
        selected_workflow,
        dirty: marks.dirty,
        conflicts: marks.conflicts,
        dirty_workflows: marks.dirty_workflows,
        git: None,
        show_branch: true,
        env_note: None,
        mock_port: None,
        offers: MenuOffers {
            open_folders: state.host.can_open_folders(),
            environment_imports: ENVIRONMENT_KINDS.map(|kind| {
                state.settings.importers.enabled(kind) && state.host.can_pick_folders()
            }),
            features: ShownFeatures {
                workflows: state.settings.features.enabled(Feature::Workflows),
                mock: state.settings.features.enabled(Feature::Examples),
                docs: state.settings.features.enabled(Feature::Docs),
            },
        },
        color_dots: state.collections.len() > 1,
        actions: Vec::new(),
    };

    scratch_row(ui, scratch_selected, &mut tree);
    ui.add_space(14.0);
    // The Projects | History tabs already say it, where there is a history (R-COL-26).
    let heading = !state.host.keeps_history();
    egui::Sides::new().show(
        ui,
        |ui| {
            if heading {
                theme::section_label(ui, "Projects");
            }
        },
        |ui| {
            if state.host.can_pick_folders() {
                let (response, inner) =
                    egui::containers::menu::MenuButton::from_button(theme::ghost_button("+ Add"))
                        .ui(ui, |ui| add_menu(ui, &state.settings.importers));
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
                        tree.mock_port = state.mocks.port(&c.root);
                        collection(ui, c, state.cookie_jar(Some(&c.root)).len(), depth, &mut tree);
                    });
                }
            }
            for (project, reason) in &state.missing {
                ui.push_id(&project.path, |ui| missing_row(ui, project, reason, &mut tree));
            }
            drag_label(ui);
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

/// The scratch request, above the projects.
fn scratch_row(ui: &mut Ui, scratch_selected: bool, tree: &mut Tree<'_>) {
    let full = egui::vec2(ui.available_width(), ROW_HEIGHT);
    let scratch = ui
        .add(
            Button::selectable(
                scratch_selected,
                (Atom::from("✏"), Atom::from("Scratch request")),
            )
            .min_size(full)
            .frame_when_inactive(scratch_selected),
        )
        .on_hover_text("A request that is not saved anywhere, for quick one-off calls");
    if scratch.double_clicked() {
        tree.actions.push(SidebarAction::Keep(None));
    } else if scratch.clicked() {
        tree.actions.push(SidebarAction::Select(None));
    }
}

/// What the right end of a project row shows: its running mock server (R-MOCK-2), and
/// the environment it uses when it lacks the workspace's (R-WS-6).
fn project_marks(ui: &Ui, tree: &Tree<'_>) -> Vec<Atom<'static>> {
    let p = theme::palette(ui);
    let mock = tree
        .mock_port
        .map(|port| RichText::new(format!("mock :{port}")).small().color(p.ok));
    let environment = tree.env_note.as_ref().map(|note| {
        RichText::new(format!("⚠ {}", note.used))
            .small()
            .color(p.redirect)
    });
    mock.into_iter()
        .chain(environment)
        .map(Atom::from)
        .collect()
}

fn collection(ui: &mut Ui, c: &Collection, cookie_count: usize, depth: usize, tree: &mut Tree<'_>) {
    let root = c.root.clone();
    let key = ItemKey::new(&root, "");
    let collapsed = tree.st.collapsed.contains(&key);
    let read_only = c.read_only;

    let offers = tree.offers;
    let environments = tree.env_note.as_ref().map(|n| n.environments.clone());
    let mock_port = tree.mock_port;
    let menu = |ui: &mut Ui| {
        collection_menu(
            ui,
            c,
            cookie_count,
            offers,
            environments.as_deref(),
            mock_port,
        )
    };
    if is_renaming(tree, &key) {
        if let Some(name) = rename_row(ui, depth, tree, &key) {
            tree.actions.push(SidebarAction::RenameCollection {
                root: root.clone(),
                name,
            });
        }
    } else {
        let mut atoms = vec![Atom::from(twisty(collapsed))];
        if tree.color_dots {
            let color = theme::project_color(ui, c.name());
            atoms.push(Atom::from(RichText::new("⏺").size(9.0).color(color)));
        }
        atoms.push(Atom::from(theme::semibold(c.name())).atom_shrink(true));
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
        atoms.extend(project_marks(ui, tree));
        let (row, from_button) = tree_row(ui, depth, false, atoms, |ui| menu_button(ui, menu));
        if !read_only {
            drop_on_project(ui, &row, &root, tree);
        }
        let mut hover = root.display().to_string();
        if let Some(summary) = crate::format::summary_line(&c.manifest.description) {
            hover = format!("{summary}\n{hover}");
        }
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
        if tree.offers.features.workflows {
            workflows(ui, c, depth + 1, tree);
        }
    }
}

/// The project's workflows, as a group under its requests (R-FLOW-2).
fn workflows(ui: &mut Ui, c: &Collection, depth: usize, tree: &mut Tree<'_>) {
    if c.workflows.is_empty() && c.broken_workflows.is_empty() {
        return;
    }
    let p = theme::palette(ui);
    // `workflows/` is never a folder of requests, so this key is free for the group.
    let group = ItemKey::new(&c.root, tortuga_core::workflow::WORKFLOWS_DIR);
    let collapsed = tree.st.collapsed.contains(&group);
    let atoms = [
        Atom::from(twisty(collapsed)),
        Atom::from(theme::semibold("Workflows")),
        Atom::from(
            RichText::new(c.workflows.len().to_string())
                .small()
                .color(p.muted),
        ),
        Atom::grow(),
    ];
    let (row, ()) = tree_row(ui, depth, false, atoms, |_| {});
    if row
        .on_hover_text("Requests run one after the other, with checks (R-FLOW-1)")
        .clicked()
    {
        toggle(&mut tree.st.collapsed, &group);
    }
    if collapsed {
        return;
    }
    for w in &c.workflows {
        let key = WorkflowKey {
            root: c.root.clone(),
            id: w.id.clone(),
        };
        let selected = tree.selected_workflow == Some(&key);
        let dirty = tree.dirty_workflows.contains(&key);
        let mut atoms = vec![
            Atom::from(RichText::new("⏵").size(10.5).color(p.accent)),
            Atom::from(w.name.clone()).atom_shrink(true),
            Atom::from(
                RichText::new(crate::format::count(w.steps.len(), "step"))
                    .small()
                    .color(p.muted),
            ),
        ];
        if dirty {
            atoms.push(Atom::from(RichText::new("⏺").size(9.0).color(p.accent)));
        }
        atoms.push(Atom::grow());
        let read_only = c.read_only;
        let menu = |ui: &mut Ui| {
            let mut choice = None;
            if ui.button("Run").clicked() {
                choice = Some(MenuChoice::Action(SidebarAction::RunWorkflow(key.clone())));
            }
            if !read_only && ui.button("Rename…").clicked() {
                choice = Some(MenuChoice::Action(SidebarAction::RenameWorkflow(
                    key.clone(),
                )));
            }
            if !read_only
                && ui
                    .button(RichText::new("Delete…").color(theme::palette(ui).fail))
                    .clicked()
            {
                choice = Some(MenuChoice::Action(SidebarAction::DeleteWorkflow(
                    key.clone(),
                )));
            }
            choice
        };
        let (row, from_button) = ui
            .push_id(("workflow", &w.id), |ui| {
                tree_row(ui, depth + 1, selected, atoms, |ui| menu_button(ui, menu))
            })
            .inner;
        let row = if dirty {
            row.on_hover_text("Unsaved changes")
        } else {
            row
        };
        if row.clicked() {
            tree.actions
                .push(SidebarAction::SelectWorkflow(key.clone()));
        }
        if let Some(MenuChoice::Action(action)) = from_button.or_else(|| context_menu(&row, menu)) {
            tree.actions.push(action);
        }
    }
    for b in &c.broken_workflows {
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

fn items(
    ui: &mut Ui,
    root: &std::path::Path,
    folder: &Path,
    list: &[Item],
    depth: usize,
    read_only: bool,
    tree: &mut Tree<'_>,
) {
    for (i, item) in list.iter().enumerate() {
        let next = list.get(i + 1).map(Item::path);
        ui.push_id(item.path(), |ui| {
            tree_item(ui, root, item, depth, read_only, next, tree);
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

/// What hovering a request or folder row explains, if anything.
fn row_hover(
    description: &str,
    folder_error: Option<&String>,
    conflict: bool,
    dirty: bool,
    git_status: Option<FileStatus>,
) -> Option<String> {
    // The first line of its description (R-COL-22), then what needs attention.
    let summary = crate::format::summary_line(description);
    let mut hover: Vec<&str> = summary.as_deref().into_iter().collect();
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
    (!hover.is_empty()).then(|| hover.join("\n"))
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
    // The item after this one in its folder, for dropping below it (R-COL-12).
    next: Option<&Path>,
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
        // A long name gives way, not the method or the twisty.
        atoms.push(Atom::from(name.clone()).atom_shrink(true));
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
        let docs = tree.offers.features.docs;
        let menu = |ui: &mut Ui| item_menu(ui, &key, is_folder, docs);
        let (mut row, from_button) = tree_row(ui, depth, selected, atoms, |ui| {
            if read_only {
                None
            } else {
                menu_button(ui, menu)
            }
        });
        if !read_only {
            drag_and_drop(ui, &row, &key, &name, is_folder, next, tree);
        }
        let description = match item {
            Item::Folder(f) => f.defaults.description.as_str(),
            Item::Request(r) => r.request.description.as_str(),
            Item::Broken(_) => "",
        };
        if let Some(text) = row_hover(description, folder_error, conflict, dirty, git_status) {
            row = row.on_hover_text(text);
        }
        if row.double_clicked() && !is_folder {
            tree.actions.push(SidebarAction::Keep(Some(key.clone())));
        } else if row.clicked() {
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
                    .min_size(egui::vec2(ui.available_width(), ROW_HEIGHT))
                    // Rows can be dragged to move them (R-COL-12); a click is still a click.
                    .sense(egui::Sense::click_and_drag()),
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

    use tortuga_core::settings::ImporterSettings;

    use super::*;

    /// R-COL-18: importers that are off are not in "+ Add".
    #[test]
    fn add_menu_lists_only_importers_that_are_on() {
        use egui_kittest::kittest::Queryable;

        let mut importers = ImporterSettings::default();
        importers.set_enabled(ImportKind::Postman, false);
        let mut h = crate::test_host::harness(|ui| {
            add_menu(ui, &importers);
        });
        h.run();
        h.get_by_label("Import OpenAPI…");
        h.get_by_label("Import .http file…");
        assert!(h.query_by_label("Import Postman collection…").is_none());
    }

    /// R-COL-20, R-COL-29, R-COL-31: a project's menu offers Postman, HTTP Client and
    /// Bruno environments only while their importer is on.
    #[test]
    fn project_menu_imports_environments() {
        use egui_kittest::kittest::Queryable;

        let dir = tempfile::tempdir().expect("dir");
        let storage = tortuga_core::storage::FsStorage;
        let c = Collection::create(&storage, dir.path(), "Shop").expect("project");
        for (kind, entry) in [
            (ImportKind::Postman, "Import Postman environments…"),
            (ImportKind::Http, "Import HTTP environments…"),
            (ImportKind::Bruno, "Import Bruno environments…"),
        ] {
            for on in [true, false] {
                let mut choices = Vec::new();
                {
                    let mut h = crate::test_host::harness(|ui| {
                        let offers = MenuOffers {
                            environment_imports: ENVIRONMENT_KINDS.map(|k| on && k == kind),
                            ..MenuOffers::default()
                        };
                        choices.push(collection_menu(ui, &c, 0, offers, None, None));
                    });
                    h.run();
                    let found = h.query_by_label(entry);
                    assert_eq!(found.is_some(), on, "{entry}");
                    if let Some(found) = found {
                        found.click();
                        h.run();
                    }
                }
                let picked = choices.iter().flatten().any(|c| {
                    matches!(c, MenuChoice::Action(SidebarAction::ImportEnvironments(r, k))
                        if *r == dir.path() && *k == kind)
                });
                assert_eq!(picked, on);
            }
        }
    }

    /// R-NF-1: a skipped row takes exactly the space of a drawn one, so the scroll bar doesn't jump.
    #[test]
    fn hidden_rows_take_the_space_of_drawn_rows() {
        let drawn = Rc::new(Cell::new(0.0));
        let skipped = Rc::new(Cell::new(0.0));
        let (d, s) = (Rc::clone(&drawn), Rc::clone(&skipped));
        let mut harness = crate::test_host::harness(move |ui| {
            let key = ItemKey::new(Path::new("/p"), "get-user.toml");
            let top = ui.cursor().min.y;
            let atoms = [
                Atom::from(RichText::new("GET  ").monospace().size(10.5)),
                Atom::from("Get user"),
                Atom::grow(),
            ];
            tree_row(ui, 2, false, atoms, |ui| {
                menu_button(ui, |ui: &mut Ui| item_menu(ui, &key, false, true))
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
