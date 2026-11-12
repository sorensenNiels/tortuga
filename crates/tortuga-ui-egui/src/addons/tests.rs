use egui_kittest::kittest::Queryable;

use super::*;
use crate::TortugaUi;
use crate::test_host::TestHost;

/// A host whose settings have the MCP server add-on on or off.
fn host(mcp_on: bool) -> TestHost {
    let mut settings = tortuga_core::AppSettings::default();
    settings.addons.set_enabled(AddOn::McpServer, mcp_on);
    TestHost {
        settings,
        ..TestHost::default()
    }
}

fn running(state: &AppState) -> bool {
    state
        .addons
        .mcp
        .as_ref()
        .and_then(mcp::McpServer::running)
        .is_some()
}

/// R-ADD-2: with every add-on off, the default, none is loaded and none of its commands
/// exists or does anything.
#[test]
fn addons_are_off_and_not_loaded_by_default() {
    let mut state = AppState::new(Box::new(TestHost::default()));
    assert!(state.addons.mcp.is_none());
    assert!(palette(&state).is_empty());
    assert_eq!(run(&mut state, Command::Mcp(mcp::Command::Open)), None);
    assert!(state.start_mcp().is_err());
    assert!(!running(&state));
}

/// R-ADD-1, R-ADD-3: turned on, the MCP server is loaded and starts; turned off in the
/// settings, it stops, says so and is unloaded; on again, it is loaded but stopped.
#[test]
fn turning_the_mcp_server_off_stops_it() {
    let mut state = AppState::new(Box::new(host(true)));
    assert_eq!(palette(&state).len(), 1, "Start MCP server");
    assert_eq!(
        run(&mut state, Command::Mcp(mcp::Command::Open)),
        Some(AddOn::McpServer)
    );
    assert!(running(&state));
    assert_eq!(palette(&state).len(), 2, "show and stop");

    let mut settings = state.settings.clone();
    settings.addons.set_enabled(AddOn::McpServer, false);
    state.apply_settings(&settings).expect("apply");
    assert!(state.addons.mcp.is_none());
    assert_eq!(
        state.toast.as_ref().map(|t| t.text.as_str()),
        Some("The MCP server stopped.")
    );
    assert!(palette(&state).is_empty());

    settings.addons.set_enabled(AddOn::McpServer, true);
    state.apply_settings(&settings).expect("apply");
    assert!(state.addons.mcp.is_some());
    assert!(!running(&state));
}

/// R-ADD-1: an add-on that can't work in this build is not listed, and stays unloaded
/// even when the settings say it is on (e.g. settings shared with the web version).
#[test]
fn addons_that_cannot_work_here_are_not_listed() {
    assert_eq!(
        listed(&TestHost::default()),
        vec![AddOn::Workflows, AddOn::MockServer, AddOn::McpServer]
    );
    let web = TestHost {
        no_servers: true,
        ..host(true)
    };
    assert_eq!(listed(&web), vec![AddOn::Workflows]);
    let state = AppState::new(Box::new(web));
    assert!(state.addons.mcp.is_none());
    assert!(
        state.addons.mock.is_none(),
        "on by default, but can't run here"
    );
}

/// R-ADD-5: the mock server add-on is on by default; turned off, running mocks stop
/// with a toast and its commands are gone.
#[test]
fn the_mock_server_is_on_by_default_and_stops_when_turned_off() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join("shop-ü😀");
    tortuga_core::Collection::create(&tortuga_core::storage::FsStorage, &root, "Shop ü😀")
        .expect("create");
    let mut state = AppState::new(Box::new(TestHost {
        open: vec![root.clone()],
        ..TestHost::default()
    }));
    assert!(state.addons.mock.is_some(), "on by default");
    assert_eq!(project_palette(&state, &root).len(), 1, "Start mock server");
    let started = run(
        &mut state,
        Command::Mock(mock::Command::Start(root.clone())),
    );
    assert_eq!(started, None, "starting opens no dialog");
    assert!(state.mock_port(&root).is_some());

    let mut settings = state.settings.clone();
    settings.addons.set_enabled(AddOn::MockServer, false);
    state.apply_settings(&settings).expect("apply");
    assert!(state.addons.mock.is_none());
    assert_eq!(state.mock_port(&root), None);
    assert_eq!(
        state.toast.as_ref().map(|t| t.text.as_str()),
        Some("The mock server add-on is off; the running mock servers stopped.")
    );
    assert!(project_palette(&state, &root).is_empty());
    assert!(state.start_mock(&root).is_err());
}

/// R-ADD-2: the whole app shows the MCP server's workspace menu item only while the
/// add-on is on.
#[test]
fn the_app_shows_an_addon_only_while_it_is_on() {
    let name = AppState::new(Box::new(TestHost::default()))
        .workspace()
        .name
        .clone();
    for on in [false, true] {
        let mut ui = TortugaUi::new(Box::new(host(on)));
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(1400.0, 860.0))
            .build_ui(|egui_ui| ui.ui(egui_ui));
        h.run_steps(2);
        h.get_by_label_contains(&name).click();
        h.run_steps(2);
        assert_eq!(
            h.query_by_label("Start MCP server for AI agents").is_some(),
            on,
            "add-on on: {on}"
        );
    }
}
