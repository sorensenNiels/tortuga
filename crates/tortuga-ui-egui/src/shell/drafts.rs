//! Unsaved edits: kept per request, written to local data now and then, and restored
//! on the next start (R-COL-6, R-COL-8).

use tortuga_core::Request;

use super::{DRAFTS_SAVE_SECONDS, DiskChange, TortugaUi};
use crate::host::{DraftConflict, SavedDraft, SavedDrafts};
use crate::state::ItemKey;

impl TortugaUi {
    /// Unsaved edits as they should be kept now.
    pub(super) fn drafts_snapshot(&self) -> SavedDrafts {
        let mut edited = self.drafts.clone();
        if let Some(k) = &self.selection {
            if self.is_dirty(k, &self.current) {
                edited.insert(k.clone(), self.current.clone());
            } else {
                edited.remove(k);
            }
        }
        let mut drafts: Vec<SavedDraft> = edited
            .into_iter()
            .map(|(key, request)| {
                let base = if self.state.collection(&key.root).is_some() {
                    self.state.saved_request(&key)
                } else {
                    self.unchecked_bases.get(&key).cloned().flatten()
                };
                let conflict = self.conflicts.get(&key).map(|c| match c {
                    DiskChange::Changed => DraftConflict::Changed,
                    DiskChange::Deleted => DraftConflict::Deleted,
                });
                SavedDraft {
                    root: key.root,
                    path: key.path,
                    request,
                    base,
                    conflict,
                }
            })
            .collect();
        // A stable order, so an unchanged set is never written again.
        drafts.sort_by(|a, b| (&a.root, &a.path).cmp(&(&b.root, &b.path)));
        let scratch = if self.selection.is_none() {
            &self.current
        } else {
            &self.scratch
        };
        SavedDrafts {
            scratch: (*scratch != Request::default()).then(|| scratch.clone()),
            drafts,
            workflows: self.workflow_drafts_snapshot(),
        }
    }

    /// Keeps unsaved edits for next time, checking at most every [`DRAFTS_SAVE_SECONDS`].
    pub(super) fn save_drafts_now_and_then(&mut self, now: f64) {
        if now - self.drafts_checked_at >= DRAFTS_SAVE_SECONDS {
            self.drafts_checked_at = now;
            self.save_drafts();
        }
    }

    /// Writes unsaved edits to local data if they changed since the last write.
    pub(super) fn save_drafts(&mut self) {
        let snapshot = self.drafts_snapshot();
        if snapshot == self.drafts_saved {
            return;
        }
        if let Err(e) = self.state.host.save_drafts(&snapshot) {
            self.state.notify(format!(
                "Unsaved edits could not be kept for next time: {e}"
            ));
        }
        self.drafts_saved = snapshot;
    }

    /// Brings back the unsaved edits of the last session.
    pub(super) fn restore_drafts(&mut self) {
        let saved = match self.state.host.load_drafts() {
            Ok(saved) => saved,
            Err(e) => {
                self.state.notify(format!(
                    "Could not read the unsaved edits of last time: {e}"
                ));
                return;
            }
        };
        self.scratch = saved.scratch.clone().unwrap_or_default();
        self.load(self.scratch.clone());
        for draft in &saved.drafts {
            let key = ItemKey::new(&draft.root, draft.path.clone());
            self.drafts.insert(key.clone(), draft.request.clone());
            let change = match draft.conflict {
                Some(DraftConflict::Changed) => Some(DiskChange::Changed),
                Some(DraftConflict::Deleted) => Some(DiskChange::Deleted),
                None => None,
            };
            match change {
                Some(change) => {
                    self.conflicts.insert(key, change);
                }
                None => {
                    self.unchecked_bases.insert(key, draft.base.clone());
                }
            }
        }
        self.restore_workflow_drafts(&saved.workflows);
        self.drafts_saved = saved;
        self.check_restored_drafts();
    }

    /// Compares restored edits with the disk once their project is open: a file that
    /// changed while the app was closed gets the conflict banner (R-GIT-11); edits
    /// that now equal the file are dropped.
    pub(super) fn check_restored_drafts(&mut self) {
        self.check_waiting_workflows();
        let open: Vec<ItemKey> = self
            .unchecked_bases
            .keys()
            .filter(|k| self.state.collection(&k.root).is_some())
            .cloned()
            .collect();
        for key in open {
            let Some(base) = self.unchecked_bases.remove(&key) else {
                continue;
            };
            let saved = self.state.saved_request(&key);
            if saved != base {
                let change = if saved.is_some() {
                    DiskChange::Changed
                } else {
                    DiskChange::Deleted
                };
                self.conflicts.insert(key, change);
            } else if saved.as_ref() == self.drafts.get(&key) {
                self.drafts.remove(&key);
            }
        }
    }
}
