//! Collections on disk (R-COL-*, R-GIT-*), against a real temporary directory.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use tortuga_core::collection::{ENVIRONMENTS_DIR, MANIFEST_FILE};
use tortuga_core::storage::FsStorage;
use tortuga_core::{Collection, Direction, Environment, Error, HttpMethod, Item, Request, Server};

const FS: FsStorage = FsStorage;

fn new_collection() -> (tempfile::TempDir, Collection) {
    let dir = tempfile::tempdir().expect("temp dir");
    let col = Collection::create(&FS, dir.path(), "Acme API").expect("create");
    (dir, col)
}

fn request(name: &str) -> Request {
    let mut r = Request::new(HttpMethod::Get, "https://a.test/x");
    r.name = name.into();
    r
}

fn reopen(col: &Collection) -> Collection {
    Collection::open(&FS, &col.root).expect("reopen")
}

fn names(items: &[Item]) -> Vec<String> {
    items.iter().map(Item::name).collect()
}

fn modified(path: &Path) -> SystemTime {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .expect("mtime")
}

/// R-GIT-1, R-COL-1
#[test]
fn create_writes_a_manifest_and_open_requires_one() {
    let (dir, col) = new_collection();
    assert_eq!(col.name(), "Acme API");
    assert_eq!(
        std::fs::read_to_string(dir.path().join(MANIFEST_FILE)).expect("manifest"),
        "format = 1\nname = \"Acme API\"\n"
    );
    assert!(matches!(
        Collection::create(&FS, dir.path(), "Again"),
        Err(Error::AlreadyACollection(_))
    ));

    let empty = tempfile::tempdir().expect("temp dir");
    assert!(matches!(
        Collection::open(&FS, empty.path()),
        Err(Error::NotACollection(_))
    ));
}

/// R-GIT-2, R-GIT-5, R-COL-1
#[test]
fn requests_and_folders_become_files_and_directories() {
    let (dir, col) = new_collection();
    let folder = col
        .new_folder(&FS, Path::new(""), "Users & Roles")
        .expect("folder");
    assert_eq!(folder, PathBuf::from("users-roles"));
    let col = reopen(&col);
    let req = col
        .new_request(&FS, &folder, &request("List users"))
        .expect("request");
    assert_eq!(req, PathBuf::from("users-roles/list-users.toml"));
    assert!(dir.path().join("users-roles/_folder.toml").exists());

    let col = reopen(&col);
    let Some(Item::Folder(f)) = col.find(&folder) else {
        panic!("folder missing")
    };
    assert_eq!(f.name, "Users & Roles");
    assert_eq!(names(&f.items), ["List users"]);
}

/// R-GIT-5: name clashes get a number instead of overwriting.
#[test]
fn clashing_names_get_unique_files() {
    let (_dir, col) = new_collection();
    let a = col
        .new_request(&FS, Path::new(""), &request("Ping"))
        .expect("a");
    let b = reopen(&col)
        .new_request(&FS, Path::new(""), &request("Ping"))
        .expect("b");
    assert_eq!(
        (a, b),
        (PathBuf::from("ping.toml"), PathBuf::from("ping-2.toml"))
    );
    // The reserved environments directory is never used for a folder.
    let f = reopen(&col)
        .new_folder(&FS, Path::new(""), "Environments")
        .expect("folder");
    assert_eq!(f, PathBuf::from("environments-2"));
}

/// R-GIT-3: saving unchanged content does not touch the file; R-GIT-5: renaming renames the file.
#[test]
fn save_is_deterministic_and_renames_files() {
    let (dir, col) = new_collection();
    let path = col
        .new_request(&FS, Path::new(""), &request("Get user"))
        .expect("new");
    let col = reopen(&col);
    let file = dir.path().join(&path);
    let before = (
        std::fs::read_to_string(&file).expect("read"),
        modified(&file),
    );

    std::thread::sleep(std::time::Duration::from_millis(20));
    let same = col.request(&path).expect("loaded").request.clone();
    assert_eq!(col.save_request(&FS, &path, &same).expect("save"), path);
    assert_eq!(
        (
            std::fs::read_to_string(&file).expect("read"),
            modified(&file)
        ),
        before
    );

    let renamed = Request {
        name: "Fetch user".into(),
        ..same
    };
    let new_path = col.save_request(&FS, &path, &renamed).expect("rename");
    assert_eq!(new_path, PathBuf::from("fetch-user.toml"));
    assert!(!file.exists());
    let col = reopen(&col);
    assert_eq!(
        col.request(&new_path).expect("renamed").seq,
        Some(100),
        "position kept"
    );
}

/// R-COL-3, R-GIT-6: moving up/down swaps two positions and touches only those files.
#[test]
fn reordering_changes_only_the_two_items() {
    let (dir, col) = new_collection();
    for name in ["A", "B", "C"] {
        reopen(&col)
            .new_request(&FS, Path::new(""), &request(name))
            .expect("new");
    }
    let col = reopen(&col);
    assert_eq!(names(&col.items), ["A", "B", "C"]);
    let untouched = modified(&dir.path().join("a.toml"));
    std::thread::sleep(std::time::Duration::from_millis(20));

    col.move_by(&FS, Path::new("c.toml"), Direction::Up)
        .expect("move");
    let col = reopen(&col);
    assert_eq!(names(&col.items), ["A", "C", "B"]);
    assert_eq!(modified(&dir.path().join("a.toml")), untouched);

    col.move_by(&FS, Path::new("a.toml"), Direction::Up)
        .expect("no-op at top");
    assert_eq!(names(&reopen(&col).items), ["A", "C", "B"]);
}

/// R-COL-3: hand-made files without positions are numbered on first reorder.
#[test]
fn reordering_items_without_positions() {
    let (dir, col) = new_collection();
    std::fs::write(dir.path().join("b.toml"), "name = \"B\"\n").expect("write");
    std::fs::write(dir.path().join("a.toml"), "name = \"A\"\n").expect("write");
    let col = reopen(&col);
    assert_eq!(
        names(&col.items),
        ["A", "B"],
        "unpositioned items sort by name"
    );
    col.move_by(&FS, Path::new("b.toml"), Direction::Up)
        .expect("move");
    assert_eq!(names(&reopen(&col).items), ["B", "A"]);
}

/// R-COL-2
#[test]
fn duplicate_move_and_delete() {
    let (_dir, col) = new_collection();
    let folder = col.new_folder(&FS, Path::new(""), "Users").expect("folder");
    let col = reopen(&col);
    let req = col
        .new_request(&FS, &folder, &request("List"))
        .expect("request");
    let col = reopen(&col);
    let other = col
        .new_request(&FS, Path::new(""), &request("Other"))
        .expect("request");

    let col = reopen(&col);
    let copy = col.duplicate(&FS, &req).expect("dup request");
    assert_eq!(copy, PathBuf::from("users/list-copy.toml"));
    let col = reopen(&col);
    let folder_copy = col.duplicate(&FS, &folder).expect("dup folder");
    let col = reopen(&col);
    assert_eq!(
        names(&col.items),
        ["Users", "Users copy", "Other"],
        "copy placed right after original"
    );
    assert_eq!(names(col.children(&folder_copy)), ["List", "List copy"]);

    let moved = col.move_to(&FS, &other, &folder).expect("move");
    assert_eq!(moved, PathBuf::from("users/other.toml"));
    let col = reopen(&col);
    assert_eq!(
        names(col.children(&folder)),
        ["List", "List copy", "Other"],
        "moved item goes last"
    );
    assert!(matches!(
        col.move_to(&FS, &folder, &folder),
        Err(Error::InvalidOperation(_))
    ));

    col.delete(&FS, &folder_copy).expect("delete folder");
    col.delete(&FS, &moved).expect("delete request");
    let col = reopen(&col);
    assert_eq!(names(&col.items), ["Users"]);
    assert_eq!(names(col.children(&folder)), ["List", "List copy"]);
}

/// R-GIT-8: a broken file is reported on its own; everything else loads.
#[test]
fn broken_files_do_not_break_the_collection() {
    let (dir, col) = new_collection();
    col.new_request(&FS, Path::new(""), &request("Good"))
        .expect("new");
    std::fs::write(
        dir.path().join("conflict.toml"),
        "<<<<<<< HEAD\nname = \"x\"\n=======\n",
    )
    .expect("write");
    std::fs::create_dir(dir.path().join("env-like")).expect("dir");
    std::fs::write(
        dir.path().join("env-like/_folder.toml"),
        "seq = \"not a number\"",
    )
    .expect("write");
    std::fs::write(dir.path().join("notes.md"), "ignored").expect("write");

    let col = reopen(&col);
    assert_eq!(col.items.len(), 3, "{:#?}", col.items);
    let broken = col.items.iter().find_map(|i| {
        if let Item::Broken(b) = i {
            Some(b)
        } else {
            None
        }
    });
    assert!(broken.expect("broken item").error.starts_with("line 1:"));
    let Some(Item::Folder(f)) = col.find(Path::new("env-like")) else {
        panic!("folder")
    };
    assert!(
        f.error
            .as_deref()
            .is_some_and(|e| e.contains("_folder.toml")),
        "{:?}",
        f.error
    );
}

/// R-GIT-7: a collection from a newer Tortuga opens read-only.
#[test]
fn newer_format_is_read_only() {
    let (dir, col) = new_collection();
    std::fs::write(
        dir.path().join(MANIFEST_FILE),
        "format = 99\nname = \"Future\"\nnew_feature = true\n",
    )
    .expect("write");
    let col = reopen(&col);
    assert!(col.read_only);
    let err = col
        .new_request(&FS, Path::new(""), &request("x"))
        .expect_err("read-only");
    assert!(
        matches!(
            err,
            Error::ReadOnly {
                found: 99,
                supported: 1
            }
        ),
        "{err}"
    );
}

/// R-VAR-2, R-SRV-1/2: servers, variables and environments are saved and loaded.
#[test]
fn environments_and_servers_round_trip() {
    let (dir, col) = new_collection();
    let mut manifest = col.manifest.clone();
    manifest.servers.push(Server {
        id: "users".into(),
        name: "Users API".into(),
    });
    manifest.variables.insert("version".into(), "v1".into());
    let dev = Environment {
        id: Collection::new_environment_id("Development", &[]),
        name: "Development".into(),
        seq: Some(1),
        servers: [("users".to_owned(), "http://localhost:8080".to_owned())].into(),
        variables: [("version".to_owned(), "v2".to_owned())].into(),
    };
    col.save_config(&FS, &manifest, std::slice::from_ref(&dev))
        .expect("save");

    let col = reopen(&col);
    assert_eq!(col.manifest, manifest);
    assert_eq!(col.environments, std::slice::from_ref(&dev));
    assert!(
        dir.path()
            .join(ENVIRONMENTS_DIR)
            .join("development.toml")
            .exists()
    );
    assert!(
        col.items.is_empty(),
        "environments/ is not shown as a folder"
    );

    let ctx = tortuga_core::vars::Context::for_collection(&col, Some("development"));
    assert_eq!(
        ctx.variables["version"], "v2",
        "environment overrides collection default"
    );

    col.save_config(&FS, &manifest, &[]).expect("remove env");
    assert_eq!(reopen(&col).environments, Vec::<Environment>::new());
}
