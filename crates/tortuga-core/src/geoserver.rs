//! The GeoServer add-on's engine (R-ADD-6, D80): WFS capabilities, the requests made
//! from them (R-GEO-1), and the features of a GeoJSON response (R-GEO-2).
//!
//! What it creates are ordinary requests on a project server, so they work without the
//! add-on and in `tortuga-cli`.

pub mod capabilities;
pub mod features;
pub mod map;
pub mod requests;

use crate::collection::Collection;
use crate::format::{self, Environment};
use crate::storage::Storage;
use crate::{Error, Result};

pub use capabilities::{Capabilities, FeatureType};
pub use features::{Bounds, Feature, Features, Shape};
pub use requests::{capabilities_request, service_folder};

/// The server a new WFS service uses unless another name is given.
pub const DEFAULT_SERVER: &str = "GeoServer";

/// The environment created for a new server's URL when the project has none.
pub const DEFAULT_ENVIRONMENT: &str = "Default";

/// A WFS version, which decides the parameters' names (R-GEO-1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Version {
    #[default]
    V2_0_0,
    V1_1_0,
    V1_0_0,
}

impl Version {
    /// Every version, newest first, as the dialog offers them.
    pub const ALL: [Self; 3] = [Self::V2_0_0, Self::V1_1_0, Self::V1_0_0];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::V2_0_0 => "2.0.0",
            Self::V1_1_0 => "1.1.0",
            Self::V1_0_0 => "1.0.0",
        }
    }

    /// The version called `text`, e.g. from a capabilities document's `version`.
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|v| v.as_str() == text.trim())
    }

    /// The parameter naming the feature types: `typeNames` from 2.0.0 on.
    pub fn type_names_param(self) -> &'static str {
        match self {
            Self::V2_0_0 => "typeNames",
            Self::V1_1_0 | Self::V1_0_0 => "typeName",
        }
    }

    /// The parameter limiting the number of features: `count` from 2.0.0 on.
    pub fn count_param(self) -> &'static str {
        match self {
            Self::V2_0_0 => "count",
            Self::V1_1_0 | Self::V1_0_0 => "maxFeatures",
        }
    }
}

/// A WFS endpoint split into the project server's base URL and the requests' path
/// (R-GEO-1, D81). By default the base ends at a `/geoserver` segment, so the same server
/// serves WMS and the REST API too; without one, at the host. The dialog can move the
/// split to any `/` in the path ([`Endpoint::splits`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    pub base: String,
    pub path: String,
}

/// Whether the last segment of `base` is `geoserver`.
fn ends_in_geoserver(base: &str) -> bool {
    base.rsplit('/')
        .next()
        .is_some_and(|last| last.eq_ignore_ascii_case("geoserver"))
}

impl Endpoint {
    /// The default split of `endpoint`; a query string (`?service=WFS&…`) pasted with it
    /// is dropped. `None` when it is empty.
    pub fn parse(endpoint: &str) -> Option<Self> {
        let (splits, host) = Self::split_points(endpoint)?;
        let at_geoserver = splits
            .iter()
            .find(|s| s.base.len() > host && ends_in_geoserver(&s.base));
        at_geoserver.or(splits.first()).cloned()
    }

    /// Every way to split `endpoint`: after the host, after each segment of its path,
    /// and the whole endpoint with no path of its own. Empty when it is empty.
    pub fn splits(endpoint: &str) -> Vec<Self> {
        Self::split_points(endpoint).map_or_else(Vec::new, |(splits, _)| splits)
    }

    /// The splits, and where the host ends.
    fn split_points(endpoint: &str) -> Option<(Vec<Self>, usize)> {
        let endpoint = endpoint.trim();
        let endpoint = endpoint
            .split_once(['?', '#'])
            .map_or(endpoint, |(before, _)| before)
            .trim_end_matches('/');
        if endpoint.is_empty() {
            return None;
        }
        // Path segments start after `scheme://host`.
        let path_start = endpoint.find("://").map_or(0, |i| i + 3);
        let host_end = endpoint
            .get(path_start..)
            .and_then(|rest| rest.find('/'))
            .map_or(endpoint.len(), |i| path_start + i);
        let mut ends = vec![host_end];
        if let Some(path) = endpoint.get(host_end..) {
            // The first `/` is where the host ends.
            ends.extend(path.match_indices('/').skip(1).map(|(i, _)| host_end + i));
        }
        ends.push(endpoint.len());
        ends.dedup();
        let splits = ends
            .into_iter()
            .filter_map(|end| {
                let (base, rest) = (endpoint.get(..end)?, endpoint.get(end..)?);
                let path = if rest.is_empty() && end > host_end && ends_in_geoserver(base) {
                    // The endpoint `…/geoserver` alone means its WFS.
                    "/wfs"
                } else {
                    rest
                };
                (!base.is_empty()).then(|| Self {
                    base: base.to_owned(),
                    path: path.to_owned(),
                })
            })
            .collect();
        Some((splits, host_end))
    }

    /// The whole endpoint again.
    pub fn url(&self) -> String {
        format!("{}{}", self.base, self.path)
    }
}

/// The id of the project's server called `name`, added when it has none, with `base` as
/// its URL in the chosen environment `environment` (a name, as environments are chosen
/// for the whole workspace) (R-GEO-1). A project without an environment of that name
/// gets one, holding just this URL: it overrides the workspace's environment for this
/// project only. Without a chosen environment, a project with no environments gets one
/// called "Default"; otherwise every environment that has no URL for the server gets
/// `base`.
pub fn use_server(
    collection: &Collection,
    storage: &dyn Storage,
    name: &str,
    base: &str,
    environment: Option<&str>,
) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::InvalidOperation("the server needs a name".into()));
    }
    let mut manifest = collection.manifest.clone();
    let id = if let Some(s) = manifest.servers.iter().find(|s| s.is_named(name)) {
        s.id.clone()
    } else {
        let id = Collection::new_server_id(name, &manifest.servers);
        manifest.servers.push(format::Server {
            id: id.clone(),
            name: name.to_owned(),
        });
        id
    };
    let mut environments = collection.environments.clone();
    if let Some(chosen) = environment.map(str::trim).filter(|n| !n.is_empty()) {
        let env = match environments.iter().position(|e| e.is_named(chosen)) {
            Some(i) => &mut environments[i],
            None => Collection::add_environment(&mut environments, chosen),
        };
        env.servers.insert(id.clone(), base.to_owned());
    } else {
        if environments.is_empty() {
            Collection::add_environment(&mut environments, DEFAULT_ENVIRONMENT);
        }
        for env in &mut environments {
            if env.servers.get(&id).is_none_or(|url| url.trim().is_empty()) {
                env.servers.insert(id.clone(), base.to_owned());
            }
        }
    }
    collection.save_config(storage, &manifest, &environments)?;
    Ok(id)
}

/// The base URL the project's server called `name` has in `env`, the environment the
/// project's requests use now, if any: the dialog says what Create will change (R-GEO-1).
pub fn server_url<'a>(
    collection: &Collection,
    name: &str,
    env: Option<&'a Environment>,
) -> Option<&'a str> {
    let server = collection
        .manifest
        .servers
        .iter()
        .find(|s| s.is_named(name))?;
    env?.servers
        .get(&server.id)
        .map(String::as_str)
        .filter(|u| !u.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::FsStorage;

    /// R-GEO-1: the server's base ends at `/geoserver`; virtual services keep their
    /// workspace in the path.
    #[test]
    fn endpoints_split_at_geoserver() {
        let split = |e: &str| Endpoint::parse(e).map(|e| (e.base, e.path));
        assert_eq!(
            split("https://maps.test/geoserver/wfs"),
            Some(("https://maps.test/geoserver".into(), "/wfs".into()))
        );
        assert_eq!(
            split(" https://maps.test/geoserver/topp/wfs?service=WFS&request=GetCapabilities "),
            Some(("https://maps.test/geoserver".into(), "/topp/wfs".into()))
        );
        assert_eq!(
            split("http://localhost:8080/geoserver/"),
            Some(("http://localhost:8080/geoserver".into(), "/wfs".into()))
        );
        // D81: without `/geoserver`, the server is the host.
        assert_eq!(
            split("https://kort.test/wfs/rk/ows"),
            Some(("https://kort.test".into(), "/wfs/rk/ows".into()))
        );
        // Only a whole segment counts, and not the host.
        assert_eq!(
            split("https://geoserver.test/mygeoserver/wfs"),
            Some(("https://geoserver.test".into(), "/mygeoserver/wfs".into()))
        );
        assert_eq!(
            split("https://geoserver"),
            Some(("https://geoserver".into(), String::new()))
        );
        assert_eq!(
            split("{{gs}}/geoserver/ü😀/wfs"),
            Some(("{{gs}}/geoserver".into(), "/ü😀/wfs".into()))
        );
        assert_eq!(split("  "), None);
        assert_eq!(split("?x=1"), None);
    }

    /// D81: the split can be after the host, after any segment, or the whole endpoint.
    #[test]
    fn an_endpoint_can_be_split_at_any_slash() {
        let splits: Vec<(String, String)> = Endpoint::splits("https://kort.test/wfs/ü😀/ows/?x=1")
            .into_iter()
            .map(|e| (e.base, e.path))
            .collect();
        assert_eq!(
            splits,
            [
                ("https://kort.test".into(), "/wfs/ü😀/ows".into()),
                ("https://kort.test/wfs".into(), "/ü😀/ows".into()),
                ("https://kort.test/wfs/ü😀".into(), "/ows".into()),
                ("https://kort.test/wfs/ü😀/ows".into(), String::new()),
            ]
        );
        for e in Endpoint::splits("https://kort.test/wfs/ü😀/ows") {
            assert_eq!(e.url(), "https://kort.test/wfs/ü😀/ows");
        }
        assert_eq!(Endpoint::splits("https://kort.test").len(), 1);
        assert_eq!(Endpoint::splits(" "), []);
    }

    #[test]
    fn versions_name_their_params() {
        assert_eq!(Version::parse("2.0.0"), Some(Version::V2_0_0));
        assert_eq!(Version::parse("1.3.0"), None);
        assert_eq!(Version::V2_0_0.type_names_param(), "typeNames");
        assert_eq!(Version::V1_1_0.type_names_param(), "typeName");
        assert_eq!(Version::V2_0_0.count_param(), "count");
        assert_eq!(Version::V1_0_0.count_param(), "maxFeatures");
    }

    fn project() -> (tempfile::TempDir, Collection) {
        let dir = tempfile::tempdir().expect("tempdir");
        let c = Collection::create(&FsStorage, dir.path(), "Maps").expect("create");
        (dir, c)
    }

    /// R-GEO-1: a project without environments gets "Default" with the server's URL.
    #[test]
    fn a_new_server_gets_a_default_environment() {
        let (dir, c) = project();
        let id = use_server(
            &c,
            &FsStorage,
            "GeoServer",
            "https://maps.test/geoserver",
            None,
        )
        .expect("server");
        let c = Collection::open(&FsStorage, dir.path()).expect("open");
        assert_eq!(c.manifest.servers[0].name, "GeoServer");
        assert_eq!(c.environments.len(), 1);
        assert_eq!(c.environments[0].name, DEFAULT_ENVIRONMENT);
        assert_eq!(
            c.environments[0].servers.get(&id).map(String::as_str),
            Some("https://maps.test/geoserver")
        );
        assert_eq!(
            server_url(&c, "geoserver", c.environments.first()),
            Some("https://maps.test/geoserver")
        );
        assert_eq!(server_url(&c, "geoserver", None), None);
    }

    /// R-GEO-1: an existing server is reused; only the active environment changes.
    #[test]
    fn an_existing_server_gets_the_url_in_the_active_environment() {
        let (dir, c) = project();
        let mut envs = c.environments.clone();
        Collection::add_environment(&mut envs, "Dev");
        Collection::add_environment(&mut envs, "Prod");
        c.save_config(&FsStorage, &c.manifest, &envs).expect("save");
        let c = Collection::open(&FsStorage, dir.path()).expect("open");
        let first = use_server(
            &c,
            &FsStorage,
            "GeoServer",
            "https://dev.test/geoserver",
            Some("Dev"),
        )
        .expect("server");
        let c = Collection::open(&FsStorage, dir.path()).expect("open");
        let again = use_server(
            &c,
            &FsStorage,
            " geoserver ",
            "https://dev2.test/geoserver",
            Some("Dev"),
        )
        .expect("server");
        assert_eq!(first, again);
        let c = Collection::open(&FsStorage, dir.path()).expect("open");
        assert_eq!(c.manifest.servers.len(), 1);
        let dev = c.environment("dev").expect("dev");
        assert_eq!(
            dev.servers.get(&first).map(String::as_str),
            Some("https://dev2.test/geoserver")
        );
        assert!(c.environment("prod").expect("prod").servers.is_empty());

        // No environment chosen: each one without a URL gets it.
        use_server(
            &c,
            &FsStorage,
            "GeoServer",
            "https://other.test/geoserver",
            None,
        )
        .expect("server");
        let c = Collection::open(&FsStorage, dir.path()).expect("open");
        assert_eq!(
            c.environment("dev")
                .and_then(|e| e.servers.get(&first))
                .map(String::as_str),
            Some("https://dev2.test/geoserver")
        );
        assert_eq!(
            c.environment("prod")
                .and_then(|e| e.servers.get(&first))
                .map(String::as_str),
            Some("https://other.test/geoserver")
        );
        assert!(use_server(&c, &FsStorage, "  ", "x", None).is_err());
    }

    /// R-GEO-1: the chosen environment comes from the workspace and the project has none
    /// of that name: the project gets its own, with just the server's URL, and no
    /// "Default".
    #[test]
    fn a_workspace_environment_gets_a_project_environment_of_its_name() {
        let (dir, c) = project();
        let mut envs = c.environments.clone();
        Collection::add_environment(&mut envs, "Dev");
        c.save_config(&FsStorage, &c.manifest, &envs).expect("save");
        let c = Collection::open(&FsStorage, dir.path()).expect("open");
        let id = use_server(
            &c,
            &FsStorage,
            "GeoServer",
            "https://kort.test/wfs/rk/ows",
            Some("Prøve ü😀"),
        )
        .expect("server");
        let c = Collection::open(&FsStorage, dir.path()).expect("open");
        let names: Vec<&str> = c.environments.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["Dev", "Prøve ü😀"]);
        let own = &c.environments[1];
        assert_eq!(
            own.servers.get(&id).map(String::as_str),
            Some("https://kort.test/wfs/rk/ows")
        );
        assert!(
            own.variables.is_empty(),
            "the rest comes from the workspace"
        );
        assert!(c.environments[0].servers.is_empty());
    }
}
