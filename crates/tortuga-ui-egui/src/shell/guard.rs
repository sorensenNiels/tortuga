//! The secret guard in the request editor (R-VAR-10, D76): values of the shown request
//! that look like credentials, and "Make it a secret".

use tortuga_core::Request;
use tortuga_core::check::request::{Flag, flags, make_secret, secret_name, unique_name};
use tortuga_core::workflow::path_text;

use super::{Selection, TortugaUi};
use crate::components::guard_place;

/// The flags of the request last checked, so a big body is scanned once per change, not
/// every frame (P2).
#[derive(Debug, Default)]
pub(super) struct Guard {
    /// The request, and its project's allow-list, the flags are for.
    checked: Option<(Selection, Request, Vec<String>)>,
    flags: Vec<Flag>,
}

impl TortugaUi {
    /// The shown request's values that look like credentials, without the places its
    /// project allows (`[check] allow`, R-CLI-6). None for the scratch request, which is
    /// never committed.
    pub(super) fn credential_flags(&mut self, key: &Selection) -> &[Flag] {
        let allow = key
            .as_ref()
            .and_then(|k| self.state.collection(&k.root))
            .map(|c| c.manifest.check.allow.as_slice())
            .unwrap_or_default();
        let fresh = self
            .guard
            .checked
            .as_ref()
            .is_some_and(|(k, r, a)| k == key && *r == self.current && a == allow);
        if !fresh {
            let allow = allow.to_vec();
            self.guard.flags = match key {
                None => Vec::new(),
                Some(k) => {
                    let file = path_text(&k.path);
                    flags(&self.current)
                        .into_iter()
                        .filter(|f| {
                            let place = format!("{file}: {}", f.place);
                            !allow.iter().any(|a| a.trim() == place)
                        })
                        .collect()
                }
            };
            self.guard.checked = Some((key.clone(), self.current.clone(), allow));
        }
        &self.guard.flags
    }

    /// Moves flagged value `index` into a new secret variable of the project, kept on this
    /// machine only, and puts `{{name}}` in its place in the edited request (R-VAR-10).
    pub(super) fn make_secret(&mut self, key: &Selection, index: usize) {
        let Some(k) = key.clone() else {
            return;
        };
        let Some(flag) = self.credential_flags(key).get(index).cloned() else {
            return;
        };
        let Some(c) = self.state.collection(&k.root) else {
            return;
        };
        let taken = |n: &str| {
            c.manifest.variables.contains_key(n)
                || c.manifest.secrets.iter().any(|s| s == n)
                || c.environments.iter().any(|e| e.variables.contains_key(n))
        };
        let name = unique_name(&secret_name(&self.current, &flag), taken);
        let mut request = self.current.clone();
        let Some(value) = make_secret(&mut request, &flag, &name) else {
            return;
        };
        let mut manifest = c.manifest.clone();
        manifest.secrets.push(name.clone());
        manifest.secrets.sort();
        let environments = c.environments.clone();
        let place = guard_place(&self.current, flag.field);
        let saved = self
            .state
            .run(&k.root, |c, s| c.save_config(s, &manifest, &environments));
        if saved.is_none() {
            return;
        }
        let mut values = self.state.secrets.get(&k.root).cloned().unwrap_or_default();
        values.values.insert((None, name.clone()), value);
        if let Err(e) = self.state.save_secrets(&k.root, values) {
            self.state.notify(e);
            return;
        }
        self.reload_editor(request);
        self.state
            .inform(t!("request-made-secret", place = place, name = name));
    }

    /// After a save: says so when the saved request still holds flagged values.
    pub(super) fn inform_saved_credentials(&mut self, key: &Selection) {
        let count = self.credential_flags(key).len();
        if count > 0 {
            self.state
                .inform(t!("request-saved-with-credentials", count = count));
        }
    }
}

#[cfg(test)]
mod tests {
    use tortuga_core::KeyValue;

    use super::super::tests::setup;

    const TOKEN: &str = "abc123def456ghi789";

    /// R-VAR-10: the token becomes a new secret of the project, its value goes to this
    /// machine's store, the field gets `{{name}}` and the edit is unsaved like any other.
    #[test]
    fn make_it_a_secret_moves_the_value_into_a_secret_variable() {
        let (mut ui, root, key) = setup("guard-ü😀");
        let selection = Some(key.clone());
        ui.state
            .run(&root, |c, s| {
                let mut manifest = c.manifest.clone();
                manifest
                    .variables
                    .insert("authorization".into(), "x".into());
                c.save_config(s, &manifest, &c.environments)
            })
            .expect("taken name");
        ui.current
            .headers
            .push(KeyValue::new("Authorization", format!("Bearer {TOKEN}")));
        assert_eq!(ui.credential_flags(&selection).len(), 1);

        ui.make_secret(&selection, 0);
        assert_eq!(
            ui.current.headers.last().map(|h| h.value.as_str()),
            Some("Bearer {{authorization_2}}")
        );
        let c = ui.state.collection(&root).expect("project");
        assert!(c.manifest.secrets.contains(&"authorization_2".to_owned()));
        let secrets = ui.state.secrets.get(&root).expect("secrets");
        assert_eq!(secrets.get(None, "authorization_2"), Some(TOKEN));
        assert_eq!(ui.credential_flags(&selection), []);
        assert!(
            ui.is_dirty(&key, &ui.current.clone()),
            "saved like any edit"
        );
        let toast = ui
            .state
            .toast
            .as_ref()
            .map(|t| t.text.clone())
            .unwrap_or_default();
        assert!(toast.contains("{{authorization_2}}"), "{toast}");
        assert!(!toast.contains(TOKEN), "{toast}");
        let manifest = std::fs::read_to_string(root.join("tortuga.toml")).expect("read");
        assert!(
            !manifest.contains(TOKEN),
            "the value is not in a shared file"
        );
        std::fs::remove_dir_all(root).expect("clean up");
    }

    /// R-VAR-10: saving a flagged value says so; an allowed place and the scratch request
    /// are not flagged.
    #[test]
    fn saving_says_so_and_allowed_places_are_quiet() {
        let (mut ui, root, key) = setup("guard-save");
        let selection = Some(key.clone());
        ui.current.url = format!(
            "https://a.test/{}/{TOKEN}",
            concat!("ghp_", "abcdefghijklmnopqrstuvwxyz0123")
        );
        ui.save();
        let toast = ui
            .state
            .toast
            .as_ref()
            .map(|t| t.text.clone())
            .unwrap_or_default();
        assert!(
            toast.contains("One value looks like a credential"),
            "{toast}"
        );

        let file = tortuga_core::workflow::path_text(&key.path);
        ui.state
            .run(&root, |c, s| {
                let mut manifest = c.manifest.clone();
                manifest.check.allow = vec![format!("{file}: url")];
                c.save_config(s, &manifest, &c.environments)
            })
            .expect("allow");
        assert_eq!(ui.credential_flags(&selection), [], "allowed at once");
        assert_eq!(ui.credential_flags(&None), [], "the scratch request");
        std::fs::remove_dir_all(root).expect("clean up");
    }
}
