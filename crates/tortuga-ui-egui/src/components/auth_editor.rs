use egui::{Grid, RichText, TextEdit, Ui};
use tortuga_core::{Auth, vars};

use crate::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Inherit,
    None,
    Basic,
    Bearer,
}

impl Kind {
    fn of(auth: &Auth) -> Self {
        match auth {
            Auth::None => Self::None,
            Auth::Basic { .. } => Self::Basic,
            Auth::Bearer { .. } => Self::Bearer,
        }
    }

    fn empty(self) -> Option<Auth> {
        match self {
            Self::Inherit => None,
            Self::None => Some(Auth::None),
            Self::Basic => Some(Auth::Basic {
                username: String::new(),
                password: String::new(),
            }),
            Self::Bearer => Some(Auth::Bearer {
                token: String::new(),
            }),
        }
    }
}

/// Short description of an auth, e.g. "Bearer token {{token}}".
pub fn describe(auth: &Auth) -> String {
    match auth {
        Auth::None => "no auth".to_owned(),
        Auth::Basic { username, .. } if username.is_empty() => "Basic auth".to_owned(),
        Auth::Basic { username, .. } => format!("Basic auth as {username}"),
        Auth::Bearer { token } if vars::names(token).is_empty() => "Bearer token".to_owned(),
        Auth::Bearer { token } => format!("Bearer token {token}"),
    }
}

/// The request's Auth tab (R-HTTP-5). `inherited` is the collection's default auth,
/// `None` for the scratch request. Returns true when the user asked to edit the
/// collection's defaults.
pub fn request_auth(
    ui: &mut Ui,
    auth: &mut Option<Auth>,
    inherited: Option<&Auth>,
    origin: &str,
    undefined: &[String],
) -> bool {
    let from_project = origin == "the project";
    let current = match auth {
        None if inherited.is_some() => Kind::Inherit,
        None => Kind::None,
        Some(a) => Kind::of(a),
    };
    let mut chosen = current;
    ui.horizontal(|ui| {
        if inherited.is_some() {
            let label = if from_project {
                "Inherit from project".to_owned()
            } else {
                format!("Inherit from {origin}")
            };
            ui.radio_value(&mut chosen, Kind::Inherit, label);
        }
        ui.radio_value(&mut chosen, Kind::None, "No auth");
        ui.radio_value(&mut chosen, Kind::Basic, "Basic");
        ui.radio_value(&mut chosen, Kind::Bearer, "Bearer token");
    });
    if chosen != current {
        *auth = chosen.empty();
    }
    ui.add_space(6.0);

    let mut edit_defaults = false;
    match (auth.as_mut(), inherited) {
        (None, Some(inherited)) => {
            theme::hint(
                ui,
                format!(
                    "This request uses the auth of {origin}: {}.",
                    describe(inherited)
                ),
            );
            if from_project {
                edit_defaults = ui.button("Edit project defaults…").clicked();
            } else {
                theme::hint(
                    ui,
                    "Change it with “Folder defaults…” in that folder's menu.",
                );
            }
        }
        (None, None) => theme::hint(ui, "This request sends no authentication."),
        (Some(a), _) => {
            auth_fields(ui, "request-auth", a, undefined, "request file");
        }
    }
    edit_defaults
}

/// Kind picker for a concrete auth (collection defaults: no "inherit").
/// Picks the kind of auth. `none_label` names "no auth here", e.g. "Inherit" for a folder.
pub fn kind_picker(ui: &mut Ui, auth: &mut Auth, none_label: &str) {
    let current = Kind::of(auth);
    let mut chosen = current;
    ui.horizontal(|ui| {
        ui.radio_value(&mut chosen, Kind::None, none_label);
        ui.radio_value(&mut chosen, Kind::Basic, "Basic");
        ui.radio_value(&mut chosen, Kind::Bearer, "Bearer token");
    });
    if chosen != current
        && let Some(new) = chosen.empty()
    {
        *auth = new;
    }
}

/// Fields of a concrete auth (no kind picker). `saved_in` names the file the
/// values are written to, for the plain-text credential warning.
pub fn auth_fields(
    ui: &mut Ui,
    id: &str,
    auth: &mut Auth,
    undefined: &[String],
    saved_in: &str,
) -> bool {
    let mut changed = false;
    let width = ui.available_width().min(420.0);
    let mut credential = None;
    Grid::new(id)
        .num_columns(2)
        .spacing([12.0, 8.0])
        .show(ui, |ui| match auth {
            Auth::None => {
                theme::hint(ui, "No Authorization header is added.");
                ui.end_row();
            }
            Auth::Basic { username, password } => {
                changed |= field(
                    ui,
                    "Username",
                    (id, "user"),
                    username,
                    undefined,
                    width,
                    false,
                );
                changed |= field(
                    ui,
                    "Password",
                    (id, "password"),
                    password,
                    undefined,
                    width,
                    true,
                );
                credential = Some(password.clone());
            }
            Auth::Bearer { token } => {
                changed |= field(ui, "Token", (id, "token"), token, undefined, width, true);
                credential = Some(token.clone());
            }
        });
    if let Some(value) = credential
        && looks_like_plain_credential(&value)
    {
        ui.add_space(6.0);
        theme::callout(
            ui,
            format!(
                "This value is saved in the {saved_in} and shared through git. Use a secret variable \
                 instead, e.g. {{{{token}}}} — mark it secret (🔒) in Environments to keep it on this machine."
            ),
        );
    }
    if !auth.is_none() {
        theme::hint(ui, "An Authorization header added by hand replaces this.");
    }
    changed
}

/// A credential typed as literal text rather than through a `{{variable}}`.
fn looks_like_plain_credential(value: &str) -> bool {
    !value.trim().is_empty() && vars::names(value).is_empty()
}

fn field(
    ui: &mut Ui,
    label: &str,
    id: (&str, &str),
    value: &mut String,
    undefined: &[String],
    width: f32,
    sensitive: bool,
) -> bool {
    ui.label(RichText::new(label).color(theme::palette(ui).muted));
    // Literal credentials are masked; `{{variables}}` stay readable.
    let mask = sensitive && looks_like_plain_credential(value);
    let flagged = theme::uses_undefined(value, undefined);
    let mut edit = TextEdit::singleline(value)
        .id_salt(id)
        .font(egui::TextStyle::Monospace)
        .password(mask)
        .hint_text(if sensitive { "{{secret}}" } else { "" });
    if flagged {
        edit = edit.background_color(theme::undefined_bg(ui));
    }
    let changed = theme::sized_edit(ui, width, edit).changed();
    ui.end_row();
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_credentials_are_detected() {
        assert!(looks_like_plain_credential("abc123"));
        assert!(!looks_like_plain_credential("{{token}}"));
        assert!(!looks_like_plain_credential("  "));
        assert_eq!(
            describe(&Auth::Bearer {
                token: "{{token}}".into()
            }),
            "Bearer token {{token}}"
        );
        assert_eq!(
            describe(&Auth::Bearer {
                token: "s3cret".into()
            }),
            "Bearer token",
            "never shows a literal token"
        );
    }
}
