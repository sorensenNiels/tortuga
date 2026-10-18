//! Tortuga's look: fonts, colours, global egui style, and small shared building
//! blocks (badges, tabs, hints, banners, framed groups). Every colour has a light
//! and a dark variant; egui follows the system theme.
//!
//! Call [`install`] once per `egui::Context` before drawing.

use std::sync::Arc;

use egui::epaint::Shadow;
use egui::{
    Color32, Context, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Frame, Margin,
    Response, RichText, Role, Stroke, TextStyle, Theme, Ui, Visuals, WidgetInfo,
};
use tortuga_core::HttpMethod;

/// Named font family for headings and emphasis (Inter `SemiBold`).
pub const SEMIBOLD: &str = "semibold";

/// Corner radius of inputs and buttons.
const RADIUS: u8 = 6;

/// Tortuga's colours for the current light/dark mode.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    /// Main content background.
    pub bg: Color32,
    /// Sidebar and title bar background.
    pub chrome: Color32,
    /// Inputs, secondary buttons, code blocks.
    pub surface: Color32,
    pub surface_hover: Color32,
    pub border: Color32,
    pub text: Color32,
    pub muted: Color32,
    /// Accent for text, lines and outlines.
    pub accent: Color32,
    /// Accent as a fill behind white text (primary buttons).
    pub accent_fill: Color32,
    pub ok: Color32,
    pub redirect: Color32,
    pub fail: Color32,
    pub blue: Color32,
    pub purple: Color32,
    pub warning_bg: Color32,
    pub warning_text: Color32,
    pub error_bg: Color32,
}

const LIGHT: Palette = Palette {
    bg: Color32::from_rgb(0xff, 0xff, 0xff),
    chrome: Color32::from_rgb(0xf6, 0xf7, 0xf9),
    surface: Color32::from_rgb(0xf3, 0xf4, 0xf6),
    surface_hover: Color32::from_rgb(0xe8, 0xea, 0xee),
    border: Color32::from_rgb(0xe2, 0xe5, 0xea),
    text: Color32::from_rgb(0x11, 0x18, 0x27),
    muted: Color32::from_rgb(0x6b, 0x72, 0x80),
    accent: Color32::from_rgb(0x0d, 0x94, 0x88),
    accent_fill: Color32::from_rgb(0x0d, 0x94, 0x88),
    ok: Color32::from_rgb(0x05, 0x96, 0x69),
    redirect: Color32::from_rgb(0xd9, 0x77, 0x06),
    fail: Color32::from_rgb(0xdc, 0x26, 0x26),
    blue: Color32::from_rgb(0x25, 0x63, 0xeb),
    purple: Color32::from_rgb(0x93, 0x33, 0xea),
    warning_bg: Color32::from_rgb(0xff, 0xfb, 0xeb),
    warning_text: Color32::from_rgb(0x92, 0x40, 0x0e),
    error_bg: Color32::from_rgb(0xfe, 0xf2, 0xf2),
};

const DARK: Palette = Palette {
    bg: Color32::from_rgb(0x13, 0x15, 0x1a),
    chrome: Color32::from_rgb(0x0d, 0x0f, 0x13),
    surface: Color32::from_rgb(0x1c, 0x1f, 0x26),
    surface_hover: Color32::from_rgb(0x25, 0x29, 0x32),
    border: Color32::from_rgb(0x2a, 0x2f, 0x3a),
    text: Color32::from_rgb(0xe7, 0xe9, 0xee),
    muted: Color32::from_rgb(0x8b, 0x93, 0xa7),
    accent: Color32::from_rgb(0x2d, 0xd4, 0xbf),
    accent_fill: Color32::from_rgb(0x0d, 0x94, 0x88),
    ok: Color32::from_rgb(0x34, 0xd3, 0x99),
    redirect: Color32::from_rgb(0xfb, 0xbf, 0x24),
    fail: Color32::from_rgb(0xf8, 0x71, 0x71),
    blue: Color32::from_rgb(0x60, 0xa5, 0xfa),
    purple: Color32::from_rgb(0xc0, 0x84, 0xfc),
    warning_bg: Color32::from_rgb(0x2a, 0x22, 0x10),
    warning_text: Color32::from_rgb(0xfc, 0xd3, 0x4d),
    error_bg: Color32::from_rgb(0x2b, 0x14, 0x16),
};

pub fn palette(ui: &Ui) -> Palette {
    if ui.visuals().dark_mode { DARK } else { LIGHT }
}

// ---------------------------------------------------------------- setup

/// Installs Tortuga's fonts and light/dark styles.
pub fn install(ctx: &Context) {
    ctx.set_fonts(fonts());
    ctx.all_styles_mut(|style| {
        use TextStyle::{Body, Button, Heading, Monospace, Small};
        let semibold = FontFamily::Name(SEMIBOLD.into());
        style.text_styles = [
            (Small, FontId::proportional(11.5)),
            (Body, FontId::proportional(13.5)),
            (Button, FontId::proportional(13.5)),
            (Heading, FontId::new(17.0, semibold)),
            (Monospace, FontId::monospace(12.5)),
        ]
        .into();
        let s = &mut style.spacing;
        s.item_spacing = egui::vec2(8.0, 6.0);
        s.button_padding = egui::vec2(10.0, 4.0);
        s.interact_size.y = 28.0;
        s.menu_margin = Margin::same(6);
        s.window_margin = Margin::same(20);
        s.combo_height = 320.0;
        s.scroll = egui::style::ScrollStyle::floating();
        style.interaction.tooltip_delay = 0.3;
    });
    ctx.style_mut_of(Theme::Dark, |s| s.visuals = visuals(&DARK, true));
    ctx.style_mut_of(Theme::Light, |s| s.visuals = visuals(&LIGHT, false));
}

fn fonts() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    let mut add = |name: &str, bytes: &'static [u8]| {
        fonts
            .font_data
            .insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
    };
    add("Inter", include_bytes!("../assets/fonts/Inter-Regular.ttf"));
    add(
        "Inter-SemiBold",
        include_bytes!("../assets/fonts/Inter-SemiBold.ttf"),
    );
    add(
        "JetBrainsMono",
        include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf"),
    );

    // Keep egui's bundled fonts after ours: they hold the symbols (⏵ ⚠ 🗙 …).
    let fallbacks = fonts.families[&FontFamily::Proportional].clone();
    let with = |first: &str| [vec![first.to_owned()], fallbacks.clone()].concat();
    fonts
        .families
        .insert(FontFamily::Proportional, with("Inter"));
    fonts
        .families
        .insert(FontFamily::Name(SEMIBOLD.into()), with("Inter-SemiBold"));
    let mono = fonts.families.entry(FontFamily::Monospace).or_default();
    mono.insert(0, "JetBrainsMono".to_owned());
    fonts
}

fn visuals(p: &Palette, dark: bool) -> Visuals {
    let mut v = if dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    let radius = CornerRadius::same(RADIUS);
    let border = Stroke::new(1.0, p.border);

    v.panel_fill = p.bg;
    v.window_fill = if dark { p.surface } else { p.bg };
    v.window_stroke = border;
    v.window_corner_radius = CornerRadius::same(12);
    v.menu_corner_radius = CornerRadius::same(8);
    let shadow_alpha = if dark { 140 } else { 40 };
    v.window_shadow = Shadow {
        offset: [0, 12],
        blur: 32,
        spread: 0,
        color: Color32::from_black_alpha(shadow_alpha),
    };
    v.popup_shadow = Shadow {
        offset: [0, 6],
        blur: 16,
        spread: 0,
        color: Color32::from_black_alpha(shadow_alpha),
    };
    v.extreme_bg_color = p.surface;
    v.text_edit_bg_color = Some(p.surface);
    v.code_bg_color = p.surface;
    v.faint_bg_color = p.chrome;
    v.hyperlink_color = p.accent;
    v.warn_fg_color = p.warning_text;
    v.error_fg_color = p.fail;
    v.selection.bg_fill = p.accent.gamma_multiply(if dark { 0.22 } else { 0.16 });
    v.selection.stroke = Stroke::new(1.0, p.accent);
    v.text_cursor.stroke = Stroke::new(2.0, p.accent);
    v.indent_has_left_vline = false;

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.bg;
    w.noninteractive.weak_bg_fill = p.bg;
    w.noninteractive.bg_stroke = border;
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    w.noninteractive.corner_radius = radius;

    w.inactive.bg_fill = p.surface;
    w.inactive.weak_bg_fill = p.surface;
    w.inactive.bg_stroke = border;
    w.inactive.fg_stroke = Stroke::new(1.0, p.text);
    w.inactive.corner_radius = radius;
    w.inactive.expansion = 0.0;

    w.hovered.bg_fill = p.surface_hover;
    w.hovered.weak_bg_fill = p.surface_hover;
    w.hovered.bg_stroke = Stroke::new(1.0, p.muted.gamma_multiply(0.6));
    w.hovered.fg_stroke = Stroke::new(1.0, p.text);
    w.hovered.corner_radius = radius;
    w.hovered.expansion = 0.0;

    w.active.bg_fill = p.surface_hover;
    w.active.weak_bg_fill = p.surface_hover;
    w.active.bg_stroke = Stroke::new(1.0, p.accent);
    w.active.fg_stroke = Stroke::new(1.0, p.text);
    w.active.corner_radius = radius;
    w.active.expansion = 0.0;

    w.open = w.active;
    w.open.bg_stroke = border;
    v
}

// ---------------------------------------------------------------- text

pub fn semibold(text: impl Into<String>) -> RichText {
    RichText::new(text).family(FontFamily::Name(SEMIBOLD.into()))
}

/// Small upper-case label above a group (e.g. "COLLECTIONS").
pub fn section_label(ui: &mut Ui, text: &str) {
    let color = palette(ui).muted;
    ui.label(semibold(text.to_uppercase()).size(10.5).color(color));
}

/// Secondary explanatory text.
pub fn hint(ui: &mut Ui, text: impl Into<String>) {
    let color = palette(ui).muted;
    ui.label(RichText::new(text).small().color(color));
}

pub fn warning(ui: &mut Ui, text: impl Into<String>) {
    let color = palette(ui).warning_text;
    ui.label(RichText::new(text).small().color(color));
}

pub fn error(ui: &mut Ui, text: impl Into<String>) {
    let color = palette(ui).fail;
    ui.label(RichText::new(text).small().color(color));
}

// ---------------------------------------------------------------- methods & badges

pub fn method_color(ui: &Ui, method: HttpMethod) -> Color32 {
    let p = palette(ui);
    match method {
        HttpMethod::Get => p.ok,
        HttpMethod::Post => p.redirect,
        HttpMethod::Put => p.blue,
        HttpMethod::Patch => p.purple,
        HttpMethod::Delete => p.fail,
        _ => p.muted,
    }
}

/// HTTP method as coloured text (method picker, tree rows).
pub fn method_text(ui: &Ui, method: HttpMethod) -> RichText {
    semibold(method.as_str()).color(method_color(ui, method))
}

/// Short method label for narrow places such as tree rows.
pub fn method_short(method: HttpMethod) -> &'static str {
    match method {
        HttpMethod::Delete => "DEL",
        HttpMethod::Options => "OPT",
        HttpMethod::Patch => "PATCH",
        m => m.as_str(),
    }
}

/// Project colours (R-COL-14): (light, dark) pairs that read well on both backgrounds.
const PROJECT_COLORS: [(Color32, Color32); 8] = [
    (
        Color32::from_rgb(0x0d, 0x94, 0x88),
        Color32::from_rgb(0x2d, 0xd4, 0xbf),
    ),
    (
        Color32::from_rgb(0x25, 0x63, 0xeb),
        Color32::from_rgb(0x60, 0xa5, 0xfa),
    ),
    (
        Color32::from_rgb(0x93, 0x33, 0xea),
        Color32::from_rgb(0xc0, 0x84, 0xfc),
    ),
    (
        Color32::from_rgb(0xdb, 0x27, 0x77),
        Color32::from_rgb(0xf4, 0x72, 0xb6),
    ),
    (
        Color32::from_rgb(0xea, 0x58, 0x0c),
        Color32::from_rgb(0xfb, 0x92, 0x3c),
    ),
    (
        Color32::from_rgb(0x16, 0xa3, 0x4a),
        Color32::from_rgb(0x4a, 0xde, 0x80),
    ),
    (
        Color32::from_rgb(0x4f, 0x46, 0xe5),
        Color32::from_rgb(0x81, 0x8c, 0xf8),
    ),
    (
        Color32::from_rgb(0xca, 0x8a, 0x04),
        Color32::from_rgb(0xfa, 0xcc, 0x15),
    ),
];

/// The colour of a project's tab group and sidebar dot, worked out from its name, so
/// it is the same every time and on every machine (R-COL-14, D32).
pub fn project_color(ui: &Ui, name: &str) -> Color32 {
    let (light, dark) = PROJECT_COLORS[project_color_index(name)];
    if ui.visuals().dark_mode { dark } else { light }
}

fn project_color_index(name: &str) -> usize {
    // FNV-1a: stable across runs and platforms, unlike `std`'s hasher.
    let hash = name.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    });
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the remainder is below the number of colours"
    )]
    let index = (hash % PROJECT_COLORS.len() as u64) as usize;
    index
}

/// A rounded, tinted pill (status codes, counts, "Unsaved").
pub fn badge(ui: &mut Ui, text: impl Into<String>, color: Color32) -> Response {
    Frame::new()
        .fill(color.gamma_multiply(0.14))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::symmetric(8, 2))
        .show(ui, |ui| ui.label(semibold(text).size(12.0).color(color)))
        .inner
}

// ---------------------------------------------------------------- tabs

/// Underlined tabs on a full-width rule. `items` are (value, label, optional badge such as a count).
pub fn tabs<T: PartialEq + Copy>(
    ui: &mut Ui,
    current: &mut T,
    items: &[(T, &str, Option<String>)],
) {
    let p = palette(ui);
    let mut underlines = Vec::new();
    let row = ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 20.0;
        for (value, label, badge) in items {
            let selected = *current == *value;
            let color = if selected { p.text } else { p.muted };
            // One real button per tab, so clicks work everywhere (also inside dialogs).
            let mut atoms = vec![egui::Atom::from(semibold(*label).color(color))];
            if let Some(b) = badge {
                let c = if selected { p.accent } else { p.muted };
                atoms.push(egui::Atom::from(semibold(b).size(10.5).color(c)));
            }
            let atoms: egui::Atoms<'_> = atoms.into_iter().collect();
            let response = ui.add(egui::Button::new(atoms).frame(false).gap(5.0));
            let name = match badge {
                Some(b) => format!("{label} ({b})"),
                None => (*label).to_owned(),
            };
            response.widget_info(|| WidgetInfo::selected(Role::Tab, true, selected, &name));
            if selected {
                underlines.push((response.rect.x_range(), p.accent));
            } else if response.hovered() {
                underlines.push((response.rect.x_range(), p.muted.gamma_multiply(0.5)));
            }
            if response.clicked() {
                *current = *value;
            }
            if response.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
        }
    });
    let y = row.response.rect.bottom() + 6.0;
    let painter = ui.painter();
    let full = ui.max_rect().x_range();
    painter.hline(full, y, Stroke::new(1.0, p.border));
    for (x, color) in underlines {
        painter.hline(x, y - 0.5, Stroke::new(2.0, color));
    }
    ui.add_space(12.0);
}

// ---------------------------------------------------------------- frames & fields

/// A bordered group that holds several widgets as one control (the request bar).
pub fn input_group(ui: &Ui) -> Frame {
    let p = palette(ui);
    Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(4, 2))
}

/// A bordered block for content (response body, code).
pub fn card(ui: &Ui) -> Frame {
    let p = palette(ui);
    Frame::new()
        .fill(
            p.surface
                .gamma_multiply(if ui.visuals().dark_mode { 0.6 } else { 0.5 }),
        )
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(10))
}

/// A rounded inline warning box with an icon.
pub fn callout(ui: &mut Ui, text: impl Into<String>) {
    let p = palette(ui);
    banner_frame(ui, false)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new("⚠").color(p.warning_text));
                ui.add(egui::Label::new(RichText::new(text).small().color(p.warning_text)).wrap());
            });
        });
}

/// Frame for a full-width warning or error banner.
pub fn banner_frame(ui: &Ui, is_error: bool) -> Frame {
    let p = palette(ui);
    Frame::new()
        .fill(if is_error { p.error_bg } else { p.warning_bg })
        .inner_margin(Margin::symmetric(16, 8))
}

/// Adds a text field of exactly `width`. Inside a [`egui::Grid`] a plain
/// `desired_width` is capped by last frame's column width, so cells would never grow.
pub fn sized_edit(ui: &mut Ui, width: f32, edit: egui::TextEdit<'_>) -> Response {
    let height = ui.spacing().interact_size.y;
    ui.add_sized([width, height], edit.align(egui::Align2::LEFT_CENTER))
}

/// Background for fields that use an undefined `{{variable}}` (R-VAR-5).
pub fn undefined_bg(ui: &Ui) -> Color32 {
    palette(ui).warning_bg
}

/// Whether `text` uses any of the `undefined` variable names.
pub fn uses_undefined(text: &str, undefined: &[String]) -> bool {
    !undefined.is_empty()
        && tortuga_core::vars::names(text)
            .iter()
            .any(|n| undefined.iter().any(|u| u == n))
}

// ---------------------------------------------------------------- buttons

/// A filled button in the accent colour (Send, Save, dialog confirmations).
pub fn primary_button<'a>(ui: &Ui, text: impl Into<String>) -> egui::Button<'a> {
    filled_button(text, palette(ui).accent_fill)
}

/// A filled button in the danger colour (Cancel request, Delete).
pub fn danger_button<'a>(ui: &Ui, text: impl Into<String>) -> egui::Button<'a> {
    filled_button(text, palette(ui).fail)
}

fn filled_button<'a>(text: impl Into<String>, fill: Color32) -> egui::Button<'a> {
    egui::Button::new(semibold(text).color(Color32::WHITE))
        .fill(fill)
        .stroke(Stroke::NONE)
}

/// A small, muted icon button (remove, dismiss).
pub fn icon_button<'a>(ui: &Ui, glyph: &str) -> egui::Button<'a> {
    ghost_button(RichText::new(glyph).size(11.0).color(palette(ui).muted))
}

/// A borderless button that only shows a background on hover (toolbar icons, row menus).
pub fn ghost_button<'a>(text: impl Into<egui::WidgetText>) -> egui::Button<'a> {
    egui::Button::new(text).frame_when_inactive(false)
}

#[cfg(test)]
mod tests {
    use egui_kittest::kittest::Queryable;

    #[test]
    fn project_colours_are_stable_and_spread() {
        assert_eq!(
            super::project_color_index("Orders API"),
            super::project_color_index("Orders API")
        );
        let used: std::collections::HashSet<usize> = ["Users API", "Orders API", "Billing", "Auth"]
            .iter()
            .map(|n| super::project_color_index(n))
            .collect();
        assert!(used.len() > 1, "different projects get different colours");
    }

    #[test]
    fn tab_clicks_work_inside_dialogs() {
        let mut tab = 0;
        let mut text = String::new();
        {
            let mut h = crate::test_host::harness(|ui| {
                egui::Modal::new(egui::Id::unique("m")).show(ui.ctx(), |ui| {
                    super::tabs(ui, &mut tab, &[(0, "One", None), (1, "Two", None)]);
                    ui.add(egui::TextEdit::singleline(&mut text).hint_text("x"));
                });
            });
            h.run();
            h.get_all_by_label("Two").next().expect("tab").click();
            h.run();
        }
        assert_eq!(tab, 1, "tab click inside a modal");
    }
}
