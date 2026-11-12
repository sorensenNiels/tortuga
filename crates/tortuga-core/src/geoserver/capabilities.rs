//! A WFS GetCapabilities document (R-GEO-1): the service's title and its feature types,
//! read the same way for WFS 1.0.0, 1.1.0 and 2.0.0.

use super::Version;
use crate::xml::{Document, Kind, local};
use crate::{Error, Result};

/// What the dialog shows and creates requests from.
#[derive(Debug, Clone, PartialEq)]
pub struct Capabilities {
    /// The service's title, or "WFS" when it has none.
    pub title: String,
    /// The version the server answered with, if it is one Tortuga knows.
    pub version: Option<Version>,
    pub feature_types: Vec<FeatureType>,
}

/// A layer the service offers.
#[derive(Debug, Clone, PartialEq)]
pub struct FeatureType {
    /// The qualified name requests use, e.g. `topp:states`.
    pub name: String,
    pub title: String,
    pub abstract_text: String,
    /// `DefaultCRS` (2.0.0), `DefaultSRS` (1.1.0) or `SRS` (1.0.0).
    pub default_crs: String,
    /// Longitude/latitude bounds: west, south, east, north.
    pub wgs84_bounds: Option<[f64; 4]>,
}

/// Reads a capabilities document. An OGC exception report or any other body is an
/// error that says what it is.
pub fn parse(text: &str) -> Result<Capabilities> {
    let invalid = |m: String| Error::InvalidWfsCapabilities(m);
    if let Some(found) = crate::ogc::exceptions(text.as_bytes()) {
        let first = found.first().map(crate::ogc::Exception::summary);
        return Err(invalid(format!(
            "the server sent an exception report: {}",
            first.unwrap_or_default()
        )));
    }
    let doc = Document::parse(text).map_err(invalid)?;
    let root = doc
        .root()
        .ok_or_else(|| invalid("the body has no XML element".into()))?;
    let root_name = name(&doc, root).unwrap_or_default();
    if root_name != "WFS_Capabilities" {
        return Err(invalid(format!(
            "expected <WFS_Capabilities>, got <{root_name}>"
        )));
    }
    let x = Doc(&doc);
    let version = x.attribute(root, "version").and_then(Version::parse);
    // `ows:ServiceIdentification/ows:Title` (1.1.0, 2.0.0) or `Service/Title` (1.0.0).
    let title = ["ServiceIdentification", "Service"]
        .into_iter()
        .find_map(|s| x.child(root, s))
        .and_then(|s| x.child_text(s, "Title"))
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| "WFS".to_owned());
    let feature_types = x
        .child(root, "FeatureTypeList")
        .map(|list| {
            x.children(list, "FeatureType")
                .filter_map(|ft| x.feature_type(ft))
                .collect()
        })
        .unwrap_or_default();
    Ok(Capabilities {
        title,
        version,
        feature_types,
    })
}

/// An element's local name.
fn name(doc: &Document, node: usize) -> Option<&str> {
    match &doc.nodes.get(node)?.kind {
        Kind::Element(e) => Some(local(&e.name)),
        _ => None,
    }
}

/// Lookups by local name, ignoring namespace prefixes, which servers choose freely.
struct Doc<'a>(&'a Document);

impl Doc<'_> {
    fn children<'s>(
        &'s self,
        node: usize,
        local_name: &'s str,
    ) -> impl Iterator<Item = usize> + 's {
        self.0.nodes[node]
            .children
            .iter()
            .copied()
            .filter(move |&c| name(self.0, c) == Some(local_name))
    }

    fn child(&self, node: usize, local_name: &str) -> Option<usize> {
        self.children(node, local_name).next()
    }

    fn child_text(&self, node: usize, local_name: &str) -> Option<String> {
        self.child(node, local_name)
            .map(|c| self.0.string_value(c).trim().to_owned())
    }

    fn attribute(&self, node: usize, local_name: &str) -> Option<&str> {
        let Kind::Element(e) = &self.0.nodes.get(node)?.kind else {
            return None;
        };
        e.attributes
            .iter()
            .find(|(k, _)| local(k) == local_name)
            .map(|(_, v)| v.as_str())
    }

    fn feature_type(&self, node: usize) -> Option<FeatureType> {
        let name = self.child_text(node, "Name").filter(|n| !n.is_empty())?;
        let default_crs = ["DefaultCRS", "DefaultSRS", "SRS"]
            .into_iter()
            .find_map(|c| self.child_text(node, c))
            .unwrap_or_default();
        Some(FeatureType {
            title: self.child_text(node, "Title").unwrap_or_default(),
            abstract_text: self.child_text(node, "Abstract").unwrap_or_default(),
            default_crs,
            wgs84_bounds: self.bounds(node),
            name,
        })
    }

    /// `ows:WGS84BoundingBox` (1.1.0, 2.0.0) or `LatLongBoundingBox` (1.0.0).
    fn bounds(&self, node: usize) -> Option<[f64; 4]> {
        if let Some(b) = self.child(node, "WGS84BoundingBox") {
            let corner = |c: &str| -> Option<[f64; 2]> {
                let text = self.child_text(b, c)?;
                let mut numbers = text.split_whitespace().map(str::parse::<f64>);
                Some([numbers.next()?.ok()?, numbers.next()?.ok()?])
            };
            let [w, s] = corner("LowerCorner")?;
            let [e, n] = corner("UpperCorner")?;
            return Some([w, s, e, n]);
        }
        let b = self.child(node, "LatLongBoundingBox")?;
        let number = |a: &str| self.attribute(b, a)?.trim().parse::<f64>().ok();
        Some([
            number("minx")?,
            number("miny")?,
            number("maxx")?,
            number("maxy")?,
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const V2: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<wfs:WFS_Capabilities version="2.0.0" xmlns:wfs="http://www.opengis.net/wfs/2.0" xmlns:ows="http://www.opengis.net/ows/1.1">
  <ows:ServiceIdentification><ows:Title>GeoServer Web Feature Service</ows:Title></ows:ServiceIdentification>
  <FeatureTypeList>
    <FeatureType xmlns:topp="http://www.openplans.org/topp">
      <Name>topp:states</Name>
      <Title>USA Population</Title>
      <Abstract>States ü😀</Abstract>
      <DefaultCRS>urn:ogc:def:crs:EPSG::4326</DefaultCRS>
      <ows:WGS84BoundingBox>
        <ows:LowerCorner>-124.73 24.96</ows:LowerCorner>
        <ows:UpperCorner>-66.97 49.37</ows:UpperCorner>
      </ows:WGS84BoundingBox>
    </FeatureType>
    <FeatureType><Name>tiger:roads</Name><Title>Roads</Title></FeatureType>
    <FeatureType><Title>No name, left out</Title></FeatureType>
  </FeatureTypeList>
</wfs:WFS_Capabilities>"#;

    /// R-GEO-1: a 2.0.0 document gives the title, names, CRS and bounds.
    #[test]
    fn wfs_2_capabilities_are_read() {
        let caps = parse(V2).expect("capabilities");
        assert_eq!(caps.title, "GeoServer Web Feature Service");
        assert_eq!(caps.version, Some(Version::V2_0_0));
        assert_eq!(caps.feature_types.len(), 2);
        let states = &caps.feature_types[0];
        assert_eq!(states.name, "topp:states");
        assert_eq!(states.title, "USA Population");
        assert_eq!(states.abstract_text, "States ü😀");
        assert_eq!(states.default_crs, "urn:ogc:def:crs:EPSG::4326");
        assert_eq!(states.wgs84_bounds, Some([-124.73, 24.96, -66.97, 49.37]));
        assert_eq!(caps.feature_types[1].wgs84_bounds, None);
    }

    /// R-GEO-1: WFS 1.0.0 names things differently.
    #[test]
    fn wfs_1_0_capabilities_are_read() {
        let caps = parse(
            r#"<WFS_Capabilities version="1.0.0">
  <Service><Name>WFS</Name><Title>Old WFS</Title></Service>
  <FeatureTypeList><FeatureType>
    <Name>topp:states</Name><SRS>EPSG:4326</SRS>
    <LatLongBoundingBox minx="-124.7" miny="24.9" maxx="-66.9" maxy="49.3"/>
  </FeatureType></FeatureTypeList>
</WFS_Capabilities>"#,
        )
        .expect("capabilities");
        assert_eq!(caps.title, "Old WFS");
        assert_eq!(caps.version, Some(Version::V1_0_0));
        assert_eq!(caps.feature_types[0].default_crs, "EPSG:4326");
        assert_eq!(
            caps.feature_types[0].wgs84_bounds,
            Some([-124.7, 24.9, -66.9, 49.3])
        );
    }

    /// R-GEO-1: an exception report, another document or no XML is said to be so.
    #[test]
    fn other_bodies_are_errors() {
        let report = parse(
            r#"<ows:ExceptionReport><ows:Exception exceptionCode="NoApplicableCode">
<ows:ExceptionText>No such workspace</ows:ExceptionText></ows:Exception></ows:ExceptionReport>"#,
        )
        .expect_err("report");
        assert!(report.to_string().contains("No such workspace"), "{report}");
        let wms = parse("<WMS_Capabilities/>").expect_err("wms");
        assert!(wms.to_string().contains("<WMS_Capabilities>"), "{wms}");
        assert!(parse("").is_err());
        assert!(parse("{\"json\": true}").is_err());
    }

    #[test]
    fn a_service_without_title_or_layers_is_still_read() {
        let caps = parse(r#"<wfs:WFS_Capabilities version="9.9"/>"#).expect("capabilities");
        assert_eq!(caps.title, "WFS");
        assert_eq!(caps.version, None);
        assert_eq!(caps.feature_types, []);
    }
}
