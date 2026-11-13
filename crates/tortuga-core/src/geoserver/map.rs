//! Putting features on a web map (R-GEO-3, D82): reading the CRS a body names, turning
//! its coordinates into longitude/latitude, then into Web Mercator "map units", and the
//! map tiles that cover a part of the map.
//!
//! Map units: x and y run from -0.5 to 0.5 across the Web Mercator square, north up.
//! Tile `(z, x, y)` follows the usual XYZ scheme (y down from the north).

use super::features::{Bounds, Features, Point, Shape};

/// The coordinate systems the map can place: everything else stays in the plain preview.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Crs {
    /// Longitude and latitude in degrees (EPSG:4326, CRS84, ETRS89's EPSG:4258).
    LonLat,
    /// Web Mercator metres (EPSG:3857, the older 900913).
    WebMercator,
    /// A UTM zone in metres (ETRS89 EPSG:258zz, WGS 84 EPSG:326zz north and 327zz south).
    Utm { zone: u8, north: bool },
}

impl Crs {
    /// The CRS a body names, e.g. `EPSG:25832`, `urn:ogc:def:crs:EPSG::4326`,
    /// `http://www.opengis.net/def/crs/EPSG/0/3857` or `urn:ogc:def:crs:OGC:1.3:CRS84`.
    /// `None` when it isn't one of these. A body naming none is longitude/latitude (RFC 7946).
    pub fn parse(name: &str) -> Option<Self> {
        let name = name.trim();
        if name.to_ascii_uppercase().ends_with("CRS84") {
            return Some(Self::LonLat);
        }
        let code: u32 = name.rsplit([':', '/']).next()?.trim().parse().ok()?;
        match code {
            4326 | 4258 => Some(Self::LonLat),
            3857 | 900_913 => Some(Self::WebMercator),
            25828..=25838 => Some(Self::utm(code - 25800, true)),
            32601..=32660 => Some(Self::utm(code - 32600, true)),
            32701..=32760 => Some(Self::utm(code - 32700, false)),
            _ => None,
        }
    }

    fn utm(zone: u32, north: bool) -> Self {
        Self::Utm {
            // Zones are 1 to 60: the ranges above guarantee it.
            zone: u8::try_from(zone).unwrap_or(0),
            north,
        }
    }

    /// `p` (x first) as longitude and latitude in degrees.
    pub fn to_lon_lat(self, p: Point) -> Point {
        match self {
            Self::LonLat => p,
            Self::WebMercator => {
                let lon = (p[0] / EARTH_RADIUS).to_degrees();
                let lat = (2.0 * (p[1] / EARTH_RADIUS).exp().atan() - std::f64::consts::FRAC_PI_2)
                    .to_degrees();
                [lon, lat]
            }
            Self::Utm { zone, north } => utm_to_lon_lat(zone, north, p),
        }
    }
}

/// The sphere Web Mercator uses, and WGS 84's semi-major axis.
const EARTH_RADIUS: f64 = 6_378_137.0;
/// How far north and south Web Mercator goes.
const MAX_LATITUDE: f64 = 85.051_128_779_806_59;

/// Inverse transverse Mercator on the WGS 84 ellipsoid (Snyder, "Map Projections — A
/// Working Manual", 1987, p. 63). GRS 80 (ETRS89) differs by far less than a pixel.
#[expect(
    clippy::many_single_char_names,
    reason = "the names in Snyder's formulas, to check them against the book"
)]
fn utm_to_lon_lat(zone: u8, north: bool, p: Point) -> Point {
    const K0: f64 = 0.9996;
    const FLATTENING: f64 = 1.0 / 298.257_223_563;
    let a = EARTH_RADIUS;
    let e2 = FLATTENING * (2.0 - FLATTENING);
    let ep2 = e2 / (1.0 - e2);
    let x = p[0] - 500_000.0;
    let y = if north { p[1] } else { p[1] - 10_000_000.0 };

    let m = y / K0;
    let mu = m / (a * (1.0 - e2 / 4.0 - 3.0 * e2.powi(2) / 64.0 - 5.0 * e2.powi(3) / 256.0));
    let root = (1.0 - e2).sqrt();
    let e1 = (1.0 - root) / (1.0 + root);
    let phi1 = mu
        + (3.0 * e1 / 2.0 - 27.0 * e1.powi(3) / 32.0) * (2.0 * mu).sin()
        + (21.0 * e1.powi(2) / 16.0 - 55.0 * e1.powi(4) / 32.0) * (4.0 * mu).sin()
        + (151.0 * e1.powi(3) / 96.0) * (6.0 * mu).sin()
        + (1097.0 * e1.powi(4) / 512.0) * (8.0 * mu).sin();

    let (sin, cos, tan) = (phi1.sin(), phi1.cos(), phi1.tan());
    let n1 = a / (1.0 - e2 * sin * sin).sqrt();
    let t1 = tan * tan;
    let c1 = ep2 * cos * cos;
    let r1 = a * (1.0 - e2) / (1.0 - e2 * sin * sin).powf(1.5);
    let d = x / (n1 * K0);

    let lat = phi1
        - (n1 * tan / r1)
            * (d.powi(2) / 2.0
                - (5.0 + 3.0 * t1 + 10.0 * c1 - 4.0 * c1.powi(2) - 9.0 * ep2) * d.powi(4) / 24.0
                + (61.0 + 90.0 * t1 + 298.0 * c1 + 45.0 * t1.powi(2)
                    - 252.0 * ep2
                    - 3.0 * c1.powi(2))
                    * d.powi(6)
                    / 720.0);
    let central = f64::from(zone) * 6.0 - 183.0;
    let lon = central.to_radians()
        + (d - (1.0 + 2.0 * t1 + c1) * d.powi(3) / 6.0
            + (5.0 - 2.0 * c1 + 28.0 * t1 - 3.0 * c1.powi(2) + 8.0 * ep2 + 24.0 * t1.powi(2))
                * d.powi(5)
                / 120.0)
            / cos;
    [lon.to_degrees(), lat.to_degrees()]
}

/// Longitude and latitude in map units (north up); latitudes beyond Web Mercator's are
/// held at its edge.
pub fn map_point(lon_lat: Point) -> Point {
    let lat = lon_lat[1].clamp(-MAX_LATITUDE, MAX_LATITUDE).to_radians();
    let y = (lat.tan() + 1.0 / lat.cos()).ln() / (2.0 * std::f64::consts::PI);
    [lon_lat[0] / 360.0, y]
}

/// The features' shapes in map units, a list per feature, and their bounds. `swap` reads
/// the body's coordinates as y first. `None` when the CRS can't be placed on the map.
pub fn on_map(f: &Features, swap: bool) -> Option<(Vec<Vec<Shape>>, Option<Bounds>)> {
    let crs = match &f.crs {
        Some(name) => Crs::parse(name)?,
        None => Crs::LonLat,
    };
    let to_map = |p: &Point| {
        let p = if swap { [p[1], p[0]] } else { *p };
        map_point(crs.to_lon_lat(p))
    };
    let mut bounds: Option<Bounds> = None;
    let mut grow = |p: Point| {
        let b = bounds.get_or_insert(Bounds { min: p, max: p });
        b.min = [b.min[0].min(p[0]), b.min[1].min(p[1])];
        b.max = [b.max[0].max(p[0]), b.max[1].max(p[1])];
    };
    let mut line = |l: &[Point]| {
        l.iter()
            .map(|p| {
                let m = to_map(p);
                grow(m);
                m
            })
            .collect::<Vec<_>>()
    };
    let shapes = f
        .features
        .iter()
        .map(|feature| {
            feature
                .shapes
                .iter()
                .map(|shape| match shape {
                    Shape::Point(p) => Shape::Point(line(std::slice::from_ref(p))[0]),
                    Shape::Line(l) => Shape::Line(line(l)),
                    Shape::Polygon(rings) => {
                        Shape::Polygon(rings.iter().map(|r| line(r)).collect())
                    }
                })
                .collect()
        })
        .collect();
    Some((shapes, bounds))
}

/// A map tile: zoom level and column and row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Tile {
    pub z: u8,
    pub x: u32,
    pub y: u32,
}

/// The deepest zoom level tiles are asked for.
pub const MAX_ZOOM: u8 = 19;

impl Tile {
    /// The tile's square in map units.
    pub fn bounds(self) -> Bounds {
        let n = f64::from(1_u32 << self.z);
        let (x, y) = (f64::from(self.x), f64::from(self.y));
        Bounds {
            min: [x / n - 0.5, 0.5 - (y + 1.0) / n],
            max: [(x + 1.0) / n - 0.5, 0.5 - y / n],
        }
    }

    /// The tile's URL from a template with `{z}`, `{x}` and `{y}` (and `{-y}` for TMS).
    pub fn url(self, template: &str) -> String {
        let flipped = (1_u32 << self.z) - 1 - self.y;
        template
            .trim()
            .replace("{z}", &self.z.to_string())
            .replace("{x}", &self.x.to_string())
            .replace("{-y}", &flipped.to_string())
            .replace("{y}", &self.y.to_string())
    }
}

/// The zoom level whose tiles, `tile_size` points wide, are about as sharp as a map
/// drawn at `points_per_unit`.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the level is rounded and clamped to 0..=MAX_ZOOM first"
)]
pub fn zoom_for(points_per_unit: f64, tile_size: f64) -> u8 {
    let level = (points_per_unit / tile_size).log2().round();
    if level.is_nan() {
        return 0;
    }
    level.clamp(0.0, f64::from(MAX_ZOOM)) as u8
}

/// The tiles at level `z` that cover `view` (map units), north row first; none when more
/// than `max` would be needed.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "tile numbers are clamped to 0..2^z first"
)]
pub fn tiles(view: Bounds, z: u8, max: usize) -> Vec<Tile> {
    let n = f64::from(1_u32 << z);
    let column = |x: f64| ((x + 0.5) * n).floor().clamp(0.0, n - 1.0) as u32;
    let row = |y: f64| ((0.5 - y) * n).floor().clamp(0.0, n - 1.0) as u32;
    let (x0, x1) = (column(view.min[0]), column(view.max[0]));
    let (y0, y1) = (row(view.max[1]), row(view.min[1]));
    let count = (x1 - x0 + 1) as usize * (y1 - y0 + 1) as usize;
    if count > max {
        return Vec::new();
    }
    (y0..=y1)
        .flat_map(|y| (x0..=x1).map(move |x| Tile { z, x, y }))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: Point, b: Point, tolerance: f64) -> bool {
        (a[0] - b[0]).abs() < tolerance && (a[1] - b[1]).abs() < tolerance
    }

    #[test]
    fn crs_names_are_read() {
        assert_eq!(Crs::parse("EPSG:4326"), Some(Crs::LonLat));
        assert_eq!(Crs::parse("urn:ogc:def:crs:EPSG::4326"), Some(Crs::LonLat));
        assert_eq!(
            Crs::parse("urn:ogc:def:crs:OGC:1.3:CRS84"),
            Some(Crs::LonLat)
        );
        assert_eq!(
            Crs::parse("http://www.opengis.net/def/crs/EPSG/0/3857"),
            Some(Crs::WebMercator)
        );
        assert_eq!(
            Crs::parse("urn:ogc:def:crs:EPSG::25832"),
            Some(Crs::Utm {
                zone: 32,
                north: true
            })
        );
        assert_eq!(
            Crs::parse("EPSG:32733"),
            Some(Crs::Utm {
                zone: 33,
                north: false
            })
        );
        assert_eq!(Crs::parse("EPSG:2154"), None, "Lambert 93: not placed");
        assert_eq!(Crs::parse("ü😀"), None);
        assert_eq!(Crs::parse(""), None);
    }

    /// The origin of a zone's central meridian, and the Eiffel Tower (UTM 31U 448252
    /// 5411932, 48.85826 N 2.29450 E).
    #[test]
    fn utm_turns_into_lon_lat() {
        let utm31 = Crs::Utm {
            zone: 31,
            north: true,
        };
        assert!(near(utm31.to_lon_lat([500_000.0, 0.0]), [3.0, 0.0], 1e-9));
        let eiffel = utm31.to_lon_lat([448_252.0, 5_411_932.0]);
        assert!(near(eiffel, [2.2945, 48.85826], 1e-4), "{eiffel:?}");
        // South: the false northing is taken off.
        let south = Crs::Utm {
            zone: 33,
            north: false,
        };
        assert!(near(
            south.to_lon_lat([500_000.0, 10_000_000.0]),
            [15.0, 0.0],
            1e-9
        ));
    }

    #[test]
    fn web_mercator_metres_and_map_units_agree() {
        let lon_lat = Crs::WebMercator.to_lon_lat([1_113_194.9, 6_446_275.8]);
        assert!(near(lon_lat, [10.0, 50.0], 1e-4), "{lon_lat:?}");
        assert!(near(map_point([0.0, 0.0]), [0.0, 0.0], 1e-12));
        assert!(near(map_point([180.0, MAX_LATITUDE]), [0.5, 0.5], 1e-9));
        assert!(
            near(map_point([-180.0, -90.0]), [-0.5, -0.5], 1e-9),
            "held at the edge"
        );
    }

    #[test]
    fn tiles_cover_a_view() {
        let world = Tile { z: 0, x: 0, y: 0 }.bounds();
        assert!(near(world.min, [-0.5, -0.5], 1e-12) && near(world.max, [0.5, 0.5], 1e-12));
        // The north-east quarter at level 1 is column 1, row 0.
        let ne = Tile { z: 1, x: 1, y: 0 }.bounds();
        assert!(near(ne.min, [0.0, 0.0], 1e-12) && near(ne.max, [0.5, 0.5], 1e-12));

        let view = Bounds {
            min: [0.01, 0.01],
            max: [0.2, 0.3],
        };
        assert_eq!(tiles(view, 1, 16), [Tile { z: 1, x: 1, y: 0 }]);
        assert_eq!(tiles(view, 3, 16).len(), 6);
        assert_eq!(tiles(view, 10, 16), [], "too many");
        // Outside the square: held to its edge.
        let beyond = Bounds {
            min: [-3.0, -3.0],
            max: [3.0, 3.0],
        };
        assert_eq!(tiles(beyond, 1, 16).len(), 4);
    }

    #[test]
    fn tile_urls_and_zoom_levels() {
        let t = Tile { z: 3, x: 4, y: 2 };
        assert_eq!(
            t.url(" https://tile.test/{z}/{x}/{y}.png "),
            "https://tile.test/3/4/2.png"
        );
        assert_eq!(
            t.url("https://tms.test/{z}/{x}/{-y}"),
            "https://tms.test/3/4/5"
        );
        assert_eq!(zoom_for(256.0, 256.0), 0);
        assert_eq!(zoom_for(256.0 * 1024.0, 256.0), 10);
        assert_eq!(zoom_for(1e30, 256.0), MAX_ZOOM);
        assert_eq!(zoom_for(1.0, 256.0), 0);
        assert_eq!(zoom_for(0.0, 256.0), 0);
    }
}
