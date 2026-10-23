//! Which requests are open in tabs (R-COL-13), grouped by project (R-COL-14).
//!
//! Kept tabs stay until closed; the preview tab is replaced by the next request
//! opened with a single click. Tabs of one project are always next to each other,
//! and the preview tab comes after all of them.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use egui::{Frame, Margin, Panel, Ui};
use tortuga_core::{HttpMethod, Item, Request};

use super::{
    CLOSE_TAB, GO_TO_TAB, Modal, NEXT_TAB, PREVIOUS_TAB, Selection, TAB_BEFORE, TortugaUi,
    breadcrumb,
};
use crate::components::{ConfirmDialog, GroupChip, TabAction, TabGroup, TabView, tab_strip};
use crate::host::{SavedTab, SavedTabs};
use crate::state::ItemKey;
use crate::theme;

impl TortugaUi {
    /// Opens the tabs remembered for the workspace, minus requests that are gone.
    pub(super) fn restore_tabs(&mut self) {
        self.tabs = Tabs::from_saved(self.state.remembered_tabs());
        self.prune_tabs();
    }

    /// Once per frame: an edited preview tab is kept, tabs of deleted requests close,
    /// the shown request always has a tab, and the tabs are remembered (R-COL-13). With
    /// the preview tab turned off, every tab stays (R-SET-9).
    pub(super) fn sync_tabs(&mut self) {
        self.keep_if_edited();
        self.prune_tabs();
        self.tabs.tidy_collapsed();
        if self.workflows.open.is_none() && !self.tabs.contains(&self.selection) {
            self.tabs.open(&self.selection, false);
        }
        if !self.state.settings.editing.preview_tab {
            self.tabs.keep_preview();
        }
        self.state.remember_tabs(self.tabs.to_saved());
    }

    /// Editing a request keeps its tab (R-COL-13, D31); sending does not.
    pub(super) fn keep_if_edited(&mut self) {
        if self.tabs.is_preview(&self.selection) && self.current_is_edited() {
            self.tabs.keep(&self.selection);
        }
    }

    fn current_is_edited(&self) -> bool {
        match &self.selection {
            Some(k) => self.is_dirty(k, &self.current),
            None => self.current != Request::default(),
        }
    }

    /// Closes tabs whose request no longer exists (deleted in git, project closed).
    /// Requests with unsaved edits keep theirs: the conflict banner asks (R-GIT-11).
    fn prune_tabs(&mut self) {
        let state = &self.state;
        let (drafts, selection) = (&self.drafts, &self.selection);
        self.tabs.retain(|tab| match tab {
            None => true,
            Some(k) => {
                tab == selection
                    || drafts.contains_key(k)
                    || state
                        .collection(&k.root)
                        .is_some_and(|c| matches!(c.find(&k.path), Some(Item::Request(_))))
            }
        });
    }

    /// The tab strip above the editor (R-COL-13, R-COL-14).
    pub(super) fn tab_bar(&mut self, ui: &mut Ui) {
        let groups = self.tab_groups(ui);
        let p = theme::palette(ui);
        let frame = Frame::new().fill(p.chrome).inner_margin(Margin {
            left: 8,
            right: 8,
            top: 6,
            bottom: 0,
        });
        let actions = Panel::top("tabs")
            .frame(frame)
            .show(ui, |ui| tab_strip(ui, &mut self.tab_strip, &groups))
            .inner;
        for action in actions {
            self.on_tab(action);
        }
    }

    pub(super) fn tab_groups(&self, ui: &Ui) -> Vec<TabGroup> {
        let mut roots: Vec<&Path> = self.tabs.projects();
        if let Some((Some(k), true)) = self.tabs.strip().last()
            && !roots.contains(&k.root.as_path())
        {
            roots.push(&k.root);
        }
        let grouped = roots.len() > 1;
        let project_name = |root: &Path| {
            self.state
                .collection(root)
                .map(|c| c.name().to_owned())
                .unwrap_or_default()
        };
        let dirty = self.dirty_keys();
        let mut groups: Vec<TabGroup> = Vec::new();
        for (key, preview) in self.tabs.strip() {
            let root = key.as_ref().map(|k| k.root.as_path());
            let color = root
                .filter(|_| grouped)
                .map(|r| theme::project_color(ui, &project_name(r)));
            let tab = self.tab_view(key, preview, color, &dirty);
            let same_group = groups.last().is_some_and(|g| {
                !preview
                    && g.tabs.last().is_some_and(|t| {
                        !t.preview && t.key.as_ref().map(|k| k.root.as_path()) == root
                    })
            });
            if same_group && let Some(group) = groups.last_mut() {
                group.tabs.push(tab);
                continue;
            }
            // The preview tab gets its project's chip when it follows another
            // project's tabs, so it does not look like part of that group.
            let after_own_group = groups
                .last()
                .and_then(|g| g.tabs.last())
                .is_some_and(|t| t.key.as_ref().map(|k| k.root.as_path()) == root);
            let chip = match (root, color) {
                (Some(root), Some(color)) if !preview || !after_own_group => Some(GroupChip {
                    root: root.to_owned(),
                    name: project_name(root),
                    color,
                    // A folded chip would hide the preview tab.
                    collapsed: !preview && self.tabs.is_collapsed(root),
                }),
                _ => None,
            };
            groups.push(TabGroup {
                chip,
                tabs: vec![tab],
            });
        }
        groups
    }

    fn tab_view(
        &self,
        key: &Selection,
        preview: bool,
        color: Option<egui::Color32>,
        dirty: &std::collections::HashSet<ItemKey>,
    ) -> TabView {
        let active = *key == self.selection && self.workflows.open.is_none();
        let (method, name) = if *key == self.selection {
            (self.current.method, self.current.name.clone())
        } else {
            match key {
                None => (self.scratch.method, self.scratch.name.clone()),
                Some(k) => match self.drafts.get(k) {
                    Some(r) => (r.method, r.name.clone()),
                    None => match self.state.collection(&k.root).and_then(|c| c.find(&k.path)) {
                        Some(Item::Request(r)) => (r.request.method, r.request.name.clone()),
                        _ => (HttpMethod::Get, String::new()),
                    },
                },
            }
        };
        let place = match key {
            None => "Scratch request".to_owned(),
            Some(k) => breadcrumb(&self.state, k)
                .trim_end_matches(" /")
                .replace(" / ", " › "),
        };
        TabView {
            key: key.clone(),
            method,
            name,
            place,
            preview,
            active,
            dirty: key.as_ref().is_some_and(|k| dirty.contains(k)),
            conflict: key.as_ref().is_some_and(|k| self.conflicts.contains_key(k)),
            color,
        }
    }

    pub(super) fn on_tab(&mut self, action: TabAction) {
        match action {
            TabAction::Select(key) => self.select(key),
            TabAction::Keep(key) => self.open_tab(key, true),
            TabAction::Close(key) => self.close_tabs(&[key]),
            TabAction::CloseOthers(key) => {
                let others: Vec<Selection> = self
                    .tabs
                    .strip()
                    .map(|(k, _)| k.clone())
                    .filter(|k| *k != key)
                    .collect();
                self.close_tabs(&others);
                self.select(key);
            }
            TabAction::CloseToTheRight(key) => {
                let right = self.tabs.right_of(&key);
                self.close_tabs(&right);
            }
            TabAction::CloseSaved => {
                let dirty = self.dirty_keys();
                let saved: Vec<Selection> = self
                    .tabs
                    .strip()
                    .map(|(k, _)| k.clone())
                    .filter(|k| match k {
                        Some(k) => !dirty.contains(k) && !self.conflicts.contains_key(k),
                        // The scratch request is never saved; it closes when empty.
                        None => {
                            *k == self.selection && !self.current_is_edited()
                                || *k != self.selection && self.scratch == Request::default()
                        }
                    })
                    .collect();
                self.close_tabs(&saved);
            }
            TabAction::ToggleGroup(root) => self.tabs.toggle_group(&root),
            TabAction::CloseGroup(root) => self.ask_to_close_project_tabs(&root),
        }
    }

    /// Closes a project's tabs, asking first when some of them have unsaved edits
    /// (R-COL-37). The edits stay either way (R-COL-6).
    fn ask_to_close_project_tabs(&mut self, root: &Path) {
        let dirty = self.dirty_keys();
        let unsaved = self
            .project_tabs(root)
            .iter()
            .flatten()
            .filter(|k| dirty.contains(*k) || self.conflicts.contains_key(*k))
            .count();
        if unsaved == 0 {
            self.close_project_tabs(root);
            return;
        }
        let name = self
            .state
            .collection(root)
            .map(|c| c.name().to_owned())
            .unwrap_or_default();
        let message = if unsaved == 1 {
            "1 of its requests has unsaved edits.".to_owned()
        } else {
            format!("{unsaved} of its requests have unsaved edits.")
        };
        let dialog = ConfirmDialog {
            title: format!("Close the tabs of “{name}”?"),
            message: format!(
                "{message} They are not lost: opening the request again shows them, until you save or revert."
            ),
            confirm_label: "Close tabs".into(),
        };
        self.modal = Some(Modal::CloseProjectTabs(root.to_owned(), dialog));
    }

    /// Closes every tab of the project at `root` (R-COL-14, R-COL-37).
    pub(super) fn close_project_tabs(&mut self, root: &Path) {
        let group = self.project_tabs(root);
        self.close_tabs(&group);
    }

    fn project_tabs(&self, root: &Path) -> Vec<Selection> {
        self.tabs
            .strip()
            .map(|(k, _)| k.clone())
            .filter(|k| k.as_ref().is_some_and(|k| k.root == root))
            .collect()
    }

    /// Closes tabs. Unsaved edits stay (R-COL-6): reopening the request shows them.
    pub(super) fn close_tabs(&mut self, keys: &[Selection]) {
        self.keep_if_edited();
        for key in keys {
            self.tabs.close(key);
        }
        if keys.contains(&self.selection) {
            let next = self.tabs.fallback().unwrap_or_default();
            // Closing the shown tab while a workflow is open leaves the workflow open.
            let workflow = self.workflows.open.take();
            self.select_and_remember(next, true);
            self.workflows.open = workflow;
        }
    }

    /// Ctrl+Tab, Ctrl+PageDown/PageUp, Ctrl+W (R-COL-13) and Ctrl+1 … Ctrl+9 (R-COL-27).
    pub(super) fn tab_shortcuts(&mut self, ui: &Ui) {
        let (previous, next, before, close, place) = ui.input_mut(|i| {
            (
                i.consume_shortcut(&PREVIOUS_TAB),
                i.consume_shortcut(&NEXT_TAB),
                i.consume_shortcut(&TAB_BEFORE),
                i.consume_shortcut(&CLOSE_TAB),
                GO_TO_TAB.iter().position(|k| i.consume_shortcut(k)),
            )
        });
        let target = if let Some(place) = place {
            self.tabs.at_place(place, place + 1 == GO_TO_TAB.len())
        } else if previous {
            self.tabs.previously_used(&self.selection)
        } else if next {
            self.tabs.neighbour(&self.selection, 1)
        } else if before {
            self.tabs.neighbour(&self.selection, -1)
        } else {
            None
        };
        if let Some(target) = target {
            self.select(target);
        }
        if close && self.workflows.open.is_none() {
            let shown = self.selection.clone();
            self.close_tabs(&[shown]);
        }
    }
}

/// The open tabs of the workspace.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(super) struct Tabs {
    /// Kept tabs in strip order; tabs of one project are next to each other.
    kept: Vec<Selection>,
    /// The preview tab, shown after the kept ones.
    preview: Option<Selection>,
    /// Projects whose group is folded to its chip (R-COL-14).
    collapsed: HashSet<PathBuf>,
    /// Tabs by last use, most recent first (Ctrl+Tab).
    used: Vec<Selection>,
}

/// The project of a tab; `None` for the scratch request.
fn root_of(key: &Selection) -> Option<&Path> {
    key.as_ref().map(|k| k.root.as_path())
}

impl Tabs {
    pub fn from_saved(saved: &SavedTabs) -> Self {
        let mut tabs = Self {
            collapsed: saved.collapsed.iter().cloned().collect(),
            ..Self::default()
        };
        for tab in &saved.open {
            let key = match (&tab.root, &tab.path) {
                (Some(root), Some(path)) => Some(ItemKey::new(root, path.clone())),
                _ => None,
            };
            if tab.preview {
                tabs.preview = Some(key);
            } else if !tabs.kept.contains(&key) {
                tabs.insert_kept(key);
            }
        }
        if tabs.preview.as_ref().is_some_and(|p| tabs.kept.contains(p)) {
            tabs.preview = None;
        }
        tabs
    }

    pub fn to_saved(&self) -> SavedTabs {
        let mut collapsed: Vec<PathBuf> = self.collapsed.iter().cloned().collect();
        collapsed.sort();
        SavedTabs {
            open: self
                .strip()
                .map(|(key, preview)| SavedTab {
                    root: key.as_ref().map(|k| k.root.clone()),
                    path: key.as_ref().map(|k| k.path.clone()),
                    preview,
                })
                .collect(),
            collapsed,
        }
    }

    /// Every tab in strip order, with whether it is the preview tab.
    pub fn strip(&self) -> impl Iterator<Item = (&Selection, bool)> {
        self.kept
            .iter()
            .map(|k| (k, false))
            .chain(self.preview.iter().map(|k| (k, true)))
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.kept.len() + usize::from(self.preview.is_some())
    }

    pub fn contains(&self, key: &Selection) -> bool {
        self.preview.as_ref() == Some(key) || self.kept.contains(key)
    }

    pub fn is_preview(&self, key: &Selection) -> bool {
        self.preview.as_ref() == Some(key)
    }

    /// Opens `key`: as a kept tab if `keep`, else in the preview tab (which replaces
    /// the previous preview). A request that already has a tab keeps it, and is kept
    /// if `keep` is set.
    pub fn open(&mut self, key: &Selection, keep: bool) {
        if self.is_preview(key) {
            if keep {
                self.keep(key);
            }
        } else if !self.kept.contains(key) {
            if keep {
                self.insert_kept(key.clone());
            } else {
                if let Some(old) = self.preview.take() {
                    self.used.retain(|u| *u != old);
                }
                self.preview = Some(key.clone());
            }
        }
        if let Some(root) = root_of(key) {
            self.collapsed.remove(root);
        }
        self.used.retain(|u| u != key);
        self.used.insert(0, key.clone());
    }

    /// The tab at `place` (from 0) in strip order, or the last tab when `last` (Ctrl+9,
    /// as in browsers) (R-COL-27).
    pub fn at_place(&self, place: usize, last: bool) -> Option<Selection> {
        if last {
            self.strip().last()
        } else {
            self.strip().nth(place)
        }
        .map(|(k, _)| k.clone())
    }

    /// Turns the preview tab, if there is one, into a kept tab (R-SET-9).
    pub fn keep_preview(&mut self) {
        if let Some(key) = self.preview.clone() {
            self.keep(&key);
        }
    }

    /// Turns the preview tab into a kept tab, in its project's group.
    pub fn keep(&mut self, key: &Selection) {
        if self.is_preview(key) {
            self.preview = None;
            self.insert_kept(key.clone());
        }
    }

    /// Adds a kept tab at the end of its project's group, or as a new group at the end.
    fn insert_kept(&mut self, key: Selection) {
        let root = root_of(&key);
        let at = self
            .kept
            .iter()
            .rposition(|k| root_of(k) == root)
            .map_or(self.kept.len(), |i| i + 1);
        self.kept.insert(at, key);
    }

    pub fn close(&mut self, key: &Selection) {
        if self.is_preview(key) {
            self.preview = None;
        }
        self.kept.retain(|k| k != key);
        self.used.retain(|u| u != key);
    }

    /// The tab to show when the shown one closed: the most recently used, else the last.
    pub fn fallback(&self) -> Option<Selection> {
        self.used
            .first()
            .or_else(|| self.strip().last().map(|(k, _)| k))
            .cloned()
    }

    /// The tab used before `current` (Ctrl+Tab).
    pub fn previously_used(&self, current: &Selection) -> Option<Selection> {
        self.used.iter().find(|u| *u != current).cloned()
    }

    /// The tab `by` places from `current` in strip order, wrapping around
    /// (Ctrl+PageDown / Ctrl+PageUp).
    pub fn neighbour(&self, current: &Selection, by: isize) -> Option<Selection> {
        let strip: Vec<&Selection> = self.strip().map(|(k, _)| k).collect();
        let len = isize::try_from(strip.len()).ok()?;
        if len < 2 {
            return None;
        }
        let i = isize::try_from(strip.iter().position(|k| *k == current)?).ok()?;
        let next = usize::try_from((i + by).rem_euclid(len)).ok()?;
        Some(strip[next].clone())
    }

    /// The tabs right of `key` in strip order.
    pub fn right_of(&self, key: &Selection) -> Vec<Selection> {
        self.strip()
            .map(|(k, _)| k)
            .skip_while(|k| *k != key)
            .skip(1)
            .cloned()
            .collect()
    }

    pub fn is_collapsed(&self, root: &Path) -> bool {
        self.collapsed.contains(root)
    }

    pub fn toggle_group(&mut self, root: &Path) {
        if !self.collapsed.remove(root) {
            self.collapsed.insert(root.to_owned());
        }
    }

    /// Forgets folded groups that have no chip any more: their project's tabs are
    /// all closed, or only one project has tabs (R-COL-14).
    pub fn tidy_collapsed(&mut self) {
        let projects: Vec<PathBuf> = self.projects().into_iter().map(Path::to_owned).collect();
        if projects.len() < 2 {
            self.collapsed.clear();
        } else {
            self.collapsed.retain(|root| projects.contains(root));
        }
    }

    /// Keeps only the tabs for which `keep` is true.
    pub fn retain(&mut self, mut keep: impl FnMut(&Selection) -> bool) {
        self.kept.retain(&mut keep);
        if self.preview.as_ref().is_some_and(|p| !keep(p)) {
            self.preview = None;
        }
        self.used.retain(keep);
    }

    /// Tabs follow a file or folder that was renamed or moved.
    pub fn remap(&mut self, root: &Path, old: &Path, new: &Path) {
        let moved = |k: &mut Selection| {
            if let Some(to) = k.as_ref().and_then(|k| k.moved(root, old, new)) {
                *k = Some(to);
            }
        };
        self.kept.iter_mut().for_each(moved);
        self.preview.iter_mut().for_each(moved);
        self.used.iter_mut().for_each(moved);
    }

    /// Projects of the kept tabs, in strip order (R-COL-14).
    pub fn projects(&self) -> Vec<&Path> {
        let mut roots: Vec<&Path> = Vec::new();
        for root in self.kept.iter().filter_map(root_of) {
            if roots.last() != Some(&root) {
                roots.push(root);
            }
        }
        roots
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[expect(clippy::unnecessary_wraps, reason = "tabs are keyed by `Selection`")]
    fn key(root: &str, path: &str) -> Selection {
        Some(ItemKey::new(Path::new(root), path))
    }

    fn strip(tabs: &Tabs) -> Vec<(Selection, bool)> {
        tabs.strip().map(|(k, p)| (k.clone(), p)).collect()
    }

    #[test]
    fn a_single_click_replaces_the_preview_tab() {
        let mut tabs = Tabs::default();
        tabs.open(&key("/a", "one.toml"), false);
        tabs.open(&key("/a", "two.toml"), false);
        assert_eq!(strip(&tabs), [(key("/a", "two.toml"), true)]);
    }

    #[test]
    fn keeping_moves_the_preview_into_its_project_group() {
        let mut tabs = Tabs::default();
        tabs.open(&key("/a", "one.toml"), true);
        tabs.open(&key("/b", "one.toml"), true);
        tabs.open(&key("/a", "two.toml"), false);
        assert!(tabs.is_preview(&key("/a", "two.toml")));
        tabs.keep(&key("/a", "two.toml"));
        assert_eq!(
            strip(&tabs),
            [
                (key("/a", "one.toml"), false),
                (key("/a", "two.toml"), false),
                (key("/b", "one.toml"), false),
            ]
        );
        assert_eq!(tabs.projects(), [Path::new("/a"), Path::new("/b")]);
    }

    #[test]
    fn opening_a_kept_tab_leaves_the_preview_alone() {
        let mut tabs = Tabs::default();
        tabs.open(&key("/a", "one.toml"), true);
        tabs.open(&key("/a", "two.toml"), false);
        tabs.open(&key("/a", "one.toml"), false);
        assert_eq!(tabs.len(), 2);
        assert!(tabs.is_preview(&key("/a", "two.toml")));
    }

    #[test]
    fn closing_goes_back_to_the_tab_used_before() {
        let mut tabs = Tabs::default();
        tabs.open(&key("/a", "one.toml"), true);
        tabs.open(&key("/a", "two.toml"), true);
        tabs.open(&key("/a", "three.toml"), true);
        tabs.open(&key("/a", "one.toml"), false);
        assert_eq!(
            tabs.previously_used(&key("/a", "one.toml")),
            Some(key("/a", "three.toml"))
        );
        tabs.close(&key("/a", "one.toml"));
        assert_eq!(tabs.len(), 2);
        assert_eq!(tabs.fallback(), Some(key("/a", "three.toml")));
    }

    #[test]
    fn neighbours_wrap_around() {
        let mut tabs = Tabs::default();
        tabs.open(&None, true);
        tabs.open(&key("/a", "one.toml"), true);
        tabs.open(&key("/a", "two.toml"), false);
        assert_eq!(tabs.neighbour(&None, 1), Some(key("/a", "one.toml")));
        assert_eq!(tabs.neighbour(&None, -1), Some(key("/a", "two.toml")));
        assert_eq!(
            tabs.right_of(&None),
            [key("/a", "one.toml"), key("/a", "two.toml")]
        );
    }

    #[test]
    fn opening_a_request_expands_its_group() {
        let mut tabs = Tabs::default();
        tabs.open(&key("/a", "one.toml"), true);
        tabs.toggle_group(Path::new("/a"));
        assert!(tabs.is_collapsed(Path::new("/a")));
        tabs.open(&key("/a", "two.toml"), false);
        assert!(!tabs.is_collapsed(Path::new("/a")));

        tabs.open(&key("/b", "one.toml"), true);
        tabs.toggle_group(Path::new("/a"));
        tabs.tidy_collapsed();
        assert!(
            tabs.is_collapsed(Path::new("/a")),
            "two projects: chips, so it stays"
        );
        tabs.close(&key("/b", "one.toml"));
        tabs.tidy_collapsed();
        assert!(!tabs.is_collapsed(Path::new("/a")), "no chips: forgotten");
    }

    #[test]
    fn tabs_follow_moves_and_go_with_deletes() {
        let mut tabs = Tabs::default();
        tabs.open(&key("/a", "users/one.toml"), true);
        tabs.open(&key("/a", "two.toml"), false);
        tabs.remap(Path::new("/a"), Path::new("users"), Path::new("people"));
        assert!(tabs.contains(&key("/a", "people/one.toml")));
        tabs.retain(|k| *k != key("/a", "two.toml"));
        assert_eq!(strip(&tabs), [(key("/a", "people/one.toml"), false)]);
    }

    #[test]
    fn saved_tabs_round_trip() {
        let mut tabs = Tabs::default();
        tabs.open(&None, true);
        tabs.open(&key("/a", "one.toml"), true);
        tabs.open(&key("/b", "two.toml"), false);
        tabs.toggle_group(Path::new("/a"));
        let restored = Tabs::from_saved(&tabs.to_saved());
        assert_eq!(strip(&restored), strip(&tabs));
        assert!(restored.is_collapsed(Path::new("/a")));
    }

    /// R-SET-9: with the preview tab off, a single click opens a tab that stays, and a
    /// preview tab left from before (or restored) is kept too.
    #[test]
    fn without_the_preview_tab_every_request_gets_a_tab() {
        let (mut ui, root, first) = crate::shell::tests::setup("no-preview-tab");
        let second = crate::shell::tests::add_request(&mut ui, &root, "List users ü😀");
        ui.sync_tabs();
        assert!(ui.tabs.is_preview(&Some(first.clone())), "on by default");

        ui.state.update_settings(|s| s.editing.preview_tab = false);
        ui.sync_tabs();
        assert_eq!(strip(&ui.tabs), [(Some(first.clone()), false)], "kept");
        ui.select(Some(second.clone()));
        ui.sync_tabs();
        assert_eq!(
            strip(&ui.tabs),
            [(Some(first), false), (Some(second), false)],
            "a single click keeps its tab"
        );
        std::fs::remove_dir_all(root).expect("clean up");
    }
}
