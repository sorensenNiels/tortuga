//! Watches open collections for changes on disk (R-GIT-10).
//!
//! Events are debounced, so a `git pull` or branch switch that touches many files
//! results in one reload. The repository's `.git` folder is watched too (not
//! recursively), so a commit or checkout refreshes the git status even when no
//! collection file changed. Where the OS gives no change events (Windows drives
//! under WSL), the files are polled instead.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use notify_debouncer_mini::notify::{self, RecursiveMode};
use notify_debouncer_mini::{Config, DebounceEventResult, Debouncer, new_debouncer_opt};

/// Quiet time before a burst of changes is reported.
const DEBOUNCE: Duration = Duration::from_millis(400);
/// How often files are checked where there are no change events.
const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// Called (on a background thread) after something in a collection changed.
pub type OnChange = Box<dyn Fn() + Send + Sync>;

/// Kept only to keep watching: dropping a debouncer stops it.
#[expect(dead_code, reason = "held for its Drop")]
enum Handle {
    Native(Debouncer<notify::RecommendedWatcher>),
    Polling(Debouncer<notify::PollWatcher>),
}

/// The active watchers, one per collection root.
#[derive(Default)]
pub struct Watchers {
    active: Mutex<HashMap<PathBuf, Handle>>,
}

impl Watchers {
    pub fn watch(&self, root: &Path, on_change: OnChange) -> Result<(), String> {
        let git_dir = gix::discover(root)
            .ok()
            .map(|repo| repo.git_dir().to_owned());
        let handler = move |result: DebounceEventResult| {
            if result.is_ok_and(|events| !events.is_empty()) {
                on_change();
            }
        };
        let handle = if needs_polling(root) {
            let config = Config::default()
                .with_timeout(DEBOUNCE)
                .with_notify_config(notify::Config::default().with_poll_interval(POLL_INTERVAL));
            let mut debouncer = new_debouncer_opt::<_, notify::PollWatcher>(config, handler)
                .map_err(|e| e.to_string())?;
            add_paths(debouncer.watcher(), root, git_dir.as_deref())?;
            Handle::Polling(debouncer)
        } else {
            let config = Config::default().with_timeout(DEBOUNCE);
            let mut debouncer = new_debouncer_opt::<_, notify::RecommendedWatcher>(config, handler)
                .map_err(|e| e.to_string())?;
            add_paths(debouncer.watcher(), root, git_dir.as_deref())?;
            Handle::Native(debouncer)
        };
        self.lock().insert(root.to_owned(), handle);
        Ok(())
    }

    pub fn unwatch(&self, root: &Path) {
        // Dropping the debouncer stops it.
        self.lock().remove(root);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<PathBuf, Handle>> {
        self.active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

fn add_paths(
    watcher: &mut dyn notify::Watcher,
    root: &Path,
    git_dir: Option<&Path>,
) -> Result<(), String> {
    watcher
        .watch(root, RecursiveMode::Recursive)
        .map_err(|e| format!("Could not watch {}: {e}", root.display()))?;
    if let Some(git_dir) = git_dir {
        // HEAD and index change on commit, checkout and staging. Not fatal if it fails.
        let _ = watcher.watch(git_dir, RecursiveMode::NonRecursive);
    }
    Ok(())
}

/// Windows drives seen from WSL (`/mnt/c/…`) send no change events.
fn needs_polling(root: &Path) -> bool {
    cfg!(target_os = "linux")
        && root.starts_with("/mnt/")
        && std::fs::read_to_string("/proc/version")
            .is_ok_and(|v| v.to_ascii_lowercase().contains("microsoft"))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[test]
    fn reports_a_change_once_after_a_burst() {
        let dir = std::env::temp_dir().join(format!("tortuga-watch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        let count = Arc::new(AtomicUsize::new(0));
        let seen = count.clone();
        let watchers = Watchers::default();
        watchers
            .watch(
                &dir,
                Box::new(move || {
                    seen.fetch_add(1, Ordering::SeqCst);
                }),
            )
            .expect("watch");
        for i in 0..5 {
            std::fs::write(dir.join(format!("r{i}.toml")), "url = \"/\"\n").expect("write");
        }
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while count.load(Ordering::SeqCst) == 0 && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        std::thread::sleep(DEBOUNCE * 2);
        assert_eq!(count.load(Ordering::SeqCst), 1, "five writes, one report");
        watchers.unwatch(&dir);
        std::fs::remove_dir_all(dir).expect("clean up");
    }

    #[test]
    fn only_windows_drives_under_wsl_are_polled() {
        assert!(!needs_polling(Path::new("/home/me/api")));
    }
}
