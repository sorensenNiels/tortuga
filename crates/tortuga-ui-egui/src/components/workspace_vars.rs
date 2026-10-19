//! The workspace variables dialog (R-WS-8): values every project of the workspace can
//! use, secret ones kept on this machine, and the values captured this session.

use std::collections::{BTreeMap, BTreeSet};

use egui::{Context, Grid, RichText, ScrollArea, TextEdit, Ui};
use tortuga_core::runner::WORKSPACE_PREFIX;
use tortuga_core::workspace::WorkspaceVariables;

use super::variable_row::{self, SecretHelp};
use super::{DialogResult, dialog};
use crate::state::AppState;
use crate::theme;

/// One variable as edited.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WorkspaceVarRow {
    pub name: String,
    pub value: String,
    pub secret: bool,
}

/// What the user asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceVarsAction {
    /// Save these variables, with this machine's secret values by name.
    Save(WorkspaceVariables, BTreeMap<String, String>),
    /// Forget the captured values; the dialog stays open.
    ClearCaptured,
}

#[derive(Debug, Clone, Default)]
pub struct WorkspaceVarsDialog {
    rows: Vec<WorkspaceVarRow>,
    new_variable: String,
    /// Shown when the rows cannot be saved.
    pub error: Option<String>,
}

const NAME_WIDTH: f32 = 220.0;
const VALUE_WIDTH: f32 = 340.0;
const SECRET_HELP: SecretHelp = SecretHelp {
    secret: "Secret: the value stays on this machine. Click to share it.",
    shared: "Shared with the workspace. Click to make it secret (the value stays on this machine).",
};

impl WorkspaceVarsDialog {
    pub fn new(state: &AppState) -> Self {
        let vars = &state.workspace().variables;
        let mut rows: Vec<WorkspaceVarRow> = vars
            .values
            .iter()
            .map(|(name, value)| WorkspaceVarRow {
                name: name.clone(),
                value: value.clone(),
                secret: false,
            })
            .chain(vars.secrets.iter().map(|name| {
                WorkspaceVarRow {
                    name: name.clone(),
                    value: state
                        .workspace_secrets
                        .get(name)
                        .cloned()
                        .unwrap_or_default(),
                    secret: true,
                }
            }))
            .collect();
        rows.sort_by(|a, b| a.name.cmp(&b.name));
        Self {
            rows,
            ..Self::default()
        }
    }

    pub fn show(&mut self, ctx: &Context, state: &AppState) -> DialogResult<WorkspaceVarsAction> {
        let ws = state.workspace();
        let title = format!("Workspace variables — {}", ws.name);
        let captured = state.workspace_runtime_vars();
        dialog(ctx, "workspace-vars", &title, 640.0, false, |ui| {
            theme::hint(
                ui,
                "Used as {{name}} by every project in this workspace and by the scratch request. \
                 A value the project defines (project, folder, environment or secret) wins over these.",
            );
            ui.add_space(8.0);
            ScrollArea::vertical()
                .id_salt("workspace-vars")
                .max_height(ctx.content_rect().height() * 0.4)
                .show(ui, |ui| {
                    variables_table(ui, &mut self.rows, &mut self.new_variable);
                });
            ui.add_space(10.0);
            theme::callout(
                ui,
                match &ws.file {
                    Some(file) => format!(
                        "Saved in {} — commit it to share these with your team. Secret (🔒) values \
                         stay on this machine; teammates see the name and fill in their own.",
                        file.display()
                    ),
                    None => {
                        "Kept on this machine only. Choose “Save as file…” in the workspace menu \
                             to share them with your team; secret (🔒) values stay on this machine."
                            .to_owned()
                    }
                },
            );
            if let Some(warning) = state.host.secrets_warning() {
                ui.add_space(4.0);
                theme::callout(ui, warning);
            }
            ui.add_space(12.0);
            let clear = captured_section(ui, captured);
            if let Some(err) = &self.error {
                theme::error(ui, err);
            }
            ui.add_space(12.0);
            let (cancel, save) = ui
                .with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let save = ui.add(theme::primary_button(ui, "Save")).clicked();
                    (ui.button("Cancel").clicked(), save)
                })
                .inner;
            if clear {
                return DialogResult::Done(WorkspaceVarsAction::ClearCaptured);
            }
            if save {
                match to_variables(&self.rows) {
                    Ok((vars, secrets)) => {
                        return DialogResult::Done(WorkspaceVarsAction::Save(vars, secrets));
                    }
                    Err(e) => self.error = Some(e),
                }
            }
            if cancel {
                DialogResult::Close
            } else {
                DialogResult::Open
            }
        })
    }
}

fn variables_table(ui: &mut Ui, rows: &mut Vec<WorkspaceVarRow>, new_variable: &mut String) {
    Grid::new("workspace-variables")
        .num_columns(2)
        .show(ui, |ui| {
            theme::section_label(ui, "Variable");
            theme::section_label(ui, "Value");
            ui.end_row();
            let mut remove = None;
            for (i, row) in rows.iter_mut().enumerate() {
                if variable_row::name_cell(
                    ui,
                    ("ws-var", i),
                    NAME_WIDTH,
                    &mut row.name,
                    &mut row.secret,
                    SECRET_HELP,
                ) {
                    remove = Some(i);
                }
                theme::sized_edit(
                    ui,
                    VALUE_WIDTH,
                    TextEdit::singleline(&mut row.value)
                        .id_salt(("ws-value", i))
                        .password(row.secret)
                        .hint_text(if row.secret { "not set" } else { "value" })
                        .font(egui::TextStyle::Monospace),
                );
                ui.end_row();
            }
            if let Some(name) = variable_row::new_row_input(
                ui,
                "ws-new-variable",
                NAME_WIDTH,
                new_variable,
                "Add variable (press Enter)…",
            ) {
                rows.push(WorkspaceVarRow {
                    name,
                    ..WorkspaceVarRow::default()
                });
            }
            ui.end_row();
            if let Some(i) = remove {
                rows.remove(i);
            }
        });
}

/// The values captured this session, with "Clear captured". Returns whether it was clicked.
fn captured_section(ui: &mut Ui, captured: Option<&BTreeMap<String, String>>) -> bool {
    theme::section_label(ui, "Captured this session");
    theme::hint(
        ui,
        format!(
            "Name a capture or vars.set() {WORKSPACE_PREFIX}<name> (for example \
             {WORKSPACE_PREFIX}token) to set {{{{<name>}}}} for every project. Captured values win \
             over saved ones and are forgotten when Tortuga closes."
        ),
    );
    let Some(captured) = captured else {
        theme::hint(ui, "None yet.");
        return false;
    };
    let p = theme::palette(ui);
    Grid::new("workspace-captured")
        .num_columns(2)
        .spacing([16.0, 4.0])
        .show(ui, |ui| {
            for (name, value) in captured {
                ui.label(RichText::new(name).monospace());
                let shown = crate::format::truncate(value, 200).0.to_owned();
                ui.add(
                    egui::Label::new(RichText::new(shown).monospace().color(p.muted)).truncate(),
                )
                .on_hover_text(value);
                ui.end_row();
            }
        });
    ui.add(theme::ghost_button("Clear captured"))
        .on_hover_text("Forget the captured values")
        .clicked()
}

/// Checks the rows and splits them into what is shared and this machine's secret values.
pub fn to_variables(
    rows: &[WorkspaceVarRow],
) -> Result<(WorkspaceVariables, BTreeMap<String, String>), String> {
    let mut seen = BTreeSet::new();
    let mut vars = WorkspaceVariables::default();
    let mut secrets = BTreeMap::new();
    for row in rows.iter().filter(|r| !r.name.trim().is_empty()) {
        let name = row.name.trim();
        if name.contains(['{', '}']) {
            return Err(format!("Variable names cannot contain braces: “{name}”."));
        }
        if let Some(rest) = name.strip_prefix(WORKSPACE_PREFIX) {
            return Err(format!(
                "Leave out “{WORKSPACE_PREFIX}”: “{name}” is used as {{{{{rest}}}}}."
            ));
        }
        if !seen.insert(name.to_owned()) {
            return Err(format!("There are two variables called “{name}”."));
        }
        if row.secret {
            vars.secrets.insert(name.to_owned());
            secrets.insert(name.to_owned(), row.value.clone());
        } else {
            vars.values.insert(name.to_owned(), row.value.clone());
        }
    }
    Ok((vars, secrets))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(name: &str, value: &str, secret: bool) -> WorkspaceVarRow {
        WorkspaceVarRow {
            name: name.into(),
            value: value.into(),
            secret,
        }
    }

    /// R-WS-8: shared values and secret values are split; names are checked.
    #[test]
    fn rows_become_variables_and_secrets() {
        let (vars, secrets) = to_variables(&[
            row(" tenant ", "acme", false),
            row("api_key", "k-1", true),
            row("", "ignored", false),
        ])
        .expect("valid");
        assert_eq!(
            vars.values,
            BTreeMap::from([("tenant".into(), "acme".into())])
        );
        assert_eq!(vars.secrets, BTreeSet::from(["api_key".into()]));
        assert_eq!(secrets, BTreeMap::from([("api_key".into(), "k-1".into())]));

        assert!(to_variables(&[row("a", "", false), row("a", "", true)]).is_err());
        assert!(to_variables(&[row("{{a}}", "", false)]).is_err());
        let err = to_variables(&[row("workspace.token", "", false)]).expect_err("prefix");
        assert!(err.contains("{{token}}"), "{err}");
    }
}
