use egui::{TextEdit, Ui};
use tortuga_core::{KeyValue, Request, RequestBody, json};

use super::key_value_table;
use crate::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    None,
    Text,
    Json,
    Form,
}

impl Kind {
    const ALL: [(Self, &'static str); 4] = [
        (Self::None, "None"),
        (Self::Text, "Text"),
        (Self::Json, "JSON"),
        (Self::Form, "Form URL-encoded"),
    ];

    fn of(body: &RequestBody) -> Self {
        match body {
            RequestBody::None => Self::None,
            RequestBody::Text(_) => Self::Text,
            RequestBody::Json(_) => Self::Json,
            RequestBody::Form(_) => Self::Form,
        }
    }
}

/// What was typed for each body type, so switching type loses nothing (R-HTTP-4).
#[derive(Debug, Clone, Default)]
pub struct BodyDrafts {
    text: String,
    form: Vec<KeyValue>,
}

impl BodyDrafts {
    pub fn new(body: &RequestBody) -> Self {
        match body {
            RequestBody::Text(t) | RequestBody::Json(t) => Self {
                text: t.clone(),
                ..Self::default()
            },
            RequestBody::Form(f) => Self {
                form: f.clone(),
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
            Kind::Form => RequestBody::Form(self.form.clone()),
        }
    }
}

/// Body type picker and editor (R-HTTP-4).
pub fn body_editor(
    ui: &mut Ui,
    request: &mut Request,
    drafts: &mut BodyDrafts,
    undefined: &[String],
) {
    let kind = Kind::of(&request.body);
    let json_error = if kind == Kind::Json {
        json::check(&drafts.text)
    } else {
        None
    };

    ui.horizontal(|ui| {
        let mut chosen = kind;
        for (k, label) in Kind::ALL {
            ui.radio_value(&mut chosen, k, label);
        }
        if chosen != kind {
            request.body = drafts.body(chosen);
        }
        if kind == Kind::Json
            && ui
                .add_enabled(json_error.is_none(), egui::Button::new("Format").small())
                .clicked()
            && let Some(pretty) = json::pretty(&drafts.text)
        {
            drafts.text = pretty;
            request.body = drafts.body(Kind::Json);
        }
    });

    match kind {
        Kind::None => theme::hint(ui, "This request has no body."),
        Kind::Text | Kind::Json => {
            let flagged = theme::uses_undefined(&drafts.text, undefined);
            let mut edit = TextEdit::multiline(&mut drafts.text)
                .id_salt("body-text")
                .code_editor()
                .desired_width(f32::INFINITY)
                .desired_rows(10)
                .hint_text(if kind == Kind::Json {
                    "{\n  \"key\": \"value\"\n}"
                } else {
                    "Request body"
                });
            if flagged {
                edit = edit.background_color(theme::undefined_bg(ui));
            }
            if ui.add(edit).changed() {
                request.body = drafts.body(kind);
            }
            if let Some(err) = json_error {
                theme::error(ui, err);
            }
        }
        Kind::Form => {
            if key_value_table(ui, "body-form", &mut drafts.form, "Field", undefined) {
                request.body = drafts.body(Kind::Form);
            }
        }
    }
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
    }
}
