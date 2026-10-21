use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use egui::{Atom, CentralPanel, Context, Frame, Grid, Panel, ScrollArea, TextEdit, Ui};
use tortuga_core::secrets::SecretValues;
use tortuga_core::{Collection, Environment, EnvironmentColor, Manifest, Server};

use super::variable_row::{self, SecretHelp, new_row_input};
use super::{DialogResult, resizable_dialog};
use crate::state::AppState;
use crate::theme;

/// Editable copy of a collection's servers, variables and environments.
/// Rows and columns are kept as parallel vectors so names can be edited
/// freely; ids are only assigned when saving (R-SRV-1).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Draft {
    pub environments: Vec<EnvColumn>,
    pub servers: Vec<ServerRow>,
    pub variables: Vec<VarRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EnvColumn {
    /// `None` until saved for the first time.
    pub id: Option<String>,
    pub name: String,
    /// Its dot's colour (R-SRV-8); `None`: the default.
    pub color: Option<EnvironmentColor>,
    /// Ask before writes are sent to it (R-SRV-9).
    pub protected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ServerRow {
    pub id: Option<String>,
    pub name: String,
    /// One base URL per environment column.
    pub urls: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VarRow {
    pub name: String,
    /// Values stay on this machine instead of the collection files (R-VAR-3).
    pub secret: bool,
    pub default: String,
    /// One value per environment column; empty = use the default (R-VAR-7).
    pub values: Vec<String>,
}

/// What saving the environments dialog writes: files, and local secret values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedConfig {
    pub manifest: Manifest,
    pub environments: Vec<Environment>,
    pub secrets: SecretValues,
}

impl Draft {
    pub fn from_collection(c: &Collection, secrets: &SecretValues) -> Self {
        let envs = &c.environments;
        let mut var_names: BTreeSet<&String> = c.manifest.variables.keys().collect();
        var_names.extend(&c.manifest.secrets);
        for e in envs {
            var_names.extend(e.variables.keys());
        }
        let is_secret = |name: &str| c.manifest.secrets.iter().any(|s| s == name);
        Self {
            environments: envs
                .iter()
                .map(|e| EnvColumn {
                    id: Some(e.id.clone()),
                    name: e.name.clone(),
                    color: e.color,
                    protected: e.protected,
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
                .map(|name| {
                    if is_secret(name) {
                        let local = |env: Option<&str>| {
                            secrets.get(env, name).unwrap_or_default().to_owned()
                        };
                        VarRow {
                            name: name.clone(),
                            secret: true,
                            default: local(None),
                            values: envs.iter().map(|e| local(Some(&e.id))).collect(),
                        }
                    } else {
                        VarRow {
                            name: name.clone(),
                            secret: false,
                            default: c.manifest.variables.get(name).cloned().unwrap_or_default(),
                            values: envs
                                .iter()
                                .map(|e| e.variables.get(name).cloned().unwrap_or_default())
                                .collect(),
                        }
                    }
                })
                .collect(),
        }
    }

    pub fn add_environment(&mut self) {
        let name = format!("Environment {}", self.environments.len() + 1);
        self.environments.push(EnvColumn {
            id: None,
            name,
            color: None,
            protected: false,
        });
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

    /// Moves environment column `from` to position `to`, with its URLs and values (R-SRV-6).
    pub fn move_environment(&mut self, from: usize, to: usize) {
        if from == to || from >= self.environments.len() || to >= self.environments.len() {
            return;
        }
        move_item(&mut self.environments, from, to);
        for s in &mut self.servers {
            move_item(&mut s.urls, from, to);
        }
        for v in &mut self.variables {
            move_item(&mut v.values, from, to);
        }
    }

    /// Moves server row `from` to position `to` (R-SRV-6).
    pub fn move_server(&mut self, from: usize, to: usize) {
        if from < self.servers.len() && to < self.servers.len() {
            move_item(&mut self.servers, from, to);
        }
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
            secret: false,
            default: String::new(),
            values,
        });
    }

    /// Checks the draft and converts it to what is written to disk, plus the
    /// secret values that stay on this machine.
    pub fn to_config(
        &self,
        base: &Manifest,
        old_envs: &[Environment],
    ) -> Result<SavedConfig, String> {
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
        let (secret_vars, vars): (Vec<&VarRow>, Vec<&VarRow>) =
            vars.into_iter().partition(|v| v.secret);
        manifest.variables = vars
            .iter()
            .filter(|v| !v.default.is_empty())
            .map(|v| (v.name.trim().to_owned(), v.default.clone()))
            .collect();
        manifest.secrets = secret_vars
            .iter()
            .map(|v| v.name.trim().to_owned())
            .collect();
        manifest.secrets.sort();
        let mut secrets = SecretValues::default();
        for v in &secret_vars {
            if !v.default.is_empty() {
                secrets
                    .values
                    .insert((None, v.name.trim().to_owned()), v.default.clone());
            }
        }

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
            for v in secret_vars.iter().filter(|v| !v.values[col].is_empty()) {
                secrets.values.insert(
                    (Some(id.clone()), v.name.trim().to_owned()),
                    v.values[col].clone(),
                );
            }
            envs.push(Environment {
                id,
                name: e.name.trim().to_owned(),
                seq: Some(seq),
                color: e.color,
                protected: e.protected,
                servers,
                variables,
            });
        }
        Ok(SavedConfig {
            manifest,
            environments: envs,
            secrets,
        })
    }
}

fn move_item<T>(list: &mut Vec<T>, from: usize, to: usize) {
    let item = list.remove(from);
    list.insert(to, item);
}

/// An environment or server row being dragged (R-SRV-6).
#[derive(Debug, Clone, Copy)]
struct EnvDrag(usize);
#[derive(Debug, Clone, Copy)]
struct ServerDrag(usize);

/// A handle to drag an environment or server by.
fn drag_handle<P: Clone + Send + Sync + 'static>(ui: &mut Ui, id: (&str, usize), payload: P) {
    let id = ui.scope_id().with(id);
    ui.dnd_drag_source(id, payload, |ui| {
        ui.label(egui::RichText::new("☰").color(theme::palette(ui).muted))
    })
    .response
    .on_hover_text("Drag to change the order");
}

/// Makes the list row `zone` a drop target for `P`. Returns the dragged index if dropped
/// here, and marks the edge it will land on.
fn drop_index<P: Copy + Send + Sync + 'static>(
    ui: &Ui,
    zone: &egui::Response,
    index: usize,
    from: impl Fn(&P) -> usize,
) -> Option<usize> {
    let dragged = from(&*zone.dnd_hover_payload::<P>()?);
    if dragged == index {
        return None;
    }
    let r = zone.rect;
    // Lands before this one when coming from below it, else after it.
    let y = if dragged > index {
        r.top() - 2.0
    } else {
        r.bottom() + 2.0
    };
    let stroke = egui::Stroke::new(2.0, theme::palette(ui).accent);
    ui.painter().hline(r.x_range(), y, stroke);
    zone.dnd_release_payload::<P>().map(|p| from(&p))
}

/// What the right side of the dialog shows: one environment at a time (R-SRV-7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shown {
    /// The names of servers and variables, secret flags and default values, which every
    /// environment shares.
    Defaults,
    Environment(usize),
}

impl Shown {
    /// What to show once environment `col` is removed and `left` environments remain:
    /// the same one, or its neighbour when it was the one removed.
    fn after_remove(self, col: usize, left: usize) -> Self {
        match self {
            Self::Environment(i) if i == col && left == 0 => Self::Defaults,
            Self::Environment(i) if i == col => Self::Environment(col.min(left - 1)),
            Self::Environment(i) if i > col => Self::Environment(i - 1),
            Self::Environment(_) | Self::Defaults => self,
        }
    }

    /// What to show once environment `from` moved to `to`: the same environment.
    fn after_move(self, from: usize, to: usize) -> Self {
        match self {
            Self::Environment(i) if i == from => Self::Environment(to),
            Self::Environment(i) if from < i && i <= to => Self::Environment(i - 1),
            Self::Environment(i) if to <= i && i < from => Self::Environment(i + 1),
            Self::Environment(_) | Self::Defaults => self,
        }
    }
}

/// Edits a project's environments one at a time, plus the servers and variables they share
/// (R-SRV-2, R-VAR-2, R-VAR-7, R-SRV-7).
#[derive(Debug, Clone)]
pub struct EnvironmentsDialog {
    pub root: PathBuf,
    draft: Draft,
    shown: Shown,
    new_server: String,
    new_variable: String,
    /// Shown when the draft cannot be saved.
    pub error: Option<String>,
}

/// Width of the environment list on the left.
const LIST_WIDTH: f32 = 200.0;

/// Space left of the chosen environment's view, after the list's separator line.
const DETAIL_PADDING: i8 = 16;

/// Width of the name column in the server and variable tables: a share of the room, so long
/// names are not truncated (BUG-1), within sensible bounds.
fn name_width(total: f32) -> f32 {
    (total * 0.4).clamp(160.0, 320.0).floor()
}

impl EnvironmentsDialog {
    pub fn new(state: &AppState, root: PathBuf) -> Self {
        let secrets = state.secrets.get(&root).cloned().unwrap_or_default();
        let collection = state.collection(&root);
        let draft = collection
            .map(|c| Draft::from_collection(c, &secrets))
            .unwrap_or_default();
        // Opens on the environment requests are sent with, where edits matter most.
        let active = state.active_environment(&root);
        let shown = draft
            .environments
            .iter()
            .position(|e| e.id.is_some() && e.id == active)
            .or_else(|| (!draft.environments.is_empty()).then_some(0))
            .map_or(Shown::Defaults, Shown::Environment);
        Self {
            root,
            draft,
            shown,
            new_server: String::new(),
            new_variable: String::new(),
            error: None,
        }
    }

    /// Done with the files to write and the secret values to keep locally.
    pub fn show(&mut self, ctx: &Context, state: &AppState) -> DialogResult<SavedConfig> {
        let Some(c) = state.collection(&self.root) else {
            return DialogResult::Close;
        };
        let title = format!("Environments — {}", c.name());
        let read_only = c.read_only;
        resizable_dialog(
            ctx,
            "environments",
            &title,
            egui::vec2(820.0, 520.0),
            egui::vec2(560.0, 360.0),
            |ui| {
                let (cancel, save) = Panel::bottom("env-footer")
                    .frame(Frame::NONE.inner_margin(egui::Margin {
                        top: 10,
                        ..egui::Margin::ZERO
                    }))
                    .show_separator_line(false)
                    .show(ui, |ui| footer(ui, state, self.error.as_deref(), read_only))
                    .inner;
                Panel::left("env-list")
                    .frame(Frame::NONE.inner_margin(egui::Margin {
                        right: 12,
                        ..egui::Margin::ZERO
                    }))
                    .resizable(false)
                    .exact_size(LIST_WIDTH)
                    .show(ui, |ui| {
                        environment_list(ui, &mut self.draft, &mut self.shown, read_only);
                    });
                CentralPanel::no_frame().show(ui, |ui| {
                    // Room between the list's separator line and the names and fields.
                    let padding = egui::Margin {
                        left: DETAIL_PADDING,
                        ..egui::Margin::ZERO
                    };
                    Frame::NONE.inner_margin(padding).show(ui, |ui| {
                        ScrollArea::vertical()
                            .id_salt("env-detail")
                            .auto_shrink([false, false])
                            .show(ui, |ui| match self.shown {
                                Shown::Defaults => defaults_view(
                                    ui,
                                    &mut self.draft,
                                    &mut self.new_server,
                                    &mut self.new_variable,
                                ),
                                Shown::Environment(col) => {
                                    if let Some(left) = environment_view(
                                        ui,
                                        &mut self.draft,
                                        col,
                                        &mut self.new_server,
                                        &mut self.new_variable,
                                    ) {
                                        self.shown = self.shown.after_remove(col, left);
                                    }
                                }
                            });
                    });
                });
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
            },
        )
    }
}

/// The notes, any error, and Cancel/Save. Returns (cancel clicked, save clicked).
fn footer(ui: &mut Ui, state: &AppState, error: Option<&str>, read_only: bool) -> (bool, bool) {
    theme::callout(
        ui,
        "Values are saved in the project files and shared through git. Mark a variable as \
         secret (🔒) under Defaults to keep its values on this machine only — teammates see \
         the name, not the value.",
    );
    if let Some(warning) = state.host.secrets_warning() {
        ui.add_space(4.0);
        theme::callout(ui, warning);
    }
    if let Some(err) = error {
        theme::error(ui, err);
    }
    ui.add_space(8.0);
    theme::button_row(ui, |ui| {
        let save = ui
            .add_enabled(!read_only, theme::primary_button(ui, "Save"))
            .clicked();
        (ui.button("Cancel").clicked(), save)
    })
}

/// "Defaults" and the environments, in order; click one to edit it, drag to reorder (R-SRV-6).
fn environment_list(ui: &mut Ui, d: &mut Draft, shown: &mut Shown, read_only: bool) {
    let row_width = ui.available_width();
    if ui
        .add(
            egui::Button::selectable(*shown == Shown::Defaults, ("Defaults", Atom::grow()))
                .min_size(egui::vec2(row_width, 0.0)),
        )
        .on_hover_text("Servers, variables and the values every environment starts from")
        .clicked()
    {
        *shown = Shown::Defaults;
    }
    ui.add_space(6.0);
    theme::section_label(ui, "Environments");
    let mut move_env = None;
    for (col, e) in d.environments.iter().enumerate() {
        let row = ui.horizontal(|ui| {
            drag_handle(ui, ("env-drag", col), EnvDrag(col));
            let name = if e.name.trim().is_empty() {
                "(no name)"
            } else {
                e.name.as_str()
            };
            let selected = *shown == Shown::Environment(col);
            let width = ui.available_width();
            // Long names are cut short, left-aligned like "Defaults".
            if theme::environment_button(ui, selected, e.color, name, width).clicked() {
                *shown = Shown::Environment(col);
            }
        });
        if let Some(from) = drop_index(ui, &row.response, col, |p: &EnvDrag| p.0) {
            move_env = Some((from, col));
        }
    }
    if d.environments.is_empty() {
        theme::hint(
            ui,
            "No environments yet. Add one (for example “Development”) to give servers their addresses.",
        );
    }
    ui.add_space(6.0);
    if ui
        .add_enabled(!read_only, theme::ghost_button("+ Add environment"))
        .clicked()
    {
        d.add_environment();
        *shown = Shown::Environment(d.environments.len() - 1);
    }
    if let Some((from, to)) = move_env {
        d.move_environment(from, to);
        *shown = shown.after_move(from, to);
    }
}

/// Server and variable names, secret flags and defaults (R-VAR-7).
fn defaults_view(ui: &mut Ui, d: &mut Draft, new_server: &mut String, new_variable: &mut String) {
    theme::hint(
        ui,
        "Servers and variables are shared by every environment; each environment sets a \
         server's base URL and can override a variable's default. Variables are used as {{name}}.",
    );
    ui.add_space(8.0);
    let total = ui.available_width() - ui.spacing().scroll.bar_width;
    let names = name_width(total);
    let values = (total - names - ui.spacing().item_spacing.x).max(120.0);

    theme::section_label(ui, "Servers");
    let mut remove = None;
    let mut move_server = None;
    Grid::new("default-servers").num_columns(1).show(ui, |ui| {
        for (row, s) in d.servers.iter_mut().enumerate() {
            let cell = ui.horizontal(|ui| {
                drag_handle(ui, ("server-drag", row), ServerDrag(row));
                theme::sized_edit(
                    ui,
                    names - 50.0,
                    TextEdit::singleline(&mut s.name).id_salt(("server", row)),
                );
                if ui
                    .add(theme::icon_button(ui, "🗙"))
                    .on_hover_text("Remove this server from every environment")
                    .clicked()
                {
                    remove = Some(row);
                }
            });
            if let Some(from) = drop_index(ui, &cell.response, row, |p: &ServerDrag| p.0) {
                move_server = Some((from, row));
            }
            ui.end_row();
        }
        add_server_row(ui, d, names, new_server);
    });
    if let Some(row) = remove {
        d.servers.remove(row);
    }
    if let Some((from, to)) = move_server {
        d.move_server(from, to);
    }

    ui.add_space(14.0);
    let mut remove = None;
    Grid::new("default-variables")
        .num_columns(2)
        .show(ui, |ui| {
            theme::section_label(ui, "Variables");
            theme::section_label(ui, "Default");
            ui.end_row();
            for (row, v) in d.variables.iter_mut().enumerate() {
                if variable_row::name_cell(
                    ui,
                    ("var", row),
                    names,
                    &mut v.name,
                    &mut v.secret,
                    SECRET_HELP,
                ) {
                    remove = Some(row);
                }
                theme::sized_edit(
                    ui,
                    values,
                    TextEdit::singleline(&mut v.default)
                        .id_salt(("default", row))
                        .password(v.secret)
                        .hint_text(if v.secret { "not set" } else { "" })
                        .font(egui::TextStyle::Monospace),
                );
                ui.end_row();
            }
            add_variable_row(ui, d, names, new_variable);
        });
    if let Some(row) = remove {
        d.variables.remove(row);
    }
}

/// An environment's colour (R-SRV-8) and whether it is protected (R-SRV-9).
fn colour_and_protection(ui: &mut Ui, env: &mut EnvColumn) {
    ui.horizontal(|ui| {
        ui.label("Colour");
        color_choice(ui, &mut env.color);
    });
    ui.checkbox(&mut env.protected, "Protected")
        .on_hover_text("Saved in the environment's file, so it is protected for the whole team");
    theme::hint(
        ui,
        "Asks before anything but GET, HEAD or OPTIONS is sent here, and before a workflow runs \
         here. tortuga-cli refuses such runs without --allow-protected.",
    );
}

/// One environment: its name, each server's base URL and each variable's value (R-SRV-7).
/// Returns how many environments are left when this one was removed.
fn environment_view(
    ui: &mut Ui,
    d: &mut Draft,
    col: usize,
    new_server: &mut String,
    new_variable: &mut String,
) -> Option<usize> {
    let total = ui.available_width() - ui.spacing().scroll.bar_width;
    let names = name_width(total);
    let values = (total - names - ui.spacing().item_spacing.x).max(120.0);
    let env = d.environments.get_mut(col)?;
    let remove = ui
        .horizontal(|ui| {
            ui.label("Name");
            theme::sized_edit(
                ui,
                names,
                TextEdit::singleline(&mut env.name).id_salt(("env-name", col)),
            );
            ui.add(theme::ghost_button("Remove environment")).clicked()
        })
        .inner;
    if remove {
        d.remove_environment(col);
        return Some(d.environments.len());
    }
    colour_and_protection(ui, env);
    ui.add_space(10.0);

    let mut remove_server = None;
    Grid::new(("env-servers", col))
        .num_columns(2)
        .show(ui, |ui| {
            theme::section_label(ui, "Servers");
            theme::section_label(ui, "Base URL");
            ui.end_row();
            for (row, s) in d.servers.iter_mut().enumerate() {
                if name_label(ui, names, &s.name, false, REMOVE_SERVER) {
                    remove_server = Some(row);
                }
                if let Some(url) = s.urls.get_mut(col) {
                    theme::sized_edit(
                        ui,
                        values,
                        TextEdit::singleline(url)
                            .id_salt(("url", row, col))
                            .font(egui::TextStyle::Monospace)
                            .hint_text("https://…"),
                    );
                }
                ui.end_row();
            }
            add_server_row(ui, d, names, new_server);
        });
    if let Some(row) = remove_server {
        d.servers.remove(row);
    }

    ui.add_space(14.0);
    let mut remove_variable = None;
    Grid::new(("env-variables", col))
        .num_columns(2)
        .show(ui, |ui| {
            theme::section_label(ui, "Variables");
            theme::section_label(ui, "Value");
            ui.end_row();
            for (row, v) in d.variables.iter_mut().enumerate() {
                if name_label(ui, names, &v.name, v.secret, REMOVE_VARIABLE) {
                    remove_variable = Some(row);
                }
                // Secret values are masked; a placeholder never shows the secret default.
                let hint = match (v.secret, v.default.is_empty()) {
                    (true, true) => "not set".to_owned(),
                    (true, false) => "default".to_owned(),
                    (false, _) => v.default.clone(),
                };
                if let Some(value) = v.values.get_mut(col) {
                    theme::sized_edit(
                        ui,
                        values,
                        TextEdit::singleline(value)
                            .id_salt(("value", row, col))
                            .password(v.secret)
                            .font(egui::TextStyle::Monospace)
                            .hint_text(hint),
                    );
                }
                ui.end_row();
            }
            add_variable_row(ui, d, names, new_variable);
        });
    if let Some(row) = remove_variable {
        d.variables.remove(row);
    }
    ui.add_space(6.0);
    theme::hint(
        ui,
        "An empty value uses the default. Servers and variables belong to every environment: \
         🗙 removes them from all. Rename them or mark variables secret under Defaults.",
    );
    None
}

/// Hover texts of the remove buttons in an environment's tables.
const REMOVE_SERVER: &str = "Remove this server from every environment";
const REMOVE_VARIABLE: &str = "Remove this variable from every environment";

/// A server or variable name in an environment's table (renamed under Defaults), with a
/// remove button (R-SRV-7). Returns whether remove was clicked.
/// The colours an environment's dot can have, the default first (R-SRV-8).
fn color_choice(ui: &mut Ui, color: &mut Option<EnvironmentColor>) {
    let choices = std::iter::once(None).chain(EnvironmentColor::ALL.into_iter().map(Some));
    for choice in choices {
        let label = choice.map_or("Default", EnvironmentColor::label);
        let button = egui::Button::selectable(
            *color == choice,
            theme::environment_dot(ui, choice).size(14.0),
        );
        let response = ui.add(button).on_hover_text(label);
        // Named for screen readers (and tests): every swatch shows the same "⏺".
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::Role::RadioButton, true, *color == choice, label)
        });
        if response.clicked() {
            *color = choice;
        }
    }
}

fn name_label(ui: &mut Ui, width: f32, name: &str, secret: bool, remove_help: &str) -> bool {
    ui.horizontal(|ui| {
        ui.set_width(width);
        let text = if secret {
            format!("{name} 🔒")
        } else {
            name.to_owned()
        };
        egui::Sides::new()
            .shrink_left()
            .show(
                ui,
                |ui| {
                    ui.add(
                        egui::Label::new(egui::RichText::new(text).monospace())
                            .truncate()
                            .selectable(false),
                    )
                    .on_hover_text(name);
                },
                |ui| {
                    ui.add(theme::icon_button(ui, "🗙"))
                        .on_hover_text(remove_help)
                        .clicked()
                },
            )
            .1
    })
    .inner
}

fn add_server_row(ui: &mut Ui, d: &mut Draft, width: f32, new_server: &mut String) {
    if let Some(name) = new_row_input(
        ui,
        "new-server",
        width,
        new_server,
        "Add server (press Enter)…",
    ) {
        d.add_server(name);
    }
    ui.end_row();
}

fn add_variable_row(ui: &mut Ui, d: &mut Draft, width: f32, new_variable: &mut String) {
    if let Some(name) = new_row_input(
        ui,
        "new-variable",
        width,
        new_variable,
        "Add variable (press Enter)…",
    ) {
        d.add_variable(name);
    }
    ui.end_row();
}

const SECRET_HELP: SecretHelp = SecretHelp {
    secret: "Secret: values stay on this machine. Click to share them through the project files.",
    shared: "Shared through the project files. Click to make it secret (values stay on this machine).",
};

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
        let SavedConfig {
            manifest,
            environments: envs,
            ..
        } = draft().to_config(&Manifest::new("C"), &[]).expect("valid");
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
        let SavedConfig {
            manifest,
            environments: envs,
            ..
        } = draft().to_config(&Manifest::new("C"), &[]).expect("valid");
        let col = Collection {
            root: PathBuf::new(),
            manifest,
            read_only: false,
            items: vec![],
            environments: envs,
            broken_environments: vec![],
            workflows: vec![],
            broken_workflows: vec![],
        };
        let mut d = Draft::from_collection(&col, &SecretValues::default());
        d.servers[0].name = "People API".into();
        d.environments[0].name = "Local".into();
        let SavedConfig {
            manifest,
            environments: envs,
            ..
        } = d
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
    fn secret_values_go_to_the_local_store_not_the_files() {
        let mut d = draft();
        d.variables[0].secret = true;
        let saved = d.to_config(&Manifest::new("C"), &[]).expect("valid");
        assert_eq!(saved.manifest.secrets, ["token"]);
        assert!(saved.manifest.variables.is_empty(), "default stays local");
        assert!(saved.environments.iter().all(|e| e.variables.is_empty()));
        assert_eq!(saved.secrets.get(None, "token"), Some("none"));
        assert_eq!(
            saved.secrets.get(Some("development"), "token"),
            Some("dev-token")
        );

        // Reading back shows the local values in the same cells.
        let col = Collection {
            root: PathBuf::new(),
            manifest: saved.manifest,
            read_only: false,
            items: vec![],
            environments: saved.environments,
            broken_environments: vec![],
            workflows: vec![],
            broken_workflows: vec![],
        };
        let back = Draft::from_collection(&col, &saved.secrets);
        assert!(back.variables[0].secret);
        assert_eq!(back.variables[0].values, ["dev-token", ""]);
    }

    #[test]
    fn removing_an_environment_removes_its_column() {
        let mut d = draft();
        d.remove_environment(0);
        assert_eq!(d.servers[0].urls, [""]);
        assert_eq!(d.variables[0].values, [""]);
    }

    /// R-SRV-6: moving an environment moves its URLs and values; the order is saved.
    #[test]
    fn environments_and_servers_can_be_reordered() {
        let mut d = draft();
        d.servers[0].urls = vec!["http://dev".into(), "http://prod".into()];
        d.add_server("Auth".into());
        d.move_environment(1, 0);
        assert_eq!(d.servers[0].urls, ["http://prod", "http://dev"]);
        d.move_server(1, 0);
        assert_eq!(d.servers[0].name, "Auth");
        assert_eq!(d.variables[0].values, ["", "dev-token"], "values move too");
        let saved = d.to_config(&Manifest::new("C"), &[]).expect("valid");
        let names: Vec<&str> = saved.environments.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["Staging", "Development"]);
        assert_eq!(saved.environments[0].seq, Some(1));
        assert_eq!(saved.manifest.servers[0].name, "Auth");
        d.move_environment(0, 9);
        d.move_server(5, 0);
    }

    #[test]
    fn names_get_a_share_of_the_width_within_bounds() {
        // BUG-1
        assert!((name_width(600.0) - 240.0).abs() < f32::EPSILON);
        assert!((name_width(100.0) - 160.0).abs() < f32::EPSILON);
        assert!((name_width(5000.0) - 320.0).abs() < f32::EPSILON);
    }

    /// R-SRV-7: the shown environment stays the same one when others move or go.
    #[test]
    fn the_shown_environment_follows_moves_and_removes() {
        use Shown::{Defaults, Environment};
        assert_eq!(Environment(2).after_remove(0, 2), Environment(1));
        assert_eq!(Environment(0).after_remove(2, 2), Environment(0));
        assert_eq!(
            Environment(1).after_remove(1, 1),
            Environment(0),
            "neighbour"
        );
        assert_eq!(Environment(0).after_remove(0, 0), Defaults, "none left");
        assert_eq!(Defaults.after_remove(0, 0), Defaults);
        assert_eq!(Environment(0).after_move(0, 2), Environment(2));
        assert_eq!(Environment(2).after_move(0, 2), Environment(1));
        assert_eq!(Environment(0).after_move(2, 0), Environment(1));
        assert_eq!(Environment(3).after_move(0, 2), Environment(3));
        assert_eq!(Defaults.after_move(0, 1), Defaults);
    }

    /// A project on disk, open in an `AppState`, for showing the dialog.
    fn open_project(name: &str) -> (PathBuf, AppState) {
        use tortuga_core::storage::FsStorage;
        let root = std::env::temp_dir().join(format!("tortuga-env-{name}-{}", std::process::id()));
        // Leftovers of an earlier run may be missing; either way the directory starts empty.
        let _ = std::fs::remove_dir_all(&root);
        Collection::create(&FsStorage, &root, "C").expect("create");
        let state = AppState::new(Box::new(crate::test_host::TestHost {
            open: vec![root.clone()],
            ..crate::test_host::TestHost::default()
        }));
        (root, state)
    }

    fn harness<'a>(
        dialog: &'a mut EnvironmentsDialog,
        state: &'a AppState,
    ) -> egui_kittest::Harness<'a> {
        let mut installed = false;
        // The URL and value cells have no accessible name yet; these tests are about layout.
        egui_kittest::Harness::builder()
            .with_size(egui::vec2(1200.0, 800.0))
            .with_accessibility_check(false)
            .build_ui(move |ui| {
                if !installed {
                    theme::install(ui.ctx());
                    installed = true;
                }
                dialog.show(ui.ctx(), state);
            })
    }

    fn on_screen(h: &egui_kittest::Harness<'_>, value: &str) -> bool {
        use egui_kittest::kittest::Queryable;
        h.query_all_by_value(value).next().is_some()
    }

    /// R-SRV-7: only the chosen environment's URLs and values are on screen.
    #[test]
    fn one_environment_is_shown_at_a_time() {
        use egui_kittest::kittest::Queryable;
        let (root, state) = open_project("one-at-a-time");
        let mut dialog = EnvironmentsDialog::new(&state, root.clone());
        dialog.draft = draft();
        dialog.draft.servers[0].urls = vec!["http://dev".into(), "http://staging".into()];
        dialog.shown = Shown::Environment(0);
        let mut h = harness(&mut dialog, &state);
        h.run();
        assert!(on_screen(&h, "http://dev"));
        assert!(!on_screen(&h, "http://staging"));
        assert!(on_screen(&h, "dev-token"));

        h.get_by_label("Staging").click();
        h.run();
        assert!(!on_screen(&h, "http://dev"));
        assert!(on_screen(&h, "http://staging"));
        assert!(!on_screen(&h, "dev-token"));

        h.get_by_label("Defaults").click();
        h.run();
        assert!(!on_screen(&h, "http://staging"));
        assert!(on_screen(&h, "none"), "the default value");
        drop(h);
        std::fs::remove_dir_all(&root).expect("clean up");
    }

    /// R-SRV-7: servers and variables can be removed while editing an environment.
    #[test]
    fn servers_and_variables_are_removed_from_an_environment_view() {
        use egui_kittest::kittest::Queryable;
        let (root, state) = open_project("remove");
        let mut dialog = EnvironmentsDialog::new(&state, root.clone());
        dialog.draft = draft();
        dialog.draft.add_variable("ü😀".into());
        dialog.shown = Shown::Environment(1);
        let mut h = harness(&mut dialog, &state);
        h.run();
        // Rows in order: the server "Users API", then the variables "token" and "ü😀".
        h.query_all_by_label("🗙")
            .nth(1)
            .expect("token's remove")
            .click();
        h.run();
        h.query_all_by_label("🗙")
            .next()
            .expect("the server's remove")
            .click();
        h.run();
        drop(h);
        assert_eq!(dialog.draft.servers, []);
        let names: Vec<&str> = dialog
            .draft
            .variables
            .iter()
            .map(|v| v.name.as_str())
            .collect();
        assert_eq!(names, ["ü😀"]);
        assert_eq!(dialog.draft.environments.len(), 2, "environments stay");
        assert_eq!(dialog.shown, Shown::Environment(1), "still on the same one");
        std::fs::remove_dir_all(&root).expect("clean up");
    }

    /// R-SRV-7: the chosen environment's view keeps some room after the list's line.
    #[test]
    fn the_view_is_padded_from_the_list() {
        use egui_kittest::kittest::Queryable;
        let (root, state) = open_project("padding");
        let mut dialog = EnvironmentsDialog::new(&state, root.clone());
        dialog.draft = draft();
        dialog.shown = Shown::Environment(0);
        let mut h = harness(&mut dialog, &state);
        h.run();
        let list_right = h.get_by_label("Defaults").rect().right();
        let name_left = h.get_by_label("Name").rect().left();
        // The list's own 12 px margin up to its line, then the padding after it.
        let gap = name_left - list_right;
        assert!(
            gap >= 12.0 + 16.0,
            "{gap} px from the list to the Name label"
        );
        drop(h);
        std::fs::remove_dir_all(&root).expect("clean up");
    }

    /// R-SRV-8: a colour picked for an environment is saved in its file, read back, and
    /// moves with it; "Default" takes it away again.
    #[test]
    fn an_environment_gets_a_colour() {
        use egui_kittest::kittest::Queryable;
        let (root, state) = open_project("colour");
        let mut dialog = EnvironmentsDialog::new(&state, root.clone());
        dialog.draft = draft();
        dialog.shown = Shown::Environment(1);
        {
            let mut h = harness(&mut dialog, &state);
            h.run();
            h.get_by_label("Red").click();
            h.run();
        }
        assert_eq!(
            dialog.draft.environments[1].color,
            Some(EnvironmentColor::Red)
        );
        assert_eq!(
            dialog.draft.environments[0].color, None,
            "only the shown one"
        );

        dialog.draft.move_environment(1, 0);
        let config = dialog
            .draft
            .to_config(&Manifest::new("C"), &[])
            .expect("valid");
        assert_eq!(config.environments[0].name, "Staging");
        assert_eq!(config.environments[0].color, Some(EnvironmentColor::Red));
        let text = tortuga_core::format::write_environment(&config.environments[0]);
        assert!(text.contains("color = \"red\""), "{text}");

        let mut c = Collection::open(&tortuga_core::storage::FsStorage, &root).expect("open");
        c.environments = config.environments;
        let mut reopened = Draft::from_collection(&c, &SecretValues::default());
        assert_eq!(reopened.environments[0].color, Some(EnvironmentColor::Red));

        dialog.draft = reopened.clone();
        dialog.shown = Shown::Environment(0);
        {
            let mut h = harness(&mut dialog, &state);
            h.run();
            h.get_by_label("Default").click();
            h.run();
        }
        reopened.environments[0].color = None;
        assert_eq!(dialog.draft, reopened);
        std::fs::remove_dir_all(&root).expect("clean up");
    }

    /// R-SRV-7: an environment is renamed in its Name field and keeps its id.
    #[test]
    fn an_environment_is_renamed_in_its_view() {
        use egui_kittest::kittest::Queryable;
        use tortuga_core::storage::FsStorage;
        let (root, mut state) = open_project("rename");
        let mut d = Draft::default();
        d.add_environment();
        d.environments[0].name = "Development".into();
        let config = d.to_config(&Manifest::new("C"), &[]).expect("valid");
        state
            .run(&root, |c, s| {
                c.save_config(s, &config.manifest, &config.environments)
            })
            .expect("saved");
        let mut dialog = EnvironmentsDialog::new(&state, root.clone());
        assert_eq!(
            dialog.shown,
            Shown::Environment(0),
            "opens on the active one"
        );
        let mut result = None;
        {
            let mut installed = false;
            let mut h = egui_kittest::Harness::builder()
                .with_size(egui::vec2(1200.0, 800.0))
                .with_accessibility_check(false)
                .build_ui(|ui| {
                    if !installed {
                        theme::install(ui.ctx());
                        installed = true;
                    }
                    if let DialogResult::Done(r) = dialog.show(ui.ctx(), &state) {
                        result = Some(r);
                    }
                });
            h.run();
            // The first text field is the environment's name.
            h.get_all_by_role(egui::accesskit::Role::TextInput)
                .next()
                .expect("name field")
                .click();
            h.run();
            h.get_all_by_role(egui::accesskit::Role::TextInput)
                .next()
                .expect("name field")
                .type_text(" ü😀");
            h.run();
            h.get_by_label("Save").click();
            h.run();
        }
        let saved = result.expect("saved");
        assert_eq!(saved.environments[0].id, "development");
        assert_eq!(saved.environments[0].name, "Development ü😀");
        assert!(Collection::open(&FsStorage, &root).is_ok());
        std::fs::remove_dir_all(&root).expect("clean up");
    }

    #[test]
    fn names_are_not_truncated() {
        // BUG-1
        use egui_kittest::kittest::Queryable;
        let (root, state) = open_project("width");
        let mut dialog = EnvironmentsDialog::new(&state, root.clone());
        dialog.draft = draft();
        dialog.shown = Shown::Defaults;
        let mut h = harness(&mut dialog, &state);
        h.run();
        let widest = |value: &str| {
            h.query_all_by_value(value)
                .map(|n| n.rect().width())
                .fold(0.0, f32::max)
        };
        let server = widest("Users API");
        assert!(server > 150.0, "server name field is only {server} wide");
        let variable = widest("token");
        assert!(
            variable > 150.0,
            "variable name field is only {variable} wide"
        );
        drop(h);
        std::fs::remove_dir_all(&root).expect("clean up");
    }

    /// R-SRV-7: dragging the bottom-right corner makes the dialog bigger, on both sides.
    #[test]
    fn the_dialog_resizes_from_its_corner() {
        use egui_kittest::kittest::Queryable;
        let (root, state) = open_project("resize");
        let mut dialog = EnvironmentsDialog::new(&state, root.clone());
        let mut h = harness(&mut dialog, &state);
        h.run();
        let heading = h.get_by_label("Environments — C").rect();
        let left = heading.left();
        // The grip's centre: just outside the 820 × 520 body, in the frame's margin.
        let grip = egui::pos2(left + 820.0 + 11.0, heading.top() + 520.0 + 11.0);
        let pointer = |h: &mut egui_kittest::Harness<'_>, pos, pressed: Option<bool>| {
            h.input_mut().events.push(egui::Event::PointerMoved(pos));
            if let Some(pressed) = pressed {
                h.input_mut().events.push(egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::default(),
                });
            }
            h.run();
        };
        pointer(&mut h, grip, None);
        pointer(&mut h, grip, Some(true));
        for step in 1..=4u8 {
            pointer(&mut h, grip + egui::vec2(10.0, 5.0) * f32::from(step), None);
        }
        pointer(&mut h, grip + egui::vec2(40.0, 20.0), Some(false));
        let moved = left - h.get_by_label("Environments — C").rect().left();
        assert!(
            (moved - 40.0).abs() < 1.0,
            "the left edge moved {moved} instead of 40"
        );
        drop(h);
        std::fs::remove_dir_all(&root).expect("clean up");
    }

    /// R-SRV-9: "Protected" is ticked per environment and saved in its file.
    #[test]
    fn an_environment_is_protected() {
        use egui_kittest::kittest::Queryable;
        let (root, state) = open_project("protected");
        let mut dialog = EnvironmentsDialog::new(&state, root.clone());
        dialog.draft = draft();
        dialog.shown = Shown::Environment(1);
        {
            let mut h = harness(&mut dialog, &state);
            h.run();
            h.get_by_label("Protected").click();
            h.run();
        }
        assert!(dialog.draft.environments[1].protected);
        assert!(
            !dialog.draft.environments[0].protected,
            "only the shown one"
        );
        let config = dialog
            .draft
            .to_config(&Manifest::new("C"), &[])
            .expect("valid");
        let text = tortuga_core::format::write_environment(&config.environments[1]);
        assert!(text.contains("protected = true"), "{text}");
        std::fs::remove_dir_all(&root).expect("clean up");
    }
}
