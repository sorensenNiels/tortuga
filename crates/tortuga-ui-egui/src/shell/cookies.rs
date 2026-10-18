//! The cookie viewer: a project's cookies, removed one by one or all at once (R-CK-3, R-CK-5).

use std::path::PathBuf;

use super::{Modal, TortugaUi};
use crate::components::{CookieAction, CookiesDialog, DialogResult};

impl TortugaUi {
    pub(super) fn show_cookies(&mut self, root: PathBuf) {
        let place = self
            .state
            .collection(&root)
            .map(|c| c.name().to_owned())
            .unwrap_or_default();
        self.modal = Some(Modal::Cookies(CookiesDialog::new(root, &place)));
    }

    /// Shows the cookie viewer. Returns whether it stays open.
    pub(super) fn show_cookies_dialog(&mut self, ctx: &egui::Context, d: &CookiesDialog) -> bool {
        let jar = self.state.cookie_jar(Some(&d.root));
        let offset = self.state.host.utc_offset_minutes();
        let (result, action) = d.show(ctx, &jar.list(), tortuga_core::history::now_ms(), offset);
        match action {
            Some(CookieAction::Remove(c)) => {
                jar.remove(&c.domain, &c.path, &c.name);
                self.state.save_cookies(Some(&d.root));
            }
            Some(CookieAction::ClearAll) => self.state.clear_cookies(&d.root),
            None => {}
        }
        result == DialogResult::Open
    }
}
