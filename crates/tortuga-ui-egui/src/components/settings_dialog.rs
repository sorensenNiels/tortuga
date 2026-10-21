//! The app's settings dialog: requests (R-HTTP-7, R-HTTP-8, R-HTTP-16, R-HTTP-20,
//! R-SET-3), appearance (R-SET-4, R-SET-5, R-SET-7, R-SET-10, R-SET-12), editing
//! (R-SET-9, R-SET-11), features (R-SET-13), history (R-SET-6) and importers (R-COL-18).

use std::path::PathBuf;

use egui::{ComboBox, Context, TextEdit, Ui};
use tortuga_core::import::ImportKind;
use tortuga_core::settings::{
    Appearance, Density, EditingSettings, Feature, FeatureSettings, HistorySettings,
    ImporterSettings, RequestLayout, Theme,
};
use tortuga_core::{AppSettings, ClientSettings, ProxyMode, check_proxy_url};

use super::modals::{DialogResult, actions, finish};
use super::request_editor::parse_timeout;
use crate::theme;

/// The tabs of the settings dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingsTab {
    Requests,
    Appearance,
    Editing,
    Features,
    History,
    Importers,
}

#[derive(Debug, Clone)]
pub struct SettingsDialog {
    draft: AppSettings,
    tab: SettingsTab,
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
    /// The height of the shown tab's content last frame (see `fitting_scroll_area`).
    content_height: f32,
}

impl SettingsDialog {
    pub fn new(settings: &AppSettings) -> Self {
        Self {
            draft: settings.clone(),
            tab: SettingsTab::Requests,
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
            content_height: 0.0,
        }
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
    /// certificates and "Choose…" for the code font.
    pub fn show(&mut self, ctx: &Context, can_pick_files: bool) -> DialogResult<AppSettings> {
        super::dialog(ctx, "settings", "Settings", 480.0, true, |ui| {
            theme::tabs(
                ui,
                &mut self.tab,
                &[
                    (SettingsTab::Requests, "Requests", None),
                    (SettingsTab::Appearance, "Appearance", None),
                    (SettingsTab::Editing, "Editing", None),
                    (SettingsTab::Features, "Features", None),
                    (SettingsTab::History, "History", None),
                    (SettingsTab::Importers, "Importers", None),
                ],
            );
            // Long tabs scroll, so Save stays on screen in a small window.
            let mut height = self.content_height;
            super::fitting_scroll_area(ui, "settings-tab", 260.0, &mut height, |ui| {
                self.tab_content(ui, can_pick_files);
            });
            self.content_height = height;
            if let Some(err) = &self.error {
                theme::error(ui, err);
            }
            let proxy_problem = (self.draft.client.proxy == ProxyMode::Manual)
                .then(|| check_proxy_url(&self.draft.client.proxy_url).err())
                .flatten();
            // The Requests tab says what is wrong; elsewhere, say why Save is off.
            if let Some(e) = &proxy_problem
                && self.tab != SettingsTab::Requests
            {
                theme::error(ui, format!("Requests tab: {e}."));
            }
            let valid = proxy_problem.is_none();
            let (cancel, confirm) = actions(ui, theme::primary_button(ui, "Save"), valid);
            finish(cancel, confirm.then(|| self.draft.clone()))
        })
    }

    fn tab_content(&mut self, ui: &mut Ui, can_pick_files: bool) {
        match self.tab {
            SettingsTab::Requests => {
                let d = &mut self.draft.client;
                request_settings(ui, d, &mut self.timeout_text, &mut self.hosts_text);
                ui.add_space(10.0);
                proxy_settings(ui, d, &mut self.no_proxy_text);
                ui.add_space(10.0);
                self.add_ca_requested |=
                    ca_certificates(ui, &mut d.ca_certificates, can_pick_files);
            }
            SettingsTab::Appearance => {
                self.choose_font_requested |=
                    appearance_settings(ui, &mut self.draft.appearance, can_pick_files);
            }
            SettingsTab::Editing => editing_settings(ui, &mut self.draft.editing),
            SettingsTab::Features => feature_settings(ui, &mut self.draft.features),
            SettingsTab::History => {
                history_settings(ui, &mut self.draft.history, &mut self.history_text);
            }
            SettingsTab::Importers => importer_settings(ui, &mut self.draft.importers),
        }
    }
}

/// HTTP settings (R-HTTP-7, R-HTTP-8, R-SET-3).
fn request_settings(
    ui: &mut Ui,
    d: &mut ClientSettings,
    timeout_text: &mut String,
    hosts_text: &mut String,
) {
    ui.checkbox(&mut d.follow_redirects, "Follow redirects");
    ui.checkbox(&mut d.send_cookies, "Send and store cookies");
    ui.checkbox(&mut d.verify_tls, "Verify TLS certificates");
    if d.verify_tls {
        insecure_hosts(ui, d, hosts_text);
    } else {
        theme::warning(
            ui,
            "Turning this off lets anyone between you and the server read and change your traffic. Only use it for local test servers.",
        );
    }
    ui.horizontal(|ui| {
        let label = ui.label("Default timeout (ms)");
        if ui
            .add(TextEdit::singleline(timeout_text).desired_width(120.0))
            .labelled_by(label.id)
            .changed()
            && let Some(ms) = parse_timeout(timeout_text)
        {
            d.default_timeout_ms = ms;
        }
    });
    theme::hint(
        ui,
        "Workspaces and projects can set their own timeout, redirects and cookies (their \
         “Defaults…”).",
    );
}

/// Hosts whose certificates are not verified, on this machine only (R-SET-3).
fn insecure_hosts(ui: &mut Ui, d: &mut ClientSettings, text: &mut String) {
    ui.indent("insecure-hosts", |ui| {
        let label = ui.label("Don't verify certificates of these hosts:");
        let edit = TextEdit::multiline(text)
            .id_salt("insecure-hosts")
            .font(egui::TextStyle::Monospace)
            .hint_text("localhost\n*.dev.internal")
            .desired_rows(2)
            .desired_width(300.0);
        if ui.add(edit).labelled_by(label.id).changed() {
            d.insecure_hosts = lines(text);
        }
        theme::hint(
            ui,
            "One per line; *.example.test covers its subdomains. Only for test servers with \
             self-signed certificates. Kept on this computer: shared files can't turn \
             verification off.",
        );
        if !d.insecure_hosts.is_empty() {
            theme::warning(
                ui,
                "Traffic to these hosts could be read and changed by anyone in between.",
            );
        }
    });
}

/// The non-empty lines of `text`, trimmed.
fn lines(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Which proxy requests go through (R-HTTP-20, D54).
fn proxy_settings(ui: &mut Ui, d: &mut ClientSettings, no_proxy_text: &mut String) {
    theme::section_label(ui, "Proxy");
    ui.horizontal(|ui| {
        ui.radio_value(&mut d.proxy, ProxyMode::System, "From the system");
        ui.radio_value(&mut d.proxy, ProxyMode::None, "None");
        ui.radio_value(&mut d.proxy, ProxyMode::Manual, "Manual");
    });
    match d.proxy {
        ProxyMode::System => theme::hint(
            ui,
            "The operating system's proxy settings, or HTTP_PROXY, HTTPS_PROXY and NO_PROXY.",
        ),
        ProxyMode::None => theme::hint(
            ui,
            "Requests go straight to the server, whatever the system's settings say.",
        ),
        ProxyMode::Manual => manual_proxy(ui, d, no_proxy_text),
    }
}

/// The manual proxy's URL and the hosts that bypass it (R-HTTP-20).
fn manual_proxy(ui: &mut Ui, d: &mut ClientSettings, no_proxy_text: &mut String) {
    ui.indent("manual-proxy", |ui| {
        ui.horizontal(|ui| {
            let label = ui.label("Proxy URL");
            ui.add(
                TextEdit::singleline(&mut d.proxy_url)
                    .id_salt("proxy-url")
                    .hint_text("http://127.0.0.1:8080")
                    .desired_width(260.0),
            )
            .labelled_by(label.id);
        });
        if let Err(e) = check_proxy_url(&d.proxy_url) {
            theme::error(ui, tortuga_core::error::message(&e));
        }
        let label = ui.label("Don't use it for these hosts:");
        let edit = TextEdit::multiline(no_proxy_text)
            .id_salt("no-proxy")
            .font(egui::TextStyle::Monospace)
            .hint_text("localhost\n*.internal")
            .desired_rows(2)
            .desired_width(300.0);
        if ui.add(edit).labelled_by(label.id).changed() {
            d.no_proxy = lines(no_proxy_text);
        }
        theme::hint(
            ui,
            "One per line; example.com and *.example.com both cover its subdomains, and \
             10.0.0.0/8 a network. For mitmproxy, Charles or Fiddler, or a company proxy. \
             A user name or password in the URL is not supported. Kept on this computer.",
        );
    });
}

/// PEM files trusted besides the system's certificates (R-HTTP-16). Returns whether
/// "Add…" was clicked.
fn ca_certificates(ui: &mut Ui, files: &mut Vec<PathBuf>, can_pick_files: bool) -> bool {
    theme::section_label(ui, "CA certificates");
    let mut remove = None;
    for (i, path) in files.iter().enumerate() {
        ui.horizontal(|ui| {
            if ui
                .add(theme::icon_button(ui, "🗙"))
                .on_hover_text("Stop trusting it")
                .clicked()
            {
                remove = Some(i);
            }
            ui.add(egui::Label::new(path.display().to_string()).truncate())
                .on_hover_text(path.display().to_string());
        });
    }
    if let Some(i) = remove {
        files.remove(i);
    }
    let add = can_pick_files && ui.button("Add…").clicked();
    theme::hint(
        ui,
        "PEM files whose certificates are trusted besides the system's, e.g. your company's CA \
         for internal APIs. Kept on this computer only.",
    );
    add
}

/// Theme, text size, where the response goes, density, code font and size, and
/// wrapping (R-SET-4, R-SET-5, R-SET-7, R-SET-10, R-SET-12). Returns whether "Choose…" was clicked for the code font.
fn appearance_settings(ui: &mut Ui, a: &mut Appearance, can_pick_files: bool) -> bool {
    let mut choose_font = false;
    egui::Grid::new("appearance")
        .num_columns(2)
        .spacing([16.0, 10.0])
        .show(ui, |ui| {
            ui.label("Theme");
            ui.horizontal(|ui| {
                ui.radio_value(&mut a.theme, Theme::System, "Like the system");
                ui.radio_value(&mut a.theme, Theme::Light, "Light");
                ui.radio_value(&mut a.theme, Theme::Dark, "Dark");
            });
            ui.end_row();
            let label = ui.label("Text size");
            percent_choice(
                ui,
                "text-size",
                &mut a.text_size,
                &[80, 90, 100, 110, 125, 150, 175, 200],
            )
            .labelled_by(label.id);
            ui.end_row();
            ui.label("Response");
            ui.horizontal(|ui| {
                ui.radio_value(&mut a.layout, RequestLayout::Stacked, "Below the request");
                ui.radio_value(&mut a.layout, RequestLayout::SideBySide, "Beside it");
                ui.radio_value(
                    &mut a.layout,
                    RequestLayout::Automatic,
                    "Beside it when wide",
                );
            });
            ui.end_row();
            ui.label("Density");
            ui.horizontal(|ui| {
                ui.radio_value(&mut a.density, Density::Comfortable, "Comfortable");
                ui.radio_value(&mut a.density, Density::Compact, "Compact");
            });
            ui.end_row();
            ui.label("Code font");
            choose_font = code_font(ui, &mut a.code_font, can_pick_files);
            ui.end_row();
            let label = ui.label("Code size");
            percent_choice(
                ui,
                "code-size",
                &mut a.code_size,
                &[80, 90, 100, 110, 125, 150],
            )
            .labelled_by(label.id);
            ui.end_row();
        });
    theme::hint(
        ui,
        "Ctrl+= and Ctrl+- change the text size too; Ctrl+0 resets it. The code font and \
         size are for bodies, scripts and URLs, relative to the text size.",
    );
    ui.add_space(8.0);
    ui.checkbox(&mut a.wrap_lines, "Wrap long lines in responses");
    choose_font
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
                ui.label("JetBrains Mono (bundled)");
            }
        }
        let choose = can_pick_files && ui.button("Choose…").clicked();
        if font.is_some() && ui.button("Use the bundled one").clicked() {
            *font = None;
        }
        choose
    })
    .inner
}

/// How requests open and are saved (R-SET-9, R-SET-11).
fn editing_settings(ui: &mut Ui, e: &mut EditingSettings) {
    ui.checkbox(&mut e.preview_tab, "Open requests in a preview tab");
    ui.indent("preview-tab", |ui| {
        theme::hint(
            ui,
            "A single click shows a request in the preview tab (its title in italics), which \
             the next single click replaces; editing it or double-clicking keeps it. Off: \
             every request you open gets a tab that stays.",
        );
    });
    ui.add_space(6.0);
    ui.checkbox(&mut e.auto_save, "Save automatically");
    ui.indent("auto-save", |ui| {
        theme::hint(
            ui,
            "Edits to requests and workflows are saved a second after you stop typing, and \
             when you open something else or close the app. Not while a file changed on disk \
             under your edits (the banner asks first), and never the scratch request. Git is \
             your undo.",
        );
    });
}

/// Which features are shown (R-SET-13).
fn feature_settings(ui: &mut Ui, features: &mut FeatureSettings) {
    theme::hint(
        ui,
        "Turn off what you don't use to keep the app simple. Nothing is deleted: a request \
         that already uses a feature still shows it, and a mock server that runs can still \
         be stopped.",
    );
    ui.add_space(6.0);
    for feature in Feature::ALL {
        let mut on = features.enabled(feature);
        if ui
            .checkbox(&mut on, feature.name())
            .on_hover_text(format!("Off hides {}.", feature.hides()))
            .changed()
        {
            features.set_enabled(feature, on);
        }
    }
}

/// How many sent requests are kept per project, or none (R-SET-6).
fn history_settings(ui: &mut Ui, h: &mut HistorySettings, text: &mut String) {
    let mut on = h.on();
    if ui
        .checkbox(&mut on, "Keep a history of sent requests")
        .changed()
    {
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
        ui.horizontal(|ui| {
            let label = ui.label("Entries per project");
            if ui
                .add(TextEdit::singleline(text).desired_width(80.0))
                .labelled_by(label.id)
                .changed()
                && let Some(n) = text.trim().parse().ok().filter(|n: &usize| *n > 0)
            {
                h.size = n;
            }
        });
        theme::hint(ui, "Lowering it removes the oldest entries when you save.");
    } else {
        theme::hint(
            ui,
            "Nothing more is recorded. History that is already there stays until you clear it \
             in the History view.",
        );
    }
}

/// Which importers "+ Add" and the command palette offer (R-COL-18).
fn importer_settings(ui: &mut Ui, importers: &mut ImporterSettings) {
    theme::hint(
        ui,
        "Turn off the importers you don't use; they disappear from “+ Add” and the command \
         palette. Projects you imported earlier are not affected.",
    );
    ui.add_space(6.0);
    for kind in ImportKind::ALL {
        let mut on = importers.enabled(kind);
        if ui.checkbox(&mut on, kind.importer_name()).changed() {
            importers.set_enabled(kind, on);
        }
    }
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
                if let DialogResult::Done(r) = dialog.show(ui.ctx(), true) {
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

    /// R-HTTP-16: CA certificates are listed, added and removed in the settings.
    #[test]
    fn ca_certificates_in_settings() {
        let mut settings = AppSettings::default();
        settings.client.ca_certificates = vec![PathBuf::from("/etc/old-ca.pem")];
        let mut dialog = SettingsDialog::new(&settings);
        let mut result = None;
        {
            let mut h = harness(|ui| {
                if let DialogResult::Done(r) = dialog.show(ui.ctx(), true) {
                    result = Some(r);
                }
            });
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
                if let DialogResult::Done(r) = dialog.show(ui.ctx(), true) {
                    result = Some(r);
                }
            });
            h.run();
            h.get_by_label("Don't verify certificates of these hosts:");
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

    /// R-COL-18: importers are turned off on the Importers tab.
    #[test]
    fn importers_are_turned_off_in_settings() {
        let mut dialog = SettingsDialog::new(&AppSettings::default());
        let mut result = None;
        {
            let mut h = harness(|ui| {
                if let DialogResult::Done(r) = dialog.show(ui.ctx(), true) {
                    result = Some(r);
                }
            });
            h.run();
            h.get_by_label("Importers").click();
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
            h.get_by_label("Beside it when wide").click();
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
                if let DialogResult::Done(_) = dialog.show(ui.ctx(), true) {
                    saved_any.set(true);
                }
            });
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
                "Requests tab: the proxy can't be used: enter the proxy's URL, e.g. http://127.0.0.1:8080.",
            );
            h.get_by_label("Requests").click();
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
}
