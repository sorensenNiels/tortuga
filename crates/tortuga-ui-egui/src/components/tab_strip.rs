//! The request tabs above the editor (R-COL-13), grouped by project (R-COL-14).

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use egui::{
    Atom, Button, Color32, CornerRadius, Frame, Margin, RichText, Role, ScrollArea, Stroke, Ui,
    WidgetInfo,
};
use tortuga_core::HttpMethod;

use crate::format::truncate;
use crate::state::ItemKey;
use crate::theme;

/// Longest tab title in bytes; the full name is in the tooltip.
const MAX_TITLE: usize = 32;

/// One tab as the strip shows it.
#[derive(Debug, Clone)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent marks of one tab"
)]
pub struct TabView {
    /// `None` = the scratch request.
    pub key: Option<ItemKey>,
    pub method: HttpMethod,
    pub name: String,
    /// Where the request is, for the tooltip (project › folder).
    pub place: String,
    pub preview: bool,
    pub active: bool,
    pub dirty: bool,
    pub conflict: bool,
    /// The project colour: a line under grouped tabs and the preview tab's stripe.
    pub color: Option<Color32>,
}

/// A project's chip in front of its tabs.
#[derive(Debug, Clone)]
pub struct GroupChip {
    pub root: PathBuf,
    pub name: String,
    pub color: Color32,
    pub collapsed: bool,
}

/// Tabs drawn together: a project's group (with a chip when there are several
/// projects), the scratch tab, or the preview tab.
#[derive(Debug, Clone)]
pub struct TabGroup {
    pub chip: Option<GroupChip>,
    pub tabs: Vec<TabView>,
}

/// What the user did in the strip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TabAction {
    Select(Option<ItemKey>),
    Keep(Option<ItemKey>),
    Close(Option<ItemKey>),
    CloseOthers(Option<ItemKey>),
    CloseToTheRight(Option<ItemKey>),
    CloseSaved,
    ToggleGroup(PathBuf),
    CloseGroup(PathBuf),
}

/// What the strip remembers between frames.
#[derive(Debug, Default)]
pub struct TabStripState {
    /// The active tab last frame: a newly active tab is scrolled into view.
    #[expect(
        clippy::option_option,
        reason = "outer `None`: no tab shown yet; inner `None`: the scratch request"
    )]
    shown: Option<Option<ItemKey>>,
}

/// Draws the tab strip; returns what the user asked for.
pub fn tab_strip(ui: &mut Ui, st: &mut TabStripState, groups: &[TabGroup]) -> Vec<TabAction> {
    let mut actions = Vec::new();
    let active = groups
        .iter()
        .flat_map(|g| &g.tabs)
        .find(|t| t.active)
        .map(|t| t.key.clone());
    let reveal = active.is_some() && st.shown != active;
    st.shown = active;
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            all_tabs_menu(ui, groups, &mut actions);
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Max), |ui| {
                ScrollArea::horizontal()
                    .id_salt("tab-strip")
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            for group in groups {
                                draw_group(ui, group, reveal, &mut actions);
                            }
                        });
                    });
            });
        });
    });
    actions
}

fn draw_group(ui: &mut Ui, group: &TabGroup, reveal: bool, actions: &mut Vec<TabAction>) {
    if let Some(chip) = &group.chip {
        ui.add_space(6.0);
        group_chip(ui, chip, group.tabs.len(), actions);
        if chip.collapsed {
            // The active tab stays visible in a folded group.
            for tab in group.tabs.iter().filter(|t| t.active) {
                tab_widget(ui, tab, reveal, actions);
            }
            return;
        }
    }
    for tab in &group.tabs {
        tab_widget(ui, tab, reveal, actions);
    }
}

fn group_chip(ui: &mut Ui, chip: &GroupChip, count: usize, actions: &mut Vec<TabAction>) {
    let mut text = chip.name.clone();
    if chip.collapsed {
        let _ = write!(text, " · {count}");
    }
    let response = ui
        .add(
            Button::new(theme::semibold(text).size(11.5).color(chip.color))
                .fill(chip.color.gamma_multiply(0.16))
                .stroke(Stroke::NONE)
                .corner_radius(CornerRadius::same(10)),
        )
        .on_hover_text(if chip.collapsed {
            t!("tabs-show-project")
        } else {
            t!("tabs-fold-project")
        });
    if response.clicked() {
        actions.push(TabAction::ToggleGroup(chip.root.clone()));
    }
    response.context_menu(|ui| {
        let label = if chip.collapsed {
            t!("expand")
        } else {
            t!("collapse")
        };
        if ui.button(label).clicked() {
            actions.push(TabAction::ToggleGroup(chip.root.clone()));
        }
        if ui.button(t!("tabs-close-project")).clicked() {
            actions.push(TabAction::CloseGroup(chip.root.clone()));
        }
    });
}

fn tab_widget(ui: &mut Ui, tab: &TabView, reveal: bool, actions: &mut Vec<TabAction>) {
    let p = theme::palette(ui);
    let fill = if tab.active {
        p.bg
    } else {
        Color32::TRANSPARENT
    };
    let frame = Frame::new()
        .fill(fill)
        .corner_radius(CornerRadius {
            nw: 6,
            ne: 6,
            sw: 0,
            se: 0,
        })
        .inner_margin(Margin {
            left: 4,
            right: 2,
            top: 2,
            bottom: 2,
        });
    let (title, cut) = truncate(&tab.name, MAX_TITLE);
    let title = if cut {
        format!("{title}…")
    } else if title.is_empty() {
        t!("untitled-request")
    } else {
        title.to_owned()
    };
    let shown = frame.show(ui, |ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        let lead = if tab.key.is_some() {
            Atom::from(
                RichText::new(theme::method_short(tab.method))
                    .monospace()
                    .size(10.5)
                    .color(theme::method_color(ui, tab.method)),
            )
        } else {
            Atom::from(RichText::new("✏").color(p.muted))
        };
        let mut name = RichText::new(&title).color(if tab.active { p.text } else { p.muted });
        if tab.preview {
            name = name.italics();
        }
        let mut atoms = vec![lead, Atom::from(name)];
        if tab.conflict {
            atoms.push(Atom::from(RichText::new("⚠").color(p.redirect)));
        }
        if tab.dirty {
            atoms.push(Atom::from(RichText::new("⏺").size(9.0).color(p.accent)));
        }
        let atoms: egui::Atoms<'_> = atoms.into_iter().collect();
        let label = ui.add(Button::new(atoms).frame(false).gap(6.0));
        label.widget_info(|| WidgetInfo::selected(Role::Tab, true, tab.active, &title));
        let close = ui
            .add(theme::icon_button(ui, "🗙").small())
            .on_hover_text(t!("tabs-close-hint"));
        (label, close)
    });
    let (label, close) = shown.inner;
    let rect = shown.response.rect;
    if let Some(color) = tab.color {
        let y = rect.bottom() - 1.0;
        ui.painter()
            .hline(rect.x_range(), y, Stroke::new(2.0, color));
    } else if tab.active {
        ui.painter().hline(
            rect.x_range(),
            rect.bottom() - 1.0,
            Stroke::new(2.0, p.accent),
        );
    }
    if tab.active && reveal {
        shown.response.scroll_to_me(None);
    }
    let key = &tab.key;
    let mut hover = tab.place.clone();
    if tab.preview {
        hover.push('\n');
        hover.push_str(&t!("tabs-preview-hint"));
    }
    let label = label.on_hover_text(hover);
    if close.clicked() || label.middle_clicked() {
        actions.push(TabAction::Close(key.clone()));
    } else if label.double_clicked() {
        actions.push(TabAction::Keep(key.clone()));
    } else if label.clicked() && !tab.active {
        actions.push(TabAction::Select(key.clone()));
    }
    label.context_menu(|ui| tab_menu(ui, tab, actions));
}

/// The right-click menu of a tab.
fn tab_menu(ui: &mut Ui, tab: &TabView, actions: &mut Vec<TabAction>) {
    let key = &tab.key;
    if tab.preview && ui.button(t!("tabs-keep-open")).clicked() {
        actions.push(TabAction::Keep(key.clone()));
    }
    if ui.button(t!("close")).clicked() {
        actions.push(TabAction::Close(key.clone()));
    }
    if ui.button(t!("tabs-close-others")).clicked() {
        actions.push(TabAction::CloseOthers(key.clone()));
    }
    if ui.button(t!("tabs-close-right")).clicked() {
        actions.push(TabAction::CloseToTheRight(key.clone()));
    }
    if ui
        .button(t!("tabs-close-saved"))
        .on_hover_text(t!("tabs-close-saved-hint"))
        .clicked()
    {
        actions.push(TabAction::CloseSaved);
    }
}

/// "⏷": every open tab, grouped by project (R-COL-13, R-COL-14).
fn all_tabs_menu(ui: &mut Ui, groups: &[TabGroup], actions: &mut Vec<TabAction>) {
    let p = theme::palette(ui);
    let (response, _) = egui::containers::menu::MenuButton::from_button(theme::ghost_button(
        RichText::new("⏷").color(p.muted),
    ))
    .ui(ui, |ui| {
        let mut last_project = None;
        for group in groups {
            for tab in &group.tabs {
                let project = tab.place.split(" › ").next().unwrap_or_default();
                if let Some(key) = &tab.key
                    && last_project != Some(project)
                {
                    project_heading(ui, project, &key.root, actions);
                    last_project = Some(project);
                }
                let mut name = RichText::new(if tab.name.is_empty() {
                    t!("untitled-request")
                } else {
                    tab.name.clone()
                });
                if tab.preview {
                    name = name.italics();
                }
                let method = if tab.key.is_some() {
                    RichText::new(format!("{:<5}", theme::method_short(tab.method)))
                        .monospace()
                        .size(10.5)
                        .color(theme::method_color(ui, tab.method))
                } else {
                    RichText::new("✏").color(p.muted)
                };
                let mut atoms = vec![Atom::from(method), Atom::from(name), Atom::grow()];
                if tab.dirty {
                    atoms.push(Atom::from(RichText::new("⏺").size(9.0).color(p.accent)));
                }
                let atoms: egui::Atoms<'_> = atoms.into_iter().collect();
                if ui.add(Button::selectable(tab.active, atoms)).clicked() {
                    actions.push(TabAction::Select(tab.key.clone()));
                }
            }
        }
        ui.separator();
        if ui.button(t!("tabs-close-saved")).clicked() {
            actions.push(TabAction::CloseSaved);
        }
    });
    response.on_hover_text(t!("tabs-all"));
}

/// A project's name in the "⏷" list, with 🗙 right after it to close its tabs (R-COL-37).
fn project_heading(ui: &mut Ui, project: &str, root: &Path, actions: &mut Vec<TabAction>) {
    ui.horizontal(|ui| {
        theme::section_label(ui, project);
        let close = ui
            .add(theme::icon_button(ui, "🗙").small())
            .on_hover_text(t!("tabs-close-project"));
        if close.clicked() {
            actions.push(TabAction::CloseGroup(root.to_owned()));
        }
    });
}

#[cfg(test)]
mod tests {
    use egui_kittest::kittest::Queryable;

    use super::*;

    fn tab(name: &str, active: bool, preview: bool) -> TabView {
        TabView {
            key: Some(ItemKey::new(Path::new("/a"), format!("{name}.toml"))),
            method: HttpMethod::Get,
            name: name.into(),
            place: "Users API".into(),
            preview,
            active,
            dirty: false,
            conflict: false,
            color: None,
        }
    }

    fn run(
        groups: &[TabGroup],
        act: impl FnOnce(&mut egui_kittest::Harness<'_>),
    ) -> Vec<TabAction> {
        let mut st = TabStripState::default();
        let mut actions = Vec::new();
        let mut harness = crate::test_host::harness(|ui| {
            actions.extend(tab_strip(ui, &mut st, groups));
        });
        harness.run();
        act(&mut harness);
        harness.run();
        drop(harness);
        actions
    }

    #[test]
    fn clicking_a_tab_selects_it_and_the_cross_closes_it() {
        let groups = [TabGroup {
            chip: None,
            tabs: vec![tab("List users", true, false), tab("Get user", false, true)],
        }];
        let key = Some(ItemKey::new(Path::new("/a"), "Get user.toml"));
        let actions = run(&groups, |h| h.get_by_label("Get user").click());
        assert_eq!(actions, [TabAction::Select(key.clone())]);
        let actions = run(&groups, |h| {
            h.get_all_by_label("🗙").nth(1).expect("close").click();
        });
        assert_eq!(actions, [TabAction::Close(key)]);
    }

    #[test]
    fn a_folded_group_shows_its_chip_with_a_count() {
        let groups = [TabGroup {
            chip: Some(GroupChip {
                root: "/a".into(),
                name: "Users API".into(),
                color: Color32::RED,
                collapsed: true,
            }),
            tabs: vec![
                tab("List users", false, false),
                tab("Get user", false, false),
            ],
        }];
        let actions = run(&groups, |h| h.get_by_label("Users API · 2").click());
        assert_eq!(actions, [TabAction::ToggleGroup("/a".into())]);
    }

    /// R-COL-37: the 🗙 next to a project's name in the "⏷" list closes its tabs.
    #[test]
    fn the_list_closes_a_projects_tabs_from_its_name() {
        let groups = [TabGroup {
            chip: None,
            tabs: vec![
                tab("List users", true, false),
                tab("Get user", false, false),
            ],
        }];
        let actions = run(&groups, |h| {
            h.get_by_label("⏷").click();
            h.run();
            h.get_by_label("USERS API");
            // The menu's 🗙 comes before the tabs' ones in the tree.
            h.get_all_by_label("🗙").next().expect("close").click();
        });
        assert_eq!(actions, [TabAction::CloseGroup("/a".into())]);
    }
}
