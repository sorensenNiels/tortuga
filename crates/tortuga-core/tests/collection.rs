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
                supported: tortuga_core::format::FORMAT_VERSION
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
        // R-SRV-8: the colour is kept in the environment file.
        color: Some(tortuga_core::EnvironmentColor::Teal),
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

/// A project with defaults in `tortuga.toml`, folders `admin` and `admin/users` with
/// their own defaults, a request in `admin/users`, and a "Dev" environment.
fn project_with_folder_defaults() -> (tempfile::TempDir, Collection, PathBuf, PathBuf) {
    use tortuga_core::format::FolderDefaults;
    use tortuga_core::{Auth, KeyValue};

    let (dir, col) = new_collection();
    let mut manifest = col.manifest.clone();
    manifest.headers = vec![
        KeyValue::new("X-Team", "core"),
        KeyValue::new("Accept", "text/plain"),
    ];
    manifest.auth = Some(Auth::Bearer {
        token: "project".into(),
    });
    manifest.variables.insert("tenant".into(), "project".into());
    manifest.variables.insert("version".into(), "v1".into());
    let env = Environment {
        id: "dev".into(),
        name: "Dev".into(),
        variables: [("version".to_owned(), "v9".to_owned())].into(),
        ..Environment::default()
    };
    col.save_config(&FS, &manifest, std::slice::from_ref(&env))
        .expect("config");
    let outer = reopen(&col)
        .new_folder(&FS, Path::new(""), "Admin")
        .expect("outer");
    let inner = reopen(&col)
        .new_folder(&FS, &outer, "Users")
        .expect("inner");
    reopen(&col)
        .save_folder_defaults(
            &FS,
            &outer,
            &FolderDefaults {
                headers: vec![KeyValue::new("Accept", "application/json")],
                auth: Some(Auth::Basic {
                    username: "admin".into(),
                    password: "{{tenant}}".into(),
                }),
                variables: [("tenant".to_owned(), "admin".to_owned())].into(),
            },
        )
        .expect("outer defaults");
    reopen(&col)
        .save_folder_defaults(
            &FS,
            &inner,
            &FolderDefaults {
                headers: vec![KeyValue::new("X-Area", "users")],
                variables: [
                    ("tenant".to_owned(), "users".to_owned()),
                    ("version".to_owned(), "v2".to_owned()),
                ]
                .into(),
                ..FolderDefaults::default()
            },
        )
        .expect("inner defaults");
    let req = reopen(&col)
        .new_request(&FS, &inner, &request("List"))
        .expect("request");
    let col = reopen(&col);
    (dir, col, outer, req)
}

/// R-COL-11: requests inherit from their folders, nearest first, then the project.
#[test]
fn folder_defaults_are_inherited_nearest_first() {
    use tortuga_core::vars::{Context, prepare};

    let (dir, col, outer, req) = project_with_folder_defaults();
    let text = std::fs::read_to_string(dir.path().join(&outer).join("_folder.toml")).expect("file");
    assert!(
        text.contains("[[headers]]") && text.contains("[auth]"),
        "{text}"
    );

    let ctx = Context::for_request(&col, Some("dev"), Some(&req));
    assert_eq!(ctx.variables["tenant"], "users", "nearest folder wins");
    assert_eq!(
        ctx.variables["version"], "v9",
        "the environment wins over folders"
    );
    let origins: Vec<(&str, &str)> = ctx
        .headers
        .iter()
        .zip(&ctx.header_origins)
        .map(|(h, o)| (h.name.as_str(), o.as_str()))
        .collect();
    assert_eq!(
        origins,
        [
            ("X-Team", "the project"),
            ("Accept", "folder “Admin”"),
            ("X-Area", "folder “Users”"),
        ]
    );
    assert_eq!(ctx.auth_origin, "folder “Admin”");
    let sent = prepare(&col.request(&req).expect("saved").request, &ctx).expect("prepare");
    assert!(
        sent.headers
            .iter()
            .any(|h| h.name == "Authorization" && h.value.starts_with("Basic ")),
        "{:?}",
        sent.headers
    );
    let project_level = tortuga_core::vars::Context::for_collection(&col, Some("dev"));
    assert_eq!(project_level.variables["tenant"], "project");
}

/// R-COL-11, R-HTTP-5: "no auth" on a folder keeps the auth of the levels above out,
/// and is saved as `kind = "none"`; an absent `[auth]` still inherits.
#[test]
fn a_folder_can_say_no_auth() {
    use tortuga_core::format::FolderDefaults;
    use tortuga_core::vars::{Context, prepare};

    let (dir, col, outer, req) = project_with_folder_defaults();
    let inner = req.parent().expect("in a folder").to_path_buf();
    let mut defaults = col.folders_of(&req).last().expect("inner").defaults.clone();
    defaults.auth = Some(tortuga_core::Auth::None);
    col.save_folder_defaults(&FS, &inner, &defaults)
        .expect("saved");
    let text = std::fs::read_to_string(dir.path().join(&inner).join("_folder.toml")).expect("file");
    assert!(text.contains("[auth]\nkind = \"none\""), "{text}");

    let col = reopen(&col);
    let ctx = Context::for_request(&col, Some("dev"), Some(&req));
    assert_eq!(ctx.auth, Some(tortuga_core::Auth::None));
    assert_eq!(ctx.auth_origin, "folder “Users”");
    let sent = prepare(&col.request(&req).expect("saved").request, &ctx).expect("prepare");
    assert!(!sent.has_header("authorization"), "{:?}", sent.headers);

    // The outer folder, with no `[auth]` of its own now, inherits the project's.
    col.save_folder_defaults(&FS, &outer, &FolderDefaults::default())
        .expect("saved");
    let col = reopen(&col);
    let outer_ctx = Context::for_request(&col, Some("dev"), Some(&outer.join("x.toml")));
    assert_eq!(outer_ctx.auth_origin, "the project");
}

/// R-COL-11: renaming, reordering and duplicating a folder keep its defaults.
#[test]
fn folder_defaults_survive_folder_operations() {
    let (_dir, col, outer, _req) = project_with_folder_defaults();
    let defaults = |col: &Collection, path: &Path| match col.find(path) {
        Some(Item::Folder(f)) => f.defaults.clone(),
        other => panic!("not a folder: {other:?}"),
    };
    let original = defaults(&col, &outer);
    let renamed = col
        .rename_folder(&FS, &outer, "Administration")
        .expect("rename");
    let col = reopen(&col);
    assert_eq!(defaults(&col, &renamed), original);
    col.new_request(&FS, Path::new(""), &request("Top"))
        .expect("top");
    let col = reopen(&col);
    col.move_by(&FS, &renamed, Direction::Down).expect("move");
    let col = reopen(&col);
    assert_eq!(defaults(&col, &renamed), original);
    let copy = col.duplicate(&FS, &renamed).expect("duplicate");
    let col = reopen(&col);
    assert_eq!(defaults(&col, &copy), original);
}

/// R-COL-11: `_folder.toml` files written before folder defaults still load.
#[test]
fn old_folder_files_load() {
    let (dir, col) = new_collection();
    std::fs::create_dir(dir.path().join("old")).expect("dir");
    std::fs::write(
        dir.path().join("old/_folder.toml"),
        "name = \"Old\"\nseq = 5\n",
    )
    .expect("file");
    let col = reopen(&col);
    let Some(Item::Folder(f)) = col.find(Path::new("old")) else {
        panic!("folder")
    };
    assert_eq!((f.name.as_str(), f.seq), ("Old", Some(5)));
    assert!(f.defaults.is_empty());
}

/// R-HTTP-15: a directory without requests or `_folder.toml` (e.g. files to upload) is
/// not a folder; one with requests is, even without `_folder.toml`.
#[test]
fn directories_without_requests_are_not_folders() {
    let (dir, col) = new_collection();
    std::fs::create_dir_all(dir.path().join("fixtures/images")).expect("dirs");
    std::fs::write(dir.path().join("fixtures/images/me.png"), b"png").expect("file");
    std::fs::create_dir(dir.path().join("handmade")).expect("dir");
    std::fs::write(dir.path().join("handmade/get.toml"), "url = \"/\"\n").expect("request");
    col.new_folder(&FS, Path::new(""), "Empty").expect("folder");
    let col = reopen(&col);
    assert_eq!(names(&col.items), ["Empty", "handmade"]);
}

/// R-COL-12: dropping an item puts it before another one, at the end, or into a
/// folder; it usually rewrites only the moved file.
#[test]
fn place_reorders_and_moves() {
    let (dir, col) = new_collection();
    for name in ["A", "B", "C"] {
        reopen(&col)
            .new_request(&FS, Path::new(""), &request(name))
            .expect("new");
    }
    let folder = reopen(&col)
        .new_folder(&FS, Path::new(""), "F")
        .expect("folder");
    reopen(&col)
        .new_request(&FS, &folder, &request("Inside"))
        .expect("inside");
    let col = reopen(&col);
    assert_eq!(names(&col.items), ["A", "B", "C", "F"]);

    let untouched = modified(&dir.path().join("b.toml"));
    std::thread::sleep(std::time::Duration::from_millis(20));
    col.place(
        &FS,
        Path::new("c.toml"),
        Path::new(""),
        Some(Path::new("a.toml")),
    )
    .expect("to the top");
    let col = reopen(&col);
    assert_eq!(names(&col.items), ["C", "A", "B", "F"]);
    assert_eq!(
        modified(&dir.path().join("b.toml")),
        untouched,
        "only C was written"
    );

    col.place(&FS, Path::new("c.toml"), Path::new(""), None)
        .expect("to the end");
    let col = reopen(&col);
    assert_eq!(names(&col.items), ["A", "B", "F", "C"]);

    let moved = col
        .place(
            &FS,
            Path::new("a.toml"),
            &folder,
            Some(&folder.join("inside.toml")),
        )
        .expect("into the folder");
    assert_eq!(moved, folder.join("a.toml"));
    let col = reopen(&col);
    assert_eq!(names(&col.items), ["B", "F", "C"]);
    assert_eq!(names(col.children(&folder)), ["A", "Inside"]);

    assert!(
        col.place(&FS, &folder, &folder, None).is_err(),
        "not into itself"
    );
    assert_eq!(
        col.place(
            &FS,
            Path::new("b.toml"),
            Path::new(""),
            Some(Path::new("b.toml"))
        )
        .expect("onto itself"),
        PathBuf::from("b.toml")
    );
}

/// R-COL-12, R-GIT-6: with no room between two positions, the folder is renumbered.
#[test]
fn place_renumbers_when_positions_touch() {
    let (dir, col) = new_collection();
    for (name, seq) in [("a", 1), ("b", 2), ("c", 3)] {
        std::fs::write(
            dir.path().join(format!("{name}.toml")),
            format!(
                "name = \"{}\"\nseq = {seq}\nurl = \"/\"\n",
                name.to_uppercase()
            ),
        )
        .expect("write");
    }
    let col = reopen(&col);
    col.place(
        &FS,
        Path::new("c.toml"),
        Path::new(""),
        Some(Path::new("b.toml")),
    )
    .expect("between a and b");
    assert_eq!(names(&reopen(&col).items), ["A", "C", "B"]);
}
