//! Fields that know their `{{variables}}` (R-VAR-5, R-VAR-9): one that uses an undefined
//! name is marked; hovering a `{{name}}` shows its value (secrets masked) and where it
//! comes from; Ctrl+click opens where it is defined; typing `{{` suggests names.

use std::cell::Cell;
use std::collections::BTreeMap;

use egui::text::{CCursor, CCursorRange};
use egui::{Id, Key, Modifiers, Response, RichText, TextEdit, Ui};
use tortuga_core::highlight::Language;
use tortuga_core::vars;

use super::code_colours;
use crate::{format, fuzzy, theme};

/// Most names suggested at once; typing narrows them.
const SUGGESTIONS: usize = 8;
/// Longest value shown on hover.
const VALUE_BYTES: usize = 300;

/// What a field can tell about the variables it may use.
#[derive(Default)]
pub struct VarHelp {
    /// Names used but not defined: their fields are marked (R-VAR-5).
    pub undefined: Vec<String>,
    /// What hovering a name shows, for the names the request uses.
    pub known: BTreeMap<String, VarInfo>,
    /// Names to suggest after `{{`, with a short note (the source, or what a dynamic
    /// one makes).
    pub names: Vec<(String, String)>,
    /// Set by a Ctrl+click on a `{{name}}`; the shell opens where it is defined.
    pub open: Cell<Option<String>>,
}

impl std::fmt::Debug for VarHelp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Values are not written out: secrets are masked in them, but keep them short.
        f.debug_struct("VarHelp")
            .field("undefined", &self.undefined)
            .field("known", &self.known.keys().collect::<Vec<_>>())
            .field("names", &self.names.len())
            .finish_non_exhaustive()
    }
}

/// One variable as hovering shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VarInfo {
    /// The value, or a note in place of it ("secret, hidden").
    pub value: String,
    /// "the environment “Staging”"; `None` when nothing defines it.
    pub from: Option<String>,
}

impl VarHelp {
    /// Whether `text` uses an undefined name.
    pub fn uses_undefined(&self, text: &str) -> bool {
        !self.undefined.is_empty()
            && vars::names(text)
                .iter()
                .any(|n| self.undefined.iter().any(|u| u == n))
    }

    /// The names that fit what was typed after `{{`: those that start with it first,
    /// then fuzzy matches, as many as are shown.
    fn suggest(&self, typed: &str) -> Vec<(String, String)> {
        // Equal scores keep the list's order: the project's names, then dynamic ones.
        let mut scored: Vec<(i64, usize, &(String, String))> = self
            .names
            .iter()
            .enumerate()
            .filter_map(|(i, n)| {
                let prefix = n.0.starts_with(typed);
                fuzzy::score(typed, &n.0).map(|s| (s + if prefix { 10_000 } else { 0 }, i, n))
            })
            .collect();
        scored.sort_by_key(|(s, i, _)| (std::cmp::Reverse(*s), *i));
        scored
            .into_iter()
            .filter(|(_, _, n)| n.0 != typed)
            .take(SUGGESTIONS)
            .map(|(_, _, n)| n.clone())
            .collect()
    }
}

/// One line or several.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lines {
    One,
    Many,
    /// Several lines of JSON or XML, in colour (R-HTTP-24).
    Code(Language),
}

/// How wide a field is.
#[derive(Debug, Clone, Copy)]
pub enum FieldWidth {
    /// As the `TextEdit` asks for itself.
    AsBuilt,
    /// Exactly this wide, one line tall (as `theme::sized_edit`).
    Fixed(f32),
}

/// The suggestions open under a field.
#[derive(Debug, Clone, Default)]
struct Completion {
    field: Option<Id>,
    items: Vec<(String, String)>,
    selected: usize,
    /// Escape closed them until the typed name changes.
    closed_for: Option<String>,
    typed: String,
    /// Where the list was drawn, so a click in it doesn't count as leaving the field.
    rect: Option<egui::Rect>,
}

fn completion_id() -> Id {
    Id::unique("variable-completion")
}

/// A text field for `text` with the id `id` that knows its variables (see the module):
/// `configure` sets the rest of the `TextEdit` (font, hint, size). Returns the field's
/// response, changed also when a suggestion was put in.
pub fn var_edit(
    ui: &mut Ui,
    id: Id,
    text: &mut String,
    help: &VarHelp,
    lines: Lines,
    width: FieldWidth,
    configure: impl for<'a> FnOnce(TextEdit<'a>) -> TextEdit<'a>,
) -> Response {
    let ctx = ui.ctx().clone();
    let mut state: Completion = ctx
        .data(|d| d.get_temp(completion_id()))
        .unwrap_or_default();
    let chosen = list_keys(ui, id, &mut state);
    if let Some(name) = &chosen {
        complete(&ctx, id, text, name);
        state.items.clear();
    }
    let output = show_field(ui, id, text, help, lines, width, configure);
    let mut response = output.response.response.clone();
    if chosen.is_some() {
        response.mark_changed();
    }
    response = explain_hovered(ui, response, &output, text, help);
    let typed = output
        .cursor_range
        .filter(|_| response.has_focus())
        .and_then(|r| typed_name(text, r.primary));
    keep_suggestions(ui, id, &mut state, typed, help);
    if state.field == Some(id) && !state.items.is_empty() {
        let at = output.cursor_range.map_or(output.galley_pos, |r| {
            output.galley_pos
                + output
                    .galley
                    .pos_from_cursor(r.primary)
                    .left_bottom()
                    .to_vec2()
        });
        if let Some(name) = suggestions(ui, &mut state, at) {
            complete(&ctx, id, text, &name);
            state = Completion::default();
            ctx.memory_mut(|m| m.request_focus(id));
            response.mark_changed();
        }
    }
    ctx.data_mut(|d| d.insert_temp(completion_id(), state));
    response
}

/// Keys for an open list go to it, not the field, so they are taken before the field is
/// drawn. Returns the name Enter or Tab chose.
fn list_keys(ui: &Ui, id: Id, state: &mut Completion) -> Option<String> {
    if state.field != Some(id) || state.items.is_empty() || !ui.memory(|m| m.has_focus(id)) {
        return None;
    }
    ui.input_mut(|i| {
        let len = state.items.len();
        if i.consume_key(Modifiers::NONE, Key::ArrowDown) {
            state.selected = (state.selected + 1) % len;
        }
        if i.consume_key(Modifiers::NONE, Key::ArrowUp) {
            state.selected = (state.selected + len - 1) % len;
        }
        if i.consume_key(Modifiers::NONE, Key::Escape) {
            state.closed_for = Some(state.typed.clone());
            state.items.clear();
            return None;
        }
        let pick =
            i.consume_key(Modifiers::NONE, Key::Enter) || i.consume_key(Modifiers::NONE, Key::Tab);
        pick.then(|| state.items.get(state.selected).map(|n| n.0.clone()))
            .flatten()
    })
}

/// The field itself, marked when it uses an undefined name (R-VAR-5).
fn show_field(
    ui: &mut Ui,
    id: Id,
    text: &mut String,
    help: &VarHelp,
    lines: Lines,
    width: FieldWidth,
    configure: impl for<'a> FnOnce(TextEdit<'a>) -> TextEdit<'a>,
) -> egui::text_edit::TextEditOutput {
    let flagged = help.uses_undefined(text);
    let language = match lines {
        Lines::Code(language) => Some(language),
        Lines::One | Lines::Many => None,
    };
    let mut layouter = |ui: &Ui, buffer: &dyn egui::TextBuffer, wrap_width: f32| {
        let job = code_colours::layout(ui, buffer.as_str(), language, wrap_width);
        ui.fonts_mut(|f| f.layout_job(job))
    };
    let edit = match lines {
        Lines::One => TextEdit::singleline(text),
        Lines::Many => TextEdit::multiline(text),
        Lines::Code(_) => TextEdit::multiline(text).layouter(&mut layouter),
    };
    let mut edit = configure(edit.id(id));
    if flagged {
        edit = edit.background_color(theme::undefined_bg(ui));
    }
    match width {
        FieldWidth::AsBuilt => edit.show(ui),
        FieldWidth::Fixed(w) => {
            let height = ui.spacing().interact_size.y;
            ui.allocate_ui_with_layout(
                egui::vec2(w, height),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    edit.desired_width(w)
                        .min_size(egui::vec2(w, height))
                        .align(egui::Align2::LEFT_CENTER)
                        .show(ui)
                },
            )
            .inner
        }
    }
}

/// The hover card for the `{{name}}` under the pointer; a Ctrl+click on it asks to open
/// where it is defined.
fn explain_hovered(
    ui: &Ui,
    response: Response,
    output: &egui::text_edit::TextEditOutput,
    text: &str,
    help: &VarHelp,
) -> Response {
    let hovered = response
        .hover_pos()
        .filter(|_| !text.is_empty())
        .and_then(|pos| name_at(text, output.galley.cursor_from_pos(pos - output.galley_pos)));
    let Some(name) = hovered else {
        return response;
    };
    if response.clicked() && ui.input(|i| i.modifiers.command) {
        help.open.set(Some(name.clone()));
    }
    response.on_hover_ui_at_pointer(|ui| hover_card(ui, &name, help))
}

/// Opens, updates or closes the suggestions for what is `typed` after `{{` in field
/// `id`. A click in the list doesn't close them, though the field lost the cursor.
fn keep_suggestions(
    ui: &Ui,
    id: Id,
    state: &mut Completion,
    typed: Option<String>,
    help: &VarHelp,
) {
    if let Some(typed) = typed {
        if state.closed_for.as_ref() == Some(&typed) {
            return;
        }
        if state.field != Some(id) || state.typed != typed {
            state.selected = 0;
            state.closed_for = None;
        }
        state.items = help.suggest(&typed);
        state.field = Some(id);
        state.typed = typed;
    } else if state.field == Some(id) {
        let in_list = state.rect.zip(ui.input(|i| i.pointer.hover_pos()));
        if !in_list.is_some_and(|(rect, pos)| rect.contains(pos)) {
            *state = Completion::default();
        }
    }
}

/// The name being typed after `{{` before `cursor`, if any (`""` right after `{{`).
fn typed_name(text: &str, cursor: CCursor) -> Option<String> {
    let byte = byte_at(text, cursor.index.0);
    let before = text.get(..byte)?;
    let start = before.rfind("{{")?;
    let typed = before.get(start + 2..)?;
    let closed = typed.contains("}}") || typed.contains('}');
    let name_like = typed
        .chars()
        .all(|c| c.is_alphanumeric() || "_.$-".contains(c));
    (!closed && name_like).then(|| typed.to_owned())
}

/// The variable whose `{{…}}` the character at `cursor` is in.
fn name_at(text: &str, cursor: CCursor) -> Option<String> {
    let byte = byte_at(text, cursor.index.0);
    vars::spans(text)
        .into_iter()
        .find(|(range, _)| range.start <= byte && byte < range.end)
        .map(|(_, name)| name.to_owned())
}

/// The byte offset of character `index` in `text` (its end, past the last one).
fn byte_at(text: &str, index: usize) -> usize {
    text.char_indices()
        .nth(index)
        .map_or(text.len(), |(b, _)| b)
}

/// Puts `{{name}}` in place of the `{{` and partial name before the field's cursor, and
/// moves the cursor after it.
fn complete(ctx: &egui::Context, id: Id, text: &mut String, name: &str) {
    let mut state = egui::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
    let cursor = state
        .cursor
        .char_range()
        .map_or(text.chars().count(), |r| r.primary.index.0);
    let byte = byte_at(text, cursor);
    let Some(start) = text.get(..byte).and_then(|b| b.rfind("{{")) else {
        return;
    };
    // A `}}` the user already typed after the cursor is reused.
    let end = if text.get(byte..).is_some_and(|rest| rest.starts_with("}}")) {
        byte + 2
    } else {
        byte
    };
    let inserted = format!("{{{{{name}}}}}");
    text.replace_range(start..end, &inserted);
    let after = text.get(..start).map_or(0, |t| t.chars().count()) + inserted.chars().count();
    state
        .cursor
        .set_char_range(Some(CCursorRange::one(CCursor::new(after))));
    state.store(ctx, id);
}

/// The list under the field; returns the name clicked.
fn suggestions(ui: &Ui, state: &mut Completion, at: egui::Pos2) -> Option<String> {
    let mut clicked = None;
    let area = egui::Area::new(Id::unique("variable-suggestions"))
        .order(egui::Order::Foreground)
        .fixed_pos(at + egui::vec2(0.0, 4.0))
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_min_width(220.0);
                let p = theme::palette(ui);
                for (i, (name, note)) in state.items.iter().enumerate() {
                    let text: egui::Atoms<'_> = [
                        egui::Atom::from(RichText::new(name).monospace()),
                        egui::Atom::grow(),
                        egui::Atom::from(RichText::new(note).small().color(p.muted)),
                    ]
                    .into_iter()
                    .collect();
                    if ui
                        .add(egui::Button::selectable(i == state.selected, text))
                        .clicked()
                    {
                        clicked = Some(name.clone());
                    }
                }
            });
        });
    state.rect = Some(area.response.rect);
    clicked
}

/// What hovering `{{name}}` shows.
fn hover_card(ui: &mut Ui, name: &str, help: &VarHelp) {
    let p = theme::palette(ui);
    ui.label(theme::semibold(format!("{{{{{name}}}}}")));
    if let Some(VarInfo {
        value,
        from: Some(from),
    }) = help.known.get(name)
    {
        let (shown, cut) = format::truncate(value, VALUE_BYTES);
        let shown = if cut {
            format!("{shown}…")
        } else {
            shown.to_owned()
        };
        ui.label(RichText::new(shown).monospace());
        ui.label(RichText::new(t!("var-from", from = from.as_str())).color(p.muted));
        ui.label(
            RichText::new(t!("var-ctrl-click-open"))
                .small()
                .color(p.muted),
        );
    } else {
        ui.label(RichText::new(t!("var-not-defined")).color(p.fail));
        ui.label(
            RichText::new(t!("var-ctrl-click-define"))
                .small()
                .color(p.muted),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// R-VAR-9: the name typed after `{{` up to the cursor, also after non-ASCII text;
    /// none once it is closed or isn't a name.
    #[test]
    fn the_typed_name_is_found() {
        let at = |text: &str| typed_name(text, CCursor::new(text.chars().count()));
        assert_eq!(at("ü😀/{{to").as_deref(), Some("to"));
        assert_eq!(at("{{").as_deref(), Some(""));
        assert_eq!(at("{{$random.uuid").as_deref(), Some("$random.uuid"));
        assert_eq!(at("{{token}}"), None);
        assert_eq!(at("{{a b"), None);
        assert_eq!(at("no braces"), None);
        assert_eq!(
            typed_name("{{tok}}/x", CCursor::new(5)).as_deref(),
            Some("tok"),
            "inside a closed one, before the cursor"
        );
    }

    /// R-VAR-9: names that start with the text come first.
    #[test]
    fn names_that_start_with_the_text_come_first() {
        let help = VarHelp {
            names: ["auth_token", "token", "tenant", "$timestamp"]
                .into_iter()
                .map(|n| (n.to_owned(), String::new()))
                .collect(),
            ..VarHelp::default()
        };
        let names: Vec<String> = help.suggest("t").into_iter().map(|n| n.0).collect();
        assert_eq!(
            names[..2],
            ["token".to_owned(), "tenant".to_owned()],
            "in the list's order"
        );
        assert!(names.contains(&"auth_token".to_owned()));
        let all: Vec<String> = help.suggest("").into_iter().map(|n| n.0).collect();
        assert_eq!(all[0], "auth_token", "nothing typed: in the list's order");
        let exact: Vec<String> = help.suggest("token").into_iter().map(|n| n.0).collect();
        assert!(
            !exact.contains(&"token".to_owned()),
            "not what is already typed"
        );
    }

    /// R-VAR-9: the name a character is in.
    #[test]
    fn the_name_under_the_pointer() {
        let text = "ü/{{ base }}/{{id}}";
        assert_eq!(name_at(text, CCursor::new(3)).as_deref(), Some("base"));
        assert_eq!(name_at(text, CCursor::new(1)), None);
        assert_eq!(name_at(text, CCursor::new(14)).as_deref(), Some("id"));
    }

    fn help() -> VarHelp {
        VarHelp {
            known: [(
                "host".to_owned(),
                VarInfo {
                    value: "dev.test".into(),
                    from: Some("the environment “Dev”".into()),
                },
            )]
            .into(),
            names: vec![
                ("host".to_owned(), "dev.test".to_owned()),
                ("token".to_owned(), "secret".to_owned()),
            ],
            ..VarHelp::default()
        }
    }

    /// Draws one field with `help` and keeps its text in `text`.
    fn field<'a>(
        text: &'a std::cell::RefCell<String>,
        help: &'a VarHelp,
    ) -> egui_kittest::Harness<'a> {
        crate::test_host::harness(move |ui| {
            let id = ui.make_persistent_id("field");
            var_edit(
                ui,
                id,
                &mut text.borrow_mut(),
                help,
                Lines::One,
                FieldWidth::Fixed(400.0),
                |e| e.font(egui::TextStyle::Monospace).hint_text("URL"),
            );
        })
    }

    /// R-VAR-9: hovering a `{{name}}` shows its value and where it comes from, and a
    /// Ctrl+click asks to open it.
    #[test]
    fn hovering_a_variable_explains_it() {
        use egui_kittest::kittest::Queryable;
        let text = std::cell::RefCell::new("{{host}}/orders".to_owned());
        let help = help();
        {
            let mut h = field(&text, &help);
            h.run();
            let rect = h
                .get_all_by_role(egui::accesskit::Role::TextInput)
                .next()
                .expect("field")
                .rect();
            let at = egui::pos2(rect.left() + 24.0, rect.center().y);
            h.hover_at(at);
            h.run_steps(8);
            h.get_by_label("From the environment “Dev”");
            h.get_by_label("dev.test");
            h.event_modifiers(
                egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::COMMAND,
                },
                Modifiers::COMMAND,
            );
            h.event_modifiers(
                egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::COMMAND,
                },
                Modifiers::COMMAND,
            );
            h.run();
        }
        assert_eq!(help.open.take().as_deref(), Some("host"));
    }

    /// R-VAR-9: typing `{{` and part of a name suggests names; Enter puts in the first
    /// as `{{name}}`, also before text that follows.
    #[test]
    fn typing_braces_suggests_names() {
        use egui_kittest::kittest::Queryable;
        let text = std::cell::RefCell::new(String::new());
        let help = help();
        {
            let mut h = field(&text, &help);
            h.run();
            h.get_all_by_role(egui::accesskit::Role::TextInput)
                .next()
                .expect("field")
                .focus();
            h.run();
            h.event(egui::Event::Text("ü/{{ho".into()));
            h.run();
            h.get_by_label_contains("host");
            h.key_press(Key::Enter);
            h.run();
            h.event(egui::Event::Text("/x".into()));
            h.run();
        }
        assert_eq!(*text.borrow(), "ü/{{host}}/x");
    }
}
