//! The app's settings dialog (R-SET-17): pages listed on the left, each a few groups of
//! rows. Appearance (R-SET-4, R-SET-5, R-SET-7, R-SET-10, R-SET-12, R-I18N-1), editing
//! (R-SET-9, R-SET-11), the script editor (R-SET-16), requests (R-HTTP-7, R-HTTP-8), the
//! network (R-HTTP-16, R-HTTP-20, R-SET-3), features and importers (R-SET-13, R-COL-18),
//! history (R-SET-6) and about, with the new-version check (R-SET-14, R-SET-15).

mod about;
mod layout;
mod network;
mod script_editor_tab;

use std::path::PathBuf;

use egui::{ComboBox, Context, TextEdit, Ui};
use tortuga_core::import::ImportKind;
use tortuga_core::settings::{
    Appearance, Density, EditingSettings, Feature, FeatureSettings, HistorySettings,
    ImporterSettings, Language, RequestLayout, Theme,
};
use tortuga_core::update::Release;
use tortuga_core::{AppSettings, ProxyMode, check_proxy_url};

use super::modals::{DialogResult, actions, finish};
use crate::theme;
use layout::{About, Page, group};
use network::switch_row;

/// What the last look for a new version found (R-SET-15).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateStatus {
    NotChecked,
    Checking,
    UpToDate,
    Available(Release),
    /// Why "Check now" failed.
    Failed(String),
    /// The app doesn't look for new versions here (the web version).
    Unsupported,
}

#[derive(Debug, Clone)]
pub struct SettingsDialog {
    draft: AppSettings,
    page: Page,
    timeout_text: String,
    /// Hosts not to verify, one per line (R-SET-3).
    hosts_text: String,
    /// Hosts that bypass the manual proxy, one per line (R-HTTP-20).
    no_proxy_text: String,
    /// History entries per project while history is on (R-SET-6).
    history_text: String,
    /// Shown when saving failed.
    pub error: Option<String>,
    /// "Add…" was clicked for a CA certificate; the shell asks for the file (R-HTTP-16).
    pub add_ca_requested: bool,
    /// "Choose…" was clicked for the code font; the shell asks for the file (R-SET-10).
    pub choose_font_requested: bool,
    /// "Check now" was clicked on the About page; the shell checks (R-SET-15).
    pub check_requested: bool,
    /// The key being recorded on the Script editor page (R-SET-16).
    script_editor_tab: script_editor_tab::ScriptEditorTab,
}

impl SettingsDialog {
    pub fn new(settings: &AppSettings) -> Self {
        Self {
            draft: settings.clone(),
            page: Page::Appearance,
            timeout_text: settings.client.default_timeout_ms.to_string(),
            hosts_text: settings.client.insecure_hosts.join("\n"),
            no_proxy_text: settings.client.no_proxy.join("\n"),
            history_text: match settings.history.size {
                0 => tortuga_core::history::DEFAULT_SIZE,
                n => n,
            }
            .to_string(),
            error: None,
            add_ca_requested: false,
            choose_font_requested: false,
            check_requested: false,
            script_editor_tab: script_editor_tab::ScriptEditorTab::default(),
        }
    }

    /// Opens on the About page, e.g. to show what "Check for updates" finds (R-SET-15).
    #[must_use]
    pub fn on_about_tab(mut self) -> Self {
        self.page = Page::About;
        self
    }

    /// Adds a CA certificate file chosen after "Add…" (R-HTTP-16).
    pub fn add_ca(&mut self, path: PathBuf) {
        let files = &mut self.draft.client.ca_certificates;
        if !files.contains(&path) {
            files.push(path);
        }
    }

    /// Uses the font file chosen after "Choose…" for code (R-SET-10).
    pub fn set_code_font(&mut self, path: PathBuf) {
        self.draft.appearance.code_font = Some(path);
    }

    /// Done with the settings to apply. `can_pick_files`: offer "Add…" for CA
    /// certificates and "Choose…" for the code font. `update`: what the last look for
    /// a new version found, for the About page.
    pub fn show(
        &mut self,
        ctx: &Context,
        can_pick_files: bool,
        update: &UpdateStatus,
    ) -> DialogResult<AppSettings> {
        super::dialog(ctx, "settings", &t!("settings"), 760.0, true, |ui| {
            // One size for every page, so the dialog doesn't jump when the page changes.
            let height = (ui.ctx().content_rect().height() - 210.0).clamp(260.0, 500.0);
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(ui.available_width(), height),
                egui::Sense::hover(),
            );
            let (nav_rect, page_rect) = layout::split(rect);
            let mut nav = ui.new_child(egui::UiBuilder::new().max_rect(nav_rect));
            layout::nav(&mut nav, &mut self.page, height);
            let mut page = ui.new_child(egui::UiBuilder::new().max_rect(page_rect));
            // Nothing too wide for the page paints outside the dialog.
            page.set_clip_rect(page_rect.expand2(egui::vec2(0.0, 4.0)));
            egui::ScrollArea::vertical()
                .id_salt(("settings-page", self.page))
                .auto_shrink([false, false])
                .show(&mut page, |ui| {
                    ui.add_space(4.0);
                    self.page_content(ui, can_pick_files, update);
                });
            ui.add_space(12.0);
            if let Some(err) = &self.error {
                theme::error(ui, err);
            }
            let proxy_problem = (self.draft.client.proxy == ProxyMode::Manual)
                .then(|| check_proxy_url(&self.draft.client.proxy_url).err())
                .flatten();
            // The Network page says what is wrong; elsewhere, say why Save is off.
            if let Some(e) = &proxy_problem
                && self.page != Page::Network
            {
                theme::error(ui, t!("settings-network-page-error", error = e.to_string()));
            }
            let valid = proxy_problem.is_none();
            let (cancel, confirm) = actions(ui, theme::primary_button(ui, t!("save")), valid);
            finish(cancel, confirm.then(|| self.draft.clone()))
        })
    }

    fn page_content(&mut self, ui: &mut Ui, can_pick_files: bool, update: &UpdateStatus) {
        match self.page {
            Page::Appearance => {
                self.choose_font_requested |=
                    appearance_page(ui, &mut self.draft.appearance, can_pick_files);
            }
            Page::Editing => editing_page(ui, &mut self.draft.editing),
            Page::ScriptEditor => self
                .script_editor_tab
                .show(ui, &mut self.draft.script_editor),
            Page::Requests => {
                network::requests_page(ui, &mut self.draft.client, &mut self.timeout_text);
            }
            Page::Network => {
                self.add_ca_requested |= network::network_page(
                    ui,
                    &mut self.draft.client,
                    &mut self.hosts_text,
                    &mut self.no_proxy_text,
                    can_pick_files,
                );
            }
            Page::Features => {
                feature_settings(ui, &mut self.draft.features);
                importer_settings(ui, &mut self.draft.importers);
            }
            Page::History => {
                history_settings(ui, &mut self.draft.history, &mut self.history_text);
            }
            Page::About => {
                self.check_requested |= about::about_page(ui, &mut self.draft.updates, update);
            }
        }
    }
}

/// Theme and language, sizes, layout and the code font (R-SET-4, R-SET-5, R-SET-7,
/// R-SET-10, R-SET-12, R-I18N-1). Returns whether "Choose…" was clicked for the code font.
fn appearance_page(ui: &mut Ui, a: &mut Appearance, can_pick_files: bool) -> bool {
    group(ui, &t!("settings-group-theme"), None, |rows| {
        rows.row(&t!("settings-theme"), About::default(), |ui, _| {
            theme::segmented(
                ui,
                &mut a.theme,
                &[
                    (Theme::System, t!("settings-like-system")),
                    (Theme::Light, t!("settings-theme-light")),
                    (Theme::Dark, t!("settings-theme-dark")),
                ],
            );
        });
        // R-I18N-1: each language by its own name, so it can be found in any.
        rows.row(&t!("settings-language"), About::default(), |ui, _| {
            theme::segmented(
                ui,
                &mut a.language,
                &[
                    (Language::System, t!("settings-like-system")),
                    (Language::English, "English".to_owned()),
                    (Language::Danish, "Dansk".to_owned()),
                ],
            );
        });
    });
    let choose = group(ui, &t!("settings-group-text"), None, |rows| {
        rows.row(
            &t!("settings-text-size"),
            About::short(&t!("settings-text-size-about")),
            |ui, name| {
                percent_choice(
                    ui,
                    "text-size",
                    &mut a.text_size,
                    &[80, 90, 100, 110, 125, 150, 175, 200],
                )
                .labelled_by(name);
            },
        );
        rows.row(&t!("settings-density"), About::default(), |ui, _| {
            theme::segmented(
                ui,
                &mut a.density,
                &[
                    (Density::Comfortable, t!("settings-density-comfortable")),
                    (Density::Compact, t!("settings-density-compact")),
                ],
            );
        });
        let choose = rows.row(
            &t!("settings-code-font"),
            About::short(&t!("settings-code-about")),
            |ui, _| code_font(ui, &mut a.code_font, can_pick_files),
        );
        rows.row(&t!("settings-code-size"), About::default(), |ui, name| {
            percent_choice(
                ui,
                "code-size",
                &mut a.code_size,
                &[80, 90, 100, 110, 125, 150],
            )
            .labelled_by(name);
        });
        choose
    });
    layout_group(ui, a);
    choose
}

/// Where the response goes, and wrapping (R-SET-12, R-SET-4).
fn layout_group(ui: &mut Ui, a: &mut Appearance) {
    group(ui, &t!("settings-group-layout"), None, |rows| {
        rows.row(
            &t!("response"),
            About::short(&t!("settings-layout-about")),
            |ui, _| {
                theme::segmented(
                    ui,
                    &mut a.layout,
                    &[
                        (RequestLayout::Stacked, t!("settings-layout-below")),
                        (RequestLayout::SideBySide, t!("settings-layout-beside")),
                        (RequestLayout::Automatic, t!("settings-layout-automatic")),
                    ],
                );
            },
        );
        switch_row(
            rows,
            &t!("settings-wrap-lines"),
            About::default(),
            &mut a.wrap_lines,
        );
    });
}

/// A list of sizes in percent, with `value` in it even when it isn't one of `sizes`.
fn percent_choice(ui: &mut Ui, id: &str, value: &mut u16, sizes: &[u16]) -> egui::Response {
    let mut sizes = sizes.to_vec();
    if !sizes.contains(value) {
        sizes.push(*value);
        sizes.sort_unstable();
    }
    ComboBox::from_id_salt(id)
        .selected_text(format!("{value} %"))
        .show_ui(ui, |ui| {
            for size in sizes {
                ui.selectable_value(value, size, format!("{size} %"));
            }
        })
        .response
}

/// The code font: the bundled one or a file, with "Choose…" and a way back (R-SET-10).
/// Returns whether "Choose…" was clicked.
fn code_font(ui: &mut Ui, font: &mut Option<PathBuf>, can_pick_files: bool) -> bool {
    ui.horizontal(|ui| {
        match font {
            Some(path) => {
                let name = path.file_name().map_or_else(
                    || path.display().to_string(),
                    |n| n.to_string_lossy().into(),
                );
                ui.add(egui::Label::new(name).truncate())
                    .on_hover_text(path.display().to_string());
            }
            None => {
                ui.label(t!("settings-code-font-bundled"));
            }
        }
        let choose = can_pick_files && ui.button(t!("choose")).clicked();
        if font.is_some() && ui.button(t!("settings-code-font-reset")).clicked() {
            *font = None;
        }
        choose
    })
    .inner
}

/// How requests open and are saved (R-SET-9, R-SET-11).
fn editing_page(ui: &mut Ui, e: &mut EditingSettings) {
    group(ui, &t!("settings-group-tabs-saving"), None, |rows| {
        switch_row(
            rows,
            &t!("settings-preview-tab"),
            About::both(
                &t!("settings-preview-tab-about"),
                &t!("settings-preview-tab-hint"),
            ),
            &mut e.preview_tab,
        );
        switch_row(
            rows,
            &t!("settings-auto-save"),
            About::both(
                &t!("settings-auto-save-about"),
                &t!("settings-auto-save-hint"),
            ),
            &mut e.auto_save,
        );
    });
}

/// Which features are shown (R-SET-13).
fn feature_settings(ui: &mut Ui, features: &mut FeatureSettings) {
    let note = (t!("settings-features-about"), t!("settings-features-hint"));
    group(
        ui,
        &t!("settings-features"),
        Some((&note.0, &note.1)),
        |rows| {
            for feature in Feature::ALL {
                let mut on = features.enabled(feature);
                if switch_row(
                    rows,
                    &feature_name(feature),
                    About::short(&feature_hides(feature)),
                    &mut on,
                ) {
                    features.set_enabled(feature, on);
                }
            }
        },
    );
}

/// What Settings calls `feature` (R-SET-13).
fn feature_name(feature: Feature) -> String {
    match feature {
        Feature::Scripts => t!("feature-scripts"),
        Feature::Examples => t!("feature-examples"),
        Feature::Workflows => t!("feature-workflows"),
        Feature::Docs => t!("feature-docs"),
    }
}

/// What turning `feature` off hides.
fn feature_hides(feature: Feature) -> String {
    match feature {
        Feature::Scripts => t!("feature-scripts-hides"),
        Feature::Examples => t!("feature-examples-hides"),
        Feature::Workflows => t!("feature-workflows-hides"),
        Feature::Docs => t!("feature-docs-hides"),
    }
}

/// Which importers "+ Add" and the command palette offer (R-COL-18).
fn importer_settings(ui: &mut Ui, importers: &mut ImporterSettings) {
    let note = (
        t!("settings-importers-about"),
        t!("settings-importers-hint"),
    );
    group(
        ui,
        &t!("settings-importers"),
        Some((&note.0, &note.1)),
        |rows| {
            for kind in ImportKind::ALL {
                let mut on = importers.enabled(kind);
                if switch_row(rows, &super::importer_name(kind), About::default(), &mut on) {
                    importers.set_enabled(kind, on);
                }
            }
        },
    );
}

/// How many sent requests are kept per project, or none (R-SET-6).
fn history_settings(ui: &mut Ui, h: &mut HistorySettings, text: &mut String) {
    group(ui, &t!("history"), None, |rows| {
        let mut on = h.on();
        let off_about = t!("settings-history-off");
        let about = if on {
            About::default()
        } else {
            About::short(&off_about)
        };
        if switch_row(rows, &t!("settings-history-on"), about, &mut on) {
            h.size = if on {
                text.trim()
                    .parse()
                    .ok()
                    .filter(|n| *n > 0)
                    .unwrap_or(tortuga_core::history::DEFAULT_SIZE)
            } else {
                0
            };
        }
        if on {
            rows.row(
                &t!("settings-history-size"),
                About::short(&t!("settings-history-lower")),
                |ui, name| {
                    if ui
                        .add(TextEdit::singleline(text).desired_width(80.0))
                        .labelled_by(name)
                        .changed()
                        && let Some(n) = text.trim().parse().ok().filter(|n: &usize| *n > 0)
                    {
                        h.size = n;
                    }
                },
            );
        }
    });
}

#[cfg(test)]
mod tests {
    use egui_kittest::kittest::Queryable;

    use super::*;

    /// A window tall enough for the longest tab without scrolling.
    fn harness<'a>(mut app: impl FnMut(&mut Ui) + 'a) -> egui_kittest::Harness<'a> {
        let mut installed = false;
        egui_kittest::Harness::builder()
            .with_size(egui::vec2(900.0, 1400.0))
            .build_ui(move |ui| {
                if !installed {
                    crate::theme::install(ui.ctx());
                    installed = true;
                }
                app(ui);
            })
    }

    /// Shows `dialog` until Save is clicked by `steps`, and returns what it saved.
    fn saved(
        dialog: &mut SettingsDialog,
        steps: impl FnOnce(&mut egui_kittest::Harness<'_>),
    ) -> Option<AppSettings> {
        let mut result = None;
        {
            let mut h = harness(|ui| {
                if let DialogResult::Done(r) =
                    dialog.show(ui.ctx(), true, &UpdateStatus::NotChecked)
                {
                    result = Some(r);
                }
            });
            h.run();
            steps(&mut h);
            h.get_by_label("Save").click();
            h.run();
        }
        result
    }

    /// R-I18N-1: a language is chosen on the Appearance tab, by its own name.
    #[test]
    fn language_is_chosen_in_settings() {
        use tortuga_core::settings::Language;

        let mut dialog = SettingsDialog::new(&AppSettings::default());
        let saved = saved(&mut dialog, |h| {
            h.get_by_label("Appearance").click();
            h.run();
            h.get_by_label("Dansk").click();
            h.run();
        })
        .expect("saved");
        assert_eq!(saved.appearance.language, Language::Danish);
    }

    /// R-HTTP-16: CA certificates are listed, added and removed in the settings.
    #[test]
    fn ca_certificates_in_settings() {
        let mut settings = AppSettings::default();
        settings.client.ca_certificates = vec![PathBuf::from("/etc/old-ca.pem")];
        let mut dialog = SettingsDialog::new(&settings);
        let mut result = None;
        {
            let mut h = harness(|ui| {
                if let DialogResult::Done(r) =
                    dialog.show(ui.ctx(), true, &UpdateStatus::NotChecked)
                {
                    result = Some(r);
                }
            });
            h.run();
            h.get_by_label("Network").click();
            h.run();
            h.get_by_label("/etc/old-ca.pem");
            h.get_by_label("Add…").click();
            h.run();
            h.get_by_label("🗙").click();
            h.run();
            h.get_by_label("Save").click();
            h.run();
        }
        assert!(dialog.add_ca_requested);
        assert_eq!(
            result.expect("saved").client.ca_certificates,
            Vec::<PathBuf>::new()
        );
        dialog.add_ca(PathBuf::from("/etc/company-ca.pem"));
        dialog.add_ca(PathBuf::from("/etc/company-ca.pem"));
        assert_eq!(
            dialog.draft.client.ca_certificates,
            [PathBuf::from("/etc/company-ca.pem")]
        );
    }

    /// R-SET-3..6: hosts not to verify, theme, wrapping and history are set in the
    /// settings dialog.
    #[test]
    fn requests_appearance_and_history_settings() {
        let mut dialog = SettingsDialog::new(&AppSettings::default());
        let mut result = None;
        {
            let mut h = harness(|ui| {
                if let DialogResult::Done(r) =
                    dialog.show(ui.ctx(), true, &UpdateStatus::NotChecked)
                {
                    result = Some(r);
                }
            });
            h.run();
            h.get_by_label("Network").click();
            h.run();
            h.get_by_label("Hosts not to verify");
            let field = h
                .get_all_by_role(egui::accesskit::Role::MultilineTextInput)
                .next()
                .expect("hosts field");
            field.focus();
            field.type_text("localhost\n *.dev.test ");
            h.run();
            h.get_by_label("Appearance").click();
            h.run();
            h.get_by_label("Dark").click();
            h.get_by_label("Wrap long lines in responses")
                .scroll_to_me();
            h.run();
            h.get_by_label("Wrap long lines in responses").click();
            h.run();
            h.get_by_label("History").click();
            h.run();
            h.get_by_label("Keep a history of sent requests").click();
            h.run();
            h.get_by_label("Save").click();
            h.run();
        }
        let saved = result.expect("saved");
        assert_eq!(saved.client.insecure_hosts, ["localhost", "*.dev.test"]);
        assert_eq!(saved.appearance.theme, Theme::Dark);
        assert!(saved.appearance.wrap_lines);
        assert_eq!(saved.history.size, 0, "history off");
    }

    /// R-COL-18: importers are turned off on the Features page.
    #[test]
    fn importers_are_turned_off_in_settings() {
        let mut dialog = SettingsDialog::new(&AppSettings::default());
        let mut result = None;
        {
            let mut h = harness(|ui| {
                if let DialogResult::Done(r) =
                    dialog.show(ui.ctx(), true, &UpdateStatus::NotChecked)
                {
                    result = Some(r);
                }
            });
            h.run();
            h.get_by_label("Features").click();
            h.run();
            h.get_by_label("Postman collections").click();
            h.run();
            h.get_by_label("Save").click();
            h.run();
        }
        let saved = result.expect("saved");
        assert!(!saved.importers.enabled(ImportKind::Postman));
        assert!(saved.importers.enabled(ImportKind::OpenApi));
        assert!(saved.importers.enabled(ImportKind::Http));
    }

    /// R-SET-9, R-SET-11: the preview tab and auto-save are switched on the Editing tab.
    #[test]
    fn editing_settings_switch_the_preview_tab_and_auto_save() {
        let mut dialog = SettingsDialog::new(&AppSettings::default());
        let saved = saved(&mut dialog, |h| {
            h.get_by_label("Editing").click();
            h.run();
            h.get_by_label("Open requests in a preview tab").click();
            h.get_by_label("Save automatically").click();
            h.run();
        })
        .expect("saved");
        assert_eq!(
            saved.editing,
            EditingSettings {
                preview_tab: false,
                auto_save: true
            }
        );
    }

    /// R-SET-16: a script key is changed by clicking it and pressing another; Escape
    /// cancels; brackets are paired or not.
    #[test]
    fn script_editor_keys_are_changed_in_settings() {
        use tortuga_core::settings::ScriptCommand;

        let mut dialog = SettingsDialog::new(&AppSettings::default());
        let saved = saved(&mut dialog, |h| {
            h.get_by_label("Script editor").click();
            h.run();
            h.get_by_label("Close brackets and quotes").click();
            h.get_by_label("Ctrl+Shift+K").scroll_to_me();
            h.run();
            h.get_by_label("Ctrl+Shift+K").click();
            h.run();
            h.get_by_label("Press a key…");
            h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::D);
            h.run();
            h.get_by_label("Alt+Shift+F").scroll_to_me();
            h.run();
            h.get_by_label("Alt+Shift+F").click();
            h.run();
            h.key_press(egui::Key::Escape);
            h.run();
            assert!(h.query_by_label("Press a key…").is_none(), "Escape cancels");
        })
        .expect("saved, Escape didn't close the dialog");
        let s = &saved.script_editor;
        assert!(!s.close_pairs);
        assert_eq!(s.keys(ScriptCommand::DeleteLines), ["Ctrl+D"]);
        assert!(!s.changed(ScriptCommand::Beautify));
    }

    /// R-SET-16: choosing a scheme gives its keys and drops the changed ones.
    #[test]
    fn a_key_scheme_is_chosen_in_settings() {
        use tortuga_core::settings::{KeyScheme, ScriptCommand};

        let mut settings = AppSettings::default();
        settings
            .script_editor
            .set_keys(ScriptCommand::DeleteLines, vec!["Ctrl+E".into()]);
        let mut dialog = SettingsDialog::new(&settings);
        let saved = saved(&mut dialog, |h| {
            h.get_by_label("Script editor").click();
            h.run();
            h.get_by_label("Choosing a scheme replaces the keys you changed.");
            h.get_by_role_and_label(egui::accesskit::Role::ComboBox, "Keys like")
                .click();
            h.run();
            h.get_by_label("JetBrains (IntelliJ, WebStorm, …)").click();
            h.run();
            h.get_by_label("Ctrl+Alt+L");
            assert!(h.query_by_label("Reset all").is_none(), "nothing changed");
        })
        .expect("saved");
        let s = &saved.script_editor;
        assert_eq!(s.scheme, KeyScheme::JetBrains);
        assert_eq!(s.keys(ScriptCommand::DeleteLines), ["Ctrl+Y"]);
        assert!(!s.changed(ScriptCommand::DeleteLines));
    }

    /// R-SET-10: density, code size and going back to the bundled code font.
    #[test]
    fn density_code_size_and_font_in_appearance() {
        let mut settings = AppSettings::default();
        settings.appearance.code_font = Some(PathBuf::from("/fonts/Fira Code ü😀.ttf"));
        let mut dialog = SettingsDialog::new(&settings);
        let saved = saved(&mut dialog, |h| {
            h.get_by_label("Appearance").click();
            h.run();
            h.get_by_label("Fira Code ü😀.ttf");
            h.get_by_label("Compact").click();
            h.get_by_label("Automatic").scroll_to_me();
            h.run();
            h.get_by_label("Automatic").click();
            h.get_by_label("Choose…").click();
            h.run();
            h.get_by_label("Use the bundled one").click();
            h.run();
            h.get_by_label("JetBrains Mono (bundled)");
            h.get_by_label("Code size").click();
            h.run();
            h.get_by_label("125 %").click();
            h.run();
        })
        .expect("saved");
        assert!(dialog.choose_font_requested);
        assert_eq!(saved.appearance.density, Density::Compact);
        assert_eq!(saved.appearance.layout, RequestLayout::Automatic);
        assert_eq!(saved.appearance.code_font, None);
        assert_eq!(saved.appearance.code_size, 125);
        dialog.set_code_font(PathBuf::from("/fonts/mono.otf"));
        assert_eq!(
            dialog.draft.appearance.code_font.as_deref(),
            Some(std::path::Path::new("/fonts/mono.otf"))
        );
    }

    /// R-HTTP-20: a manual proxy can't be saved with a URL that can't be used; its
    /// bypass list is one host per line.
    #[test]
    fn manual_proxy_needs_a_usable_url() {
        let mut dialog = SettingsDialog::new(&AppSettings::default());
        let saved_any = std::cell::Cell::new(false);
        {
            let mut h = harness(|ui| {
                if let DialogResult::Done(_) =
                    dialog.show(ui.ctx(), true, &UpdateStatus::NotChecked)
                {
                    saved_any.set(true);
                }
            });
            h.run();
            h.get_by_label("Network").click();
            h.run();
            h.get_by_label("Manual").click();
            h.run();
            h.get_by_label(
                "The proxy can't be used: enter the proxy's URL, e.g. http://127.0.0.1:8080",
            );
            h.get_by_label("Save").click();
            h.run();
            assert!(!saved_any.get(), "no URL yet");
            h.get_by_label("Editing").click();
            h.run();
            h.get_by_label(
                "Network: the proxy can't be used: enter the proxy's URL, e.g. http://127.0.0.1:8080.",
            );
            h.get_by_label("Network").click();
            h.run();
            let url = h.get_by_label("Proxy URL");
            url.focus();
            url.type_text("http://me:pw@127.0.0.1:8080");
            h.run();
            h.get_by_label(
                "The proxy can't be used: a user name or password in the URL is not supported",
            );
            h.get_by_label("Save").click();
            h.run();
            assert!(!saved_any.get(), "credentials refused");
        }
        dialog.draft.client.proxy_url = "127.0.0.1:8080".into();
        let saved = saved(&mut dialog, |h| {
            let hosts = h
                .get_all_by_role(egui::accesskit::Role::MultilineTextInput)
                .last()
                .expect("bypass field");
            hosts.focus();
            hosts.type_text("localhost\n\n *.internal ");
            h.run();
        })
        .expect("saved");
        assert_eq!(saved.client.proxy, ProxyMode::Manual);
        assert_eq!(saved.client.no_proxy, ["localhost", "*.internal"]);
    }

    /// R-SET-13: features are turned off on the Features tab.
    #[test]
    fn features_are_turned_off_in_settings() {
        let mut dialog = SettingsDialog::new(&AppSettings::default());
        let saved = saved(&mut dialog, |h| {
            h.get_by_label("Features").click();
            h.run();
            h.get_by_label("Workflows").click();
            h.get_by_label("Examples and the mock server").click();
            h.run();
        })
        .expect("saved");
        assert!(!saved.features.enabled(Feature::Workflows));
        assert!(!saved.features.enabled(Feature::Examples));
        assert!(saved.features.enabled(Feature::Scripts));
        assert!(saved.features.enabled(Feature::Docs));
    }

    /// R-SET-14: the About tab shows the version, and copies it for a bug report.
    #[test]
    fn about_shows_the_version() {
        let mut dialog = SettingsDialog::new(&AppSettings::default());
        let mut h = harness(|ui| {
            dialog.show(ui.ctx(), true, &UpdateStatus::NotChecked);
        });
        h.run();
        h.get_by_label("About").click();
        h.run();
        h.get_by_label(&format!("Version {}", env!("CARGO_PKG_VERSION")));
        h.get_by_label("Copy version information").click();
        // The press and the release take a frame each; the copy is in one of them.
        let mut copied = false;
        for _ in 0..3 {
            h.step();
            copied |= h.output().platform_output.commands.iter().any(|c| {
                matches!(c, egui::OutputCommand::CopyText(t)
                    if t.starts_with("Tortuga ") && t.contains(std::env::consts::OS))
            });
        }
        assert!(copied);
    }

    /// R-SET-15: the About tab turns the check at start-up off, asks for a check, and
    /// says what the last one found.
    #[test]
    fn about_checks_for_a_new_version() {
        let mut dialog = SettingsDialog::new(&AppSettings::default());
        let saved = saved(&mut dialog, |h| {
            h.get_by_label("About").click();
            h.run();
            h.get_by_label("Look for a new version when the app starts")
                .click();
            h.get_by_label("Check now").click();
            h.run();
        })
        .expect("saved");
        assert!(dialog.check_requested);
        assert!(!saved.updates.check_at_start);

        let shows = |status: UpdateStatus, text: &str| {
            let mut dialog = SettingsDialog::new(&AppSettings::default());
            dialog.page = Page::About;
            let mut h = harness(|ui| {
                dialog.show(ui.ctx(), true, &status);
            });
            h.run();
            h.get_by_label(text);
        };
        shows(UpdateStatus::UpToDate, "You have the latest version.");
        shows(
            UpdateStatus::Available(Release {
                version: "1.0.0".into(),
                page: "https://github.com/sorensenNiels/tortuga/releases/tag/v1.0.0".into(),
                downloads: Vec::new(),
            }),
            "Tortuga 1.0.0 is available",
        );
        shows(
            UpdateStatus::Failed("Could not look for a new version: ü😀".into()),
            "Could not look for a new version: ü😀",
        );
        shows(
            UpdateStatus::Unsupported,
            "Tortuga sends nothing on its own: no telemetry, no update checks. It only contacts the servers your requests go to.",
        );
    }
}
