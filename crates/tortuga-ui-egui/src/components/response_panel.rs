use std::ops::Range;
use std::sync::{Arc, OnceLock};

use egui::text::{LayoutJob, TextFormat};
use egui::{Grid, Key, Label, RichText, ScrollArea, TextEdit, TextStyle, TextWrapMode, Ui};
use tortuga_core::{BodyView, Response, jsonpath};

use crate::{format, theme};

/// Longest piece of a line drawn as one row; longer lines are split so a
/// minified multi-megabyte body never becomes one giant text layout.
const MAX_ROW_CHARS: usize = 2000;

/// Longest side of an image as drawn; bigger images are scaled down to it once, when
/// the response arrives (R-HTTP-17).
const MAX_IMAGE_SIDE: u32 = 4096;

/// Images with a longer side are not decoded at all, nor more than this many bytes of
/// pixels, so a hostile response cannot exhaust memory (R-HTTP-17).
const MAX_DECODED_SIDE: u32 = 16_384;
const MAX_DECODED_BYTES: u64 = 512 * 1024 * 1024;

/// Bytes of a binary body shown in the raw (hex) view (R-HTTP-18).
const MAX_HEX_BYTES: usize = 256 * 1024;

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
    #[cfg(test)]
    pub fn from_result(result: tortuga_core::Result<Response>) -> Self {
        match result {
            Ok(response) => Self::Response(Arc::new(ResponseView::new(response))),
            Err(e) => Self::Error(format::sentence(&e.to_string())),
        }
    }

    /// The outcome of a run with scripts (R-TEST-2): with no response, the first error
    /// (e.g. from the pre-request script) explains why.
    pub fn from_step(response: Option<Result<Response, String>>, errors: &[String]) -> Self {
        match response {
            Some(Ok(response)) => Self::Response(Arc::new(ResponseView::new(response))),
            Some(Err(e)) => Self::Error(e),
            None => Self::Error(
                errors
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "Not sent.".into()),
            ),
        }
    }
}

/// What the scripts and captures of a send did (R-TEST-1, R-TEST-4, R-TEST-5).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TestsView {
    pub tests: Vec<tortuga_core::script::TestResult>,
    pub logs: Vec<String>,
    pub errors: Vec<String>,
    /// Variables set (`Some`) or removed (`None`).
    pub vars: std::collections::BTreeMap<String, Option<String>>,
}

impl TestsView {
    /// `None` when the request has no scripts or captures.
    pub fn from_step(o: &tortuga_core::runner::StepOutcome) -> Option<Self> {
        let view = Self {
            tests: o.tests.clone(),
            logs: o.logs.clone(),
            errors: o.errors.clone(),
            vars: o.vars.clone(),
        };
        (view != Self::default()).then_some(view)
    }

    fn failed(&self) -> usize {
        self.tests.iter().filter(|t| !t.passed).count()
    }
}

/// A response with its body ready to draw.
#[derive(Debug)]
pub struct ResponseView {
    pub response: Response,
    body: Body,
    /// The body as it came, when that differs from `body`: the JSON before
    /// pretty-printing, or binary content as hex (R-HTTP-18).
    raw: Option<TextBody>,
    /// The whole body parsed as JSON, on first use by the filter (R-HTTP-13).
    json: OnceLock<Option<serde_json::Value>>,
}

#[derive(Debug)]
enum Body {
    Empty,
    Binary,
    /// A picture the panel can show (R-HTTP-17).
    Image(ImageBody),
    Text(TextBody),
}

/// A decoded image, ready to upload as a texture.
pub struct ImageBody {
    /// Scaled down to at most [`MAX_IMAGE_SIDE`].
    pixels: Arc<egui::ColorImage>,
    /// Width and height of the image as sent.
    size: [u32; 2],
    /// "PNG", "JPEG", …
    format: &'static str,
}

impl std::fmt::Debug for ImageBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImageBody")
            .field("size", &self.size)
            .field("format", &self.format)
            .finish_non_exhaustive()
    }
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
    /// How much of the body is shown when it is `truncated`.
    limit: usize,
}

impl TextBody {
    fn new(text: String, truncated: bool, json: bool) -> Self {
        Self {
            rows: rows(&text),
            text,
            truncated,
            json,
            limit: format::MAX_DISPLAY_BYTES,
        }
    }

    /// The start of `bytes` as a hex dump (R-HTTP-18).
    fn hex(bytes: &[u8]) -> Self {
        let shown = &bytes[..bytes.len().min(MAX_HEX_BYTES)];
        Self {
            limit: MAX_HEX_BYTES,
            ..Self::new(hex_dump(shown), shown.len() < bytes.len(), false)
        }
    }
}

impl ResponseView {
    pub fn new(response: Response) -> Self {
        if let Some(image) = decode_image(&response) {
            return Self {
                raw: Some(TextBody::hex(&response.body)),
                body: Body::Image(image),
                response,
                json: OnceLock::new(),
            };
        }
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
        let raw = match &body {
            Body::Binary => Some(TextBody::hex(&response.body)),
            // Pretty-printed: the raw view shows the text as it came. A truncated body
            // is never pretty-printed, so it is shown as it came already.
            Body::Text(t) if t.json && !t.truncated => std::str::from_utf8(&response.body)
                .ok()
                .map(|text| shown(text.to_owned(), false)),
            _ => None,
        };
        Self {
            response,
            body,
            raw,
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

/// The body as an image, if it is one in a format Tortuga can decode (R-HTTP-17).
/// Decoding is slow for big images: call it off the UI thread.
fn decode_image(response: &Response) -> Option<ImageBody> {
    let said_image = response
        .content_type()
        .is_some_and(|ct| ct.trim_start().to_ascii_lowercase().starts_with("image/"));
    // The content type can be missing or wrong; the first bytes tell.
    let format = image::guess_format(&response.body).ok()?;
    if !said_image && std::str::from_utf8(&response.body).is_ok() {
        return None;
    }
    let mut reader =
        image::ImageReader::with_format(std::io::Cursor::new(&response.body[..]), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_DECODED_SIDE);
    limits.max_image_height = Some(MAX_DECODED_SIDE);
    limits.max_alloc = Some(MAX_DECODED_BYTES);
    reader.limits(limits);
    let decoded = reader.decode().ok()?;
    let size = [decoded.width(), decoded.height()];
    let decoded = if size.iter().any(|&side| side > MAX_IMAGE_SIDE) {
        decoded.thumbnail(MAX_IMAGE_SIDE, MAX_IMAGE_SIDE)
    } else {
        decoded
    };
    let rgba = decoded.to_rgba8();
    let pixels = egui::ColorImage::from_rgba_unmultiplied(
        [rgba.width() as usize, rgba.height() as usize],
        rgba.as_raw(),
    );
    Some(ImageBody {
        pixels: Arc::new(pixels),
        size,
        format: image_format_name(format),
    })
}

fn image_format_name(format: image::ImageFormat) -> &'static str {
    match format {
        image::ImageFormat::Png => "PNG",
        image::ImageFormat::Jpeg => "JPEG",
        image::ImageFormat::Gif => "GIF",
        image::ImageFormat::WebP => "WebP",
        image::ImageFormat::Bmp => "BMP",
        image::ImageFormat::Ico => "ICO",
        _ => "Image",
    }
}

/// `bytes` as rows of 16: offset, hex, and the printable ASCII characters (R-HTTP-18).
fn hex_dump(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(bytes.len() / 16 * 80 + 80);
    for (n, chunk) in bytes.chunks(16).enumerate() {
        if n > 0 {
            out.push('\n');
        }
        let _ = write!(out, "{:08x} ", n * 16);
        for i in 0..16 {
            if i == 8 {
                out.push(' ');
            }
            match chunk.get(i) {
                Some(b) => {
                    let _ = write!(out, " {b:02x}");
                }
                None => out.push_str("   "),
            }
        }
        out.push_str("  |");
        out.extend(chunk.iter().map(|&b| {
            if b.is_ascii_graphic() || b == b' ' {
                char::from(b)
            } else {
                '.'
            }
        }));
        out.push('|');
    }
    out
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

/// `rows` of `text` cut into pieces of at most `width` characters (R-SET-7).
fn wrap_rows(text: &str, rows: &[Range<usize>], width: usize) -> Vec<Range<usize>> {
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let mut from = row.start;
        for (chars, (i, _)) in text[row.clone()].char_indices().enumerate() {
            if chars > 0 && chars % width == 0 {
                out.push(from..row.start + i);
                from = row.start + i;
            }
        }
        out.push(from..row.end);
    }
    out
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
    /// Test results, captured values and console output (R-TEST-1).
    Tests,
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
    /// The rows of the shown text wrapped at the panel's width (R-SET-7).
    wrapped: Option<WrappedRows>,
    /// Show the body as it came instead of pretty-printed or as a picture (R-HTTP-18).
    /// Kept for the next response.
    raw: bool,
    /// The shown image on the GPU (R-HTTP-17).
    texture: Option<ImageTexture>,
}

/// An uploaded image, with the address of the pixels it was made from.
#[derive(Clone)]
struct ImageTexture {
    pixels: usize,
    handle: egui::TextureHandle,
}

impl std::fmt::Debug for ImageTexture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImageTexture")
            .field("id", &self.handle.id())
            .finish_non_exhaustive()
    }
}

/// Rows of a text cut at a width, kept until the text or the width changes (R-SET-7).
#[derive(Debug, Clone)]
struct WrappedRows {
    /// The text's address and length, and the width in characters.
    key: (usize, usize, usize),
    rows: Vec<Range<usize>>,
}

impl ResponseState {
    /// The state for a new response: everything is reset except the choice of view.
    pub fn reset(&mut self) {
        *self = Self {
            raw: self.raw,
            ..Self::default()
        };
    }

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
    /// What the request's scripts and captures did, if it has any (R-TEST-1).
    pub tests: Option<&'a TestsView>,
    /// Wrap long lines of the body (R-SET-7).
    pub wrap: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseAction {
    SaveBody,
    /// Turn wrapping long lines on or off (R-SET-7).
    SetWrap(bool),
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
        tests,
        wrap,
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
    let mut tabs = vec![
        (Tab::Body, "Body", None),
        (Tab::Headers, "Headers", Some(header_count)),
    ];
    if let Some(t) = tests {
        let label = if t.tests.is_empty() {
            (!t.errors.is_empty()).then(|| "✗".to_owned())
        } else {
            Some(format!("{}/{}", t.tests.len() - t.failed(), t.tests.len()))
        };
        tabs.push((Tab::Tests, "Tests", label));
    } else if st.tab == Tab::Tests {
        st.tab = Tab::Body;
    }
    theme::tabs(ui, &mut st.tab, &tabs);

    theme::card(ui).show(ui, |ui| match st.tab {
        Tab::Body => {
            if let Some(wrap) = body(ui, view, st, filter, filtered, wrap) {
                action = Some(ResponseAction::SetWrap(wrap));
            }
        }
        Tab::Tests => tests_tab(ui, tests),
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

/// The body with its toolbar. Returns the new wrapping choice if it was changed.
fn body(
    ui: &mut Ui,
    view: &ResponseView,
    st: &mut ResponseState,
    filter: &mut String,
    filtered: Filtered<'_>,
    wrap: bool,
) -> Option<bool> {
    // "Preview" for content that is not text, "Pretty" for text (R-HTTP-18).
    let modes = view.raw.as_ref().map(|_| match view.body {
        Body::Image(_) | Body::Binary => "Preview",
        _ => "Pretty",
    });
    let raw = view.raw.as_ref().filter(|_| st.raw);
    let full = match (&view.body, raw) {
        (_, Some(raw)) => raw,
        (Body::Empty, None) => {
            theme::hint(ui, "No body.");
            return None;
        }
        (Body::Binary, None) => {
            view_modes(ui, &mut st.raw, modes);
            theme::hint(
                ui,
                "Binary content. Use “Save to file” to keep it, or “Raw” to see its bytes.",
            );
            return None;
        }
        (Body::Image(img), None) => {
            view_modes(ui, &mut st.raw, modes);
            image_view(ui, img, st);
            return None;
        }
        (Body::Text(text), None) => text,
    };
    // The JSON filter works on the pretty view only.
    let filtered = if raw.is_some() {
        Filtered::Off
    } else {
        filtered
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
    let wrap_changed = toolbar(ui, st, modes, full.json.then_some(filter), wrap);
    if let Filtered::Failed(message) = filtered {
        theme::warning(ui, message);
    }
    let Some(shown) = shown else {
        ui.horizontal(|ui| {
            ui.spinner();
            theme::hint(ui, "Filtering…");
        });
        return wrap_changed;
    };
    if shown.truncated {
        let limit = format::size(shown.limit);
        theme::hint(
            ui,
            format!("Showing the first {limit}. Use “Save to file” for the full body."),
        );
    }
    text_rows(ui, shown, st, wrap);
    wrap_changed
}

/// The "Pretty" (or "Preview") and "Raw" switch, if the body has a raw view (R-HTTP-18).
fn view_modes(ui: &mut Ui, raw: &mut bool, modes: Option<&str>) {
    let Some(pretty) = modes else {
        return;
    };
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        if ui
            .add(egui::Button::selectable(!*raw, pretty))
            .on_hover_text("Show the body formatted")
            .clicked()
        {
            *raw = false;
        }
        if ui
            .add(egui::Button::selectable(*raw, "Raw"))
            .on_hover_text("Show the body as it came: unformatted text, or the bytes as hex")
            .clicked()
        {
            *raw = true;
        }
    });
}

/// The image, scaled down to fit the panel, never up (R-HTTP-17).
fn image_view(ui: &mut Ui, img: &ImageBody, st: &mut ResponseState) {
    let p = theme::palette(ui);
    let [w, h] = img.size;
    ui.add_space(4.0);
    ui.label(
        RichText::new(format!("{} · {w} × {h} px", img.format))
            .small()
            .color(p.muted),
    );
    let max_side = ui.input(|i| i.max_texture_side);
    if img.pixels.size.iter().any(|&side| side > max_side) {
        theme::hint(ui, "Too large to show here. Use “Save to file” to keep it.");
        return;
    }
    let key = Arc::as_ptr(&img.pixels) as usize;
    if st.texture.as_ref().is_none_or(|t| t.pixels != key) {
        let handle = ui.ctx().load_texture(
            "response-image",
            Arc::clone(&img.pixels),
            egui::TextureOptions::LINEAR,
        );
        st.texture = Some(ImageTexture {
            pixels: key,
            handle,
        });
    }
    let Some(texture) = &st.texture else {
        return;
    };
    let texture = egui::load::SizedTexture::from_handle(&texture.handle);
    ScrollArea::both()
        .id_salt("response-image")
        .auto_shrink(false)
        .show(ui, |ui| {
            let room = ui.available_size() - egui::vec2(4.0, 4.0);
            let scale = (room.x / texture.size.x)
                .min(room.y / texture.size.y)
                .clamp(0.01, 1.0);
            let size = texture.size * scale;
            ui.vertical_centered(|ui| {
                ui.add(egui::Image::from_texture(texture).fit_to_exact_size(size))
                    .on_hover_text(format!("{w} × {h} px, shown at {:.0}%", scale * 100.0));
            });
        });
}

/// Test results, errors, variables set and console output (R-TEST-1, R-TEST-4, R-TEST-5).
fn tests_tab(ui: &mut Ui, tests: Option<&TestsView>) {
    let Some(t) = tests else {
        return;
    };
    ScrollArea::vertical()
        .id_salt("response-tests")
        .auto_shrink(false)
        .show(ui, |ui| tests_details(ui, t));
}

/// Test results, errors, variables set and console output of a run (R-TEST-1, R-TEST-5).
pub fn tests_details(ui: &mut Ui, t: &TestsView) {
    let p = theme::palette(ui);
    {
        {
            if t.tests.is_empty() && t.errors.is_empty() {
                theme::hint(
                    ui,
                    "No tests ran. Add some with test(\"name\", () => expect(res.status).toBe(200)) in the Scripts tab.",
                );
            }
            for test in &t.tests {
                ui.horizontal_wrapped(|ui| {
                    if test.passed {
                        ui.label(RichText::new("✓").color(p.ok));
                        ui.label(&test.name);
                    } else {
                        ui.label(RichText::new("✗").color(p.fail));
                        ui.label(RichText::new(&test.name).color(p.fail));
                        if let Some(m) = &test.message {
                            ui.label(RichText::new(m).color(p.muted));
                        }
                    }
                });
            }
            for e in &t.errors {
                ui.add(Label::new(RichText::new(e).color(p.fail)).wrap());
            }
            if !t.vars.is_empty() {
                ui.add_space(8.0);
                theme::section_label(ui, "Variables set");
                Grid::new("tests-vars")
                    .num_columns(2)
                    .spacing([16.0, 4.0])
                    .show(ui, |ui| {
                        for (name, value) in &t.vars {
                            ui.label(RichText::new(name).monospace());
                            let shown = value.as_deref().map_or_else(
                                || "(removed)".to_owned(),
                                |v| format::truncate(v, 120).0.to_owned(),
                            );
                            ui.add(
                                Label::new(RichText::new(shown).monospace().color(p.muted))
                                    .truncate(),
                            );
                            ui.end_row();
                        }
                    });
                theme::hint(
                    ui,
                    "Kept for this project until Tortuga closes; they win over environment values.",
                );
            }
            if !t.logs.is_empty() {
                ui.add_space(8.0);
                theme::section_label(ui, "Console");
                for line in &t.logs {
                    ui.add(Label::new(RichText::new(line).monospace()).wrap());
                }
            }
        }
    }
}

/// The view switch, the JSON filter (for JSON bodies), the search field and "Wrap", above
/// the body.
/// Returns the new wrapping choice if it was changed (R-SET-7).
fn toolbar(
    ui: &mut Ui,
    st: &mut ResponseState,
    modes: Option<&str>,
    filter: Option<&mut String>,
    wrap: bool,
) -> Option<bool> {
    let p = theme::palette(ui);
    let mut wrap_changed = None;
    let mut raw = st.raw;
    egui::Sides::new().shrink_left().show(
        ui,
        |ui| {
            view_modes(ui, &mut raw, modes);
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
            if ui
                .add(egui::Button::selectable(wrap, "Wrap"))
                .on_hover_text("Wrap long lines (also in Settings → Appearance)")
                .clicked()
            {
                wrap_changed = Some(!wrap);
            }
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
    st.raw = raw;
    ui.add_space(4.0);
    wrap_changed
}

/// The text, one row per line; only the visible rows are laid out, so big bodies
/// scroll smoothly (R-NF-2). Matches of the search are highlighted (R-HTTP-12).
fn text_rows(ui: &mut Ui, body: &TextBody, st: &mut ResponseState, wrap: bool) {
    let row_height = ui.text_style_height(&TextStyle::Monospace);
    ui.spacing_mut().item_spacing.y = 3.0;
    let char_width =
        ui.fonts_mut(|f| f.glyph_width(&TextStyle::Monospace.resolve(ui.style()), 'x'));
    // Wrapped: rows as wide as the panel, worked out from the monospace character width
    // and kept until the text or the width changes (R-SET-7).
    let rows: &[Range<usize>] = if wrap {
        // Leave room for the scroll bar.
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a panel is a few thousand points wide at most"
        )]
        let width = ((ui.available_width() - 16.0) / char_width.max(1.0)).max(10.0) as usize;
        let key = (body.text.as_ptr() as usize, body.text.len(), width);
        if st.wrapped.as_ref().is_none_or(|w| w.key != key) {
            st.wrapped = Some(WrappedRows {
                key,
                rows: wrap_rows(&body.text, &body.rows, width),
            });
        }
        st.wrapped.as_ref().map_or(&body.rows, |w| &w.rows)
    } else {
        &body.rows
    };
    // A new text (another response, or a filter result) starts at the top.
    let mut area = if wrap {
        ScrollArea::vertical()
    } else {
        ScrollArea::both()
    }
    .id_salt(("response-body", body.text.as_ptr() as usize, wrap))
    .auto_shrink(false);
    if std::mem::take(&mut st.scroll_to_match)
        && let Some(m) = st.matches.get(st.current)
    {
        let row = rows.partition_point(|r| r.end < m.start);
        let stride = row_height + ui.spacing().item_spacing.y;
        #[expect(clippy::cast_precision_loss, reason = "row numbers are far below 2^23")]
        let y = (row as f32 - 3.0).max(0.0) * stride;
        let column = rows
            .get(row)
            .map_or(0, |r| body.text[r.start..m.start].chars().count());
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
    area.show_rows(ui, row_height, rows.len(), |ui, visible| {
        for range in &rows[visible] {
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
                tests: None,
                wrap: false,
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

    fn png(width: u32, height: u32) -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(width, height, image::Rgba([200, 30, 30, 255]));
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png)
            .expect("encodes");
        out.into_inner()
    }

    fn image_response(content_type: &str, body: Vec<u8>) -> Response {
        Response {
            headers: vec![KeyValue::new("content-type", content_type)],
            body,
            ..json_response()
        }
    }

    /// R-HTTP-17: an image is shown with its format and size, also without the right
    /// content type; "Raw" shows its bytes as hex (R-HTTP-18).
    #[test]
    fn images_are_shown_and_raw_shows_their_bytes() {
        for content_type in ["image/png", "application/octet-stream"] {
            let outcome = Outcome::from_result(Ok(image_response(content_type, png(3, 2))));
            let Outcome::Response(view) = &outcome else {
                panic!("expected response")
            };
            assert!(matches!(view.body, Body::Image(_)), "{content_type}");
            let mut h = render(Some(outcome));
            h.get_by_label("PNG · 3 × 2 px");
            h.get_by_label("Raw").click();
            h.run();
            h.get_by_label_contains("89 50 4e 47 0d 0a 1a 0a");
            h.get_by_label("Preview").click();
            h.run();
            h.get_by_label("PNG · 3 × 2 px");
        }
    }

    /// R-HTTP-17: big images are scaled down once, when the response arrives.
    #[test]
    fn big_images_are_scaled_down_to_draw() {
        let view = ResponseView::new(image_response("image/png", png(MAX_IMAGE_SIDE * 2, 4)));
        let Body::Image(img) = &view.body else {
            panic!("expected an image")
        };
        assert_eq!(img.size, [MAX_IMAGE_SIDE * 2, 4]);
        assert_eq!(img.pixels.size[0], MAX_IMAGE_SIDE as usize);
    }

    /// Text and broken images are not images; a broken one is binary with a hex view.
    #[test]
    fn only_decodable_images_are_images() {
        let svg = image_response("image/svg+xml", b"<svg xmlns='x'/>".to_vec());
        assert!(matches!(ResponseView::new(svg).body, Body::Text(_)));
        let mut broken = png(2, 2);
        broken.truncate(30);
        let view = ResponseView::new(image_response("image/png", broken));
        assert!(matches!(view.body, Body::Binary));
        assert!(view.raw.is_some());
    }

    /// R-HTTP-18: "Raw" shows JSON as it came, and stays chosen for the next response.
    #[test]
    fn raw_shows_json_as_it_came() {
        let mut h = render(Some(Outcome::from_result(Ok(json_response()))));
        h.get_by_label("  \"id\": 7");
        h.get_by_label("Raw").click();
        h.run();
        h.get_by_label(r#"{"id":7}"#);
        assert!(
            h.query_by_label("Filter").is_none(),
            "no JSON filter on raw text"
        );

        let mut st = ResponseState {
            raw: true,
            search: "x".into(),
            ..ResponseState::default()
        };
        st.reset();
        assert!(st.raw && st.search.is_empty());

        // Plain text has nothing to switch.
        let mut plain = json_response();
        plain.headers.clear();
        plain.body = b"hello".to_vec();
        assert!(ResponseView::new(plain).raw.is_none());
    }

    #[test]
    fn hex_dump_shows_offset_bytes_and_text() {
        let dump = hex_dump(b"Hello, world!\n\x00\xffAB");
        let lines: Vec<&str> = dump.lines().collect();
        assert_eq!(
            lines,
            [
                "00000000  48 65 6c 6c 6f 2c 20 77  6f 72 6c 64 21 0a 00 ff  |Hello, world!...|",
                "00000010  41 42                                             |AB|",
            ]
        );
        let big = TextBody::hex(&vec![0; MAX_HEX_BYTES + 1]);
        assert!(big.truncated);
        assert_eq!(big.rows.len(), MAX_HEX_BYTES / 16);
    }

    #[test]
    fn text_start_cuts_on_a_character_boundary() {
        assert_eq!(text_start("aé".as_bytes(), 2), Some("a"));
        assert_eq!(text_start("aé".as_bytes(), 3), Some("aé"));
        assert_eq!(text_start(&[0xff, b'a'], 2), None);
    }

    /// R-SET-7: wrapped rows are at most `width` characters, also with multi-byte ones.
    #[test]
    fn wrapped_rows_are_cut_at_the_width() {
        let text = "abcdefg\nhé";
        let rows = rows(text);
        let wrapped = wrap_rows(text, &rows, 3);
        let pieces: Vec<&str> = wrapped.iter().map(|r| &text[r.clone()]).collect();
        assert_eq!(pieces, ["abc", "def", "g", "hé"]);
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
