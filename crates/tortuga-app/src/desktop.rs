//! Desktop launch and platform services: local files, native dialogs, a tokio runtime.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui;
use tortuga_core::AppSettings;
use tortuga_core::history::{self, HistoryStore};
use tortuga_core::secrets::SecretStore;
use tortuga_core::storage::{FsStorage, Storage};
use tortuga_ui_egui::{BoxTask, Host, LocalState, TortugaUi};

use crate::{App, dialogs, discover, folders, git, local_store, secret_store, watch};

const USAGE: &str = "\
Usage: tortuga [OPTION]

Options:
  --startup-time  Open the app, print how long start-up took, and quit
  --version       Print the version and quit
  --help          Print this help and quit";

pub fn run() -> eframe::Result {
    let started = Instant::now();
    let mut measure_startup = false;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--startup-time" => measure_startup = true,
            "--version" | "-V" => {
                println!("tortuga {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "--help" | "-h" => {
                println!("{USAGE}");
                return Ok(());
            }
            // Older macOS versions pass a process serial number to apps opened from Finder.
            _ if arg.starts_with("-psn_") => {}
            _ => {
                eprintln!("tortuga: unknown option '{arg}'\n\n{USAGE}");
                std::process::exit(2);
            }
        }
    }

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("tortuga-http")
        .enable_all()
        .build()
        .map_err(|e| eframe::Error::AppCreation(Box::new(e)))?;
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Tortuga")
        .with_app_id("tortuga")
        .with_inner_size([1200.0, 800.0])
        .with_min_inner_size([640.0, 420.0]);
    match eframe::icon_data::from_png_bytes(include_bytes!("../icons/256.png")) {
        Ok(icon) => viewport = viewport.with_icon(Arc::new(icon)),
        Err(e) => eprintln!("tortuga: could not load the window icon: {e}"),
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "Tortuga",
        options,
        Box::new(move |_cc| {
            let window = started.elapsed();
            let (secrets, secrets_warning) = secret_store::open(local_store::secrets_path());
            let host = DesktopHost {
                runtime,
                secrets,
                secrets_warning,
                watchers: watch::Watchers::default(),
                history_lock: Arc::default(),
            };
            let app = App(TortugaUi::new(Box::new(host)));
            if measure_startup {
                Ok(Box::new(StartupTimer {
                    app,
                    started,
                    window,
                    loaded: started.elapsed(),
                    frames: 0,
                    frame_times: Vec::new(),
                }))
            } else {
                Ok(Box::new(app))
            }
        }),
    )
}

/// `--startup-time`: runs the app until its first frame is on screen, prints how long
/// each step took, and closes the window (R-NF-1: cold start under 1 s).
struct StartupTimer {
    app: App,
    started: Instant,
    /// Until the window and graphics context existed.
    window: Duration,
    /// Until the workspace, its projects, secrets and drafts were loaded.
    loaded: Duration,
    frames: usize,
    frame_times: Vec<Duration>,
}

/// Frames timed after the first one is shown.
const FRAMES_TIMED: usize = 30;

impl eframe::App for StartupTimer {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let frame_started = Instant::now();
        self.app.ui(ui, frame);
        self.frames += 1;
        // The first frame is shown once its `ui` call returns and it is painted,
        // so it is on screen when the second one starts.
        if self.frames == 2 {
            println!(
                "Window opened after {} ms, workspace loaded after {} ms, first frame shown after {} ms.",
                self.window.as_millis(),
                self.loaded.as_millis(),
                self.started.elapsed().as_millis(),
            );
        }
        // Then a few more frames, to see how long building one takes (typing must never lag).
        if self.frames > 2 {
            self.frame_times.push(frame_started.elapsed());
        }
        if self.frames == 2 + FRAMES_TIMED {
            self.frame_times.sort();
            let median = self.frame_times[self.frame_times.len() / 2];
            let slowest = self.frame_times.last().copied().unwrap_or_default();
            println!(
                "Building a frame takes {:.1} ms (slowest of {FRAMES_TIMED}: {:.1} ms).",
                median.as_secs_f64() * 1000.0,
                slowest.as_secs_f64() * 1000.0,
            );
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
        ui.ctx().request_repaint();
    }

    fn on_exit(&mut self, gl: Option<&eframe::glow::Context>) {
        self.app.on_exit(gl);
    }
}

/// Requests run on a small tokio runtime, so the UI thread never waits for the network.
struct DesktopHost {
    runtime: tokio::runtime::Runtime,
    secrets: Box<dyn SecretStore>,
    secrets_warning: Option<String>,
    watchers: watch::Watchers,
    /// One history write at a time, so two sends can't both rewrite an index.
    history_lock: Arc<std::sync::Mutex<()>>,
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

    fn watch(
        &self,
        root: &std::path::Path,
        changed: Box<dyn Fn() + Send + Sync>,
    ) -> Result<(), String> {
        self.watchers.watch(root, changed)
    }

    fn unwatch(&self, root: &std::path::Path) {
        self.watchers.unwatch(root);
    }

    fn git_status(
        &self,
        root: &std::path::Path,
        done: Box<dyn FnOnce(Result<Option<tortuga_core::git::GitStatus>, String>) + Send>,
    ) {
        // Reading a big repository can take a moment: never on the UI thread.
        let root = root.to_owned();
        drop(
            self.runtime
                .spawn_blocking(move || done(git::status(&root))),
        );
    }

    fn repository_of(&self, dir: &std::path::Path) -> Option<PathBuf> {
        git::repository_of(dir)
    }

    fn can_init_git(&self) -> bool {
        true
    }

    fn init_git(&self, dir: &std::path::Path) -> Result<(), String> {
        git::init(dir)
    }

    fn find_projects(
        &self,
        dir: &std::path::Path,
        done: Box<dyn FnOnce(Result<Vec<PathBuf>, String>) + Send>,
    ) {
        // Searching a big folder takes a moment: never on the UI thread.
        let dir = dir.to_owned();
        drop(
            self.runtime
                .spawn_blocking(move || done(discover::find_projects(&dir))),
        );
    }

    fn can_open_folders(&self) -> bool {
        true
    }

    fn open_in_file_manager(&self, path: &std::path::Path) -> Result<(), String> {
        folders::open_in_file_manager(path)
    }

    fn open_in_terminal(&self, path: &std::path::Path) -> Result<(), String> {
        folders::open_in_terminal(path)
    }

    fn spawn(&self, task: BoxTask) {
        drop(self.runtime.spawn(task));
    }

    fn file_reader(&self) -> Option<tortuga_core::FileReader> {
        Some(tortuga_core::FileReader(Arc::new(
            |path: &std::path::Path| std::fs::read(path),
        )))
    }

    fn pick_any_file(&self, title: &str) -> Option<PathBuf> {
        dialogs::pick_any_file(title)
    }

    fn keeps_history(&self) -> bool {
        true
    }

    fn history_list(
        &self,
        root: Option<&std::path::Path>,
    ) -> Result<Vec<history::Summary>, String> {
        let Some(dir) = local_store::history_dir(root) else {
            return Ok(Vec::new());
        };
        HistoryStore::new(&local_store::PrivateStorage, dir)
            .list()
            .map_err(|e| e.to_string())
    }

    fn history_entry(
        &self,
        root: Option<&std::path::Path>,
        id: &str,
    ) -> Result<history::Entry, String> {
        let dir = local_store::history_dir(root).ok_or("No app-data directory available")?;
        HistoryStore::new(&local_store::PrivateStorage, dir)
            .load(id)
            .map_err(|e| e.to_string())
    }

    fn add_history(&self, root: Option<&std::path::Path>, entry: history::Entry, keep: usize) {
        let Some(dir) = local_store::history_dir(root) else {
            return;
        };
        let lock = Arc::clone(&self.history_lock);
        // Writing up to a megabyte (and syncing it) would make the UI stutter.
        drop(self.runtime.spawn_blocking(move || {
            let _one_at_a_time = lock
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Err(e) = HistoryStore::new(&local_store::PrivateStorage, dir).add(&entry, keep) {
                eprintln!("tortuga: could not add to the history: {e}");
            }
        }));
    }

    fn trim_history(&self, keep: usize) -> Result<(), String> {
        let Some(all) = local_store::all_history_dir() else {
            return Ok(());
        };
        let _one_at_a_time = self
            .history_lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Ok(dirs) = std::fs::read_dir(&all) else {
            return Ok(()); // no history yet
        };
        for dir in dirs.flatten().filter(|d| d.path().is_dir()) {
            HistoryStore::new(&local_store::PrivateStorage, dir.path())
                .trim(keep)
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    fn clear_history(&self, root: Option<&std::path::Path>) -> Result<(), String> {
        let Some(dir) = local_store::history_dir(root) else {
            return Ok(());
        };
        let _one_at_a_time = self
            .history_lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        HistoryStore::new(&local_store::PrivateStorage, dir)
            .clear()
            .map_err(|e| e.to_string())
    }

    fn clear_all_history(&self) -> Result<(), String> {
        let Some(dir) = local_store::all_history_dir() else {
            return Ok(());
        };
        let _one_at_a_time = self
            .history_lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match std::fs::remove_dir_all(&dir) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                Err(format!("{}: {e}", dir.display()))
            }
            _ => Ok(()),
        }
    }

    fn utc_offset_minutes(&self) -> i32 {
        chrono::Local::now().offset().local_minus_utc() / 60
    }

    fn load_settings(&self) -> Result<AppSettings, String> {
        local_store::load_settings()
    }

    fn save_settings(&self, settings: &AppSettings) -> Result<(), String> {
        local_store::save_settings(settings)
    }

    fn load_local(&self) -> Result<LocalState, String> {
        local_store::load_local()
    }

    fn save_local(&self, state: &LocalState) -> Result<(), String> {
        local_store::save_local(state)
    }

    fn load_drafts(&self) -> Result<tortuga_ui_egui::SavedDrafts, String> {
        local_store::load_drafts()
    }

    fn save_drafts(&self, drafts: &tortuga_ui_egui::SavedDrafts) -> Result<(), String> {
        local_store::save_drafts(drafts)
    }

    fn can_pick_folders(&self) -> bool {
        true
    }

    fn pick_folder(&self, title: &str) -> Option<PathBuf> {
        dialogs::pick_folder(title)
    }

    fn pick_file(&self, title: &str, suffix: &str) -> Option<PathBuf> {
        dialogs::pick_file(title, suffix)
    }

    fn pick_save_path(&self, title: &str, suggested_name: &str) -> Option<PathBuf> {
        dialogs::save_path(title, suggested_name)
    }

    fn can_save_files(&self) -> bool {
        true
    }

    fn save_file(&self, suggested_name: &str, contents: &[u8]) -> Option<Result<PathBuf, String>> {
        let path = dialogs::save_path("Save response body", suggested_name)?;
        Some(
            std::fs::write(&path, contents)
                .map(|()| path)
                .map_err(|e| e.to_string()),
        )
    }
}
