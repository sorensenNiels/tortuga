//! The requests a WFS service gets (R-GEO-1): ordinary requests on a project server.

use super::{Capabilities, FeatureType, Version};
use crate::collection::NewItem;
use crate::format::FolderDefaults;
use crate::model::{Auth, HttpMethod, KeyValue, Request};

/// How many features a generated GetFeature asks for.
pub const FIRST_FEATURES: u32 = 10;

/// The GeoJSON output format GeoServer offers for every layer.
pub const GEOJSON: &str = "application/json";

/// GetCapabilities on the endpoint as typed, before the project has its server: what
/// the dialog sends to list the layers.
pub fn capabilities_request(endpoint: &str, version: Version) -> Request {
    let mut request = Request::new(HttpMethod::Get, endpoint.trim());
    "GetCapabilities".clone_into(&mut request.name);
    request.params = service_params(version, "GetCapabilities");
    request
}

/// The folder for a service: GetCapabilities, then a folder per chosen feature type with
/// DescribeFeatureType and GetFeature, on the project server `server` at `path`. The
/// folder's requests use `auth` (`None`: what the folder inherits) (R-GEO-1).
pub fn service_folder(
    caps: &Capabilities,
    chosen: &[&FeatureType],
    version: Version,
    server: &str,
    path: &str,
    auth: Option<&Auth>,
) -> NewItem {
    let on_server = |name: &str, operation: &str, extra: Vec<KeyValue>| {
        let mut request = Request::new(HttpMethod::Get, path);
        name.clone_into(&mut request.name);
        request.server = Some(server.to_owned());
        request.params = service_params(version, operation);
        request.params.extend(extra);
        request
    };
    let mut items = vec![NewItem::Request(Box::new(on_server(
        "GetCapabilities",
        "GetCapabilities",
        Vec::new(),
    )))];
    for ft in chosen {
        let type_names = || KeyValue::new(version.type_names_param(), &ft.name);
        let describe = on_server(
            "DescribeFeatureType",
            "DescribeFeatureType",
            vec![type_names()],
        );
        let mut get = on_server(
            "GetFeature",
            "GetFeature",
            vec![
                type_names(),
                KeyValue::new(version.count_param(), FIRST_FEATURES.to_string()),
                KeyValue::new("outputFormat", GEOJSON),
            ],
        );
        get.description = format!(
            "The first {FIRST_FEATURES} features of `{}`, as GeoJSON.",
            ft.name
        );
        items.push(NewItem::Folder {
            name: ft.name.clone(),
            defaults: FolderDefaults {
                description: layer_description(ft),
                ..FolderDefaults::default()
            },
            items: vec![
                NewItem::Request(Box::new(describe)),
                NewItem::Request(Box::new(get)),
            ],
        });
    }
    NewItem::Folder {
        name: caps.title.clone(),
        defaults: FolderDefaults {
            auth: auth.cloned(),
            ..FolderDefaults::default()
        },
        items,
    }
}

fn service_params(version: Version, operation: &str) -> Vec<KeyValue> {
    vec![
        KeyValue::new("service", "WFS"),
        KeyValue::new("version", version.as_str()),
        KeyValue::new("request", operation),
    ]
}

/// The layer's title, abstract, CRS and bounds, for its folder's description.
fn layer_description(ft: &FeatureType) -> String {
    let mut lines = Vec::new();
    if !ft.title.is_empty() {
        lines.push(format!("**{}**", ft.title));
    }
    if !ft.abstract_text.is_empty() {
        lines.push(ft.abstract_text.clone());
    }
    if !ft.default_crs.is_empty() {
        lines.push(format!("Default CRS: `{}`", ft.default_crs));
    }
    if let Some([w, s, e, n]) = ft.wgs84_bounds {
        lines.push(format!(
            "WGS84 bounds: {w}, {s}, {e}, {n} (west, south, east, north)"
        ));
    }
    lines.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caps() -> Capabilities {
        Capabilities {
            title: "Maps ü😀".into(),
            version: Some(Version::V2_0_0),
            feature_types: vec![FeatureType {
                name: "topp:states".into(),
                title: "USA Population".into(),
                abstract_text: String::new(),
                default_crs: "EPSG:4326".into(),
                wgs84_bounds: Some([-124.7, 24.9, -66.9, 49.3]),
            }],
        }
    }

    fn query(r: &Request) -> String {
        r.params
            .iter()
            .map(|p| format!("{}={}", p.name, p.value))
            .collect::<Vec<_>>()
            .join("&")
    }

    /// R-GEO-1: the dialog's GetCapabilities goes to the endpoint as typed.
    #[test]
    fn capabilities_go_to_the_endpoint() {
        let r = capabilities_request(" {{gs}}/geoserver/wfs ", Version::V1_1_0);
        assert_eq!(r.url, "{{gs}}/geoserver/wfs");
        assert_eq!(r.server, None);
        assert_eq!(
            query(&r),
            "service=WFS&version=1.1.0&request=GetCapabilities"
        );
    }

    /// R-GEO-1: a folder per layer, the requests on the server, params per version.
    #[test]
    fn a_service_becomes_folders_of_requests() {
        let caps = caps();
        let chosen: Vec<_> = caps.feature_types.iter().collect();
        let NewItem::Folder { name, items, .. } = service_folder(
            &caps,
            &chosen,
            Version::V2_0_0,
            "geoserver",
            "/topp/wfs",
            None,
        ) else {
            panic!("a folder");
        };
        assert_eq!(name, "Maps ü😀");
        let NewItem::Request(first) = &items[0] else {
            panic!("GetCapabilities first");
        };
        assert_eq!(first.server.as_deref(), Some("geoserver"));
        assert_eq!(first.url, "/topp/wfs");
        let NewItem::Folder {
            name,
            defaults,
            items: layer,
        } = &items[1]
        else {
            panic!("a layer folder");
        };
        assert_eq!(name, "topp:states");
        assert!(defaults.description.contains("USA Population"));
        assert!(defaults.description.contains("-124.7"));
        let NewItem::Request(get) = &layer[1] else {
            panic!("GetFeature");
        };
        assert_eq!(
            query(get),
            "service=WFS&version=2.0.0&request=GetFeature&typeNames=topp:states&count=10&outputFormat=application/json"
        );

        let NewItem::Folder { items, .. } =
            service_folder(&caps, &chosen, Version::V1_0_0, "geoserver", "/wfs", None)
        else {
            panic!("a folder");
        };
        let NewItem::Folder { items: layer, .. } = &items[1] else {
            panic!("a layer folder");
        };
        let NewItem::Request(get) = &layer[1] else {
            panic!("GetFeature");
        };
        assert!(query(get).contains("typeName=topp:states&maxFeatures=10"));
    }

    #[test]
    fn no_layers_chosen_still_gives_get_capabilities() {
        let NewItem::Folder { items, .. } =
            service_folder(&caps(), &[], Version::V2_0_0, "gs", "", None)
        else {
            panic!("a folder");
        };
        assert_eq!(items.len(), 1);
    }

    /// R-GEO-1: the auth chosen in the dialog goes on the service folder, so every
    /// request in it sends what fetching the layers sent.
    #[test]
    fn the_chosen_auth_goes_on_the_service_folder() {
        let bearer = Auth::Bearer {
            token: "{{wfs_token}}".into(),
        };
        let NewItem::Folder { defaults, .. } =
            service_folder(&caps(), &[], Version::V2_0_0, "gs", "", Some(&bearer))
        else {
            panic!("a folder");
        };
        assert_eq!(defaults.auth, Some(bearer));
        let NewItem::Folder { defaults, .. } =
            service_folder(&caps(), &[], Version::V2_0_0, "gs", "", None)
        else {
            panic!("a folder");
        };
        assert_eq!(defaults.auth, None, "inherits");
    }
}
