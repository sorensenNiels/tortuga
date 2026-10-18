use std::ops::Range;
use std::sync::{Arc, OnceLock};

use egui::text::{LayoutJob, TextFormat};
use egui::{Grid, Key, Label, RichText, ScrollArea, TextEdit, TextStyle, TextWrapMode, Ui};
use tortuga_core::{BodyView, Response, jsonpath};

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
    /// The whole body parsed as JSON, on first use by the filter (R-HTTP-13).
    json: OnceLock<Option<serde_json::Value>>,
}

#[derive(Debug)]
enum Body {
    Empty,
    Binary,
    Text(TextBody),
}

/// Text split into rows, ready to draw.
#[derive(Debug)]
pub struct TextBody {
    text: String,
    /// Byte ranges of the rows to draw.
    rows: Vec<Range<usize>>,
    /// Only the start of the text is kept.
    truncated: bool,
    /// The response is JSON (so it can be filtered).
    json: bool,
}

impl TextBody {
    fn new(text: String, truncated: bool, json: bool) -> Self {
        Self {
            rows: rows(&text),
            text,
            truncated,
            json,
        }
    }
}

impl ResponseView {
    pub fn new(response: Response) -> Self {
        let body = if response.body.len() > format::MAX_DISPLAY_BYTES {
            // Pretty-printing huge bodies would take too long, and only the start is
            // shown: copy just that (R-NF-2).
            match text_start(&response.body, format::MAX_DISPLAY_BYTES) {
                Some(text) => Body::Text(TextBody::new(
                    text.to_owned(),
                    true,
                    looks_json(&response, text),
                )),
                None => Body::Binary,
            }
        } else {
            match response.body_view() {
                BodyView::Empty => Body::Empty,
                BodyView::Binary => Body::Binary,
                BodyView::Json(text) => Body::Text(shown(text, true)),
                BodyView::Text(text) => Body::Text(shown(text, false)),
            }
        };
        Self {
            response,
            body,
            json: OnceLock::new(),
        }
    }

    /// The whole body as JSON, parsed the first time. Slow for big bodies: call it off
    /// the UI thread.
    pub fn json(&self) -> Option<&serde_json::Value> {
        self.json
            .get_or_init(|| serde_json::from_slice(&self.response.body).ok())
            .as_ref()
    }
}

/// `text` cut to what the viewer shows. Pretty-printing can make it longer than the limit.
fn shown(text: String, json: bool) -> TextBody {
    let (start, truncated) = format::truncate(&text, format::MAX_DISPLAY_BYTES);
    let text = if truncated { start.to_owned() } else { text };
    TextBody::new(text, truncated, json)
}

fn looks_json(response: &Response, start: &str) -> bool {
    response
        .content_type()
        .is_some_and(|ct| ct.to_ascii_lowercase().contains("json"))
        || matches!(start.trim_start().as_bytes().first(), Some(b'{' | b'['))
}

/// Filters a JSON body with a `JSONPath` expression (R-HTTP-13). Parses the whole body the
/// first time, so call it off the UI thread.
pub fn filter_body(view: &ResponseView, expr: &str) -> Result<TextBody, String> {
    let value = view
        .json()
        .ok_or_else(|| "The body is not valid JSON.".to_owned())?;
    let found = jsonpath::query(expr, value).map_err(|e| format::sentence(&e))?;
    let text = serde_json::to_string_pretty(&found).map_err(|e| e.to_string())?;
    Ok(shown(text, true))
}

/// The first `max` bytes of `body` as text, cut on a character boundary.
/// `None` if they are not UTF-8.
fn text_start(body: &[u8], max: usize) -> Option<&str> {
    let head = &body[..max.min(body.len())];
    match std::str::from_utf8(head) {
        Ok(text) => Some(text),
        // Only the last character was cut in two.
        Err(e) if e.error_len().is_none() => std::str::from_utf8(&head[..e.valid_up_to()]).ok(),
        Err(_) => None,
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

/// Every match of `query` in `text`, ignoring case, as byte ranges (R-HTTP-12).
fn find_all(text: &str, query: &str) -> Vec<Range<usize>> {
    if query.is_empty() {
        return Vec::new();
    }
    let (haystack, needle) = (fold_case(text), fold_case(query));
    haystack
        .match_indices(&needle)
        .map(|(at, m)| at..at + m.len())
        .collect()
}

/// Lower case, but only where that keeps each character's length in bytes, so byte
/// positions in the result are positions in `text` too.
fn fold_case(text: &str) -> String {
    text.chars()
        .map(|c| {
            let mut lower = c.to_lowercase();
            match (lower.next(), lower.next()) {
                (Some(l), None) if l.len_utf8() == c.len_utf8() => l,
                _ => c,
            }
        })
        .collect()
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
    /// Search in the body (R-HTTP-12).
    search: String,
    /// Index of the current match.
    current: usize,
    matches: Vec<Range<usize>>,
    /// What `matches` were found for: (query, address and length of the text).
    matches_for: Option<(String, usize, usize)>,
    /// Ctrl+F: put the cursor in the search field.
    pub focus_search: bool,
    scroll_to_match: bool,
}

impl ResponseState {
    /// Finds the matches again if the query or the shown text changed.
    fn update_matches(&mut self, text: &str) {
        let key = (self.search.clone(), text.as_ptr() as usize, text.len());
        if self.matches_for.as_ref() != Some(&key) {
            self.matches = find_all(text, &self.search);
            self.current = 0;
            self.matches_for = Some(key);
            self.scroll_to_match = !self.matches.is_empty();
        }
    }

    fn step(&mut self, forward: bool) {
        let n = self.matches.len();
        if n > 0 {
            self.current = if forward {
                (self.current + 1) % n
            } else {
                (self.current + n - 1) % n
            };
            self.scroll_to_match = true;
        }
    }
}

/// The JSON filter's result for the shown response (R-HTTP-13).
#[derive(Debug, Clone, Copy)]
pub enum Filtered<'a> {
    /// No filter typed.
    Off,
    /// Being worked out.
    Pending,
    Done(&'a TextBody),
    Failed(&'a str),
}

/// What the response panel needs besides the response.
#[derive(Debug)]
pub struct PanelContext<'a> {
    pub sending: bool,
    pub can_save: bool,
    /// The JSON filter typed for this request.
    pub filter: &'a mut String,
    pub filtered: Filtered<'a>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseAction {
    SaveBody,
}

/// Response status, timing, size, headers and body (R-HTTP-6).
pub fn response_panel(
    ui: &mut Ui,
    outcome: Option<&Outcome>,
    st: &mut ResponseState,
    cx: PanelContext<'_>,
) -> Option<ResponseAction> {
    let p = theme::palette(ui);
    if cx.sending {
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
        Some(Outcome::Response(view)) => details(ui, view, st, cx),
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
    cx: PanelContext<'_>,
) -> Option<ResponseAction> {
    let PanelContext {
        can_save,
        filter,
        filtered,
        ..
    } = cx;
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
    if st.focus_search {
        st.tab = Tab::Body;
    }
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
        Tab::Body => body(ui, &view.body, st, filter, filtered),
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

fn body(
    ui: &mut Ui,
    body: &Body,
    st: &mut ResponseState,
    filter: &mut String,
    filtered: Filtered<'_>,
) {
    let full = match body {
        Body::Empty => return theme::hint(ui, "No body."),
        Body::Binary => return theme::hint(ui, "Binary content. Use “Save to file” to keep it."),
        Body::Text(text) => text,
    };
    let shown = match filtered {
        Filtered::Off | Filtered::Failed(_) => Some(full),
        Filtered::Done(filtered) => Some(filtered),
        Filtered::Pending => None,
    };
    // Before the toolbar, so its count is up to date.
    if let Some(shown) = shown {
        st.update_matches(&shown.text);
    }
    toolbar(ui, st, full.json.then_some(filter));
    if let Filtered::Failed(message) = filtered {
        theme::warning(ui, message);
    }
    let Some(shown) = shown else {
        ui.horizontal(|ui| {
            ui.spinner();
            theme::hint(ui, "Filtering…");
        });
        return;
    };
    if shown.truncated {
        let limit = format::size(format::MAX_DISPLAY_BYTES);
        theme::hint(
            ui,
            format!("Showing the first {limit}. Use “Save to file” for the full body."),
        );
    }
    text_rows(ui, shown, st);
}

/// The JSON filter (for JSON bodies) and the search field, above the body.
fn toolbar(ui: &mut Ui, st: &mut ResponseState, filter: Option<&mut String>) {
    let p = theme::palette(ui);
    egui::Sides::new().shrink_left().show(
        ui,
        |ui| {
            if let Some(filter) = filter {
                ui.label(RichText::new("Filter").small().color(p.muted));
                let edit = TextEdit::singleline(filter)
                    .id_salt("json-filter")
                    .font(TextStyle::Monospace)
                    .hint_text("$.items[*].id")
                    .desired_width(ui.available_width().min(320.0));
                ui.add(edit).on_hover_text(
                    "JSONPath: $.name, [0], [-1], [1:3], [*], ..name, [?(@.price < 10)]",
                );
            }
        },
        |ui| {
            let n = st.matches.len();
            if !st.search.is_empty() {
                if ui
                    .add_enabled(n > 0, theme::icon_button(ui, "↓"))
                    .on_hover_text("Next match (Enter)")
                    .clicked()
                {
                    st.step(true);
                }
                if ui
                    .add_enabled(n > 0, theme::icon_button(ui, "↑"))
                    .on_hover_text("Previous match (Shift+Enter)")
                    .clicked()
                {
                    st.step(false);
                }
                let count = if n == 0 {
                    "No matches".to_owned()
                } else {
                    format!("{} of {n}", st.current + 1)
                };
                ui.label(RichText::new(count).small().color(p.muted));
            }
            let search = ui
                .add(
                    TextEdit::singleline(&mut st.search)
                        .id_salt("response-search")
                        .hint_text("Search (Ctrl+F)")
                        .desired_width(180.0),
                )
                .on_hover_text(
                    "Search in the body (Ctrl+F). Enter: next match, Shift+Enter: previous.",
                );
            if std::mem::take(&mut st.focus_search) {
                search.request_focus();
            }
            if search.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                let back = ui.input(|i| i.modifiers.shift);
                st.step(!back);
                search.request_focus();
            }
        },
    );
    ui.add_space(4.0);
}

/// The text, one row per line; only the visible rows are laid out, so big bodies
/// scroll smoothly (R-NF-2). Matches of the search are highlighted (R-HTTP-12).
fn text_rows(ui: &mut Ui, body: &TextBody, st: &mut ResponseState) {
    let row_height = ui.text_style_height(&TextStyle::Monospace);
    ui.spacing_mut().item_spacing.y = 3.0;
    // A new text (another response, or a filter result) starts at the top.
    let mut area = ScrollArea::both()
        .id_salt(("response-body", body.text.as_ptr() as usize))
        .auto_shrink(false);
    if std::mem::take(&mut st.scroll_to_match)
        && let Some(m) = st.matches.get(st.current)
    {
        let row = body.rows.partition_point(|r| r.end < m.start);
        let stride = row_height + ui.spacing().item_spacing.y;
        #[expect(clippy::cast_precision_loss, reason = "row numbers are far below 2^23")]
        let y = (row as f32 - 3.0).max(0.0) * stride;
        let column = body
            .rows
            .get(row)
            .map_or(0, |r| body.text[r.start..m.start].chars().count());
        let char_width =
            ui.fonts_mut(|f| f.glyph_width(&TextStyle::Monospace.resolve(ui.style()), 'x'));
        #[expect(
            clippy::cast_precision_loss,
            reason = "rows are at most a few thousand characters"
        )]
        let x = (column as f32 * char_width - 120.0).max(0.0);
        area = area.vertical_scroll_offset(y).horizontal_scroll_offset(x);
    }
    let p = theme::palette(ui);
    let font = TextStyle::Monospace.resolve(ui.style());
    let text_color = ui.visuals().text_color();
    area.show_rows(ui, row_height, body.rows.len(), |ui, visible| {
        for range in &body.rows[visible] {
            let found = matches_in(&st.matches, range);
            if found.is_empty() {
                ui.add(
                    Label::new(RichText::new(&body.text[range.clone()]).monospace())
                        .wrap_mode(TextWrapMode::Extend),
                );
                continue;
            }
            let mut job = LayoutJob::default();
            let plain = TextFormat::simple(font.clone(), text_color);
            let mut at = range.start;
            for (i, m) in found {
                let (start, end) = (m.start.max(range.start), m.end.min(range.end));
                job.append(&body.text[at..start], 0.0, plain.clone());
                let background = if i == st.current {
                    p.redirect
                } else {
                    p.redirect.gamma_multiply(0.35)
                };
                let color = if i == st.current { p.bg } else { text_color };
                job.append(
                    &body.text[start..end],
                    0.0,
                    TextFormat {
                        background,
                        ..TextFormat::simple(font.clone(), color)
                    },
                );
                at = end;
            }
            job.append(&body.text[at..range.end], 0.0, plain);
            ui.add(Label::new(job).wrap_mode(TextWrapMode::Extend));
        }
    });
}

/// The matches (with their index) that overlap `row`. `matches` is sorted.
fn matches_in<'a>(
    matches: &'a [Range<usize>],
    row: &Range<usize>,
) -> Vec<(usize, &'a Range<usize>)> {
    let first = matches.partition_point(|m| m.end <= row.start);
    matches[first..]
        .iter()
        .take_while(|m| m.start < row.end)
        .enumerate()
        .map(|(i, m)| (first + i, m))
        .collect()
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
        let mut filter = String::new();
        let mut harness = crate::test_host::harness(move |ui| {
            let cx = PanelContext {
                sending: false,
                can_save: true,
                filter: &mut filter,
                filtered: Filtered::Off,
            };
            response_panel(ui, outcome.as_ref(), &mut st, cx);
        });
        harness.run();
        harness
    }

    fn text(view: &ResponseView) -> &TextBody {
        match &view.body {
            Body::Text(t) => t,
            other => panic!("expected text, got {other:?}"),
        }
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
        let body = text(view);
        assert!(body.truncated);
        assert!(body.rows.len() > 1, "a single long line is split into rows");
        render(Some(outcome)).get_by_label_contains("Showing the first 2.0 MB");
    }

    /// R-NF-2: a 50 MB response keeps only what is shown, and drawing it stays quick.
    #[test]
    fn a_50_mb_body_is_prepared_and_drawn_quickly() {
        let mut res = json_response();
        let item = r#"{"id": 12345, "name": "Ærøskøbing", "tags": ["a", "b"]},"#;
        res.body = format!("[{}]", item.repeat(50 * 1024 * 1024 / item.len())).into_bytes();
        assert!(res.body.len() > 50 * 1000 * 1000);

        let started = std::time::Instant::now();
        let outcome = Outcome::from_result(Ok(res));
        let prepared = started.elapsed();
        let Outcome::Response(view) = &outcome else {
            panic!("expected response")
        };
        let body = text(view);
        assert!(body.truncated);
        assert!(body.json, "big JSON bodies can still be filtered");
        assert!(
            body.text.len() <= format::MAX_DISPLAY_BYTES,
            "only the shown part is kept"
        );

        let mut harness = render(Some(outcome));
        harness.get_by_label_contains("Showing the first 2.0 MB");
        let started = std::time::Instant::now();
        harness.run();
        let drawn = started.elapsed();
        println!("50 MB body: prepared in {prepared:?}, frames drawn in {drawn:?}");
        // Preparing runs off the UI thread; drawing must never freeze it.
        assert!(drawn < Duration::from_millis(500), "drawing took {drawn:?}");
    }

    /// R-HTTP-13: the whole body is filtered, not just the shown part.
    #[test]
    fn json_filter_uses_the_whole_body() {
        let mut res = json_response();
        let items: Vec<String> = (0..200_000).map(|i| format!(r#"{{"id":{i}}}"#)).collect();
        res.body = format!("[{}]", items.join(",")).into_bytes();
        assert!(res.body.len() > format::MAX_DISPLAY_BYTES);
        let view = ResponseView::new(res);
        let last = filter_body(&view, "$[-1].id").expect("filters");
        assert_eq!(last.text, "199999");
        let some = filter_body(&view, "$[?(@.id < 2)]").expect("filters");
        assert_eq!(
            some.text,
            "[\n  {\n    \"id\": 0\n  },\n  {\n    \"id\": 1\n  }\n]"
        );
        assert!(filter_body(&view, "$[").is_err());

        let mut plain = json_response();
        plain.body = b"not json".to_vec();
        let err = filter_body(&ResponseView::new(plain), "$").expect_err("not JSON");
        assert!(err.contains("not valid JSON"), "{err}");
    }

    /// R-HTTP-12: case-insensitive matches, counted; Enter goes to the next one.
    #[test]
    fn search_counts_and_steps_through_matches() {
        assert_eq!(find_all("Aa aA", "aa"), [0..2, 3..5]);
        assert_eq!(find_all("ÆBLE æble", "æble"), [0..5, 6..11]);
        assert_eq!(find_all("abc", ""), []);

        let mut res = json_response();
        res.body = br#"{"name":"Ann","friends":["anne","ANNA"]}"#.to_vec();
        let mut h = render(Some(Outcome::from_result(Ok(res))));
        let field = h.get_by_label_contains("Search in the body");
        field.focus();
        field.type_text("ann");
        h.run();
        h.get_by_label("1 of 3");
        h.key_press(Key::Enter);
        h.run();
        h.get_by_label("2 of 3");
    }

    #[test]
    fn text_start_cuts_on_a_character_boundary() {
        assert_eq!(text_start("aé".as_bytes(), 2), Some("a"));
        assert_eq!(text_start("aé".as_bytes(), 3), Some("aé"));
        assert_eq!(text_start(&[0xff, b'a'], 2), None);
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
