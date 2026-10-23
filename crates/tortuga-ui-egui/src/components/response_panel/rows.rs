//! The rows of a text body: only the visible ones are laid out (R-NF-2), search matches
//! are highlighted (R-HTTP-12), JSON and XML are in colour (R-HTTP-24), and formatted JSON has a menu per row (R-TEST-13).

use std::ops::Range;

use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, Label, RichText, ScrollArea, TextStyle, TextWrapMode, Ui};
use tortuga_core::highlight::Token;

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
    let font = TextStyle::Monospace.resolve(ui.style());
    let json_lines = st.json_lines.lines().filter(|_| menus == RowMenus::Json);
    let mut action = None;
    let mut wants_json_lines = false;
    area.show_rows(ui, row_height, rows.len(), |ui, visible| {
        for range in &rows[visible] {
            let found = matches_in(&st.matches, range);
            let row = if found.is_empty() && spans_in(&body.spans, range).is_empty() {
                ui.add(
                    Label::new(RichText::new(&body.text[range.clone()]).monospace())
                        .wrap_mode(TextWrapMode::Extend)
                        .sense(egui::Sense::click()),
                )
            } else {
                highlighted_row(ui, body, range, &found, st.current, &font)
            };
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
    if wants_json_lines {
        st.json_lines.ask();
        ui.ctx().request_repaint();
    }
    action
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
            .sense(egui::Sense::click()),
    )
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
