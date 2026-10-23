//! The environments dialog (R-WS-9, D62; R-SRV-7): the workspace's one list of
//! environments on the left. For the chosen one: its name, colour and protection, the
//! values every project shares, and each project's own values, which win. Opened from a
//! project, it shows that project's values only. With several projects, each project's
//! values start folded under its name (R-WS-10). A project's servers and variables
//! themselves (names, secret flags, defaults) are edited under its name in the list.

mod draft;
mod views;

use std::path::PathBuf;

use egui::{CentralPanel, CollapsingHeader, Context, Frame, Panel, ScrollArea, TextEdit, Ui};
use tortuga_core::workspace::WorkspaceEnvironment;

pub use draft::{Draft, EnvDraft, SavedConfig};
use draft::{move_item, to_environments};
use views::{EnvDrag, Inherited};

use super::{DialogResult, resizable_dialog};
use crate::environments::SecretsByEnvironment;
use crate::state::AppState;
use crate::theme;

/// What saving the dialog writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedEnvironments {
    /// The workspace's list, with its shared values.
    pub list: Vec<WorkspaceEnvironment>,
    /// This machine's secret values of the shared values.
    pub secrets: SecretsByEnvironment,
    /// Each project's files and secret values.
    pub projects: Vec<(PathBuf, SavedConfig)>,
}

/// One project's servers, variables and values, a column per environment in the list.
#[derive(Debug, Clone)]
struct ProjectDraft {
    root: PathBuf,
    name: String,
    read_only: bool,
    draft: Draft,
    /// What its "Add server" and "Add variable" inputs hold.
    new_server: String,
    new_variable: String,
    /// Its values are unfolded in an environment's view (R-WS-10).
    expanded: bool,
}

/// What the right side shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shown {
    /// The environment at this index of the list.
    Environment(usize),
    /// The servers and variables of the project at this index.
    Project(usize),
}

#[derive(Debug, Clone)]
pub struct EnvironmentsDialog {
    list: Vec<EnvDraft>,
    projects: Vec<ProjectDraft>,
    shown: Shown,
    /// Only this project's values are shown (opened from its menu).
    focus: Option<PathBuf>,
    new_server: String,
    new_variable: String,
    /// Shown when the environments cannot be saved.
    pub error: Option<String>,
}

/// Width of the list on the left.
const LIST_WIDTH: f32 = 210.0;

impl EnvironmentsDialog {
    /// Opened from the project at `root`: its values only.
    pub fn new(state: &AppState, root: PathBuf) -> Self {
        let mut dialog = Self::for_workspace(state);
        for p in &mut dialog.projects {
            p.expanded = p.root == root;
        }
        Self {
            focus: Some(root),
            ..dialog
        }
    }

    /// Every project's values.
    pub fn for_workspace(state: &AppState) -> Self {
        let ws = state.workspace();
        let list: Vec<EnvDraft> = ws
            .environments
            .iter()
            .map(|e| EnvDraft::new(e, state.workspace_env_secrets.get(&e.id)))
            .collect();
        let names: Vec<String> = list.iter().map(|e| e.name.clone()).collect();
        // Folded when there are several, so each is one line (R-WS-10).
        let expanded = state.collections.len() == 1;
        let projects = state
            .collections
            .iter()
            .map(|c| {
                let secrets = state.secrets.get(&c.root).cloned().unwrap_or_default();
                ProjectDraft {
                    root: c.root.clone(),
                    name: c.name().to_owned(),
                    read_only: c.read_only,
                    draft: Draft::from_collection(c, &secrets).aligned(&names),
                    new_server: String::new(),
                    new_variable: String::new(),
                    expanded,
                }
            })
            .collect();
        // Opens on the environment requests are sent with, where edits matter most.
        let shown = state
            .chosen_environment()
            .and_then(|chosen| list.iter().position(|e| e.id.as_ref() == Some(&chosen.id)))
            .unwrap_or(0);
        Self {
            list,
            projects,
            shown: Shown::Environment(shown),
            focus: None,
            new_server: String::new(),
            new_variable: String::new(),
            error: None,
        }
    }

    /// Opens on the environment called `name`, if the list has it.
    #[must_use]
    pub fn on_environment(mut self, name: &str) -> Self {
        let found = self
            .list
            .iter()
            .position(|e| tortuga_core::format::names_match(&e.name, name));
        if let Some(i) = found {
            self.shown = Shown::Environment(i);
        }
        self
    }

    pub fn show(&mut self, ctx: &Context, state: &AppState) -> DialogResult<SavedEnvironments> {
        let focused = self
            .focus
            .as_deref()
            .and_then(|root| state.collection(root));
        let title = t!(
            "env-title",
            name = focused.map_or(state.workspace().name.as_str(), |c| c.name())
        );
        let servers = server_names(state);
        resizable_dialog(
            ctx,
            "environments",
            &title,
            egui::vec2(860.0, 560.0),
            egui::vec2(580.0, 380.0),
            |ui| {
                let (cancel, save) = Panel::bottom("env-footer")
                    .frame(Frame::NONE.inner_margin(egui::Margin {
                        top: 10,
                        ..egui::Margin::ZERO
                    }))
                    .show_separator_line(false)
                    .show(ui, |ui| footer(ui, state, self.error.as_deref()))
                    .inner;
                Panel::left("env-list")
                    .frame(Frame::NONE.inner_margin(egui::Margin {
                        right: 12,
                        ..egui::Margin::ZERO
                    }))
                    .resizable(false)
                    .exact_size(LIST_WIDTH)
                    .show(ui, |ui| {
                        ScrollArea::vertical()
                            .id_salt("env-list")
                            .show(ui, |ui| self.list_panel(ui));
                    });
                CentralPanel::no_frame().show(ui, |ui| {
                    let padding = egui::Margin {
                        left: 16,
                        ..egui::Margin::ZERO
                    };
                    Frame::NONE.inner_margin(padding).show(ui, |ui| {
                        ScrollArea::vertical()
                            .id_salt("env-detail")
                            .auto_shrink([false, false])
                            .show(ui, |ui| match self.shown {
                                Shown::Environment(i) => self.environment_view(ui, i, &servers),
                                Shown::Project(p) => self.project_view(ui, p),
                            });
                    });
                });
                if save {
                    match self.to_saved(state) {
                        Ok(saved) => return DialogResult::Done(saved),
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

    /// The projects shown: the one it was opened from, or all.
    fn shown_projects(&self) -> Vec<usize> {
        (0..self.projects.len())
            .filter(|&p| {
                self.focus
                    .as_ref()
                    .is_none_or(|root| self.projects[p].root == *root)
            })
            .collect()
    }

    /// The workspace's environments (drag ☰ to reorder, R-SRV-6), then each project's
    /// servers and variables.
    fn list_panel(&mut self, ui: &mut Ui) {
        theme::section_label(ui, &t!("import-row-environments"));
        let mut move_env = None;
        for i in 0..self.list.len() {
            let e = &self.list[i];
            let row = ui.horizontal(|ui| {
                views::drag_handle(ui, ("env-drag", i), EnvDrag(i));
                let name = if e.name.trim().is_empty() {
                    t!("example-no-name")
                } else {
                    e.name.clone()
                };
                let selected = self.shown == Shown::Environment(i);
                let width = ui.available_width();
                if theme::environment_button(ui, selected, e.color, &name, width).clicked() {
                    self.shown = Shown::Environment(i);
                }
            });
            if let Some(from) = views::drop_index(ui, &row.response, i, |p: &EnvDrag| p.0) {
                move_env = Some((from, i));
            }
        }
        if self.list.is_empty() {
            theme::hint(ui, t!("env-none-yet"));
        }
        ui.add_space(6.0);
        if ui.add(theme::ghost_button(t!("env-add"))).clicked() {
            self.add_environment();
        }
        if let Some((from, to)) = move_env {
            self.move_environment(from, to);
        }
        ui.add_space(14.0);
        theme::section_label(ui, &t!("env-servers-and-variables"));
        let width = ui.available_width();
        for p in self.shown_projects() {
            let selected = self.shown == Shown::Project(p);
            let button = egui::Button::selectable(selected, self.projects[p].name.as_str())
                .min_size(egui::vec2(width, 0.0));
            if ui
                .add(button)
                .on_hover_text(t!("env-project-hint"))
                .clicked()
            {
                self.shown = Shown::Project(p);
            }
        }
    }

    fn add_environment(&mut self) {
        let name = t!("env-default-name", number = self.list.len() + 1);
        for p in &mut self.projects {
            p.draft.add_environment();
            if let Some(col) = p.draft.environments.last_mut() {
                col.name.clone_from(&name);
            }
        }
        self.list.push(EnvDraft {
            name,
            ..EnvDraft::default()
        });
        self.shown = Shown::Environment(self.list.len() - 1);
    }

    fn remove_environment(&mut self, i: usize) {
        self.list.remove(i);
        for p in &mut self.projects {
            p.draft.remove_environment(i);
        }
        self.shown = Shown::Environment(i.min(self.list.len().saturating_sub(1)));
    }

    fn move_environment(&mut self, from: usize, to: usize) {
        if from == to || from >= self.list.len() || to >= self.list.len() {
            return;
        }
        move_item(&mut self.list, from, to);
        for p in &mut self.projects {
            p.draft.move_environment(from, to);
        }
        if let Shown::Environment(i) = self.shown {
            let i = if i == from {
                to
            } else if from < i && i <= to {
                i - 1
            } else if to <= i && i < from {
                i + 1
            } else {
                i
            };
            self.shown = Shown::Environment(i);
        }
    }

    /// Environment `i`: its name, colour and protection, what all projects share, and
    /// each shown project's own values.
    fn environment_view(&mut self, ui: &mut Ui, i: usize, servers: &[String]) {
        let Some(env) = self.list.get_mut(i) else {
            theme::hint(
                ui,
                "Environments belong to the workspace: every project has them. Each project \
                 fills in its own server URLs and values.",
            );
            return;
        };
        let remove = ui
            .horizontal(|ui| {
                ui.label(t!("name"));
                theme::sized_edit(
                    ui,
                    220.0,
                    TextEdit::singleline(&mut env.name).id_salt(("env-name", i)),
                );
                ui.add(theme::ghost_button(t!("env-remove")))
                    .on_hover_text(t!("env-remove-hint"))
                    .clicked()
            })
            .inner;
        if remove {
            self.remove_environment(i);
            return;
        }
        ui.horizontal(|ui| {
            ui.label(t!("env-colour"));
            views::color_choice(ui, &mut env.color);
        });
        ui.checkbox(&mut env.protected, t!("env-protected"))
            .on_hover_text(t!("env-protected-hint"));
        let inherited = Inherited::of(env);
        ui.add_space(12.0);
        if self.focus.is_none() {
            ui.label(theme::semibold(t!("env-all-projects")));
            theme::hint(ui, t!("env-all-projects-hint"));
            ui.add_space(4.0);
            views::shared_servers(ui, env, servers, &mut self.new_server);
            ui.add_space(10.0);
            views::shared_variables(ui, env, &mut self.new_variable);
        }
        for p in self.shown_projects() {
            ui.add_space(16.0);
            ui.separator();
            ui.push_id(("project", p), |ui| {
                project_section(ui, &mut self.projects[p], i, &inherited);
            });
        }
        if self.focus.is_some() {
            ui.add_space(12.0);
            if ui.add(theme::ghost_button(t!("env-show-all"))).clicked() {
                self.focus = None;
            }
        }
    }

    /// Project `p`'s servers and variables, secret flags and defaults (R-VAR-7).
    fn project_view(&mut self, ui: &mut Ui, p: usize) {
        let Some(project) = self.projects.get_mut(p) else {
            return;
        };
        ui.label(theme::semibold(t!(
            "env-project-servers",
            name = &project.name
        )));
        ui.add_space(4.0);
        ui.push_id(("defaults", p), |ui| {
            ui.add_enabled_ui(!project.read_only, |ui| {
                views::defaults_view(
                    ui,
                    &mut project.draft,
                    &mut project.new_server,
                    &mut project.new_variable,
                );
            });
        });
    }

    /// What Save writes, or why it can't.
    fn to_saved(&self, state: &AppState) -> Result<SavedEnvironments, String> {
        let (list, secrets) = to_environments(&self.list)?;
        let mut projects = Vec::new();
        for p in self.projects.iter().filter(|p| !p.read_only) {
            let Some(c) = state.collection(&p.root) else {
                continue;
            };
            let mut draft = p.draft.clone();
            // Each project's file says what the workspace says about it, so a project
            // run on its own (tortuga-cli) knows its name and protection too.
            for (col, env) in draft.environments.iter_mut().zip(&list) {
                col.name.clone_from(&env.name);
                col.color = env.color;
                col.protected = env.protected;
            }
            let saved = draft
                .to_project_config(&c.manifest, &c.environments)
                .map_err(|e| format!("{}: {e}", p.name))?;
            projects.push((p.root.clone(), saved));
        }
        Ok(SavedEnvironments {
            list,
            secrets,
            projects,
        })
    }
}

/// A project's values for environment `col`, folded under its name (R-WS-10).
fn project_section(ui: &mut Ui, project: &mut ProjectDraft, col: usize, inherited: &Inherited) {
    let title = if project.read_only {
        t!("env-read-only", name = &project.name)
    } else {
        project.name.clone()
    };
    let header = CollapsingHeader::new(theme::semibold(title))
        .id_salt("values")
        .open(Some(project.expanded))
        .show(ui, |ui| {
            theme::hint(ui, t!("env-project-values-hint"));
            ui.add_enabled_ui(!project.read_only, |ui| {
                views::project_values(
                    ui,
                    &mut project.draft,
                    col,
                    inherited,
                    &mut project.new_server,
                    &mut project.new_variable,
                );
            });
        });
    if header.header_response.clicked() {
        project.expanded = !project.expanded;
    }
}

/// The servers of the open projects, by name, each once whatever its case.
fn server_names(state: &AppState) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for s in state.collections.iter().flat_map(|c| &c.manifest.servers) {
        if !names.iter().any(|n| s.is_named(n)) {
            names.push(s.name.clone());
        }
    }
    names
}

/// Where everything is kept, any error, and Cancel/Save. Returns (cancel, save).
fn footer(ui: &mut Ui, state: &AppState, error: Option<&str>) -> (bool, bool) {
    let kept = match &state.workspace().file {
        Some(file) => t!("env-kept-in-file", file = file.display().to_string()),
        None => t!("env-kept-on-machine"),
    };
    theme::callout(ui, t!("env-kept", kept = kept));
    if let Some(warning) = state.host.secrets_warning() {
        ui.add_space(4.0);
        theme::callout(ui, warning.text());
    }
    if let Some(err) = error {
        theme::error(ui, err);
    }
    ui.add_space(8.0);
    theme::button_row(ui, |ui| {
        let save = ui.add(theme::primary_button(ui, t!("save"))).clicked();
        (ui.button(t!("cancel")).clicked(), save)
    })
}

#[cfg(test)]
mod tests;
