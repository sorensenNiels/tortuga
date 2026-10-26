//! Where a response's time went (R-HTTP-25, D68): the time in the response line opens
//! the send's phases as a small waterfall.

use std::time::Duration;

use egui::{CornerRadius, Grid, Popup, PopupCloseBehavior, Rect, RichText, Sense, Ui, vec2};
use tortuga_core::{Connection, Phase, Response, Timing};

use crate::{format, theme};

const BAR_WIDTH: f32 = 160.0;
const BAR_HEIGHT: f32 = 8.0;

/// The response's time; a button that opens the breakdown when the response has one.
pub fn time_chip(ui: &mut Ui, response: &Response) {
    let text = RichText::new(format::elapsed(response.elapsed)).color(theme::palette(ui).muted);
    let Some(timing) = response.timing else {
        ui.label(text).on_hover_text(t!("mock-time"));
        return;
    };
    let chip = ui
        .add(egui::Button::new(text).frame(false))
        .on_hover_text(t!("timing-hint"));
    Popup::from_toggle_button_response(&chip)
        .close_behavior(PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| breakdown(ui, &timing, response.elapsed));
}

fn breakdown(ui: &mut Ui, timing: &Timing, total: Duration) {
    let p = theme::palette(ui);
    theme::section_label(ui, &t!("timing-title"));
    ui.add_space(4.0);
    let mut start = Duration::ZERO;
    Grid::new("timing")
        .num_columns(3)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            for (phase, d) in timing.phases() {
                let name = ui.label(phase_name(phase));
                if phase == Phase::Waiting {
                    name.on_hover_text(t!("timing-waiting-hint"));
                }
                bar(ui, start, d, total);
                ui.label(RichText::new(format::phase(d)).monospace());
                ui.end_row();
                start += d;
            }
        });
    ui.add_space(4.0);
    let mut footer = vec![format!("{} {}", t!("timing-total"), format::elapsed(total))];
    footer.extend(timing.version.map(str::to_owned));
    match timing.connection {
        Connection::New => footer.push(t!("timing-new-connection")),
        Connection::Reused => footer.push(t!("timing-reused-connection")),
        Connection::Unknown => {}
    }
    ui.label(RichText::new(footer.join(" · ")).color(p.muted));
}

fn phase_name(phase: Phase) -> String {
    match phase {
        Phase::Dns => t!("timing-dns"),
        Phase::Connect => t!("timing-connect"),
        Phase::Waiting => t!("timing-waiting"),
        Phase::Download => t!("timing-download"),
    }
}

/// The phase as a bar that starts where the previous phases ended.
fn bar(ui: &mut Ui, start: Duration, d: Duration, total: Duration) {
    let p = theme::palette(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(BAR_WIDTH, BAR_HEIGHT), Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(2), p.surface_hover);
    let (from, to) = span(start, d, total);
    let left = rect.left() + from * BAR_WIDTH;
    // At least a sliver, so a fast phase still shows where it is.
    let right = (rect.left() + to * BAR_WIDTH).max(left + 1.0);
    let filled = Rect::from_x_y_ranges(left..=right, rect.y_range());
    ui.painter()
        .rect_filled(filled, CornerRadius::same(2), p.accent);
}

/// Where a phase starts and ends, as fractions of the total (0 to 1).
fn span(start: Duration, d: Duration, total: Duration) -> (f32, f32) {
    if total.is_zero() {
        return (0.0, 0.0);
    }
    let fraction = |x: Duration| (x.as_secs_f32() / total.as_secs_f32()).clamp(0.0, 1.0);
    (fraction(start), fraction(start + d))
}

#[cfg(test)]
mod tests {
    use egui_kittest::kittest::Queryable;

    use super::*;

    fn response(timing: Option<Timing>) -> Response {
        Response {
            status: 200,
            status_text: "OK".into(),
            headers: Vec::new(),
            body: Vec::new(),
            elapsed: Duration::from_millis(234),
            timing,
        }
    }

    fn timing(connection: Connection) -> Timing {
        Timing {
            dns: Some(Duration::from_millis(12)),
            connect: Some(Duration::from_millis(48)),
            waiting: Duration::from_millis(160),
            download: Duration::from_millis(14),
            connection,
            version: Some("HTTP/2"),
        }
    }

    #[test]
    fn spans_follow_each_other_and_stay_inside_the_bar() {
        let total = Duration::from_millis(200);
        assert_eq!(
            span(Duration::ZERO, Duration::from_millis(50), total),
            (0.0, 0.25)
        );
        assert_eq!(
            span(Duration::from_millis(50), Duration::from_millis(150), total),
            (0.25, 1.0)
        );
        // Rounding can make the phases a little longer than the total.
        assert_eq!(span(total, Duration::from_millis(1), total), (1.0, 1.0));
        assert_eq!(
            span(Duration::ZERO, Duration::ZERO, Duration::ZERO),
            (0.0, 0.0)
        );
    }

    /// R-HTTP-25: clicking the time opens the breakdown.
    #[test]
    fn the_time_opens_the_breakdown() {
        let res = response(Some(timing(Connection::New)));
        let mut h = crate::test_host::harness(|ui| time_chip(ui, &res));
        h.run();
        assert!(h.query_by_label("Connect + TLS").is_none());
        h.get_by_label("234 ms").click();
        h.run();
        for label in [
            "DNS lookup",
            "Connect + TLS",
            "Waiting for the server",
            "Download",
        ] {
            h.get_by_label(label);
        }
        h.get_by_label("48 ms");
        h.get_by_label("Total 234 ms · HTTP/2 · New connection");
    }

    /// R-HTTP-25: a reused connection had no lookup or connecting to show.
    #[test]
    fn a_reused_connection_shows_no_connecting() {
        let res = response(Some(timing(Connection::Reused)));
        let mut h = crate::test_host::harness(|ui| time_chip(ui, &res));
        h.run();
        h.get_by_label("234 ms").click();
        h.run();
        assert!(h.query_by_label("DNS lookup").is_none());
        assert!(h.query_by_label("Connect + TLS").is_none());
        h.get_by_label("Total 234 ms · HTTP/2 · Reused connection");
    }

    /// R-HTTP-25: a response from history has no breakdown, so its time is plain text.
    #[test]
    fn a_response_without_timing_is_not_a_button() {
        let res = response(None);
        let mut h = crate::test_host::harness(|ui| time_chip(ui, &res));
        h.run();
        h.get_by_label("234 ms").click();
        h.run();
        assert!(h.query_by_label("Waiting for the server").is_none());
    }
}
