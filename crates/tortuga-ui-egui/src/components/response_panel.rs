use std::ops::Range;
use std::sync::Arc;

use egui::{Grid, Label, RichText, ScrollArea, TextStyle, TextWrapMode, Ui};
use tortuga_core::{BodyView, Response};

use crate::{format, theme};

/// Longest piece of a line drawn as one row; longer lines are split so a
/// minified multi-megabyte body never becomes one giant text layout.
const MAX_ROW_CHARS: usize = 2000;

/// What the response panel shows after a send.
#[derive(Debug, Clone)]
pub enum Outcome {
    /// Shared so keeping it around never copies large bodies.
    Response(Arc<ResponseView>),
    Error(String),
    Cancelled,
}

impl Outcome {
    /// Prepares a result for display. Can be slow for big bodies (pretty-printing,
    /// splitting into rows), so call it off the UI thread (R-NF-2).
    pub fn from_result(result: tortuga_core::Result<Response>) -> Self {
        match result {
            Ok(response) => Self::Response(Arc::new(ResponseView::new(response))),
            Err(e) => Self::Error(format::sentence(&e.to_string())),
        }
    }
}

/// A response with its body ready to draw.
#[derive(Debug)]
pub struct ResponseView {
    pub response: Response,
    body: Body,
}

#[derive(Debug)]
enum Body {
    Empty,
    Binary,
    Text {
        text: String,
        /// Byte ranges of the rows to draw.
        rows: Vec<Range<usize>>,
        truncated: bool,
    },
}

impl ResponseView {
    pub fn new(response: Response) -> Self {
        // Pretty-printing huge bodies would take too long (R-NF-2).
        let view = if response.body.len() > format::MAX_DISPLAY_BYTES {
            match std::str::from_utf8(&response.body) {
                Ok(text) => BodyView::Text(text.to_owned()),
                Err(_) => BodyView::Binary,
            }
        } else {
            response.body_view()
        };
        let body = match view {
            BodyView::Empty => Body::Empty,
            BodyView::Binary => Body::Binary,
            BodyView::Json(text) | BodyView::Text(text) => {
                let (shown, truncated) = format::truncate(&text, format::MAX_DISPLAY_BYTES);
                let rows = rows(shown);
                let text = if truncated { shown.to_owned() } else { text };
                Body::Text {
                    text,
                    rows,
                    truncated,
                }
            }
        };
        Self { response, body }
    }
}

/// Splits `text` into lines, and lines longer than [`MAX_ROW_CHARS`] into pieces.
fn rows(text: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut start = 0;
    loop {
        let newline = text[start..].find('\n').map(|i| start + i);
        let line = &text[start..newline.unwrap_or(text.len())];
        let line = line.strip_suffix('\r').unwrap_or(line);
        let mut from = start;
        for (chars, (i, _)) in line.char_indices().enumerate() {
            if chars > 0 && chars % MAX_ROW_CHARS == 0 {
                out.push(from..start + i);
                from = start + i;
            }
        }
        out.push(from..start + line.len());
        match newline {
            Some(nl) => start = nl + 1,
            None => return out,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Tab {
    #[default]
    Body,
    Headers,
}

/// Panel state that is not part of the response; reset for each new response.
#[derive(Debug, Clone, Default)]
pub struct ResponseState {
    tab: Tab,
    /// Result of the last "Save to file…".
    pub save_status: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseAction {
    SaveBody,
}

/// Response status, timing, size, headers and body (R-HTTP-6).
pub fn response_panel(
    ui: &mut Ui,
    outcome: Option<&Outcome>,
    sending: bool,
    st: &mut ResponseState,
    can_save: bool,
) -> Option<ResponseAction> {
    let p = theme::palette(ui);
    if sending {
        empty_state(ui, |ui| {
            ui.spinner();
            ui.label(RichText::new("Sending request…").color(p.muted));
        });
        return None;
    }
    match outcome {
        None => {
            empty_state(ui, |ui| {
                ui.label(RichText::new("⏵").size(28.0).color(p.border));
                ui.label(theme::semibold("No response yet").color(p.muted));
                ui.label(
                    RichText::new("Send a request to see the response here.")
                        .small()
                        .color(p.muted),
                );
                ui.label(
                    RichText::new("Ctrl+Enter sends, Ctrl+S saves")
                        .small()
                        .color(p.muted.gamma_multiply(0.7)),
                );
            });
            None
        }
        Some(Outcome::Cancelled) => {
            empty_state(ui, |ui| {
                ui.label(theme::semibold("Request cancelled.").color(p.muted));
            });
            None
        }
        Some(Outcome::Error(message)) => {
            theme::banner_frame(ui, true)
                .corner_radius(egui::CornerRadius::same(8))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(theme::semibold("Could not get a response").color(p.fail));
                    ui.add(Label::new(RichText::new(message).color(p.fail)).wrap());
                });
            None
        }
        Some(Outcome::Response(view)) => details(ui, view, st, can_save),
    }
}

/// Centred content for the empty and in-progress states.
fn empty_state(ui: &mut Ui, content: impl FnOnce(&mut Ui)) {
    let height = ui.available_height();
    ui.vertical_centered(|ui| {
        ui.add_space((height / 2.0 - 50.0).max(8.0));
        content(ui);
    });
}

fn details(
    ui: &mut Ui,
    view: &ResponseView,
    st: &mut ResponseState,
    can_save: bool,
) -> Option<ResponseAction> {
    let response = &view.response;
    let mut action = None;
    let p = theme::palette(ui);
    let status_color = match response.status {
        200..=299 => p.ok,
        300..=399 => p.redirect,
        _ => p.fail,
    };

    egui::Sides::new().shrink_left().show(
        ui,
        |ui| {
            theme::section_label(ui, "Response");
            ui.add_space(4.0);
            theme::badge(
                ui,
                format!("{} {}", response.status, response.status_text),
                status_color,
            );
            ui.label(RichText::new(format::elapsed(response.elapsed)).color(p.muted))
                .on_hover_text("Time");
            ui.label(RichText::new("·").color(p.muted));
            ui.label(RichText::new(format::size(response.body.len())).color(p.muted))
                .on_hover_text("Size");
        },
        |ui| {
            if can_save && ui.button("Save to file…").clicked() {
                action = Some(ResponseAction::SaveBody);
            }
            if let Some(msg) = &st.save_status {
                ui.add(Label::new(RichText::new(msg).small().color(p.muted)).truncate())
                    .on_hover_text(msg);
            }
        },
    );
    ui.add_space(6.0);
    let header_count = response.headers.len().to_string();
    theme::tabs(
        ui,
        &mut st.tab,
        &[
            (Tab::Body, "Body", None),
            (Tab::Headers, "Headers", Some(header_count)),
        ],
    );

    theme::card(ui).show(ui, |ui| match st.tab {
        Tab::Body => body(ui, &view.body),
        Tab::Headers => {
            ScrollArea::vertical()
                .id_salt("response-headers")
                .auto_shrink(false)
                .show(ui, |ui| {
                    Grid::new("response-headers-grid")
                        .num_columns(2)
                        .spacing([24.0, 8.0])
                        .show(ui, |ui| {
                            for h in &response.headers {
                                ui.label(RichText::new(&h.name).monospace().color(p.muted));
                                ui.add(Label::new(RichText::new(&h.value).monospace()).wrap());
                                ui.end_row();
                            }
                        });
                });
        }
    });
    action
}

fn body(ui: &mut Ui, body: &Body) {
    let (text, rows, truncated) = match body {
        Body::Empty => return theme::hint(ui, "No body."),
        Body::Binary => return theme::hint(ui, "Binary content. Use “Save to file” to keep it."),
        Body::Text {
            text,
            rows,
            truncated,
        } => (text, rows, *truncated),
    };
    if truncated {
        let limit = format::size(format::MAX_DISPLAY_BYTES);
        theme::hint(
            ui,
            format!("Showing the first {limit}. Use “Save to file” for the full body."),
        );
    }
    // Only the visible rows are laid out, so big bodies scroll smoothly (R-NF-2).
    let row_height = ui.text_style_height(&TextStyle::Monospace);
    ui.spacing_mut().item_spacing.y = 3.0;
    ScrollArea::both()
        .id_salt("response-body")
        .auto_shrink(false)
        .show_rows(ui, row_height, rows.len(), |ui, visible| {
            for range in &rows[visible] {
                ui.add(
                    Label::new(RichText::new(&text[range.clone()]).monospace())
                        .wrap_mode(TextWrapMode::Extend),
                );
            }
        });
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;
    use tortuga_core::KeyValue;

    use super::*;

    fn json_response() -> Response {
        Response {
            status: 201,
            status_text: "Created".into(),
            headers: vec![KeyValue::new("content-type", "application/json")],
            body: br#"{"id":7}"#.to_vec(),
            elapsed: Duration::from_millis(12),
        }
    }

    fn render(outcome: Option<Outcome>) -> Harness<'static> {
        let mut st = ResponseState::default();
        let mut harness = crate::test_host::harness(move |ui| {
            response_panel(ui, outcome.as_ref(), false, &mut st, true);
        });
        harness.run();
        harness
    }

    #[test]
    fn shows_status_time_size_and_pretty_json() {
        let h = render(Some(Outcome::from_result(Ok(json_response()))));
        h.get_by_label("201 Created");
        h.get_by_label("12 ms");
        h.get_by_label("8 B");
        h.get_by_label("Headers (1)");
        h.get_by_label("  \"id\": 7");
    }

    #[test]
    fn shows_errors_and_cancellation() {
        render(Some(Outcome::Error("Connection refused".into())))
            .get_by_label("Connection refused");
        render(Some(Outcome::Cancelled)).get_by_label("Request cancelled.");
        render(None).get_by_label("Send a request to see the response here.");
    }

    #[test]
    fn huge_bodies_are_truncated_and_not_pretty_printed() {
        let mut res = json_response();
        res.body = format!("[{}]", "1,".repeat(format::MAX_DISPLAY_BYTES)).into_bytes();
        let outcome = Outcome::from_result(Ok(res));
        let Outcome::Response(view) = &outcome else {
            panic!("expected response")
        };
        let Body::Text {
            rows, truncated, ..
        } = &view.body
        else {
            panic!("expected text")
        };
        assert!(truncated);
        assert!(rows.len() > 1, "a single long line is split into rows");
        render(Some(outcome)).get_by_label_contains("Showing the first 2.0 MB");
    }

    #[test]
    fn rows_split_lines_and_long_lines() {
        let text = "ab\r\ncd\n\nef";
        let r: Vec<&str> = rows(text).into_iter().map(|r| &text[r]).collect();
        assert_eq!(r, ["ab", "cd", "", "ef"]);

        let long = "é".repeat(MAX_ROW_CHARS + 1);
        let r: Vec<&str> = rows(&long).into_iter().map(|r| &long[r]).collect();
        assert_eq!(r.len(), 2);
        assert_eq!(r[1], "é");
    }
}
