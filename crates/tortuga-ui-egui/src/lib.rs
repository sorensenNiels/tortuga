//! Tortuga's user interface, built with [egui](https://docs.rs/egui).
//!
//! UI state and rendering only: parsing, HTTP, variables and file formats live in
//! `tortuga-core`. Everything that depends on the platform (file system, native
//! dialogs, async runtime, where settings are kept) comes in through a [`Host`],
//! which the launcher (`tortuga-app`) provides.
//!
//! ```ignore
//! let mut ui = TortugaUi::new(Box::new(my_host));
//! // every frame:
//! ui.ui(egui_ui);
//! ```

mod components;
mod format;
mod fuzzy;
mod host;
mod shell;
mod state;
mod theme;

pub use host::{
    BoxTask, DraftConflict, Host, LocalState, SavedDraft, SavedDrafts, SavedTab, SavedTabs,
    SavedWorkflowDraft, Workspace,
};
pub use shell::TortugaUi;

#[cfg(test)]
pub(crate) mod test_host {
    use std::path::PathBuf;

    use tortuga_core::ClientSettings;
    use tortuga_core::secrets::{MemorySecretStore, SecretStore};
    use tortuga_core::storage::{FsStorage, Storage};

    use crate::host::{BoxTask, Host, LocalState};

    /// A test harness for `app` with Tortuga's fonts and style installed.
    pub fn harness<'a>(mut app: impl FnMut(&mut egui::Ui) + 'a) -> egui_kittest::Harness<'a> {
        let mut installed = false;
        egui_kittest::Harness::new_ui(move |ui| {
            if !installed {
                crate::theme::install(ui.ctx());
                installed = true;
            }
            app(ui);
        })
    }

    /// A host that keeps nothing and runs no tasks (it counts them).
    #[derive(Debug, Default)]
    pub struct TestHost {
        pub open: Vec<PathBuf>,
        pub spawned: std::rc::Rc<std::cell::Cell<usize>>,
        pub secrets: MemorySecretStore,
        /// Saved cookie jars, by collection root.
        pub cookies: std::rc::Rc<std::cell::RefCell<std::collections::BTreeMap<PathBuf, String>>>,
        /// The last saved local state; loaded instead of `open` once set.
        pub local: std::rc::Rc<std::cell::RefCell<Option<LocalState>>>,
        /// The last saved unsaved edits (R-COL-8).
        pub drafts: std::rc::Rc<std::cell::RefCell<crate::host::SavedDrafts>>,
        /// History entries by project (`None`: scratch), newest first (R-HIST-1).
        pub history: std::rc::Rc<
            std::cell::RefCell<
                std::collections::BTreeMap<Option<PathBuf>, Vec<tortuga_core::history::Entry>>,
            >,
        >,
    }

    impl Host for TestHost {
        fn storage(&self) -> &dyn Storage {
            &FsStorage
        }
        fn secrets(&self) -> &dyn SecretStore {
            &self.secrets
        }
        fn load_cookies(&self, root: &std::path::Path) -> Result<Option<String>, String> {
            Ok(self.cookies.borrow().get(root).cloned())
        }
        fn save_cookies(&self, root: &std::path::Path, json: &str) -> Result<(), String> {
            self.cookies
                .borrow_mut()
                .insert(root.to_owned(), json.to_owned());
            Ok(())
        }
        fn spawn(&self, _task: BoxTask) {
            self.spawned.set(self.spawned.get() + 1);
        }
        fn load_settings(&self) -> Result<ClientSettings, String> {
            Ok(ClientSettings::default())
        }
        fn save_settings(&self, _: &ClientSettings) -> Result<(), String> {
            Ok(())
        }
        fn load_local(&self) -> Result<LocalState, String> {
            Ok(self.local.borrow().clone().unwrap_or_else(|| LocalState {
                open_collections: self.open.clone(),
                ..LocalState::default()
            }))
        }
        fn save_local(&self, state: &LocalState) -> Result<(), String> {
            *self.local.borrow_mut() = Some(state.clone());
            Ok(())
        }
        fn load_drafts(&self) -> Result<crate::host::SavedDrafts, String> {
            Ok(self.drafts.borrow().clone())
        }
        fn save_drafts(&self, drafts: &crate::host::SavedDrafts) -> Result<(), String> {
            *self.drafts.borrow_mut() = drafts.clone();
            Ok(())
        }
        fn keeps_history(&self) -> bool {
            true
        }
        fn file_reader(&self) -> Option<tortuga_core::FileReader> {
            Some(tortuga_core::FileReader(std::sync::Arc::new(
                |path: &std::path::Path| std::fs::read(path),
            )))
        }
        fn history_list(
            &self,
            root: Option<&std::path::Path>,
        ) -> Result<Vec<tortuga_core::history::Summary>, String> {
            let history = self.history.borrow();
            let entries = history.get(&root.map(std::path::Path::to_path_buf));
            Ok(entries
                .map(|e| e.iter().map(|e| e.summary.clone()).collect())
                .unwrap_or_default())
        }
        fn history_entry(
            &self,
            root: Option<&std::path::Path>,
            id: &str,
        ) -> Result<tortuga_core::history::Entry, String> {
            self.history
                .borrow()
                .get(&root.map(std::path::Path::to_path_buf))
                .and_then(|e| e.iter().find(|e| e.summary.id == id).cloned())
                .ok_or_else(|| "no such entry".to_owned())
        }
        fn add_history(&self, root: Option<&std::path::Path>, entry: tortuga_core::history::Entry) {
            self.history
                .borrow_mut()
                .entry(root.map(std::path::Path::to_path_buf))
                .or_default()
                .insert(0, entry);
        }
        fn clear_all_history(&self) -> Result<(), String> {
            self.history.borrow_mut().clear();
            Ok(())
        }
        fn clear_history(&self, root: Option<&std::path::Path>) -> Result<(), String> {
            self.history
                .borrow_mut()
                .remove(&root.map(std::path::Path::to_path_buf));
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    use super::*;

    #[test]
    fn app_renders_the_request_workspace() {
        let mut app = TortugaUi::new(Box::new(test_host::TestHost::default()));
        let mut harness = Harness::new_ui(move |ui| app.ui(ui));
        harness.run();
        harness.get_by_label("Send");
        harness.get_by_label("Scratch request");
        harness.get_by_label("Send a request to see the response here.");
    }

    #[test]
    fn ctrl_enter_sends() {
        let host = test_host::TestHost::default();
        let spawned = host.spawned.clone();
        let mut app = TortugaUi::new(Box::new(host));
        let mut harness = test_host::harness(move |ui| app.ui(ui));
        harness.run();
        harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Enter);
        // `step`, not `run`: the "Sending…" spinner keeps repainting.
        harness.step();
        assert_eq!(spawned.get(), 1, "Ctrl+Enter starts a send");
    }

    /// egui panics on characters its bundled fonts lack (e.g. ▸ ● ✕ ⋯), so every symbol
    /// the UI uses is checked here. Add new ones to this list.
    #[test]
    fn bundled_fonts_have_every_symbol_the_ui_uses() {
        let mut harness = test_host::harness(|ui| {
            for symbol in [
                "⏵", "⏷", "⏺", "⚠", "🗙", "⚙", "✏", "•", "…", "—", "“", "”", "🐢", "·", "🔒", "›",
                "↑", "↓", "→", "☰", "✓", "✗",
            ] {
                ui.label(symbol);
                ui.monospace(symbol);
                ui.label(theme::semibold(symbol));
            }
        });
        harness.run();
    }
}
