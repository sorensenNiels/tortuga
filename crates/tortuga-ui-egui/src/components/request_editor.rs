use egui::{Color32, ComboBox, Key, RichText, Stroke, TextEdit, Ui};
use tortuga_core::script::{Breakpoints, ScriptKind};
use tortuga_core::settings::{Feature, FeatureSettings, ResolvedSettings};
use tortuga_core::{Auth, HttpMethod, KeyValue, Request, RequestBody};

use super::TestsView;
use super::auth_editor::EditDefaults;
use super::request_examples::{ExamplesState, request_examples};
use super::request_preview::{ServerLine, preview_line, preview_panel};
use super::request_scripts::{ScriptTab, request_scripts};
use super::{
    BodyDrafts, FieldWidth, Lines, VarHelp, body_editor, description_editor, key_value_table,
    request_auth, var_edit,
};
use crate::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Params,
    Headers,
    Auth,
    Body,
    Options,
    /// Pre-request script, test script and captures (R-TEST-1, R-TEST-2, R-TEST-4).
    Scripts,
    /// Responses the mock server gives (R-MOCK-1).
    Examples,
    /// The request's description (R-COL-22).
    Docs,
}

/// Editor state that is not part of the request. Recreated (with [`EditorState::new`])
/// whenever the edited request is replaced from outside the editor.
#[derive(Debug, Clone)]
pub struct EditorState {
    /// Raw URL text as typed; only rewritten when the params table changes (R-HTTP-2).
    url_text: String,
    timeout_text: String,
    tab: Tab,
    body: BodyDrafts,
    focus_url: bool,
    /// The captures as table rows (R-TEST-4).
    pub(super) captures: Vec<KeyValue>,
    /// The open sub-tab of the Scripts tab (R-TEST-11).
    pub(super) script_tab: ScriptTab,
    /// The open example (R-MOCK-1).
    examples: ExamplesState,
    /// The Docs tab shows the description's text, not its rendering (R-COL-22).
    editing_docs: bool,
    /// The request as it will be sent shows under the URL (R-HTTP-14, R-COL-24).
    preview_open: bool,
}

impl EditorState {
    /// Sets the file of multipart part `index` after "Choose…" (R-HTTP-15).
    pub fn set_file(&mut self, request: &mut Request, index: usize, path: String) {
        self.body.set_file(request, index, path);
    }

    /// Opens the Scripts tab on its sub-tab `sub` (R-TEST-13).
    pub fn open_scripts(&mut self, sub: ScriptTab) {
        self.tab = Tab::Scripts;
        self.script_tab = sub;
    }

    /// Adds the capture `from` into `name`, or `name_2`, … when that name is taken
    /// (R-TEST-13). Nothing is added if the same capture is there already.
    pub fn add_capture(&mut self, request: &mut Request, name: &str, from: &str) {
        if request.capture.get(name).is_some_and(|f| f == from) {
            return;
        }
        let taken = |n: &str| self.captures.iter().any(|c| c.name.trim() == n);
        let name = if taken(name) {
            // At most `captures.len()` names are taken, so one of these is free.
            (2..=self.captures.len() + 2)
                .map(|i| format!("{name}_{i}"))
                .find(|n| !taken(n))
                .unwrap_or_else(|| name.to_owned())
        } else {
            name.to_owned()
        };
        request.capture.insert(name.clone(), from.to_owned());
        self.captures.push(KeyValue::new(name, from));
    }

    /// Opens the Examples tab on example `index` (R-MOCK-1).
    pub fn open_example(&mut self, index: usize) {
        self.tab = Tab::Examples;
        self.examples.open(index);
    }

    /// Puts the cursor in the URL field on the next frame (Ctrl+L, R-COL-27).
    pub fn focus_url(&mut self) {
        self.focus_url = true;
    }

    /// Whether the preview under the URL is open (so the shell works out the preview).
    pub fn shows_preview(&self) -> bool {
        self.preview_open
    }

    pub fn new(request: &Request) -> Self {
        Self {
            url_text: request.display_url(),
            timeout_text: request
                .timeout_ms
                .map(|ms| ms.to_string())
                .unwrap_or_default(),
            tab: Tab::Params,
            body: BodyDrafts::new(&request.body),
            focus_url: true,
            captures: request
                .capture
                .iter()
                .map(|(name, from)| KeyValue::new(name, from))
                .collect(),
            script_tab: ScriptTab::first_for(request),
            examples: ExamplesState::default(),
            editing_docs: false,
            preview_open: false,
        }
    }
}

/// Context the editor shows but does not change.
#[derive(Debug, Clone, Copy)]
pub struct EditorContext<'a> {
    /// Servers of the request's collection: (id, name).
    pub servers: &'a [(String, String)],
    /// Base URL of the selected server in the active environment.
    pub server_base: Option<&'a str>,
    /// The variables the request may use: undefined ones (R-VAR-5) and what hovering
    /// and `{{` show (R-VAR-9).
    pub vars: &'a VarHelp,
    pub sending: bool,
    /// The timeout, redirect and cookie settings the request inherits, with where each
    /// comes from (R-SET-1).
    pub settings: ResolvedSettings,
    /// The request's host when its certificate is not verified because it is in the
    /// settings' list (R-SET-3).
    pub unverified_host: Option<&'a str>,
    /// The collection's default auth; `None` for the scratch request (R-HTTP-5).
    pub collection_auth: Option<&'a Auth>,
    /// Headers this request inherits, with where each comes from (R-COL-4, R-COL-11),
    /// shown read-only.
    pub inherited_headers: &'a [(KeyValue, String)],
    /// Where the inherited auth comes from, e.g. "the project" or "folder “Admin”".
    pub auth_origin: &'a str,
    /// Cookies in the jar this request uses (R-CK-1).
    pub cookie_count: usize,
    /// The request belongs to a collection (its jar is saved); `false` for the scratch request.
    pub in_collection: bool,
    /// The request as it will be sent, while the Preview tab is open (R-HTTP-14).
    pub preview: Option<&'a Result<Request, String>>,
    /// Files to upload can be picked with a dialog (R-HTTP-15).
    pub can_pick_files: bool,
    /// Breakpoints in the request's scripts, kept while the app runs (R-TEST-10).
    pub breakpoints: &'a Breakpoints,
    /// What the request's scripts and captures did on its last send, shown on their lines
    /// (R-TEST-11).
    pub last_run: Option<&'a TestsView>,
    /// Whether the tests can run again on the last response (R-TEST-12).
    pub rerun: Rerun,
    /// How much of the editor shows (R-SET-12).
    pub view: EditorView,
    /// Which features are on (R-SET-13).
    pub features: &'a FeatureSettings,
}

/// How much of the request editor shows (R-SET-12).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorView {
    /// The URL line and the tabs.
    Full,
    /// Only the URL line: the response has the room.
    UrlOnly,
}

/// Whether "Re-run tests" can run (R-TEST-12).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rerun {
    /// The last send has a response to run the tests on.
    Ready,
    /// Sending, or no response yet.
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorAction {
    Send,
    /// Send to each environment of the project and compare (R-SRV-10).
    SendEverywhere,
    Cancel,
    /// Open the collection's default headers and auth.
    EditCollectionDefaults,
    /// Open the workspace's default headers and auth (R-SET-2).
    EditWorkspaceDefaults,
    /// A `curl` command was pasted into the URL field (R-HTTP-11).
    PasteCurl(String),
    /// Set or clear the breakpoint on a line of one of the scripts (R-TEST-10).
    ToggleBreakpoint(ScriptKind, u32),
    /// Run the captures and tests again on the last response, without sending (R-TEST-12).
    RerunTests,
    /// "Choose…" for the file of multipart part `n` (R-HTTP-15).
    ChooseFile(usize),
}

/// Method + server + URL bar and the request detail tabs (R-HTTP-1..4, R-HTTP-7, R-SRV-3).
pub fn request_editor(
    ui: &mut Ui,
    request: &mut Request,
    st: &mut EditorState,
    cx: EditorContext<'_>,
) -> Option<EditorAction> {
    let base_hint = match (&request.server, cx.server_base) {
        (Some(_), Some(base)) => format!("{base}…"),
        (Some(_), None) => "No URL for this server in the active environment".to_owned(),
        _ => String::new(),
    };
    let action = request_bar(ui, request, st, cx, &base_hint);

    let server = match (&request.server, cx.server_base) {
        (None, _) => ServerLine::None,
        (Some(_), Some(_)) => ServerLine::Base(&base_hint),
        (Some(_), None) => ServerLine::Missing(&base_hint),
    };
    preview_line(ui, &mut st.preview_open, server);
    if st.preview_open {
        preview_panel(ui, cx.preview);
    }
    if !cx.vars.undefined.is_empty() {
        let names: Vec<String> = cx
            .vars
            .undefined
            .iter()
            .map(|n| format!("{{{{{n}}}}}"))
            .collect();
        theme::warning(
            ui,
            format!(
                "Undefined: {} — define them in the project's environments or the workspace's variables.",
                names.join(", ")
            ),
        );
    }
    if let Some(host) = cx.unverified_host {
        theme::warning(
            ui,
            format!(
                "⚠ The certificate of {host} is not verified (Settings → Requests). Use this for \
                 test servers only."
            ),
        );
    }

    if cx.view == EditorView::UrlOnly {
        return action;
    }

    let param_count =
        request.params.iter().filter(|p| p.is_active()).count() + request.path_vars.len();
    let header_count = request.headers.iter().filter(|h| h.is_active()).count();
    let has_body = request.body != RequestBody::None;
    let effective_auth = request.auth.as_ref().or(cx.collection_auth);
    let has_auth = effective_auth.is_some_and(|a| !a.is_none());
    ui.add_space(6.0);
    let count = |n: usize| (n > 0).then(|| n.to_string());
    let scripted = !request.scripts.is_empty() || !request.capture.is_empty();
    let documented = !request.description.trim().is_empty();
    // A feature that is off hides its tab, unless this request uses it (R-SET-13).
    let shown = |feature: Feature, used: bool| used || cx.features.enabled(feature);
    let mut tabs = vec![
        (Tab::Params, "Params", count(param_count)),
        (
            Tab::Headers,
            "Headers",
            count(header_count + cx.inherited_headers.len()),
        ),
        (Tab::Auth, "Auth", has_auth.then(|| "⏺".to_owned())),
        (Tab::Body, "Body", has_body.then(|| "⏺".to_owned())),
        (Tab::Options, "Options", None),
    ];
    if shown(Feature::Scripts, scripted) {
        tabs.push((Tab::Scripts, "Scripts", scripted.then(|| "⏺".to_owned())));
    }
    // Only a project's requests are served by its mock, and documented.
    if cx.in_collection && shown(Feature::Examples, !request.examples.is_empty()) {
        tabs.push((Tab::Examples, "Examples", count(request.examples.len())));
    }
    if cx.in_collection && shown(Feature::Docs, documented) {
        tabs.push((Tab::Docs, "Docs", documented.then(|| "⏺".to_owned())));
    }
    if !tabs.iter().any(|(tab, _, _)| *tab == st.tab) {
        st.tab = Tab::Params;
    }
    theme::tabs(ui, &mut st.tab, &tabs);

    let in_tab = egui::ScrollArea::vertical()
        .id_salt("request-tab")
        .auto_shrink([false, true])
        .show(ui, |ui| tab_content(ui, request, st, cx))
        .inner;
    action.or(in_tab)
}

/// What the open tab shows.
fn tab_content(
    ui: &mut Ui,
    request: &mut Request,
    st: &mut EditorState,
    cx: EditorContext<'_>,
) -> Option<EditorAction> {
    let mut action = None;
    match st.tab {
        Tab::Params => {
            if !request.path_vars.is_empty() {
                path_variables(ui, &mut request.path_vars, cx.vars);
                ui.add_space(12.0);
            }
            if key_value_table(ui, "params", &mut request.params, "Parameter", cx.vars) {
                st.url_text = request.display_url();
            }
        }
        Tab::Headers => {
            key_value_table(ui, "headers", &mut request.headers, "Header", cx.vars);
            if !cx.inherited_headers.is_empty() {
                ui.add_space(8.0);
                inherited_headers(ui, cx.inherited_headers);
                ui.horizontal(|ui| {
                    if ui.small_button("Edit project defaults…").clicked() {
                        action = Some(EditorAction::EditCollectionDefaults);
                    }
                    let from_workspace = cx
                        .inherited_headers
                        .iter()
                        .any(|(_, origin)| origin == "the workspace");
                    if from_workspace && ui.small_button("Edit workspace defaults…").clicked() {
                        action = Some(EditorAction::EditWorkspaceDefaults);
                    }
                });
            }
            theme::hint(
                ui,
                "Content-Type is set automatically from the body type unless you add it here.",
            );
        }
        Tab::Auth => {
            match request_auth(
                ui,
                &mut request.auth,
                cx.collection_auth,
                cx.auth_origin,
                cx.vars,
            ) {
                Some(EditDefaults::Project) => action = Some(EditorAction::EditCollectionDefaults),
                Some(EditDefaults::Workspace) => {
                    action = Some(EditorAction::EditWorkspaceDefaults);
                }
                None => {}
            }
        }
        Tab::Body => {
            if let Some(i) = body_editor(ui, request, &mut st.body, cx.vars, cx.can_pick_files) {
                action = Some(EditorAction::ChooseFile(i));
            }
        }
        Tab::Options => request_options(ui, request, st, cx),
        Tab::Scripts => {
            if let Some(asked) = request_scripts(ui, request, st, cx) {
                action = Some(asked);
            }
        }
        Tab::Examples => {
            request_examples(ui, &mut request.examples, &mut st.examples, cx.vars);
        }
        Tab::Docs => {
            description_editor(
                ui,
                "request",
                &mut request.description,
                &mut st.editing_docs,
                false,
                f32::INFINITY,
            );
        }
    }
    action
}

/// Method, server, URL and Send/Cancel.
fn request_bar(
    ui: &mut Ui,
    request: &mut Request,
    st: &mut EditorState,
    cx: EditorContext<'_>,
    base_hint: &str,
) -> Option<EditorAction> {
    let mut action = None;
    let send_width = 92.0;
    let url_id = ui.make_persistent_id("url");
    let url_focused = ui.memory(|m| m.has_focus(url_id));
    ui.horizontal(|ui| {
        let group_width = ui.available_width() - send_width - ui.spacing().item_spacing.x - 10.0;
        let mut frame = theme::input_group(ui);
        if url_focused {
            frame = frame.stroke(Stroke::new(1.0, theme::palette(ui).accent));
        }
        let group = frame.show(ui, |ui| {
            ui.set_width(group_width);
            // The group draws the border; the controls inside are flat.
            let v = ui.visuals_mut();
            for w in [
                &mut v.widgets.inactive,
                &mut v.widgets.hovered,
                &mut v.widgets.active,
                &mut v.widgets.open,
            ] {
                w.bg_stroke = Stroke::NONE;
            }
            v.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
            v.widgets.inactive.bg_fill = Color32::TRANSPARENT;
            v.text_edit_bg_color = Some(Color32::TRANSPARENT);
            // The group shows focus instead of the field.
            v.selection.stroke = Stroke::NONE;
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.horizontal(|ui| {
                method_picker(ui, request);
                ui.separator();
                if !cx.servers.is_empty() || request.server.is_some() {
                    server_picker(ui, request, cx, base_hint);
                    ui.separator();
                }
                action = url_field(ui, url_id, request, st, cx);
            });
        });

        let size = [send_width, group.response.rect.height()];
        if cx.sending {
            if ui
                .add_sized(size, theme::danger_button(ui, "Cancel"))
                .clicked()
            {
                action = Some(EditorAction::Cancel);
            }
        } else {
            let send = ui
                .add_sized(size, theme::primary_button(ui, "Send"))
                .on_hover_text("Send (Ctrl+Enter). Right-click to send to every environment.");
            if send.clicked() {
                action = Some(EditorAction::Send);
            }
            if cx.in_collection {
                send.context_menu(|ui| {
                    if ui.button("Send to every environment").clicked() {
                        action = Some(EditorAction::SendEverywhere);
                    }
                });
            }
        }
    });
    ui.add_space(2.0);
    action
}

/// Values of the URL's `:name` path variables (R-HTTP-10). Names follow the URL.
fn path_variables(ui: &mut Ui, vars: &mut [KeyValue], help: &VarHelp) {
    // Same column layout as the params table below, so the two line up.
    let width = ((ui.available_width() - 90.0) / 2.0).max(80.0);
    egui::Grid::new("path-vars")
        .num_columns(3)
        .spacing([6.0, 4.0])
        .show(ui, |ui| {
            ui.label("");
            theme::section_label(ui, "Path variable");
            theme::section_label(ui, "Value");
            ui.end_row();
            for (i, var) in vars.iter_mut().enumerate() {
                ui.label("");
                // The name comes from the URL: shown read-only (still selectable).
                let mut name = format!(":{}", var.name);
                let name_field = TextEdit::singleline(&mut name)
                    .id_salt(("path-var-name", i))
                    .font(egui::TextStyle::Monospace)
                    .interactive(false)
                    .hint_text("Name");
                theme::sized_edit(ui, width, name_field)
                    .on_hover_text("Change the name in the URL");
                let id = ui.make_persistent_id(("path-var", i));
                let size = FieldWidth::Fixed(width);
                var_edit(ui, id, &mut var.value, help, Lines::One, size, |e| {
                    e.hint_text("Value")
                });
                ui.end_row();
            }
        });
    theme::hint(
        ui,
        "Path variables come from :name segments in the URL. An empty value stops the send.",
    );
}

/// Headers this request inherits, grouped by where they come from (R-COL-4, R-COL-11), read-only.
fn inherited_headers(ui: &mut Ui, headers: &[(KeyValue, String)]) {
    let p = theme::palette(ui);
    let mut origins: Vec<&str> = headers.iter().map(|(_, o)| o.as_str()).collect();
    origins.dedup();
    for (i, origin) in origins.into_iter().enumerate() {
        if i > 0 {
            ui.add_space(6.0);
        }
        theme::section_label(ui, &format!("From {origin}"));
        egui::Grid::new(("inherited-headers", i))
            .num_columns(2)
            .spacing([24.0, 4.0])
            .show(ui, |ui| {
                for (h, _) in headers.iter().filter(|(_, o)| o == origin) {
                    ui.label(RichText::new(&h.name).monospace().color(p.muted));
                    ui.label(RichText::new(&h.value).monospace().color(p.muted));
                    ui.end_row();
                }
            });
    }
    theme::hint(
        ui,
        "Add a header with the same name above to replace one, or add it switched off to leave it out.",
    );
}

fn method_picker(ui: &mut Ui, request: &mut Request) {
    let method = request.method;
    ComboBox::from_id_salt("method")
        .selected_text(theme::method_text(ui, method))
        .width(84.0)
        .show_ui(ui, |ui| {
            for m in HttpMethod::ALL {
                ui.selectable_value(&mut request.method, m, theme::method_text(ui, m));
            }
        })
        .response
        .on_hover_text("HTTP method");
}

fn server_picker(ui: &mut Ui, request: &mut Request, cx: EditorContext<'_>, base_hint: &str) {
    let missing_server = request
        .server
        .as_ref()
        .filter(|id| !cx.servers.iter().any(|(s, _)| s == *id))
        .cloned();
    let selected = match &request.server {
        None => "No server".to_owned(),
        Some(id) => cx
            .servers
            .iter()
            .find(|(s, _)| s == id)
            .map_or_else(|| format!("{id} (missing)"), |(_, name)| name.clone()),
    };
    let combo = ComboBox::from_id_salt("server")
        .selected_text(selected)
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut request.server, None, "No server");
            for (id, name) in cx.servers {
                ui.selectable_value(&mut request.server, Some(id.clone()), name);
            }
            if let Some(id) = &missing_server {
                ui.selectable_value(
                    &mut request.server,
                    Some(id.clone()),
                    format!("{id} (missing)"),
                );
            }
        })
        .response;
    if !base_hint.is_empty() {
        combo.on_hover_text(base_hint);
    }
}

fn url_field(
    ui: &mut Ui,
    id: egui::Id,
    request: &mut Request,
    st: &mut EditorState,
    cx: EditorContext<'_>,
) -> Option<EditorAction> {
    let hint = if request.server.is_some() {
        "/path"
    } else {
        "Enter request URL"
    };
    let min_size = egui::vec2(0.0, ui.spacing().interact_size.y);
    let width = ui.available_width();
    // A pasted curl command fills the whole request instead of the URL (R-HTTP-11).
    let pasted_curl = ui
        .memory(|m| m.has_focus(id))
        .then(|| ui.input_mut(take_pasted_curl))
        .flatten();
    let url = var_edit(
        ui,
        id,
        &mut st.url_text,
        cx.vars,
        Lines::One,
        FieldWidth::AsBuilt,
        |e| {
            e.font(egui::TextStyle::Monospace)
                .align(egui::Align2::LEFT_CENTER)
                .min_size(min_size)
                .hint_text(hint)
                .desired_width(width)
        },
    );
    if let Some(text) = pasted_curl {
        return Some(EditorAction::PasteCurl(text));
    }
    if st.focus_url {
        url.request_focus();
        st.focus_url = false;
    }
    if url.changed() {
        request.set_display_url(&st.url_text);
    }
    (url.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) && !cx.sending)
        .then_some(EditorAction::Send)
}

/// Removes a paste of a `curl` command from this frame's input and returns its text.
fn take_pasted_curl(input: &mut egui::InputState) -> Option<String> {
    let at = input.events.iter().position(
        |e| matches!(e, egui::Event::Paste(text) if tortuga_core::curl::import::is_command(text)),
    )?;
    match input.events.remove(at) {
        egui::Event::Paste(text) => Some(text),
        _ => None,
    }
}

/// Per-request options: timeout (R-HTTP-7).
fn request_options(
    ui: &mut Ui,
    request: &mut Request,
    st: &mut EditorState,
    cx: EditorContext<'_>,
) {
    let timeout = cx.settings.timeout_ms;
    ui.horizontal(|ui| {
        ui.label("Timeout (ms)");
        let edit = TextEdit::singleline(&mut st.timeout_text)
            .id_salt("timeout")
            .hint_text(format!("{} (from {})", timeout.value, timeout.from.label()))
            .desired_width(220.0);
        if ui.add(edit).changed() {
            request.timeout_ms = parse_timeout(&st.timeout_text);
        }
    });
    theme::hint(
        ui,
        "Leave empty to use the project's, the workspace's or the app's setting.",
    );

    // R-CK-2: per request, saved in the request file.
    ui.add_space(12.0);
    let jar = if cx.in_collection {
        "this project's cookie jar"
    } else {
        "the scratch cookie jar (kept until the app closes)"
    };
    // Off for the project or workspace: a request can only turn cookies off (R-SET-1).
    let cookies = cx.settings.cookies;
    ui.add_enabled_ui(cookies.value, |ui| {
        ui.checkbox(&mut request.cookies, format!("Use cookies from {jar}"));
    });
    if !cookies.value {
        theme::hint(
            ui,
            format!(
                "Cookies are turned off in {}, so this request sends and stores none.",
                cookies.from.label()
            ),
        );
    }
    let stored = match cx.cookie_count {
        1 => "1 cookie stored".to_owned(),
        n => format!("{n} cookies stored"),
    };
    theme::hint(
        ui,
        format!(
            "{stored}. When off, no cookies from the jar are sent and cookies in the response are not \
             stored. A Cookie header you add yourself is always sent."
        ),
    );
}

/// A positive number of milliseconds; anything else means "use the default".
pub fn parse_timeout(text: &str) -> Option<u64> {
    text.trim().parse().ok().filter(|ms| *ms > 0)
}

#[cfg(test)]
mod tests {
    use egui_kittest::kittest::Queryable;

    use super::*;

    fn test_settings() -> ResolvedSettings {
        ResolvedSettings::resolve(&[], &tortuga_core::ClientSettings::default())
    }

    #[test]
    fn path_variables_get_a_value_row() {
        let mut request = Request::new(HttpMethod::Get, "https://a.test/users/:id");
        let mut st = EditorState::new(&request);
        st.focus_url = false;
        {
            let mut h = crate::test_host::harness(|ui| {
                request_editor(
                    ui,
                    &mut request,
                    &mut st,
                    EditorContext {
                        servers: &[],
                        server_base: None,
                        vars: &VarHelp::default(),
                        sending: false,
                        settings: test_settings(),
                        unverified_host: None,
                        collection_auth: None,
                        inherited_headers: &[],
                        auth_origin: "the project",
                        cookie_count: 0,
                        in_collection: false,
                        preview: None,
                        can_pick_files: false,
                        breakpoints: &Breakpoints::default(),
                        last_run: None,
                        rerun: Rerun::Unavailable,
                        view: EditorView::Full,
                        features: &FeatureSettings::default(),
                    },
                );
            });
            h.run();
            let inputs: Vec<_> = h
                .get_all_by_role(egui::accesskit::Role::TextInput)
                .collect();
            let name_at = inputs
                .iter()
                .position(|n| n.value().as_deref() == Some(":id"))
                .expect("name shown");
            let value = &inputs[name_at + 1];
            value.focus();
            value.type_text("42");
            h.run();
        }
        assert_eq!(request.path_vars, [KeyValue::new("id", "42")]);
    }

    /// R-COL-22: a project's request has a Docs tab showing its description rendered;
    /// the scratch request has none.
    #[test]
    fn docs_tab_shows_the_description() {
        for in_collection in [true, false] {
            let mut request = Request::new(HttpMethod::Get, "https://a.test/");
            request.description = "## Lists **orders**\n\nNewest first.".into();
            let mut st = EditorState::new(&request);
            st.focus_url = false;
            let mut h = crate::test_host::harness(|ui| {
                request_editor(
                    ui,
                    &mut request,
                    &mut st,
                    EditorContext {
                        servers: &[],
                        server_base: None,
                        vars: &VarHelp::default(),
                        sending: false,
                        settings: test_settings(),
                        unverified_host: None,
                        collection_auth: None,
                        inherited_headers: &[],
                        auth_origin: "the project",
                        cookie_count: 0,
                        in_collection,
                        preview: None,
                        can_pick_files: false,
                        breakpoints: &Breakpoints::default(),
                        last_run: None,
                        rerun: Rerun::Unavailable,
                        view: EditorView::Full,
                        features: &FeatureSettings::default(),
                    },
                );
            });
            h.run();
            let tab = h.query_by_label_contains("Docs");
            assert_eq!(tab.is_some(), in_collection);
            if let Some(tab) = tab {
                tab.click();
                h.run();
                h.get_by_label_contains("Newest first.");
                h.get_by_label("Edit");
            }
        }
    }

    /// R-HTTP-14, R-COL-24: the line under the URL opens the resolved request, with Basic
    /// credentials readable.
    #[test]
    fn the_preview_opens_under_the_url() {
        let mut request = Request::new(HttpMethod::Get, "https://a.test/");
        let mut st = EditorState::new(&request);
        st.focus_url = false;
        let mut sent = Request::new(HttpMethod::Post, "https://api.test/users?page=2");
        // `alice:<secret:pw>` in base64
        sent.headers = vec![KeyValue::new(
            "Authorization",
            "Basic YWxpY2U6PHNlY3JldDpwdz4=",
        )];
        sent.body = RequestBody::Json("{\"a\": 1}".into());
        let preview = Ok(sent);
        let mut h = crate::test_host::harness(|ui| {
            request_editor(
                ui,
                &mut request,
                &mut st,
                EditorContext {
                    servers: &[],
                    server_base: None,
                    vars: &VarHelp::default(),
                    sending: false,
                    settings: test_settings(),
                    unverified_host: None,
                    collection_auth: None,
                    inherited_headers: &[],
                    auth_origin: "the project",
                    cookie_count: 0,
                    in_collection: false,
                    preview: Some(&preview),
                    can_pick_files: false,
                    breakpoints: &Breakpoints::default(),
                    last_run: None,
                    rerun: Rerun::Unavailable,
                    view: EditorView::Full,
                    features: &FeatureSettings::default(),
                },
            );
        });
        h.run();
        assert!(
            h.query_by_label("https://api.test/users?page=2").is_none(),
            "closed"
        );
        h.get_by_label_contains("Preview").click();
        h.run();
        h.get_by_label("https://api.test/users?page=2");
        h.get_by_label("Basic alice:<secret:pw>");
        h.get_by_label("{\"a\": 1}");
        assert!(
            h.query_by_label("Preview").is_none(),
            "no Preview tab any more"
        );
        h.key_press(egui::Key::Escape);
        h.run();
        assert!(
            h.query_by_label("https://api.test/users?page=2").is_none(),
            "Escape closes it"
        );
    }

    /// R-TEST-1, R-TEST-4, R-TEST-11: scripts and captures are edited in the Scripts
    /// tab's sub-tabs.
    #[test]
    fn scripts_tab_edits_scripts_and_captures() {
        let mut request = Request::new(HttpMethod::Get, "https://a.test/");
        let mut st = EditorState::new(&request);
        st.focus_url = false;
        st.tab = Tab::Scripts;
        {
            let mut h = crate::test_host::harness(|ui| {
                request_editor(
                    ui,
                    &mut request,
                    &mut st,
                    EditorContext {
                        servers: &[],
                        server_base: None,
                        vars: &VarHelp::default(),
                        sending: false,
                        settings: test_settings(),
                        unverified_host: None,
                        collection_auth: None,
                        inherited_headers: &[],
                        auth_origin: "the project",
                        cookie_count: 0,
                        in_collection: false,
                        preview: None,
                        can_pick_files: false,
                        breakpoints: &Breakpoints::default(),
                        last_run: None,
                        rerun: Rerun::Unavailable,
                        view: EditorView::Full,
                        features: &FeatureSettings::default(),
                    },
                );
            });
            h.run();
            let fields: Vec<_> = h
                .get_all_by_role(egui::accesskit::Role::MultilineTextInput)
                .collect();
            assert_eq!(fields.len(), 1, "one script at a time: the tests first");
            fields[0].focus();
            fields[0].type_text("test('ok', () => {})");
            h.run();
            h.get_by_label("Captures").click();
            h.run();
            let capture_name = h
                .get_all_by_role(egui::accesskit::Role::TextInput)
                .find(|n| n.value().as_deref() == Some(""))
                .expect("capture row");
            capture_name.focus();
            capture_name.type_text("token");
            h.run();
        }
        assert_eq!(request.scripts.tests, "test('ok', () => {})");
        assert_eq!(request.capture.keys().collect::<Vec<_>>(), ["token"]);
    }

    /// R-TEST-7: "Beautify" tidies a script.
    #[test]
    fn beautify_tidies_a_script() {
        let mut request = Request::new(HttpMethod::Get, "https://a.test/");
        request.scripts.tests = "test('ok',()=>{expect(res.status).toBe(200);});".into();
        let mut st = EditorState::new(&request);
        st.focus_url = false;
        st.tab = Tab::Scripts;
        {
            let mut h = crate::test_host::harness(|ui| {
                request_editor(
                    ui,
                    &mut request,
                    &mut st,
                    EditorContext {
                        servers: &[],
                        server_base: None,
                        vars: &VarHelp::default(),
                        sending: false,
                        settings: test_settings(),
                        unverified_host: None,
                        collection_auth: None,
                        inherited_headers: &[],
                        auth_origin: "the project",
                        cookie_count: 0,
                        in_collection: false,
                        preview: None,
                        can_pick_files: false,
                        breakpoints: &Breakpoints::default(),
                        last_run: None,
                        rerun: Rerun::Unavailable,
                        view: EditorView::Full,
                        features: &FeatureSettings::default(),
                    },
                );
            });
            h.run();
            h.get_by_label("Beautify").click();
            h.run();
        }
        assert_eq!(
            request.scripts.tests,
            "test('ok', () => {\n  expect(res.status).toBe(200);\n});"
        );
        assert_eq!(request.scripts.pre_request, "");
    }

    /// R-TEST-13: captures from the response get a free name and are not doubled; the
    /// Scripts tab opens on them.
    #[test]
    fn captures_from_the_response_get_free_names() {
        let mut request = Request::new(HttpMethod::Get, "https://a.test/");
        let mut st = EditorState::new(&request);
        st.add_capture(&mut request, "token", "$.data.token");
        st.add_capture(&mut request, "token", "$.data.token");
        st.add_capture(&mut request, "token", "$.other.token");
        st.add_capture(&mut request, "ü😀", "$['ü😀']");
        let names: Vec<&str> = request.capture.keys().map(String::as_str).collect();
        assert_eq!(names, ["token", "token_2", "ü😀"]);
        assert_eq!(st.captures.len(), 3, "the table rows match");
        st.open_scripts(ScriptTab::Captures);
        assert_eq!((st.tab, st.script_tab), (Tab::Scripts, ScriptTab::Captures));
    }

    #[test]
    fn timeouts_must_be_positive_numbers() {
        assert_eq!(parse_timeout(" 500 "), Some(500));
        assert_eq!(parse_timeout("0"), None);
        assert_eq!(parse_timeout(""), None);
        assert_eq!(parse_timeout("abc"), None);
    }
}
