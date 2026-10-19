//! Settings of the app itself, kept on this machine (R-LOC-1): the HTTP client's
//! (R-HTTP-7, R-HTTP-8, R-HTTP-16) and which importers are offered (R-COL-18).
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
