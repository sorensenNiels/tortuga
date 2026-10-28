//! Folding many rows at once: every project of the workspace (R-WS-13), or every folder below
//! a project or folder (R-COL-43).

use std::path::Path;

use egui::Ui;
use tortuga_core::Item;

use super::SidebarState;
use super::menus::Fold;
use crate::state::{AppState, ItemKey};
use crate::theme;

impl SidebarState {
    /// Whether some project shows its requests: its row is unfolded and not under a folded
    /// repository row (R-WS-13).
    fn any_project_open(&self, rows: &[ProjectRow]) -> bool {
        rows.iter().any(|row| {
            !self.collapsed.contains(&row.project)
                && row
                    .repository
                    .as_ref()
                    .is_none_or(|r| !self.collapsed.contains(r))
        })
    }

    /// Folds every project's row, or unfolds them and the repository rows above them; folders
    /// keep their own state (R-WS-13).
    fn fold_projects(&mut self, rows: &[ProjectRow], fold: bool) {
        for row in rows {
            if fold {
                self.collapsed.insert(row.project.clone());
            } else {
                self.collapsed.remove(&row.project);
                if let Some(repository) = &row.repository {
                    self.collapsed.remove(repository);
                }
            }
        }
        self.changed = true;
    }

    /// Folds every folder below `row` and the `groups` (and unfolds `row`, so its top level
    /// shows), or unfolds `row` and everything below it (R-COL-43).
    pub(super) fn fold_below(
        &mut self,
        row: &ItemKey,
        below: &[Item],
        groups: &[ItemKey],
        fold: Fold,
    ) {
        match fold {
            Fold::Collapse => {
                self.collapsed.remove(row);
                let mut folders = Vec::new();
                folders_in(&row.root, below, &mut folders);
                self.collapsed.extend(folders);
                self.collapsed.extend(groups.iter().cloned());
            }
            Fold::Expand => {
                self.collapsed
                    .retain(|k| !(k.root == row.root && k.path.starts_with(&row.path)));
                for group in groups {
                    self.collapsed.remove(group);
                }
            }
        }
        self.changed = true;
    }
}

/// A project's row and the row of the repository it shares with other projects (R-WS-4).
#[derive(Debug)]
pub(super) struct ProjectRow {
    project: ItemKey,
    repository: Option<ItemKey>,
}

/// The project rows of the workspace, for "Collapse all" and "Expand all" (R-WS-13).
pub(super) fn project_rows(state: &AppState) -> Vec<ProjectRow> {
    state
        .project_groups()
        .into_iter()
        .flat_map(|(repository, members)| {
            let shared = repository.filter(|_| members.len() > 1);
            members.into_iter().map(move |c| ProjectRow {
                project: ItemKey::new(&c.root, ""),
                repository: shared.as_deref().map(repository_key),
            })
        })
        .collect()
}

/// The keys of every folder in `items`, at any depth.
fn folders_in(root: &Path, items: &[Item], out: &mut Vec<ItemKey>) {
    for item in items {
        if let Item::Folder(f) = item {
            out.push(ItemKey::new(root, f.path.clone()));
            folders_in(root, &f.items, out);
        }
    }
}

pub(super) fn repository_key(repo: &Path) -> ItemKey {
    ItemKey::new(repo, ".repository")
}

/// Collapses every project when one shows its requests, else expands them all (R-WS-13).
pub(super) fn fold_all_button(ui: &mut Ui, st: &mut SidebarState, rows: &[ProjectRow]) {
    let fold = st.any_project_open(rows);
    let (label, hint) = if fold {
        (t!("sidebar-collapse-all"), t!("sidebar-collapse-all-hint"))
    } else {
        (t!("sidebar-expand-all"), t!("sidebar-expand-all-hint"))
    };
    if ui
        .add(theme::ghost_button(label))
        .on_hover_text(hint)
        .clicked()
    {
        st.fold_projects(rows, fold);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::super::tests::tree_items;
    use super::*;

    fn rows() -> Vec<ProjectRow> {
        let repo = repository_key(Path::new("/repo"));
        vec![
            ProjectRow {
                project: ItemKey::new(Path::new("/repo/a"), ""),
                repository: Some(repo.clone()),
            },
            ProjectRow {
                project: ItemKey::new(Path::new("/repo/b"), ""),
                repository: Some(repo),
            },
            ProjectRow {
                project: ItemKey::new(Path::new("/ü😀"), ""),
                repository: None,
            },
        ]
    }

    /// R-WS-13: "Collapse all" folds every project and leaves its folders as they were.
    #[test]
    fn collapse_all_folds_every_project_but_not_its_folders() {
        let rows = rows();
        let folder = ItemKey::new(Path::new("/repo/a"), "users");
        let mut st = SidebarState::default();
        st.toggle(&folder);
        assert!(st.any_project_open(&rows));
        st.fold_projects(&rows, true);
        assert!(!st.any_project_open(&rows));
        assert!(rows.iter().all(|r| st.collapsed.contains(&r.project)));
        assert!(st.collapsed.contains(&folder), "folders keep their state");
        assert!(st.take_folded().is_some(), "kept per workspace (R-COL-36)");
    }

    /// R-WS-13: "Expand all" unfolds every project and the repository rows above them.
    #[test]
    fn expand_all_unfolds_projects_and_repository_rows() {
        let rows = rows();
        let folder = ItemKey::new(Path::new("/repo/a"), "users");
        let mut st = SidebarState::default();
        st.fold_projects(&rows, true);
        st.toggle(&repository_key(Path::new("/repo")));
        st.toggle(&folder);
        st.fold_projects(&rows, false);
        assert_eq!(st.collapsed, HashSet::from([folder]));
    }

    /// R-WS-13: a project under a folded repository row shows nothing, so it counts as folded.
    #[test]
    fn a_project_under_a_folded_repository_counts_as_folded() {
        let rows = rows();
        let mut st = SidebarState::default();
        st.toggle(&rows[2].project);
        st.toggle(&repository_key(Path::new("/repo")));
        assert!(!st.any_project_open(&rows));
    }

    /// R-COL-43: "Collapse all" on a project folds its folders at every depth and its
    /// Workflows group, and leaves the project open so its top level shows.
    #[test]
    fn collapse_all_folds_every_folder_below_the_project() {
        let root = Path::new("/p");
        let project = ItemKey::new(root, "");
        let workflows = ItemKey::new(root, tortuga_core::workflow::WORKFLOWS_DIR);
        let mut st = SidebarState::default();
        st.toggle(&project);
        st.fold_below(
            &project,
            &tree_items(),
            std::slice::from_ref(&workflows),
            Fold::Collapse,
        );
        let expected: HashSet<ItemKey> = ["users", "users/ü😀", "users/ü😀/deep", "orders"]
            .into_iter()
            .map(|p| ItemKey::new(root, p))
            .chain([workflows])
            .collect();
        assert_eq!(st.collapsed, expected);
        assert!(st.take_folded().is_some(), "kept per workspace (R-COL-36)");
    }

    /// R-COL-43: "Expand all" on a folder unfolds it and everything below it, nothing else.
    #[test]
    fn expand_all_unfolds_only_the_folder_and_below() {
        let root = Path::new("/p");
        let users = ItemKey::new(root, "users");
        let items = tree_items();
        let Some(Item::Folder(f)) = items.first() else {
            panic!("users is a folder");
        };
        let mut st = SidebarState::default();
        st.fold_below(&ItemKey::new(root, ""), &items, &[], Fold::Collapse);
        st.toggle(&users);
        st.fold_below(&users, &f.items, &[], Fold::Expand);
        let other = ItemKey::new(Path::new("/other"), "users");
        st.toggle(&other);
        assert_eq!(
            st.collapsed,
            HashSet::from([ItemKey::new(root, "orders"), other])
        );
    }
}
