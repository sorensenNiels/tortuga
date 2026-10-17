use std::collections::HashSet;
use std::path::PathBuf;

use egui::text::{CCursor, CCursorRange};
use egui::{Atom, Button, Key, RichText, ScrollArea, Sides, TextEdit, Ui};
use tortuga_core::collection::Direction;
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
    Reload(PathBuf),
    Close(PathBuf),
    OpenFolder,
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
    actions: Vec<SidebarAction>,
}

/// Collection tree (R-COL-1, R-COL-2, R-COL-3).
pub fn sidebar(
    ui: &mut Ui,
    state: &AppState,
    st: &mut SidebarState,
    selected: Option<&ItemKey>,
    dirty: &HashSet<ItemKey>,
) -> Vec<SidebarAction> {
    let mut tree = Tree {
        st,
        selected,
        dirty,
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
        |ui| theme::section_label(ui, "Collections"),
        |ui| {
            if state.host.can_pick_folders()
                && ui
                    .add(theme::ghost_button("+ Open…"))
                    .on_hover_text(
                        "Open a collection folder, or turn a folder into a new collection",
                    )
                    .clicked()
            {
                tree.actions.push(SidebarAction::OpenFolder);
            }
        },
    );

    ScrollArea::vertical()
        .id_salt("sidebar")
        .auto_shrink(false)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            for c in &state.collections {
                ui.push_id(&c.root, |ui| collection(ui, c, &mut tree));
            }
            if state.collections.is_empty() {
                ui.add_space(4.0);
                theme::hint(
                    ui,
                    "Open a folder to start a collection. Collections are plain files you can commit to git.",
                );
            }
        });

    tree.actions
}

fn collection(ui: &mut Ui, c: &Collection, tree: &mut Tree<'_>) {
    let root = c.root.clone();
    let key = ItemKey::new(&root, "");
    let collapsed = tree.st.collapsed.contains(&key);
    let read_only = c.read_only;

    let menu = |ui: &mut Ui| collection_menu(ui, c);
    if is_renaming(tree, &key) {
        if let Some(name) = rename_row(ui, 0, tree, &key) {
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
        atoms.push(Atom::grow());
        let (row, from_button) = tree_row(ui, 0, false, atoms, |ui| menu_button(ui, menu));
        let row = row.on_hover_text(root.display().to_string());
        if row.clicked() {
            toggle(&mut tree.st.collapsed, &key);
        }
        let choice = from_button.or_else(|| context_menu(&row, menu));
        apply(tree, choice, &key, c.name());
    }

    if !collapsed {
        items(ui, &root, &c.items, 1, read_only, tree);
        for b in &c.broken_environments {
            let label = Atom::from(format!("⚠ {}", b.path.display()));
            let (row, ()) = tree_row(ui, 1, false, [label, Atom::grow()], |_| {});
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

fn items(
    ui: &mut Ui,
    root: &std::path::Path,
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
        let file = b.path.file_name().unwrap_or_default().to_string_lossy();
        let atoms = [Atom::from(format!("⚠ {file}")), Atom::grow()];
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
    } else {
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
        let selected = tree.selected == Some(&key);
        let menu = |ui: &mut Ui| item_menu(ui, &key, is_folder);
        let (mut row, from_button) = tree_row(ui, depth, selected, atoms, |ui| {
            if read_only {
                None
            } else {
                menu_button(ui, menu)
            }
        });
        if let Some(err) = folder_error {
            row = row.on_hover_text(err);
        } else if dirty {
            row = row.on_hover_text("Unsaved changes");
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
        items(ui, root, &f.items, depth + 1, read_only, tree);
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
fn collection_menu(ui: &mut Ui, c: &Collection) -> Option<MenuChoice> {
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
    let environments = format!("Environments ({})…", c.environments.len());
    item(ui, environments, &|| {
        MenuChoice::Action(SidebarAction::EditEnvironments(root.clone()))
    });
    item(ui, "Reload from disk".into(), &|| {
        MenuChoice::Action(SidebarAction::Reload(root.clone()))
    });
    item(ui, "Close".into(), &|| {
        MenuChoice::Action(SidebarAction::Close(root.clone()))
    });
    choice
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
