use egui::{ComboBox, Grid, TextEdit, Ui};
use tortuga_core::{FormPart, KeyValue, Request, RequestBody, json, xml};

use super::key_value_table;
use crate::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    None,
    Text,
    Json,
    Xml,
    Form,
    Multipart,
}

impl Kind {
    const ALL: [(Self, &'static str); 6] = [
        (Self::None, "None"),
        (Self::Text, "Text"),
        (Self::Json, "JSON"),
        (Self::Xml, "XML"),
        (Self::Form, "Form URL-encoded"),
        (Self::Multipart, "Multipart"),
    ];

    fn of(body: &RequestBody) -> Self {
        match body {
            RequestBody::None => Self::None,
            RequestBody::Text(_) => Self::Text,
            RequestBody::Json(_) => Self::Json,
            RequestBody::Xml(_) => Self::Xml,
            RequestBody::Form(_) => Self::Form,
            RequestBody::Multipart(_) => Self::Multipart,
        }
    }

    /// What is wrong with `text`, for the kinds that are checked while typing.
    fn check(self, text: &str) -> Option<String> {
        match self {
            Self::Json => json::check(text),
            Self::Xml => xml::check(text),
            Self::None | Self::Text | Self::Form | Self::Multipart => None,
        }
    }

    /// `text` pretty-printed, for the kinds that have a "Beautify" button.
    fn pretty(self, text: &str) -> Option<String> {
        match self {
            Self::Json => json::pretty(text),
            Self::Xml => xml::pretty(text),
            Self::None | Self::Text | Self::Form | Self::Multipart => None,
        }
    }

    fn formats(self) -> bool {
        matches!(self, Self::Json | Self::Xml)
    }
}

/// What was typed for each body type, so switching type loses nothing (R-HTTP-4).
#[derive(Debug, Clone, Default)]
pub struct BodyDrafts {
    text: String,
    form: Vec<KeyValue>,
    parts: Vec<FormPart>,
}

impl BodyDrafts {
    pub fn new(body: &RequestBody) -> Self {
        match body {
            RequestBody::Text(t) | RequestBody::Json(t) | RequestBody::Xml(t) => Self {
                text: t.clone(),
                ..Self::default()
            },
            RequestBody::Form(f) => Self {
                form: f.clone(),
                ..Self::default()
            },
            RequestBody::Multipart(p) => Self {
                parts: p.clone(),
                ..Self::default()
            },
            RequestBody::None => Self::default(),
        }
    }

    fn body(&self, kind: Kind) -> RequestBody {
        match kind {
            Kind::None => RequestBody::None,
            Kind::Text => RequestBody::Text(self.text.clone()),
            Kind::Json => RequestBody::Json(self.text.clone()),
            Kind::Xml => RequestBody::Xml(self.text.clone()),
            Kind::Form => RequestBody::Form(self.form.clone()),
            Kind::Multipart => RequestBody::Multipart(self.parts.clone()),
        }
    }

    /// Sets the file of multipart part `index`, chosen with "Choose…" (R-HTTP-15).
    pub fn set_file(&mut self, request: &mut Request, index: usize, path: String) {
        if let Some(part) = self.parts.get_mut(index) {
            part.value = path;
            part.file = true;
            request.body = self.body(Kind::Multipart);
        }
    }
}

/// Body type picker and editor (R-HTTP-4, R-HTTP-15, R-HTTP-19). Returns the multipart part whose
/// "Choose…" was clicked.
pub fn body_editor(
    ui: &mut Ui,
    request: &mut Request,
    drafts: &mut BodyDrafts,
    undefined: &[String],
    can_pick_files: bool,
) -> Option<usize> {
    let kind = Kind::of(&request.body);
    let error = kind.check(&drafts.text);

    // The body types on the left, "Beautify" at the right edge, as above scripts (R-TEST-7).
    let (chosen, beautify) = egui::Sides::new().shrink_left().show(
        ui,
        |ui| {
            let mut chosen = kind;
            for (k, label) in Kind::ALL {
                ui.radio_value(&mut chosen, k, label);
            }
            chosen
        },
        |ui| {
            kind.formats()
                && ui
                    .add_enabled(error.is_none(), egui::Button::new("Beautify").small())
                    .on_hover_text("Indent the body; it must be valid first")
                    .clicked()
        },
    );
    if chosen != kind {
        request.body = drafts.body(chosen);
    } else if beautify && let Some(pretty) = kind.pretty(&drafts.text) {
        drafts.text = pretty;
        request.body = drafts.body(kind);
    }

    match kind {
        Kind::None => theme::hint(ui, "This request has no body."),
        Kind::Text | Kind::Json | Kind::Xml => {
            let flagged = theme::uses_undefined(&drafts.text, undefined);
            let mut edit = TextEdit::multiline(&mut drafts.text)
                .id_salt("body-text")
                .code_editor()
                .desired_width(f32::INFINITY)
                .desired_rows(10)
                .hint_text(match kind {
                    Kind::Json => "{\n  \"key\": \"value\"\n}",
                    Kind::Xml => "<order>\n  <id>{{id}}</id>\n</order>",
                    Kind::None | Kind::Text | Kind::Form | Kind::Multipart => "Request body",
                });
            if flagged {
                edit = edit.background_color(theme::undefined_bg(ui));
            }
            if ui.add(edit).changed() {
                request.body = drafts.body(kind);
            }
            if let Some(err) = error {
                theme::error(ui, err);
            }
        }
        Kind::Form => {
            if key_value_table(ui, "body-form", &mut drafts.form, "Field", undefined) {
                request.body = drafts.body(Kind::Form);
            }
        }
        Kind::Multipart => {
            let (changed, choose) = parts_table(ui, &mut drafts.parts, undefined, can_pick_files);
            if changed {
                request.body = drafts.body(Kind::Multipart);
            }
            multipart_hints(ui, &drafts.parts);
            return choose;
        }
    }
    None
}

/// Notes under the parts: where relative paths start, and full paths (D21).
fn multipart_hints(ui: &mut Ui, parts: &[FormPart]) {
    let full_paths = parts
        .iter()
        .any(|p| p.enabled && p.file && std::path::Path::new(p.value.trim()).is_absolute());
    if full_paths {
        theme::warning(
            ui,
            "A file outside the project keeps its full path: the request only works on this computer.",
        );
    }
    theme::hint(
        ui,
        "Files are read when sending. A file inside the project is saved relative to the project \
         folder, so teammates can send the request too. The Content-Type with the boundary is set \
         automatically.",
    );
}

/// The parts of a multipart body, with a blank row at the end that becomes a part when
/// typed in. Returns (changed, part whose "Choose…" was clicked).
fn parts_table(
    ui: &mut Ui,
    parts: &mut Vec<FormPart>,
    undefined: &[String],
    can_pick_files: bool,
) -> (bool, Option<usize>) {
    let mut changed = false;
    let mut remove = None;
    let mut choose = None;
    let mut blank = FormPart::text("", "");
    let count = parts.len();
    // Toggle, kind, choose, remove and spacing take ~290 px; the text columns share the rest.
    let rest = (ui.available_width() - 290.0).max(240.0);
    let widths = [rest * 0.28, rest * 0.47, rest * 0.25];
    Grid::new("body-multipart")
        .num_columns(7)
        .spacing([6.0, 4.0])
        .show(ui, |ui| {
            ui.label("");
            theme::section_label(ui, "Part");
            ui.label("");
            theme::section_label(ui, "Value or file");
            ui.label("");
            theme::section_label(ui, "Content type");
            ui.label("");
            ui.end_row();
            for (i, part) in parts
                .iter_mut()
                .chain(std::iter::once(&mut blank))
                .enumerate()
            {
                let is_blank = i == count;
                if is_blank {
                    ui.label("");
                } else {
                    changed |= ui
                        .checkbox(&mut part.enabled, "")
                        .on_hover_text("Include")
                        .changed();
                }
                let name_hint = if is_blank { "Add part" } else { "Part" };
                changed |= cell(
                    ui,
                    ("part-name", i),
                    &mut part.name,
                    name_hint,
                    undefined,
                    widths[0],
                );
                changed |= kind_picker(ui, i, part);
                let value_hint = if part.file { "path/to/file" } else { "Value" };
                changed |= cell(
                    ui,
                    ("part-value", i),
                    &mut part.value,
                    value_hint,
                    undefined,
                    widths[1],
                );
                if part.file && can_pick_files && !is_blank {
                    if ui
                        .small_button("Choose…")
                        .on_hover_text("Pick the file to upload")
                        .clicked()
                    {
                        choose = Some(i);
                    }
                } else {
                    ui.label("");
                }
                let mut content_type = part.content_type.clone().unwrap_or_default();
                if cell(
                    ui,
                    ("part-type", i),
                    &mut content_type,
                    "auto",
                    undefined,
                    widths[2],
                ) {
                    part.content_type = Some(content_type).filter(|c| !c.trim().is_empty());
                    changed = true;
                }
                if !is_blank
                    && ui
                        .add(theme::icon_button(ui, "🗙"))
                        .on_hover_text("Remove")
                        .clicked()
                {
                    remove = Some(i);
                }
                ui.end_row();
            }
        });
    if blank != FormPart::text("", "") {
        parts.push(blank);
        changed = true;
    }
    if let Some(i) = remove {
        parts.remove(i);
        changed = true;
    }
    (changed, choose)
}

/// Text or File.
fn kind_picker(ui: &mut Ui, i: usize, part: &mut FormPart) -> bool {
    let before = part.file;
    ComboBox::from_id_salt(("part-kind", i))
        .selected_text(if part.file { "File" } else { "Text" })
        .width(60.0)
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut part.file, false, "Text");
            ui.selectable_value(&mut part.file, true, "File");
        })
        .response
        .on_hover_text("A text value, or a file to upload");
    part.file != before
}

fn cell(
    ui: &mut Ui,
    id_salt: (&str, usize),
    text: &mut String,
    hint: &str,
    undefined: &[String],
    width: f32,
) -> bool {
    let flagged = theme::uses_undefined(text, undefined);
    let mut edit = TextEdit::singleline(text)
        .id_salt(id_salt)
        .hint_text(hint.to_owned());
    if flagged {
        edit = edit.background_color(theme::undefined_bg(ui));
    }
    theme::sized_edit(ui, width, edit).changed()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switching_type_keeps_what_was_typed() {
        let mut drafts = BodyDrafts::new(&RequestBody::Json("{}".into()));
        drafts.form = vec![KeyValue::new("a", "1")];
        assert_eq!(drafts.body(Kind::Text), RequestBody::Text("{}".into()));
        assert_eq!(
            drafts.body(Kind::Form),
            RequestBody::Form(vec![KeyValue::new("a", "1")])
        );
        assert_eq!(drafts.body(Kind::Json), RequestBody::Json("{}".into()));
        assert_eq!(drafts.body(Kind::Xml), RequestBody::Xml("{}".into()));
    }

    /// R-HTTP-19: XML is checked while typing and "Beautify" indents it.
    #[test]
    fn xml_bodies_are_checked_and_formatted() {
        use egui_kittest::kittest::Queryable;

        let mut request = Request {
            body: RequestBody::Xml("<a><b>ü</b>".into()),
            ..Request::default()
        };
        let mut drafts = BodyDrafts::new(&request.body);
        {
            let mut h = crate::test_host::harness(|ui| {
                body_editor(ui, &mut request, &mut drafts, &[], false);
            });
            h.run();
            assert!(
                h.query_by_label_contains("Invalid XML").is_some(),
                "the error is shown"
            );
        }
        drafts.text.push_str("</a>");
        request.body = drafts.body(Kind::Xml);
        {
            let mut h = crate::test_host::harness(|ui| {
                body_editor(ui, &mut request, &mut drafts, &[], false);
            });
            h.run();
            assert!(h.query_by_label_contains("Invalid XML").is_none());
            let beautify = h.get_by_label("Beautify");
            assert!(
                beautify.rect().left() > h.get_by_label("Multipart").rect().right() + 50.0,
                "Beautify sits at the right edge, apart from the body types"
            );
            beautify.click();
            h.run();
        }
        assert_eq!(
            request.body,
            RequestBody::Xml("<a>\n  <b>ü</b>\n</a>".into())
        );
    }

    /// R-HTTP-15: typing in the blank row adds a part; "Choose…" is reported for files.
    #[test]
    fn multipart_parts_are_added_and_files_chosen() {
        use egui_kittest::kittest::Queryable;

        let mut request = Request {
            body: RequestBody::Multipart(vec![FormPart::file("avatar", "")]),
            ..Request::default()
        };
        let mut drafts = BodyDrafts::new(&request.body);
        let mut chosen = None;
        {
            let mut h = crate::test_host::harness(|ui| {
                if let Some(i) = body_editor(ui, &mut request, &mut drafts, &[], true) {
                    chosen = Some(i);
                }
            });
            h.run();
            h.get_by_label("Choose…").click();
            h.run();
            // Empty fields in order: part 0's value and content type, then the blank row's name.
            let blank_name = h
                .get_all_by_role(egui::accesskit::Role::TextInput)
                .filter(|n| n.value().as_deref() == Some(""))
                .nth(2)
                .expect("blank row");
            blank_name.focus();
            blank_name.type_text("title");
            h.run();
        }
        assert_eq!(chosen, Some(0));
        let RequestBody::Multipart(parts) = &request.body else {
            panic!("multipart")
        };
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[1], FormPart::text("title", ""));

        drafts.set_file(&mut request, 0, "fixtures/me.png".into());
        let RequestBody::Multipart(parts) = &request.body else {
            panic!("multipart")
        };
        assert_eq!(parts[0], FormPart::file("avatar", "fixtures/me.png"));
    }
}
