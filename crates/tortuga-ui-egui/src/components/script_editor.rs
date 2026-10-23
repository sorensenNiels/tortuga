//! Editing scripts (R-TEST-1, R-TEST-2): a code field with line numbers, where clicking a
//! number sets or clears a breakpoint (R-TEST-10) and the last run's results show at the
//! end of their lines (R-TEST-11), with "Beautify" (R-TEST-7), "Re-run tests" (R-TEST-12),
//! editing keys and colours (R-TEST-14), snippets (R-TEST-15), and line commands, pairs and
//! a menu on the field (R-TEST-17).

use std::collections::{BTreeMap, BTreeSet};

use egui::text::{CCursor, CCursorRange, LayoutJob, TextFormat};
use egui::{
    Align2, Color32, FontId, Id, Key, KeyboardShortcut, Margin, Modifiers, Pos2, Rect, Sense,
    TextEdit, Ui,
};
use tortuga_core::js_format::{Token, scan};
use tortuga_core::script::{ConsoleEntry, ScriptKind};
use tortuga_core::settings::ScriptCommand;

use super::script_editing::{self, Direction, Edited};
use super::script_keys::{self, ScriptKeys};
use crate::{format, theme};

/// Keys of a script field that can't be changed (R-TEST-14), also listed on the shortcut
/// sheet (R-COL-27). The others come from the settings (R-SET-16).
pub(crate) const INDENT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::NONE, Key::Tab);
pub(crate) const OUTDENT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::SHIFT, Key::Tab);

/// Longest script that is coloured; longer ones are shown plain, so typing stays quick.
const MAX_COLOURED: usize = 100_000;

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

/// The id of the code field `id`, to reach its cursor.
fn field_id(ui: &Ui, id: &str) -> Id {
    ui.make_persistent_id(("script", id))
}

/// A snippet's menu label, in the UI's language, and its code (R-TEST-15).
pub type Snippet = (fn() -> String, &'static str);

/// What a script sub-tab shows above the field `id`: when it runs, and its buttons:
/// Beautify, "Snippets" (R-TEST-15) and, where `rerun` is `Some(enabled)`, "Re-run
/// tests" (R-TEST-12). Returns whether that was clicked.
pub fn script_toolbar(
    ui: &mut Ui,
    about: &str,
    id: &str,
    script: &mut String,
    snippets: &[Snippet],
    rerun: Option<bool>,
) -> bool {
    let field = field_id(ui, id);
    egui::Sides::new()
        .show(
            ui,
            |ui| theme::hint(ui, about),
            |ui| {
                beautify_button(ui, script);
                ui.menu_button(t!("script-snippets"), |ui| {
                    for (label, code) in snippets {
                        if ui.button(label()).clicked() {
                            insert_snippet(ui, field, script, code);
                        }
                    }
                });
                rerun.is_some_and(|enabled| {
                    ui.add_enabled(enabled, egui::Button::new(t!("script-rerun")).small())
                        .on_hover_text(t!("script-rerun-hint"))
                        .on_disabled_hover_text(t!("script-rerun-disabled"))
                        .clicked()
                })
            },
        )
        .1
}

/// Puts `code` on lines of its own after the cursor's line, or at the end (R-TEST-15).
fn insert_snippet(ui: &Ui, field: Id, script: &mut String, code: &str) {
    let mut state = TextEdit::load_state(ui.ctx(), field).unwrap_or_default();
    let cursor = state.cursor.char_range().map(|r| r.primary.index.0);
    let edited = script_editing::insert_block(script, cursor, code);
    apply(&mut state, script, edited);
    TextEdit::store_state(ui.ctx(), field, state);
    ui.memory_mut(|m| m.request_focus(field));
}

/// Takes the edit into the script and moves the field's cursor.
fn apply(state: &mut egui::text_edit::TextEditState, script: &mut String, edited: Edited) {
    *script = edited.text;
    let (start, end) = edited.selection;
    state.cursor.set_char_range(Some(CCursorRange::two(
        CCursor::new(start),
        CCursor::new(end),
    )));
}

/// The selection of a field state as (cursor, anchor).
fn selection_of(state: &egui::text_edit::TextEditState) -> (usize, usize) {
    let range = state.cursor.char_range().unwrap_or_default();
    (range.primary.index.0, range.secondary.index.0)
}

/// What `command` makes of `text` (R-TEST-17).
fn command_edit(command: ScriptCommand, text: &str, selection: (usize, usize)) -> Edited {
    match command {
        ScriptCommand::ToggleComment => script_editing::toggle_comment(text, selection),
        ScriptCommand::BlockComment => script_editing::block_comment(text, selection),
        ScriptCommand::MoveLinesUp => script_editing::move_lines(text, selection, Direction::Up),
        ScriptCommand::MoveLinesDown => {
            script_editing::move_lines(text, selection, Direction::Down)
        }
        ScriptCommand::CopyLinesUp => script_editing::copy_lines(text, selection, Direction::Up),
        ScriptCommand::CopyLinesDown => {
            script_editing::copy_lines(text, selection, Direction::Down)
        }
        ScriptCommand::DeleteLines => script_editing::delete_lines(text, selection),
        ScriptCommand::IndentLines => script_editing::indent_lines(text, selection),
        ScriptCommand::OutdentLines => script_editing::outdent(text, selection),
        ScriptCommand::Beautify => script_editing::beautify(text, selection),
    }
}

/// The keys of the focused field `field`: the commands' keys from the settings, Tab,
/// Shift+Tab and Enter (R-TEST-14), Home, Backspace and paired brackets (R-TEST-17).
/// Runs before the field, which then does not see these keys.
fn editing_keys(ui: &Ui, field: Id, script: &mut String) {
    if !ui.memory(|m| m.has_focus(field)) {
        return;
    }
    script_keys::note_focus(ui.ctx(), field);
    let keys = script_keys::current(ui.ctx());
    let mut state = TextEdit::load_state(ui.ctx(), field).unwrap_or_default();
    let selection = selection_of(&state);
    let edited = ui.input_mut(|i| typed_edit(i, &keys, script, selection));
    if let Some(edited) = edited {
        apply(&mut state, script, edited);
        TextEdit::store_state(ui.ctx(), field, state);
        return;
    }
    // Home moves the cursor only; with Shift the other end of the selection stays.
    let home = ui.input_mut(|i| {
        if i.consume_key(Modifiers::SHIFT, Key::Home) {
            Some(true)
        } else {
            i.consume_key(Modifiers::NONE, Key::Home).then_some(false)
        }
    });
    if let Some(extend) = home {
        let (cursor, anchor) = selection;
        let to = script_editing::smart_home(script, cursor);
        let anchor = if extend { anchor } else { to };
        state.cursor.set_char_range(Some(CCursorRange::two(
            CCursor::new(anchor),
            CCursor::new(to),
        )));
        TextEdit::store_state(ui.ctx(), field, state);
    }
}

/// The edit a key or typed character this frame asks for, taking it from `i`.
fn typed_edit(
    i: &mut egui::InputState,
    keys: &ScriptKeys,
    script: &str,
    selection: (usize, usize),
) -> Option<Edited> {
    // The commands first: Tab and Enter below would also match them with Shift or Alt.
    if let Some(command) = keys.take_pressed(i) {
        return Some(command_edit(command, script, selection));
    }
    // Shift is ignored by `consume_key` unless asked for, so Shift+Tab goes first.
    if i.consume_shortcut(&OUTDENT) {
        return Some(script_editing::outdent(script, selection));
    }
    if i.consume_shortcut(&INDENT) {
        return Some(script_editing::indent(script, selection));
    }
    if i.consume_key(Modifiers::NONE, Key::Enter) {
        return Some(script_editing::newline(script, selection));
    }
    if !keys.close_pairs() {
        return None;
    }
    if i.key_pressed(Key::Backspace) && i.modifiers.is_none() {
        let edited = script_editing::backspace_pair(script, selection)?;
        i.consume_key(Modifiers::NONE, Key::Backspace);
        return Some(edited);
    }
    // One character typed this frame: with more, pairing it could put things out of order.
    let mut texts = i.events.iter().enumerate().filter_map(|(n, e)| match e {
        egui::Event::Text(t) => Some((n, t)),
        _ => None,
    });
    let (n, typed) = texts.next()?;
    if texts.next().is_some() {
        return None;
    }
    let mut chars = typed.chars();
    let (Some(c), None) = (chars.next(), chars.next()) else {
        return None;
    };
    let edited = script_editing::type_paired(script, selection, c)?;
    i.events.remove(n);
    Some(edited)
}

/// The menu on a script field (R-TEST-17): Cut, Copy, Paste and the line commands, each
/// with its keys.
fn field_menu(ui: &mut Ui, field: Id, script: &mut String, keys: &ScriptKeys) {
    let mac = ui.ctx().os() == egui::os::OperatingSystem::Mac;
    let mut state = TextEdit::load_state(ui.ctx(), field).unwrap_or_default();
    let selection = selection_of(&state);
    let picked = script_editing::selected(script, selection).to_owned();
    let item = |ui: &mut Ui, label: &str, keys: String, enabled: bool| {
        ui.add_enabled(enabled, egui::Button::new(label).shortcut_text(keys))
            .clicked()
    };
    let label = |k: KeyboardShortcut| script_keys::key_label(k, mac);
    let mut edited = None;
    if item(ui, &t!("cut"), label(CUT), !picked.is_empty()) {
        ui.ctx().copy_text(picked.clone());
        edited = Some(script_editing::replace_selection(script, selection, ""));
    }
    if item(ui, &t!("copy"), label(COPY), !picked.is_empty()) {
        ui.ctx().copy_text(picked);
    }
    if item(ui, &t!("paste"), label(PASTE), true) {
        // The pasted text arrives as an event for the focused field.
        ui.memory_mut(|m| m.request_focus(field));
        ui.ctx()
            .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
    }
    ui.separator();
    for command in ScriptCommand::ALL {
        let enabled = command != ScriptCommand::Beautify || !script.trim().is_empty();
        let shown = script_keys::keys_label(keys, command, mac);
        if item(ui, &script_keys::command_name(command), shown, enabled) {
            edited = Some(command_edit(command, script, selection));
        }
    }
    if let Some(edited) = edited {
        apply(&mut state, script, edited);
        TextEdit::store_state(ui.ctx(), field, state);
        ui.memory_mut(|m| m.request_focus(field));
    }
}

/// The menu's clipboard keys, as the field itself handles them.
const CUT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::X);
const COPY: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::C);
const PASTE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::V);

/// A right-click moves the field's cursor to the click. One inside the selection keeps the
/// selection, so the menu works on it.
fn keep_selection_on_right_click(
    ui: &Ui,
    field: Id,
    response: &egui::Response,
    before: (usize, usize),
) {
    let right = ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Secondary));
    if !right || !response.hovered() || before.0 == before.1 {
        return;
    }
    let Some(mut state) = TextEdit::load_state(ui.ctx(), field) else {
        return;
    };
    let at = selection_of(&state).0;
    let (start, end) = (before.0.min(before.1), before.0.max(before.1));
    if (start..=end).contains(&at) {
        state.cursor.set_char_range(Some(CCursorRange::two(
            CCursor::new(before.1),
            CCursor::new(before.0),
        )));
        TextEdit::store_state(ui.ctx(), field, state);
    }
}

/// `text` laid out with keywords, strings, numbers, comments and regular expressions in
/// colour (R-TEST-14).
fn coloured(ui: &Ui, text: &str, wrap_width: f32) -> LayoutJob {
    let p = theme::palette(ui);
    let font = egui::TextStyle::Monospace.resolve(ui.style());
    let plain = ui.visuals().text_color();
    let format = |color| TextFormat::simple(font.clone(), color);
    let mut job = LayoutJob::default();
    job.wrap.max_width = wrap_width;
    let spans = if text.len() > MAX_COLOURED {
        Vec::new()
    } else {
        scan(text).spans
    };
    let mut at = 0;
    for (range, token) in spans {
        // The scanner's ranges are on character boundaries and in order.
        if range.start < at || text.get(range.clone()).is_none() {
            continue;
        }
        job.append(&text[at..range.start], 0.0, format(plain));
        let color = match token {
            Token::Keyword => p.purple,
            Token::String => p.ok,
            Token::Number => p.blue,
            Token::Comment => p.muted,
            Token::Regex => p.redirect,
        };
        job.append(&text[range.clone()], 0.0, format(color));
        at = range.end;
    }
    job.append(&text[at..], 0.0, format(plain));
    job
}

fn beautify_button(ui: &mut Ui, script: &mut String) {
    let beautify = ui
        .add_enabled(
            !script.trim().is_empty(),
            egui::Button::new(t!("script-beautify")).small(),
        )
        .on_hover_text(t!("script-beautify-hint"));
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
                        .push(message.clone().unwrap_or_else(|| t!("script-test-failed")));
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
            n => t!("script-and-more", count = n - 1),
        };
        parts.push(format!("✗ {first}{more}"));
        if mark.passed > 0 {
            parts.push(t!("script-passed", count = mark.passed));
        }
    } else if mark.passed == 1 {
        parts.push("✓".to_owned());
    } else if mark.passed > 1 {
        parts.push(format!("✓ {}", t!("script-passed", count = mark.passed)));
    }
    match mark.hits {
        0 => {}
        n => parts.push(format!("⏺ {}", t!("script-hits", count = n))),
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
    let field = field_id(ui, id);
    editing_keys(ui, field, text);
    let before = TextEdit::load_state(ui.ctx(), field)
        .map(|s| selection_of(&s))
        .unwrap_or_default();
    let mut layouter = |ui: &Ui, buffer: &dyn egui::TextBuffer, wrap_width: f32| {
        let job = coloured(ui, buffer.as_str(), wrap_width);
        ui.fonts_mut(|f| f.layout_job(job))
    };
    let out = TextEdit::multiline(text)
        .id(field)
        .code_editor()
        .layouter(&mut layouter)
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
    keep_selection_on_right_click(ui, field, &out.response.response, before);
    let keys = script_keys::current(ui.ctx());
    out.response
        .response
        .context_menu(|ui| field_menu(ui, field, text, &keys));
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
        .on_hover_text(t!("script-gutter-hint"));
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

    /// R-TEST-14: Enter keeps the indent and opens a block, Tab indents, Ctrl+/ comments
    /// the line out, in the field itself.
    #[test]
    fn editing_keys_work_in_the_field() {
        let mut text = "if (x) {".to_owned();
        let mut h = crate::test_host::harness(|ui| {
            script_editor(ui, "keys", &mut text, 4, "script", None, &BTreeMap::new());
        });
        h.run();
        h.get_all_by_role(egui::accesskit::Role::MultilineTextInput)
            .next()
            .expect("field")
            .click();
        h.run();
        h.key_press(Key::End);
        h.run();
        h.key_press(Key::Enter);
        h.run();
        h.key_press(Key::Tab);
        h.run();
        h.key_press_modifiers(Modifiers::COMMAND, Key::Slash);
        h.run();
        drop(h);
        // The new line is blank, so Ctrl+/ left it alone: nothing to comment out.
        assert_eq!(text, "if (x) {\n    ");

        let mut text = "a();".to_owned();
        let mut h = crate::test_host::harness(|ui| {
            script_editor(ui, "keys", &mut text, 4, "script", None, &BTreeMap::new());
        });
        h.run();
        h.get_all_by_role(egui::accesskit::Role::MultilineTextInput)
            .next()
            .expect("field")
            .click();
        h.run();
        h.key_press_modifiers(Modifiers::COMMAND, Key::Slash);
        h.run();
        drop(h);
        assert_eq!(text, "// a();");
    }

    /// The script after `steps` in a focused field with `text` and the cursor at `cursor`,
    /// with the script editor `settings`.
    fn edited_in_field(
        text: &str,
        settings: &tortuga_core::settings::ScriptEditorSettings,
        cursor: usize,
        steps: impl FnOnce(&mut egui_kittest::Harness<'_>),
    ) -> String {
        let mut text = text.to_owned();
        let mut place = Some(cursor);
        {
            let mut h = crate::test_host::harness(|ui| {
                script_keys::use_settings(ui.ctx(), settings);
                if let Some(at) = place.take() {
                    let field = field_id(ui, "keys");
                    let mut state = TextEdit::load_state(ui.ctx(), field).unwrap_or_default();
                    state
                        .cursor
                        .set_char_range(Some(CCursorRange::one(CCursor::new(at))));
                    TextEdit::store_state(ui.ctx(), field, state);
                    ui.memory_mut(|m| m.request_focus(field));
                }
                script_editor(ui, "keys", &mut text, 4, "script", None, &BTreeMap::new());
            });
            h.run();
            steps(&mut h);
            h.run();
        }
        text
    }

    /// R-TEST-17: line commands and pairs in the field; R-SET-16: with changed keys.
    #[test]
    fn line_commands_and_pairs_work_in_the_field() {
        use tortuga_core::settings::ScriptEditorSettings;

        let usual = ScriptEditorSettings::default();
        let moved = edited_in_field("a\nb", &usual, 0, |h| {
            h.key_press_modifiers(Modifiers::ALT, Key::ArrowDown);
        });
        assert_eq!(moved, "b\na");
        let commented = edited_in_field("a();", &usual, 0, |h| {
            h.key_press_modifiers(Modifiers::COMMAND, Key::Quote);
        });
        assert_eq!(commented, "// a();", "Ctrl+' as Ctrl+/");
        let paired = edited_in_field("f", &usual, 1, |h| {
            h.event(egui::Event::Text("(".into()));
            h.run();
            h.event(egui::Event::Text(")".into()));
            h.run();
            h.event(egui::Event::Text("ü".into()));
        });
        assert_eq!(paired, "f()ü", "the ) was stepped over");

        let mut changed = ScriptEditorSettings {
            close_pairs: false,
            ..ScriptEditorSettings::default()
        };
        changed.set_keys(ScriptCommand::DeleteLines, vec!["Ctrl+D".into()]);
        let deleted = edited_in_field("a\nb", &changed, 0, |h| {
            h.key_press_modifiers(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::K);
            h.run();
            h.key_press_modifiers(Modifiers::COMMAND, Key::D);
            h.run();
            h.event(egui::Event::Text("(".into()));
        });
        assert_eq!(
            deleted, "(b",
            "Ctrl+D deletes, Ctrl+Shift+K no longer; no pairs"
        );
    }

    /// R-TEST-17: a focused script field takes its keys before the app's shortcuts, so
    /// Ctrl+Shift+K deletes lines rather than opening the palette (Ctrl+K).
    #[test]
    fn the_field_takes_its_keys_before_the_app() {
        let mut text = "a();".to_owned();
        let taken = std::cell::Cell::new(false);
        {
            let mut h = crate::test_host::harness(|ui| {
                taken.set(taken.get() || script_keys::field_takes_keys(ui.ctx()));
                script_editor(ui, "keys", &mut text, 4, "script", None, &BTreeMap::new());
            });
            h.run();
            h.get_all_by_role(egui::accesskit::Role::MultilineTextInput)
                .next()
                .expect("field")
                .click();
            h.run();
            assert!(!taken.get(), "no key of the field pressed yet");
            h.key_press_modifiers(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::K);
            h.run();
        }
        assert!(taken.get());
        assert_eq!(text, "");
    }

    /// R-TEST-17: the field's menu runs a line command on the cursor's line.
    #[test]
    fn the_field_menu_runs_line_commands() {
        let settings = tortuga_core::settings::ScriptEditorSettings::default();
        let copied = edited_in_field("a();", &settings, 4, |h| {
            h.get_all_by_role(egui::accesskit::Role::MultilineTextInput)
                .next()
                .expect("field")
                .click_secondary();
            h.run();
            // Each item shows its keys.
            h.get_by_label("Cut Ctrl+X");
            h.get_by_label("Copy lines down Alt+Shift+Down").click();
        });
        assert_eq!(copied, "a();\na();");
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
