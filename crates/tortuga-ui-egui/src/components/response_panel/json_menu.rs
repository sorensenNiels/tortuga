//! Tests and captures from the response (R-TEST-13): right-click a line of the formatted
//! JSON body to add a test for its value or capture it into a variable.

use std::ops::Range;

use egui::{RichText, Ui};
use tortuga_core::json::{Point, line_points};

use super::ResponseAction;
use crate::theme;

/// The value on each line of a formatted JSON body: worked out once asked for (the first
/// right-click), then kept for that text.
#[derive(Debug, Clone, Default)]
pub(super) enum JsonLines {
    #[default]
    NotAsked,
    Asked,
    Ready {
        /// The text's address and length.
        key: (usize, usize),
        /// `None` when the text is not formatted JSON.
        lines: Option<Vec<Option<Point>>>,
    },
}

impl JsonLines {
    /// A right-click asks for the values.
    pub(super) fn ask(&mut self) {
        if matches!(self, Self::NotAsked) {
            *self = Self::Asked;
        }
    }

    /// Works the values out for `text` if they were asked for and are not there yet.
    pub(super) fn update(&mut self, text: &str) {
        let key = (text.as_ptr() as usize, text.len());
        let current = match self {
            Self::NotAsked => return,
            Self::Asked => false,
            Self::Ready { key: k, .. } => *k == key,
        };
        if !current {
            *self = Self::Ready {
                key,
                lines: line_points(text),
            };
        }
    }

    pub(super) fn lines(&self) -> Option<&[Option<Point>]> {
        match self {
            Self::NotAsked | Self::Asked => None,
            Self::Ready { lines, .. } => lines.as_deref(),
        }
    }
}

/// The menu of the row at `range` of `text`. `lines` is `None` until the values have been
/// worked out (the frame after the first right-click).
pub(super) fn row_menu(
    ui: &mut Ui,
    text: &str,
    range: &Range<usize>,
    lines: Option<&[Option<Point>]>,
) -> Option<ResponseAction> {
    let Some(lines) = lines else {
        theme::hint(ui, "Reading the JSON…");
        return None;
    };
    // Wrapped rows are parts of a line: count the line breaks before this one.
    let line = text
        .get(..range.start)
        .map_or(0, |t| t.matches('\n').count());
    let Some(point) = lines.get(line).and_then(Option::as_ref) else {
        theme::hint(ui, "Right-click a line with a value.");
        return None;
    };
    menu(ui, point)
}

fn menu(ui: &mut Ui, point: &Point) -> Option<ResponseAction> {
    let p = theme::palette(ui);
    let mut action = None;
    // Menu entries keep to one line; the menu grows to fit them.
    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
    ui.label(RichText::new(point.display()).monospace().color(p.muted));
    ui.separator();
    ui.menu_button("Add test", |ui| {
        for (label, code) in point.tests() {
            if ui.button(label).clicked() {
                action = Some(ResponseAction::AddTest(code));
            }
        }
    });
    if !point.path.is_empty() {
        let name = point.variable_name();
        if ui
            .button(format!("Capture into “{name}”"))
            .on_hover_text(format!("Adds the capture {} = {}", name, point.json_path()))
            .clicked()
        {
            action = Some(ResponseAction::AddCapture {
                name,
                from: point.json_path(),
            });
        }
        if ui
            .button("Copy path")
            .on_hover_text(point.json_path())
            .clicked()
        {
            ui.ctx().copy_text(point.json_path());
        }
    }
    action
}

#[cfg(test)]
mod tests {
    use egui_kittest::kittest::Queryable;

    use super::*;

    const TEXT: &str = "{\n  \"data\": {\n    \"token\": \"t\"\n  }\n}";

    /// R-TEST-13: a line's menu offers tests and a capture for its value.
    #[test]
    fn a_line_offers_tests_and_a_capture() {
        let mut cache = JsonLines::NotAsked;
        cache.ask();
        cache.update(TEXT);
        let lines = cache.lines().map(<[_]>::to_vec);
        let token_line = TEXT.find("    \"token\"").expect("line");
        let range = token_line..token_line + 16;
        let mut action = None;
        let mut h = crate::test_host::harness(|ui| {
            if let Some(a) = row_menu(ui, TEXT, &range, lines.as_deref()) {
                action = Some(a);
            }
        });
        h.run();
        h.get_by_label("data.token");
        h.get_by_label("Add test").click();
        h.run();
        h.get_by_label("Is a string").click();
        h.run();
        drop(h);
        let Some(ResponseAction::AddTest(code)) = action else {
            panic!("no test added: {action:?}");
        };
        assert!(
            code.contains(r#"expect(res.json().data.token).to.be.a("string")"#),
            "{code}"
        );

        let mut action = None;
        let mut h = crate::test_host::harness(|ui| {
            if let Some(a) = row_menu(ui, TEXT, &range, lines.as_deref()) {
                action = Some(a);
            }
        });
        h.run();
        h.get_by_label("Capture into “token”").click();
        h.run();
        drop(h);
        assert_eq!(
            action,
            Some(ResponseAction::AddCapture {
                name: "token".into(),
                from: "$.data.token".into()
            })
        );
    }

    #[test]
    fn values_are_worked_out_when_asked_and_per_text() {
        let mut cache = JsonLines::NotAsked;
        cache.update(TEXT);
        assert!(cache.lines().is_none(), "not asked yet");
        cache.ask();
        cache.update(TEXT);
        assert!(cache.lines().is_some());
        let plain = "plain text".to_owned();
        cache.update(&plain);
        assert!(cache.lines().is_none(), "not JSON");
        cache.update("");
        assert!(cache.lines().is_none());
    }
}
