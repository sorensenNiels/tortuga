//! Following changes on disk, e.g. from `git pull`, and the conflict banner (R-GIT-10, R-GIT-11).

use std::path::Path;

use egui::{RichText, Ui};
use tortuga_core::Request;

use super::{ConflictChoice, DiskChange, TortugaUi};
use crate::state::ItemKey;
use crate::theme;

impl TortugaUi {
    /// A collection changed on disk (git pull, branch switch, another editor): reload
    /// it and bring open requests up to date (R-GIT-10). Requests with unsaved edits
    /// are not overwritten; the user chooses in a banner (R-GIT-11).
    pub(super) fn on_disk_change(&mut self, root: &Path) {
        let in_root = |k: &ItemKey| k.root == root;
        let mut watched: Vec<ItemKey> =
            self.drafts.keys().filter(|k| in_root(k)).cloned().collect();
        if let Some(k) = self.selection.as_ref().filter(|k| in_root(k))
            && !watched.contains(k)
        {
            watched.push(k.clone());
        }
        let before: Vec<(ItemKey, Option<Request>, bool)> = watched
            .into_iter()
            .map(|k| {
                let saved = self.state.saved_request(&k);
                let dirty = self.drafts.contains_key(&k)
                    || (self.selection.as_ref() == Some(&k)
                        && saved.as_ref() != Some(&self.current));
                (k, saved, dirty)
            })
            .collect();

        self.state.reload(root);
        self.state.refresh_git(root);
        // Responses of requests deleted in git go too; unsaved edits stay for the banner.
        let state = &self.state;
        let exists = |k: &ItemKey| {
            k.root != root
                || state
                    .collection(root)
                    .is_some_and(|c| c.request(&k.path).is_some())
        };
        self.sent.retain(|k, _| k.as_ref().is_none_or(exists));
        self.recent.retain(exists);

        for (key, old, dirty) in before {
            let new = self.state.saved_request(&key);
            if new == old {
                continue;
            }
            if dirty {
                let change = if new.is_some() {
                    DiskChange::Changed
                } else {
                    DiskChange::Deleted
                };
                self.conflicts.insert(key, change);
            } else if self.selection.as_ref() == Some(&key) {
                match new {
                    Some(request) => self.load(request),
                    None => self.forget(&key.root, &key.path),
                }
            }
        }
    }

    /// Answers the conflict banner of the selected request (R-GIT-11, D9).
    pub(super) fn resolve_conflict(&mut self, key: &ItemKey, choice: ConflictChoice) {
        let Some(change) = self.conflicts.remove(key) else {
            return;
        };
        match (change, choice) {
            // Keep the edits; saving will write them over the new version.
            (DiskChange::Changed, ConflictChoice::KeepMine) => {}
            (DiskChange::Changed, ConflictChoice::TakeTheirs) => {
                self.drafts.remove(key);
                if self.selection.as_ref() == Some(key) {
                    self.load(self.state.saved_request(key).unwrap_or_default());
                }
            }
            // The file is gone: restore it, with the edits, where it was.
            (DiskChange::Deleted, ConflictChoice::KeepMine) => {
                let request = if self.selection.as_ref() == Some(key) {
                    self.current.clone()
                } else {
                    self.drafts.get(key).cloned().unwrap_or_default()
                };
                let parent = key.path.parent().map(Path::to_path_buf).unwrap_or_default();
                if let Some(path) = self
                    .state
                    .run(&key.root, |c, s| c.new_request(s, &parent, &request))
                {
                    self.drafts.remove(key);
                    let restored = ItemKey::new(&key.root, path);
                    if self.selection.as_ref() == Some(key) {
                        self.selection = Some(restored);
                        self.current = request;
                    }
                }
            }
            (DiskChange::Deleted, ConflictChoice::TakeTheirs) => self.forget(&key.root, &key.path),
        }
    }

    /// The banner shown above a request with a pending conflict.
    pub(super) fn conflict_banner(&mut self, ui: &mut Ui, key: &ItemKey) {
        let Some(change) = self.conflicts.get(key).copied() else {
            return;
        };
        let p = theme::palette(ui);
        let (message, keep, take) = match change {
            DiskChange::Changed => (
                t!("conflict-changed"),
                t!("conflict-keep-mine"),
                t!("conflict-take-theirs"),
            ),
            DiskChange::Deleted => (
                t!("conflict-deleted"),
                t!("conflict-restore"),
                t!("conflict-discard"),
            ),
        };
        let mut choice = None;
        theme::banner_frame(ui, false)
            .corner_radius(egui::CornerRadius::same(8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("⚠").color(p.warning_text));
                    ui.label(RichText::new(message).color(p.warning_text));
                    if ui.button(keep).clicked() {
                        choice = Some(ConflictChoice::KeepMine);
                    }
                    if ui.button(take).clicked() {
                        choice = Some(ConflictChoice::TakeTheirs);
                    }
                });
            });
        ui.add_space(8.0);
        if let Some(choice) = choice {
            self.resolve_conflict(key, choice);
        }
    }
}
