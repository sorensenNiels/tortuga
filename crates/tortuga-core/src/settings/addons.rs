//! Add-ons (D78, P8, R-ADD-1): parts of the app beyond sending requests and reading
//! responses. Each is turned on or off in Settings → Add-ons (most are off by default)
//! and is not loaded while off.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::format::names_match;

/// An add-on of the app. The one list of them: the UI reaches an add-on only through
/// this enum (R-ADD-2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AddOn {
    /// The app's MCP server for AI coding agents (R-MCP-5, R-ADD-3).
    McpServer,
    /// Workflows: the sidebar's workflows, running a project or folder, and their
    /// commands (R-FLOW-1, R-ADD-4).
    Workflows,
    /// A project's mock server answering from its examples, and the "Mock" environment
    /// (R-MOCK-2, R-MOCK-4, R-ADD-5). On by default.
    MockServer,
    /// Creating WFS requests from a service's capabilities and the Features tab of
    /// GeoJSON responses (R-ADD-6, R-GEO-1, R-GEO-2).
    GeoServer,
}

impl AddOn {
    /// Every add-on, in the order Settings lists them.
    pub const ALL: [Self; 4] = [
        Self::Workflows,
        Self::MockServer,
        Self::McpServer,
        Self::GeoServer,
    ];

    /// The name the settings file keeps it under.
    pub fn id(self) -> &'static str {
        match self {
            Self::McpServer => "mcp-server",
            Self::Workflows => "workflows",
            Self::MockServer => "mock-server",
            Self::GeoServer => "geoserver",
        }
    }

    /// Whether it is on until turned off. Most add-ons are off by default (P8).
    pub fn on_by_default(self) -> bool {
        match self {
            Self::MockServer => true,
            Self::McpServer | Self::Workflows | Self::GeoServer => false,
        }
    }
}

/// Which add-ons are on (R-ADD-1). Only what differs from each add-on's default is
/// kept, so an add-on added in a later version starts with its own default.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AddOnSettings {
    /// Ids of the add-ons turned on ([`AddOn::id`]). Kept as text, so a file written by a
    /// newer version with add-ons this one doesn't know still loads.
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    pub enabled: BTreeSet<String>,
    /// Ids of the add-ons that are on by default ([`AddOn::on_by_default`]) and turned off.
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    pub disabled: BTreeSet<String>,
}

impl AddOnSettings {
    pub fn is_default(&self) -> bool {
        self.enabled.is_empty() && self.disabled.is_empty()
    }

    pub fn enabled(&self, addon: AddOn) -> bool {
        if addon.on_by_default() {
            !self.disabled.contains(addon.id())
        } else {
            self.enabled.contains(addon.id())
        }
    }

    pub fn set_enabled(&mut self, addon: AddOn, enabled: bool) {
        let (differs, other) = if addon.on_by_default() {
            (&mut self.disabled, &mut self.enabled)
        } else {
            (&mut self.enabled, &mut self.disabled)
        };
        other.remove(addon.id());
        if enabled == addon.on_by_default() {
            differs.remove(addon.id());
        } else {
            differs.insert(addon.id().to_owned());
        }
    }
}

/// Where the Features tab puts the shape preview (R-GEO-2): the user's choice, changed in
/// the tab and kept like the other settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FeaturesLayout {
    /// Above the table, the whole width: fits a narrow response panel.
    #[default]
    Above,
    /// Beside the table, on the right.
    Beside,
}

/// The GeoServer add-on's settings (R-ADD-6).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct GeoServerSettings {
    pub features_layout: FeaturesLayout,
    /// The name of the chosen map background (R-GEO-3, D83). Empty, or a name no longer
    /// listed: OpenStreetMap.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub basemap: String,
    /// The backgrounds the user added, besides [`OPENSTREETMAP`].
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub basemaps: Vec<Basemap>,
}

/// A map background: tiles from a URL with `{z}`, `{x}` and `{y}` (R-GEO-3).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Basemap {
    pub name: String,
    pub url: String,
}

/// The background every list offers; its tiles are fetched only once it is chosen (D82).
pub const OPENSTREETMAP: (&str, &str) = (
    "OpenStreetMap",
    "https://tile.openstreetmap.org/{z}/{x}/{y}.png",
);

impl GeoServerSettings {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// Every background to choose from: OpenStreetMap, then the user's.
    pub fn backgrounds(&self) -> Vec<Basemap> {
        let (name, url) = OPENSTREETMAP;
        let mut all = vec![Basemap {
            name: name.to_owned(),
            url: url.to_owned(),
        }];
        all.extend(self.basemaps.iter().cloned());
        all
    }

    /// The chosen background: OpenStreetMap unless another is chosen (D83).
    pub fn chosen_basemap(&self) -> Basemap {
        let mut all = self.backgrounds();
        let i = all
            .iter()
            .position(|b| names_match(&b.name, &self.basemap))
            .unwrap_or(0);
        all.swap_remove(i)
    }

    /// Whether a background is called `name` already.
    pub fn has_basemap(&self, name: &str) -> bool {
        self.backgrounds()
            .iter()
            .any(|b| names_match(&b.name, name))
    }

    /// Adds `basemap` and chooses it; one of the same name is replaced, except
    /// OpenStreetMap's.
    pub fn add_basemap(&mut self, basemap: &Basemap) {
        let basemap = Basemap {
            name: basemap.name.trim().to_owned(),
            url: basemap.url.trim().to_owned(),
        };
        if basemap.name.is_empty() || names_match(&basemap.name, OPENSTREETMAP.0) {
            return;
        }
        self.basemaps
            .retain(|b| !names_match(&b.name, &basemap.name));
        self.basemap.clone_from(&basemap.name);
        self.basemaps.push(basemap);
    }

    /// Removes the user's background `name`; if it was chosen, OpenStreetMap is.
    pub fn remove_basemap(&mut self, name: &str) {
        self.basemaps.retain(|b| !names_match(&b.name, name));
        if names_match(&self.basemap, name) {
            self.basemap.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AppSettings;

    /// R-GEO-2: the Features layout is written only when it is not the default.
    #[test]
    fn the_features_layout_round_trips() {
        let mut settings = AppSettings::default();
        assert!(
            !toml::to_string(&settings)
                .expect("serialize")
                .contains("geoserver")
        );
        settings.geoserver.features_layout = FeaturesLayout::Beside;
        let text = toml::to_string(&settings).expect("serialize");
        assert!(
            text.contains("[geoserver]\nfeatures_layout = \"beside\""),
            "{text}"
        );
        let back: AppSettings = toml::from_str(&text).expect("parse");
        assert_eq!(back.geoserver.features_layout, FeaturesLayout::Beside);
    }

    /// R-GEO-3: backgrounds are written once added or chosen, and read back.
    #[test]
    fn backgrounds_round_trip() {
        let mut settings = AppSettings::default();
        settings.geoserver.add_basemap(&Basemap {
            name: " Ortofoto ü😀 ".into(),
            url: "https://tiles.test/ü😀/{z}/{x}/{y}.png".into(),
        });
        let text = toml::to_string(&settings).expect("serialize");
        assert!(text.contains("basemap = \"Ortofoto ü😀\""), "{text}");
        let back: AppSettings = toml::from_str(&text).expect("parse");
        assert_eq!(back.geoserver, settings.geoserver);
        // Older settings without them still load, with none chosen.
        let old: AppSettings =
            toml::from_str("[geoserver]\nfeatures_layout = \"beside\"\n").expect("parse");
        assert_eq!(old.geoserver.chosen_basemap().name, OPENSTREETMAP.0);
    }

    /// R-GEO-3, D83: OpenStreetMap is always offered, and chosen unless another is; added
    /// backgrounds are chosen, replace one of the same name, and go when removed.
    #[test]
    fn backgrounds_are_chosen_added_and_removed() {
        let mut g = GeoServerSettings::default();
        assert_eq!(g.chosen_basemap().url, OPENSTREETMAP.1, "the default");
        assert_eq!(g.backgrounds().len(), 1);
        g.basemap = "gone".into();
        assert_eq!(g.chosen_basemap().name, OPENSTREETMAP.0, "a removed one");

        let ortho = |url: &str| Basemap {
            name: "Ortho".into(),
            url: url.into(),
        };
        g.add_basemap(&ortho("https://a.test/{z}/{x}/{y}"));
        g.add_basemap(&ortho("https://b.test/{z}/{x}/{y}"));
        assert_eq!(g.basemaps.len(), 1, "replaced");
        assert_eq!(g.chosen_basemap(), ortho("https://b.test/{z}/{x}/{y}"));
        assert!(g.has_basemap(" ORTHO "));

        g.add_basemap(&Basemap {
            name: "OpenStreetMap".into(),
            url: "https://evil.test/{z}/{x}/{y}".into(),
        });
        assert_eq!(g.basemaps.len(), 1, "the built-in one isn't replaced");

        g.remove_basemap("ortho");
        assert_eq!(g.chosen_basemap().name, OPENSTREETMAP.0);
        assert_eq!(g.basemaps, []);
    }

    /// R-ADD-1: add-ons have their own default (the mock server on, the rest off); only a
    /// change from it is written.
    #[test]
    fn addons_have_their_default_and_round_trip() {
        let mut settings = AppSettings::default();
        for addon in AddOn::ALL {
            assert_eq!(settings.addons.enabled(addon), addon.on_by_default());
        }
        assert!(settings.addons.enabled(AddOn::MockServer));
        assert!(!settings.addons.enabled(AddOn::McpServer));
        let text = toml::to_string(&settings).expect("serialize");
        assert!(!text.contains("addons"), "nothing while all are off");

        settings.addons.set_enabled(AddOn::McpServer, true);
        let text = toml::to_string(&settings).expect("serialize");
        assert!(
            text.contains("[addons]\nenabled = [\"mcp-server\"]"),
            "{text}"
        );
        let back: AppSettings = toml::from_str(&text).expect("parse");
        assert!(back.addons.enabled(AddOn::McpServer));

        settings.addons.set_enabled(AddOn::McpServer, false);
        assert!(settings.addons.is_default());

        // R-ADD-5: the mock server is on until turned off.
        settings.addons.set_enabled(AddOn::MockServer, true);
        assert!(settings.addons.is_default(), "on is its default");
        settings.addons.set_enabled(AddOn::MockServer, false);
        let text = toml::to_string(&settings).expect("serialize");
        assert!(
            text.contains("[addons]\ndisabled = [\"mock-server\"]"),
            "{text}"
        );
        let back: AppSettings = toml::from_str(&text).expect("parse");
        assert!(!back.addons.enabled(AddOn::MockServer));
        settings.addons.set_enabled(AddOn::MockServer, true);
        assert!(settings.addons.is_default());
    }

    /// R-ADD-1: an add-on only a newer version knows still loads, and stays on.
    #[test]
    fn unknown_addons_still_load() {
        let back: AppSettings =
            toml::from_str("[addons]\nenabled = [\"geoserver\", \"mcp-server\"]").expect("parse");
        assert!(back.addons.enabled(AddOn::McpServer));
        assert!(back.addons.enabled.contains("geoserver"));
    }
}
