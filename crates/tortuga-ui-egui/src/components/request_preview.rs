//! The request as it will be sent (R-HTTP-14), opened from the line under the URL
//! (R-COL-24): that line shows the server's base URL, if any, and a ⏵ that opens the
//! preview right there, above the tabs.

use egui::{Button, Key, Label, RichText, Ui};
use tortuga_core::{KeyValue, Request, RequestBody};

use crate::theme;

/// Closes the preview (R-COL-24), also listed on the shortcut sheet (R-COL-27).
pub(crate) const CLOSE_PREVIEW: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::NONE, Key::Escape);

/// What the line under the URL says about the request's server.
#[derive(Debug, Clone, Copy)]
pub(super) enum ServerLine<'a> {
    /// No server: the line only offers the preview.
    None,
    /// The server's base URL in the active environment.
    Base(&'a str),
    /// The server has no URL in the active environment: why, as a warning.
    Missing(&'a str),
}

/// The line under the URL: ⏵ or ⏷, the server's base URL, and "Preview". Clicking it
/// opens or closes the preview; Escape closes it when nothing else wants the key.
pub(super) fn preview_line(ui: &mut Ui, open: &mut bool, server: ServerLine<'_>) {
    let p = theme::palette(ui);
    let mut atoms: Vec<egui::Atom<'_>> = vec![
        RichText::new(if *open { "⏷" } else { "⏵" })
            .small()
            .color(p.muted)
            .into(),
    ];
    match server {
        ServerLine::None => {}
        ServerLine::Base(base) => {
            atoms.push(RichText::new(base).small().monospace().weak().into());
        }
        ServerLine::Missing(why) => {
            atoms.push(
                RichText::new(why)
                    .small()
                    .monospace()
                    .color(p.warning_text)
                    .into(),
            );
        }
    }
    atoms.push(RichText::new(t!("preview")).small().color(p.muted).into());
    let atoms: egui::Atoms<'_> = atoms.into_iter().collect();
    let hint = if *open {
        t!("preview-hide")
    } else {
        t!("preview-show")
    };
    if ui
        .add(Button::new(atoms).frame(false).gap(6.0))
        .on_hover_text(hint)
        .clicked()
    {
        *open = !*open;
    }
    let escape = *open
        && ui.memory(|m| m.focused().is_none() && m.top_modal_layer().is_none())
        && ui.input_mut(|i| i.consume_key(CLOSE_PREVIEW.modifiers, CLOSE_PREVIEW.logical_key));
    if escape {
        *open = false;
    }
}

/// The preview under the line, at most a third of the window tall (R-COL-24).
pub(super) fn preview_panel(ui: &mut Ui, preview: Option<&Result<Request, String>>) {
    let max_height = ui.ctx().content_rect().height() / 3.0;
    theme::card(ui).show(ui, |ui| {
        egui::ScrollArea::vertical()
            .id_salt("request-preview")
            .max_height(max_height)
            .auto_shrink([false, true])
            .show(ui, |ui| request_preview(ui, preview));
    });
    ui.add_space(6.0);
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
    theme::section_label(ui, &t!("headers"));
    let active: Vec<&KeyValue> = request.headers.iter().filter(|h| h.is_active()).collect();
    if active.is_empty() {
        theme::hint(ui, t!("preview-no-headers"));
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
                        t!(
                            "preview-form-file",
                            name = &p.name,
                            file = &p.value,
                            content_type = ct.as_str()
                        )
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
        theme::section_label(ui, &t!("body"));
        let (shown, cut) = crate::format::truncate(&body, PREVIEW_BODY_BYTES);
        ui.add(mono(shown));
        if cut {
            theme::hint(ui, t!("preview-body-cut"));
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
