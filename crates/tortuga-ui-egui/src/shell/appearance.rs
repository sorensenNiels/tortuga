//! The theme and text size (R-SET-4, R-SET-5): given to egui when the settings change,
//! and a text size changed with Ctrl+= / Ctrl+- / Ctrl+0 (egui's zoom keys) is saved.

use tortuga_core::settings::{Appearance, Theme};

use super::TortugaUi;

impl TortugaUi {
    pub(super) fn apply_appearance(&mut self, ctx: &egui::Context) {
        let wanted = self.state.settings.appearance;
        if let Some(applied) = self.applied_appearance {
            // The zoom keys changed the size since the last frame: keep it.
            let size = Appearance::text_size_for(ctx.zoom_factor());
            if size != applied.text_size && wanted.text_size == applied.text_size {
                self.state
                    .update_settings(|s| s.appearance.text_size = size);
                self.applied_appearance = Some(self.state.settings.appearance);
                return;
            }
            if applied.theme == wanted.theme && applied.text_size == wanted.text_size {
                return;
            }
        }
        ctx.set_theme(match wanted.theme {
            Theme::System => egui::ThemePreference::System,
            Theme::Light => egui::ThemePreference::Light,
            Theme::Dark => egui::ThemePreference::Dark,
        });
        ctx.set_zoom_factor(wanted.zoom());
        self.applied_appearance = Some(wanted);
    }
}

#[cfg(test)]
mod tests {
    use egui::{Key, Modifiers};

    use super::*;
    use crate::test_host::TestHost;

    /// R-SET-4, R-SET-5: the settings' theme and size are applied; the zoom keys change
    /// the size and it is saved.
    #[test]
    fn theme_and_text_size() {
        let mut ui = TortugaUi::new(Box::new(TestHost::default()));
        ui.state.update_settings(|s| {
            s.appearance.theme = Theme::Dark;
            s.appearance.text_size = 120;
        });
        let ui = std::rc::Rc::new(std::cell::RefCell::new(ui));
        let shared = std::rc::Rc::clone(&ui);
        let mut harness = crate::test_host::harness(move |egui_ui| shared.borrow_mut().ui(egui_ui));
        harness.run();
        let ctx = harness.ctx.clone();
        assert_eq!(
            ctx.options(|o| o.theme_preference),
            egui::ThemePreference::Dark
        );
        assert!((ctx.zoom_factor() - 1.2).abs() < 1e-6);

        harness.key_press_modifiers(Modifiers::COMMAND, Key::Minus);
        harness.run();
        harness.run();
        assert_eq!(ui.borrow().state.settings.appearance.text_size, 110);

        ui.borrow_mut()
            .state
            .update_settings(|s| s.appearance.text_size = 150);
        harness.run();
        assert!(
            (ctx.zoom_factor() - 1.5).abs() < 1e-6,
            "changed in Settings"
        );
    }
}
