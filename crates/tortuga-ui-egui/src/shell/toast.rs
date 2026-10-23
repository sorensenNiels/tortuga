//! Messages that need no action (R-COL-28): shown for a moment in the bottom-right
//! corner, then they fade. A click dismisses one. Problems keep the banner instead.

use egui::{Align2, Area, Frame, Id, Margin, Order, RichText, Sense};

use super::TortugaUi;
use crate::theme;

/// How long a message shows, and how much of that it takes to fade.
const SHOWN_SECONDS: f64 = 4.0;
const FADE_SECONDS: f64 = 0.6;

impl TortugaUi {
    /// Draws the message, if there is one, over everything else.
    pub(super) fn toast(&mut self, ctx: &egui::Context) {
        let Some(toast) = self.state.toast.as_mut() else {
            return;
        };
        let now = ctx.input(|i| i.time);
        let shown_at = *toast.shown_at.get_or_insert(now);
        let left = SHOWN_SECONDS - (now - shown_at);
        if left <= 0.0 {
            self.state.toast = None;
            return;
        }
        let opacity = (left / FADE_SECONDS).min(1.0);
        // Again when it starts to fade, then every frame while it does.
        let wait = if left > FADE_SECONDS {
            left - FADE_SECONDS
        } else {
            0.0
        };
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(wait));
        let text = toast.text.clone();
        let clicked = Area::new(Id::unique("toast"))
            .order(Order::Tooltip)
            .anchor(Align2::RIGHT_BOTTOM, [-16.0, -16.0])
            .interactable(true)
            .show(ctx, |ui| {
                #[expect(clippy::cast_possible_truncation, reason = "between 0 and 1")]
                ui.set_opacity(opacity as f32);
                let p = theme::palette(ui);
                Frame::popup(ui.style())
                    .inner_margin(Margin::symmetric(14, 10))
                    .show(ui, |ui| {
                        ui.set_max_width(420.0);
                        ui.label(RichText::new(text).color(p.text));
                    })
                    .response
                    .interact(Sense::click())
                    .on_hover_text(t!("toast-dismiss"))
                    .clicked()
            })
            .inner;
        if clicked {
            self.state.toast = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use egui_kittest::kittest::Queryable;

    use super::*;
    use crate::test_host::TestHost;

    /// R-COL-28: a message shows for a few seconds and goes; a problem stays in the
    /// banner; a click dismisses a message early.
    #[test]
    fn messages_fade_and_problems_stay() {
        let mut ui = TortugaUi::new(Box::new(TestHost::default()));
        ui.state.inform("mock server stopped ü😀");
        ui.state.notify("Could not save cookies");
        {
            let mut h = egui_kittest::Harness::builder()
                .with_step_dt(0.5)
                .build_ui(|egui_ui| ui.ui(egui_ui));
            h.step();
            h.get_by_label("Mock server stopped ü😀");
            for _ in 0..10 {
                h.step();
            }
            assert!(
                h.query_by_label("Mock server stopped ü😀").is_none(),
                "gone"
            );
            h.get_by_label("Could not save cookies");
        }
        ui.state.inform("History cleared");
        ui.styled = false;
        let mut h = egui_kittest::Harness::builder().build_ui(|egui_ui| ui.ui(egui_ui));
        h.step();
        h.get_by_label("History cleared").click();
        h.step();
        h.step();
        assert!(h.query_by_label("History cleared").is_none(), "dismissed");
    }
}
