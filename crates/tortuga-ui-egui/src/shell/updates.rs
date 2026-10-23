//! Whether a new version is out (R-SET-15, D63): checked in the background at start-up
//! unless turned off, and from Settings → About; shown as a quiet link in the top bar.

use egui::Ui;
use tortuga_core::update::{self, Found};

use super::{Inbox, TortugaUi};
use crate::components::UpdateStatus;

/// What started a check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Check {
    /// At start-up: a failure says nothing (P1).
    AtStart,
    /// "Check now": the About tab says what happened.
    Now,
}

/// The last check and where its result arrives.
#[derive(Debug)]
pub(super) struct Updates {
    pub(super) status: UpdateStatus,
    results: Inbox<(Check, Result<Found, String>)>,
}

impl Default for Updates {
    fn default() -> Self {
        Self {
            status: UpdateStatus::NotChecked,
            results: Inbox::new(),
        }
    }
}

impl TortugaUi {
    /// Starts the check at start-up, when the host can and the settings allow it.
    pub(super) fn check_for_updates_at_start(&mut self, ctx: &egui::Context) {
        if !self.state.host.checks_for_updates() {
            self.updates.status = UpdateStatus::Unsupported;
        } else if self.state.settings.updates.check_at_start {
            self.check_for_updates(ctx, Check::AtStart);
        }
    }

    /// "Check now" on the About tab.
    pub(super) fn check_for_updates_now(&mut self, ctx: &egui::Context) {
        if self.updates.status != UpdateStatus::Checking {
            self.check_for_updates(ctx, Check::Now);
        }
    }

    fn check_for_updates(&mut self, ctx: &egui::Context, check: Check) {
        self.updates.status = UpdateStatus::Checking;
        let client = self.state.client.clone();
        let tx = self.updates.results.sender();
        let repaint = ctx.clone();
        self.state.host.spawn(Box::pin(async move {
            let current = env!("CARGO_PKG_VERSION");
            let result = update::check(&client, update::LATEST_RELEASE_URL, current)
                .await
                .map_err(|e| tortuga_core::error::message(&e));
            // The receiver only goes away with the app.
            let _ = tx.send((check, result));
            repaint.request_repaint();
        }));
    }

    /// Takes the result of a check that finished.
    pub(super) fn collect_update_check(&mut self) {
        while let Some((check, result)) = self.updates.results.try_recv() {
            self.updates.status = match (result, check) {
                (Ok(Found::Newer(release)), _) => UpdateStatus::Available(release),
                (Ok(Found::UpToDate), _) => UpdateStatus::UpToDate,
                (Err(e), Check::Now) => UpdateStatus::Failed(e),
                (Err(_), Check::AtStart) => UpdateStatus::NotChecked,
            };
        }
    }

    /// The top bar's link to a newer version's release page, if one is out.
    pub(super) fn update_link(&self, ui: &mut Ui) {
        if let UpdateStatus::Available(release) = &self.updates.status {
            ui.hyperlink_to(
                t!("update-available", version = release.version.as_str()),
                &release.page,
            )
            .on_hover_text(t!("update-available-hint"));
        }
    }
}

#[cfg(test)]
mod tests {
    use egui_kittest::kittest::Queryable;
    use tortuga_core::update::Release;

    use super::*;
    use crate::test_host::TestHost;

    fn app(host: TestHost) -> TortugaUi {
        TortugaUi::new(Box::new(host))
    }

    /// R-SET-15: the check runs once at start-up where the host can check.
    #[test]
    fn the_check_runs_at_start_up() {
        let host = TestHost {
            checks_for_updates: true,
            ..TestHost::default()
        };
        let spawned = std::rc::Rc::clone(&host.spawned);
        let mut ui = app(host);
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(1400.0, 860.0))
            .build_ui(|egui_ui| ui.ui(egui_ui));
        h.run_steps(3);
        drop(h);
        assert_eq!(spawned.get(), 1, "one check, not one per frame");
        assert_eq!(ui.updates.status, UpdateStatus::Checking);
    }

    /// R-SET-15: turned off in the settings, or where the host can't check, nothing is sent.
    #[test]
    fn no_check_when_turned_off_or_unsupported() {
        let host = TestHost {
            checks_for_updates: true,
            ..TestHost::default()
        };
        let spawned = std::rc::Rc::clone(&host.spawned);
        let mut ui = app(host);
        ui.state.settings.updates.check_at_start = false;
        let ctx = egui::Context::default();
        ui.check_for_updates_at_start(&ctx);
        assert_eq!(spawned.get(), 0);
        assert_eq!(ui.updates.status, UpdateStatus::NotChecked);

        let mut ui = app(TestHost::default());
        ui.check_for_updates_at_start(&ctx);
        assert_eq!(ui.updates.status, UpdateStatus::Unsupported);
    }

    /// R-SET-15: "Check for updates" is in the palette where the host checks; it checks
    /// and opens Settings on the About tab, which shows the answer.
    #[test]
    fn the_palette_checks_for_updates() {
        let has_command = |ui: &TortugaUi| {
            ui.command_items()
                .iter()
                .any(|i| i.label == "Check for updates")
        };
        assert!(
            !has_command(&app(TestHost::default())),
            "not where it can't check"
        );

        let host = TestHost {
            checks_for_updates: true,
            ..TestHost::default()
        };
        let spawned = std::rc::Rc::clone(&host.spawned);
        let mut ui = app(host);
        assert!(has_command(&ui));
        let ctx = egui::Context::default();
        ui.run_command(super::super::commands::Command::CheckForUpdates, &ctx);
        assert_eq!(spawned.get(), 1);
        assert_eq!(ui.updates.status, UpdateStatus::Checking);
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(1400.0, 860.0))
            .build_ui(|egui_ui| ui.ui(egui_ui));
        h.run_steps(2);
        h.get_by_label("Checking…");
    }

    /// R-SET-15: a newer version shows in the top bar; a failed check at start-up
    /// says nothing, but "Check now" says why it failed.
    #[test]
    fn results_show_quietly() {
        let mut ui = app(TestHost::default());
        let tx = ui.updates.results.sender();
        tx.send((Check::AtStart, Err("offline".into())))
            .expect("inbox open");
        ui.collect_update_check();
        assert_eq!(ui.updates.status, UpdateStatus::NotChecked);
        tx.send((Check::Now, Err("offline".into())))
            .expect("inbox open");
        ui.collect_update_check();
        assert_eq!(ui.updates.status, UpdateStatus::Failed("offline".into()));

        tx.send((
            Check::AtStart,
            Ok(Found::Newer(Release {
                version: "9.0.0".into(),
                page: "https://github.com/sorensenNiels/tortuga/releases/tag/v9.0.0".into(),
            })),
        ))
        .expect("inbox open");
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(1400.0, 860.0))
            .build_ui(|egui_ui| ui.ui(egui_ui));
        h.run_steps(2);
        h.get_by_label("Tortuga 9.0.0 is available");
    }
}
