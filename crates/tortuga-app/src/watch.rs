//! Watches open projects for changes on disk (R-GIT-10), with one watcher per git
//! repository however many of its projects are open (R-WS-4).
//!
//! Events are debounced, so a `git pull` or branch switch that touches many files
//! results in one reload. The repository's `.git` folder is watched too (not
//! recursively), so a commit or checkout refreshes the git status of every project
//! in it even when no project file changed. Where the OS gives no change events
//! (Windows drives under WSL), the files are polled instead.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use notify_debouncer_mini::notify::{self, RecursiveMode};
use notify_debouncer_mini::{Config, DebounceEventResult, Debouncer, new_debouncer_opt};

/// Quiet time before a burst of changes is reported.
const DEBOUNCE: Duration = Duration::from_millis(400);
/// How often files are checked where there are no change events.
const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// Called (on a background thread) after something in a project changed.
pub type OnChange = Box<dyn Fn() + Send + Sync>;

/// The open projects of one watcher: (canonical root, callback).
type Projects = Arc<Mutex<Vec<(PathBuf, OnChange)>>>;

/// Dropping a debouncer stops it.
enum Handle {
    Native(Debouncer<notify::RecommendedWatcher>),
    Polling(Debouncer<notify::PollWatcher>),
}

impl Handle {
    fn watcher(&mut self) -> &mut dyn notify::Watcher {
        match self {
            Self::Native(d) => d.watcher(),
            Self::Polling(d) => d.watcher(),
        }
    }
}

/// One watcher: a repository (or a project outside git) and its open projects.
struct Watch {
    handle: Handle,
    projects: Projects,
}

/// The active watchers, by repository `.git` folder (or project root outside git).
#[derive(Default)]
pub struct Watchers {
    active: Mutex<HashMap<PathBuf, Watch>>,
    /// Project root, as the UI names it → (watcher key, canonical root).
    roots: Mutex<HashMap<PathBuf, (PathBuf, PathBuf)>>,
}

impl Watchers {
    pub fn watch(&self, root: &Path, on_change: OnChange) -> Result<(), String> {
        self.unwatch(root);
        let canonical = std::fs::canonicalize(root)
            .map_err(|e| format!("Could not watch {}: {e}", root.display()))?;
        let git_dir = gix::discover(&canonical)
            .ok()
            .and_then(|repo| std::fs::canonicalize(repo.git_dir()).ok());
        let key = git_dir.clone().unwrap_or_else(|| canonical.clone());

        let mut active = lock(&self.active);
        if !active.contains_key(&key) {
            let watch = start(&canonical, git_dir.as_deref())?;
            active.insert(key.clone(), watch);
        }
        let Some(watch) = active.get_mut(&key) else {
            return Err("watcher vanished".into());
        };
        watch
            .handle
            .watcher()
            .watch(&canonical, RecursiveMode::Recursive)
            .map_err(|e| format!("Could not watch {}: {e}", root.display()))?;
        lock(&watch.projects).push((canonical.clone(), on_change));
        lock(&self.roots).insert(root.to_owned(), (key, canonical));
        Ok(())
    }

    pub fn unwatch(&self, root: &Path) {
        let Some((key, canonical)) = lock(&self.roots).remove(root) else {
            return;
        };
        let mut active = lock(&self.active);
        let Some(watch) = active.get_mut(&key) else {
            return;
        };
        let left = {
            let mut projects = lock(&watch.projects);
            projects.retain(|(r, _)| *r != canonical);
            projects.len()
        };
        if left == 0 {
            // Dropping the debouncer stops it.
            active.remove(&key);
        } else {
            let _ = watch.handle.watcher().unwatch(&canonical);
        }
    }

    /// Number of running watchers (for tests).
    #[cfg(test)]
    fn count(&self) -> usize {
        lock(&self.active).len()
    }
}

/// Starts a watcher that passes each burst of changes to the projects it concerns.
fn start(root: &Path, git_dir: Option<&Path>) -> Result<Watch, String> {
    let projects: Projects = Arc::default();
    let for_handler = projects.clone();
    let git = git_dir.map(Path::to_path_buf);
    let handler = move |result: DebounceEventResult| {
        let Ok(events) = result else {
            return;
        };
        let projects = lock(&for_handler);
        for (root, on_change) in projects.iter() {
            let concerned = events.iter().any(|e| {
                e.path.starts_with(root) || git.as_ref().is_some_and(|g| e.path.starts_with(g))
            });
            if concerned {
                on_change();
            }
        }
    };
    let mut handle = if needs_polling(root) {
        let config = Config::default()
            .with_timeout(DEBOUNCE)
            .with_notify_config(notify::Config::default().with_poll_interval(POLL_INTERVAL));
        Handle::Polling(
            new_debouncer_opt::<_, notify::PollWatcher>(config, handler)
                .map_err(|e| e.to_string())?,
        )
    } else {
        let config = Config::default().with_timeout(DEBOUNCE);
        Handle::Native(
            new_debouncer_opt::<_, notify::RecommendedWatcher>(config, handler)
                .map_err(|e| e.to_string())?,
        )
    };
    if let Some(git_dir) = git_dir {
        // HEAD and index change on commit, checkout and staging. Not fatal if it fails.
        let _ = handle.watcher().watch(git_dir, RecursiveMode::NonRecursive);
    }
    Ok(Watch { handle, projects })
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
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
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    fn counter() -> (Arc<AtomicUsize>, OnChange) {
        let count = Arc::new(AtomicUsize::new(0));
        let seen = count.clone();
        (
            count,
            Box::new(move || {
                seen.fetch_add(1, Ordering::SeqCst);
            }),
        )
    }

    /// Waits until `count` is non-zero (or a timeout), then for the debouncer to settle.
    fn settle(count: &AtomicUsize) {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while count.load(Ordering::SeqCst) == 0 && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        std::thread::sleep(DEBOUNCE * 3);
    }

    #[test]
    fn reports_a_burst_of_changes_together() {
        let dir = std::env::temp_dir().join(format!("tortuga-watch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        let (count, on_change) = counter();
        let watchers = Watchers::default();
        watchers.watch(&dir, on_change).expect("watch");
        for i in 0..5 {
            std::fs::write(dir.join(format!("r{i}.toml")), "url = \"/\"\n").expect("write");
        }
        settle(&count);
        // Each file is debounced on its own, so a burst can be split in two under load;
        // the UI takes in all reports once per frame anyway.
        let reports = count.load(Ordering::SeqCst);
        assert!((1..=2).contains(&reports), "five writes, {reports} reports");
        watchers.unwatch(&dir);
        assert_eq!(watchers.count(), 0);
        std::fs::remove_dir_all(dir).expect("clean up");
    }

    /// R-WS-4: projects in one repository share a watcher, and each hears only its own changes.
    #[test]
    fn projects_in_one_repository_share_a_watcher() {
        let dir = std::env::temp_dir().join(format!("tortuga-watch-repo-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("a")).expect("mkdir");
        std::fs::create_dir_all(dir.join("b")).expect("mkdir");
        if gix::init(&dir).is_err() {
            eprintln!("could not create a repository; skipping");
            return;
        }
        let (a_count, a_change) = counter();
        let (b_count, b_change) = counter();
        let watchers = Watchers::default();
        watchers.watch(&dir.join("a"), a_change).expect("watch a");
        watchers.watch(&dir.join("b"), b_change).expect("watch b");
        assert_eq!(watchers.count(), 1, "one watcher for the repository");
        // Events from creating the repository may still arrive; start counting after them.
        std::thread::sleep(DEBOUNCE * 3);
        a_count.store(0, Ordering::SeqCst);
        b_count.store(0, Ordering::SeqCst);

        std::fs::write(dir.join("a/r.toml"), "url = \"/\"\n").expect("write");
        settle(&a_count);
        assert_eq!(a_count.load(Ordering::SeqCst), 1);
        assert_eq!(b_count.load(Ordering::SeqCst), 0, "b did not change");

        // A commit or checkout: every project in the repository refreshes.
        std::fs::write(dir.join(".git/HEAD"), "ref: refs/heads/other\n").expect("write");
        settle(&b_count);
        assert_eq!(b_count.load(Ordering::SeqCst), 1);
        assert_eq!(a_count.load(Ordering::SeqCst), 2);

        watchers.unwatch(&dir.join("a"));
        assert_eq!(watchers.count(), 1, "b still watched");
        watchers.unwatch(&dir.join("b"));
        assert_eq!(watchers.count(), 0);
        std::fs::remove_dir_all(dir).expect("clean up");
    }

    #[test]
    fn only_windows_drives_under_wsl_are_polled() {
        assert!(!needs_polling(Path::new("/home/me/api")));
    }
}
