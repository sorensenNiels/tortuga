use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use egui::{Context, Grid, ScrollArea, TextEdit, Ui};
use tortuga_core::{Collection, Environment, Manifest, Server};

use super::{DialogResult, dialog};
use crate::state::AppState;
use crate::theme;

/// Editable copy of a collection's servers, variables and environments.
/// Rows and columns are kept as parallel vectors so names can be edited
/// freely; ids are only assigned when saving (R-SRV-1).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Draft {
    pub environments: Vec<EnvColumn>,
    pub servers: Vec<ServerRow>,
    pub variables: Vec<VarRow>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct EnvColumn {
    /// `None` until saved for the first time.
    pub id: Option<String>,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ServerRow {
    pub id: Option<String>,
    pub name: String,
    /// One base URL per environment column.
    pub urls: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct VarRow {
    pub name: String,
    pub default: String,
    /// One value per environment column; empty = use the default (R-VAR-7).
    pub values: Vec<String>,
}

impl Draft {
    pub fn from_collection(c: &Collection) -> Self {
        let envs = &c.environments;
        let mut var_names: BTreeSet<&String> = c.manifest.variables.keys().collect();
        for e in envs {
            var_names.extend(e.variables.keys());
        }
        Self {
            environments: envs
                .iter()
                .map(|e| EnvColumn {
                    id: Some(e.id.clone()),
                    name: e.name.clone(),
                })
                .collect(),
            servers: c
                .manifest
                .servers
                .iter()
                .map(|s| ServerRow {
                    id: Some(s.id.clone()),
                    name: s.name.clone(),
                    urls: envs
                        .iter()
                        .map(|e| e.servers.get(&s.id).cloned().unwrap_or_default())
                        .collect(),
                })
                .collect(),
            variables: var_names
                .into_iter()
                .map(|name| VarRow {
                    name: name.clone(),
                    default: c.manifest.variables.get(name).cloned().unwrap_or_default(),
                    values: envs
                        .iter()
                        .map(|e| e.variables.get(name).cloned().unwrap_or_default())
                        .collect(),
                })
                .collect(),
        }
    }

    pub fn add_environment(&mut self) {
        let name = format!("Environment {}", self.environments.len() + 1);
        self.environments.push(EnvColumn { id: None, name });
        self.servers
            .iter_mut()
            .for_each(|s| s.urls.push(String::new()));
        self.variables
            .iter_mut()
            .for_each(|v| v.values.push(String::new()));
    }

    pub fn remove_environment(&mut self, col: usize) {
        self.environments.remove(col);
        self.servers
            .iter_mut()
            .for_each(|s| drop(s.urls.remove(col)));
        self.variables
            .iter_mut()
            .for_each(|v| drop(v.values.remove(col)));
    }

    pub fn add_server(&mut self, name: String) {
        let urls = vec![String::new(); self.environments.len()];
        self.servers.push(ServerRow {
            id: None,
            name,
            urls,
        });
    }

    pub fn add_variable(&mut self, name: String) {
        let values = vec![String::new(); self.environments.len()];
        self.variables.push(VarRow {
            name,
            default: String::new(),
            values,
        });
    }

    /// Checks the draft and converts it to what is written to disk.
    pub fn to_config(
        &self,
        base: &Manifest,
        old_envs: &[Environment],
    ) -> Result<(Manifest, Vec<Environment>), String> {
        let mut manifest = base.clone();
        let mut seen = BTreeSet::new();
        for e in &self.environments {
            if e.name.trim().is_empty() {
                return Err("Every environment needs a name.".into());
            }
            if !seen.insert(e.name.trim().to_lowercase()) {
                return Err(format!(
                    "There are two environments called “{}”.",
                    e.name.trim()
                ));
            }
        }

        manifest.servers = Vec::new();
        let mut server_ids = Vec::new();
        for s in self.servers.iter().filter(|s| !s.name.trim().is_empty()) {
            let id =
                s.id.clone()
                    .unwrap_or_else(|| Collection::new_server_id(&s.name, &manifest.servers));
            manifest.servers.push(Server {
                id: id.clone(),
                name: s.name.trim().to_owned(),
            });
            server_ids.push((id, s));
        }

        let mut var_seen = BTreeSet::new();
        let vars: Vec<&VarRow> = self
            .variables
            .iter()
            .filter(|v| !v.name.trim().is_empty())
            .collect();
        for v in &vars {
            let name = v.name.trim();
            if name.contains(['{', '}']) {
                return Err(format!("Variable names cannot contain braces: “{name}”."));
            }
            if !var_seen.insert(name) {
                return Err(format!("There are two variables called “{name}”."));
            }
        }
        manifest.variables = vars
            .iter()
            .filter(|v| !v.default.is_empty())
            .map(|v| (v.name.trim().to_owned(), v.default.clone()))
            .collect();

        let mut envs: Vec<Environment> = Vec::new();
        for (col, (seq, e)) in (1..).zip(&self.environments).enumerate() {
            let id = e.id.clone().unwrap_or_else(|| {
                let taken: Vec<Environment> = old_envs.iter().chain(&envs).cloned().collect();
                Collection::new_environment_id(&e.name, &taken)
            });
            let servers: BTreeMap<String, String> = server_ids
                .iter()
                .filter(|(_, s)| !s.urls[col].trim().is_empty())
                .map(|(id, s)| (id.clone(), s.urls[col].trim().to_owned()))
                .collect();
            let variables = vars
                .iter()
                .filter(|v| !v.values[col].is_empty())
                .map(|v| (v.name.trim().to_owned(), v.values[col].clone()))
                .collect();
            envs.push(Environment {
                id,
                name: e.name.trim().to_owned(),
                seq: Some(seq),
                servers,
                variables,
            });
        }
        Ok((manifest, envs))
    }
}

/// Servers × environments and variables × environments (R-SRV-2, R-VAR-2, R-VAR-7).
#[derive(Debug, Clone)]
pub struct EnvironmentsDialog {
    pub root: PathBuf,
    draft: Draft,
    new_server: String,
    new_variable: String,
    /// Shown when the draft cannot be saved.
    pub error: Option<String>,
}

const CELL_WIDTH: f32 = 170.0;

impl EnvironmentsDialog {
    pub fn new(state: &AppState, root: PathBuf) -> Self {
        let draft = state
            .collection(&root)
            .map(Draft::from_collection)
            .unwrap_or_default();
        Self {
            root,
            draft,
            new_server: String::new(),
            new_variable: String::new(),
            error: None,
        }
    }

    /// Done with the manifest and environments to write.
    pub fn show(
        &mut self,
        ctx: &Context,
        state: &AppState,
    ) -> DialogResult<(Manifest, Vec<Environment>)> {
        let Some(c) = state.collection(&self.root) else {
            return DialogResult::Close;
        };
        let title = format!("Environments — {}", c.name());
        let read_only = c.read_only;
        dialog(ctx, "environments", &title, 900.0, false, |ui| {
            theme::hint(
                ui,
                "Each server gets a base URL per environment; requests pick a server and add the path. \
                 Variables are used as {{name}}. An empty cell uses the default.",
            );
            ui.add_space(4.0);
            if ui
                .add_enabled(!read_only, egui::Button::new("+ Add environment"))
                .clicked()
            {
                self.draft.add_environment();
            }
            if self.draft.environments.is_empty() {
                theme::hint(
                    ui,
                    "No environments yet. Add one (for example “Development”) to give servers their addresses.",
                );
            }
            ScrollArea::both()
                .id_salt("env-matrix")
                .max_height(ctx.content_rect().height() * 0.6)
                .show(ui, |ui| {
                    servers_table(ui, &mut self.draft, &mut self.new_server);
                    ui.add_space(12.0);
                    variables_table(ui, &mut self.draft, &mut self.new_variable);
                });
            ui.add_space(10.0);
            theme::callout(
                ui,
                "These values are saved in the collection files and shared with everyone who has the repository. \
                 Don't enter real passwords or tokens yet — secret variables are coming in a later version.",
            );
            if let Some(err) = &self.error {
                theme::error(ui, err);
            }
            ui.add_space(8.0);
            let (cancel, save) = ui
                .with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let save = ui
                        .add_enabled(!read_only, theme::primary_button(ui, "Save"))
                        .clicked();
                    (ui.button("Cancel").clicked(), save)
                })
                .inner;
            if save {
                match self.draft.to_config(&c.manifest, &c.environments) {
                    Ok(config) => return DialogResult::Done(config),
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

fn servers_table(ui: &mut Ui, d: &mut Draft, new_server: &mut String) {
    Grid::new("servers")
        .num_columns(d.environments.len() + 1)
        .show(ui, |ui| {
            theme::section_label(ui, "Servers");
            let mut remove_env = None;
            for (col, e) in d.environments.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    theme::sized_edit(
                        ui,
                        CELL_WIDTH - 30.0,
                        TextEdit::singleline(&mut e.name).id_salt(("env", col)),
                    );
                    if ui
                        .add(theme::icon_button(ui, "🗙"))
                        .on_hover_text("Remove this environment")
                        .clicked()
                    {
                        remove_env = Some(col);
                    }
                });
            }
            ui.end_row();

            let mut remove = None;
            for (row, s) in d.servers.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    theme::sized_edit(
                        ui,
                        CELL_WIDTH - 30.0,
                        TextEdit::singleline(&mut s.name).id_salt(("server", row)),
                    );
                    if ui
                        .add(theme::icon_button(ui, "🗙"))
                        .on_hover_text("Remove this server")
                        .clicked()
                    {
                        remove = Some(row);
                    }
                });
                for (col, url) in s.urls.iter_mut().enumerate() {
                    theme::sized_edit(
                        ui,
                        CELL_WIDTH,
                        TextEdit::singleline(url)
                            .id_salt(("url", row, col))
                            .font(egui::TextStyle::Monospace)
                            .hint_text("https://…"),
                    );
                }
                ui.end_row();
            }
            if let Some(name) =
                new_row_input(ui, "new-server", new_server, "Add server (press Enter)…")
            {
                d.add_server(name);
            }
            ui.end_row();

            if let Some(row) = remove {
                d.servers.remove(row);
            }
            if let Some(col) = remove_env {
                d.remove_environment(col);
            }
        });
}

fn variables_table(ui: &mut Ui, d: &mut Draft, new_variable: &mut String) {
    Grid::new("variables")
        .num_columns(d.environments.len() + 2)
        .show(ui, |ui| {
            theme::section_label(ui, "Variables");
            theme::section_label(ui, "Default");
            for e in &d.environments {
                theme::section_label(ui, &e.name);
            }
            ui.end_row();

            let mut remove = None;
            for (row, v) in d.variables.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    theme::sized_edit(
                        ui,
                        CELL_WIDTH - 30.0,
                        TextEdit::singleline(&mut v.name)
                            .id_salt(("var", row))
                            .font(egui::TextStyle::Monospace),
                    );
                    if ui
                        .add(theme::icon_button(ui, "🗙"))
                        .on_hover_text("Remove this variable")
                        .clicked()
                    {
                        remove = Some(row);
                    }
                });
                theme::sized_edit(
                    ui,
                    CELL_WIDTH,
                    TextEdit::singleline(&mut v.default)
                        .id_salt(("default", row))
                        .font(egui::TextStyle::Monospace),
                );
                for (col, value) in v.values.iter_mut().enumerate() {
                    theme::sized_edit(
                        ui,
                        CELL_WIDTH,
                        TextEdit::singleline(value)
                            .id_salt(("value", row, col))
                            .font(egui::TextStyle::Monospace)
                            .hint_text(v.default.clone()),
                    );
                }
                ui.end_row();
            }
            if let Some(name) = new_row_input(
                ui,
                "new-variable",
                new_variable,
                "Add variable (press Enter)…",
            ) {
                d.add_variable(name);
            }
            ui.end_row();

            if let Some(row) = remove {
                d.variables.remove(row);
            }
        });
}

/// A blank input that adds a row when Enter is pressed or the field is left.
fn new_row_input(ui: &mut Ui, id: &str, value: &mut String, hint: &str) -> Option<String> {
    let response = theme::sized_edit(
        ui,
        CELL_WIDTH,
        TextEdit::singleline(value)
            .id_salt(id)
            .hint_text(hint.to_owned()),
    );
    let name = value.trim().to_owned();
    if response.lost_focus() && !name.is_empty() {
        value.clear();
        return Some(name);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft() -> Draft {
        let mut d = Draft::default();
        d.add_environment();
        d.environments[0].name = "Development".into();
        d.add_environment();
        d.environments[1].name = "Staging".into();
        d.add_server("Users API".into());
        d.servers[0].urls = vec!["http://localhost:8080".into(), String::new()];
        d.add_variable("token".into());
        d.variables[0].default = "none".into();
        d.variables[0].values = vec!["dev-token".into(), String::new()];
        d
    }

    #[test]
    fn draft_converts_to_files_with_generated_ids() {
        let (manifest, envs) = draft().to_config(&Manifest::new("C"), &[]).expect("valid");
        assert_eq!(
            manifest.servers,
            [Server {
                id: "users-api".into(),
                name: "Users API".into()
            }]
        );
        assert_eq!(
            manifest.variables,
            BTreeMap::from([("token".into(), "none".into())])
        );
        assert_eq!(
            envs.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
            ["development", "staging"]
        );
        assert_eq!(envs[0].servers["users-api"], "http://localhost:8080");
        assert!(envs[1].servers.is_empty(), "empty cells are not written");
        assert_eq!(envs[0].variables["token"], "dev-token");
        assert!(
            envs[1].variables.is_empty(),
            "empty cell falls back to the default"
        );
    }

    #[test]
    fn existing_ids_survive_renames() {
        let (manifest, envs) = draft().to_config(&Manifest::new("C"), &[]).expect("valid");
        let col = Collection {
            root: PathBuf::new(),
            manifest,
            read_only: false,
            items: vec![],
            environments: envs,
            broken_environments: vec![],
        };
        let mut d = Draft::from_collection(&col);
        d.servers[0].name = "People API".into();
        d.environments[0].name = "Local".into();
        let (manifest, envs) = d
            .to_config(&col.manifest, &col.environments)
            .expect("valid");
        assert_eq!(manifest.servers[0].id, "users-api");
        assert_eq!(
            (envs[0].id.as_str(), envs[0].name.as_str()),
            ("development", "Local")
        );
    }

    #[test]
    fn rejects_duplicates_and_blank_environment_names() {
        let mut d = draft();
        d.add_variable("token".into());
        assert!(
            d.to_config(&Manifest::new("C"), &[])
                .expect_err("dup")
                .contains("two variables")
        );
        let mut d = draft();
        d.environments[1].name = " ".into();
        assert!(d.to_config(&Manifest::new("C"), &[]).is_err());
    }

    #[test]
    fn removing_an_environment_removes_its_column() {
        let mut d = draft();
        d.remove_environment(0);
        assert_eq!(d.servers[0].urls, [""]);
        assert_eq!(d.variables[0].values, [""]);
    }
}
