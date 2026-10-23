use std::path::PathBuf;
use std::sync::mpsc::Receiver;

use egui::{Context, Id, Modal, RichText, TextEdit, Ui};
use tortuga_core::CookieInfo;

use crate::state::AppState;
use crate::theme;

mod destinations;
pub use destinations::{CopyToProjectDialog, MoveDialog, SaveToCollectionDialog};

/// What a dialog wants after this frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogResult<T> {
    Open,
    Close,
    Done(T),
}

/// Shows a modal dialog with a heading. Escape or a click outside closes it
/// when `dismissable` (dialogs with a lot of input are not, so nothing is lost by accident).
pub(super) fn dialog<T>(
    ctx: &Context,
    id: &str,
    title: &str,
    width: f32,
    dismissable: bool,
    content: impl FnOnce(&mut Ui) -> DialogResult<T>,
) -> DialogResult<T> {
    let frame = egui::Frame::window(&ctx.global_style()).inner_margin(egui::Margin::same(22));
    let response = Modal::new(Id::unique(id)).frame(frame).show(ctx, |ui| {
        ui.set_width(width.min(ctx.content_rect().width() - 48.0));
        ui.heading(title);
        ui.add_space(10.0);
        content(ui)
    });
    match response.inner {
        DialogResult::Open if dismissable && response.should_close() => DialogResult::Close,
        other => other,
    }
}

/// A dialog's content that scrolls when the window is too small for it, leaving `room`
/// for the rest of the dialog. `last_height` is the content's height last frame, kept by
/// the dialog: a modal only offers a little more room each frame, so the scroll area is
/// given that height at once rather than growing over many frames, which would also
/// move the buttons under a click.
pub(super) fn fitting_scroll_area<R>(
    ui: &mut Ui,
    id: &str,
    room: f32,
    last_height: &mut f32,
    add: impl FnOnce(&mut Ui) -> R,
) -> R {
    let max_height = (ui.ctx().content_rect().height() - room).max(200.0);
    let output = egui::ScrollArea::vertical()
        .id_salt(id)
        .auto_shrink([false, true])
        .max_height(max_height)
        .min_scrolled_height(last_height.min(max_height))
        .show(ui, add);
    let height = output.content_size.y;
    if (height - *last_height).abs() > 0.5 {
        *last_height = height;
        // The dialog is centred on its size: draw again now, not when the pointer moves.
        ui.ctx().request_repaint();
    }
    output.inner
}

/// Width and height of the grip in a resizable dialog's bottom-right corner.
const GRIP: f32 = 14.0;

/// Like [`dialog`] (not dismissable), but the user resizes it by dragging its bottom-right
/// corner (R-SRV-7). `content` gets exactly the dialog's size below the heading; the size is
/// kept for the rest of the session.
pub(super) fn resizable_dialog<T>(
    ctx: &Context,
    id: &str,
    title: &str,
    default_size: egui::Vec2,
    min_size: egui::Vec2,
    content: impl FnOnce(&mut Ui) -> DialogResult<T>,
) -> DialogResult<T> {
    const MARGIN: i8 = 22;
    let size_id = Id::unique(("dialog-size", id));
    let max_size = ctx.content_rect().size() - egui::Vec2::splat(2.0 * f32::from(MARGIN) + 48.0);
    let size = ctx.data(|d| d.get_temp(size_id)).unwrap_or(default_size);
    let size = clamp_size(size, min_size, max_size);
    let frame = egui::Frame::window(&ctx.global_style()).inner_margin(egui::Margin::same(MARGIN));
    Modal::new(Id::unique(id))
        .frame(frame)
        .show(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
            let mut body = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(rect)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            body.set_clip_rect(rect.expand(f32::from(MARGIN)));
            body.heading(title);
            body.add_space(10.0);
            let result = content(&mut body);

            // The grip sits in the frame's margin, so it never covers the dialog's buttons.
            let corner = rect.max + egui::Vec2::splat(f32::from(MARGIN) - GRIP - 4.0);
            let grip_rect = egui::Rect::from_min_size(corner, egui::Vec2::splat(GRIP));
            let grip = ui
                .interact(grip_rect, size_id.with("grip"), egui::Sense::drag())
                .on_hover_cursor(egui::CursorIcon::ResizeNwSe)
                .on_hover_text(t!("drag-to-resize"));
            paint_grip(ui, grip_rect, grip.hovered() || grip.dragged());
            if grip.dragged() {
                // The dialog stays centred, so it grows on both sides: twice the pointer's move
                // keeps the corner under the pointer.
                let resized = clamp_size(size + 2.0 * grip.drag_delta(), min_size, max_size);
                ui.ctx().data_mut(|d| d.insert_temp(size_id, resized));
            }
            result
        })
        .inner
}

/// `size` within `min` and `max`; `min` wins on a window too small for it.
fn clamp_size(size: egui::Vec2, min: egui::Vec2, max: egui::Vec2) -> egui::Vec2 {
    size.min(max).max(min)
}

/// Three diagonal lines, like the corner of a resizable window.
fn paint_grip(ui: &Ui, rect: egui::Rect, active: bool) {
    let p = theme::palette(ui);
    let stroke = egui::Stroke::new(1.0, if active { p.accent } else { p.muted });
    for i in 1..=3u8 {
        let d = f32::from(i) * GRIP / 3.0;
        ui.painter().line_segment(
            [
                egui::pos2(rect.max.x - d, rect.max.y),
                egui::pos2(rect.max.x, rect.max.y - d),
            ],
            stroke,
        );
    }
}

/// Cancel and confirm buttons, right-aligned. Returns (cancel clicked, confirm clicked).
pub(super) fn actions(
    ui: &mut Ui,
    confirm: egui::Button<'_>,
    confirm_enabled: bool,
) -> (bool, bool) {
    ui.add_space(16.0);
    theme::button_row(ui, |ui| {
        let confirm = ui.add_enabled(confirm_enabled, confirm).clicked();
        let cancel = ui.button(t!("cancel")).clicked();
        (cancel, confirm)
    })
}

pub(super) fn finish<T>(cancel: bool, done: Option<T>) -> DialogResult<T> {
    match done {
        Some(value) => DialogResult::Done(value),
        None if cancel => DialogResult::Close,
        None => DialogResult::Open,
    }
}

/// A small heading above a field; returns its id, to name the field for screen readers.
fn field_label(ui: &mut Ui, text: &str) -> Id {
    ui.add_space(8.0);
    let color = theme::palette(ui).muted;
    ui.label(theme::semibold(text).size(12.0).color(color)).id
}

// ---------------------------------------------------------------- confirm

#[derive(Debug, Clone)]
pub struct ConfirmDialog {
    pub title: String,
    pub message: String,
    pub confirm_label: String,
}

impl ConfirmDialog {
    pub fn show(&self, ctx: &Context) -> DialogResult<()> {
        dialog(ctx, "confirm", &self.title, 380.0, true, |ui| {
            ui.label(&self.message);
            let (cancel, confirm) =
                actions(ui, theme::danger_button(ui, &self.confirm_label), true);
            finish(cancel, confirm.then_some(()))
        })
    }
}

// ---------------------------------------------------------------- create project

/// The chosen folder is not a project yet: offer to create one there (R-WS-7).
#[derive(Debug, Clone)]
pub struct CreateProjectDialog {
    pub dir: PathBuf,
    name: String,
    /// The git repository the folder is already in, if any.
    repository: Option<PathBuf>,
    can_init_git: bool,
    init_git: bool,
}

impl CreateProjectDialog {
    pub fn new(dir: PathBuf, repository: Option<PathBuf>, can_init_git: bool) -> Self {
        let name = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        Self {
            dir,
            name,
            repository,
            can_init_git,
            init_git: false,
        }
    }

    /// Done with (project name, make the folder a git repository).
    pub fn show(&mut self, ctx: &Context) -> DialogResult<(String, bool)> {
        dialog(
            ctx,
            "create-project",
            &t!("new-project-title"),
            440.0,
            true,
            |ui| {
                theme::hint(
                    ui,
                    t!("new-project-hint", path = self.dir.display().to_string()),
                );
                field_label(ui, &t!("new-project-name"));
                ui.add(TextEdit::singleline(&mut self.name).desired_width(f32::INFINITY));
                ui.add_space(8.0);
                match &self.repository {
                    Some(repo) => theme::hint(
                        ui,
                        t!(
                            "import-inside-repository",
                            path = repo.display().to_string()
                        ),
                    ),
                    None if self.can_init_git => {
                        ui.checkbox(&mut self.init_git, t!("new-project-git-init"))
                            .on_hover_text(t!("new-project-git-init-hint"));
                    }
                    None => {}
                }
                let name = self.name.trim();
                let (cancel, confirm) = actions(
                    ui,
                    theme::primary_button(ui, t!("create")),
                    !name.is_empty(),
                );
                finish(cancel, confirm.then(|| (name.to_owned(), self.init_git)))
            },
        )
    }
}

// ---------------------------------------------------------------- add existing projects

/// What the user chose in [`AddProjectsDialog`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddProjects {
    Add(Vec<PathBuf>),
    /// Nothing was found: create a project in the chosen folder instead.
    CreateHere(PathBuf),
}

/// Projects found in a folder or repository, as a checklist (R-WS-3).
#[derive(Debug)]
pub struct AddProjectsDialog {
    pub dir: PathBuf,
    results: Receiver<Result<Vec<PathBuf>, String>>,
    /// `None` while searching; then each project and whether it is ticked.
    found: Option<Result<Vec<(PathBuf, bool)>, String>>,
}

impl AddProjectsDialog {
    /// `results` delivers the search started for `dir`.
    pub fn new(dir: PathBuf, results: Receiver<Result<Vec<PathBuf>, String>>) -> Self {
        Self {
            dir,
            results,
            found: None,
        }
    }

    pub fn show(&mut self, ctx: &Context, state: &AppState) -> DialogResult<AddProjects> {
        if self.found.is_none()
            && let Ok(result) = self.results.try_recv()
        {
            // Everything starts ticked; projects already open stay ticked and disabled.
            self.found = Some(result.map(|paths| paths.into_iter().map(|p| (p, true)).collect()));
        }
        let dir = self.dir.clone();
        dialog(
            ctx,
            "add-projects",
            &t!("add-projects-title"),
            520.0,
            true,
            |ui| {
                let mut create_here = false;
                match &mut self.found {
                    None => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(t!("add-projects-looking", path = dir.display().to_string()));
                        });
                    }
                    Some(Err(e)) => theme::error(ui, e.as_str()),
                    Some(Ok(found)) if found.is_empty() => {
                        ui.label(t!("add-projects-none", path = dir.display().to_string()));
                        ui.add_space(8.0);
                        create_here = ui.button(t!("add-projects-create-there")).clicked();
                    }
                    Some(Ok(found)) => {
                        theme::hint(
                            ui,
                            t!("add-projects-found", path = dir.display().to_string()),
                        );
                        ui.add_space(6.0);
                        egui::ScrollArea::vertical()
                            .max_height(320.0)
                            .show(ui, |ui| {
                                for (path, ticked) in found.iter_mut() {
                                    let open = state.collection(path).is_some();
                                    let label = match path.strip_prefix(&dir) {
                                        Ok(rel) if rel.as_os_str().is_empty() => ".".to_owned(),
                                        Ok(rel) => rel.display().to_string(),
                                        Err(_) => path.display().to_string(),
                                    };
                                    let label = if open {
                                        t!("add-projects-already", path = label.as_str())
                                    } else {
                                        label
                                    };
                                    ui.add_enabled(!open, egui::Checkbox::new(ticked, label));
                                }
                            });
                    }
                }
                let chosen: Vec<PathBuf> = match &self.found {
                    Some(Ok(found)) => found
                        .iter()
                        .filter(|(p, ticked)| *ticked && state.collection(p).is_none())
                        .map(|(p, _)| p.clone())
                        .collect(),
                    _ => Vec::new(),
                };
                let label = t!("add-projects-add", count = chosen.len());
                let (cancel, confirm) =
                    actions(ui, theme::primary_button(ui, &label), !chosen.is_empty());
                if create_here {
                    return DialogResult::Done(AddProjects::CreateHere(dir.clone()));
                }
                finish(cancel, confirm.then_some(AddProjects::Add(chosen)))
            },
        )
    }
}

// ---------------------------------------------------------------- copy as curl

/// The request uses secret values: copy them masked, or include them (R-HTTP-9, P6).
#[derive(Debug, Clone)]
pub struct CopyCurlDialog {
    command: tortuga_core::curl::CurlCommand,
}

impl CopyCurlDialog {
    pub fn new(command: tortuga_core::curl::CurlCommand) -> Self {
        Self { command }
    }

    /// Done with the text to copy.
    pub fn show(&self, ctx: &Context) -> DialogResult<String> {
        let names = self.command.secrets.join(", ");
        dialog(ctx, "copy-curl", &t!("copy-as-curl"), 460.0, true, |ui| {
            ui.label(t!("curl-secrets-question", names = names.as_str()));
            ui.add_space(6.0);
            theme::hint(ui, t!("curl-secrets-hint"));
            ui.add_space(16.0);
            theme::button_row(ui, |ui| {
                let masked = ui
                    .add(theme::primary_button(ui, t!("curl-copy-masked")))
                    .clicked();
                let full = ui.button(t!("curl-include-secrets")).clicked();
                let cancel = ui.button(t!("cancel")).clicked();
                let done = if masked {
                    self.command.masked.clone()
                } else if full {
                    Some(self.command.full.clone())
                } else {
                    None
                };
                finish(cancel, done)
            })
        })
    }
}

// ---------------------------------------------------------------- name

/// Asks for a name, e.g. of a workspace.
#[derive(Debug, Clone)]
pub struct NameDialog {
    title: String,
    confirm_label: String,
    name: String,
    focused: bool,
}

impl NameDialog {
    pub fn new(title: &str, confirm_label: &str, name: &str) -> Self {
        Self {
            title: title.to_owned(),
            confirm_label: confirm_label.to_owned(),
            name: name.to_owned(),
            focused: false,
        }
    }

    pub fn show(&mut self, ctx: &Context) -> DialogResult<String> {
        dialog(ctx, "name", &self.title.clone(), 380.0, true, |ui| {
            field_label(ui, &t!("name"));
            let field = ui.add(
                TextEdit::singleline(&mut self.name)
                    .id_salt("name-dialog")
                    .desired_width(f32::INFINITY),
            );
            if !self.focused {
                field.request_focus();
                self.focused = true;
            }
            let name = self.name.trim().to_owned();
            let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let (cancel, confirm) = actions(
                ui,
                theme::primary_button(ui, &self.confirm_label),
                !name.is_empty(),
            );
            finish(
                cancel,
                ((confirm || enter) && !name.is_empty()).then_some(name),
            )
        })
    }
}

/// The runtime variables of a project, with "Clear all" (R-TEST-5).
#[derive(Debug, Clone)]
pub struct RuntimeVarsDialog {
    /// The project (`None`: the scratch request).
    pub root: Option<PathBuf>,
    title: String,
}

impl RuntimeVarsDialog {
    pub fn new(root: Option<PathBuf>, place: &str) -> Self {
        Self {
            root,
            title: t!("runtime-vars-title", place = place),
        }
    }

    /// `Done` means "clear them all".
    pub fn show(
        &self,
        ctx: &Context,
        vars: Option<&std::collections::BTreeMap<String, String>>,
    ) -> DialogResult<()> {
        dialog(ctx, "runtime-vars", &self.title, 520.0, true, |ui| {
            theme::hint(
                ui,
                "Values captured from responses or set by scripts. They win over environment and \
                 project values, and are forgotten when Tortuga closes.",
            );
            ui.add_space(8.0);
            match vars {
                None => theme::hint(ui, t!("none-yet")),
                Some(vars) => {
                    let p = theme::palette(ui);
                    egui::ScrollArea::vertical()
                        .max_height(320.0)
                        .show(ui, |ui| {
                            egui::Grid::new("runtime-vars")
                                .num_columns(2)
                                .spacing([16.0, 4.0])
                                .show(ui, |ui| {
                                    for (name, value) in vars {
                                        ui.label(RichText::new(name).monospace());
                                        let shown =
                                            crate::format::truncate(value, 200).0.to_owned();
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(shown).monospace().color(p.muted),
                                            )
                                            .truncate(),
                                        )
                                        .on_hover_text(value);
                                        ui.end_row();
                                    }
                                });
                        });
                }
            }
            let (close, clear) = actions(
                ui,
                theme::danger_button(ui, t!("clear-all")),
                vars.is_some(),
            );
            finish(close, clear.then_some(()))
        })
    }
}

/// What the user did in the cookie viewer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CookieAction {
    /// Remove this one; the dialog stays open.
    Remove(CookieInfo),
    /// Empty the jar; the dialog closes.
    ClearAll,
}

/// The cookies in a project's jar, each with a remove button, and "Clear all" (R-CK-5).
#[derive(Debug, Clone)]
pub struct CookiesDialog {
    pub root: PathBuf,
    title: String,
}

impl CookiesDialog {
    pub fn new(root: PathBuf, place: &str) -> Self {
        Self {
            root,
            title: t!("cookies-title", place = place),
        }
    }

    /// `now_ms` and `offset_minutes` format the expiry dates in local time.
    pub fn show(
        &self,
        ctx: &Context,
        cookies: &[CookieInfo],
        now_ms: u64,
        offset_minutes: i32,
    ) -> (DialogResult<()>, Option<CookieAction>) {
        let mut action = None;
        let result = dialog(ctx, "cookies", &self.title, 760.0, true, |ui| {
            theme::hint(
                ui,
                "Set by responses and sent with later matching requests. Kept on this computer \
                 only, never in the project.",
            );
            ui.add_space(8.0);
            if cookies.is_empty() {
                theme::hint(ui, t!("cookies-jar-empty"));
            } else {
                let p = theme::palette(ui);
                egui::ScrollArea::vertical()
                    .max_height(360.0)
                    .show(ui, |ui| {
                        egui::Grid::new("cookies")
                            .num_columns(6)
                            .spacing([14.0, 6.0])
                            .striped(true)
                            .show(ui, |ui| {
                                for heading in [
                                    String::new(),
                                    t!("name"),
                                    t!("value"),
                                    t!("cookies-domain-path"),
                                    t!("cookies-expires"),
                                    String::new(),
                                ] {
                                    theme::section_label(ui, &heading);
                                }
                                ui.end_row();
                                for c in cookies {
                                    if ui
                                        .add(theme::icon_button(ui, "🗙"))
                                        .on_hover_text(t!("cookies-remove"))
                                        .clicked()
                                    {
                                        action = Some(CookieAction::Remove(c.clone()));
                                    }
                                    ui.label(RichText::new(&c.name).monospace());
                                    let shown = crate::format::truncate(&c.value, 60).0.to_owned();
                                    // A truncated label in a Grid gets almost no width of its own.
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(200.0, ui.spacing().interact_size.y),
                                        egui::Layout::left_to_right(egui::Align::Center),
                                        |ui| {
                                            ui.set_width(200.0);
                                            ui.add(
                                                egui::Label::new(
                                                    RichText::new(shown).monospace().color(p.muted),
                                                )
                                                .truncate(),
                                            )
                                            .on_hover_text(&c.value);
                                        },
                                    );
                                    let place = cookie_place(c);
                                    ui.label(&place.0).on_hover_text(place.1);
                                    ui.label(cookie_expiry(c, now_ms, offset_minutes));
                                    ui.label(RichText::new(cookie_flags(c)).small().color(p.muted));
                                    ui.end_row();
                                }
                            });
                    });
            }
            let (close, clear) = actions(
                ui,
                theme::danger_button(ui, t!("clear-all")),
                !cookies.is_empty(),
            );
            if clear {
                action = Some(CookieAction::ClearAll);
            }
            finish(close, clear.then_some(()))
        });
        (result, action)
    }
}

/// "example.test/api" and what it means, for a cookie row.
fn cookie_place(c: &CookieInfo) -> (String, String) {
    let path = if c.path == "/" { "" } else { c.path.as_str() };
    let shown = if c.host_only {
        format!("{}{path}", c.domain)
    } else {
        format!("*.{}{path}", c.domain)
    };
    let hover = if c.host_only {
        t!("cookies-host-only", domain = &c.domain, path = &c.path)
    } else {
        t!(
            "cookies-with-subdomains",
            domain = &c.domain,
            path = &c.path
        )
    };
    (shown, hover)
}

fn cookie_expiry(c: &CookieInfo, now_ms: u64, offset_minutes: i32) -> String {
    match c.expires_ms {
        None => t!("cookies-session"),
        Some(ms) => format!(
            "{} {}",
            crate::format::day_label(ms, now_ms, offset_minutes),
            crate::format::clock(ms, offset_minutes)
        ),
    }
}

fn cookie_flags(c: &CookieInfo) -> String {
    [(c.secure, "Secure"), (c.http_only, "HttpOnly")]
        .iter()
        .filter(|(on, _)| *on)
        .map(|(_, name)| *name)
        .collect::<Vec<_>>()
        .join(" · ")
}

/// CA certificate files trusted besides the system's (R-HTTP-16). Returns whether "Add…"
/// was clicked.
#[cfg(test)]
mod tests {
    use egui_kittest::kittest::Queryable;

    use super::*;
    use crate::test_host::TestHost;

    /// R-SRV-7
    #[test]
    fn a_resized_dialog_stays_between_its_minimum_and_the_window() {
        let (min, max) = (egui::vec2(560.0, 360.0), egui::vec2(1000.0, 700.0));
        assert_eq!(
            clamp_size(egui::vec2(10.0, 900.0), min, max),
            egui::vec2(560.0, 700.0)
        );
        assert_eq!(
            clamp_size(egui::vec2(800.0, 500.0), min, max),
            egui::vec2(800.0, 500.0)
        );
        // A window smaller than the minimum keeps the minimum.
        assert_eq!(
            clamp_size(egui::vec2(800.0, 500.0), min, egui::vec2(300.0, 200.0)),
            min
        );
    }

    /// R-CK-5: the cookie viewer lists cookies with where they go and when they expire;
    /// 🗙 removes one, "Clear all" empties the jar.
    #[test]
    fn cookie_viewer_lists_removes_and_clears() {
        let cookie = |name: &str, host_only, expires_ms| CookieInfo {
            domain: "example.test".into(),
            host_only,
            path: "/".into(),
            name: name.into(),
            value: format!("{name}-value"),
            expires_ms,
            secure: true,
            http_only: false,
        };
        let cookies = [
            cookie("session", true, None),
            cookie("theme", false, Some(4_733_510_400_000)),
        ];
        let dialog = CookiesDialog::new(PathBuf::from("/p"), "API");
        let mut actions = Vec::new();
        {
            let mut h = crate::test_host::harness(|ui| {
                let (_, action) = dialog.show(ui.ctx(), &cookies, 0, 0);
                actions.extend(action);
            });
            h.run();
            h.get_by_label("Cookies — API");
            h.get_by_label("session-value");
            h.get_by_label("example.test");
            h.get_by_label("*.example.test");
            h.get_by_label("Session");
            h.get_by_label("1 Jan 2120 00:00");
            h.query_all_by_label("🗙")
                .nth(1)
                .expect("second row")
                .click();
            h.run();
            h.get_by_label("Clear all").click();
            h.run();
        }
        assert_eq!(
            actions,
            [
                CookieAction::Remove(cookies[1].clone()),
                CookieAction::ClearAll
            ]
        );
    }

    /// R-WS-3: found projects are a checklist; projects already open can't be added twice.
    #[test]
    fn found_projects_are_a_checklist() {
        let dir = std::env::temp_dir().join(format!("tortuga-addprojects-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let open = dir.join("users");
        tortuga_core::Collection::create(&tortuga_core::storage::FsStorage, &open, "Users")
            .expect("create");
        let state = AppState::new(Box::new(TestHost {
            open: vec![open.clone()],
            ..TestHost::default()
        }));
        let (tx, rx) = std::sync::mpsc::channel();
        let mut dialog = AddProjectsDialog::new(dir.clone(), rx);
        let mut result = None;
        {
            let mut h = crate::test_host::harness(|ui| {
                if let DialogResult::Done(r) = dialog.show(ui.ctx(), &state) {
                    result = Some(r);
                }
            });
            // `step`: the spinner keeps repainting while searching.
            h.step();
            assert!(h.query_by_label_contains("Looking for projects").is_some());
            tx.send(Ok(vec![
                dir.join("orders"),
                dir.join("payments/api"),
                open.clone(),
            ]))
            .expect("send");
            h.run();
            h.get_by_label("users  (already in the workspace)");
            h.get_by_label("orders").click(); // untick
            h.run();
            h.get_by_label("Add 1 project").click();
            h.run();
        }
        assert_eq!(
            result,
            Some(AddProjects::Add(vec![dir.join("payments/api")]))
        );
        std::fs::remove_dir_all(dir).expect("clean up");
    }

    /// R-WS-3: with nothing found, a project can be created there instead.
    #[test]
    fn nothing_found_offers_to_create_a_project() {
        let state = AppState::new(Box::new(TestHost::default()));
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(Ok(Vec::new())).expect("send");
        let dir = PathBuf::from("/somewhere");
        let mut dialog = AddProjectsDialog::new(dir.clone(), rx);
        let mut result = None;
        {
            let mut h = crate::test_host::harness(|ui| {
                if let DialogResult::Done(r) = dialog.show(ui.ctx(), &state) {
                    result = Some(r);
                }
            });
            h.run();
            h.get_by_label("Create a project there…").click();
            h.run();
        }
        assert_eq!(result, Some(AddProjects::CreateHere(dir)));
    }
}
