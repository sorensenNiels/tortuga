//! Tortuga application entry point.
//!
//! Keep this crate thin: it opens the window (or browser canvas) and provides the
//! platform services ([`tortuga_ui_egui::Host`]) the UI needs. UI code belongs in
//! `tortuga-ui-egui`; anything that could be reused by a CLI belongs in `tortuga-core`.

// Release builds on Windows are GUI apps: no console window opens next to Tortuga.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
#![cfg_attr(
    not(target_arch = "wasm32"),
    expect(
        clippy::print_stdout,
        clippy::print_stderr,
        reason = "a binary reports to the terminal"
    )
)]

#[cfg(all(feature = "desktop", not(target_arch = "wasm32")))]
mod desktop;
#[cfg(all(feature = "desktop", not(target_arch = "wasm32")))]
mod dialogs;
#[cfg(all(feature = "desktop", not(target_arch = "wasm32")))]
mod discover;
#[cfg(all(feature = "desktop", not(target_arch = "wasm32")))]
mod folders;
#[cfg(all(feature = "desktop", not(target_arch = "wasm32")))]
mod git;
#[cfg(all(feature = "desktop", not(target_arch = "wasm32")))]
mod local_store;
#[cfg(all(feature = "desktop", not(target_arch = "wasm32")))]
mod secret_store;
#[cfg(all(feature = "desktop", not(target_arch = "wasm32")))]
mod watch;
#[cfg(all(feature = "web", target_arch = "wasm32"))]
mod web;

use tortuga_ui_egui::TortugaUi;

#[cfg(all(not(feature = "desktop"), not(target_arch = "wasm32")))]
compile_error!("Build natively with the `desktop` feature (the default); `web` is for wasm32.");

/// Runs the UI inside eframe.
struct App(TortugaUi);

impl eframe::App for App {
    fn ui(&mut self, ui: &mut eframe::egui::Ui, _frame: &mut eframe::Frame) {
        self.0.ui(ui);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.0.on_exit();
    }
}

#[cfg(all(feature = "desktop", not(target_arch = "wasm32")))]
fn main() -> eframe::Result {
    desktop::run()
}

#[cfg(target_arch = "wasm32")]
fn main() {
    #[cfg(feature = "web")]
    web::start();
}
