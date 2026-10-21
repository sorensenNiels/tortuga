//! A request's trend (R-HIST-4): its last sends from history as small bars in the
//! response line, as tall as they took, failures in red, so "it got slower" shows.

use std::fmt::Write as _;

use egui::{Rect, Sense, Ui, vec2};

use crate::{format, theme};

/// Most sends shown.
pub const TREND_LENGTH: usize = 20;
const BAR_WIDTH: f32 = 3.0;
const BAR_GAP: f32 = 1.0;
const HEIGHT: f32 = 16.0;

/// One send of the request, from its history entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrendPoint {
    pub sent_at_ms: u64,
    pub elapsed_ms: Option<u64>,
    /// `None` when no response came back.
    pub status: Option<u16>,
}

impl TrendPoint {
    /// No response, or a 4xx/5xx one.
    pub fn failed(&self) -> bool {
        self.status.is_none_or(|s| s >= 400)
    }
}

/// The request's sends, oldest first, and how to write their times.
#[derive(Debug, Clone, Copy, Default)]
pub struct Trend<'a> {
    pub points: &'a [TrendPoint],
    pub now_ms: u64,
    pub offset_minutes: i32,
}

/// The bars, when there are two sends or more; hovering one says when it was sent,
/// what came back and how long it took.
pub(super) fn trend(ui: &mut Ui, trend: Trend<'_>) {
    let points = trend.points;
    if points.len() < 2 {
        return;
    }
    let p = theme::palette(ui);
    #[expect(clippy::cast_precision_loss, reason = "at most 20 bars")]
    let width = points.len() as f32 * (BAR_WIDTH + BAR_GAP);
    let (rect, response) = ui.allocate_exact_size(vec2(width, HEIGHT), Sense::hover());
    let slowest = points
        .iter()
        .filter_map(|t| t.elapsed_ms)
        .max()
        .unwrap_or(1)
        .max(1);
    let painter = ui.painter_at(rect);
    let mut hovered = None;
    for (i, point) in points.iter().enumerate() {
        #[expect(clippy::cast_precision_loss, reason = "small numbers")]
        let x = rect.left() + i as f32 * (BAR_WIDTH + BAR_GAP);
        #[expect(clippy::cast_precision_loss, reason = "milliseconds of one send")]
        let share = point
            .elapsed_ms
            .map_or(1.0, |ms| ms as f32 / slowest as f32);
        let height = (HEIGHT * share).max(2.0);
        let bar = Rect::from_min_max(
            egui::pos2(x, rect.bottom() - height),
            egui::pos2(x + BAR_WIDTH, rect.bottom()),
        );
        let color = if point.failed() { p.fail } else { p.muted };
        painter.rect_filled(bar, 1.0, color);
        if response
            .hover_pos()
            .is_some_and(|pos| pos.x >= x && pos.x < x + BAR_WIDTH + BAR_GAP)
        {
            hovered = Some(*point);
        }
    }
    let summary = summary(points);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Image, true, &summary));
    response.on_hover_ui(|ui| {
        ui.label(summary);
        if let Some(point) = hovered {
            ui.label(describe(&point, trend.now_ms, trend.offset_minutes));
        }
    });
}

/// "Last 12 sends: 80 ms to 1.2 s, 2 failed".
fn summary(points: &[TrendPoint]) -> String {
    let times = points.iter().filter_map(|t| t.elapsed_ms);
    let (fastest, slowest) = (times.clone().min(), times.max());
    let failed = points.iter().filter(|t| t.failed()).count();
    let mut text = format!("Last {} sends", points.len());
    if let (Some(a), Some(b)) = (fastest, slowest) {
        let _ = write!(text, ": {} to {}", ms(a), ms(b));
    }
    if failed > 0 {
        let _ = write!(text, ", {failed} failed");
    }
    text
}

/// "Today 14:32 · 200 · 120 ms".
fn describe(point: &TrendPoint, now_ms: u64, offset_minutes: i32) -> String {
    let when = format!(
        "{} {}",
        format::day_label(point.sent_at_ms, now_ms, offset_minutes),
        format::clock(point.sent_at_ms, offset_minutes)
    );
    let status = point
        .status
        .map_or_else(|| "no response".to_owned(), |s| s.to_string());
    match point.elapsed_ms {
        Some(t) => format!("{when} · {status} · {}", ms(t)),
        None => format!("{when} · {status}"),
    }
}

fn ms(ms: u64) -> String {
    format::elapsed(std::time::Duration::from_millis(ms))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(status: Option<u16>, elapsed_ms: Option<u64>) -> TrendPoint {
        TrendPoint {
            sent_at_ms: 0,
            elapsed_ms,
            status,
        }
    }

    /// R-HIST-4: the summary gives the range of times and the failures.
    #[test]
    fn the_trend_is_summed_up() {
        let points = [
            point(Some(200), Some(80)),
            point(Some(500), Some(1200)),
            point(None, None),
        ];
        assert!(points[1].failed() && points[2].failed() && !points[0].failed());
        let text = summary(&points);
        assert!(text.starts_with("Last 3 sends: 80"), "{text}");
        assert!(text.ends_with(", 2 failed"), "{text}");
        assert_eq!(
            summary(&[point(None, None), point(None, None)]),
            "Last 2 sends, 2 failed"
        );
    }
}
