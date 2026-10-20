//! Editing scripts (R-TEST-1, R-TEST-2): a code field with line numbers, where clicking a
//! number sets or clears a breakpoint (R-TEST-10), under a heading with "Beautify" (R-TEST-7).

use std::collections::BTreeSet;

use egui::{Align2, FontId, Margin, Pos2, Rect, Sense, TextEdit, Ui};

use crate::theme;

/// Width of the line numbers, inside the field's left edge.
const GUTTER: f32 = 34.0;

/// A script's heading, with a "Beautify" button at the right edge of the field below that
/// tidies `script` (R-TEST-7).
pub fn script_heading(ui: &mut Ui, label: &str, script: &mut String) {
    egui::Sides::new().show(
        ui,
        |ui| theme::section_label(ui, label),
        |ui| {
            let beautify = ui
                .add_enabled(
                    !script.trim().is_empty(),
                    egui::Button::new("Beautify").small(),
                )
                .on_hover_text("One statement per line, indented; comments and strings are kept");
            if beautify.clicked() {
                *script = tortuga_core::js_format::beautify(script);
            }
        },
    );
}

/// A code field with line numbers. With `breakpoints`, clicking a line number sets or
/// clears a breakpoint there: that line is returned (R-TEST-10).
pub fn script_editor(
    ui: &mut Ui,
    id: &str,
    text: &mut String,
    rows: usize,
    hint: &str,
    breakpoints: Option<&BTreeSet<u32>>,
) -> Option<u32> {
    let out = TextEdit::multiline(text)
        .id_salt(("script", id))
        .code_editor()
        .desired_width(f32::INFINITY)
        .desired_rows(rows)
        .hint_text(hint)
        .margin(Margin {
            left: 40,
            right: 6,
            top: 4,
            bottom: 4,
        })
        .show(ui);
    let field = out.response.response.rect;
    let gutter = Rect::from_min_max(field.min, Pos2::new(field.min.x + GUTTER, field.max.y));
    let lines = line_positions(&out.galley, out.galley_pos.y);

    let p = theme::palette(ui);
    let painter = ui.painter_at(gutter);
    let font = FontId::monospace(10.5);
    for &(line, y) in &lines {
        let set = breakpoints.is_some_and(|b| b.contains(&line));
        if set {
            painter.circle_filled(Pos2::new(gutter.left() + 8.0, y), 4.0, p.fail);
        }
        painter.text(
            Pos2::new(gutter.right() - 4.0, y),
            Align2::RIGHT_CENTER,
            line.to_string(),
            font.clone(),
            if set { p.fail } else { p.muted },
        );
    }

    breakpoints?;
    let click = ui
        .interact(gutter, ui.scope_id().with(("gutter", id)), Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text("Click a line number to set or clear a breakpoint");
    let y = click.interact_pointer_pos().filter(|_| click.clicked())?.y;
    // The line whose row is nearest to the click.
    lines
        .iter()
        .min_by(|a, b| (a.1 - y).abs().total_cmp(&(b.1 - y).abs()))
        .map(|&(line, _)| line)
}

/// (line number, screen y of its first row) for each line of `galley`, which starts at
/// `top`. Wrapped rows belong to the line they continue.
fn line_positions(galley: &egui::Galley, top: f32) -> Vec<(u32, f32)> {
    let mut out = Vec::new();
    let mut line = 1;
    let mut starts_line = true;
    for row in &galley.rows {
        if starts_line {
            out.push((line, top + row.rect().center().y));
        }
        starts_line = row.ends_with_newline;
        if row.ends_with_newline {
            line += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use egui_kittest::kittest::Queryable;

    use super::*;

    /// R-TEST-10: clicking a line number returns that line, once per click.
    #[test]
    fn clicking_a_line_number_toggles_its_breakpoint() {
        let mut text = "const a = 1;\nconst b = 2;\nconst c = 3;".to_owned();
        let mut breakpoints = BTreeSet::new();
        let mut h = crate::test_host::harness(|ui| {
            if let Some(line) = script_editor(ui, "t", &mut text, 4, "script", Some(&breakpoints))
                && !breakpoints.remove(&line)
            {
                breakpoints.insert(line);
            }
        });
        h.run();
        let field = h
            .get_all_by_role(egui::accesskit::Role::MultilineTextInput)
            .next()
            .expect("field")
            .rect();
        // The second line, in the gutter.
        let row = (field.height() - 8.0) / 4.0;
        let at = egui::pos2(field.left() + 12.0, field.top() + 4.0 + row * 1.5);
        h.input_mut().events.push(egui::Event::PointerMoved(at));
        h.run();
        for pressed in [true, false] {
            h.input_mut().events.push(egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::default(),
            });
        }
        h.run();
        drop(h);
        assert_eq!(breakpoints.into_iter().collect::<Vec<_>>(), [2]);
    }
}
