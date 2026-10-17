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
mod host;
mod shell;
mod state;
mod theme;

pub use host::{BoxTask, Host, LocalState};
pub use shell::TortugaUi;

#[cfg(test)]
pub(crate) mod test_host {
    use std::path::PathBuf;

    use tortuga_core::ClientSettings;
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
    }

    impl Host for TestHost {
        fn storage(&self) -> &dyn Storage {
            &FsStorage
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
            Ok(LocalState {
                open_collections: self.open.clone(),
                ..LocalState::default()
            })
        }
        fn save_local(&self, _: &LocalState) -> Result<(), String> {
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
                "⏵", "⏷", "⏺", "⚠", "🗙", "⚙", "✏", "•", "…", "—", "“", "”", "🐢", "·",
            ] {
                ui.label(symbol);
                ui.monospace(symbol);
                ui.label(theme::semibold(symbol));
            }
        });
        harness.run();
    }
}
