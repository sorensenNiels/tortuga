//! How the settings dialog is laid out: pages listed on the left, and on the right
//! groups of rows, each a name with a short line about it and the control beside it.
//! Long explanations show on hover, so the page stays calm.

use egui::{Align, CornerRadius, Frame, Id, Layout, Margin, RichText, Stroke, Ui, vec2};

use crate::theme;

/// Width of the list of pages.
pub(super) const NAV_WIDTH: f32 = 168.0;

/// The pages of the settings dialog, in the order the list shows them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum Page {
    Appearance,
    Editing,
    ScriptEditor,
    Requests,
    Network,
    Features,
    History,
    About,
}

impl Page {
    fn name(self) -> String {
        match self {
            Self::Appearance => t!("settings-appearance"),
            Self::Editing => t!("settings-editing"),
            Self::ScriptEditor => t!("settings-script-editor"),
            Self::Requests => t!("import-row-requests"),
            Self::Network => t!("settings-network"),
            Self::Features => t!("settings-features"),
            Self::History => t!("history"),
            Self::About => t!("settings-about"),
        }
    }
}

/// The pages under their headings.
fn sections() -> [(String, &'static [Page]); 3] {
    [
        (
            t!("settings-group-general"),
            &[Page::Appearance, Page::Editing, Page::ScriptEditor],
        ),
        (
            t!("settings-group-connections"),
            &[Page::Requests, Page::Network],
        ),
        (
            t!("settings-group-app"),
            &[Page::Features, Page::History, Page::About],
        ),
    ]
}

/// The dialog's room split into the list of pages on the left and the page beside it.
pub(super) fn split(rect: egui::Rect) -> (egui::Rect, egui::Rect) {
    let nav = egui::Rect::from_min_size(rect.min, vec2(NAV_WIDTH, rect.height()));
    let page = egui::Rect::from_min_max(egui::pos2(nav.right() + 16.0, rect.top()), rect.max);
    (nav, page)
}

/// The list of pages, `height` tall; a click shows another page.
pub(super) fn nav(ui: &mut Ui, page: &mut Page, height: f32) {
    let p = theme::palette(ui);
    Frame::new()
        .fill(p.chrome)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(NAV_WIDTH - 16.0);
            ui.set_height(height - 16.0);
            ui.spacing_mut().item_spacing.y = 2.0;
            for (i, (heading, pages)) in sections().into_iter().enumerate() {
                if i > 0 {
                    ui.add_space(10.0);
                }
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    theme::section_label(ui, &heading);
                });
                ui.add_space(2.0);
                for &value in pages {
                    nav_item(ui, page, value);
                }
            }
        });
}

fn nav_item(ui: &mut Ui, page: &mut Page, value: Page) {
    let p = theme::palette(ui);
    let chosen = *page == value;
    let name = value.name();
    let text = if chosen {
        theme::semibold(name.as_str()).color(p.text)
    } else {
        RichText::new(name.as_str()).color(p.muted)
    };
    let atoms = (text, egui::Atom::grow());
    let mut button = egui::Button::new(atoms)
        .min_size(vec2(ui.available_width(), 28.0))
        .corner_radius(CornerRadius::same(6));
    button = if chosen {
        button.fill(p.bg).stroke(Stroke::new(1.0, p.border))
    } else {
        button.frame_when_inactive(false)
    };
    let response = ui.add(button);
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Tab, true, chosen, &name));
    if chosen {
        let r = response.rect;
        let bar = egui::Rect::from_min_size(
            egui::pos2(r.left() + 1.0, r.top() + 7.0),
            vec2(3.0, r.height() - 14.0),
        );
        ui.painter().rect_filled(bar, 1.5, p.accent);
    }
    if response.clicked() {
        *page = value;
    }
}

/// A titled group of rows on a card. `note` is a short line under the title, with its
/// long explanation on hover.
pub(super) fn group<R>(
    ui: &mut Ui,
    title: &str,
    note: Option<(&str, &str)>,
    add: impl FnOnce(&mut Rows<'_>) -> R,
) -> R {
    let p = theme::palette(ui);
    theme::section_label(ui, title);
    if let Some((short, long)) = note {
        ui.add(egui::Label::new(RichText::new(short).small().color(p.muted)).wrap())
            .on_hover_text(long);
    }
    ui.add_space(4.0);
    let inner = Frame::new()
        .fill(p.bg)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(14, 0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let label_width = (ui.available_width() * 0.42).clamp(140.0, 230.0);
            add(&mut Rows {
                ui,
                first: true,
                label_width,
            })
        })
        .inner;
    ui.add_space(18.0);
    inner
}

/// The rows of a [`group`].
pub(super) struct Rows<'a> {
    ui: &'a mut Ui,
    first: bool,
    label_width: f32,
}

/// What a row says about its setting.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct About<'a> {
    /// One short line under the name.
    pub short: Option<&'a str>,
    /// The long explanation, on hover.
    pub long: Option<&'a str>,
}

impl<'a> About<'a> {
    pub(super) fn short(short: &'a str) -> Self {
        Self {
            short: Some(short),
            long: None,
        }
    }

    pub(super) fn both(short: &'a str, long: &'a str) -> Self {
        Self {
            short: Some(short),
            long: Some(long),
        }
    }
}

impl Rows<'_> {
    /// A setting: its name and `about` on the left, `control` beside them at the top.
    /// `control` gets the name's id, to name its widget with `labelled_by`.
    pub(super) fn row<R>(
        &mut self,
        name: &str,
        about: About<'_>,
        control: impl FnOnce(&mut Ui, Id) -> R,
    ) -> R {
        self.divider();
        self.ui.add_space(10.0);
        let width = self.label_width;
        let inner = self
            .ui
            .horizontal_top(|ui| {
                // The name lines up with the text of a button or field beside it.
                let text = ui.text_style_height(&egui::TextStyle::Body);
                let pad = ((ui.spacing().interact_size.y - text) / 2.0).max(0.0);
                let name_id = ui
                    .allocate_ui_with_layout(vec2(width, 0.0), Layout::top_down(Align::Min), |ui| {
                        ui.set_width(width);
                        ui.add_space(pad);
                        label_column(ui, name, about)
                    })
                    .inner;
                ui.add_space(12.0);
                ui.vertical(|ui| control(ui, name_id)).inner
            })
            .inner;
        self.ui.add_space(10.0);
        inner
    }

    /// A fact, not a setting: its name and `value` as text side by side, on a lower row.
    pub(super) fn fact(&mut self, name: &str, value: impl FnOnce(&mut Ui)) {
        self.divider();
        self.ui.add_space(7.0);
        let width = self.label_width;
        let muted = theme::palette(self.ui).muted;
        self.ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(vec2(width, 0.0), Layout::top_down(Align::Min), |ui| {
                ui.set_width(width);
                ui.label(RichText::new(name).color(muted));
            });
            ui.add_space(12.0);
            ui.vertical(value);
        });
        self.ui.add_space(7.0);
    }

    /// A row across the whole card, for what isn't a setting (notes, buttons).
    pub(super) fn full_row<R>(&mut self, add: impl FnOnce(&mut Ui) -> R) -> R {
        self.divider();
        self.ui.add_space(10.0);
        let inner = add(self.ui);
        self.ui.add_space(10.0);
        inner
    }

    /// A line between rows; none above the first.
    fn divider(&mut self) {
        if std::mem::take(&mut self.first) {
            return;
        }
        let p = theme::palette(self.ui);
        let rect = self.ui.available_rect_before_wrap();
        let y = self.ui.cursor().top();
        self.ui
            .painter()
            .hline(rect.x_range(), y, Stroke::new(1.0, p.border));
    }
}

/// The name and the short line, both showing the long explanation on hover. Returns
/// the name's id.
fn label_column(ui: &mut Ui, name: &str, about: About<'_>) -> Id {
    let p = theme::palette(ui);
    ui.spacing_mut().item_spacing.y = 2.0;
    let mut label = ui.add(egui::Label::new(name).wrap());
    if let Some(long) = about.long {
        label = label.on_hover_text(long);
    }
    if let Some(short) = about.short {
        let line = ui.add(egui::Label::new(RichText::new(short).small().color(p.muted)).wrap());
        if let Some(long) = about.long {
            line.on_hover_text(long);
        }
    }
    label.id
}
