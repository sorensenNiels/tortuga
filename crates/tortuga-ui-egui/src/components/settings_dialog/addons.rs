//! Settings → Add-ons (R-ADD-1, D78): a switch per add-on this build has, all off by
//! default.

use egui::Ui;
use tortuga_core::settings::AddOnSettings;

use super::layout::{About, group};
use super::network::switch_row;
use crate::addons::{self, AddOn};
use crate::theme;

/// The add-ons in `listed`, each with its switch.
pub(super) fn addons_page(ui: &mut Ui, settings: &mut AddOnSettings, listed: &[AddOn]) {
    if listed.is_empty() {
        theme::hint(ui, t!("settings-addons-none"));
        return;
    }
    let note = (t!("settings-addons-about"), t!("settings-addons-hint"));
    group(
        ui,
        &t!("settings-addons"),
        Some((&note.0, &note.1)),
        |rows| {
            for addon in listed {
                let mut on = settings.enabled(*addon);
                let about = addons::about(*addon);
                if switch_row(rows, &addons::name(*addon), About::short(&about), &mut on) {
                    settings.set_enabled(*addon, on);
                }
            }
        },
    );
}
