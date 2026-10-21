//! "Save automatically" (R-SET-11, D53): the shown request and the open workflow are
//! written to disk a moment after the last key press or click, and before the app
//! moves to another one or closes. Never while a conflict banner waits for an answer
//! (R-GIT-11), never the scratch request, and never in a project that is read-only.

use egui::Event;

use super::TortugaUi;

/// How long input has to stop before edits are saved.
pub(super) const AUTO_SAVE_SECONDS: f64 = 1.0;

impl TortugaUi {
    /// Once per frame: saves once input has stopped for [`AUTO_SAVE_SECONDS`].
    pub(super) fn auto_save_now_and_then(&mut self, ctx: &egui::Context) {
        if !self.state.settings.editing.auto_save {
            return;
        }
        let (now, input) = ctx.input(|i| (i.time, i.events.iter().any(is_input)));
        if input {
            self.last_input_at = now;
        }
        if !self.request_to_auto_save() && !self.workflow_to_auto_save() {
            return;
        }
        let wait = AUTO_SAVE_SECONDS - (now - self.last_input_at);
        if wait > 0.0 {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(wait));
        } else {
            self.auto_save();
        }
    }

    /// Saves what can be saved automatically now, e.g. before another request opens.
    pub(super) fn auto_save(&mut self) {
        if !self.state.settings.editing.auto_save {
            return;
        }
        if self.request_to_auto_save() {
            self.save();
        }
        if self.workflow_to_auto_save() {
            self.save_open_workflow();
        }
    }

    /// The shown request has edits that may be saved automatically.
    fn request_to_auto_save(&self) -> bool {
        let Some(key) = &self.selection else {
            return false;
        };
        self.state
            .collection(&key.root)
            .is_some_and(|c| !c.read_only)
            && !self.conflicts.contains_key(key)
            && !self.unchecked_bases.contains_key(key)
            && self.is_dirty(key, &self.current)
    }

    /// The open workflow has edits that may be saved automatically.
    fn workflow_to_auto_save(&self) -> bool {
        self.workflows.open.as_ref().is_some_and(|key| {
            self.state
                .collection(&key.root)
                .is_some_and(|c| !c.read_only)
                && self.workflow_dirty(key)
        })
    }
}

/// Anything the user typed or clicked; moving the pointer doesn't count.
fn is_input(event: &Event) -> bool {
    !matches!(
        event,
        Event::PointerMoved(_)
            | Event::MouseMoved(_)
            | Event::PointerGone
            | Event::WindowFocused(_)
    )
}

#[cfg(test)]
pub(super) mod tests {
    use std::path::Path;

    use super::super::DiskChange;
    use super::super::tests::{add_request, setup};
    use super::*;

    /// Runs `ui` for `seconds` of egui time (a frame every 0.25 s), with a key press in
    /// every frame while `typing`.
    pub(in crate::shell) fn run(ui: &mut TortugaUi, seconds: f32, typing: bool) {
        // A new egui context each time, which needs the fonts and style again.
        ui.styled = false;
        ui.applied_appearance = None;
        let mut h = egui_kittest::Harness::builder().build_ui(|egui_ui| ui.ui(egui_ui));
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a few frames"
        )]
        for _ in 0..(seconds / 0.25).round() as usize {
            if typing {
                h.key_press(egui::Key::A);
            }
            h.step();
        }
    }

    fn on_disk(root: &Path, key: &crate::state::ItemKey) -> String {
        std::fs::read_to_string(root.join(&key.path)).expect("request file")
    }

    /// R-SET-11, D53: edits are saved a moment after input stops, not while typing.
    #[test]
    fn edits_are_saved_once_typing_stops() {
        let (mut ui, root, key) = setup("auto-save-pause");
        ui.state.update_settings(|s| s.editing.auto_save = true);
        ui.current.url = "https://a.test/ü😀".into();
        let saved = || on_disk(&root, &key).contains("ü😀");
        {
            let mut h = egui_kittest::Harness::builder().build_ui(|egui_ui| ui.ui(egui_ui));
            for _ in 0..8 {
                h.key_press(egui::Key::A);
                h.step();
            }
            assert!(!saved(), "not while typing");
            h.step();
            h.step();
            assert!(!saved(), "not half a second after");
            h.step();
            h.step();
            assert!(saved(), "a second after the last key press");
        }
        assert!(!ui.is_dirty(&key, &ui.current));
        std::fs::remove_dir_all(root).expect("clean up");
    }

    /// R-SET-11: off by default; the scratch request is never saved.
    #[test]
    fn nothing_is_saved_when_off_or_for_the_scratch_request() {
        let (mut ui, root, key) = setup("auto-save-off");
        ui.current.url = "https://a.test/edited".into();
        run(&mut ui, 2.0, false);
        assert!(!on_disk(&root, &key).contains("edited"), "off by default");

        ui.state.update_settings(|s| s.editing.auto_save = true);
        ui.select(None);
        ui.current.url = "https://scratch.test/".into();
        run(&mut ui, 2.0, false);
        assert_eq!(ui.current.url, "https://scratch.test/");
        assert!(ui.modal.is_none(), "no \"Save to project\" dialog");
        std::fs::remove_dir_all(root).expect("clean up");
    }

    /// R-SET-11, R-GIT-11: while the conflict banner waits for an answer, nothing is
    /// saved over the file that changed on disk.
    #[test]
    fn not_while_a_conflict_waits() {
        let (mut ui, root, key) = setup("auto-save-conflict");
        ui.state.update_settings(|s| s.editing.auto_save = true);
        ui.current.url = "https://a.test/mine".into();
        ui.conflicts.insert(key.clone(), DiskChange::Changed);
        run(&mut ui, 2.0, false);
        assert!(!on_disk(&root, &key).contains("mine"));
        ui.conflicts.clear();
        run(&mut ui, 1.5, false);
        assert!(on_disk(&root, &key).contains("mine"), "saved once answered");
        std::fs::remove_dir_all(root).expect("clean up");
    }

    /// R-SET-11: opening another request saves the edits first, without waiting; a
    /// renamed request is saved under its new file name.
    #[test]
    fn edits_are_saved_before_another_request_opens() {
        let (mut ui, root, key) = setup("auto-save-switch");
        let other = add_request(&mut ui, &root, "Other");
        ui.state.update_settings(|s| s.editing.auto_save = true);
        ui.current.url = "https://a.test/switched".into();
        ui.current.name = "Renamed ü😀".into();
        ui.select(Some(other.clone()));
        assert_eq!(ui.selection, Some(other));
        assert!(ui.drafts.is_empty(), "nothing left unsaved");
        let files: Vec<String> = std::fs::read_dir(&root)
            .expect("project")
            .map(|e| std::fs::read_to_string(e.expect("entry").path()).unwrap_or_default())
            .collect();
        assert!(
            files
                .iter()
                .any(|f| f.contains("Renamed ü😀") && f.contains("https://a.test/switched")),
            "saved with its new name"
        );
        assert!(!root.join(&key.path).exists(), "the old file name is gone");
        std::fs::remove_dir_all(root).expect("clean up");
    }
}
