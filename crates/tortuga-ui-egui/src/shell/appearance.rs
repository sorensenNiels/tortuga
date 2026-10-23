//! The theme, text size, density, code font and language (R-SET-4, R-SET-5, R-SET-10,
//! R-I18N-1): given to egui when the settings change, and a text size changed with
//! Ctrl+= / Ctrl+- / Ctrl+0 (egui's zoom keys) is saved.

use std::path::Path;

use tortuga_core::settings::{Appearance, Theme, check_font_file};

use super::TortugaUi;
use crate::host::Host;
use crate::i18n;
use crate::theme;

impl TortugaUi {
    pub(super) fn apply_appearance(&mut self, ctx: &egui::Context) {
        if let Some(applied) = &self.applied_appearance {
            let wanted = &self.state.settings.appearance;
            // The zoom keys changed the size since the last frame: keep it.
            let size = Appearance::text_size_for(ctx.zoom_factor());
            if size != applied.text_size && wanted.text_size == applied.text_size {
                self.state
                    .update_settings(|s| s.appearance.text_size = size);
                self.applied_appearance = Some(self.state.settings.appearance.clone());
                return;
            }
            if applied == wanted {
                return;
            }
        }
        let wanted = self.state.settings.appearance.clone();
        let applied = self.applied_appearance.take();
        let os_language = self.state.host.os_language();
        i18n::set(i18n::resolve(wanted.language, os_language.as_deref()));
        ctx.set_theme(match wanted.theme {
            Theme::System => egui::ThemePreference::System,
            Theme::Light => egui::ThemePreference::Light,
            Theme::Dark => egui::ThemePreference::Dark,
        });
        ctx.set_zoom_factor(wanted.zoom());
        if applied
            .as_ref()
            .is_none_or(|a| a.density != wanted.density || a.code_size != wanted.code_size)
        {
            theme::set_sizes(ctx, wanted.density, wanted.code_scale());
        }
        // Only on a change: the bundled fonts are installed with the theme.
        if applied.as_ref().map_or(wanted.code_font.is_some(), |a| {
            a.code_font != wanted.code_font
        }) {
            let bytes = wanted.code_font.as_deref().and_then(|path| {
                read_font(self.state.host.as_ref(), path)
                    .map_err(|e| {
                        self.state.notify(t!("code-font-fallback", error = e));
                    })
                    .ok()
            });
            theme::set_code_font(ctx, bytes);
        }
        self.applied_appearance = Some(wanted);
    }
}

/// The bytes of the font file at `path`, if it is one (R-SET-10).
pub(super) fn read_font(host: &dyn Host, path: &Path) -> Result<Vec<u8>, String> {
    let Some(reader) = host.file_reader() else {
        return Err(t!("code-font-no-files"));
    };
    let bytes = (reader.0)(path).map_err(|e| format!("{}: {e}", path.display()))?;
    check_font_file(path, &bytes).map_err(|e| e.to_string())?;
    Ok(bytes)
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

    /// R-I18N-1, R-I18N-2: the UI follows the OS's language unless one is chosen in
    /// Settings, and changes at once when the setting does.
    #[test]
    fn the_ui_is_shown_in_the_chosen_language() {
        use egui_kittest::kittest::Queryable;
        use tortuga_core::settings::Language;

        let host = TestHost {
            os_language: Some("da-DK".into()),
            ..TestHost::default()
        };
        let ui = std::rc::Rc::new(std::cell::RefCell::new(TortugaUi::new(Box::new(host))));
        let shared = std::rc::Rc::clone(&ui);
        let mut harness = crate::test_host::harness(move |egui_ui| shared.borrow_mut().ui(egui_ui));
        harness.run();
        harness.get_by_label("Kladdeanmodning");
        harness.get_by_label("Send en anmodning for at se svaret her.");

        ui.borrow_mut()
            .state
            .update_settings(|s| s.appearance.language = Language::English);
        harness.run();
        harness.run();
        harness.get_by_label("Scratch request");
        assert!(harness.query_by_label("Kladdeanmodning").is_none());

        ui.borrow_mut()
            .state
            .update_settings(|s| s.appearance.language = Language::Danish);
        harness.run();
        harness.run();
        harness.get_by_label("Kladdeanmodning");
        crate::i18n::set(Language::English);
    }

    /// R-SET-10: density and code size reach egui's style; a code font file is
    /// installed, and one that can't be read leaves the bundled font with a notice.
    #[test]
    fn density_code_size_and_code_font() {
        use tortuga_core::settings::Density;

        let font = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("assets/fonts/JetBrainsMono-Regular.ttf");
        let mut ui = TortugaUi::new(Box::new(TestHost::default()));
        ui.state.update_settings(|s| {
            s.appearance.density = Density::Compact;
            s.appearance.code_size = 120;
            s.appearance.code_font = Some(font.clone());
        });
        let ui = std::rc::Rc::new(std::cell::RefCell::new(ui));
        let shared = std::rc::Rc::clone(&ui);
        let mut harness = crate::test_host::harness(move |egui_ui| shared.borrow_mut().ui(egui_ui));
        harness.run();
        let ctx = harness.ctx.clone();
        let style = ctx.global_style();
        assert!(
            (style.spacing.interact_size.y - 22.0).abs() < 1e-6,
            "compact"
        );
        let mono = &style.text_styles[&egui::TextStyle::Monospace];
        assert!((mono.size - 15.0).abs() < 1e-6, "12.5 at 120 %");
        let has_code_font =
            || ctx.fonts(|f| f.definitions().font_data.contains_key("user-code-font"));
        assert!(has_code_font());
        assert_eq!(ui.borrow().state.notice, None);

        ui.borrow_mut().state.update_settings(|s| {
            s.appearance.code_font = Some("/no/such/font ü😀.ttf".into());
        });
        harness.run();
        assert!(!has_code_font(), "back to the bundled font");
        let notice = ui.borrow().state.notice.clone().expect("notice");
        assert!(notice.contains("The code font can't be used"), "{notice}");

        ui.borrow_mut().state.update_settings(|s| {
            s.appearance.density = Density::Comfortable;
            s.appearance.code_font = None;
        });
        harness.run();
        assert!((ctx.global_style().spacing.interact_size.y - 28.0).abs() < 1e-6);
        // The semibold family must survive a change of fonts: egui panics without it.
        assert!(ctx.fonts(|f| {
            f.definitions()
                .families
                .contains_key(&egui::FontFamily::Name(crate::theme::SEMIBOLD.into()))
        }));
    }
}
