//! The About page: what this build is (R-SET-14), and looking for a new version
//! (R-SET-15, D63).

use egui::{RichText, Ui};
use tortuga_core::settings::UpdateSettings;

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

/// The build and the updates. Returns whether "Check now" was clicked.
pub(super) fn about_page(
    ui: &mut Ui,
    settings: &mut UpdateSettings,
    status: &UpdateStatus,
) -> bool {
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

/// Looking for a new version (R-SET-15, D63). Returns whether "Check now" was clicked.
fn updates(ui: &mut Ui, settings: &mut UpdateSettings, status: &UpdateStatus) -> bool {
    if *status == UpdateStatus::Unsupported {
        theme::hint(ui, t!("updates-unsupported"));
        return false;
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
            let check = ui
                .horizontal(|ui| {
                    let check = ui
                        .add_enabled(
                            *status != UpdateStatus::Checking,
                            egui::Button::new(t!("updates-check-now")),
                        )
                        .clicked();
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
                            );
                        }
                    }
                    check
                })
                .inner;
            if let UpdateStatus::Failed(e) = status {
                theme::error(ui, e);
            }
            check
        })
    })
}
