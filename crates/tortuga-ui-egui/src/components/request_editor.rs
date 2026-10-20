use egui::{Color32, ComboBox, Key, Label, RichText, Stroke, TextEdit, Ui};
use tortuga_core::script::{Breakpoints, ScriptKind};
use tortuga_core::settings::ResolvedSettings;
use tortuga_core::{Auth, HttpMethod, KeyValue, Request, RequestBody};

use super::auth_editor::EditDefaults;
use super::script_editor::{script_editor, script_heading};
use super::{BodyDrafts, body_editor, key_value_table, request_auth};
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
    /// The request as it will be sent (R-HTTP-14).
    Preview,
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
    captures: Vec<KeyValue>,
}

impl EditorState {
    /// Sets the file of multipart part `index` after "Choose…" (R-HTTP-15).
    pub fn set_file(&mut self, request: &mut Request, index: usize, path: String) {
        self.body.set_file(request, index, path);
    }

    /// Whether the Preview tab is open (so the shell works out the preview).
    pub fn shows_preview(&self) -> bool {
        self.tab == Tab::Preview
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
    /// Variables used by the request that are not defined (R-VAR-5).
    pub undefined: &'a [String],
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorAction {
    Send,
    Cancel,
    /// Open the collection's default headers and auth.
    EditCollectionDefaults,
    /// Open the workspace's default headers and auth (R-SET-2).
    EditWorkspaceDefaults,
    /// A `curl` command was pasted into the URL field (R-HTTP-11).
    PasteCurl(String),
    /// Set or clear the breakpoint on a line of one of the scripts (R-TEST-10).
    ToggleBreakpoint(ScriptKind, u32),
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

    if request.server.is_some() {
        let text = RichText::new(&base_hint).small().monospace();
        if cx.server_base.is_some() {
            ui.label(text.weak());
        } else {
            ui.label(text.color(theme::palette(ui).warning_text));
        }
    }
    if !cx.undefined.is_empty() {
        let names: Vec<String> = cx
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

    let param_count =
        request.params.iter().filter(|p| p.is_active()).count() + request.path_vars.len();
    let header_count = request.headers.iter().filter(|h| h.is_active()).count();
    let has_body = request.body != RequestBody::None;
    let effective_auth = request.auth.as_ref().or(cx.collection_auth);
    let has_auth = effective_auth.is_some_and(|a| !a.is_none());
    ui.add_space(6.0);
    let count = |n: usize| (n > 0).then(|| n.to_string());
    theme::tabs(
        ui,
        &mut st.tab,
        &[
            (Tab::Params, "Params", count(param_count)),
            (
                Tab::Headers,
                "Headers",
                count(header_count + cx.inherited_headers.len()),
            ),
            (Tab::Auth, "Auth", has_auth.then(|| "⏺".to_owned())),
            (Tab::Body, "Body", has_body.then(|| "⏺".to_owned())),
            (Tab::Options, "Options", None),
            (
                Tab::Scripts,
                "Scripts",
                (!request.scripts.is_empty() || !request.capture.is_empty())
                    .then(|| "⏺".to_owned()),
            ),
            (Tab::Preview, "Preview", None),
        ],
    );

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
                path_variables(ui, &mut request.path_vars, cx.undefined);
                ui.add_space(12.0);
            }
            if key_value_table(ui, "params", &mut request.params, "Parameter", cx.undefined) {
                st.url_text = request.display_url();
            }
        }
        Tab::Headers => {
            key_value_table(ui, "headers", &mut request.headers, "Header", cx.undefined);
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
                cx.undefined,
            ) {
                Some(EditDefaults::Project) => action = Some(EditorAction::EditCollectionDefaults),
                Some(EditDefaults::Workspace) => {
                    action = Some(EditorAction::EditWorkspaceDefaults);
                }
                None => {}
            }
        }
        Tab::Body => {
            if let Some(i) = body_editor(ui, request, &mut st.body, cx.undefined, cx.can_pick_files)
            {
                action = Some(EditorAction::ChooseFile(i));
            }
        }
        Tab::Options => request_options(ui, request, st, cx),
        Tab::Scripts => {
            if let Some(toggled) = request_scripts(ui, request, st, cx.breakpoints) {
                action = Some(toggled);
            }
        }
        Tab::Preview => request_preview(ui, cx.preview),
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
        } else if ui
            .add_sized(size, theme::primary_button(ui, "Send"))
            .on_hover_text("Send (Ctrl+Enter)")
            .clicked()
        {
            action = Some(EditorAction::Send);
        }
    });
    ui.add_space(2.0);
    action
}

/// Values of the URL's `:name` path variables (R-HTTP-10). Names follow the URL.
fn path_variables(ui: &mut Ui, vars: &mut [KeyValue], undefined: &[String]) {
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
                let flagged = theme::uses_undefined(&var.value, undefined);
                let mut edit = TextEdit::singleline(&mut var.value)
                    .id_salt(("path-var", i))
                    .hint_text("Value");
                if flagged {
                    edit = edit.background_color(theme::undefined_bg(ui));
                }
                theme::sized_edit(ui, width, edit);
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
    let flagged = theme::uses_undefined(&st.url_text, cx.undefined);
    let mut url = TextEdit::singleline(&mut st.url_text)
        .id(id)
        .font(egui::TextStyle::Monospace)
        .align(egui::Align2::LEFT_CENTER)
        .min_size(egui::vec2(0.0, ui.spacing().interact_size.y))
        .hint_text(if request.server.is_some() {
            "/path"
        } else {
            "Enter request URL"
        })
        .desired_width(ui.available_width());
    if flagged {
        url = url.background_color(theme::undefined_bg(ui));
    }
    // A pasted curl command fills the whole request instead of the URL (R-HTTP-11).
    let pasted_curl = ui
        .memory(|m| m.has_focus(id))
        .then(|| ui.input_mut(take_pasted_curl))
        .flatten();
    let url = ui.add(url);
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

/// The scripts and captures of a request (R-TEST-1, R-TEST-2, R-TEST-4). Returns the
/// breakpoint the user set or cleared (R-TEST-10).
fn request_scripts(
    ui: &mut Ui,
    request: &mut Request,
    st: &mut EditorState,
    breakpoints: &Breakpoints,
) -> Option<EditorAction> {
    script_heading(ui, "Pre-request script", &mut request.scripts.pre_request);
    let pre = script_editor(
        ui,
        "pre-request",
        &mut request.scripts.pre_request,
        4,
        "vars.set(\"now\", String(Date.now()));\nreq.setHeader(\"X-Request-Id\", vars.get(\"now\"));",
        Some(&breakpoints.pre_request),
    )
    .map(|line| EditorAction::ToggleBreakpoint(ScriptKind::PreRequest, line));
    theme::hint(
        ui,
        "Runs before variables are filled in: vars.get/set/unset, req.setHeader(name, value), console.log.",
    );
    ui.add_space(10.0);
    script_heading(ui, "Tests", &mut request.scripts.tests);
    let tests = script_editor(
        ui,
        "tests",
        &mut request.scripts.tests,
        6,
        "test(\"status is 200\", () => expect(res.status).toBe(200));\ntest(\"has an id\", () => expect(res.json()).toHaveProperty(\"id\"));",
        Some(&breakpoints.tests),
    )
    .map(|line| EditorAction::ToggleBreakpoint(ScriptKind::Tests, line));
    theme::hint(
        ui,
        "Runs after the response: test(name, fn), expect(x).toBe/toEqual/toContain/toMatch/toHaveProperty/… (and .not) \
         or Chai's expect(x).to.equal/.to.be.an(\"object\")/.to.have.property/…, \
         res.status, res.json(), res.header(name), res.time, vars, console.log. \
         Click a line number to set a breakpoint: the Console then shows the variables there.",
    );
    ui.add_space(10.0);
    theme::section_label(ui, "Captures");
    if key_value_table(ui, "captures", &mut st.captures, "Variable", &[]) {
        request.capture = st
            .captures
            .iter()
            .filter(|c| c.is_active())
            .map(|c| (c.name.trim().to_owned(), c.value.trim().to_owned()))
            .collect();
    }
    theme::hint(
        ui,
        "Puts a value from the response into a variable: $.json.path, xpath://xml/path, \
         header:Name or status. \
         Captured values are kept until Tortuga closes and win over environment values.",
    );
    pre.or(tests)
}

/// The request exactly as it will be sent, secrets masked (R-HTTP-14).
fn request_preview(ui: &mut Ui, preview: Option<&Result<Request, String>>) {
    let p = theme::palette(ui);
    let request = match preview {
        None => return,
        Some(Err(message)) => return theme::warning(ui, message),
        Some(Ok(request)) => request,
    };
    let mono = |text: &str| {
        Label::new(RichText::new(text).monospace())
            .selectable(true)
            .wrap()
    };
    let url = tortuga_core::query::join_url(&request.url, &request.params);
    ui.horizontal_wrapped(|ui| {
        ui.label(theme::method_text(ui, request.method));
        ui.add(mono(&url));
    });
    ui.add_space(8.0);
    theme::section_label(ui, "Headers");
    let active: Vec<&KeyValue> = request.headers.iter().filter(|h| h.is_active()).collect();
    if active.is_empty() {
        theme::hint(ui, "No headers.");
    }
    for h in active {
        // Basic credentials are readable here (and secrets stay masked); they are base64 when sent.
        let value = h
            .name
            .eq_ignore_ascii_case("authorization")
            .then(|| tortuga_core::curl::basic_credentials(&h.value))
            .flatten()
            .map_or_else(|| h.value.clone(), |user| format!("Basic {user}"));
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new(format!("{}:", h.name))
                    .monospace()
                    .color(p.muted),
            );
            ui.add(mono(&value));
        });
    }
    let body = match &request.body {
        RequestBody::None => None,
        RequestBody::Text(t) | RequestBody::Json(t) | RequestBody::Xml(t) => Some(t.clone()),
        RequestBody::Form(fields) => Some(tortuga_core::query::encode_form(fields)),
        RequestBody::Multipart(parts) => Some(
            parts
                .iter()
                .filter(|p| p.is_active())
                .map(|p| {
                    let ct = p
                        .content_type
                        .as_deref()
                        .map(|c| format!(" ({c})"))
                        .unwrap_or_default();
                    if p.file {
                        format!("{}: file {}{ct}", p.name, p.value)
                    } else {
                        format!("{}: {}{ct}", p.name, p.value)
                    }
                })
                .collect::<Vec<_>>()
                .join("\n"),
        ),
    };
    if let Some(body) = body {
        ui.add_space(8.0);
        theme::section_label(ui, "Body");
        let (shown, cut) = crate::format::truncate(&body, PREVIEW_BODY_BYTES);
        ui.add(mono(shown));
        if cut {
            theme::hint(ui, "The body is longer; only the start is shown.");
        }
    }
    ui.add_space(8.0);
    theme::hint(
        ui,
        "Variables, server and path variables filled in; secret values shown as <secret:name>. \
         Basic credentials are base64-encoded when sent. Cookies from the jar are added when sending.",
    );
}

/// Longest body shown in the preview.
const PREVIEW_BODY_BYTES: usize = 64 * 1024;

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
                        undefined: &[],
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

    /// R-HTTP-14: the Preview tab shows the resolved request with Basic credentials readable.
    #[test]
    fn preview_tab_shows_the_request_as_sent() {
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
                    undefined: &[],
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
                },
            );
        });
        h.run();
        h.get_by_label("Preview").click();
        h.run();
        h.get_by_label("https://api.test/users?page=2");
        h.get_by_label("Basic alice:<secret:pw>");
        h.get_by_label("{\"a\": 1}");
    }

    /// R-TEST-1, R-TEST-4: scripts and captures are edited in the Scripts tab.
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
                        undefined: &[],
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
                    },
                );
            });
            h.run();
            let fields: Vec<_> = h
                .get_all_by_role(egui::accesskit::Role::MultilineTextInput)
                .collect();
            assert_eq!(fields.len(), 2, "pre-request and tests");
            fields[1].focus();
            fields[1].type_text("test('ok', () => {})");
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
                        undefined: &[],
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
                    },
                );
            });
            h.run();
            let buttons: Vec<_> = h.get_all_by_label("Beautify").collect();
            assert_eq!(buttons.len(), 2, "pre-request and tests");
            buttons[1].click();
            h.run();
        }
        assert_eq!(
            request.scripts.tests,
            "test('ok', () => {\n  expect(res.status).toBe(200);\n});"
        );
        assert_eq!(request.scripts.pre_request, "");
    }

    #[test]
    fn timeouts_must_be_positive_numbers() {
        assert_eq!(parse_timeout(" 500 "), Some(500));
        assert_eq!(parse_timeout("0"), None);
        assert_eq!(parse_timeout(""), None);
        assert_eq!(parse_timeout("abc"), None);
    }
}
