//! The rows of a text body: only the visible ones are laid out (R-NF-2), search matches
//! are highlighted (R-HTTP-12), JSON and XML are in colour (R-HTTP-24), formatted JSON has a menu per row (R-TEST-13),
//! formatted JSON and XML fold (R-HTTP-26), and the keyboard scrolls (R-HTTP-27).

use std::ops::Range;

use egui::text::{LayoutJob, TextFormat};
use egui::{
    Color32, Key, Label, Modifiers, Rect, RichText, ScrollArea, Sense, TextStyle, TextWrapMode, Ui,
    Vec2,
};
use tortuga_core::highlight::Token;

use super::folding::Fold;
use super::{ResponseAction, ResponseState, TextBody, WrappedRows, json_menu, wrap_rows};
use crate::components::code_colours;
use crate::theme;

/// Whether the body's rows have a menu (R-TEST-13).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RowMenus {
    /// Formatted JSON: tests and captures for the value on a line.
    Json,
    None,
}

/// The text, one row per line; only the visible rows are laid out, so big bodies
/// scroll smoothly (R-NF-2). Matches of the search are highlighted (R-HTTP-12).
pub(super) fn text_rows(
    ui: &mut Ui,
    body: &TextBody,
    st: &mut ResponseState,
    wrap: bool,
    menus: RowMenus,
) -> Option<ResponseAction> {
    if menus == RowMenus::Json {
        st.json_lines.update(&body.text);
    }
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
    let scroll_to_match = std::mem::take(&mut st.scroll_to_match);
    if scroll_to_match && let Some(m) = st.matches.get(st.current) {
        st.folded.reveal(&body.folds, m.start);
    }
    st.folded.prepare(&body.folds, rows);
    let rows = st.folded.visible(rows);
    if scroll_to_match && let Some(m) = st.matches.get(st.current) {
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
    } else if let Some(to) = st
        .keys
        .pressed(ui, row_height + ui.spacing().item_spacing.y)
    {
        area = area.vertical_scroll_offset(to.y);
        if !wrap {
            area = area.horizontal_scroll_offset(to.x);
        }
    }
    let font = TextStyle::Monospace.resolve(ui.style());
    let json_lines = st.json_lines.lines().filter(|_| menus == RowMenus::Json);
    let mut action = None;
    let mut wants_json_lines = false;
    let mut toggle = None;
    let gutter = (!body.folds.is_empty()).then_some(row_height);
    let output = area.show_rows(ui, row_height, rows.len(), |ui, visible| {
        for range in &rows[visible] {
            let (row, clicked) = text_row(ui, body, range, st, gutter, &font);
            toggle = toggle.or(clicked);
            if menus == RowMenus::Json {
                wants_json_lines |= row.secondary_clicked();
                row.context_menu(|ui| {
                    if let Some(a) = json_menu::row_menu(ui, &body.text, range, json_lines) {
                        action = Some(a);
                    }
                });
            }
        }
    });
    st.keys.seen(
        ui,
        output.inner_rect,
        output.state.clamped_offset(),
        output.content_size,
    );
    if let Some(i) = toggle
        && let Some(fold) = body.folds.get(i)
    {
        st.folded.toggle(fold);
    }
    if wants_json_lines {
        st.json_lines.ask();
        ui.ctx().request_repaint();
    }
    action
}

/// One row: the fold toggle in the `gutter`, the text, and the marker of a folded section.
/// Returns the text's response, and the fold (its index) whose toggle or marker was clicked.
fn text_row(
    ui: &mut Ui,
    body: &TextBody,
    range: &Range<usize>,
    st: &ResponseState,
    gutter: Option<f32>,
    font: &egui::FontId,
) -> (egui::Response, Option<usize>) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let mut toggle =
            gutter.and_then(|width| fold_toggle(ui, &body.folds, range, &st.folded, width));
        let found = matches_in(&st.matches, range);
        let row = if found.is_empty() && spans_in(&body.spans, range).is_empty() {
            ui.add(
                Label::new(RichText::new(&body.text[range.clone()]).monospace())
                    .wrap_mode(TextWrapMode::Extend)
                    .sense(Sense::CLICK),
            )
        } else {
            highlighted_row(ui, body, range, &found, st.current, font)
        };
        if let Some(at) = folded_marker(ui, body, range, &st.folded) {
            toggle = Some(at);
        }
        (row, toggle)
    })
    .inner
}

/// A row in colour (R-HTTP-24) with the search matches in it highlighted (R-HTTP-12).
fn highlighted_row(
    ui: &mut Ui,
    body: &TextBody,
    range: &Range<usize>,
    found: &[(usize, &Range<usize>)],
    current: usize,
    font: &egui::FontId,
) -> egui::Response {
    let p = theme::palette(ui);
    let text_color = ui.visuals().text_color();
    let spans = spans_in(&body.spans, range);
    // The row is cut wherever a coloured part or a match starts or ends.
    let mut cuts = vec![range.start, range.end];
    let clamp = |at: usize| at.clamp(range.start, range.end);
    for r in spans
        .iter()
        .map(|(r, _)| r)
        .chain(found.iter().map(|(_, m)| *m))
    {
        cuts.extend([clamp(r.start), clamp(r.end)]);
    }
    cuts.sort_unstable();
    cuts.dedup();
    let mut job = LayoutJob::default();
    let (mut span, mut hit) = (spans.iter().peekable(), found.iter().peekable());
    for piece in cuts.windows(2) {
        let [start, end] = [piece[0], piece[1]];
        let Some(text) = body.text.get(start..end) else {
            continue;
        };
        // Both lists are in order: skip what ended before this piece.
        while span.next_if(|(r, _)| r.end <= start).is_some() {}
        while hit.next_if(|(_, m)| m.end <= start).is_some() {}
        let mut color = span
            .peek()
            .filter(|(r, _)| r.start <= start)
            .map_or(text_color, |(_, token)| code_colours::colour(&p, *token));
        let mut background = Color32::TRANSPARENT;
        if let Some((i, _)) = hit.peek().filter(|(_, m)| m.start <= start) {
            if *i == current {
                background = p.redirect;
                color = p.bg;
            } else {
                background = p.redirect.gamma_multiply(0.35);
            }
        }
        job.append(
            text,
            0.0,
            TextFormat {
                background,
                ..TextFormat::simple(font.clone(), color)
            },
        );
    }
    ui.add(
        Label::new(job)
            .wrap_mode(TextWrapMode::Extend)
            .sense(Sense::CLICK),
    )
}

/// The ⏷/⏵ before a row that opens a section, else as much space. Returns the fold
/// (its index) when clicked (R-HTTP-26).
fn fold_toggle(
    ui: &mut Ui,
    folds: &[Fold],
    row: &Range<usize>,
    folded: &super::Folded,
    width: f32,
) -> Option<usize> {
    let Ok(i) = folds.binary_search_by_key(&row.start, |f| f.start) else {
        ui.add_space(width);
        return None;
    };
    let fold = &folds[i];
    let (symbol, hint) = if folded.is_folded(fold) {
        ("⏵", t!("response-unfold-hint"))
    } else {
        ("⏷", t!("response-fold-hint"))
    };
    let p = theme::palette(ui);
    let clicked = ui
        .add(
            egui::Button::new(RichText::new(symbol).small().color(p.muted))
                .frame(false)
                .min_size(Vec2::splat(width)),
        )
        .on_hover_text(hint)
        .clicked();
    clicked.then_some(i)
}

/// After the last row of a folded section's opening line: `… }` and how many lines are
/// hidden. Returns the fold (its index) when clicked, to unfold it (R-HTTP-26).
fn folded_marker(
    ui: &mut Ui,
    body: &TextBody,
    row: &Range<usize>,
    folded: &super::Folded,
) -> Option<usize> {
    let i = body
        .folds
        .binary_search_by_key(&row.end, |f| f.open_end)
        .ok()?;
    let fold = &body.folds[i];
    if !folded.is_folded(fold) {
        return None;
    }
    let closing = body.text.get(fold.closing.clone()).unwrap_or_default();
    let p = theme::palette(ui);
    let clicked = ui
        .add(
            Label::new(
                RichText::new(format!(" … {closing}"))
                    .monospace()
                    .color(p.muted),
            )
            .wrap_mode(TextWrapMode::Extend)
            .sense(Sense::CLICK),
        )
        .on_hover_text(t!("response-folded-lines", count = fold.lines))
        .clicked();
    clicked.then_some(i)
}

/// Scrolling the body with the keyboard (R-HTTP-27): while the pointer is over it, or it
/// was the last thing clicked, and no text field has the keyboard.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct KeyScroll {
    /// The body was clicked last.
    active: bool,
    /// Where the body was drawn, and its scroll offset and content size, last frame.
    rect: Option<Rect>,
    offset: Vec2,
    content: Vec2,
}

impl KeyScroll {
    /// The offset the keys pressed this frame scroll to, if any.
    fn pressed(&self, ui: &Ui, line: f32) -> Option<Vec2> {
        let rect = self.rect?;
        if !(self.active || ui.rect_contains_pointer(rect)) || ui.ctx().egui_wants_keyboard_input()
        {
            return None;
        }
        let key = ui.input_mut(|i| {
            SCROLL_KEYS
                .iter()
                .find(|(modifiers, key, _)| i.consume_key(*modifiers, *key))
                .map(|(.., step)| *step)
        })?;
        let to = scrolled(key, self.offset, rect.size(), self.content, line);
        ui.ctx().request_repaint();
        Some(to)
    }

    /// Remembers where the body is, and whether it was clicked.
    fn seen(&mut self, ui: &Ui, rect: Rect, offset: Vec2, content: Vec2) {
        if let Some(at) = ui.input(|i| {
            i.pointer
                .any_pressed()
                .then(|| i.pointer.interact_pos())
                .flatten()
        }) {
            self.active = rect.contains(at);
        }
        self.rect = Some(rect);
        self.offset = offset;
        self.content = content;
    }
}

/// How far a key scrolls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    Up,
    Down,
    Left,
    Right,
    PageUp,
    PageDown,
    Top,
    Bottom,
}

/// The keys that scroll the body (R-HTTP-27). Ctrl+PageUp/PageDown stay with the tabs.
const SCROLL_KEYS: [(Modifiers, Key, Step); 10] = [
    (Modifiers::NONE, Key::ArrowUp, Step::Up),
    (Modifiers::NONE, Key::ArrowDown, Step::Down),
    (Modifiers::NONE, Key::ArrowLeft, Step::Left),
    (Modifiers::NONE, Key::ArrowRight, Step::Right),
    (Modifiers::NONE, Key::PageUp, Step::PageUp),
    (Modifiers::NONE, Key::PageDown, Step::PageDown),
    (Modifiers::NONE, Key::Home, Step::Top),
    (Modifiers::NONE, Key::End, Step::Bottom),
    (Modifiers::COMMAND, Key::Home, Step::Top),
    (Modifiers::COMMAND, Key::End, Step::Bottom),
];

/// The scroll offset after `step`, kept inside the content.
fn scrolled(step: Step, offset: Vec2, viewport: Vec2, content: Vec2, line: f32) -> Vec2 {
    let page = (viewport.y - line).max(line);
    let column = line * 2.0;
    let to = match step {
        Step::Up => offset - Vec2::new(0.0, line),
        Step::Down => offset + Vec2::new(0.0, line),
        Step::Left => offset - Vec2::new(column, 0.0),
        Step::Right => offset + Vec2::new(column, 0.0),
        Step::PageUp => offset - Vec2::new(0.0, page),
        Step::PageDown => offset + Vec2::new(0.0, page),
        Step::Top => Vec2::new(offset.x, 0.0),
        Step::Bottom => Vec2::new(offset.x, f32::INFINITY),
    };
    to.clamp(Vec2::ZERO, (content - viewport).max(Vec2::ZERO))
}

/// The coloured parts that overlap `row`. `spans` is sorted.
fn spans_in<'a>(
    spans: &'a [(Range<usize>, Token)],
    row: &Range<usize>,
) -> &'a [(Range<usize>, Token)] {
    let first = spans.partition_point(|(r, _)| r.end <= row.start);
    let rest = &spans[first..];
    &rest[..rest.partition_point(|(r, _)| r.start < row.end)]
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
    use super::*;

    /// R-HTTP-27: each key scrolls by its step and never past the content.
    #[test]
    fn keys_scroll_by_line_page_and_to_the_ends() {
        let (viewport, content, line) = (Vec2::new(100.0, 200.0), Vec2::new(300.0, 1000.0), 20.0);
        let at = |step, y| scrolled(step, Vec2::new(0.0, y), viewport, content, line).y;
        assert_eq!(at(Step::Down, 100.0), 120.0);
        assert_eq!(at(Step::Up, 100.0), 80.0);
        assert_eq!(at(Step::Up, 0.0), 0.0);
        assert_eq!(at(Step::PageDown, 100.0), 280.0);
        assert_eq!(at(Step::PageDown, 790.0), 800.0);
        assert_eq!(at(Step::PageUp, 100.0), 0.0);
        assert_eq!(at(Step::Bottom, 0.0), 800.0);
        assert_eq!(at(Step::Top, 500.0), 0.0);
        let x = |step, x| scrolled(step, Vec2::new(x, 0.0), viewport, content, line).x;
        assert_eq!(x(Step::Right, 0.0), 40.0);
        assert_eq!(x(Step::Right, 190.0), 200.0);
        assert_eq!(x(Step::Left, 10.0), 0.0);
    }

    #[test]
    fn content_smaller_than_the_view_does_not_scroll() {
        let to = scrolled(
            Step::Bottom,
            Vec2::ZERO,
            Vec2::new(100.0, 200.0),
            Vec2::new(50.0, 50.0),
            20.0,
        );
        assert_eq!(to, Vec2::ZERO);
    }
}
