//! Desktop launch and platform services: local files, native dialogs, a tokio runtime.

use std::path::PathBuf;

use eframe::egui;
use tortuga_core::ClientSettings;
use tortuga_core::secrets::SecretStore;
use tortuga_core::storage::{FsStorage, Storage};
use tortuga_ui_egui::{BoxTask, Host, LocalState, TortugaUi};

use crate::{App, dialogs, local_store, secret_store};

pub fn run() -> eframe::Result {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("tortuga-http")
        .enable_all()
        .build()
        .map_err(|e| eframe::Error::AppCreation(Box::new(e)))?;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Tortuga")
            .with_app_id("tortuga")
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([640.0, 420.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Tortuga",
        options,
        Box::new(|_cc| {
            let (secrets, secrets_warning) = secret_store::open(local_store::secrets_path());
            let host = DesktopHost {
                runtime,
                secrets,
                secrets_warning,
            };
            Ok(Box::new(App(TortugaUi::new(Box::new(host)))))
        }),
    )
}

/// Requests run on a small tokio runtime, so the UI thread never waits for the network.
struct DesktopHost {
    runtime: tokio::runtime::Runtime,
    secrets: Box<dyn SecretStore>,
    secrets_warning: Option<String>,
}

impl Host for DesktopHost {
    fn storage(&self) -> &dyn Storage {
        &FsStorage
    }

    fn secrets(&self) -> &dyn SecretStore {
        self.secrets.as_ref()
    }

    fn secrets_warning(&self) -> Option<&str> {
        self.secrets_warning.as_deref()
    }

    fn load_cookies(&self, root: &std::path::Path) -> Result<Option<String>, String> {
        local_store::load_cookies(root)
    }

    fn save_cookies(&self, root: &std::path::Path, json: &str) -> Result<(), String> {
        local_store::save_cookies(root, json)
    }

    fn spawn(&self, task: BoxTask) {
        drop(self.runtime.spawn(task));
    }

    fn load_settings(&self) -> Result<ClientSettings, String> {
        local_store::load_settings()
    }

    fn save_settings(&self, settings: &ClientSettings) -> Result<(), String> {
        local_store::save_settings(settings)
    }

    fn load_local(&self) -> Result<LocalState, String> {
        local_store::load_local()
    }

    fn save_local(&self, state: &LocalState) -> Result<(), String> {
        local_store::save_local(state)
    }

    fn can_pick_folders(&self) -> bool {
        true
    }

    fn pick_folder(&self, title: &str) -> Option<PathBuf> {
        dialogs::pick_folder(title)
    }

    fn can_save_files(&self) -> bool {
        true
    }

    fn save_file(&self, suggested_name: &str, contents: &[u8]) -> Option<Result<PathBuf, String>> {
        let path = dialogs::save_path(suggested_name)?;
        Some(
            std::fs::write(&path, contents)
                .map(|()| path)
                .map_err(|e| e.to_string()),
        )
    }
}
