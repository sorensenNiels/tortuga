//! Quick open (Ctrl+P) and the command palette (Ctrl+K): one search box over the
//! workspace's requests, or, when the text starts with `>`, over commands (R-COL-9, R-COL-10).

use egui::{
    Align2, Atom, AtomExt as _, Button, Context, Id, Key, Modal, RichText, ScrollArea, TextEdit,
};
use tortuga_core::HttpMethod;

use super::DialogResult;
use crate::{fuzzy, theme};

/// Most entries shown at once; typing narrows the list.
const MAX_SHOWN: usize = 60;
const ROW_HEIGHT: f32 = 28.0;
const MAX_LIST_HEIGHT: f32 = 380.0;

/// One entry in the list.
#[derive(Debug, Clone)]
pub struct PaletteItem<T> {
    /// The method of a request, shown in front in its colour.
    pub method: Option<HttpMethod>,
    pub label: String,
    /// Shown muted after the label, e.g. the project and folder.
    pub detail: String,
    /// Shown on the right, e.g. a shortcut.
    pub hint: String,
    /// What the query is matched against, besides the label and detail.
    pub keywords: String,
    pub value: T,
}

impl<T> PaletteItem<T> {
    pub fn new(label: impl Into<String>, value: T) -> Self {
        Self {
            method: None,
            label: label.into(),
            detail: String::new(),
            hint: String::new(),
            keywords: String::new(),
            value,
        }
    }

    #[must_use]
    pub fn method(mut self, method: HttpMethod) -> Self {
        self.method = Some(method);
        self
    }

    #[must_use]
    pub fn detail(mut self, text: impl Into<String>) -> Self {
        self.detail = text.into();
        self
    }

    #[must_use]
    pub fn hint(mut self, text: impl Into<String>) -> Self {
        self.hint = text.into();
        self
    }

    #[must_use]
    pub fn keywords(mut self, text: impl Into<String>) -> Self {
        self.keywords = text.into();
        self
    }

    fn score(&self, query: &str) -> Option<i64> {
        // The label counts double: a match in the name beats one in the folder path.
        let label = fuzzy::score(query, &self.label).map(|s| s * 2);
        let all = fuzzy::score(
            query,
            &format!(
                "{} {} {} {}",
                self.label,
                self.detail,
                self.keywords,
                self.method.map_or("", HttpMethod::as_str)
            ),
        );
        label.max(all)
    }
}

/// The search box and its state.
#[derive(Debug, Clone, Default)]
pub struct Palette {
    query: String,
    selected: usize,
    /// The selection moved with the keyboard: scroll it into view.
    scroll: bool,
}

impl Palette {
    /// Quick open: requests (R-COL-9).
    pub fn requests() -> Self {
        Self::default()
    }

    /// The command palette (R-COL-10).
    pub fn commands() -> Self {
        Self {
            query: ">".into(),
            ..Self::default()
        }
    }

    /// The text starts with `>`: commands are listed instead of requests.
    pub fn shows_commands(&self) -> bool {
        self.query.starts_with('>')
    }

    /// Shows the box over `items` (in their order when nothing is typed) and returns the
    /// chosen value.
    pub fn show<T: Clone>(&mut self, ctx: &Context, items: &[PaletteItem<T>]) -> DialogResult<T> {
        let query = self.query.trim_start_matches('>').trim().to_owned();
        let mut shown: Vec<(i64, usize)> = items
            .iter()
            .enumerate()
            .filter_map(|(i, item)| item.score(&query).map(|s| (s, i)))
            .collect();
        if !query.is_empty() {
            // Best first; equal scores keep the given order.
            shown.sort_by_key(|&(s, i)| (std::cmp::Reverse(s), i));
        }
        let total = shown.len();
        shown.truncate(MAX_SHOWN);

        self.selected = self.selected.min(shown.len().saturating_sub(1));

        let mut chosen = self
            .keys(ctx, shown.len())
            .then(|| shown.get(self.selected).map(|&(_, i)| i))
            .flatten();

        let width = 620.0_f32.min(ctx.content_rect().width() - 48.0);
        let area =
            Modal::default_area(Id::unique("palette")).anchor(Align2::CENTER_TOP, [0.0, 70.0]);
        let frame = egui::Frame::window(&ctx.global_style()).inner_margin(egui::Margin::same(10));
        let response = Modal::new(Id::unique("palette"))
            .area(area)
            .frame(frame)
            .show(ctx, |ui| {
                ui.set_width(width);
                let hint = if self.shows_commands() {
                    "Type a command"
                } else {
                    "Search requests by name, folder, project or URL. Type > for commands"
                };
                let edit = ui.add(
                    TextEdit::singleline(&mut self.query)
                        .id_salt("palette-query")
                        .hint_text(hint)
                        .desired_width(f32::INFINITY)
                        .margin(egui::Margin::symmetric(8, 6)),
                );
                edit.request_focus();
                if edit.changed() {
                    self.selected = 0;
                }
                ui.add_space(6.0);
                if shown.is_empty() {
                    theme::hint(
                        ui,
                        if self.shows_commands() {
                            "No command matches."
                        } else {
                            "No request matches."
                        },
                    );
                }
                // Sized from the rows, not from what an earlier list remembered.
                #[expect(clippy::cast_precision_loss, reason = "at most MAX_SHOWN rows")]
                let rows = (shown.len() + usize::from(total > shown.len())) as f32;
                let height = (rows * (ROW_HEIGHT + 1.0)).min(MAX_LIST_HEIGHT);
                ScrollArea::vertical()
                    .id_salt(("palette-list", self.shows_commands()))
                    .max_height(height)
                    .min_scrolled_height(height)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 1.0;
                        for (row, &(_, i)) in shown.iter().enumerate() {
                            let selected = row == self.selected;
                            let response = item_row(ui, &items[i], selected);
                            if selected && self.scroll {
                                response.scroll_to_me(None);
                            }
                            if response.clicked() {
                                chosen = Some(i);
                            }
                        }
                        if total > shown.len() {
                            theme::hint(
                                ui,
                                format!("{} more: type to narrow the list.", total - shown.len()),
                            );
                        }
                    });
            });
        self.scroll = false;
        if let Some(i) = chosen {
            return DialogResult::Done(items[i].value.clone());
        }
        if response.should_close() {
            DialogResult::Close
        } else {
            DialogResult::Open
        }
    }

    /// Arrow keys move the selection among `count` entries. Returns whether Enter was pressed.
    fn keys(&mut self, ctx: &Context, count: usize) -> bool {
        let (up, down, enter) = ctx.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::NONE, Key::ArrowUp),
                i.consume_key(egui::Modifiers::NONE, Key::ArrowDown),
                i.consume_key(egui::Modifiers::NONE, Key::Enter),
            )
        });
        if up && self.selected > 0 {
            self.selected -= 1;
            self.scroll = true;
        }
        if down && self.selected + 1 < count {
            self.selected += 1;
            self.scroll = true;
        }
        enter
    }
}

/// One entry: method, label, detail, and the hint on the right.
fn item_row<T>(ui: &mut egui::Ui, item: &PaletteItem<T>, selected: bool) -> egui::Response {
    let p = theme::palette(ui);
    let mut atoms: Vec<Atom<'_>> = Vec::new();
    if let Some(m) = item.method {
        atoms.push(Atom::from(
            RichText::new(format!("{:<6}", theme::method_short(m)))
                .monospace()
                .size(10.5)
                .color(theme::method_color(ui, m)),
        ));
    }
    // The label gives way when the row is narrow, not the method.
    atoms.push(Atom::from(item.label.as_str()).atom_shrink(true));
    if !item.detail.is_empty() {
        atoms.push(Atom::from(
            RichText::new(&item.detail).small().color(p.muted),
        ));
    }
    atoms.push(Atom::grow());
    if !item.hint.is_empty() {
        atoms.push(Atom::from(RichText::new(&item.hint).small().color(p.muted)));
    }
    let atoms: egui::Atoms<'_> = atoms.into_iter().collect();
    ui.add(
        Button::selectable(selected, atoms)
            .truncate()
            .min_size(egui::vec2(ui.available_width(), ROW_HEIGHT)),
    )
}

#[cfg(test)]
mod tests {
    use egui_kittest::kittest::Queryable;

    use super::*;

    fn items() -> Vec<PaletteItem<u32>> {
        vec![
            PaletteItem::new("List users", 1).detail("Users API"),
            PaletteItem::new("Get user", 2).detail("Users API"),
            PaletteItem::new("Create order", 3).detail("Orders API"),
        ]
    }

    fn run(palette: Palette, keys: &[Key], text: &str) -> (Palette, Option<u32>) {
        let result = std::rc::Rc::new(std::cell::RefCell::new((palette, None)));
        let shared = result.clone();
        let items = items();
        let mut h = crate::test_host::harness(move |ui| {
            let mut r = shared.borrow_mut();
            if let DialogResult::Done(v) = r.0.show(ui.ctx(), &items) {
                r.1 = Some(v);
            }
        });
        h.run();
        if !text.is_empty() {
            h.get_by_role(egui::accesskit::Role::TextInput)
                .type_text(text);
            h.run();
        }
        for key in keys {
            h.key_press(*key);
            h.run();
        }
        drop(h);
        let r = result.borrow();
        (r.0.clone(), r.1)
    }

    #[test]
    fn typing_narrows_and_enter_opens_the_best_match() {
        let (_, chosen) = run(Palette::requests(), &[Key::Enter], "order");
        assert_eq!(chosen, Some(3));
        let (_, chosen) = run(Palette::requests(), &[Key::Enter], "usr get");
        assert_eq!(chosen, Some(2));
    }

    #[test]
    fn arrows_move_the_selection() {
        let (_, chosen) = run(
            Palette::requests(),
            &[Key::ArrowDown, Key::ArrowDown, Key::ArrowUp, Key::Enter],
            "",
        );
        assert_eq!(chosen, Some(2));
    }

    #[test]
    fn a_leading_angle_bracket_means_commands() {
        assert!(Palette::commands().shows_commands());
        assert!(!Palette::requests().shows_commands());
        let (palette, _) = run(Palette::requests(), &[], ">");
        assert!(palette.shows_commands());
    }
}
