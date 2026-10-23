//! The tables of the environments dialog (R-SRV-7, R-WS-9): a project's servers and
//! variables, its values for one environment, and the workspace's shared values.

use std::collections::{BTreeMap, BTreeSet};

use egui::{Grid, TextEdit, Ui};
use tortuga_core::EnvironmentColor;
use tortuga_core::format::names_match;

use super::draft::{Draft, EnvDraft, VarDraft};
use crate::components::variable_row::{self, SecretHelp, new_row_input};
use crate::theme;

/// An environment or server row being dragged (R-SRV-6).
#[derive(Debug, Clone, Copy)]
pub(super) struct EnvDrag(pub usize);
#[derive(Debug, Clone, Copy)]
pub(super) struct ServerDrag(pub usize);

/// A handle to drag an environment or server by.
pub(super) fn drag_handle<P: Clone + Send + Sync + 'static>(
    ui: &mut Ui,
    id: (&str, usize),
    payload: P,
) {
    let id = ui.scope_id().with(id);
    ui.dnd_drag_source(id, payload, |ui| {
        ui.label(egui::RichText::new("☰").color(theme::palette(ui).muted))
    })
    .response
    .on_hover_text("Drag to change the order");
}

/// Makes the list row `zone` a drop target for `P`. Returns the dragged index if dropped
/// here, and marks the edge it will land on.
pub(super) fn drop_index<P: Copy + Send + Sync + 'static>(
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

/// Width of the name column in the server and variable tables: a share of the room, so long
/// names are not truncated (BUG-1), within sensible bounds.
pub(super) fn name_width(total: f32) -> f32 {
    (total * 0.4).clamp(160.0, 320.0).floor()
}

/// Server and variable names, secret flags and defaults (R-VAR-7).
pub(super) fn defaults_view(
    ui: &mut Ui,
    d: &mut Draft,
    new_server: &mut String,
    new_variable: &mut String,
) {
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

/// What a project's empty values use instead (R-WS-9): the workspace's values for that
/// environment.
#[derive(Debug, Clone, Default)]
pub(super) struct Inherited {
    variables: BTreeMap<String, String>,
    secrets: BTreeSet<String>,
    /// (server name, base URL).
    servers: Vec<(String, String)>,
}

impl Inherited {
    pub(super) fn of(env: &EnvDraft) -> Self {
        let mut out = Self::default();
        for v in env.variables.iter().filter(|v| !v.name.trim().is_empty()) {
            if v.secret {
                out.secrets.insert(v.name.trim().to_owned());
            } else if !v.value.is_empty() {
                out.variables
                    .insert(v.name.trim().to_owned(), v.value.clone());
            }
        }
        out.servers = env
            .servers
            .iter()
            .filter(|(_, url)| !url.trim().is_empty())
            .cloned()
            .collect();
        out
    }

    pub(super) fn server(&self, name: &str) -> Option<&str> {
        self.servers
            .iter()
            .find(|(n, _)| names_match(n, name))
            .map(|(_, url)| url.as_str())
    }

    /// The placeholder of a variable's empty value: what it inherits.
    pub(super) fn variable_hint(&self, v: &super::draft::VarRow) -> String {
        if self.secrets.contains(&v.name) {
            return "workspace secret".to_owned();
        }
        if let Some(value) = self.variables.get(&v.name) {
            return value.clone();
        }
        match (v.secret, v.default.is_empty()) {
            (true, true) => "not set".to_owned(),
            (true, false) => "default".to_owned(),
            (false, _) => v.default.clone(),
        }
    }
}

/// A project's values for environment `col`: each server's base URL and each variable's
/// value (R-SRV-7). Empty ones use what they inherit, shown greyed (R-WS-9).
pub(super) fn project_values(
    ui: &mut Ui,
    d: &mut Draft,
    col: usize,
    inherited: &Inherited,
    new_server: &mut String,
    new_variable: &mut String,
) {
    let total = ui.available_width() - ui.spacing().scroll.bar_width;
    let names = name_width(total);
    let values = (total - names - ui.spacing().item_spacing.x).max(120.0);
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
                let hint = inherited.server(&s.name).unwrap_or("https://…").to_owned();
                if let Some(url) = s.urls.get_mut(col) {
                    theme::sized_edit(
                        ui,
                        values,
                        TextEdit::singleline(url)
                            .id_salt(("url", row, col))
                            .font(egui::TextStyle::Monospace)
                            .hint_text(hint),
                    );
                }
                ui.end_row();
            }
            add_server_row(ui, d, names, new_server);
        });
    if let Some(row) = remove_server {
        d.servers.remove(row);
    }
    ui.add_space(10.0);
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
                // Secret values are masked; a placeholder never shows a secret value.
                let hint = inherited.variable_hint(v);
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
}

/// Width of the name column in the workspace's tables.
const NAME_WIDTH: f32 = 200.0;

/// Hover texts of the remove buttons in an environment's tables.
const REMOVE_SERVER: &str = "Remove this server from the project, in every environment";
const REMOVE_VARIABLE: &str = "Remove this variable from the project, in every environment";

/// The colours an environment's dot can have, the default first (R-SRV-8).
pub(super) fn color_choice(ui: &mut Ui, color: &mut Option<EnvironmentColor>) {
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

/// A server or variable name in an environment's table (renamed under the project's
/// servers and variables), with a remove button (R-SRV-7). Returns whether remove was clicked.
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

const SHARED_SECRET_HELP: SecretHelp = SecretHelp {
    secret: "Secret: the value stays on this machine. Click to share it.",
    shared: "Shared with the workspace. Click to make it secret (the value stays on this machine).",
};

/// The workspace's base URLs by server name: every project's server of that name uses it.
pub(super) fn shared_servers(
    ui: &mut Ui,
    d: &mut EnvDraft,
    known: &[String],
    new_server: &mut String,
) {
    let mut remove = None;
    Grid::new("ws-env-servers").num_columns(2).show(ui, |ui| {
        theme::section_label(ui, "Servers");
        theme::section_label(ui, "Base URL");
        ui.end_row();
        for (row, (name, url)) in d.servers.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                theme::sized_edit(
                    ui,
                    NAME_WIDTH - 30.0,
                    TextEdit::singleline(name)
                        .id_salt(("ws-server", row))
                        .hint_text("server name"),
                );
                if ui
                    .add(theme::icon_button(ui, "🗙"))
                    .on_hover_text("Remove this server's URL")
                    .clicked()
                {
                    remove = Some(row);
                }
            });
            theme::sized_edit(
                ui,
                320.0,
                TextEdit::singleline(url)
                    .id_salt(("ws-url", row))
                    .font(egui::TextStyle::Monospace)
                    .hint_text("https://…"),
            );
            ui.end_row();
        }
        if let Some(name) = new_row_input(
            ui,
            "ws-new-server",
            NAME_WIDTH,
            new_server,
            "Add server (press Enter)…",
        ) {
            d.servers.push((name, String::new()));
        }
        ui.end_row();
    });
    if let Some(row) = remove {
        d.servers.remove(row);
    }
    let missing: Vec<&str> = known
        .iter()
        .filter(|k| !d.servers.iter().any(|(n, _)| names_match(n, k)))
        .map(String::as_str)
        .collect();
    let hint = if missing.is_empty() {
        "Each URL goes to every project's server of that name.".to_owned()
    } else {
        format!(
            "Each URL goes to every project's server of that name. Servers in the projects: {}.",
            missing.join(", ")
        )
    };
    theme::hint(ui, hint);
}

/// The workspace's variables for one environment (🔒 values stay on this machine).
pub(super) fn shared_variables(ui: &mut Ui, d: &mut EnvDraft, new_variable: &mut String) {
    let mut remove = None;
    Grid::new("ws-env-variables").num_columns(2).show(ui, |ui| {
        theme::section_label(ui, "Variables");
        theme::section_label(ui, "Value");
        ui.end_row();
        for (row, v) in d.variables.iter_mut().enumerate() {
            if variable_row::name_cell(
                ui,
                ("ws-env-var", row),
                NAME_WIDTH,
                &mut v.name,
                &mut v.secret,
                SHARED_SECRET_HELP,
            ) {
                remove = Some(row);
            }
            theme::sized_edit(
                ui,
                320.0,
                TextEdit::singleline(&mut v.value)
                    .id_salt(("ws-env-value", row))
                    .password(v.secret)
                    .hint_text(if v.secret { "not set" } else { "" })
                    .font(egui::TextStyle::Monospace),
            );
            ui.end_row();
        }
        if let Some(name) = new_row_input(
            ui,
            "ws-env-new-variable",
            NAME_WIDTH,
            new_variable,
            "Add variable (press Enter)…",
        ) {
            d.variables.push(VarDraft {
                name,
                ..VarDraft::default()
            });
        }
        ui.end_row();
    });
    if let Some(row) = remove {
        d.variables.remove(row);
    }
}
