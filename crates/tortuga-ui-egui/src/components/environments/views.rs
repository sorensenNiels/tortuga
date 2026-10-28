//! The tables of the environments dialog (R-SRV-7, R-WS-9): a project's servers and
//! variables, its values for one environment, and the workspace's shared values.

use std::collections::{BTreeMap, BTreeSet};

use egui::{Grid, TextEdit, Ui};
use tortuga_core::EnvironmentColor;
use tortuga_core::format::names_match;

use super::draft::{Draft, EnvDraft, ServerEdit, VarDraft};
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
    .on_hover_text(t!("drag-to-reorder"));
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

    theme::section_label(ui, &t!("servers"));
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
                    .on_hover_text(t!("env-remove-server-everywhere"))
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
            theme::section_label(ui, &t!("variables"));
            theme::section_label(ui, &t!("default"));
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
                        .hint_text(if v.secret {
                            t!("not-set")
                        } else {
                            String::new()
                        })
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
    /// Servers any of the workspace's environments gives a URL to (R-WS-12).
    workspace_servers: Vec<String>,
}

impl Inherited {
    /// What `env` gives projects; `workspace_servers` from [`super::draft::server_names`].
    pub(super) fn of(env: &EnvDraft, workspace_servers: Vec<String>) -> Self {
        let mut out = Self {
            workspace_servers,
            ..Self::default()
        };
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
            return t!("env-workspace-secret");
        }
        if let Some(value) = self.variables.get(&v.name) {
            return value.clone();
        }
        match (v.secret, v.default.is_empty()) {
            (true, true) => t!("not-set"),
            (true, false) => t!("env-default-hint"),
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
            theme::section_label(ui, &t!("servers"));
            theme::section_label(ui, &t!("base-url"));
            ui.end_row();
            for (row, s) in d.servers.iter_mut().enumerate() {
                if name_label(ui, names, &s.name, false, &t!("env-remove-server")) {
                    remove_server = Some(row);
                }
                let hint = inherited.server(&s.name).unwrap_or("https://…").to_owned();
                if let Some(url) = s.urls.get_mut(col) {
                    theme::sized_edit(
                        ui,
                        values,
                        TextEdit::singleline(url)
                            // By name: typing into a workspace's server keeps focus
                            // when it becomes the project's (R-WS-12).
                            .id_salt(("url", s.name.trim().to_lowercase(), col))
                            .font(egui::TextStyle::Monospace)
                            .hint_text(hint),
                    );
                }
                ui.end_row();
            }
            workspace_server_rows(ui, d, col, inherited, [names, values]);
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
            theme::section_label(ui, &t!("variables"));
            theme::section_label(ui, &t!("value"));
            ui.end_row();
            for (row, v) in d.variables.iter_mut().enumerate() {
                if name_label(ui, names, &v.name, v.secret, &t!("env-remove-variable")) {
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

/// What a colour is called (R-SRV-8).
fn color_name(color: EnvironmentColor) -> String {
    match color {
        EnvironmentColor::Green => t!("color-green"),
        EnvironmentColor::Teal => t!("color-teal"),
        EnvironmentColor::Blue => t!("color-blue"),
        EnvironmentColor::Purple => t!("color-purple"),
        EnvironmentColor::Pink => t!("color-pink"),
        EnvironmentColor::Red => t!("color-red"),
        EnvironmentColor::Orange => t!("color-orange"),
        EnvironmentColor::Yellow => t!("color-yellow"),
        EnvironmentColor::Grey => t!("color-grey"),
    }
}

/// The colours an environment's dot can have, the default first (R-SRV-8).
pub(super) fn color_choice(ui: &mut Ui, color: &mut Option<EnvironmentColor>) {
    let choices = std::iter::once(None).chain(EnvironmentColor::ALL.into_iter().map(Some));
    for choice in choices {
        let label = choice.map_or_else(|| t!("default"), color_name);
        let button = egui::Button::selectable(
            *color == choice,
            theme::environment_dot(ui, choice).size(14.0),
        );
        let response = ui.add(button).on_hover_text(&label);
        // Named for screen readers (and tests): every swatch shows the same "⏺".
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::Role::RadioButton, true, *color == choice, &label)
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

/// The workspace's servers the project lacks (R-WS-12), greyed with the URL it would use;
/// typing a URL adds the server to the project.
fn workspace_server_rows(
    ui: &mut Ui,
    d: &mut Draft,
    col: usize,
    inherited: &Inherited,
    [names, values]: [f32; 2],
) {
    let missing: Vec<&String> = inherited
        .workspace_servers
        .iter()
        .filter(|n| !d.servers.iter().any(|s| names_match(&s.name, n)))
        .collect();
    for name in missing {
        ui.horizontal(|ui| {
            ui.set_width(names);
            ui.add(
                egui::Label::new(egui::RichText::new(name).monospace().weak())
                    .truncate()
                    .selectable(false),
            )
            .on_hover_text(t!("env-workspace-server-hint"));
        });
        let mut url = String::new();
        theme::sized_edit(
            ui,
            values,
            TextEdit::singleline(&mut url)
                .id_salt(("url", name.trim().to_lowercase(), col))
                .font(egui::TextStyle::Monospace)
                .hint_text(inherited.server(name).unwrap_or("https://…")),
        );
        if !url.is_empty() {
            d.add_server(name.trim().to_owned());
            if let Some(slot) = d.servers.last_mut().and_then(|s| s.urls.get_mut(col)) {
                *slot = url;
            }
        }
        ui.end_row();
    }
}

fn add_server_row(ui: &mut Ui, d: &mut Draft, width: f32, new_server: &mut String) {
    if let Some(name) = new_row_input(ui, "new-server", width, new_server, &t!("add-server")) {
        d.add_server(name);
    }
    ui.end_row();
}

fn add_variable_row(ui: &mut Ui, d: &mut Draft, width: f32, new_variable: &mut String) {
    if let Some(name) = new_row_input(ui, "new-variable", width, new_variable, &t!("add-variable"))
    {
        d.add_variable(name);
    }
    ui.end_row();
}

const SECRET_HELP: SecretHelp = SecretHelp {
    secret: || t!("secret-help-project-secret"),
    shared: || t!("secret-help-project-shared"),
};

const SHARED_SECRET_HELP: SecretHelp = SecretHelp {
    secret: || t!("secret-help-workspace-secret"),
    shared: || t!("secret-help-workspace-shared"),
};

/// The workspace's servers, the same in every environment, with this environment's base
/// URLs (R-WS-12). Returns a change to the names, for every environment.
pub(super) fn shared_servers(
    ui: &mut Ui,
    d: &mut EnvDraft,
    known: &[String],
    new_server: &mut String,
) -> Option<ServerEdit> {
    let mut edit = None;
    Grid::new("ws-env-servers").num_columns(2).show(ui, |ui| {
        theme::section_label(ui, &t!("servers"));
        theme::section_label(ui, &t!("base-url"));
        ui.end_row();
        for (row, (name, url)) in d.servers.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                let renamed = theme::sized_edit(
                    ui,
                    NAME_WIDTH - 30.0,
                    TextEdit::singleline(name)
                        .id_salt(("ws-server", row))
                        .hint_text(t!("env-server-name-hint")),
                )
                .changed();
                if renamed {
                    edit = Some(ServerEdit::Rename(row, name.clone()));
                }
                if ui
                    .add(theme::icon_button(ui, "🗙"))
                    .on_hover_text(t!("env-remove-server-everywhere"))
                    .clicked()
                {
                    edit = Some(ServerEdit::Remove(row));
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
            &t!("add-server"),
        ) {
            edit = Some(ServerEdit::Add(name));
        }
        ui.end_row();
    });
    let missing: Vec<&str> = known
        .iter()
        .filter(|k| !d.servers.iter().any(|(n, _)| names_match(n, k)))
        .map(String::as_str)
        .collect();
    let hint = if missing.is_empty() {
        t!("env-shared-servers-hint")
    } else {
        t!(
            "env-shared-servers-hint-missing",
            servers = missing.join(", ")
        )
    };
    theme::hint(ui, hint);
    edit
}

/// The workspace's variables for one environment (🔒 values stay on this machine).
pub(super) fn shared_variables(ui: &mut Ui, d: &mut EnvDraft, new_variable: &mut String) {
    let mut remove = None;
    Grid::new("ws-env-variables").num_columns(2).show(ui, |ui| {
        theme::section_label(ui, &t!("variables"));
        theme::section_label(ui, &t!("value"));
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
                    .hint_text(if v.secret {
                        t!("not-set")
                    } else {
                        String::new()
                    })
                    .font(egui::TextStyle::Monospace),
            );
            ui.end_row();
        }
        if let Some(name) = new_row_input(
            ui,
            "ws-env-new-variable",
            NAME_WIDTH,
            new_variable,
            &t!("add-variable"),
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
