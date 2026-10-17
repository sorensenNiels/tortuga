use std::path::PathBuf;

use egui::{Context, Ui};
use tortuga_core::{Auth, KeyValue};

use super::auth_editor::{auth_fields, kind_picker};
use super::{DialogResult, dialog, key_value_table};
use crate::state::AppState;
use crate::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Headers,
    Auth,
}

/// Headers and auth every request in a collection inherits (R-COL-4, R-HTTP-5).
#[derive(Debug, Clone)]
pub struct DefaultsDialog {
    pub root: PathBuf,
    headers: Vec<KeyValue>,
    auth: Auth,
    tab: Tab,
}

impl DefaultsDialog {
    pub fn new(state: &AppState, root: PathBuf) -> Self {
        let (headers, auth) = state
            .collection(&root)
            .map(|c| (c.manifest.headers.clone(), c.manifest.auth.clone()))
            .unwrap_or_default();
        Self {
            root,
            headers,
            auth,
            tab: Tab::Headers,
        }
    }

    /// Opens on the Auth tab (from a request's "Edit collection defaults…").
    #[must_use]
    pub fn on_auth(mut self) -> Self {
        self.tab = Tab::Auth;
        self
    }

    /// Done with the new (headers, auth).
    pub fn show(&mut self, ctx: &Context, state: &AppState) -> DialogResult<(Vec<KeyValue>, Auth)> {
        let Some(c) = state.collection(&self.root) else {
            return DialogResult::Close;
        };
        let title = format!("Project defaults — {}", c.name());
        let read_only = c.read_only;
        let undefined = undefined_names(state, &self.root, &self.headers, &self.auth);
        dialog(ctx, "collection-defaults", &title, 640.0, false, |ui| {
            theme::hint(
                ui,
                "Every request in this project inherits these. A request can replace a header by \
                 adding one with the same name (or switch it off), and can choose its own auth.",
            );
            ui.add_space(8.0);
            let header_count = self.headers.iter().filter(|h| h.is_active()).count();
            theme::tabs(
                ui,
                &mut self.tab,
                &[
                    (
                        Tab::Headers,
                        "Headers",
                        (header_count > 0).then(|| header_count.to_string()),
                    ),
                    (
                        Tab::Auth,
                        "Auth",
                        (!self.auth.is_none()).then(|| "⏺".to_owned()),
                    ),
                ],
            );
            match self.tab {
                Tab::Headers => {
                    key_value_table(
                        ui,
                        "default-headers",
                        &mut self.headers,
                        "Header",
                        &undefined,
                    );
                }
                Tab::Auth => auth_tab(ui, &mut self.auth, &undefined),
            }
            ui.add_space(16.0);
            let (cancel, save) = ui
                .with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let save = ui
                        .add_enabled(!read_only, theme::primary_button(ui, "Save"))
                        .clicked();
                    (ui.button("Cancel").clicked(), save)
                })
                .inner;
            if save {
                DialogResult::Done((self.headers.clone(), self.auth.clone()))
            } else if cancel {
                DialogResult::Close
            } else {
                DialogResult::Open
            }
        })
    }
}

fn auth_tab(ui: &mut Ui, auth: &mut Auth, undefined: &[String]) {
    kind_picker(ui, auth);
    ui.add_space(6.0);
    auth_fields(
        ui,
        "default-auth",
        auth,
        undefined,
        "project file (tortuga.toml)",
    );
}

/// Variables used in the defaults that the active environment does not define (R-VAR-5).
fn undefined_names(
    state: &AppState,
    root: &std::path::Path,
    headers: &[KeyValue],
    auth: &Auth,
) -> Vec<String> {
    let ctx = state.context(Some(root));
    let mut texts: Vec<&str> = auth.fields();
    for h in headers.iter().filter(|h| h.enabled) {
        texts.extend([h.name.as_str(), h.value.as_str()]);
    }
    let mut names: Vec<String> = texts
        .into_iter()
        .flat_map(tortuga_core::vars::names)
        .filter(|n| !ctx.is_defined(n))
        .map(str::to_owned)
        .collect();
    names.sort();
    names.dedup();
    names
}

#[cfg(test)]
mod tests {
    use egui_kittest::kittest::Queryable;
    use tortuga_core::Collection;
    use tortuga_core::storage::FsStorage;

    use super::*;
    use crate::test_host::TestHost;

    #[test]
    fn typing_a_default_header_and_saving() {
        let root = std::env::temp_dir().join(format!("tortuga-defaults-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        Collection::create(&FsStorage, &root, "C").expect("create");
        let state = AppState::new(Box::new(TestHost {
            open: vec![root.clone()],
            ..TestHost::default()
        }));
        let mut dialog = DefaultsDialog::new(&state, root.clone());
        let mut result = None;
        {
            let mut h = crate::test_host::harness(|ui| {
                if let DialogResult::Done(r) = dialog.show(ui.ctx(), &state) {
                    result = Some(r);
                }
            });
            h.run();
            let field = h
                .get_all_by_role(egui::accesskit::Role::TextInput)
                .next()
                .expect("header name field");
            field.focus();
            field.type_text("X-Team");
            h.run();
            h.get_by_label("Save").click();
            h.run();
        }
        let (headers, auth) = result.expect("saved");
        assert_eq!(headers, [KeyValue::new("X-Team", "")]);
        assert_eq!(auth, Auth::None);
        std::fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn choosing_bearer_and_typing_a_token() {
        let root =
            std::env::temp_dir().join(format!("tortuga-defaults-auth-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        Collection::create(&FsStorage, &root, "C").expect("create");
        let state = AppState::new(Box::new(TestHost {
            open: vec![root.clone()],
            ..TestHost::default()
        }));
        let mut dialog = DefaultsDialog::new(&state, root.clone()).on_auth();
        let mut result = None;
        {
            let mut h = crate::test_host::harness(|ui| {
                if let DialogResult::Done(r) = dialog.show(ui.ctx(), &state) {
                    result = Some(r);
                }
            });
            h.run();
            h.get_by_label("Bearer token").click();
            h.run();
            let field = h
                .get_all_by_role(egui::accesskit::Role::TextInput)
                .next()
                .expect("token field");
            field.click();
            h.run();
            h.get_all_by_role(egui::accesskit::Role::TextInput)
                .next()
                .expect("token field")
                .type_text("{{token}}");
            h.run();
            h.get_by_label("Save").click();
            h.run();
        }
        let (_, auth) = result.expect("saved");
        assert_eq!(
            auth,
            Auth::Bearer {
                token: "{{token}}".into()
            }
        );
        std::fs::remove_dir_all(root).expect("clean up");
    }
}
