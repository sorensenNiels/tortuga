//! Request and response, stacked or side by side, with a divider between them
//! (R-SET-12). Stacked, the request takes the height its tab needs, at most the height
//! the divider was dragged to; double-clicking the divider gives the response all of it.

use egui::{Rect, Sense, Ui};
use tortuga_core::settings::RequestLayout;

use super::TortugaUi;
use crate::components::EditorView;
use crate::host::SavedSplit;
use crate::theme;

/// Stacked: the request's height until the divider is first dragged.
const DEFAULT_REQUEST_HEIGHT: f32 = 300.0;
/// Stacked: the request never gets less than this, nor the response.
const MIN_REQUEST_HEIGHT: f32 = 120.0;
const MIN_RESPONSE_HEIGHT: f32 = 160.0;
/// Side by side: the request's share until the divider is first dragged, and the least
/// width either side gets.
const DEFAULT_REQUEST_SHARE: f32 = 0.5;
const MIN_COLUMN_WIDTH: f32 = 360.0;
/// "Automatic" puts them side by side from this width of the main area on.
const SIDE_BY_SIDE_FROM: f32 = 1100.0;
/// The divider's thickness; the line is drawn in its middle.
pub(super) const DIVIDER: f32 = 13.0;

/// How request and response are arranged this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Arrangement {
    Stacked,
    SideBySide,
}

/// What the divider shows while the app runs.
#[derive(Debug, Default)]
pub(super) struct Split {
    /// Stacked: only the URL line of the request shows, the response has the rest.
    pub response_full: bool,
}

impl TortugaUi {
    /// The arrangement for a main area `width` wide, as the settings ask (R-SET-12).
    pub(super) fn arrangement(&self, width: f32) -> Arrangement {
        match self.state.settings.appearance.layout {
            RequestLayout::SideBySide => Arrangement::SideBySide,
            RequestLayout::Automatic if width >= SIDE_BY_SIDE_FROM => Arrangement::SideBySide,
            RequestLayout::Stacked | RequestLayout::Automatic => Arrangement::Stacked,
        }
    }

    /// How much of the request editor shows in `arrangement`.
    pub(super) fn editor_view(&self, arrangement: Arrangement) -> EditorView {
        if self.split.response_full && arrangement == Arrangement::Stacked {
            EditorView::UrlOnly
        } else {
            EditorView::Full
        }
    }

    /// Stacked: the most height the request may take of `available`.
    pub(super) fn request_cap(&self, available: f32) -> f32 {
        let wanted = self
            .state
            .remembered_split()
            .request_height
            .map_or(DEFAULT_REQUEST_HEIGHT, f32::from);
        clamp_between(wanted, MIN_REQUEST_HEIGHT, available - MIN_RESPONSE_HEIGHT)
    }

    /// Side by side: the request's width of `available`.
    pub(super) fn request_width(&self, available: f32) -> f32 {
        let share = self
            .state
            .remembered_split()
            .request_share
            .map_or(DEFAULT_REQUEST_SHARE, |s| f32::from(s) / 1000.0);
        clamp_between(
            available * share,
            MIN_COLUMN_WIDTH,
            available - MIN_COLUMN_WIDTH - DIVIDER,
        )
    }

    /// Stacked: the divider below a request `used` points tall, of `available` in all.
    /// Dragging sets the request's height, double-clicking gives the response all of it.
    pub(super) fn stacked_divider(&mut self, ui: &mut Ui, used: f32, available: f32) {
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), DIVIDER), Sense::hover());
        let hint = if self.split.response_full {
            "Drag to change the request's height. Double-click to show the request again."
        } else {
            "Drag to change the request's height. Double-click to give the response all of it."
        };
        let response = divider(ui, rect, false).on_hover_text(hint);
        if response.double_clicked() {
            self.split.response_full = !self.split.response_full;
        } else if response.dragged() {
            self.split.response_full = false;
            let height = clamp_between(
                used + response.drag_delta().y,
                MIN_REQUEST_HEIGHT,
                available - MIN_RESPONSE_HEIGHT,
            );
            self.state.set_split(SavedSplit {
                request_height: Some(points(height)),
                ..self.state.remembered_split()
            });
        }
        if response.drag_stopped() {
            self.state.save_local();
        }
    }

    /// Side by side: the divider in `rect`, between columns `available` wide in all.
    pub(super) fn side_divider(&mut self, ui: &Ui, rect: Rect, left: f32, available: f32) {
        let response = divider(ui, rect, true)
            .on_hover_text("Drag to change the widths. Double-click for equal widths.");
        let share = if response.double_clicked() {
            Some(DEFAULT_REQUEST_SHARE)
        } else if response.dragged() && available > 0.0 {
            Some((left + response.drag_delta().x) / available)
        } else {
            None
        };
        if let Some(share) = share {
            self.state.set_split(SavedSplit {
                request_share: Some(thousandths(share)),
                ..self.state.remembered_split()
            });
        }
        if response.drag_stopped() || response.double_clicked() {
            self.state.save_local();
        }
    }
}

/// A divider in `rect`: a line across its middle, `vertical` or not, that highlights
/// on hover and can be dragged and double-clicked.
fn divider(ui: &Ui, rect: Rect, vertical: bool) -> egui::Response {
    let id = ui.make_persistent_id("request-response-divider");
    let response = ui.interact(rect, id, Sense::click_and_drag());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::accesskit::Role::Splitter,
            true,
            "Request and response divider",
        )
    });
    let p = theme::palette(ui);
    let active = response.hovered() || response.dragged();
    let stroke = if active {
        egui::Stroke::new(2.0, p.accent)
    } else {
        egui::Stroke::new(1.0, p.border)
    };
    if vertical {
        ui.painter().vline(rect.center().x, rect.y_range(), stroke);
    } else {
        ui.painter().hline(rect.x_range(), rect.center().y, stroke);
    }
    if active {
        ui.ctx().set_cursor_icon(if vertical {
            egui::CursorIcon::ResizeHorizontal
        } else {
            egui::CursorIcon::ResizeVertical
        });
    }
    response
}

/// `value` within `min` and `max`; `min` wins when there is no room for it.
fn clamp_between(value: f32, min: f32, max: f32) -> f32 {
    value.min(max).max(min)
}

/// A height in whole points, as local data keeps it.
fn points(height: f32) -> u16 {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "clamped to 0..=u16::MAX first"
    )]
    let points = height.round().clamp(0.0, f32::from(u16::MAX)) as u16;
    points
}

/// A share in thousandths, as local data keeps it.
fn thousandths(share: f32) -> u16 {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "clamped to 0..=1000 first"
    )]
    let thousandths = (share * 1000.0).round().clamp(0.0, 1000.0) as u16;
    thousandths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_stay_within_their_limits() {
        assert!((clamp_between(50.0, 120.0, 600.0) - 120.0).abs() < f32::EPSILON);
        assert!((clamp_between(900.0, 120.0, 600.0) - 600.0).abs() < f32::EPSILON);
        assert!(
            (clamp_between(300.0, 120.0, 80.0) - 120.0).abs() < f32::EPSILON,
            "a window too small keeps the minimum"
        );
        assert_eq!(points(299.6), 300);
        assert_eq!(points(-5.0), 0);
        assert_eq!(thousandths(0.4567), 457);
        assert_eq!(thousandths(3.0), 1000);
    }

    /// Runs the whole app in a 1200 × 900 window, at 20 frames a second so that two
    /// clicks (a press and a release a frame each) are quick enough for a double click.
    fn app_harness(ui: &mut TortugaUi) -> egui_kittest::Harness<'_> {
        ui.styled = false;
        ui.applied_appearance = None;
        egui_kittest::Harness::builder()
            .with_size(egui::vec2(1200.0, 900.0))
            .with_step_dt(0.05)
            .build_ui(|egui_ui| ui.ui(egui_ui))
    }

    /// Double-clicks the divider with pointer events, as kittest's `click` can't.
    fn double_click(h: &mut egui_kittest::Harness<'_>) {
        use egui_kittest::kittest::Queryable;

        // Half a second after any earlier click, so this isn't a triple click. Moving the
        // pointer makes each frame count.
        for i in 0..10_u8 {
            h.hover_at(egui::pos2(5.0, 5.0 + f32::from(i)));
            h.step();
        }
        let at = h
            .get_by_label("Request and response divider")
            .rect()
            .center();
        h.hover_at(at);
        h.step();
        for _ in 0..2 {
            h.drag_at(at);
            h.event(egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            });
        }
        h.run();
    }

    /// R-SET-12: stacked, an empty Params tab leaves the room to the response; dragging
    /// the divider sets the request's height, written to local data when let go;
    /// double-clicking gives the response all of it, and again brings the tabs back.
    #[test]
    fn the_divider_sizes_the_request() {
        use egui_kittest::kittest::Queryable;

        let (mut ui, root, _, local, _) = crate::shell::tests::setup_shared("layout-stacked");
        {
            let mut h = app_harness(&mut ui);
            h.run();
            let url_top = h.get_by_label("Params").rect().top();
            let divider = h.get_by_label("Request and response divider").rect();
            let used = divider.top() - url_top;
            assert!(used < 150.0, "an empty table takes little room: {used}");

            let from = divider.center();
            h.hover_at(from);
            h.run();
            h.drag_at(from);
            h.step();
            h.hover_at(from + egui::vec2(0.0, 200.0));
            h.step();
            h.drop_at(from + egui::vec2(0.0, 200.0));
            h.run();

            double_click(&mut h);
            assert!(h.query_by_label("Params").is_none(), "only the URL line");
            double_click(&mut h);
            h.get_by_label("Params");
        }
        let saved = local.borrow().as_ref().expect("local data written").split;
        assert_eq!(saved, ui.state.remembered_split(), "written when let go");
        let height = ui.state.remembered_split().request_height.expect("dragged");
        assert!(height > 250, "the drag grew the request's height: {height}");
        std::fs::remove_dir_all(root).expect("clean up");
    }

    /// R-SET-12: side by side, the response is to the right of the request.
    #[test]
    fn side_by_side_puts_the_response_beside_the_request() {
        use egui_kittest::kittest::Queryable;

        let (mut ui, root, _) = crate::shell::tests::setup("layout-side");
        ui.state
            .update_settings(|s| s.appearance.layout = RequestLayout::SideBySide);
        let mut h = app_harness(&mut ui);
        h.run();
        let params = h.get_by_label("Params").rect();
        let empty = h
            .get_by_label("Send a request to see the response here.")
            .rect();
        assert!(
            empty.left() > params.right() + 200.0,
            "{empty:?} beside {params:?}"
        );
        drop(h);
        std::fs::remove_dir_all(root).expect("clean up");
    }

    /// R-SET-12: automatic is stacked in a narrow window, side by side in a wide one.
    #[test]
    fn automatic_follows_the_width() {
        let (mut ui, root, _) = crate::shell::tests::setup("layout-auto");
        ui.state
            .update_settings(|s| s.appearance.layout = RequestLayout::Automatic);
        assert_eq!(ui.arrangement(900.0), Arrangement::Stacked);
        assert_eq!(ui.arrangement(1400.0), Arrangement::SideBySide);
        std::fs::remove_dir_all(root).expect("clean up");
    }

    /// R-COL-26: Ctrl+B hides the sidebar and the ☰ button brings it back; the choice is
    /// kept with the workspace.
    #[test]
    fn the_sidebar_hides_and_comes_back() {
        use egui_kittest::kittest::Queryable;

        let (mut ui, root, _, local, _) = crate::shell::tests::setup_shared("sidebar-hidden");
        {
            let mut h = app_harness(&mut ui);
            h.run();
            h.get_by_label("History");
            assert!(
                h.query_by_label("PROJECTS").is_none(),
                "the tabs name the view"
            );
            h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::B);
            h.run();
            assert!(h.query_by_label("History").is_none(), "hidden");
        }
        let saved = local.borrow().clone().expect("written");
        assert!(saved.workspaces[saved.active_workspace].sidebar_hidden);
        {
            let mut h = app_harness(&mut ui);
            h.run();
            assert!(h.query_by_label("History").is_none(), "still hidden");
            h.get_by_label("☰").click();
            h.run();
            h.get_by_label("History");
        }
        assert!(!ui.state.sidebar_hidden());
        std::fs::remove_dir_all(root).expect("clean up");
    }
}
