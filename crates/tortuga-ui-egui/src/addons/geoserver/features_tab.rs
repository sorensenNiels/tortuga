//! The Features tab of a GeoJSON response (R-GEO-2): a line about the features, a shape
//! preview that zooms and pans, and a table of their properties. Hovering or clicking a
//! feature in one shows it in the other. The preview is a map on the chosen background
//! (R-GEO-3, D83).

use std::sync::Arc;

use egui::{Color32, Label, Pos2, Rect, RichText, ScrollArea, Sense, Stroke, Ui, Vec2, vec2};
use tortuga_core::format::names_match;
use tortuga_core::geoserver::features::{self, Point};
use tortuga_core::geoserver::map;
use tortuga_core::geoserver::{Bounds, Features, Shape};
use tortuga_core::settings::{Basemap, FeaturesLayout, OPENSTREETMAP};

use super::map_view::{
    BackgroundEdit, MapTiles, NewBackground, Projection, View, attribution, background_row, padded,
    paint_bounds, paint_tiles,
};
use crate::components::ResponseView;
use crate::theme;

/// Bodies bigger than this are not read for the tab (R-GEO-2): reading and drawing them
/// every frame would make the app stutter.
pub(crate) const MAX_BYTES: usize = 5 * 1024 * 1024;

const ROW_HEIGHT: f32 = 22.0;
const INDEX_WIDTH: f32 = 44.0;
const ID_WIDTH: f32 = 130.0;
const GEOMETRY_WIDTH: f32 = 110.0;
const PROPERTY_WIDTH: f32 = 150.0;
/// How near, in points, the pointer must be to a line or point to hit it.
const HIT_DISTANCE: f32 = 5.0;

/// What the tab keeps about the shown response.
#[derive(Debug, Clone, Default)]
pub(crate) struct FeaturesState {
    /// The features read from the shown body.
    read: Option<ReadBody>,
    selected: Option<usize>,
    /// The table row under the pointer last frame, highlighted in the preview.
    hovered_row: Option<usize>,
    /// Scroll the table to the selected row (it was clicked in the preview).
    scroll_to_selected: bool,
    /// Swap x and y; `None` follows the CRS ([`Features::lat_lon`]).
    swap: Option<bool>,
    /// Zoom and pan over the fitted preview.
    view: View,
    /// The features placed on the map, for the swap they were placed with.
    placed: Option<(bool, Option<Arc<Placed>>)>,
    /// A background being added, while its row is open.
    adding: Option<NewBackground>,
}

/// The features in map units (R-GEO-3).
#[derive(Debug)]
struct Placed {
    shapes: Vec<Vec<Shape>>,
    bounds: Option<Bounds>,
}

/// What the preview draws: each feature's shapes, their bounds, how to read them, and the
/// map under them.
struct Scene<'a> {
    shapes: Vec<&'a [Shape]>,
    bounds: Option<Bounds>,
    swap: bool,
    map: Option<&'a MapTiles>,
}

/// The features read from the body at `key` (address, length); `None` when the body is
/// not features after all.
#[derive(Debug, Clone)]
struct ReadBody {
    key: (usize, usize),
    features: Option<Arc<Features>>,
}

/// What the tab asks of the app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TabAction {
    SetLayout(FeaturesLayout),
    /// Choose the background of this name; empty: none (R-GEO-3).
    ChooseBackground(String),
    /// Add a background and choose it.
    AddBackground(Basemap),
    /// Remove the user's background of this name.
    RemoveBackground(String),
}

/// The backgrounds to choose from, and the chosen one (R-GEO-3, D83).
#[derive(Debug, Clone)]
pub(crate) struct Backgrounds {
    /// Their names: OpenStreetMap, then the user's.
    pub names: Vec<String>,
    /// The chosen one, with its tiles.
    pub map: MapTiles,
}

/// Whether a response gets the tab: its body starts like GeoJSON features.
pub(crate) fn offered(body: &[u8]) -> bool {
    features::looks_like_features(body)
}

pub(crate) fn features_tab(
    ui: &mut Ui,
    view: &ResponseView,
    st: &mut FeaturesState,
    layout: FeaturesLayout,
    backgrounds: &Backgrounds,
) -> Option<TabAction> {
    let map = &backgrounds.map;
    let body = &view.response.body;
    if body.len() > MAX_BYTES {
        theme::hint(
            ui,
            t!("features-too-big", limit = crate::format::size(MAX_BYTES)),
        );
        return None;
    }
    let key = (body.as_ptr() as usize, body.len());
    if st.read.as_ref().is_none_or(|r| r.key != key) {
        let features = view.json().and_then(features::read).map(Arc::new);
        *st = FeaturesState {
            read: Some(ReadBody { key, features }),
            swap: st.swap,
            adding: st.adding.take(),
            ..FeaturesState::default()
        };
    }
    let Some(found) = st.read.as_ref().and_then(|r| r.features.clone()) else {
        theme::hint(ui, t!("features-none"));
        return None;
    };
    let swap = st.swap.unwrap_or_else(|| found.lat_lon());
    let mut action = summary(ui, &found, st, swap, layout, backgrounds);
    if let Some(new) = &mut st.adding {
        let taken = |name: &str| backgrounds.names.iter().any(|n| names_match(n, name));
        match background_row(ui, new, taken) {
            Some(BackgroundEdit::Add) => {
                action = Some(TabAction::AddBackground(Basemap {
                    name: new.name.trim().to_owned(),
                    url: new.url.trim().to_owned(),
                }));
                st.adding = None;
                st.view = View::default();
            }
            Some(BackgroundEdit::Cancel) => st.adding = None,
            None => {}
        }
    }
    if found.features.is_empty() {
        theme::hint(ui, t!("features-empty"));
        return action;
    }
    let placed = st.placed(&found, swap);
    let scene = if let Some(placed) = &placed {
        Scene {
            shapes: placed.shapes.iter().map(Vec::as_slice).collect(),
            bounds: placed.bounds,
            swap: false,
            map: Some(map),
        }
    } else {
        // A CRS the map can't place: drawn as the body has it, on no background.
        let crs = found.crs.as_deref().unwrap_or_default();
        theme::hint(ui, t!("features-map-crs", crs = crs));
        Scene {
            shapes: found.features.iter().map(|f| f.shapes.as_slice()).collect(),
            bounds: found.bounds,
            swap,
            map: None,
        }
    };
    ui.add_space(6.0);
    match layout {
        FeaturesLayout::Above => {
            let height = (ui.available_height() * 0.45).clamp(160.0, 420.0);
            preview(ui, &found, &scene, st, vec2(ui.available_width(), height));
            ui.add_space(6.0);
            table(ui, &found, st);
        }
        FeaturesLayout::Beside => {
            let size = ui.available_size();
            let table_width = (size.x * 0.55).max(200.0);
            ui.horizontal_top(|ui| {
                ui.allocate_ui(vec2(table_width, size.y), |ui| {
                    ui.set_min_size(vec2(table_width, size.y));
                    table(ui, &found, st);
                });
                let rest = ui.available_size();
                preview(ui, &found, &scene, st, vec2(rest.x, rest.y.min(size.y)));
            });
        }
    }
    action
}

/// "12 features of 49 · CRS · bounds", Swap x/y, and the layout switch.
fn summary(
    ui: &mut Ui,
    f: &Features,
    st: &mut FeaturesState,
    swap: bool,
    layout: FeaturesLayout,
    backgrounds: &Backgrounds,
) -> Option<TabAction> {
    let p = theme::palette(ui);
    let mut action = None;
    egui::Sides::new().shrink_left().show(
        ui,
        |ui| {
            let count = match f.matched {
                Some(m) if m != f.features.len() as u64 => {
                    t!("features-count-of", count = f.features.len(), matched = m)
                }
                _ => t!("features-count", count = f.features.len()),
            };
            ui.label(theme::semibold(count));
            // Long names give way to the buttons on the right.
            if let Some(crs) = &f.crs {
                ui.label(RichText::new("·").color(p.muted));
                ui.add(
                    Label::new(RichText::new(crs).monospace().small().color(p.muted)).truncate(),
                );
            }
            if let Some(b) = f.bounds {
                ui.label(RichText::new("·").color(p.muted));
                ui.add(Label::new(RichText::new(bounds_text(b)).small().color(p.muted)).truncate())
                    .on_hover_text(t!("features-bounds-hint"));
            }
        },
        |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            for (value, label) in [
                (FeaturesLayout::Beside, t!("features-layout-beside")),
                (FeaturesLayout::Above, t!("features-layout-above")),
            ] {
                if ui
                    .add(egui::Button::selectable(layout == value, label))
                    .on_hover_text(t!("features-layout-hint"))
                    .clicked()
                    && layout != value
                {
                    action = Some(TabAction::SetLayout(value));
                }
            }
            ui.add_space(10.0);
            if let Some(asked) = background_picker(ui, st, backgrounds) {
                action = Some(asked);
            }
            ui.add_space(10.0);
            let mut on = swap;
            if ui
                .checkbox(&mut on, t!("features-swap"))
                .on_hover_text(t!("features-swap-hint"))
                .changed()
            {
                st.swap = Some(on);
            }
        },
    );
    action
}

fn bounds_text(b: Bounds) -> String {
    format!(
        "{:.4}, {:.4} → {:.4}, {:.4}",
        b.min[0], b.min[1], b.max[0], b.max[1]
    )
}

/// The background drop-down (OpenStreetMap, the user's, "Add background…"), and 🗙
/// to remove the chosen one if the user added it (R-GEO-3).
fn background_picker(
    ui: &mut Ui,
    st: &mut FeaturesState,
    backgrounds: &Backgrounds,
) -> Option<TabAction> {
    let mut asked = None;
    let chosen = backgrounds.map.name.as_str();
    // Right to left, as the summary's right side is laid out: 🗙, the list, its label.
    if !names_match(chosen, OPENSTREETMAP.0)
        && ui
            .add(theme::icon_button(ui, "🗙"))
            .on_hover_text(t!("features-background-remove", name = chosen))
            .clicked()
    {
        asked = Some(TabAction::RemoveBackground(chosen.to_owned()));
    }
    let list = egui::ComboBox::from_id_salt("features-background")
        .selected_text(chosen)
        .show_ui(ui, |ui| {
            for name in &backgrounds.names {
                let on = names_match(chosen, name);
                if ui.selectable_label(on, name.as_str()).clicked() && !on {
                    asked = Some(TabAction::ChooseBackground(name.clone()));
                }
            }
            ui.separator();
            if ui
                .selectable_label(false, t!("features-background-add"))
                .clicked()
            {
                st.adding = Some(NewBackground::default());
            }
        })
        .response
        .on_hover_text(t!("features-background-hint"));
    let label = ui.label(RichText::new(t!("features-background")).color(theme::palette(ui).muted));
    list.labelled_by(label.id);
    if asked.is_some() {
        st.view = View::default();
    }
    asked
}

impl FeaturesState {
    /// The features in map units for `swap`, worked out once; `None` inside when their
    /// CRS can't be placed (R-GEO-3).
    fn placed(&mut self, f: &Features, swap: bool) -> Option<Arc<Placed>> {
        if let Some((for_swap, placed)) = &self.placed
            && *for_swap == swap
        {
            return placed.clone();
        }
        let placed = map::on_map(f, swap).map(|(shapes, bounds)| {
            Arc::new(Placed {
                shapes,
                bounds: bounds.map(padded),
            })
        });
        self.placed = Some((swap, placed.clone()));
        placed
    }
}

/// A shape in screen positions.
enum Drawn {
    Point(Pos2),
    Line(Vec<Pos2>),
    Polygon(Vec<Vec<Pos2>>),
}

fn project(shape: &Shape, proj: Projection) -> Drawn {
    let line = |l: &[Point]| l.iter().map(|p| proj.to_screen(*p)).collect::<Vec<_>>();
    match shape {
        Shape::Point(p) => Drawn::Point(proj.to_screen(*p)),
        Shape::Line(l) => Drawn::Line(line(l)),
        Shape::Polygon(rings) => Drawn::Polygon(rings.iter().map(|r| line(r)).collect()),
    }
}

/// Whether `at` is on or in the drawn shape.
fn hits(drawn: &Drawn, at: Pos2) -> bool {
    match drawn {
        Drawn::Point(p) => p.distance(at) <= HIT_DISTANCE + 2.0,
        Drawn::Line(l) => near_line(l, at),
        Drawn::Polygon(rings) => {
            rings.iter().any(|r| near_line(r, at))
                || rings.first().is_some_and(|outer| inside(outer, at))
                    && !rings[1..].iter().any(|hole| inside(hole, at))
        }
    }
}

fn near_line(points: &[Pos2], at: Pos2) -> bool {
    points
        .windows(2)
        .any(|w| distance_to_segment(at, w[0], w[1]) <= HIT_DISTANCE)
}

fn distance_to_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let length = ab.length_sq();
    if length == 0.0 {
        return p.distance(a);
    }
    let t = ((p - a).dot(ab) / length).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

/// Even-odd rule: whether `p` is inside the ring.
fn inside(ring: &[Pos2], p: Pos2) -> bool {
    let mut odd = false;
    let n = ring.len();
    for i in 0..n {
        let (a, b) = (ring[i], ring[(i + n - 1) % n]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            odd = !odd;
        }
    }
    odd
}

/// The shapes, zoomed by the wheel and panned by dragging, on the map when there is one.
fn preview(ui: &mut Ui, f: &Features, scene: &Scene<'_>, st: &mut FeaturesState, size: Vec2) {
    let p = theme::palette(ui);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 6.0, p.surface);
    let Some(bounds) = scene.bounds else {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            t!("features-no-geometry"),
            egui::FontId::proportional(12.0),
            p.muted,
        );
        return;
    };
    zoom_and_pan(ui, &response, rect, &mut st.view);
    let proj = Projection::new(bounds, rect, scene.swap).viewed(st.view, rect);
    if let Some(m) = scene.map {
        paint_tiles(ui, rect, proj, m);
    }
    paint_bounds(&painter, bounds, proj, p.muted);
    let width = if scene.map.is_some() { 1.6 } else { 1.0 };
    let hover_at = response.hover_pos();
    let mut under_pointer = None;
    let mut highlighted = Vec::new();
    for (i, shapes) in scene.shapes.iter().enumerate() {
        let drawn: Vec<Drawn> = shapes.iter().map(|s| project(s, proj)).collect();
        if hover_at.is_some_and(|at| drawn.iter().any(|d| hits(d, at))) {
            under_pointer = Some(i);
        }
        if Some(i) == st.selected || Some(i) == st.hovered_row {
            // Drawn last, on top.
            highlighted.push(drawn);
        } else {
            paint(&painter, &drawn, Stroke::new(width, p.blue));
        }
    }
    for drawn in &highlighted {
        paint(&painter, drawn, Stroke::new(2.5, p.accent));
    }
    if let Some(i) = under_pointer {
        if Some(i) != st.selected && Some(i) != st.hovered_row {
            let drawn: Vec<Drawn> = scene.shapes[i].iter().map(|s| project(s, proj)).collect();
            paint(&painter, &drawn, Stroke::new(2.0, p.accent));
        }
        let label = f
            .features
            .get(i)
            .and_then(|feature| feature.id.clone())
            .unwrap_or_else(|| format!("#{}", i + 1));
        response.clone().on_hover_text_at_pointer(label);
    }
    if response.clicked() {
        st.selected = under_pointer;
        st.scroll_to_selected = under_pointer.is_some();
    }
    painter.rect_stroke(
        rect,
        6.0,
        Stroke::new(1.0, p.border),
        egui::StrokeKind::Inside,
    );
    if let Some(m) = scene.map {
        attribution(ui, rect, &m.template);
    }
    if !st.view.is_fitted() {
        let at = Rect::from_min_size(rect.right_top() + vec2(-58.0, 6.0), vec2(52.0, 20.0));
        // On top of the preview, outside the layout: `put` would move what follows up
        // under the button.
        let mut over = ui.new_child(egui::UiBuilder::new().max_rect(at));
        if over
            .add(egui::Button::new(t!("features-fit")).small())
            .on_hover_text(t!("features-fit-hint"))
            .clicked()
        {
            st.view = View::default();
        }
    }
}

/// The wheel zooms about the pointer, dragging pans, a double click zooms in.
fn zoom_and_pan(ui: &Ui, response: &egui::Response, rect: Rect, view: &mut View) {
    if response.dragged() {
        view.pan_by(response.drag_delta());
    }
    if let Some(at) = response.hover_pos() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            view.zoom_at(f64::from(scroll / 200.0).exp(), at, rect);
            // The wheel zooms the preview, not what it is in.
            ui.ctx().input_mut(|i| i.smooth_scroll_delta.y = 0.0);
        }
        if response.double_clicked() {
            view.zoom_at(2.0, at, rect);
        }
    }
}

fn paint(painter: &egui::Painter, drawn: &[Drawn], stroke: Stroke) {
    for d in drawn {
        match d {
            Drawn::Point(p) => {
                painter.circle_filled(*p, 3.0 + stroke.width / 2.0, stroke.color);
            }
            Drawn::Line(l) => {
                painter.line(l.clone(), stroke);
            }
            Drawn::Polygon(rings) => {
                for ring in rings {
                    painter.add(egui::Shape::closed_line(ring.clone(), stroke));
                }
            }
        }
    }
}

/// A row per feature: its number (click to select), id, geometry type and properties.
#[expect(
    clippy::cast_precision_loss,
    reason = "column and row counts are far below f32's exact range"
)]
fn table(ui: &mut Ui, f: &Features, st: &mut FeaturesState) {
    let p = theme::palette(ui);
    let width = INDEX_WIDTH + ID_WIDTH + GEOMETRY_WIDTH + PROPERTY_WIDTH * f.columns.len() as f32;
    let mut hovered_row = None;
    ScrollArea::horizontal()
        .id_salt("features-table-h")
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.set_min_width(width);
            row(ui, p.muted, |cell| {
                cell(INDEX_WIDTH, String::new());
                cell(ID_WIDTH, t!("features-id"));
                cell(GEOMETRY_WIDTH, t!("features-geometry"));
                for c in &f.columns {
                    cell(PROPERTY_WIDTH, c.clone());
                }
            });
            ui.separator();
            let mut scroll = ScrollArea::vertical()
                .id_salt("features-table-v")
                .auto_shrink([false, false]);
            if st.scroll_to_selected
                && let Some(i) = st.selected
            {
                scroll = scroll
                    .vertical_scroll_offset(i as f32 * (ROW_HEIGHT + ui.spacing().item_spacing.y));
                st.scroll_to_selected = false;
            }
            scroll.show_rows(ui, ROW_HEIGHT, f.features.len(), |ui, range| {
                for i in range {
                    let feature = &f.features[i];
                    let selected = st.selected == Some(i);
                    let top = ui.cursor().min;
                    let rect = Rect::from_min_size(top, vec2(width, ROW_HEIGHT));
                    if selected {
                        ui.painter().rect_filled(rect, 3.0, p.accent_fill);
                    }
                    ui.horizontal(|ui| {
                        ui.set_height(ROW_HEIGHT);
                        let clicked = ui
                            .add_sized(
                                [INDEX_WIDTH - 4.0, ROW_HEIGHT - 2.0],
                                egui::Button::selectable(selected, (i + 1).to_string()),
                            )
                            .on_hover_text(t!("features-select-hint"))
                            .clicked();
                        if clicked {
                            st.selected = (!selected).then_some(i);
                        }
                        let text = |ui: &mut Ui, w: f32, s: &str, muted: bool| {
                            let mut rich = RichText::new(s);
                            if muted {
                                rich = rich.color(p.muted);
                            }
                            ui.allocate_ui(vec2(w, ROW_HEIGHT), |ui| {
                                ui.set_width(w - 8.0);
                                ui.add(Label::new(rich).truncate());
                            });
                        };
                        text(ui, ID_WIDTH, feature.id.as_deref().unwrap_or(""), true);
                        text(
                            ui,
                            GEOMETRY_WIDTH,
                            feature.geometry_type.as_deref().unwrap_or("—"),
                            true,
                        );
                        for value in &feature.properties {
                            text(ui, PROPERTY_WIDTH, value, false);
                        }
                    });
                    if ui.rect_contains_pointer(rect) {
                        hovered_row = Some(i);
                    }
                }
            });
        });
    if st.hovered_row != hovered_row {
        st.hovered_row = hovered_row;
        ui.ctx().request_repaint();
    }
}

/// The header row: a cell per column, each `width` wide.
fn row(ui: &mut Ui, color: Color32, cells: impl FnOnce(&mut dyn FnMut(f32, String))) {
    ui.horizontal(|ui| {
        let mut cell = |w: f32, s: String| {
            ui.allocate_ui(vec2(w, ROW_HEIGHT), |ui| {
                ui.set_width(w - 8.0);
                ui.add(Label::new(theme::semibold(s).size(12.0).color(color)).truncate());
            });
        };
        cells(&mut cell);
    });
}

#[cfg(test)]
mod tests {
    use egui::pos2;

    use super::*;

    fn square() -> Vec<Pos2> {
        vec![
            pos2(0.0, 0.0),
            pos2(10.0, 0.0),
            pos2(10.0, 10.0),
            pos2(0.0, 10.0),
            pos2(0.0, 0.0),
        ]
    }

    /// R-GEO-2: the pointer hits a polygon inside it, not in its hole.
    #[test]
    fn polygons_are_hit_inside_but_not_in_holes() {
        let outer: Vec<Pos2> = square()
            .iter()
            .map(|p| pos2(p.x * 4.0, p.y * 4.0))
            .collect();
        let hole = vec![
            pos2(10.0, 10.0),
            pos2(30.0, 10.0),
            pos2(30.0, 30.0),
            pos2(10.0, 30.0),
        ];
        let drawn = Drawn::Polygon(vec![outer, hole]);
        assert!(hits(&drawn, pos2(2.0, 20.0)));
        assert!(!hits(&drawn, pos2(20.0, 20.0)), "in the hole");
        assert!(
            hits(&drawn, pos2(20.0, 10.0 + HIT_DISTANCE - 1.0)),
            "near the hole's edge"
        );
        assert!(!hits(&drawn, pos2(60.0, 60.0)));
        assert!(
            hits(&drawn, pos2(40.0 + HIT_DISTANCE - 1.0, 20.0)),
            "near the outer edge"
        );
    }

    #[test]
    fn lines_and_points_are_hit_near_them() {
        let line = Drawn::Line(vec![pos2(0.0, 0.0), pos2(100.0, 0.0)]);
        assert!(hits(&line, pos2(50.0, 4.0)));
        assert!(!hits(&line, pos2(50.0, 9.0)));
        assert!(!hits(&line, pos2(110.0, 0.0)));
        assert!(hits(&Drawn::Point(pos2(5.0, 5.0)), pos2(9.0, 5.0)));
        assert!(!hits(&Drawn::Line(Vec::new()), pos2(0.0, 0.0)));
        assert!(!hits(&Drawn::Polygon(Vec::new()), pos2(0.0, 0.0)));
    }
}
