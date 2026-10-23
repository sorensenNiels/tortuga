//! Settings of the app itself, kept on this machine (R-LOC-1): the HTTP client's
//! (R-HTTP-7, R-HTTP-8, R-HTTP-16, R-SET-3), which importers are offered (R-COL-18),
//! appearance (R-SET-4, R-SET-5, R-SET-7, R-SET-10), history (R-SET-6), editing
//! (R-SET-9, R-SET-11), the script editor (R-SET-16) and the new-version check
//! (R-SET-15); and the request settings a workspace or project can override (R-SET-1,
//! D39).
//!
//! Every setting has a default that suits most developers, and only settings that
//! differ from it are written, so a new setting never needs a migration.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::http::ClientSettings;
use crate::import::ImportKind;

mod script_editor;
pub use script_editor::{KeyScheme, ScriptCommand, ScriptEditorSettings};

/// Everything in the app's settings file.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    /// Kept at the top level of the file, where settings saved before importers
    /// existed have them.
    #[serde(flatten)]
    pub client: ClientSettings,
    #[serde(skip_serializing_if = "ImporterSettings::is_default")]
    pub importers: ImporterSettings,
    #[serde(skip_serializing_if = "Appearance::is_default")]
    pub appearance: Appearance,
    #[serde(skip_serializing_if = "HistorySettings::is_default")]
    pub history: HistorySettings,
    #[serde(skip_serializing_if = "EditingSettings::is_default")]
    pub editing: EditingSettings,
    #[serde(skip_serializing_if = "FeatureSettings::is_default")]
    pub features: FeatureSettings,
    #[serde(skip_serializing_if = "UpdateSettings::is_default")]
    pub updates: UpdateSettings,
    #[serde(skip_serializing_if = "ScriptEditorSettings::is_default")]
    pub script_editor: ScriptEditorSettings,
}

/// Whether the app looks for a new version by itself (R-SET-15, D63).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdateSettings {
    /// Ask GitHub for the latest release when the app starts.
    pub check_at_start: bool,
}

impl Default for UpdateSettings {
    fn default() -> Self {
        Self {
            check_at_start: true,
        }
    }
}

impl UpdateSettings {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// A part of the app that can be turned off where it isn't used (R-SET-13).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feature {
    /// Pre-request and test scripts, and captures (R-TEST-1, R-TEST-4).
    Scripts,
    /// Example responses and the mock server (R-MOCK-1, R-MOCK-2).
    Examples,
    /// Workflows (R-FLOW-1).
    Workflows,
    /// Descriptions of requests, folders and projects (R-COL-22).
    Docs,
}

impl Feature {
    /// Every feature, in the order Settings lists them.
    pub const ALL: [Self; 4] = [Self::Scripts, Self::Examples, Self::Workflows, Self::Docs];

    /// The name the settings file keeps it under.
    pub fn id(self) -> &'static str {
        match self {
            Self::Scripts => "scripts",
            Self::Examples => "examples",
            Self::Workflows => "workflows",
            Self::Docs => "docs",
        }
    }
}

/// Which features are shown (R-SET-13). All are on by default, including ones added in
/// later versions; turning one off only hides it, its files stay as they are.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct FeatureSettings {
    /// Ids of the features turned off ([`Feature::id`]). Kept as text, so a file written
    /// by a newer version with features this one doesn't know still loads.
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    pub disabled: BTreeSet<String>,
}

impl FeatureSettings {
    pub fn is_default(&self) -> bool {
        self.disabled.is_empty()
    }

    pub fn enabled(&self, feature: Feature) -> bool {
        !self.disabled.contains(feature.id())
    }

    pub fn set_enabled(&mut self, feature: Feature, enabled: bool) {
        if enabled {
            self.disabled.remove(feature.id());
        } else {
            self.disabled.insert(feature.id().to_owned());
        }
    }
}

/// How requests open and are saved (R-SET-9, R-SET-11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EditingSettings {
    /// A single click opens a request in the preview tab (R-COL-13). Off: in a tab
    /// that stays (R-SET-9).
    pub preview_tab: bool,
    /// Edits are written to disk shortly after typing stops (R-SET-11, D53).
    pub auto_save: bool,
}

impl Default for EditingSettings {
    fn default() -> Self {
        Self {
            preview_tab: true,
            auto_save: false,
        }
    }
}

impl EditingSettings {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// Where the response goes relative to the request (R-SET-12).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RequestLayout {
    /// The request above, the response below.
    #[default]
    Stacked,
    /// The request on the left, the response on the right.
    SideBySide,
    /// Side by side when the window is wide enough, else stacked.
    Automatic,
}

/// Room around controls (R-SET-10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Density {
    #[default]
    Comfortable,
    Compact,
}

/// Light, dark, or whatever the operating system uses (R-SET-4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

/// The language of the UI (R-I18N-1): the operating system's, or one chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Language {
    #[serde(rename = "en")]
    English,
    #[serde(rename = "da")]
    Danish,
    /// The operating system's language, English when the UI has no translation for it.
    /// Also what a language a newer version wrote becomes, so the file still loads.
    #[default]
    #[serde(rename = "system", other)]
    System,
}

/// How the app looks (R-SET-4, R-SET-5, R-SET-7, R-SET-10, R-I18N-1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Appearance {
    pub theme: Theme,
    /// Size of the whole UI in percent (R-SET-5).
    pub text_size: u16,
    /// Wrap long lines in response bodies (R-SET-7).
    pub wrap_lines: bool,
    #[serde(skip_serializing_if = "is_default")]
    pub density: Density,
    /// A font file for code: bodies, scripts, URLs (R-SET-10). `None`: the bundled one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code_font: Option<std::path::PathBuf>,
    /// Size of code relative to the rest of the UI, in percent (R-SET-10).
    #[serde(skip_serializing_if = "is_hundred")]
    pub code_size: u16,
    #[serde(skip_serializing_if = "is_default")]
    pub layout: RequestLayout,
    #[serde(skip_serializing_if = "is_default")]
    pub language: Language,
}

/// Checks that `bytes`, read from `path`, are a font egui can draw with (R-SET-10):
/// TrueType or OpenType, or a collection of them. A file that only looks right is
/// still skipped by egui without harm, with the bundled font used instead.
pub fn check_font_file(path: &std::path::Path, bytes: &[u8]) -> crate::Result<()> {
    const SIGNATURES: [&[u8]; 4] = [b"\x00\x01\x00\x00", b"OTTO", b"true", b"ttcf"];
    if SIGNATURES.iter().any(|s| bytes.starts_with(s)) {
        Ok(())
    } else {
        Err(crate::Error::InvalidFile {
            path: path.to_owned(),
            message: "not a TrueType or OpenType font (.ttf, .otf, .ttc)".into(),
        })
    }
}

fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's skip_serializing_if passes a reference"
)]
fn is_hundred(percent: &u16) -> bool {
    *percent == 100
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            text_size: 100,
            wrap_lines: false,
            density: Density::Comfortable,
            code_font: None,
            code_size: 100,
            layout: RequestLayout::Stacked,
            language: Language::System,
        }
    }
}

impl Appearance {
    /// Text sizes egui's zoom keys can reach that make sense here, in percent.
    pub const TEXT_SIZES: std::ops::RangeInclusive<u16> = 50..=300;
    /// Code sizes the settings offer, in percent of the UI's (R-SET-10).
    pub const CODE_SIZES: std::ops::RangeInclusive<u16> = 70..=200;

    /// The code size as a factor, within [`Self::CODE_SIZES`] (R-SET-10).
    pub fn code_scale(&self) -> f32 {
        let percent = self
            .code_size
            .clamp(*Self::CODE_SIZES.start(), *Self::CODE_SIZES.end());
        f32::from(percent) / 100.0
    }

    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// The UI's zoom factor for [`Self::text_size`], within [`Self::TEXT_SIZES`].
    pub fn zoom(&self) -> f32 {
        let percent = self
            .text_size
            .clamp(*Self::TEXT_SIZES.start(), *Self::TEXT_SIZES.end());
        f32::from(percent) / 100.0
    }

    /// The text size for a zoom factor, e.g. after Ctrl+= (R-SET-5).
    pub fn text_size_for(zoom: f32) -> u16 {
        let percent = (zoom * 100.0).round().clamp(
            f32::from(*Self::TEXT_SIZES.start()),
            f32::from(*Self::TEXT_SIZES.end()),
        );
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "clamped to 50..=300 above"
        )]
        let percent = percent as u16;
        percent
    }
}

/// How much request history is kept (R-SET-6, R-HIST-1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct HistorySettings {
    /// Entries kept per project; 0 turns history off (nothing more is recorded).
    pub size: usize,
}

impl Default for HistorySettings {
    fn default() -> Self {
        Self {
            size: crate::history::DEFAULT_SIZE,
        }
    }
}

impl HistorySettings {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// History is recorded.
    pub fn on(&self) -> bool {
        self.size > 0
    }
}

/// Which importers "+ Add" and the command palette offer (R-COL-18). All are on by
/// default, including ones added in later versions.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ImporterSettings {
    /// Ids of the importers turned off ([`ImportKind::id`]). Kept as text, so a file
    /// written by a newer version with importers this one doesn't know still loads.
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    pub disabled: BTreeSet<String>,
}

impl ImporterSettings {
    pub fn is_default(&self) -> bool {
        self.disabled.is_empty()
    }

    pub fn enabled(&self, kind: ImportKind) -> bool {
        !self.disabled.contains(kind.id())
    }

    pub fn set_enabled(&mut self, kind: ImportKind, enabled: bool) {
        if enabled {
            self.disabled.remove(kind.id());
        } else {
            self.disabled.insert(kind.id().to_owned());
        }
    }

    /// The importers that are on, in menu order.
    pub fn enabled_kinds(&self) -> impl Iterator<Item = ImportKind> + '_ {
        ImportKind::ALL.into_iter().filter(|k| self.enabled(*k))
    }
}

/// How requests are sent, as a workspace or project sets it (R-SET-1). `None` inherits
/// from the level above: request → project → workspace → app (D39).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RequestSettings {
    /// Timeout for requests without their own (R-HTTP-7).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub follow_redirects: Option<bool>,
    /// Send and store cookies (R-CK-2); a request can still turn them off.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cookies: Option<bool>,
}

impl RequestSettings {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// These settings, with what they leave unset taken from `fallback`.
    #[must_use]
    pub fn or(self, fallback: Self) -> Self {
        Self {
            timeout_ms: self.timeout_ms.or(fallback.timeout_ms),
            follow_redirects: self.follow_redirects.or(fallback.follow_redirects),
            cookies: self.cookies.or(fallback.cookies),
        }
    }

    /// The app's settings, the last level, where everything is set.
    pub fn from_app(client: &ClientSettings) -> Self {
        Self {
            timeout_ms: Some(client.default_timeout_ms),
            follow_redirects: Some(client.follow_redirects),
            cookies: Some(client.send_cookies),
        }
    }
}

/// A level settings come from (D39).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Project,
    Workspace,
    App,
}

/// A value and the level it comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Inherited<T> {
    pub value: T,
    pub from: Level,
}

/// Every request setting with where it comes from (R-SET-1), to show inherited values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedSettings {
    pub timeout_ms: Inherited<u64>,
    pub follow_redirects: Inherited<bool>,
    pub cookies: Inherited<bool>,
}

impl ResolvedSettings {
    /// The settings of `levels`, nearest first, over the app's.
    pub fn resolve(levels: &[(Level, RequestSettings)], app: &ClientSettings) -> Self {
        fn pick<T: Copy>(
            levels: &[(Level, RequestSettings)],
            get: impl Fn(&RequestSettings) -> Option<T>,
            app: T,
        ) -> Inherited<T> {
            levels
                .iter()
                .find_map(|(level, s)| {
                    get(s).map(|value| Inherited {
                        value,
                        from: *level,
                    })
                })
                .unwrap_or(Inherited {
                    value: app,
                    from: Level::App,
                })
        }
        Self {
            timeout_ms: pick(levels, |s| s.timeout_ms, app.default_timeout_ms),
            follow_redirects: pick(levels, |s| s.follow_redirects, app.follow_redirects),
            cookies: pick(levels, |s| s.cookies, app.send_cookies),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// R-COL-18: importers are on by default; turning one off is the only thing written.
    #[test]
    fn importers_default_on_and_round_trip() {
        let mut settings = AppSettings::default();
        assert!(
            ImportKind::ALL
                .iter()
                .all(|k| settings.importers.enabled(*k))
        );
        assert_eq!(
            toml::to_string(&settings).expect("toml"),
            toml::to_string(&ClientSettings::default()).expect("toml"),
            "nothing about importers while all are on"
        );

        settings.importers.set_enabled(ImportKind::Postman, false);
        let text = toml::to_string(&settings).expect("toml");
        assert!(
            text.contains("[importers]\ndisabled = [\"postman\"]"),
            "{text}"
        );
        let back: AppSettings = toml::from_str(&text).expect("read");
        assert_eq!(back, settings);
        assert_eq!(
            back.importers.enabled_kinds().collect::<Vec<_>>(),
            [
                ImportKind::OpenApi,
                ImportKind::Insomnia,
                ImportKind::Http,
                ImportKind::Bruno
            ]
        );
        settings.importers.set_enabled(ImportKind::Postman, true);
        assert!(settings.importers.is_default());
    }

    /// R-SET-1: the nearest level that sets a value wins.
    #[test]
    fn request_settings_cascade() {
        let project = RequestSettings {
            timeout_ms: Some(5_000),
            ..RequestSettings::default()
        };
        let workspace = RequestSettings {
            timeout_ms: Some(10_000),
            follow_redirects: Some(false),
            ..RequestSettings::default()
        };
        let app = RequestSettings::from_app(&ClientSettings::default());
        assert_eq!(
            project.or(workspace).or(app),
            RequestSettings {
                timeout_ms: Some(5_000),
                follow_redirects: Some(false),
                cookies: Some(true),
            }
        );
        assert_eq!(
            toml::to_string(&workspace).expect("toml"),
            "timeout_ms = 10000\nfollow_redirects = false\n",
            "only what is set"
        );
        assert!(RequestSettings::default().is_empty());
    }

    /// R-SET-1: each value says which level it comes from.
    #[test]
    fn resolved_settings_know_their_level() {
        let project = RequestSettings {
            cookies: Some(false),
            ..RequestSettings::default()
        };
        let workspace = RequestSettings {
            timeout_ms: Some(9_000),
            cookies: Some(true),
            ..RequestSettings::default()
        };
        let r = ResolvedSettings::resolve(
            &[(Level::Project, project), (Level::Workspace, workspace)],
            &ClientSettings::default(),
        );
        assert_eq!(
            r.timeout_ms,
            Inherited {
                value: 9_000,
                from: Level::Workspace
            }
        );
        assert_eq!(
            r.cookies,
            Inherited {
                value: false,
                from: Level::Project
            }
        );
        assert_eq!(
            r.follow_redirects,
            Inherited {
                value: true,
                from: Level::App
            }
        );
    }

    /// R-SET-4..7: appearance and history are only written when changed, and come back.
    #[test]
    fn appearance_and_history_round_trip() {
        let mut settings = AppSettings::default();
        assert_eq!(settings.appearance.zoom(), 1.0);
        assert!(settings.history.on());
        settings.appearance = Appearance {
            theme: Theme::Dark,
            text_size: 125,
            wrap_lines: true,
            ..Appearance::default()
        };
        settings.history.size = 0;
        let text = toml::to_string(&settings).expect("toml");
        assert!(
            text.contains("[appearance]\ntheme = \"dark\"\ntext_size = 125\nwrap_lines = true\n"),
            "{text}"
        );
        assert!(text.contains("[history]\nsize = 0\n"), "{text}");
        let back: AppSettings = toml::from_str(&text).expect("read");
        assert_eq!(back, settings);
        assert!(!back.history.on());
        assert_eq!(Appearance::text_size_for(1.1), 110);
        assert_eq!(Appearance::text_size_for(9.0), 300);
        let silly = Appearance {
            text_size: 5,
            ..Appearance::default()
        };
        assert_eq!(silly.zoom(), 0.5);
    }

    /// R-SET-9..11, R-HTTP-20: new settings are only written when changed, come back,
    /// and files without them load with the defaults.
    #[test]
    fn editing_density_code_font_and_proxy_round_trip() {
        let defaults = toml::to_string(&AppSettings::default()).expect("toml");
        assert!(!defaults.contains("editing"), "{defaults}");
        assert!(!defaults.contains("density"), "{defaults}");
        assert!(!defaults.contains("layout"), "{defaults}");
        assert!(!defaults.contains("proxy"), "{defaults}");
        let back: AppSettings = toml::from_str("").expect("empty");
        assert!(back.editing.preview_tab && !back.editing.auto_save);
        assert_eq!(back.appearance.density, Density::Comfortable);
        assert_eq!(back.client.proxy, crate::ProxyMode::System);

        let mut settings = AppSettings {
            editing: EditingSettings {
                preview_tab: false,
                auto_save: true,
            },
            ..AppSettings::default()
        };
        settings.appearance.density = Density::Compact;
        settings.appearance.code_font = Some("/fonts/Fira Code ü😀.ttf".into());
        settings.appearance.code_size = 125;
        settings.appearance.layout = RequestLayout::SideBySide;
        settings.client.proxy = crate::ProxyMode::Manual;
        settings.client.proxy_url = "http://127.0.0.1:8080".into();
        settings.client.no_proxy = vec!["localhost".into(), "*.internal".into()];
        let text = toml::to_string(&settings).expect("toml");
        assert!(
            text.contains("proxy = \"manual\"\nproxy_url = \"http://127.0.0.1:8080\"\n"),
            "{text}"
        );
        assert!(
            text.contains("[editing]\npreview_tab = false\nauto_save = true\n"),
            "{text}"
        );
        assert!(text.contains("density = \"compact\"\n"), "{text}");
        assert!(text.contains("layout = \"side-by-side\"\n"), "{text}");
        let back: AppSettings = toml::from_str(&text).expect("read");
        assert_eq!(back, settings);
        assert!((back.appearance.code_scale() - 1.25).abs() < 1e-6);
        let tiny = Appearance {
            code_size: 1,
            ..Appearance::default()
        };
        assert!((tiny.code_scale() - 0.7).abs() < 1e-6, "clamped");
    }

    /// R-I18N-1: the language is only written when chosen, comes back, and one a newer
    /// version wrote loads as the system's.
    #[test]
    fn language_round_trips() {
        let defaults = toml::to_string(&AppSettings::default()).expect("toml");
        assert!(!defaults.contains("language"), "{defaults}");
        let mut settings = AppSettings::default();
        settings.appearance.language = Language::Danish;
        let text = toml::to_string(&settings).expect("toml");
        assert!(text.contains("language = \"da\"\n"), "{text}");
        let back: AppSettings = toml::from_str(&text).expect("read");
        assert_eq!(back, settings);
        let newer: AppSettings =
            toml::from_str("[appearance]\nlanguage = \"de\"\ntext_size = 125\n").expect("newer");
        assert_eq!(newer.appearance.language, Language::System);
        assert_eq!(newer.appearance.text_size, 125);
    }

    /// R-SET-10: font files are recognised by their first bytes.
    #[test]
    fn font_files_are_checked() {
        let path = std::path::Path::new("font.ttf");
        for ok in [
            &b"\x00\x01\x00\x00rest"[..],
            b"OTTOrest",
            b"truerest",
            b"ttcfrest",
        ] {
            assert!(check_font_file(path, ok).is_ok());
        }
        for bad in [&b""[..], b"\x00\x01", b"<html>", "ü😀".as_bytes()] {
            let err = check_font_file(path, bad)
                .expect_err("not a font")
                .to_string();
            assert!(err.contains("not a TrueType or OpenType font"), "{err}");
        }
    }

    /// R-SET-13: features are on by default, turned off by id, and an id a newer version
    /// wrote still loads.
    #[test]
    fn features_default_on_and_round_trip() {
        let mut settings = AppSettings::default();
        assert!(Feature::ALL.iter().all(|f| settings.features.enabled(*f)));
        settings.features.set_enabled(Feature::Workflows, false);
        settings.features.set_enabled(Feature::Docs, false);
        settings.features.set_enabled(Feature::Docs, true);
        let text = toml::to_string(&settings).expect("toml");
        assert!(
            text.contains("[features]\ndisabled = [\"workflows\"]\n"),
            "{text}"
        );
        let back: AppSettings = toml::from_str(&text).expect("read");
        assert!(!back.features.enabled(Feature::Workflows));
        assert!(back.features.enabled(Feature::Docs));
        let newer: AppSettings =
            toml::from_str("[features]\ndisabled = [\"graphql\"]\n").expect("newer");
        assert!(Feature::ALL.iter().all(|f| newer.features.enabled(*f)));
    }

    /// R-SET-15: the check at start-up is on by default, and only "off" is written.
    #[test]
    fn update_check_defaults_on_and_round_trips() {
        let mut settings = AppSettings::default();
        assert!(settings.updates.check_at_start);
        assert!(
            !toml::to_string(&settings)
                .expect("toml")
                .contains("updates")
        );
        settings.updates.check_at_start = false;
        let text = toml::to_string(&settings).expect("toml");
        assert!(
            text.contains("[updates]\ncheck_at_start = false\n"),
            "{text}"
        );
        let back: AppSettings = toml::from_str(&text).expect("read");
        assert_eq!(back, settings);
    }

    /// Settings saved before `AppSettings` existed (HTTP settings only) still load,
    /// and so do importers a newer version knows about.
    #[test]
    fn older_and_newer_files_load() {
        let old: AppSettings =
            toml::from_str("follow_redirects = false\ndefault_timeout_ms = 5000\n").expect("old");
        assert!(!old.client.follow_redirects);
        assert_eq!(old.client.default_timeout_ms, 5000);
        assert!(old.client.verify_tls, "missing settings keep their default");

        let newer: AppSettings =
            toml::from_str("[importers]\ndisabled = [\"insomnia\", \"http\"]\n").expect("newer");
        assert!(!newer.importers.enabled(ImportKind::Http));
        assert!(newer.importers.enabled(ImportKind::OpenApi));
    }
}
