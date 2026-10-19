//! Settings of the app itself, kept on this machine (R-LOC-1): the HTTP client's
//! (R-HTTP-7, R-HTTP-8, R-HTTP-16, R-SET-3), which importers are offered (R-COL-18),
//! appearance (R-SET-4, R-SET-5, R-SET-7) and history (R-SET-6); and the request
//! settings a workspace or project can override (R-SET-1, D39).
//!
//! Every setting has a default that suits most developers, and only settings that
//! differ from it are written, so a new setting never needs a migration.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::http::ClientSettings;
use crate::import::ImportKind;

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

/// How the app looks (R-SET-4, R-SET-5, R-SET-7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Appearance {
    pub theme: Theme,
    /// Size of the whole UI in percent (R-SET-5).
    pub text_size: u16,
    /// Wrap long lines in response bodies (R-SET-7).
    pub wrap_lines: bool,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            text_size: 100,
            wrap_lines: false,
        }
    }
}

impl Appearance {
    /// Text sizes egui's zoom keys can reach that make sense here, in percent.
    pub const TEXT_SIZES: std::ops::RangeInclusive<u16> = 50..=300;

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

impl Level {
    /// "the project", "the workspace" or "the app's settings", to say where a value comes from.
    pub fn label(self) -> &'static str {
        match self {
            Self::Project => "the project",
            Self::Workspace => "the workspace",
            Self::App => "the app's settings",
        }
    }
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
            [ImportKind::OpenApi, ImportKind::Http]
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
        assert_eq!(Level::App.label(), "the app's settings");
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
