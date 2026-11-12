//! The features of a GeoJSON response, ready for the Features tab (R-GEO-2): a table of
//! properties and the shapes to draw.

use serde_json::{Map, Value};

/// How far into the body a GeoJSON `type` must show, for the cheap check.
const LOOK_AHEAD: usize = 2048;

/// A coordinate pair as the body has it: `[x, y]`, which is longitude/latitude in plain
/// GeoJSON but latitude/longitude for some CRSs (see [`Features::lat_lon`]).
pub type Point = [f64; 2];

/// A geometry, flattened to what is drawn.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    Point(Point),
    Line(Vec<Point>),
    /// The outer ring, then any holes.
    Polygon(Vec<Vec<Point>>),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    pub min: Point,
    pub max: Point,
}

impl Bounds {
    fn around(points: impl IntoIterator<Item = Point>) -> Option<Self> {
        let mut out: Option<Self> = None;
        for [x, y] in points {
            if !x.is_finite() || !y.is_finite() {
                continue;
            }
            let b = out.get_or_insert(Self {
                min: [x, y],
                max: [x, y],
            });
            b.min = [b.min[0].min(x), b.min[1].min(y)];
            b.max = [b.max[0].max(x), b.max[1].max(y)];
        }
        out
    }
}

/// A feature: its id, geometry and properties as text, one per column.
#[derive(Debug, Clone, PartialEq)]
pub struct Feature {
    pub id: Option<String>,
    /// `Point`, `MultiPolygon`, … or `None` without a geometry.
    pub geometry_type: Option<String>,
    pub shapes: Vec<Shape>,
    /// Aligned with [`Features::columns`]; empty for a property the feature lacks.
    pub properties: Vec<String>,
}

/// A GeoJSON `FeatureCollection` (or one `Feature`) as the Features tab shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct Features {
    pub features: Vec<Feature>,
    /// Every property name, in the order they first appear.
    pub columns: Vec<String>,
    /// `numberMatched` or GeoServer's `totalFeatures`, when the server says.
    pub matched: Option<u64>,
    /// The CRS named in the body (GeoServer's `crs.properties.name`).
    pub crs: Option<String>,
    /// Around every coordinate, as the body has them.
    pub bounds: Option<Bounds>,
}

impl Features {
    /// Whether the coordinates are latitude first: GeoServer writes `urn:ogc:def:crs:EPSG::4326`
    /// GeoJSON from WFS 1.1.0 and 2.0.0 in that order, so the preview swaps them to draw
    /// north up (R-GEO-2).
    pub fn lat_lon(&self) -> bool {
        self.crs.as_deref().is_some_and(|crs| {
            let crs = crs.trim().to_ascii_lowercase();
            crs == "urn:ogc:def:crs:epsg::4326" || crs == "urn:x-ogc:def:crs:epsg:4326"
        })
    }
}

/// Whether `body` may be GeoJSON features, from its start only: cheap enough to ask of
/// every JSON response.
pub fn looks_like_features(body: &[u8]) -> bool {
    let start = &body[..body.len().min(LOOK_AHEAD)];
    let has = |needle: &[u8]| start.windows(needle.len()).any(|w| w == needle);
    start.iter().find(|b| !b.is_ascii_whitespace()) == Some(&b'{')
        && (has(b"\"FeatureCollection\"") || has(b"\"Feature\""))
}

/// The features of a parsed GeoJSON body; `None` when it is not a `FeatureCollection`
/// or `Feature`.
pub fn read(value: &Value) -> Option<Features> {
    let object = value.as_object()?;
    let list: Vec<&Map<String, Value>> = match object.get("type")?.as_str()? {
        "FeatureCollection" => object
            .get("features")?
            .as_array()?
            .iter()
            .filter_map(Value::as_object)
            .collect(),
        "Feature" => vec![object],
        _ => return None,
    };
    let mut columns: Vec<String> = Vec::new();
    for f in &list {
        if let Some(props) = f.get("properties").and_then(Value::as_object) {
            for key in props.keys() {
                if !columns.contains(key) {
                    columns.push(key.clone());
                }
            }
        }
    }
    let features: Vec<Feature> = list.iter().map(|f| feature(f, &columns)).collect();
    let bounds = Bounds::around(
        features
            .iter()
            .flat_map(|f| f.shapes.iter())
            .flat_map(points),
    );
    let matched = ["numberMatched", "totalFeatures"]
        .into_iter()
        .find_map(|k| object.get(k)?.as_u64());
    let crs = object
        .get("crs")
        .and_then(|c| c.get("properties")?.get("name")?.as_str())
        .map(str::to_owned);
    Some(Features {
        features,
        columns,
        matched,
        crs,
        bounds,
    })
}

fn feature(f: &Map<String, Value>, columns: &[String]) -> Feature {
    let geometry = f.get("geometry").filter(|g| !g.is_null());
    let mut shapes = Vec::new();
    if let Some(g) = geometry {
        add_shapes(g, &mut shapes, 0);
    }
    let props = f.get("properties").and_then(Value::as_object);
    Feature {
        id: f.get("id").map(text),
        geometry_type: geometry
            .and_then(|g| g.get("type")?.as_str())
            .map(str::to_owned),
        shapes,
        properties: columns
            .iter()
            .map(|c| props.and_then(|p| p.get(c)).map(text).unwrap_or_default())
            .collect(),
    }
}

/// A property value as the table shows it: text as is, `null` empty, the rest as JSON.
fn text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// How deep `GeometryCollection`s may nest; deeper ones are not drawn.
const MAX_DEPTH: usize = 8;

fn add_shapes(geometry: &Value, out: &mut Vec<Shape>, depth: usize) {
    if depth > MAX_DEPTH {
        return;
    }
    let coords = geometry.get("coordinates");
    let kind = geometry.get("type").and_then(Value::as_str).unwrap_or("");
    match kind {
        "Point" => out.extend(coords.and_then(point).map(Shape::Point)),
        "MultiPoint" => out.extend(list(coords, point).into_iter().map(Shape::Point)),
        "LineString" => out.extend(line(coords).map(Shape::Line)),
        "MultiLineString" => {
            out.extend(list(coords, |c| line(Some(c))).into_iter().map(Shape::Line));
        }
        "Polygon" => out.extend(polygon(coords).map(Shape::Polygon)),
        "MultiPolygon" => out.extend(
            list(coords, |c| polygon(Some(c)))
                .into_iter()
                .map(Shape::Polygon),
        ),
        "GeometryCollection" => {
            for g in geometry
                .get("geometries")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                add_shapes(g, out, depth + 1);
            }
        }
        _ => {}
    }
}

fn point(value: &Value) -> Option<Point> {
    let a = value.as_array()?;
    Some([a.first()?.as_f64()?, a.get(1)?.as_f64()?])
}

fn list<T>(value: Option<&Value>, each: impl Fn(&Value) -> Option<T>) -> Vec<T> {
    value
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(each).collect())
        .unwrap_or_default()
}

fn line(value: Option<&Value>) -> Option<Vec<Point>> {
    let points = list(value, point);
    (!points.is_empty()).then_some(points)
}

fn polygon(value: Option<&Value>) -> Option<Vec<Vec<Point>>> {
    let rings = list(value, |r| line(Some(r)));
    (!rings.is_empty()).then_some(rings)
}

/// Every coordinate of a shape.
pub fn points(shape: &Shape) -> Box<dyn Iterator<Item = Point> + '_> {
    match shape {
        Shape::Point(p) => Box::new(std::iter::once(*p)),
        Shape::Line(l) => Box::new(l.iter().copied()),
        Shape::Polygon(rings) => Box::new(rings.iter().flatten().copied()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn collection() -> Value {
        json!({
            "type": "FeatureCollection",
            "totalFeatures": 49,
            "numberReturned": 3,
            "crs": {"type": "name", "properties": {"name": "urn:ogc:def:crs:EPSG::4326"}},
            "features": [
                {"type": "Feature", "id": "states.1",
                 "geometry": {"type": "MultiPolygon", "coordinates": [[[[0, 0], [4, 0], [4, 4], [0, 0]], [[1, 1], [2, 1], [1, 2], [1, 1]]]]},
                 "properties": {"name": "ü😀", "pop": 12, "note": null}},
                {"type": "Feature", "id": 2,
                 "geometry": {"type": "GeometryCollection", "geometries": [
                     {"type": "Point", "coordinates": [-3, 9]},
                     {"type": "LineString", "coordinates": [[1, 1], [2, 2]]}]},
                 "properties": {"extra": {"a": [1]}}},
                {"type": "Feature", "geometry": null, "properties": null}
            ]
        })
    }

    /// R-GEO-2: features, columns in order, counts, CRS and bounds.
    #[test]
    fn a_feature_collection_is_read() {
        let f = read(&collection()).expect("features");
        assert_eq!(f.features.len(), 3);
        assert_eq!(f.columns, ["name", "pop", "note", "extra"]);
        assert_eq!(f.matched, Some(49));
        assert!(f.lat_lon());
        assert_eq!(
            f.bounds,
            Some(Bounds {
                min: [-3.0, 0.0],
                max: [4.0, 9.0]
            })
        );
        let first = &f.features[0];
        assert_eq!(first.id.as_deref(), Some("states.1"));
        assert_eq!(first.geometry_type.as_deref(), Some("MultiPolygon"));
        assert_eq!(first.properties, ["ü😀", "12", "", ""]);
        let Shape::Polygon(rings) = &first.shapes[0] else {
            panic!("polygon");
        };
        assert_eq!(rings.len(), 2, "outer ring and hole");
        let second = &f.features[1];
        assert_eq!(second.id.as_deref(), Some("2"));
        assert_eq!(second.shapes.len(), 2);
        assert_eq!(second.properties[3], r#"{"a":[1]}"#);
        let third = &f.features[2];
        assert_eq!(third.geometry_type, None);
        assert_eq!(third.shapes, []);
    }

    #[test]
    fn a_single_feature_is_read() {
        let f = read(&json!({"type": "Feature", "geometry": {"type": "Point", "coordinates": [1.5, 2]}, "properties": {}}))
            .expect("feature");
        assert_eq!(f.features[0].shapes, [Shape::Point([1.5, 2.0])]);
        assert!(!f.lat_lon());
        assert_eq!(f.matched, None);
    }

    #[test]
    fn other_json_is_not_features() {
        for v in [
            json!({}),
            json!([]),
            json!({"type": "Point"}),
            json!({"type": "FeatureCollection"}),
        ] {
            assert_eq!(read(&v), None, "{v}");
        }
        let empty = read(&json!({"type": "FeatureCollection", "features": []})).expect("empty");
        assert_eq!(empty.features, []);
        assert_eq!(empty.bounds, None);
    }

    /// R-GEO-2: broken geometries are skipped, not a panic.
    #[test]
    fn broken_geometries_are_skipped() {
        let f = read(&json!({"type": "Feature", "properties": {}, "geometry": {"type": "Polygon", "coordinates": [[["a", 1]], 7]}}))
            .expect("feature");
        assert_eq!(f.features[0].shapes, []);
    }

    #[test]
    fn the_cheap_check_looks_at_the_start() {
        assert!(looks_like_features(
            br#" {"type":"FeatureCollection","features":[]}"#
        ));
        assert!(looks_like_features(br#"{"type": "Feature"}"#));
        assert!(!looks_like_features(br#"["FeatureCollection"]"#));
        assert!(!looks_like_features(br#"{"type":"Point"}"#));
        assert!(!looks_like_features(b""));
        let mut late = b"{\"pad\":\"".to_vec();
        late.extend(std::iter::repeat_n(b'x', 4096));
        late.extend(br#"","type":"FeatureCollection"}"#);
        assert!(!looks_like_features(&late));
    }
}
