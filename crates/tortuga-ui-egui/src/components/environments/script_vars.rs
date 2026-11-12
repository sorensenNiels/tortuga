//! A project's variables that its captures and scripts set (R-VAR-11, D79): read-only,
//! found in its files, with their values this session (R-TEST-5).

use std::collections::BTreeMap;
use std::path::PathBuf;

use egui::{Grid, RichText, Ui};
use tortuga_core::vars::set_by::{Place, SetBy, Setter};

use crate::theme;

/// One variable as the section shows it.
pub(super) struct Row<'a> {
    pub name: &'a str,
    pub places: &'a [SetBy],
    /// Its value this session; `None`: not set yet.
    pub value: Option<&'a str>,
    /// What "Add a default" can do for it.
    pub default: Default,
}

/// Whether a project variable can be added for a name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Default {
    /// Not a project variable yet.
    Missing,
    /// The project has a variable with this name.
    Exists,
    /// A `workspace.<name>`: its default belongs to the workspace (R-WS-8).
    Workspace,
}

/// What the user asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Action {
    /// Open the request at this path in the project.
    Open(PathBuf),
    /// Add the name as a project variable.
    AddDefault(String),
    /// Forget the project's values from this session.
    Clear,
}

/// The rows for `set_by`, with values from `value`; `has_variable` tells whether the
/// project already has a variable of that name.
pub(super) fn rows<'a>(
    set_by: &'a BTreeMap<String, Vec<SetBy>>,
    value: impl Fn(&str) -> Option<&'a str>,
    has_variable: impl Fn(&str) -> bool,
) -> Vec<Row<'a>> {
    set_by
        .iter()
        .map(|(name, places)| Row {
            name,
            places,
            value: value(name),
            default: if tortuga_core::runner::target(name).0 {
                Default::Workspace
            } else if has_variable(name) {
                Default::Exists
            } else {
                Default::Missing
            },
        })
        .collect()
}

/// The section; `editable` is false for a read-only project, `any_value` whether the
/// project has values to clear.
pub(super) fn section(
    ui: &mut Ui,
    rows: &[Row<'_>],
    editable: bool,
    any_value: bool,
) -> Option<Action> {
    let mut action = None;
    theme::section_label(ui, &t!("env-script-vars"));
    theme::hint(ui, t!("env-script-vars-hint"));
    ui.add_space(4.0);
    if rows.is_empty() {
        theme::hint(ui, t!("env-script-vars-none"));
        return None;
    }
    let muted = theme::palette(ui).muted;
    Grid::new("script-vars")
        .num_columns(4)
        .spacing([16.0, 6.0])
        .show(ui, |ui| {
            theme::section_label(ui, &t!("variable"));
            theme::section_label(ui, &t!("env-script-vars-set-by"));
            theme::section_label(ui, &t!("env-script-vars-value"));
            ui.label("");
            ui.end_row();
            for row in rows {
                ui.label(RichText::new(row.name).monospace());
                ui.vertical(|ui| {
                    for by in row.places {
                        if let Some(a) = place(ui, by) {
                            action = Some(a);
                        }
                    }
                });
                value_cell(ui, row.value, muted);
                match row.default {
                    Default::Missing => {
                        let add = ui
                            .add_enabled(
                                editable,
                                theme::ghost_button(t!("env-script-vars-add-default")),
                            )
                            .on_hover_text(t!("env-script-vars-add-default-hint"));
                        if add.clicked() {
                            action = Some(Action::AddDefault(row.name.to_owned()));
                        }
                    }
                    Default::Exists => {
                        ui.label(RichText::new(t!("env-script-vars-has-default")).color(muted));
                    }
                    Default::Workspace => {
                        ui.label(RichText::new(t!("env-script-vars-workspace")).color(muted));
                    }
                }
                ui.end_row();
            }
        });
    ui.add_space(6.0);
    let clear = ui
        .add_enabled(any_value, theme::ghost_button(t!("env-script-vars-clear")))
        .on_hover_text(t!("env-script-vars-clear-hint"));
    if clear.clicked() {
        action = Some(Action::Clear);
    }
    action
}

/// Where a variable is set: a request is a link that opens it.
fn place(ui: &mut Ui, by: &SetBy) -> Option<Action> {
    let setter = match by.setter {
        Setter::Capture => t!("env-script-vars-capture"),
        Setter::PreRequest => t!("env-script-vars-pre-request"),
        Setter::Tests => t!("env-script-vars-tests"),
    };
    let muted = theme::palette(ui).muted;
    ui.horizontal(|ui| match &by.place {
        Place::Request(path) => {
            let open = ui
                .link(by.name.as_str())
                .on_hover_text(t!("env-script-vars-open"))
                .clicked();
            ui.label(RichText::new(setter).color(muted));
            open.then(|| Action::Open(path.clone()))
        }
        Place::Workflow(_) => {
            ui.label(t!("env-script-vars-workflow", name = by.name.as_str()));
            ui.label(RichText::new(setter).color(muted));
            None
        }
    })
    .inner
}

/// The value this session; a credential-like one is hidden until hovered (P6, R-VAR-10).
fn value_cell(ui: &mut Ui, value: Option<&str>, muted: egui::Color32) {
    let Some(value) = value else {
        ui.label(
            RichText::new(t!("env-script-vars-not-set"))
                .color(muted)
                .italics(),
        );
        return;
    };
    if tortuga_core::check::credential::credential_in(value).is_some() {
        ui.label(RichText::new(t!("variable-secret-hidden")).color(muted))
            .on_hover_text(RichText::new(value).monospace());
        return;
    }
    let shown = crate::format::truncate(value, 200).0.to_owned();
    ui.add(egui::Label::new(RichText::new(shown).monospace()).truncate())
        .on_hover_text(value);
}

#[cfg(test)]
mod tests {
    use egui_kittest::kittest::Queryable;

    use super::*;

    fn by(path: &str, setter: Setter) -> SetBy {
        SetBy {
            place: Place::Request(path.into()),
            name: "Create batch".into(),
            setter,
        }
    }

    /// R-VAR-11: each name is listed with its value, or "not set yet", and "Add a
    /// default" adds a project variable; clicking the request asks to open it.
    #[test]
    fn rows_show_values_and_offer_actions() {
        let set_by: BTreeMap<String, Vec<SetBy>> = [
            ("batchId".to_owned(), vec![by("b.toml", Setter::PreRequest)]),
            ("ü😀".to_owned(), vec![by("b.toml", Setter::Capture)]),
            (
                "workspace.token".to_owned(),
                vec![by("b.toml", Setter::Tests)],
            ),
        ]
        .into();
        let values: BTreeMap<&str, &str> = [("batchId", "b-4711")].into();
        let rows = rows(&set_by, |n| values.get(n).copied(), |n| n == "ü😀");
        assert_eq!(rows[0].value, Some("b-4711"));
        assert_eq!(rows[0].default, Default::Missing);
        assert_eq!(rows[1].default, Default::Workspace);
        assert_eq!(rows[2].value, None);
        assert_eq!(rows[2].default, Default::Exists);

        let mut action = None;
        let mut harness = crate::test_host::harness(|ui| {
            if let Some(a) = section(ui, &rows, true, true) {
                action = Some(a);
            }
        });
        harness.run();
        harness.get_by_label("b-4711");
        assert_eq!(
            harness
                .query_all_by_label(&t!("env-script-vars-not-set"))
                .count(),
            2
        );
        harness
            .get_by_label(&t!("env-script-vars-add-default"))
            .click();
        harness.run();
        drop(harness);
        assert_eq!(action, Some(Action::AddDefault("batchId".into())));

        let mut action = None;
        let mut harness = crate::test_host::harness(|ui| {
            if let Some(a) = section(ui, &rows, true, true) {
                action = Some(a);
            }
        });
        harness.run();
        harness
            .query_all_by_label("Create batch")
            .next()
            .expect("link")
            .click();
        harness.run();
        drop(harness);
        assert_eq!(action, Some(Action::Open("b.toml".into())));
    }

    /// R-VAR-11, P6: a value that looks like a credential is not shown as text.
    #[test]
    fn credential_values_are_hidden() {
        let set_by: BTreeMap<String, Vec<SetBy>> =
            [("key".to_owned(), vec![by("b.toml", Setter::Tests)])].into();
        let token = concat!("ghp_", "0123456789abcdefghijABCDEFGHIJ012345");
        let rows = rows(&set_by, |_| Some(token), |_| false);
        let mut harness = crate::test_host::harness(|ui| {
            section(ui, &rows, true, true);
        });
        harness.run();
        assert!(harness.query_by_label(token).is_none());
        harness.get_by_label(&t!("variable-secret-hidden"));
    }

    #[test]
    fn a_project_without_script_variables_says_so() {
        let set_by = BTreeMap::new();
        let rows = rows(&set_by, |_| None, |_| false);
        let mut harness = crate::test_host::harness(|ui| {
            section(ui, &rows, true, false);
        });
        harness.run();
        harness.get_by_label(&t!("env-script-vars-none"));
    }
}
