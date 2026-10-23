use egui::{AsIdSalt, Grid, Ui};
use tortuga_core::KeyValue;

use super::{FieldWidth, Lines, VarHelp, var_edit};
use crate::theme;

/// Editable name/value table with enable toggles (params, headers, form fields).
/// A blank row at the end turns into a real row as soon as the user types in it.
/// Returns whether `rows` changed.
pub fn key_value_table(
    ui: &mut Ui,
    id_salt: impl AsIdSalt,
    rows: &mut Vec<KeyValue>,
    name_hint: &str,
    // The variables cells may use: undefined ones are marked (R-VAR-5), and hovering or
    // typing `{{` helps (R-VAR-9).
    vars: &VarHelp,
) -> bool {
    let mut changed = false;
    let mut remove = None;
    let mut blank = KeyValue::new("", "");
    let count = rows.len();
    // Toggle + remove columns and spacing take ~90 px; the rest is split between the text columns.
    let text_width = ((ui.available_width() - 90.0) / 2.0).max(80.0);

    Grid::new(id_salt)
        .num_columns(4)
        .spacing([6.0, 4.0])
        .show(ui, |ui| {
            let cx = RowContext {
                name_hint,
                vars,
                text_width,
            };
            ui.label("");
            theme::section_label(ui, name_hint);
            theme::section_label(ui, &t!("value"));
            ui.label("");
            ui.end_row();
            for (i, row) in rows.iter_mut().enumerate() {
                let (c, r) = row_ui(ui, i, row, false, &cx);
                changed |= c;
                if r {
                    remove = Some(i);
                }
            }
            changed |= row_ui(ui, count, &mut blank, true, &cx).0;
        });

    changed |= append_if_filled(rows, blank);
    if let Some(i) = remove {
        rows.remove(i);
        changed = true;
    }
    changed
}

struct RowContext<'a> {
    name_hint: &'a str,
    vars: &'a VarHelp,
    text_width: f32,
}

/// One table row. Returns (changed, remove clicked).
fn row_ui(
    ui: &mut Ui,
    i: usize,
    row: &mut KeyValue,
    is_blank: bool,
    cx: &RowContext<'_>,
) -> (bool, bool) {
    let mut changed = false;
    if is_blank {
        ui.label("");
    } else {
        changed |= ui
            .checkbox(&mut row.enabled, "")
            .on_hover_text(t!("kv-include"))
            .changed();
    }
    // Ids by row index, so focus stays put when the blank row becomes a real one.
    let (hint, vars, width) = (cx.name_hint, cx.vars, cx.text_width);
    // Placeholders only show in empty fields; they also name the fields for screen readers.
    let name_hint = if is_blank {
        t!("kv-add", name = hint.to_lowercase())
    } else {
        hint.to_owned()
    };
    let value_hint = t!("value");
    changed |= cell(ui, ("name", i), &mut row.name, &name_hint, vars, width);
    changed |= cell(ui, ("value", i), &mut row.value, &value_hint, vars, width);
    let remove = !is_blank
        && ui
            .add(theme::icon_button(ui, "🗙"))
            .on_hover_text(t!("remove"))
            .clicked();
    ui.end_row();
    (changed, remove)
}

/// One text cell of a table, `width` wide, that knows its variables (R-VAR-5, R-VAR-9).
/// Returns whether it changed.
pub(super) fn cell(
    ui: &mut Ui,
    id_salt: (&str, usize),
    text: &mut String,
    hint: &str,
    vars: &VarHelp,
    width: f32,
) -> bool {
    let id = ui.make_persistent_id(id_salt);
    var_edit(
        ui,
        id,
        text,
        vars,
        Lines::One,
        FieldWidth::Fixed(width),
        |e| e.hint_text(hint.to_owned()),
    )
    .changed()
}

/// Adds the blank row once the user typed in it.
fn append_if_filled(rows: &mut Vec<KeyValue>, blank: KeyValue) -> bool {
    if blank.name.is_empty() && blank.value.is_empty() {
        return false;
    }
    rows.push(blank);
    true
}

#[cfg(test)]
mod tests {
    use egui_kittest::kittest::Queryable;

    use super::*;

    #[test]
    fn typing_in_the_blank_row_appends() {
        let mut rows = vec![KeyValue::new("a", "1")];
        assert!(!append_if_filled(&mut rows, KeyValue::new("", "")));
        assert!(append_if_filled(&mut rows, KeyValue::new("b", "")));
        assert_eq!(rows, [KeyValue::new("a", "1"), KeyValue::new("b", "")]);
    }

    #[test]
    fn renders_rows_plus_one_blank_row() {
        let mut rows = vec![KeyValue::new("Accept", "text/html")];
        let mut harness = crate::test_host::harness(|ui| {
            key_value_table(ui, "t", &mut rows, "Header", &VarHelp::default());
        });
        harness.run();
        assert!(harness.query_all_by_value("Accept").next().is_some());
        // One toggle and one remove button: the blank row has neither.
        assert_eq!(harness.query_all_by_label("Include").count(), 1);
        assert_eq!(harness.query_all_by_label("🗙").count(), 1);
    }

    #[test]
    fn removing_a_row() {
        let mut rows = vec![KeyValue::new("a", "1"), KeyValue::new("b", "2")];
        {
            let mut harness = crate::test_host::harness(|ui| {
                key_value_table(ui, "t", &mut rows, "Header", &VarHelp::default());
            });
            harness.run();
            harness
                .query_all_by_label("🗙")
                .next()
                .expect("remove button")
                .click();
            harness.run();
        }
        assert_eq!(rows, [KeyValue::new("b", "2")]);
    }
}

#[cfg(test)]
mod dialog_tests {
    use egui_kittest::kittest::Queryable;
    use tortuga_core::KeyValue;

    #[test]
    fn typing_in_a_table_inside_a_dialog() {
        let mut rows: Vec<KeyValue> = vec![];
        {
            let mut h = crate::test_host::harness(|ui| {
                egui::Modal::new(egui::Id::unique("m")).show(ui.ctx(), |ui| {
                    ui.set_width(600.0);
                    super::key_value_table(
                        ui,
                        "t",
                        &mut rows,
                        "Header",
                        &super::VarHelp::default(),
                    );
                });
            });
            h.run();
            let field = h
                .get_all_by_role(egui::accesskit::Role::TextInput)
                .next()
                .expect("name field");
            field.focus();
            field.type_text("X-Team");
            h.run();
        }
        assert_eq!(rows.first().map(|r| r.name.as_str()), Some("X-Team"));
    }
}
