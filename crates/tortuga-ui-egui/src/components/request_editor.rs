use egui::{Color32, ComboBox, Key, RichText, Stroke, TextEdit, Ui};
use tortuga_core::{Auth, HttpMethod, KeyValue, Request, RequestBody};

use super::{BodyDrafts, body_editor, key_value_table, request_auth};
use crate::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Params,
    Headers,
    Auth,
    Body,
    Options,
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
}

impl EditorState {
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
    pub default_timeout_ms: u64,
    /// The collection's default auth; `None` for the scratch request (R-HTTP-5).
    pub collection_auth: Option<&'a Auth>,
    /// Collection headers this request inherits (R-COL-4), shown read-only.
    pub inherited_headers: &'a [KeyValue],
    /// Cookies in the jar this request uses (R-CK-1).
    pub cookie_count: usize,
    /// The request belongs to a collection (its jar is saved); `false` for the scratch request.
    pub in_collection: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorAction {
    Send,
    Cancel,
    /// Open the collection's default headers and auth.
    EditCollectionDefaults,
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
                "Undefined: {} — define them in the project's environments.",
                names.join(", ")
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
        ],
    );

    let mut action = action;
    egui::ScrollArea::vertical()
        .id_salt("request-tab")
        .auto_shrink([false, true])
        .show(ui, |ui| match st.tab {
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
                    if ui.small_button("Edit project defaults…").clicked() {
                        action = Some(EditorAction::EditCollectionDefaults);
                    }
                }
                theme::hint(
                    ui,
                    "Content-Type is set automatically from the body type unless you add it here.",
                );
            }
            Tab::Auth => {
                if request_auth(ui, &mut request.auth, cx.collection_auth, cx.undefined) {
                    action = Some(EditorAction::EditCollectionDefaults);
                }
            }
            Tab::Body => body_editor(ui, request, &mut st.body, cx.undefined),
            Tab::Options => request_options(ui, request, st, cx),
        });

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

/// Collection headers this request inherits (R-COL-4), read-only.
fn inherited_headers(ui: &mut Ui, headers: &[KeyValue]) {
    let p = theme::palette(ui);
    theme::section_label(ui, "From the project");
    egui::Grid::new("inherited-headers")
        .num_columns(2)
        .spacing([24.0, 4.0])
        .show(ui, |ui| {
            for h in headers {
                ui.label(RichText::new(&h.name).monospace().color(p.muted));
                ui.label(RichText::new(&h.value).monospace().color(p.muted));
                ui.end_row();
            }
        });
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
    let url = ui.add(url);
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

/// Per-request options: timeout (R-HTTP-7).
fn request_options(
    ui: &mut Ui,
    request: &mut Request,
    st: &mut EditorState,
    cx: EditorContext<'_>,
) {
    let default_ms = cx.default_timeout_ms;
    ui.horizontal(|ui| {
        ui.label("Timeout (ms)");
        let edit = TextEdit::singleline(&mut st.timeout_text)
            .id_salt("timeout")
            .hint_text(format!("{default_ms} (default)"))
            .desired_width(140.0);
        if ui.add(edit).changed() {
            request.timeout_ms = parse_timeout(&st.timeout_text);
        }
    });
    theme::hint(ui, "Leave empty to use the default from Settings.");

    // R-CK-2: per request, saved in the request file.
    ui.add_space(12.0);
    let jar = if cx.in_collection {
        "this project's cookie jar"
    } else {
        "the scratch cookie jar (kept until the app closes)"
    };
    ui.checkbox(&mut request.cookies, format!("Use cookies from {jar}"));
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
                        default_timeout_ms: 1000,
                        collection_auth: None,
                        inherited_headers: &[],
                        cookie_count: 0,
                        in_collection: false,
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

    #[test]
    fn timeouts_must_be_positive_numbers() {
        assert_eq!(parse_timeout(" 500 "), Some(500));
        assert_eq!(parse_timeout("0"), None);
        assert_eq!(parse_timeout(""), None);
        assert_eq!(parse_timeout("abc"), None);
    }
}
