//! The request's Examples tab (R-MOCK-1, R-MOCK-3): the responses the mock server
//! answers this request with, and when each one answers.

use std::collections::BTreeMap;

use egui::{Grid, RichText, TextEdit, Ui};
use tortuga_core::KeyValue;
use tortuga_core::mock::{EXAMPLE_HEADER, Example};

use super::{VarHelp, key_value_table};
use crate::theme;

/// Which example is open, and the text of its fields that are not plain strings.
#[derive(Debug, Clone, Default)]
pub struct ExamplesState {
    chosen: usize,
    drafts: Option<Drafts>,
}

/// Field buffers of the open example; made again when another example opens, so typing
/// "4" on the way to "404" is not undone.
#[derive(Debug, Clone)]
struct Drafts {
    index: usize,
    status: String,
    delay: String,
    query: Vec<KeyValue>,
    headers: Vec<KeyValue>,
    body: Vec<KeyValue>,
}

impl Drafts {
    fn new(index: usize, example: &Example) -> Self {
        let rows =
            |map: &BTreeMap<String, String>| map.iter().map(|(k, v)| KeyValue::new(k, v)).collect();
        Self {
            index,
            status: example.status.to_string(),
            delay: example.delay_ms.map(|d| d.to_string()).unwrap_or_default(),
            query: rows(&example.conditions.query),
            headers: rows(&example.conditions.headers),
            body: rows(&example.conditions.body),
        }
    }
}

impl ExamplesState {
    /// Opens example `index`, e.g. one just saved from a response.
    pub fn open(&mut self, index: usize) {
        self.chosen = index;
        self.drafts = None;
    }
}

/// The examples of a request: a row to pick one (or add one), then the open one's
/// status, delay, headers, body and conditions.
pub fn request_examples(
    ui: &mut Ui,
    examples: &mut Vec<Example>,
    st: &mut ExamplesState,
    help: &VarHelp,
) {
    theme::hint(
        ui,
        format!(
            "The mock server (project menu → Start mock server) answers this request with the \
             first example whose conditions hold. A request with the header \
             {EXAMPLE_HEADER}: <name> gets that example."
        ),
    );
    ui.add_space(6.0);
    example_picker(ui, examples, st);
    if examples.is_empty() {
        return;
    }
    st.chosen = st.chosen.min(examples.len() - 1);
    let index = st.chosen;
    if st.drafts.as_ref().is_none_or(|d| d.index != index) {
        st.drafts = Some(Drafts::new(index, &examples[index]));
    }
    let Some(drafts) = st.drafts.as_mut() else {
        return;
    };
    ui.add_space(8.0);
    match example_fields(ui, &mut examples[index], drafts, help) {
        Some(Change::Up) if index > 0 => {
            examples.swap(index, index - 1);
            st.open(index - 1);
        }
        Some(Change::Down) if index + 1 < examples.len() => {
            examples.swap(index, index + 1);
            st.open(index + 1);
        }
        Some(Change::Remove) => {
            examples.remove(index);
            st.open(index.saturating_sub(1));
        }
        Some(Change::Up | Change::Down) | None => {}
    }
}

/// What the buttons beside the open example ask for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Change {
    Up,
    Down,
    Remove,
}

fn example_picker(ui: &mut Ui, examples: &mut Vec<Example>, st: &mut ExamplesState) {
    ui.horizontal_wrapped(|ui| {
        let p = theme::palette(ui);
        for (i, example) in examples.iter().enumerate() {
            let name = if example.name.trim().is_empty() {
                "(no name)"
            } else {
                example.name.as_str()
            };
            let label = (
                RichText::new(name),
                RichText::new(format!(" {}", example.status))
                    .small()
                    .color(p.muted),
            );
            if ui.selectable_label(st.chosen == i, label).clicked() {
                st.open(i);
            }
        }
        if ui.button("+ Add example").clicked() {
            let name = tortuga_core::import::unique_name("Example", |n| {
                examples.iter().any(|e| e.name == n)
            });
            examples.push(Example::new(name));
            st.open(examples.len() - 1);
        }
    });
}

fn example_fields(
    ui: &mut Ui,
    example: &mut Example,
    drafts: &mut Drafts,
    help: &VarHelp,
) -> Option<Change> {
    let mut change = None;
    Grid::new("example-fields")
        .num_columns(2)
        .spacing([8.0, 6.0])
        .show(ui, |ui| {
            let label = ui.label("Name");
            ui.horizontal(|ui| {
                theme::sized_edit(
                    ui,
                    220.0,
                    TextEdit::singleline(&mut example.name).id_salt("example-name"),
                )
                .labelled_by(label.id);
                let buttons = [
                    ("↑", "Earlier: asked before the ones below", Change::Up),
                    ("↓", "Later", Change::Down),
                    ("🗙", "Remove this example", Change::Remove),
                ];
                for (glyph, hover, asked) in buttons {
                    if ui
                        .add(theme::icon_button(ui, glyph))
                        .on_hover_text(hover)
                        .clicked()
                    {
                        change = Some(asked);
                    }
                }
            });
            ui.end_row();
            let label = ui.label("Status");
            ui.horizontal(|ui| {
                let edit = TextEdit::singleline(&mut drafts.status).id_salt("example-status");
                if theme::sized_edit(ui, 60.0, edit)
                    .labelled_by(label.id)
                    .changed()
                    && let Some(status) = parse_status(&drafts.status)
                {
                    example.status = status;
                }
                if parse_status(&drafts.status).is_none() {
                    theme::warning(ui, "A status from 100 to 999");
                }
                ui.add_space(12.0);
                let label = ui.label("Delay (ms)");
                let edit = TextEdit::singleline(&mut drafts.delay)
                    .id_salt("example-delay")
                    .hint_text("none");
                if theme::sized_edit(ui, 70.0, edit)
                    .labelled_by(label.id)
                    .changed()
                {
                    example.delay_ms = drafts.delay.trim().parse().ok().filter(|d| *d > 0);
                }
            });
            ui.end_row();
        });

    ui.add_space(8.0);
    theme::section_label(ui, "Headers");
    // Each table in its own id scope: their fields are salted by row only.
    ui.push_id("example-headers", |ui| {
        key_value_table(ui, "table", &mut example.headers, "Header", help);
    });
    theme::hint(
        ui,
        "Without a Content-Type, JSON, XML or text is worked out from the body.",
    );
    ui.add_space(8.0);
    theme::section_label(ui, "Body");
    ui.add(
        TextEdit::multiline(&mut example.body)
            .id_salt("example-body")
            .code_editor()
            .desired_rows(8)
            .desired_width(f32::INFINITY)
            .hint_text("{\"id\": 1}"),
    );
    theme::hint(
        ui,
        "{{variables}} are filled from the project's active environment.",
    );
    ui.add_space(8.0);
    conditions(ui, example, drafts);
    change
}

/// "Answers when": query, header and body conditions (R-MOCK-3).
fn conditions(ui: &mut Ui, example: &mut Example, drafts: &mut Drafts) {
    theme::section_label(ui, "Answers when");
    theme::hint(
        ui,
        "Every condition must hold. Without conditions this example answers any request \
         that reaches it.",
    );
    let tables = [
        (
            "example-query",
            "Query parameter",
            &mut drafts.query,
            &mut example.conditions.query,
        ),
        (
            "example-match-headers",
            "Header",
            &mut drafts.headers,
            &mut example.conditions.headers,
        ),
        (
            "example-match-body",
            "Body ($.path or xpath:…)",
            &mut drafts.body,
            &mut example.conditions.body,
        ),
    ];
    for (id, hint, rows, map) in tables {
        if ui
            .push_id(id, |ui| {
                key_value_table(ui, "table", rows, hint, &VarHelp::default())
            })
            .inner
        {
            *map = rows
                .iter()
                .filter(|r| r.is_active())
                .map(|r| (r.name.trim().to_owned(), r.value.clone()))
                .collect();
        }
        ui.add_space(4.0);
    }
}

fn parse_status(text: &str) -> Option<u16> {
    text.trim().parse().ok().filter(|s| (100..=999).contains(s))
}

#[cfg(test)]
mod tests {
    use egui::accesskit::Role;
    use egui_kittest::kittest::Queryable;

    use super::*;

    fn harness(
        examples: Vec<Example>,
    ) -> egui_kittest::Harness<'static, (Vec<Example>, ExamplesState)> {
        crate::test_host::harness_with_state(
            |ui, (examples, st): &mut (Vec<Example>, ExamplesState)| {
                request_examples(ui, examples, st, &VarHelp::default());
            },
            (examples, ExamplesState::default()),
        )
    }

    /// R-MOCK-1: adding examples gives each a free name and opens it.
    #[test]
    fn examples_are_added_with_free_names() {
        let mut h = harness(vec![]);
        h.run();
        h.get_by_label("+ Add example").click();
        h.run();
        h.get_by_label("+ Add example").click();
        h.run();
        let names: Vec<&str> = h.state().0.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["Example", "Example 2"]);
        assert_eq!(h.state().1.chosen, 1);
    }

    /// R-MOCK-1: the status keeps what was typed until it is a valid status.
    #[test]
    fn the_status_changes_once_it_is_valid() {
        let mut h = harness(vec![Example::new("ok")]);
        h.run();
        let status = h
            .get_all_by_role(Role::TextInput)
            .find(|n| n.value().as_deref() == Some("200"))
            .expect("status field");
        status.focus();
        status.type_text("1");
        h.run();
        assert_eq!(h.state().0[0].status, 200, "2001 is no status");
        assert_eq!(
            h.state().1.drafts.as_ref().map(|d| d.status.as_str()),
            Some("2001")
        );
        h.get_by_label("A status from 100 to 999");
    }

    /// R-MOCK-3: conditions typed into the tables become the example's conditions.
    #[test]
    fn conditions_are_kept_by_name() {
        let mut example = Example::new("role check");
        example
            .conditions
            .headers
            .insert("X-Role".into(), "admin".into());
        let mut h = harness(vec![example]);
        h.run();
        let value = h
            .get_all_by_role(Role::TextInput)
            .find(|n| n.value().as_deref() == Some("admin"))
            .expect("condition value");
        value.focus();
        value.type_text(" ü😀");
        h.run();
        assert_eq!(h.state().0[0].conditions.headers["X-Role"], "admin ü😀");
    }

    /// R-MOCK-3: examples are reordered and removed with the buttons beside the name.
    #[test]
    fn examples_are_reordered_and_removed() {
        let mut h = harness(vec![Example::new("first"), Example::new("second")]);
        h.run();
        h.get_by_label_contains("second").click();
        h.run();
        h.get_by_label("↑").click();
        h.run();
        let names = |h: &egui_kittest::Harness<'_, (Vec<Example>, ExamplesState)>| -> Vec<String> {
            h.state().0.iter().map(|e| e.name.clone()).collect()
        };
        assert_eq!(names(&h), ["second", "first"]);
        assert_eq!(h.state().1.chosen, 0, "the moved example stays open");
        h.get_by_label("🗙").click();
        h.run();
        assert_eq!(names(&h), ["first"]);
    }

    #[test]
    fn statuses() {
        assert_eq!(parse_status(" 404 "), Some(404));
        assert_eq!(parse_status("99"), None);
        assert_eq!(parse_status("ü😀"), None);
        assert_eq!(parse_status(""), None);
    }
}
