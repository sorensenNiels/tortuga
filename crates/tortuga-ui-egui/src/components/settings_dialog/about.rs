//! The About page: what this build is (R-SET-14), and looking for a new version
//! (R-SET-15, D63), on Linux with the `curl` command that downloads a package (R-SET-18).

use egui::{RichText, Ui};
use tortuga_core::settings::UpdateSettings;
use tortuga_core::update::Download;

use super::UpdateStatus;
use super::layout::{About, group};
use super::network::switch_row;
use crate::theme;

/// Where the source and the issues are.
const REPOSITORY: &str = "https://github.com/sorensenNiels/tortuga";

/// The app's version, for bug reports and to know what a teammate runs (R-SET-14).
pub(super) fn version_info() -> String {
    format!(
        "Tortuga {} ({} {}, project format {}, workspace format {})",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        tortuga_core::format::FORMAT_VERSION,
        tortuga_core::workspace::WORKSPACE_FORMAT,
    )
}

/// What the user asked for on the About page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum AboutAction {
    /// "Check now" (R-SET-15).
    CheckNow,
    /// A package's download command was picked, to copy (R-SET-18).
    CopyCommand(String),
}

/// The build and the updates.
pub(super) fn about_page(
    ui: &mut Ui,
    settings: &mut UpdateSettings,
    status: &UpdateStatus,
) -> Option<AboutAction> {
    heading(ui);
    group(ui, &t!("settings-about"), None, |rows| {
        let mut fact = |name: &str, value: String| {
            rows.fact(name, |ui| {
                ui.label(value);
            });
        };
        fact(
            &t!("about-built-for"),
            format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
        );
        fact(
            &t!("about-project-files"),
            t!(
                "about-project-format",
                format = tortuga_core::format::FORMAT_VERSION
            ),
        );
        fact(
            &t!("about-workspace-files"),
            t!(
                "about-format",
                format = tortuga_core::workspace::WORKSPACE_FORMAT
            ),
        );
        fact(
            &t!("about-licence"),
            env!("CARGO_PKG_LICENSE").replace(" OR ", &t!("or-separator")),
        );
        rows.fact(&t!("about-source"), |ui| {
            ui.hyperlink(REPOSITORY);
        });
        rows.full_row(|ui| {
            if ui
                .button(t!("about-copy-version"))
                .on_hover_text(version_info())
                .clicked()
            {
                ui.ctx().copy_text(version_info());
            }
        });
    });
    updates(ui, settings, status)
}

/// The turtle, the name and the version.
fn heading(ui: &mut Ui) {
    let p = theme::palette(ui);
    ui.horizontal(|ui| {
        ui.label(theme::semibold("🐢").size(28.0).color(p.accent));
        ui.vertical(|ui| {
            ui.label(theme::semibold("Tortuga").size(18.0));
            ui.label(
                RichText::new(t!("about-version", version = env!("CARGO_PKG_VERSION")))
                    .color(p.muted),
            );
        });
    });
    ui.add_space(14.0);
}

/// Looking for a new version (R-SET-15, D63), and on Linux copying the command that
/// downloads it (R-SET-18).
fn updates(
    ui: &mut Ui,
    settings: &mut UpdateSettings,
    status: &UpdateStatus,
) -> Option<AboutAction> {
    if *status == UpdateStatus::Unsupported {
        theme::hint(ui, t!("updates-unsupported"));
        return None;
    }
    let note = (t!("updates-about"), t!("updates-hint"));
    group(ui, &t!("updates"), Some((&note.0, &note.1)), |rows| {
        switch_row(
            rows,
            &t!("updates-check-at-start"),
            About::default(),
            &mut settings.check_at_start,
        );
        rows.full_row(|ui| {
            let action = ui
                .horizontal(|ui| {
                    let mut action = ui
                        .add_enabled(
                            *status != UpdateStatus::Checking,
                            egui::Button::new(t!("updates-check-now")),
                        )
                        .clicked()
                        .then_some(AboutAction::CheckNow);
                    match status {
                        UpdateStatus::NotChecked
                        | UpdateStatus::Unsupported
                        | UpdateStatus::Failed(_) => {}
                        UpdateStatus::Checking => {
                            ui.spinner();
                            ui.label(t!("updates-checking"));
                        }
                        UpdateStatus::UpToDate => {
                            ui.label(t!("updates-up-to-date"));
                        }
                        UpdateStatus::Available(release) => {
                            ui.hyperlink_to(
                                t!("update-available", version = &release.version),
                                &release.page,
                            )
                            .on_hover_text(t!("update-available-hint"));
                            let packages = if cfg!(target_os = "linux") {
                                release.linux_packages()
                            } else {
                                Vec::new()
                            };
                            if let Some(command) = download_command_menu(ui, &packages) {
                                action = Some(AboutAction::CopyCommand(command));
                            }
                        }
                    }
                    action
                })
                .inner;
            if let UpdateStatus::Failed(e) = status {
                theme::error(ui, e);
            }
            action
        })
    })
}

/// "Copy download command", listing `packages` by file name; returns the command picked.
/// Nothing when there are none (R-SET-18).
fn download_command_menu(ui: &mut Ui, packages: &[&Download]) -> Option<String> {
    if packages.is_empty() {
        return None;
    }
    ui.menu_button(t!("update-copy-command"), |ui| {
        let mut picked = None;
        for package in packages {
            let command = package.curl_command();
            if ui
                .button(&package.name)
                .on_hover_text(command.as_str())
                .clicked()
            {
                picked = Some(command);
            }
        }
        picked
    })
    .inner
    .flatten()
}
