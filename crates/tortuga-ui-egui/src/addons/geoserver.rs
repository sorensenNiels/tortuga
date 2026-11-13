//! The GeoServer add-on (R-ADD-6, D80): "New WFS service…" in project and folder menus,
//! which creates ordinary requests from a service's capabilities (R-GEO-1), and the
//! Features tab of GeoJSON responses (R-GEO-2). Its engine is `tortuga_core::geoserver`.

mod dialog;
pub(crate) mod features_tab;
pub(crate) mod map_view;
pub(crate) mod tiles;

use std::path::{Path, PathBuf};

use futures_util::future::abortable;
use tortuga_core::Item;
use tortuga_core::geoserver::{self, Capabilities};
use tortuga_core::runner::{Extra, run_step};
use tortuga_core::vars::Context as VarContext;

use super::DialogShown;
use crate::components::{DialogResult, VarHelp, undefined_names};
use crate::shell::Inbox;
use crate::state::{AppState, ItemKey};
use dialog::{Fetch, WfsAction, WfsDialog, WfsView};

/// What a menu item asks of the add-on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Command {
    /// Open "New WFS service…" for a folder of a project (empty: the project itself).
    NewService { root: PathBuf, parent: PathBuf },
}

/// What a fetch of capabilities sent back: its id and the result.
type Fetched = (u64, Result<Capabilities, String>);

/// The add-on's state while it is on.
#[derive(Debug)]
pub struct GeoServer {
    dialog: Option<WfsDialog>,
    fetched: Inbox<Fetched>,
    next_id: u64,
    /// The Features tab's map tiles (R-GEO-3); dropped with the add-on.
    pub(crate) tiles: tiles::SharedTiles,
}

impl Default for GeoServer {
    fn default() -> Self {
        Self {
            dialog: None,
            fetched: Inbox::new(),
            next_id: 0,
            tiles: tiles::SharedTiles::default(),
        }
    }
}

impl GeoServer {
    /// Takes in a fetch that finished, if it is the dialog's latest.
    pub(crate) fn take_fetched(&mut self) {
        while let Some((id, result)) = self.fetched.try_recv() {
            let Some(d) = &mut self.dialog else {
                continue;
            };
            if !matches!(d.fetch, Fetch::Fetching { id: latest, .. } if latest == id) {
                continue;
            }
            d.fetch = match result {
                Ok(caps) => Fetch::Fetched {
                    chosen: vec![false; caps.feature_types.len()],
                    caps,
                    what: d.current(),
                },
                Err(message) => Fetch::Failed(message),
            };
        }
    }
}

/// Does what was asked. Returns whether to show the dialog.
pub(crate) fn run(state: &mut AppState, command: Command) -> bool {
    let Command::NewService { root, parent } = command;
    let Some(c) = state.collection(&root) else {
        return false;
    };
    let place = if parent.as_os_str().is_empty() {
        c.name().to_owned()
    } else {
        c.folders_of(&parent.join("_"))
            .last()
            .map_or_else(|| parent.display().to_string(), |f| f.name.clone())
    };
    let servers = server_names(c);
    let Some(g) = &mut state.addons.geoserver else {
        return false;
    };
    g.dialog = Some(WfsDialog::new(root, parent, &place, &servers));
    true
}

/// Shows the dialog, and does what it asks.
pub(crate) fn dialog(ctx: &egui::Context, state: &mut AppState) -> DialogShown {
    let Some(mut d) = state
        .addons
        .geoserver
        .as_mut()
        .and_then(|g| g.dialog.take())
    else {
        return DialogShown::Closed;
    };
    let Some(c) = state.collection(&d.root) else {
        return DialogShown::Closed;
    };
    let servers = server_names(c);
    let environment = state.chosen_environment_name();
    let own_environment = environment
        .as_deref()
        .is_some_and(|name| c.environments.iter().any(|e| e.is_named(name)));
    let effective = state.effective_environment(&d.root);
    let server_url =
        geoserver::server_url(c, d.server_name(), effective.as_ref()).map(str::to_owned);
    let (ctx_vars, help) = auth_help(state, &d);
    let inherited_undefined = undefined_names(&ctx_vars, &[], ctx_vars.auth.as_ref());
    let view = WfsView {
        servers: &servers,
        environment: environment.as_deref(),
        own_environment,
        server_url: server_url.as_deref(),
        inherited: ctx_vars.auth.as_ref().map(|a| (a, &ctx_vars.auth_origin)),
        inherited_undefined: &inherited_undefined,
        help: &help,
    };
    let (result, action) = d.show(ctx, view);
    match action {
        Some(WfsAction::Fetch) => fetch(state, &mut d, ctx),
        Some(WfsAction::Create) => {
            if let Some(key) = create(state, &d, environment.as_deref()) {
                stop_fetch(&d);
                return DialogShown::OpenRequest(key);
            }
        }
        None => {}
    }
    if result != DialogResult::Open {
        stop_fetch(&d);
        return DialogShown::Closed;
    }
    if let Some(g) = &mut state.addons.geoserver {
        g.dialog = Some(d);
    }
    DialogShown::Open
}

/// The names of the project's servers, in its order.
fn server_names(c: &tortuga_core::Collection) -> Vec<String> {
    c.manifest.servers.iter().map(|s| s.name.clone()).collect()
}

/// The variables a request in the service folder gets, and what the auth fields can
/// tell about the variables they use (R-VAR-5).
fn auth_help(state: &AppState, d: &WfsDialog) -> (VarContext, VarHelp) {
    let key = service_key(d);
    let ctx = state.request_context(Some(&key));
    let mut help = state.var_help(Some(&key), &fetch_request(d), &ctx);
    help.undefined = undefined_names(&ctx, &[], d.auth.as_ref());
    (ctx, help)
}

/// GetCapabilities on the endpoint as typed, with the auth chosen in the dialog.
fn fetch_request(d: &WfsDialog) -> tortuga_core::Request {
    let mut request = geoserver::capabilities_request(&d.endpoint, d.version);
    request.auth.clone_from(&d.auth);
    request
}

/// Where the service's GetCapabilities will be: its context is the one the fetch uses.
fn service_key(d: &WfsDialog) -> ItemKey {
    ItemKey::new(&d.root, d.parent.join("GetCapabilities.toml"))
}

fn stop_fetch(d: &WfsDialog) {
    if let Fetch::Fetching { abort, .. } = &d.fetch {
        abort.abort();
    }
}

/// Sends GetCapabilities to the endpoint as typed, with the dialog's auth, and the
/// headers and variables a request in the dialog's folder would have.
fn fetch(state: &mut AppState, d: &mut WfsDialog, ctx: &egui::Context) {
    stop_fetch(d);
    let request = fetch_request(d);
    let vars = state.request_context(Some(&service_key(d)));
    let client = state.client.clone();
    let Some(g) = &mut state.addons.geoserver else {
        return;
    };
    g.next_id += 1;
    let id = g.next_id;
    let tx = g.fetched.sender();
    let repaint = ctx.clone();
    let (task, abort) = abortable(async move {
        let step = run_step(&client, None, &request, &vars, Extra::default()).await;
        let result = match step.response {
            Some(Ok(r)) => {
                match geoserver::capabilities::parse(&String::from_utf8_lossy(&r.body)) {
                    Ok(caps) => Ok(caps),
                    Err(_) if !(200..300).contains(&r.status) => {
                        Err(format!("{} {}", r.status, r.status_text).trim().to_owned())
                    }
                    Err(e) => Err(tortuga_core::error::message(&e)),
                }
            }
            Some(Err(e)) => Err(e),
            None => Err(step.errors.first().cloned().unwrap_or_default()),
        };
        // The receiver only goes away with the app.
        let _ = tx.send((id, result));
        repaint.request_repaint();
    });
    state.host.spawn(Box::pin(async move {
        // `Err` means the dialog was closed or fetched again.
        let _ = task.await;
    }));
    d.fetch = Fetch::Fetching { id, abort };
}

/// Writes the server, with its URL in the environment chosen for the workspace
/// (`environment`, a name), and the requests (R-GEO-1). Returns the request to open: the
/// first layer's GetFeature, else the service's GetCapabilities.
fn create(state: &mut AppState, d: &WfsDialog, environment: Option<&str>) -> Option<ItemKey> {
    let (caps, chosen) = d.chosen()?;
    let endpoint = d.split()?;
    let picked: Vec<_> = chosen.iter().map(|&i| &caps.feature_types[i]).collect();
    let (title, layers) = (caps.title.clone(), picked.len());
    let server = d.server_name().to_owned();
    let version = d.version;
    let folder_item = |id: &str| {
        geoserver::service_folder(caps, &picked, version, id, &endpoint.path, d.auth.as_ref())
    };
    let folder = state.run(&d.root, |c, s| {
        let id = geoserver::use_server(c, s, &server, &endpoint.base, environment)?;
        let paths = c.add_items(s, &d.parent, &[folder_item(&id)])?;
        Ok(paths.into_iter().next())
    })??;
    state.inform(t!("wfs-created", title = title.as_str(), count = layers));
    let c = state.collection(&d.root)?;
    first_request(c.children(&folder), &d.root)
}

/// The first layer's GetFeature in the service's folder, else its first request.
fn first_request(items: &[Item], root: &Path) -> Option<ItemKey> {
    let get_feature = items.iter().find_map(|i| match i {
        Item::Folder(f) => f.items.iter().find_map(|r| match r {
            Item::Request(r) if r.request.name == "GetFeature" => Some(r.path.clone()),
            _ => None,
        }),
        Item::Request(_) | Item::Broken(_) => None,
    });
    let first = || {
        items.iter().find_map(|i| match i {
            Item::Request(r) => Some(r.path.clone()),
            Item::Folder(_) | Item::Broken(_) => None,
        })
    };
    get_feature
        .or_else(first)
        .map(|path| ItemKey::new(root, path))
}

#[cfg(test)]
mod tests;
