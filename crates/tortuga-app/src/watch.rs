//! Watches open projects for changes on disk (R-GIT-10), with one watcher per git
//! repository however many of its projects are open (R-WS-4).
//!
//! Events are debounced, so a `git pull` or branch switch that touches many files
//! results in one reload. Reading a file is not a change: otherwise every reload
//! (and every git status) would trigger the next one. The repository's `.git` folder
//! is watched too (not recursively), so a commit or checkout refreshes the git status
//! of every project in it even when no project file changed. Where the OS gives no
//! change events (Windows drives under WSL), the files are polled instead.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use notify::event::{AccessKind, AccessMode};
use notify::{EventKind, RecursiveMode, Watcher as _};

/// Quiet time before a burst of changes is reported.
const DEBOUNCE: Duration = Duration::from_millis(400);
/// Longest wait before reporting, for changes that never stop (e.g. a build writing files).
const MAX_DELAY: Duration = Duration::from_secs(2);
/// How often files are checked where there are no change events.
const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// Called (on a background thread) after something in a project changed.
pub type OnChange = Box<dyn Fn() + Send + Sync>;

/// The open projects of one watcher: (canonical root, callback).
type Projects = Arc<Mutex<Vec<(PathBuf, OnChange)>>>;

/// Dropping the watcher stops it, and with it the thread that reports its changes.
enum Handle {
    Native(notify::RecommendedWatcher),
    Polling(notify::PollWatcher),
}

impl Handle {
    fn watcher(&mut self) -> &mut dyn notify::Watcher {
        match self {
            Self::Native(w) => w,
            Self::Polling(w) => w,
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
            // Dropping the watcher stops it.
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
    let (tx, rx) = mpsc::channel::<PathBuf>();
    let handler = move |result: notify::Result<notify::Event>| {
        if let Ok(event) = result
            && is_change(event.kind)
        {
            for path in event.paths {
                // Fails only when the reporting thread is gone, i.e. while stopping.
                let _ = tx.send(path);
            }
        }
    };
    let mut handle = if needs_polling(root) {
        let config = notify::Config::default().with_poll_interval(POLL_INTERVAL);
        Handle::Polling(notify::PollWatcher::new(handler, config).map_err(|e| e.to_string())?)
    } else {
        Handle::Native(
            notify::RecommendedWatcher::new(handler, notify::Config::default())
                .map_err(|e| e.to_string())?,
        )
    };
    let for_reports = projects.clone();
    let git = git_dir.map(Path::to_path_buf);
    std::thread::Builder::new()
        .name("tortuga-watch".into())
        .spawn(move || report_bursts(&rx, &for_reports, git.as_deref()))
        .map_err(|e| e.to_string())?;
    if let Some(git_dir) = git_dir {
        // HEAD and index change on commit, checkout and staging. Not fatal if it fails.
        let _ = handle.watcher().watch(git_dir, RecursiveMode::NonRecursive);
    }
    Ok(Watch { handle, projects })
}

/// Opening, reading and closing a file unchanged are not changes. Tortuga reads the
/// files itself (reload, git status), so counting them would reload forever.
fn is_change(kind: EventKind) -> bool {
    match kind {
        EventKind::Access(AccessKind::Close(AccessMode::Write)) => true,
        EventKind::Access(_) => false,
        _ => true,
    }
}

/// Collects changed paths until it has been quiet for [`DEBOUNCE`] (or [`MAX_DELAY`]
/// passed), then tells each concerned project once. Ends when the watcher is dropped.
fn report_bursts(rx: &mpsc::Receiver<PathBuf>, projects: &Projects, git: Option<&Path>) {
    while let Ok(first) = rx.recv() {
        let started = Instant::now();
        let mut paths = vec![first];
        loop {
            let wait = DEBOUNCE.min(MAX_DELAY.saturating_sub(started.elapsed()));
            match rx.recv_timeout(wait) {
                Ok(path) => paths.push(path),
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
        for (root, on_change) in lock(projects).iter() {
            let concerned = paths
                .iter()
                .any(|p| p.starts_with(root) || git.is_some_and(|g| p.starts_with(g)));
            if concerned {
                on_change();
            }
        }
    }
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

    /// Waits until `count` is non-zero (or a timeout), then for the burst to settle.
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
        assert_eq!(count.load(Ordering::SeqCst), 1, "five writes, one report");
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

    /// R-GIT-10: reading files is not a change, or every reload would trigger the next one.
    #[test]
    fn reading_files_is_not_a_change() {
        let dir = std::env::temp_dir().join(format!("tortuga-watch-read-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("folder")).expect("mkdir");
        std::fs::write(dir.join("folder/r.toml"), "url = \"/\"\n").expect("write");
        let (count, on_change) = counter();
        let watchers = Watchers::default();
        watchers.watch(&dir, on_change).expect("watch");
        // What a reload does: list the folders and read every file.
        for entry in std::fs::read_dir(dir.join("folder")).expect("list") {
            std::fs::read_to_string(entry.expect("entry").path()).expect("read");
        }
        std::thread::sleep(DEBOUNCE * 3);
        assert_eq!(count.load(Ordering::SeqCst), 0, "reads are not reported");

        std::fs::write(dir.join("folder/r.toml"), "url = \"/new\"\n").expect("write");
        settle(&count);
        assert_eq!(count.load(Ordering::SeqCst), 1, "a write is");
        watchers.unwatch(&dir);
        std::fs::remove_dir_all(dir).expect("clean up");
    }

    #[test]
    fn only_windows_drives_under_wsl_are_polled() {
        assert!(!needs_polling(Path::new("/home/me/api")));
    }
}
