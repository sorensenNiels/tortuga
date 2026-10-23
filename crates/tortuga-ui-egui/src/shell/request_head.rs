//! The request's header line (R-COL-23): where it is, its name and its actions; and
//! `curl`: copying the request as a command (R-HTTP-9) and pasting one (R-HTTP-11).

use egui::{Frame, RichText, TextEdit, Ui};

use super::{Modal, TortugaUi, breadcrumb};
use crate::components::CopyCurlDialog;
use crate::state::ItemKey;
use crate::theme;

/// Height of the request's header line, and the size of its name (R-COL-23).
const HEAD_HEIGHT: f32 = 32.0;
const TITLE_SIZE: f32 = 16.0;

impl TortugaUi {
    /// Request name, unsaved marker and save/revert (R-COL-6).
    pub(super) fn request_head(
        &mut self,
        ui: &mut Ui,
        key: Option<&ItemKey>,
        current_dirty: bool,
        read_only: bool,
    ) {
        let p = theme::palette(ui);
        // One line (IDEA-73, R-COL-23): where the request is, its name, and its actions.
        let Some(k) = key else {
            egui::Sides::new().height(HEAD_HEIGHT).show(
                ui,
                |ui| {
                    ui.label(RichText::new(t!("scratch-request")).color(p.muted));
                    ui.label(theme::semibold(t!("untitled-request")).size(TITLE_SIZE));
                },
                |ui| {
                    if ui
                        .button(t!("save-to-project"))
                        .on_hover_text(t!("save-shortcut"))
                        .clicked()
                    {
                        self.save();
                    }
                    if copy_curl_button(ui, self.copied_at) {
                        self.copy_as_curl(ui.ctx());
                    }
                },
            );
            ui.add_space(6.0);
            return;
        };
        let crumb = breadcrumb(&self.state, k);
        let copied_at = self.copied_at;
        let ((), (save, revert, copy)) = egui::Sides::new().height(HEAD_HEIGHT).shrink_left().show(
            ui,
            |ui| {
                ui.add(egui::Label::new(RichText::new(crumb).color(p.muted)).truncate());
                // Looks like a title; becomes a field on hover/focus.
                let title =
                    egui::FontId::new(TITLE_SIZE, egui::FontFamily::Name(theme::SEMIBOLD.into()));
                ui.add_enabled(
                    !read_only,
                    TextEdit::singleline(&mut self.current.name)
                        .id_salt("request-name")
                        .font(title)
                        .frame(Frame::new())
                        // No margin, so the name sits on the breadcrumb's line.
                        .margin(egui::Margin::ZERO)
                        // Centred in the line's height, like the breadcrumb beside it.
                        .align(egui::Align2::LEFT_CENTER)
                        // Grows with the name, so the badge sits right after it.
                        .clip_text(false)
                        .desired_width(24.0),
                )
                .on_hover_text(t!("request-name"));
                if current_dirty {
                    theme::badge(ui, t!("unsaved"), p.redirect)
                        .on_hover_text(t!("request-unsaved-hint"));
                }
            },
            |ui| {
                let save = ui
                    .add_enabled(
                        current_dirty && !read_only,
                        theme::primary_button(ui, t!("save")),
                    )
                    .on_hover_text(t!("save-shortcut"))
                    .clicked();
                let revert = current_dirty && ui.button(t!("revert")).clicked();
                let copy = copy_curl_button(ui, copied_at);
                (save, revert, copy)
            },
        );
        if copy {
            self.copy_as_curl(ui.ctx());
        }
        if save {
            self.save();
        }
        if revert {
            self.revert();
        }
        ui.add_space(6.0);
        let key = k.clone();
        self.conflict_banner(ui, &key);
    }

    /// Copies the request as curl; asks first if it contains secret values (R-HTTP-9).
    pub(super) fn copy_as_curl(&mut self, egui_ctx: &egui::Context) {
        let ctx = self.state.request_context(self.selection.as_ref());
        match tortuga_core::curl::command(&self.current, &ctx, &self.state.settings.client) {
            Err(e) => {
                self.copied_at = None;
                self.state
                    .notify(t!("curl-copy-failed", error = e.to_string()));
            }
            Ok(command) if command.masked.is_none() => {
                egui_ctx.copy_text(command.full);
                self.copied_at = Some(egui_ctx.input(|i| i.time));
            }
            Ok(command) => self.modal = Some(Modal::CopyCurl(CopyCurlDialog::new(command))),
        }
    }

    /// Fills the current request from a pasted `curl` command, as unsaved edits
    /// (R-HTTP-11, D19). Its name and cookie setting stay.
    pub(super) fn paste_curl(&mut self, text: &str) {
        use tortuga_core::curl::import;

        let pasted = match import::parse(text) {
            Ok(pasted) => pasted,
            Err(e) => {
                self.state.notify(t!("curl-read-failed", error = e));
                return;
            }
        };
        let mut request = pasted.request;
        import::use_server(
            &mut request,
            &self.state.request_context(self.selection.as_ref()),
        );
        request.name.clone_from(&self.current.name);
        request.cookies = self.current.cookies;
        let response = std::mem::take(&mut self.response);
        self.load(request);
        self.response = response;
        if !pasted.ignored.is_empty() {
            self.state
                .inform(t!("curl-filled-in", ignored = pasted.ignored.join("; ")));
        }
    }
}

/// "Copy as curl" (R-HTTP-9); shows "Copied" for a moment after `copied_at`. Returns whether it was clicked.
fn copy_curl_button(ui: &mut Ui, copied_at: Option<f64>) -> bool {
    const FEEDBACK_SECONDS: f64 = 2.0;
    let now = ui.input(|i| i.time);
    let copied = copied_at.is_some_and(|t| now - t < FEEDBACK_SECONDS);
    if copied {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_secs_f64(FEEDBACK_SECONDS));
    }
    let label = if copied {
        t!("copied")
    } else {
        t!("copy-as-curl")
    };
    ui.add(theme::ghost_button(label))
        .on_hover_text(t!("copy-as-curl-hint"))
        .clicked()
}

#[cfg(test)]
mod tests {
    use egui_kittest::kittest::{NodeT, Queryable};

    use super::super::tests::setup;

    /// R-COL-23: the request name sits on the breadcrumb's line, not above it.
    #[test]
    fn request_name_is_centred_on_the_breadcrumb_line() {
        let (mut ui, root, _key) = setup("head-align");
        let mut h = egui_kittest::Harness::builder().build_ui(|egui_ui| ui.ui(egui_ui));
        h.run();
        let text_centre = |value: &str| {
            h.query_all_by(|n| n.value().as_deref() == Some(value))
                .find(|n| n.accesskit_node().role() == egui::accesskit::Role::TextRun)
                .map(|n| n.rect().center().y)
                .expect("text on screen")
        };
        let (crumb, name) = (text_centre("C /"), text_centre("Get user"));
        assert!(
            (crumb - name).abs() < 1.0,
            "breadcrumb at {crumb}, name at {name}"
        );
        drop(h);
        std::fs::remove_dir_all(root).expect("clean up");
    }
}
