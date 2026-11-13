//! Placing features in the Features tab's preview: the projection that fits them, zoom
//! and pan over it (R-GEO-2), and the map under them, from the tile URL the user set
//! (R-GEO-3, D82).

use egui::{Color32, Pos2, Rect, Stroke, TextEdit, Ui, Vec2, pos2, vec2};
use tortuga_core::geoserver::Bounds;
use tortuga_core::geoserver::features::Point;
use tortuga_core::geoserver::map::{self, Tile};

use super::tiles::{SharedTiles, lock};
use crate::theme;

/// The chosen background: what the map draws tiles from (R-GEO-3).
#[derive(Debug, Clone)]
pub(crate) struct MapTiles {
    /// The background's name.
    pub name: String,
    pub template: String,
    pub tiles: SharedTiles,
}

/// How wide a tile is, in pixels.
const TILE_PIXELS: f32 = 256.0;
/// The most tiles drawn at once.
const MAX_TILES: usize = 64;
/// The smallest extent fitted on the map, in map units (about 400 m), so a single point
/// isn't fitted to a speck.
const MIN_MAP_EXTENT: f64 = 1e-5;
const MIN_ZOOM: f64 = 0.25;
const MAX_ZOOM: f64 = 1e7;

/// Zoom and pan over the view that fits the features; the default fits them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct View {
    zoom: f64,
    pan: Vec2,
}

impl Default for View {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            pan: Vec2::ZERO,
        }
    }
}

impl View {
    pub fn is_fitted(self) -> bool {
        self == Self::default()
    }

    /// Zooms by `factor`, keeping what is under `at` where it is.
    pub fn zoom_at(&mut self, factor: f64, at: Pos2, rect: Rect) {
        let zoom = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a ratio of two zoom levels is near 1"
        )]
        let f = (zoom / self.zoom) as f32;
        let d = at - rect.center();
        self.pan = d - (d - self.pan) * f;
        self.zoom = zoom;
    }

    pub fn pan_by(&mut self, delta: Vec2) {
        self.pan += delta;
    }
}

/// Maps coordinates into a rectangle, north up, keeping the aspect ratio. In `f64` until
/// the end, so a deeply zoomed map stays exact.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Projection {
    min: Point,
    /// Points per unit.
    scale: f64,
    /// Where `min` is drawn.
    origin: [f64; 2],
    swap: bool,
}

impl Projection {
    /// For `bounds` (as the body has them) fitted into `rect` with a margin.
    pub fn new(bounds: Bounds, rect: Rect, swap: bool) -> Self {
        let (min, max) = if swap {
            (
                [bounds.min[1], bounds.min[0]],
                [bounds.max[1], bounds.max[0]],
            )
        } else {
            (bounds.min, bounds.max)
        };
        let inner = rect.shrink(12.0);
        let (w, h) = (max[0] - min[0], max[1] - min[1]);
        let (width, height) = (f64::from(inner.width()), f64::from(inner.height()));
        // A single point, or features on one line, still get a scale.
        let scale = match (w > 0.0, h > 0.0) {
            (true, true) => (width / w).min(height / h),
            (true, false) => width / w,
            (false, true) => height / h,
            (false, false) => 1.0,
        };
        // Centre what is drawn.
        let center = inner.center();
        let origin = [
            f64::from(center.x) - w * scale / 2.0,
            f64::from(center.y) + h * scale / 2.0,
        ];
        Self {
            min,
            scale,
            origin,
            swap,
        }
    }

    /// This projection zoomed and panned by `view` about the centre of `rect`.
    #[must_use]
    pub fn viewed(self, view: View, rect: Rect) -> Self {
        let c = rect.center();
        let (cx, cy) = (f64::from(c.x), f64::from(c.y));
        Self {
            scale: self.scale * view.zoom,
            origin: [
                cx + (self.origin[0] - cx) * view.zoom + f64::from(view.pan.x),
                cy + (self.origin[1] - cy) * view.zoom + f64::from(view.pan.y),
            ],
            ..self
        }
    }

    #[expect(
        clippy::cast_possible_truncation,
        reason = "screen positions need no more than f32"
    )]
    pub fn to_screen(self, p: Point) -> Pos2 {
        let [x, y] = if self.swap { [p[1], p[0]] } else { p };
        pos2(
            (self.origin[0] + (x - self.min[0]) * self.scale) as f32,
            (self.origin[1] - (y - self.min[1]) * self.scale) as f32,
        )
    }

    /// The point drawn at `s`, x first.
    pub fn point_at(self, s: Pos2) -> Point {
        [
            (f64::from(s.x) - self.origin[0]) / self.scale + self.min[0],
            (self.origin[1] - f64::from(s.y)) / self.scale + self.min[1],
        ]
    }

    /// What `rect` shows, x first.
    pub fn shown(self, rect: Rect) -> Bounds {
        let [x0, y0] = self.point_at(rect.left_bottom());
        let [x1, y1] = self.point_at(rect.right_top());
        Bounds {
            min: [x0, y0],
            max: [x1, y1],
        }
    }
}

/// Bounds grown to at least [`MIN_MAP_EXTENT`] each way, about their centre.
pub(crate) fn padded(b: Bounds) -> Bounds {
    let grow = |lo: f64, hi: f64| {
        let half = ((hi - lo).max(MIN_MAP_EXTENT)) / 2.0;
        let mid = f64::midpoint(lo, hi);
        (mid - half, mid + half)
    };
    let (x0, x1) = grow(b.min[0], b.max[0]);
    let (y0, y1) = grow(b.min[1], b.max[1]);
    Bounds {
        min: [x0, y0],
        max: [x1, y1],
    }
}

/// Draws the tiles under `rect` for a map projected with `proj` (map units).
pub(crate) fn paint_tiles(ui: &Ui, rect: Rect, proj: Projection, tiles: &MapTiles) {
    let tile_points = f64::from(TILE_PIXELS / ui.ctx().pixels_per_point());
    let z = map::zoom_for(proj.scale, tile_points);
    let painter = ui.painter_at(rect);
    let shown = map::tiles(proj.shown(rect), z, MAX_TILES);
    let ready = lock(&tiles.tiles).draw(tiles.template.trim(), &shown);
    for (tile, texture) in ready {
        painter.image(
            texture.id(),
            tile_rect(tile, proj),
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    }
}

fn tile_rect(tile: Tile, proj: Projection) -> Rect {
    let b = tile.bounds();
    Rect::from_min_max(
        proj.to_screen([b.min[0], b.max[1]]),
        proj.to_screen([b.max[0], b.min[1]]),
    )
}

/// The features' bounding box.
pub(crate) fn paint_bounds(painter: &egui::Painter, b: Bounds, proj: Projection, color: Color32) {
    let r = Rect::from_two_pos(proj.to_screen(b.min), proj.to_screen(b.max));
    painter.rect_stroke(r, 0.0, Stroke::new(1.0, color), egui::StrokeKind::Middle);
}

/// Who the tiles are from, in the map's corner: OpenStreetMap asks for its credit.
pub(crate) fn attribution(ui: &Ui, rect: Rect, template: &str) {
    let text = if template.contains("openstreetmap.org") {
        t!("features-map-osm-credit")
    } else {
        host(template).to_owned()
    };
    let p = theme::palette(ui);
    let painter = ui.painter_at(rect);
    let galley = painter.layout_no_wrap(text, egui::FontId::proportional(10.0), p.muted);
    let at = rect.right_bottom() - galley.size() - vec2(6.0, 4.0);
    painter.rect_filled(
        Rect::from_min_size(at - vec2(3.0, 1.0), galley.size() + vec2(6.0, 2.0)),
        3.0,
        p.surface.gamma_multiply(0.85),
    );
    painter.galley(at, galley, p.muted);
}

/// The host of a URL template, or the template.
fn host(template: &str) -> &str {
    let rest = template
        .split_once("://")
        .map_or(template, |(_, rest)| rest);
    rest.split('/').next().unwrap_or(rest)
}

/// A background being added: what is typed.
#[derive(Debug, Clone, Default)]
pub(crate) struct NewBackground {
    pub name: String,
    pub url: String,
}

/// What the "Add background" row asked.
pub(crate) enum BackgroundEdit {
    Add,
    Cancel,
}

/// The row for adding a background (R-GEO-3); `taken` tells whether a name is in use.
pub(crate) fn background_row(
    ui: &mut Ui,
    new: &mut NewBackground,
    taken: impl Fn(&str) -> bool,
) -> Option<BackgroundEdit> {
    let mut asked = None;
    ui.add_space(4.0);
    theme::hint(ui, t!("features-map-setup-hint"));
    let name_taken = taken(new.name.trim());
    let ready = !new.name.trim().is_empty() && !name_taken && valid_template(&new.url);
    ui.horizontal(|ui| {
        ui.add(
            TextEdit::singleline(&mut new.name)
                .id_salt("features-background-name")
                .hint_text(t!("features-background-name"))
                .desired_width(140.0),
        );
        let url = ui.add(
            TextEdit::singleline(&mut new.url)
                .id_salt("features-background-url")
                .hint_text("https://tiles.example.com/{z}/{x}/{y}.png")
                .desired_width((ui.available_width() - 150.0).max(160.0)),
        );
        let enter = url.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if ui
            .add_enabled(ready, egui::Button::new(t!("features-background-add-it")))
            .clicked()
            || enter && ready
        {
            asked = Some(BackgroundEdit::Add);
        }
        if ui.add(theme::ghost_button(t!("cancel"))).clicked() {
            asked = Some(BackgroundEdit::Cancel);
        }
    });
    if name_taken {
        theme::warning(ui, t!("features-background-taken", name = new.name.trim()));
    } else if !new.url.trim().is_empty() && !valid_template(&new.url) {
        theme::warning(ui, t!("features-map-needs-placeholders"));
    }
    asked
}

/// A template with `{z}`, `{x}` and `{y}` (or `{-y}`).
fn valid_template(text: &str) -> bool {
    let t = text.trim();
    t.contains("{z}") && t.contains("{x}") && (t.contains("{y}") || t.contains("{-y}"))
}

#[cfg(test)]
mod tests {
    use tortuga_core::settings::OPENSTREETMAP;

    use super::*;

    /// R-GEO-2: north is up, the aspect ratio is kept, and swapping reads lat/lon.
    #[test]
    fn the_projection_puts_north_up() {
        let bounds = Bounds {
            min: [0.0, 0.0],
            max: [10.0, 5.0],
        };
        let rect = Rect::from_min_size(Pos2::ZERO, vec2(224.0, 224.0));
        let proj = Projection::new(bounds, rect, false);
        let sw = proj.to_screen([0.0, 0.0]);
        let ne = proj.to_screen([10.0, 5.0]);
        assert!(ne.y < sw.y, "north is up");
        assert!(ne.x > sw.x, "east is right");
        assert!(
            (ne.x - sw.x - 2.0 * (sw.y - ne.y)).abs() < 0.01,
            "aspect kept"
        );
        assert!(rect.contains(sw) && rect.contains(ne));

        let swapped = Projection::new(bounds, rect, true);
        // [lat, lon]: the body's first number goes up.
        let a = swapped.to_screen([0.0, 0.0]);
        let b = swapped.to_screen([10.0, 0.0]);
        assert!(b.y < a.y && (b.x - a.x).abs() < 0.01);
    }

    #[test]
    fn a_single_point_still_projects() {
        let bounds = Bounds {
            min: [3.0, 4.0],
            max: [3.0, 4.0],
        };
        let rect = Rect::from_min_size(Pos2::ZERO, vec2(100.0, 100.0));
        let at = Projection::new(bounds, rect, false).to_screen([3.0, 4.0]);
        assert!((at - rect.center()).length() < 0.01);
    }

    /// R-GEO-2: zooming keeps what is under the pointer there; panning moves it; the
    /// screen maps back to the same point.
    #[test]
    fn zooming_keeps_the_point_under_the_pointer() {
        let bounds = Bounds {
            min: [0.0, 0.0],
            max: [10.0, 10.0],
        };
        let rect = Rect::from_min_size(Pos2::ZERO, vec2(200.0, 200.0));
        let fitted = Projection::new(bounds, rect, false);
        let mut view = View::default();
        assert!(view.is_fitted());
        let at = pos2(150.0, 60.0);
        let under = fitted.viewed(view, rect).point_at(at);
        view.zoom_at(8.0, at, rect);
        let again = fitted.viewed(view, rect).to_screen(under);
        assert!((again - at).length() < 0.01, "{again:?}");
        view.pan_by(vec2(10.0, -5.0));
        let moved = fitted.viewed(view, rect).to_screen(under);
        assert!((moved - (at + vec2(10.0, -5.0))).length() < 0.01);
        assert!(!view.is_fitted());
        view.zoom_at(1e30, at, rect);
        assert!((view.zoom - MAX_ZOOM).abs() < f64::EPSILON, "held");
    }

    /// R-GEO-3: deep zoom on the map stays exact to the pixel.
    #[test]
    fn a_deep_zoom_stays_exact() {
        let b = padded(Bounds {
            min: [0.1, 0.2],
            max: [0.1, 0.2],
        });
        let rect = Rect::from_min_size(Pos2::ZERO, vec2(400.0, 400.0));
        let mut view = View::default();
        view.zoom_at(1000.0, rect.center(), rect);
        let proj = Projection::new(b, rect, false).viewed(view, rect);
        let p = [0.1 + 1e-9, 0.2];
        let back = proj.point_at(proj.to_screen(p));
        assert!((back[0] - p[0]).abs() < 1e-10, "{back:?}");
    }

    #[test]
    fn padding_widens_only_tiny_extents() {
        let point = padded(Bounds {
            min: [0.1, 0.2],
            max: [0.1, 0.2],
        });
        assert!((point.max[0] - point.min[0] - MIN_MAP_EXTENT).abs() < 1e-15);
        let wide = Bounds {
            min: [0.0, 0.0],
            max: [0.1, 0.1],
        };
        assert_eq!(padded(wide), wide);
    }

    #[test]
    fn tile_url_templates_need_their_placeholders() {
        assert!(!valid_template(" "));
        assert!(valid_template(OPENSTREETMAP.1));
        assert!(valid_template("https://t.test/{z}/{x}/{-y}.png"));
        assert!(!valid_template("https://t.test/tiles.png"));
        assert_eq!(host(OPENSTREETMAP.1), "tile.openstreetmap.org");
        assert_eq!(host("ü😀"), "ü😀");
    }
}
