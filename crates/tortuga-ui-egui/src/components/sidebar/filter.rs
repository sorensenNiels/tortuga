//! The sidebar's filter box: which projects, folders, requests and workflows match (R-COL-46),
//! optionally by what their request bodies hold (R-COL-47).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use tortuga_core::{Collection, Item};

use super::SidebarState;
use crate::fuzzy;
use crate::state::{AppState, ItemKey, WorkflowKey};

/// The rows to show while the filter box has text. A row shows when its name matches, when a
/// project or folder above it matches (to see what it holds), or when something below it
/// matches (to see where that is). With `in_bodies`, a request also matches when its body
/// holds every word of the query.
#[derive(Debug, Default)]
pub(super) struct Filter {
    projects: HashSet<PathBuf>,
    items: HashSet<ItemKey>,
    workflows: HashSet<WorkflowKey>,
}

impl Filter {
    /// `None` when the query is blank: everything shows.
    pub(super) fn new<'a>(
        query: &str,
        in_bodies: bool,
        collections: impl IntoIterator<Item = &'a Collection>,
    ) -> Option<Self> {
        if query.trim().is_empty() {
            return None;
        }
        let mut filter = Self::default();
        let search = Search { query, in_bodies };
        for c in collections {
            let matched = matches(query, c.name());
            let below = filter.add_items(&search, &c.root, &c.items, matched);
            let mut flows = false;
            for w in &c.workflows {
                if matched || matches(query, &w.name) {
                    flows = true;
                    filter.workflows.insert(WorkflowKey {
                        root: c.root.clone(),
                        id: w.id.clone(),
                    });
                }
            }
            if matched || below || flows {
                filter.projects.insert(c.root.clone());
            }
        }
        Some(filter)
    }

    /// Adds the items to show; returns whether any shows.
    fn add_items(&mut self, search: &Search<'_>, root: &Path, items: &[Item], above: bool) -> bool {
        let mut any = false;
        for item in items {
            let matched = above || search.finds(item);
            let below = match item {
                Item::Folder(f) => self.add_items(search, root, &f.items, matched),
                Item::Request(_) | Item::Broken(_) => false,
            };
            if matched || below {
                self.items.insert(ItemKey::new(root, item.path()));
                any = true;
            }
        }
        any
    }

    pub(super) fn project(&self, root: &Path) -> bool {
        self.projects.contains(root)
    }

    pub(super) fn item(&self, key: &ItemKey) -> bool {
        self.items.contains(key)
    }

    pub(super) fn workflow(&self, key: &WorkflowKey) -> bool {
        self.workflows.contains(key)
    }

    pub(super) fn has_workflows(&self, root: &Path) -> bool {
        self.workflows.iter().any(|w| w.root == root)
    }

    pub(super) fn is_empty(&self) -> bool {
        self.projects.is_empty()
    }
}

/// A filter and what it was worked out from, so that looking through every request body is
/// done again only when the query, the bodies switch or the projects change (R-COL-47).
#[derive(Debug)]
pub(super) struct KeptFilter {
    query: String,
    in_bodies: bool,
    revision: u64,
    filter: Filter,
}

impl SidebarState {
    /// The filter for this frame: last frame's while nothing it depends on changed.
    pub(super) fn take_filter(&mut self, state: &AppState) -> Option<Filter> {
        match self.kept_filter.take() {
            Some(kept)
                if kept.query == self.filter
                    && kept.in_bodies == self.in_bodies
                    && kept.revision == state.revision() =>
            {
                Some(kept.filter)
            }
            _ => Filter::new(&self.filter, self.in_bodies, &state.collections),
        }
    }

    /// Keeps the frame's filter for the next one.
    pub(super) fn keep_filter(&mut self, filter: Option<Filter>, revision: u64) {
        self.kept_filter = filter.map(|filter| KeptFilter {
            query: self.filter.clone(),
            in_bodies: self.in_bodies,
            revision,
            filter,
        });
    }
}

/// What the filter box asks for.
struct Search<'a> {
    query: &'a str,
    in_bodies: bool,
}

impl Search<'_> {
    /// Whether the item's own name, or a request's body, matches.
    fn finds(&self, item: &Item) -> bool {
        matches(self.query, &item.name())
            || match item {
                Item::Request(r) if self.in_bodies => self
                    .query
                    .split_whitespace()
                    .all(|word| r.request.body.contains_text(word)),
                Item::Request(_) | Item::Folder(_) | Item::Broken(_) => false,
            }
    }
}

fn matches(query: &str, name: &str) -> bool {
    fuzzy::score(query, name).is_some()
}

#[cfg(test)]
mod tests {
    use tortuga_core::storage::FsStorage;
    use tortuga_core::{HttpMethod, Request, RequestBody};

    use super::*;

    const FS: FsStorage = FsStorage;

    fn request(name: &str) -> Request {
        let mut r = Request::new(HttpMethod::Get, "https://a.test");
        r.name = name.into();
        r
    }

    /// Shop: Users/{Get user, Ünïcode 😀}, Orders/{List orders}, Health.
    fn shop(dir: &Path) -> Collection {
        let c = Collection::create(&FS, dir, "Shop").expect("project");
        let users = c.new_folder(&FS, Path::new(""), "Users").expect("folder");
        let c = Collection::open(&FS, dir).expect("open");
        let orders = c.new_folder(&FS, Path::new(""), "Orders").expect("folder");
        for (folder, name) in [
            (&users, "Get user"),
            (&users, "Ünïcode 😀"),
            (&orders, "List orders"),
            (&PathBuf::new(), "Health"),
        ] {
            let mut r = request(name);
            if name == "List orders" {
                r.body = RequestBody::Json(r#"{"customerId": "Ärla-42"}"#.into());
            }
            Collection::open(&FS, dir)
                .expect("open")
                .new_request(&FS, folder, &r)
                .expect("request");
        }
        Collection::open(&FS, dir).expect("open")
    }

    fn shown(filter: &Filter, c: &Collection) -> Vec<String> {
        let mut names = Vec::new();
        collect(filter, &c.root, &c.items, &mut names);
        names
    }

    fn collect(filter: &Filter, root: &Path, items: &[Item], out: &mut Vec<String>) {
        for item in items {
            if filter.item(&ItemKey::new(root, item.path())) {
                out.push(item.name());
            }
            if let Item::Folder(f) = item {
                collect(filter, root, &f.items, out);
            }
        }
    }

    /// R-COL-46: a blank query shows everything.
    #[test]
    fn a_blank_query_filters_nothing() {
        let dir = tempfile::tempdir().expect("dir");
        let c = shop(dir.path());
        assert!(Filter::new("", false, [&c]).is_none());
        assert!(Filter::new("  ", true, [&c]).is_none());
    }

    /// R-COL-46: a matching request shows with the folders above it, and nothing else.
    #[test]
    fn a_request_shows_with_its_folders() {
        let dir = tempfile::tempdir().expect("dir");
        let c = shop(dir.path());
        let filter = Filter::new("gtusr", false, [&c]).expect("filter");
        assert!(filter.project(&c.root));
        assert_eq!(shown(&filter, &c), ["Users", "Get user"]);
        let filter = Filter::new("ünï 😀", false, [&c]).expect("filter");
        assert_eq!(shown(&filter, &c), ["Users", "Ünïcode 😀"]);
    }

    /// R-COL-46: a matching folder or project shows everything in it.
    #[test]
    fn a_matching_folder_or_project_shows_what_it_holds() {
        let dir = tempfile::tempdir().expect("dir");
        let c = shop(dir.path());
        let filter = Filter::new("orders", false, [&c]).expect("filter");
        assert_eq!(shown(&filter, &c), ["Orders", "List orders"]);
        let filter = Filter::new("shop", false, [&c]).expect("filter");
        assert_eq!(shown(&filter, &c).len(), 6);
    }

    /// R-COL-46: a project with no match is hidden.
    #[test]
    fn a_project_without_matches_is_hidden() {
        let dir = tempfile::tempdir().expect("dir");
        let c = shop(dir.path());
        let filter = Filter::new(&"x".repeat(500), false, [&c]).expect("filter");
        assert!(!filter.project(&c.root));
        assert!(filter.is_empty());
        assert_eq!(shown(&filter, &c), Vec::<String>::new());
    }

    /// R-COL-47: with bodies on, a request whose body holds every word matches, with its
    /// folders; with bodies off, the body is not looked at.
    #[test]
    fn body_search_finds_requests_by_what_they_send() {
        let dir = tempfile::tempdir().expect("dir");
        let c = shop(dir.path());
        let filter = Filter::new("CUSTOMERID ärla-42", true, [&c]).expect("filter");
        assert_eq!(shown(&filter, &c), ["Orders", "List orders"]);
        let filter = Filter::new("customerid nowhere", true, [&c]).expect("filter");
        assert!(filter.is_empty(), "every word must be in the body");
        let filter = Filter::new("customerid", false, [&c]).expect("filter");
        assert!(filter.is_empty(), "names only");
        let filter = Filter::new("gtusr", true, [&c]).expect("filter");
        assert_eq!(
            shown(&filter, &c),
            ["Users", "Get user"],
            "names still match"
        );
    }
}
