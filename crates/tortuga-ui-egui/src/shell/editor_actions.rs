//! What the request editor asks for: sending, defaults dialogs, files to upload, and
//! breakpoints in its scripts (R-TEST-10).

use tortuga_core::runner::{self, Extra, StepOutcome};
use tortuga_core::script::ScriptKind;
use tortuga_core::{Scripts, vars};

use super::{Modal, Selection, TortugaUi};
use crate::components::{DefaultsDialog, EditorAction, Outcome, TestsView};

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
            EditorAction::RerunTests => self.rerun_tests(key),
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

    /// Runs the captures and test script of the selected request again on its last
    /// response, without sending (R-TEST-12). What the pre-request script did on that send
    /// stays in the console; variables the tests set apply as after a send.
    fn rerun_tests(&mut self, key: &Selection) {
        let Some(sent) = self.sent.get(key) else {
            return;
        };
        let Some(Outcome::Response(view)) = &sent.outcome else {
            return;
        };
        let response = view.response.clone();
        let breakpoints = sent.breakpoints.clone();
        let earlier = sent.tests.clone().unwrap_or_default();

        let mut ctx = self.state.request_context(key.as_ref());
        // As it would be sent now: `req` in the tests.
        let as_sent = vars::prepare(&self.current, &ctx).unwrap_or_else(|_| self.current.clone());
        let mut out = StepOutcome::default();
        let extra = Extra {
            breakpoints: Some(&breakpoints),
            ..Extra::default()
        };
        runner::after_response(
            &self.current,
            &as_sent,
            &response,
            &mut ctx,
            &extra,
            &mut out,
        );
        let pre_request = earlier
            .console
            .iter()
            .filter(|(k, _)| *k == ScriptKind::PreRequest)
            .cloned();
        out.console.splice(0..0, pre_request);

        let tests = TestsView::from_step(&out).map(|t| TestsView {
            ran: Scripts {
                pre_request: earlier.ran.pre_request.clone(),
                tests: self.current.scripts.tests.clone(),
            },
            ..t
        });
        if let Some(t) = &tests {
            self.state
                .apply_runtime(key.as_ref().map(|k| k.root.as_path()), &t.vars);
        }
        self.sent.entry(key.clone()).or_default().tests = tests;
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
