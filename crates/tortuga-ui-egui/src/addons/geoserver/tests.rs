use std::path::{Path, PathBuf};
use std::time::Duration;

use egui_kittest::kittest::{NodeT as _, Queryable};
use futures_util::future::AbortHandle;
use tortuga_core::geoserver::{Capabilities, FeatureType, Version};
use tortuga_core::settings::{FeaturesLayout, OPENSTREETMAP};
use tortuga_core::storage::FsStorage;
use tortuga_core::{Auth, Collection, KeyValue, Response};

use super::dialog::{Fetch, ServerPick, WfsDialog, WfsView};
use super::*;
use crate::addons::{AddOn, ResponseTabs, folder_menu, run as run_addon};
use crate::components::{
    CookiesTab, Filtered, Outcome, PanelContext, ResponseAction, ResponseState, SaveExample, Trend,
    response_panel,
};
use crate::test_host::TestHost;

const GEOJSON: &str = r#"{"type":"FeatureCollection","totalFeatures":49,
"crs":{"type":"name","properties":{"name":"urn:ogc:def:crs:EPSG::4326"}},
"features":[
 {"type":"Feature","id":"states.1","geometry":{"type":"Point","coordinates":[40.1,-83.0]},"properties":{"name":"Ohio ü😀"}},
 {"type":"Feature","id":"states.2","geometry":{"type":"Point","coordinates":[31.0,-99.9]},"properties":{"name":"Texas"}},
 {"type":"Feature","id":"states.3","geometry":null,"properties":{"name":"Nowhere"}}]}"#;

/// A project, and an app with the GeoServer add-on on or off that has it open.
fn app(on: bool) -> (tempfile::TempDir, PathBuf, AppState) {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join("maps-ü😀");
    Collection::create(&FsStorage, &root, "Maps").expect("create");
    let mut settings = tortuga_core::AppSettings::default();
    settings.addons.set_enabled(AddOn::GeoServer, on);
    let state = AppState::new(Box::new(TestHost {
        settings,
        open: vec![root.clone()],
        ..TestHost::default()
    }));
    (dir, root, state)
}

fn new_service(root: &Path) -> crate::addons::Command {
    crate::addons::Command::GeoServer(Command::NewService {
        root: root.to_owned(),
        parent: PathBuf::new(),
    })
}

fn caps() -> Capabilities {
    let layer = |name: &str| FeatureType {
        name: name.into(),
        title: String::new(),
        abstract_text: String::new(),
        default_crs: "EPSG:4326".into(),
        wgs84_bounds: None,
    };
    Capabilities {
        title: "Maps WFS".into(),
        version: Some(Version::V2_0_0),
        feature_types: vec![layer("topp:states"), layer("tiger:roads")],
    }
}

fn geojson_response() -> Response {
    Response {
        status: 200,
        status_text: "OK".into(),
        headers: vec![KeyValue::new("content-type", "application/json")],
        body: GEOJSON.as_bytes().to_vec(),
        elapsed: Duration::from_millis(5),
        timing: None,
    }
}

/// R-ADD-2, R-ADD-6: off by default, the add-on is not loaded: no menu entry, no
/// dialog, no Features tab.
#[test]
fn the_geoserver_addon_is_off_and_absent_by_default() {
    let (_dir, root, mut state) = app(false);
    assert!(state.addons.geoserver.is_none());
    assert!(folder_menu(state.menu_addons(), &root, Path::new("")).is_empty());
    assert_eq!(run_addon(&mut state, new_service(&root)), None);
    assert!(state.response_tabs().geoserver.is_none());
    assert_eq!(state.response_tabs().offered(GEOJSON.as_bytes()), []);
}

/// R-ADD-6: turned on, project and folder menus offer "New WFS service…", which opens
/// the dialog, and GeoJSON responses (only) get the Features tab.
#[test]
fn turned_on_it_offers_the_menu_entry_and_the_tab() {
    let (_dir, root, mut state) = app(true);
    let entries = folder_menu(state.menu_addons(), &root, Path::new("folder"));
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].label, "New WFS service…");
    assert_eq!(
        run_addon(&mut state, new_service(&root)),
        Some(AddOn::GeoServer)
    );
    let tabs = state.response_tabs();
    assert_eq!(tabs.offered(GEOJSON.as_bytes()).len(), 1);
    assert_eq!(tabs.offered(br#"{"type":"Point"}"#), []);
    assert_eq!(tabs.offered(b"<xml/>"), []);

    // Turned off, it is unloaded again.
    let mut settings = state.settings.clone();
    settings.addons.set_enabled(AddOn::GeoServer, false);
    state.apply_settings(&settings).expect("apply");
    assert!(state.addons.geoserver.is_none());
}

/// R-GEO-1: a fetch's result is taken only while the dialog waits for that fetch.
#[test]
fn only_the_latest_fetch_is_taken() {
    let (_dir, root, mut state) = app(true);
    run_addon(&mut state, new_service(&root));
    let g = state.addons.geoserver.as_mut().expect("on");
    let d = g.dialog.as_mut().expect("dialog");
    d.endpoint = "https://maps.test/geoserver/wfs".into();
    d.fetch = Fetch::Fetching {
        id: 2,
        abort: AbortHandle::new_pair().0,
    };
    let tx = g.fetched.sender();
    tx.send((1, Err("old".into()))).expect("send");
    g.take_fetched();
    assert!(matches!(
        g.dialog.as_ref().expect("dialog").fetch,
        Fetch::Fetching { .. }
    ));
    tx.send((2, Ok(caps()))).expect("send");
    g.take_fetched();
    let d = g.dialog.as_ref().expect("dialog");
    let Fetch::Fetched { chosen, what, .. } = &d.fetch else {
        panic!("fetched");
    };
    assert_eq!(chosen, &[false, false], "nothing chosen yet");
    assert_eq!(what, &d.current());
    assert_eq!(d.chosen().map(|(_, c)| c), Some(Vec::new()));
}

/// R-GEO-1: Create adds the server with its URL in a new "Default" environment, writes
/// the folders and requests, and opens the first layer's GetFeature.
#[test]
fn create_writes_the_server_and_the_requests() {
    let (_dir, root, mut state) = app(true);
    run_addon(&mut state, new_service(&root));
    let mut d = state
        .addons
        .geoserver
        .as_mut()
        .and_then(|g| g.dialog.take())
        .expect("dialog");
    d.endpoint = "https://maps.test/geoserver/topp/wfs".into();
    d.fetch = Fetch::Fetched {
        caps: caps(),
        chosen: vec![false, true],
        what: d.current(),
    };
    // A changed endpoint makes the list stale: nothing to create until fetched again.
    d.endpoint.push('s');
    assert!(d.chosen().is_none());
    d.endpoint.pop();

    let key = create(&mut state, &d, None).expect("created");
    let c = state.collection(&root).expect("project");
    assert_eq!(c.manifest.servers.len(), 1);
    let server = &c.manifest.servers[0];
    assert_eq!(server.name, "GeoServer");
    assert_eq!(c.environments.len(), 1);
    assert_eq!(c.environments[0].name, "Default");
    assert_eq!(
        c.environments[0]
            .servers
            .get(&server.id)
            .map(String::as_str),
        Some("https://maps.test/geoserver")
    );
    let opened = c.request(&key.path).expect("opened request");
    assert_eq!(opened.request.name, "GetFeature");
    assert_eq!(opened.request.server.as_deref(), Some(server.id.as_str()));
    assert_eq!(opened.request.url, "/topp/wfs");
    assert!(
        opened
            .request
            .params
            .iter()
            .any(|p| p.value == "tiger:roads")
    );
    assert_eq!(
        state.toast.as_ref().map(|t| t.text.as_str()),
        Some("Created requests for Maps WFS and 1 layer.")
    );
}

/// R-GEO-2: the Features tab counts the features, follows the CRS's axis order, lists
/// them, and asks to keep another layout.
#[test]
fn the_features_tab_shows_the_features() {
    let outcome = Some(Outcome::from_result(Ok(geojson_response())));
    let mut st = ResponseState::default();
    let mut filter = String::new();
    let mut asked = None;
    {
        let mut h = crate::test_host::harness(|ui| {
            let cx = PanelContext {
                sending: false,
                can_save: false,
                filter: &mut filter,
                filtered: Filtered::Off,
                tests: None,
                wrap: false,
                example: SaveExample::Hidden,
                trend: Trend::default(),
                changes: None,
                cookies: CookiesTab::Hidden,
                addon_tabs: ResponseTabs::with_geoserver(FeaturesLayout::Above),
            };
            if let Some(a) = response_panel(ui, outcome.as_ref(), &mut st, cx) {
                asked = Some(a);
            }
        });
        h.run();
        h.get_by_label("Features").click();
        h.run();
        h.get_by_label("3 of 49 features");
        h.get_by_label("urn:ogc:def:crs:EPSG::4326");
        assert_eq!(
            h.get_by_label("Swap x/y").accesskit_node().toggled(),
            Some(egui::accesskit::Toggled::True),
            "lat/lon for this CRS"
        );
        h.get_by_label("Ohio ü😀");
        h.get_by_label("Nowhere");
        h.get_by_label("Beside").click();
        h.run();
    }
    assert_eq!(
        asked,
        Some(ResponseAction::AddOn(
            crate::addons::ResponseTabAction::GeoServer(features_tab::TabAction::SetLayout(
                FeaturesLayout::Beside
            ))
        ))
    );
}

fn bearer(token: &str) -> Auth {
    Auth::Bearer {
        token: token.into(),
    }
}

/// R-GEO-1: with the project's auth using a variable no environment defines, the dialog
/// says so; the auth chosen in the dialog is what the fetch sends, and Create saves it
/// on the service folder, so the created requests send it too.
#[test]
fn the_dialogs_auth_is_fetched_with_and_saved_on_the_service_folder() {
    let (_dir, root, mut state) = app(true);
    state
        .run(&root, |c, s| {
            let mut manifest = c.manifest.clone();
            manifest.auth = Some(bearer("{{cAccessToken}}"));
            c.save_config(s, &manifest, &c.environments)
        })
        .expect("saved");
    run_addon(&mut state, new_service(&root));
    let mut d = state
        .addons
        .geoserver
        .as_mut()
        .and_then(|g| g.dialog.take())
        .expect("dialog");
    d.endpoint = "https://maps.test/geoserver/wfs".into();

    let (ctx, help) = auth_help(&state, &d);
    assert_eq!(ctx.auth, Some(bearer("{{cAccessToken}}")), "inherited");
    assert_eq!(
        crate::components::undefined_names(&ctx, &[], ctx.auth.as_ref()),
        ["cAccessToken"]
    );
    assert_eq!(fetch_request(&d).auth, None, "inherits");
    assert_eq!(help.undefined, Vec::<String>::new());

    d.auth = Some(bearer("{{wfs_token}}"));
    assert_eq!(fetch_request(&d).auth, Some(bearer("{{wfs_token}}")));
    let (_, help) = auth_help(&state, &d);
    assert_eq!(help.undefined, ["wfs_token"]);

    d.fetch = Fetch::Fetched {
        caps: caps(),
        chosen: vec![true, false],
        what: d.current(),
    };
    let key = create(&mut state, &d, None).expect("created");
    let c = state.collection(&root).expect("project");
    let service = c
        .folders_of(&key.path)
        .first()
        .map(|f| f.defaults.auth.clone());
    assert_eq!(service, Some(Some(bearer("{{wfs_token}}"))));
    assert_eq!(
        state.request_context(Some(&key)).auth,
        Some(bearer("{{wfs_token}}")),
        "GetFeature sends the service's auth"
    );
}

/// R-GEO-1: the Auth section names the inherited auth and the variables it lacks.
#[test]
fn the_dialog_warns_about_an_inherited_auth_with_undefined_variables() {
    let mut d = WfsDialog::new(PathBuf::from("p"), PathBuf::new(), "Maps", &[]);
    let inherited = bearer("{{cAccessToken}}");
    let origin = tortuga_core::vars::Origin::Project;
    let undefined = ["cAccessToken".to_owned()];
    let help = crate::components::VarHelp::default();
    let mut h = crate::test_host::harness(|ui| {
        let view = WfsView {
            servers: &[],
            environment: None,
            own_environment: false,
            server_url: None,
            inherited: Some((&inherited, &origin)),
            inherited_undefined: &undefined,
            help: &help,
        };
        d.show(ui.ctx(), view);
    });
    h.run();
    h.get_by_label("Inherit from project");
    assert!(
        h.query_by_label_contains("Bearer token {{cAccessToken}}")
            .is_some()
    );
    assert!(
        h.query_by_label_contains("Not defined in the active environment: {{cAccessToken}}")
            .is_some()
    );
}

/// Picks "New server…" or an existing server, and fills in the endpoint, a fetched list
/// and the first layer, ready to create.
fn ready_dialog(state: &mut AppState, root: &Path, endpoint: &str) -> WfsDialog {
    run_addon(state, new_service(root));
    let mut d = state
        .addons
        .geoserver
        .as_mut()
        .and_then(|g| g.dialog.take())
        .expect("dialog");
    d.endpoint = endpoint.into();
    d.fetch = Fetch::Fetched {
        caps: caps(),
        chosen: vec![true, false],
        what: d.current(),
    };
    d
}

/// R-GEO-1: the environment is chosen for the workspace, and the project has none of
/// that name: Create gives the project its own, with just the server's URL, instead of
/// a "Default" one; the requests then go to the endpoint in that environment.
#[test]
fn a_workspace_environment_gets_the_url_in_the_projects_own_environment() {
    let (_dir, root, mut state) = app(true);
    let staging = tortuga_core::workspace::WorkspaceEnvironment {
        id: "staging".into(),
        name: "Staging".into(),
        variables: [("region".to_owned(), "rk".to_owned())].into(),
        ..tortuga_core::workspace::WorkspaceEnvironment::default()
    };
    state
        .save_workspace_environments(vec![staging], &std::collections::BTreeMap::new())
        .expect("saved");
    state.set_workspace_environment("Staging");
    let d = ready_dialog(&mut state, &root, "https://kort.test/wfs/rk/ows");
    assert_eq!(d.server, ServerPick::New, "the project has no servers");
    assert_eq!(d.server_name(), "GeoServer");

    let chosen = state.chosen_environment_name();
    let key = create(&mut state, &d, chosen.as_deref()).expect("created");
    let c = state.collection(&root).expect("project");
    let names: Vec<&str> = c.environments.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["Staging"], "no Default");
    let server = &c.manifest.servers[0];
    let env = state.effective_environment(&root).expect("environment");
    assert_eq!(
        env.servers.get(&server.id).map(String::as_str),
        Some("https://kort.test")
    );
    assert_eq!(
        env.variables.get("region").map(String::as_str),
        Some("rk"),
        "still the workspace's"
    );
    assert_eq!(
        geoserver::server_url(c, "GeoServer", Some(&env)),
        Some("https://kort.test")
    );
    assert_eq!(
        c.request(&key.path).expect("request").request.url,
        "/wfs/rk/ows",
        "D81: the server is the host"
    );

    // Next time, the server is picked from the list; the split can be moved (D81).
    let mut d = ready_dialog(&mut state, &root, "https://kort.test/wfs/rk/ows");
    assert_eq!(d.server, ServerPick::Existing("GeoServer".into()));
    d.base = Some("https://kort.test/wfs".into());
    let key = create(&mut state, &d, chosen.as_deref()).expect("created");
    let c = state.collection(&root).expect("project");
    assert_eq!(
        c.request(&key.path).expect("request").request.url,
        "/rk/ows"
    );
    let env = state.effective_environment(&root).expect("environment");
    assert_eq!(
        env.servers
            .get(&c.manifest.servers[0].id)
            .map(String::as_str),
        Some("https://kort.test/wfs")
    );
    // A split the endpoint no longer has falls back to the default.
    d.endpoint = "https://other.test/ows".into();
    assert_eq!(d.split().map(|e| e.base), Some("https://other.test".into()));
}

/// R-GEO-1: the server line says what Create does with the URL; a new server needs a
/// name that the project's servers don't have.
#[test]
fn the_server_line_says_what_create_changes() {
    let servers = ["GeoServer".to_owned(), "api".to_owned()];
    let mut d = WfsDialog::new(PathBuf::from("p"), PathBuf::new(), "Maps", &servers);
    assert_eq!(d.server, ServerPick::Existing("GeoServer".into()));
    d.endpoint = "https://maps.test/geoserver/topp/wfs".into();
    let help = crate::components::VarHelp::default();
    let show =
        |d: &mut WfsDialog, own: bool, now: Option<&str>, expect: &[&str], absent: &[&str]| {
            let mut h = crate::test_host::harness(|ui| {
                let view = WfsView {
                    servers: &servers,
                    environment: Some("Staging"),
                    own_environment: own,
                    server_url: now,
                    inherited: None,
                    inherited_undefined: &[],
                    help: &help,
                };
                d.show(ui.ctx(), view);
            });
            h.run();
            for text in expect {
                assert!(h.query_by_label_contains(text).is_some(), "shows {text}");
            }
            for text in absent {
                assert!(h.query_by_label_contains(text).is_none(), "hides {text}");
            }
        };
    show(
        &mut d,
        true,
        Some("https://old.test/geoserver"),
        &[
            "In Staging, Create changes GeoServer from https://old.test/geoserver to https://maps.test/geoserver.",
            "Requests go to /topp/wfs on GeoServer.",
        ],
        &["of its own yet"],
    );
    show(
        &mut d,
        false,
        None,
        &[
            "In Staging, GeoServer will be https://maps.test/geoserver.",
            "The project has no “Staging” environment of its own yet",
        ],
        &[],
    );
    d.server = ServerPick::New;
    d.new_server = " API ".into();
    show(
        &mut d,
        true,
        None,
        &["The project already has a server called api"],
        &["Requests go to"],
    );
    d.new_server = "Kort ü😀".into();
    assert_eq!(d.server_name(), "Kort ü😀");
    show(&mut d, true, None, &["In Staging, Kort ü😀 will be"], &[]);
}

/// A GeoJSON response with one polygon in `crs`.
fn polygon_response(crs: &str, ring: &str) -> Response {
    let body = format!(
        r#"{{"type":"FeatureCollection","crs":{{"type":"name","properties":{{"name":"{crs}"}}}},
"features":[{{"type":"Feature","id":"area.1","geometry":{{"type":"Polygon","coordinates":[{ring}]}},"properties":{{"navn":"Kø ü😀"}}}}]}}"#
    );
    Response {
        body: body.into_bytes(),
        ..geojson_response()
    }
}

/// Shows the Features tab of `response` with `tabs`, runs `steps`, and returns what the
/// panel asked last.
fn features_panel(
    response: Response,
    tabs: &ResponseTabs,
    steps: impl FnOnce(&mut egui_kittest::Harness<'_>),
) -> Option<ResponseAction> {
    let outcome = Some(Outcome::from_result(Ok(response)));
    let mut st = ResponseState::default();
    let mut filter = String::new();
    let mut asked = None;
    {
        let mut h = crate::test_host::harness(|ui| {
            let cx = PanelContext {
                sending: false,
                can_save: false,
                filter: &mut filter,
                filtered: Filtered::Off,
                tests: None,
                wrap: false,
                example: SaveExample::Hidden,
                trend: Trend::default(),
                changes: None,
                cookies: CookiesTab::Hidden,
                addon_tabs: tabs.clone(),
            };
            if let Some(a) = response_panel(ui, outcome.as_ref(), &mut st, cx) {
                asked = Some(a);
            }
        });
        h.run();
        h.get_by_label("Features").click();
        h.run();
        steps(&mut h);
    }
    asked
}

const UTM_RING: &str =
    "[[724000,6175000],[725000,6175000],[725000,6176000],[724000,6176000],[724000,6175000]]";

/// Opens the Background list and clicks `entry`.
fn pick_background(h: &mut egui_kittest::Harness<'_>, entry: &str) {
    h.get_by_label("Background").click();
    h.run();
    h.get_by_label(entry).click();
    h.run();
}

/// R-GEO-3, D83: the map is on OpenStreetMap until another background is chosen;
/// adding one asks for a name and a tile URL and chooses it, and the list goes back to
/// OpenStreetMap. What is asked is kept in the settings.
#[test]
fn backgrounds_are_added_and_chosen_from_the_tab() {
    let response = || polygon_response("urn:ogc:def:crs:EPSG::25832", UTM_RING);
    let (_dir, _root, mut state) = app(true);
    let apply = |state: &mut AppState, asked: Option<ResponseAction>| {
        let Some(ResponseAction::AddOn(action)) = asked else {
            panic!("asked: {asked:?}");
        };
        state.apply_response_tab_action(action);
    };
    let tab = state.response_tabs().geoserver.expect("on");
    assert_eq!(tab.backgrounds.map.template, OPENSTREETMAP.1, "the default");

    let added = features_panel(response(), &state.response_tabs(), |h| {
        assert!(h.query_by_label("None").is_none(), "always a background");
        pick_background(h, "Add background…");
        // The row's two fields, name then URL.
        for (i, text) in ["Ortofoto ü😀", "https://tiles.test/{z}/{x}/{-y}.jpeg"]
            .into_iter()
            .enumerate()
        {
            let field = h
                .get_all_by_role(egui::accesskit::Role::TextInput)
                .nth(i)
                .expect("field");
            field.focus();
            field.type_text(text);
            h.run();
        }
        h.get_by_label("Add").click();
        h.run();
    });
    apply(&mut state, added);
    let tab = state.response_tabs().geoserver.expect("on");
    assert_eq!(tab.backgrounds.names, ["OpenStreetMap", "Ortofoto ü😀"]);
    assert_eq!(tab.backgrounds.map.name, "Ortofoto ü😀");
    assert_eq!(
        tab.backgrounds.map.template,
        "https://tiles.test/{z}/{x}/{-y}.jpeg"
    );

    let back = features_panel(response(), &state.response_tabs(), |h| {
        pick_background(h, "OpenStreetMap");
    });
    apply(&mut state, back);
    let tab = state.response_tabs().geoserver.expect("on");
    assert_eq!(tab.backgrounds.map.name, OPENSTREETMAP.0);
}

/// R-GEO-3, D83: a background the user added can be removed, and the map goes back to
/// OpenStreetMap, which can't be removed.
#[test]
fn an_added_background_can_be_removed() {
    let (_dir, _root, mut state) = app(true);
    assert!(
        features_panel(
            polygon_response("urn:ogc:def:crs:EPSG::25832", UTM_RING),
            &state.response_tabs(),
            |h| assert!(h.query_by_label("🗙").is_none()),
        )
        .is_none()
    );
    state.update_settings(|s| {
        s.geoserver.add_basemap(&tortuga_core::settings::Basemap {
            name: "Ortho".into(),
            url: "https://tiles.test/{z}/{x}/{y}".into(),
        });
    });
    let tabs = state.response_tabs();
    let asked = features_panel(
        polygon_response("urn:ogc:def:crs:EPSG::25832", UTM_RING),
        &tabs,
        |h| {
            h.get_by_label("🗙").click();
            h.run();
        },
    );
    let Some(ResponseAction::AddOn(action)) = asked else {
        panic!("asked: {asked:?}");
    };
    state.apply_response_tab_action(action);
    assert_eq!(state.settings.geoserver.basemaps, []);
    let tab = state.response_tabs().geoserver.expect("on");
    assert_eq!(tab.backgrounds.map.name, OPENSTREETMAP.0);
}

/// R-GEO-3, D83: UTM features are placed on the map, whose tiles are fetched from the
/// chosen background; a CRS the map can't place keeps the plain preview and says how to fix it.
#[test]
fn the_map_fetches_the_tiles_it_draws() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut settings = tortuga_core::AppSettings::default();
    settings.addons.set_enabled(AddOn::GeoServer, true);
    settings
        .geoserver
        .add_basemap(&tortuga_core::settings::Basemap {
            name: "Tiles".into(),
            url: "https://tiles.test/{z}/{x}/{y}.png".into(),
        });
    let host = TestHost {
        settings,
        ..TestHost::default()
    };
    let spawned = std::rc::Rc::clone(&host.spawned);
    let state = AppState::new(Box::new(host));
    let tabs = state.response_tabs();

    features_panel(
        polygon_response("urn:ogc:def:crs:EPSG::25832", UTM_RING),
        &tabs,
        |h| {
            assert!(h.query_by_label_contains("can't place").is_none());
            h.get_by_label("Kø ü😀");
        },
    );
    let before = spawned.get();
    state.addon_frame(&egui::Context::default());
    let fetched = spawned.get() - before;
    assert!(fetched > 0, "tiles are fetched");
    assert!(fetched <= 6, "a few at a time: {fetched}");

    features_panel(polygon_response("EPSG:2154", UTM_RING), &tabs, |h| {
        assert!(h.query_by_label_contains("can't place EPSG:2154").is_some());
    });
    drop(dir);
}

/// R-GEO-2: zooming the preview with the wheel, above the table, leaves the table where
/// it is (the Fit button that zooming shows took part in the layout and pulled the table
/// up into the preview).
#[test]
fn zooming_the_preview_leaves_the_table_where_it_is() {
    features_panel(
        polygon_response("urn:ogc:def:crs:EPSG::25832", UTM_RING),
        &ResponseTabs::with_geoserver(FeaturesLayout::Above),
        |h| {
            let summary = h.get_by_label("1 feature").rect();
            let header = h.get_by_label("navn").rect();
            let over_map = egui::pos2(
                header.center().x,
                f32::midpoint(summary.max.y, header.min.y),
            );
            h.hover_at(over_map);
            h.run();
            for _ in 0..5 {
                h.event(egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, 40.0),
                    phase: egui::TouchPhase::Move,
                    source: egui::MouseWheelSource::Unknown,
                    modifiers: egui::Modifiers::NONE,
                });
                h.step();
            }
            h.run_steps(20);
            let summary_after = h.get_by_label("1 feature").rect();
            let header_after = h.get_by_label("navn").rect();
            assert_eq!(summary_after, summary, "the summary stays");
            assert_eq!(header_after, header, "the table stays");
            // Zoomed: Fit is offered, and goes back.
            h.get_by_label("Fit").click();
            h.run();
            assert!(h.query_by_label("Fit").is_none());
            assert_eq!(h.get_by_label("navn").rect(), header);
        },
    );
}
