//! What the request editor asks for: sending, defaults dialogs, files to upload, and
//! breakpoints in its scripts (R-TEST-10).

use tortuga_core::script::ScriptKind;

use super::{Modal, Selection, TortugaUi};
use crate::components::{DefaultsDialog, EditorAction};

impl TortugaUi {
    /// Does what the request editor asked for.
    pub(super) fn apply_editor_action(
        &mut self,
        ctx: &egui::Context,
        action: EditorAction,
        key: &Selection,
    ) {
        let root = key.as_ref().map(|k| k.root.clone());
        match action {
            EditorAction::Send => self.send(ctx),
            EditorAction::Cancel => self.cancel(),
            EditorAction::PasteCurl(text) => self.paste_curl(&text),
            EditorAction::ToggleBreakpoint(script, line) => {
                self.toggle_breakpoint(key, script, line);
            }
            EditorAction::ChooseFile(index) => self.choose_upload(root.as_deref(), index),
            EditorAction::EditCollectionDefaults => {
                if let Some(root) = root {
                    self.modal = Some(Modal::Defaults(
                        DefaultsDialog::new(&self.state, root).on_auth(),
                    ));
                }
            }
            EditorAction::EditWorkspaceDefaults => {
                self.modal = Some(Modal::Defaults(
                    DefaultsDialog::for_workspace(&self.state).on_auth(),
                ));
            }
        }
    }

    /// Sets or clears the breakpoint on `line` of a script of the request `key` (R-TEST-10).
    fn toggle_breakpoint(&mut self, key: &Selection, script: ScriptKind, line: u32) {
        let breakpoints = &mut self.sent.entry(key.clone()).or_default().breakpoints;
        let lines = match script {
            ScriptKind::PreRequest => &mut breakpoints.pre_request,
            ScriptKind::Tests | ScriptKind::StepTests => &mut breakpoints.tests,
        };
        if !lines.remove(&line) {
            lines.insert(line);
        }
    }
}
