//! Web launch (experimental): draws on the `tortuga_canvas` element of the page.
//! Nothing is saved yet, and collections need a file system, so they are unavailable.

use eframe::wasm_bindgen::JsCast;
use eframe::web_sys;
use tortuga_core::AppSettings;
use tortuga_core::secrets::{MemorySecretStore, SecretStore};
use tortuga_core::storage::{Storage, UnsupportedStorage};
use tortuga_ui_egui::{BoxTask, Host, LocalState, TortugaUi};

use crate::App;

pub fn start() {
    wasm_bindgen_futures::spawn_local(async {
        let Some(canvas) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("tortuga_canvas"))
            .and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok())
        else {
            web_sys::console::error_1(
                &"Tortuga: no <canvas id=\"tortuga_canvas\"> on the page".into(),
            );
            return;
        };
        let started = eframe::WebRunner::new()
            .start(
                canvas,
                eframe::WebOptions::default(),
                Box::new(|_cc| Ok(Box::new(App(TortugaUi::new(Box::new(WebHost::default())))))),
            )
            .await;
        if let Err(e) = started {
            web_sys::console::error_1(&e);
        }
    });
}

/// Keeps everything for the session only.
#[derive(Default)]
struct WebHost {
    secrets: MemorySecretStore,
}

impl Host for WebHost {
    fn storage(&self) -> &dyn Storage {
        &UnsupportedStorage
    }

    fn secrets(&self) -> &dyn SecretStore {
        &self.secrets
    }

    fn secrets_warning(&self) -> Option<&str> {
        Some("The web version keeps secret values only until the page is closed.")
    }

    fn spawn(&self, task: BoxTask) {
        wasm_bindgen_futures::spawn_local(task);
    }

    fn load_settings(&self) -> Result<AppSettings, String> {
        Ok(AppSettings::default())
    }

    fn save_settings(&self, _: &AppSettings) -> Result<(), String> {
        Ok(())
    }

    fn load_local(&self) -> Result<LocalState, String> {
        Ok(LocalState::default())
    }

    fn save_local(&self, _: &LocalState) -> Result<(), String> {
        Ok(())
    }
}
