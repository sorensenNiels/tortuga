//! Rows for what can't be shown as it is: a project missing on this machine (R-WS-5)
//! and a file that can't be read (R-COL-7).

use egui::{Atom, RichText, Ui};
use tortuga_core::workspace::ProjectRef;

use super::menus::{MenuChoice, context_menu, menu_button};
use super::{SidebarAction, Tree, git_marker, tree_row};
use crate::state::ItemKey;
use crate::theme;

/// A project of the workspace that isn't on this machine (moved, or not cloned yet), R-WS-5.
pub(super) fn missing_row(ui: &mut Ui, project: &ProjectRef, reason: &str, tree: &mut Tree<'_>) {
    let p = theme::palette(ui);
    let name = project.path.file_name().map_or_else(
        || project.path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    let atoms = [
        Atom::from(RichText::new("⚠").color(p.muted)),
        Atom::from(RichText::new(name).color(p.muted)),
        Atom::from(RichText::new(t!("project-missing")).small().color(p.muted)),
        Atom::grow(),
    ];
    let path = project.path.clone();
    let remote = project.remote.clone();
    let menu = |ui: &mut Ui| {
        let mut choice = None;
        if ui
            .button(t!("project-locate"))
            .on_hover_text(t!("project-locate-hint"))
            .clicked()
        {
            choice = Some(MenuChoice::Action(SidebarAction::LocateMissing(
                path.clone(),
            )));
        }
        if let Some(remote) = &remote
            && ui
                .button(t!("project-copy-clone-url"))
                .on_hover_text(remote.as_str())
                .clicked()
        {
            ui.ctx().copy_text(remote.clone());
        }
        if ui.button(t!("project-remove-from-workspace")).clicked() {
            choice = Some(MenuChoice::Action(SidebarAction::RemoveMissing(
                path.clone(),
            )));
        }
        choice
    };
    let (row, from_button) = tree_row(ui, 0, false, atoms, |ui| menu_button(ui, menu));
    let mut hover = format!("{}\n{reason}", project.path.display());
    if let Some(remote) = &project.remote {
        hover = format!(
            "{hover}\n{}",
            t!("project-clone-hint", remote = remote.as_str())
        );
    }
    let row = row.on_hover_text(hover);
    let choice = from_button.or_else(|| context_menu(&row, menu));
    if let Some(MenuChoice::Action(action)) = choice {
        tree.actions.push(action);
    }
}

/// A request file that could not be read (R-GIT-8), e.g. with merge-conflict markers.
pub(super) fn broken_row(
    ui: &mut Ui,
    b: &tortuga_core::collection::Broken,
    key: ItemKey,
    depth: usize,
    read_only: bool,
    tree: &mut Tree<'_>,
) {
    let file = b.path.file_name().unwrap_or_default().to_string_lossy();
    let mut atoms = vec![Atom::from(format!("⚠ {file}")), Atom::grow()];
    if let Some(status) = tree.git.and_then(|g| g.file(&key.path)) {
        atoms.push(git_marker(ui, status));
    }
    let (row, delete) = tree_row(ui, depth, false, atoms, |ui| {
        !read_only
            && ui
                .add(theme::icon_button(ui, "🗙"))
                .on_hover_text(t!("broken-file-delete"))
                .clicked()
    });
    if row.on_hover_text(&b.error).clicked() {
        tree.actions.push(SidebarAction::ShowError(t!(
            "broken-file-error",
            path = b.path.display().to_string(),
            error = &b.error
        )));
    }
    if delete {
        tree.actions.push(SidebarAction::Delete(key));
    }
}
