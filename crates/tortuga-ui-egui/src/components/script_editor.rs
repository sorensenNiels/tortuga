//! Editing scripts (R-TEST-1, R-TEST-2): a code field with line numbers, where clicking a
//! number sets or clears a breakpoint (R-TEST-10) and the last run's results show at the
//! end of their lines (R-TEST-11), with "Beautify" (R-TEST-7) and "Re-run tests" (R-TEST-12).

use std::collections::{BTreeMap, BTreeSet};

use egui::{Align2, Color32, FontId, Margin, Pos2, Rect, Sense, TextEdit, Ui};
use tortuga_core::script::{ConsoleEntry, ScriptKind};

use crate::{format, theme};

/// Width of the line numbers, inside the field's left edge.
const GUTTER: f32 = 34.0;
/// Longest error or failure text shown at the end of a line.
const MAX_MARK: usize = 120;

/// A script's heading, with a "Beautify" button at the right edge of the field below that
/// tidies `script` (R-TEST-7).
pub fn script_heading(ui: &mut Ui, label: &str, script: &mut String) {
    egui::Sides::new().show(
        ui,
        |ui| theme::section_label(ui, label),
        |ui| beautify_button(ui, script),
    );
}

/// What a script sub-tab shows above its field: when it runs, and its buttons. `rerun`
/// is `Some(enabled)` where the tests can run again on the last response (R-TEST-12);
/// returns whether that was clicked.
pub fn script_toolbar(ui: &mut Ui, about: &str, script: &mut String, rerun: Option<bool>) -> bool {
    egui::Sides::new()
        .show(
            ui,
            |ui| theme::hint(ui, about),
            |ui| {
                beautify_button(ui, script);
                rerun.is_some_and(|enabled| {
                    ui.add_enabled(enabled, egui::Button::new("Re-run tests").small())
                        .on_hover_text(
                            "Run the captures and tests again on the last response, without sending",
                        )
                        .on_disabled_hover_text("Send the request first: tests run on its response")
                        .clicked()
                })
            },
        )
        .1
}

fn beautify_button(ui: &mut Ui, script: &mut String) {
    let beautify = ui
        .add_enabled(
            !script.trim().is_empty(),
            egui::Button::new("Beautify").small(),
        )
        .on_hover_text("One statement per line, indented; comments and strings are kept");
    if beautify.clicked() {
        *script = tortuga_core::js_format::beautify(script);
    }
}

/// What the last run did on one line of a script (R-TEST-11).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LineMark {
    pub passed: usize,
    /// Why each failed test there failed.
    pub failed: Vec<String>,
    /// The error that stopped the script there.
    pub error: Option<String>,
    /// How often a breakpoint there was passed.
    pub hits: usize,
}

/// The marks of the script `kind` in a run's console, by line.
pub fn line_marks(
    console: &[(ScriptKind, ConsoleEntry)],
    kind: ScriptKind,
) -> BTreeMap<u32, LineMark> {
    let mut marks: BTreeMap<u32, LineMark> = BTreeMap::new();
    for (_, entry) in console.iter().filter(|(k, _)| *k == kind) {
        match entry {
            ConsoleEntry::Test {
                passed,
                message,
                line: Some(line),
                ..
            } => {
                let mark = marks.entry(*line).or_default();
                if *passed {
                    mark.passed += 1;
                } else {
                    mark.failed
                        .push(message.clone().unwrap_or_else(|| "failed".into()));
                }
            }
            ConsoleEntry::Error {
                message,
                line: Some(line),
            } => marks.entry(*line).or_default().error = Some(message.clone()),
            ConsoleEntry::Break { line, .. } => marks.entry(*line).or_default().hits += 1,
            ConsoleEntry::Test { line: None, .. }
            | ConsoleEntry::Error { line: None, .. }
            | ConsoleEntry::Log { .. }
            | ConsoleEntry::Note { .. } => {}
        }
    }
    marks
}

/// The text shown after a line, and whether it reports a failure.
fn mark_text(mark: &LineMark) -> (String, bool) {
    let mut parts = Vec::new();
    let failed = mark.error.is_some() || !mark.failed.is_empty();
    if let Some(e) = &mark.error {
        parts.push(format!("✗ {e}"));
    } else if let Some(first) = mark.failed.first() {
        let more = match mark.failed.len() {
            1 => String::new(),
            n => format!(" (and {} more)", n - 1),
        };
        parts.push(format!("✗ {first}{more}"));
        if mark.passed > 0 {
            parts.push(format!("{} passed", mark.passed));
        }
    } else if mark.passed == 1 {
        parts.push("✓".to_owned());
    } else if mark.passed > 1 {
        parts.push(format!("✓ {} passed", mark.passed));
    }
    match mark.hits {
        0 => {}
        1 => parts.push("⏺ 1 hit".to_owned()),
        n => parts.push(format!("⏺ {n} hits")),
    }
    let text = parts.join(" · ");
    (format::truncate(&text, MAX_MARK).0.to_owned(), failed)
}

/// A code field with line numbers. With `breakpoints`, clicking a line number sets or
/// clears a breakpoint there: that line is returned (R-TEST-10). `marks` are shown at the
/// end of their lines (R-TEST-11).
pub fn script_editor(
    ui: &mut Ui,
    id: &str,
    text: &mut String,
    rows: usize,
    hint: &str,
    breakpoints: Option<&BTreeSet<u32>>,
    marks: &BTreeMap<u32, LineMark>,
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
    let lines = line_positions(&out.galley, out.galley_pos);

    let p = theme::palette(ui);
    let painter = ui.painter_at(gutter);
    let font = FontId::monospace(10.5);
    for l in &lines {
        let set = breakpoints.is_some_and(|b| b.contains(&l.line));
        if set {
            painter.circle_filled(Pos2::new(gutter.left() + 8.0, l.y), 4.0, p.fail);
        }
        painter.text(
            Pos2::new(gutter.right() - 4.0, l.y),
            Align2::RIGHT_CENTER,
            l.line.to_string(),
            font.clone(),
            if set { p.fail } else { p.muted },
        );
    }
    paint_marks(ui, field, &lines, marks);

    breakpoints?;
    let click = ui
        .interact(gutter, ui.scope_id().with(("gutter", id)), Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text("Click a line number to set or clear a breakpoint");
    let y = click.interact_pointer_pos().filter(|_| click.clicked())?.y;
    // The line whose first row is nearest to the click.
    lines
        .iter()
        .min_by(|a, b| (a.y - y).abs().total_cmp(&(b.y - y).abs()))
        .map(|l| l.line)
}

/// The marks after the code of their lines, inside the field.
fn paint_marks(ui: &Ui, field: Rect, lines: &[LinePos], marks: &BTreeMap<u32, LineMark>) {
    let p = theme::palette(ui);
    let painter = ui.painter_at(field.shrink(2.0));
    let font = FontId::proportional(11.5);
    for l in lines {
        let Some(mark) = marks.get(&l.line) else {
            continue;
        };
        let (text, failed) = mark_text(mark);
        let color: Color32 = match (failed, mark.passed > 0) {
            (true, _) => p.fail,
            (false, true) => p.ok,
            (false, false) => p.muted,
        };
        painter.text(
            Pos2::new(l.end.x + 16.0, l.end.y),
            Align2::LEFT_CENTER,
            text,
            font.clone(),
            color,
        );
    }
}

/// Where a line of the field is: the centre of its first row, for its number, and the
/// end of its last row, for its mark.
#[derive(Debug, Clone, Copy)]
struct LinePos {
    line: u32,
    y: f32,
    end: Pos2,
}

/// The lines of `galley`, drawn at `origin`. Wrapped rows belong to the line they continue.
fn line_positions(galley: &egui::Galley, origin: Pos2) -> Vec<LinePos> {
    let mut out: Vec<LinePos> = Vec::new();
    let mut line = 1;
    let mut starts_line = true;
    for row in &galley.rows {
        let rect = row.rect().translate(origin.to_vec2());
        let end = Pos2::new(rect.right(), rect.center().y);
        if starts_line {
            out.push(LinePos {
                line,
                y: rect.center().y,
                end,
            });
        } else if let Some(last) = out.last_mut() {
            last.end = end;
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

    /// R-TEST-11: tests, errors and breakpoint hits become marks on their lines, for the
    /// script they ran in.
    #[test]
    fn the_console_becomes_marks_on_lines() {
        let test = |passed: bool, line| ConsoleEntry::Test {
            name: "t".into(),
            passed,
            message: (!passed).then(|| "Expected 1 to be 2".to_owned()),
            line: Some(line),
        };
        let console = vec![
            (ScriptKind::PreRequest, test(false, 1)),
            (ScriptKind::Tests, test(true, 2)),
            (ScriptKind::Tests, test(true, 4)),
            (ScriptKind::Tests, test(true, 4)),
            (ScriptKind::Tests, test(false, 4)),
            (
                ScriptKind::Tests,
                ConsoleEntry::Break {
                    line: 3,
                    values: vec![],
                },
            ),
            (
                ScriptKind::Tests,
                ConsoleEntry::Break {
                    line: 3,
                    values: vec![],
                },
            ),
            (
                ScriptKind::Tests,
                ConsoleEntry::Error {
                    message: "TypeError: ü😀".into(),
                    line: Some(5),
                },
            ),
        ];
        let marks = line_marks(&console, ScriptKind::Tests);
        assert_eq!(marks.keys().copied().collect::<Vec<_>>(), [2, 3, 4, 5]);
        assert_eq!(mark_text(&marks[&2]), ("✓".to_owned(), false));
        assert_eq!(mark_text(&marks[&3]), ("⏺ 2 hits".to_owned(), false));
        assert_eq!(
            mark_text(&marks[&4]),
            ("✗ Expected 1 to be 2 · 2 passed".to_owned(), true)
        );
        assert_eq!(mark_text(&marks[&5]), ("✗ TypeError: ü😀".to_owned(), true));
        assert_eq!(line_marks(&console, ScriptKind::PreRequest).len(), 1);
        let long = LineMark {
            error: Some("x".repeat(500)),
            ..LineMark::default()
        };
        assert!(mark_text(&long).0.chars().count() <= MAX_MARK + 1);
    }

    /// R-TEST-10: clicking a line number returns that line, once per click.
    #[test]
    fn clicking_a_line_number_toggles_its_breakpoint() {
        let mut text = "const a = 1;\nconst b = 2;\nconst c = 3;".to_owned();
        let mut breakpoints = BTreeSet::new();
        let mut h = crate::test_host::harness(|ui| {
            if let Some(line) = script_editor(
                ui,
                "t",
                &mut text,
                4,
                "script",
                Some(&breakpoints),
                &BTreeMap::new(),
            ) && !breakpoints.remove(&line)
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
