use egui::{Grid, RichText, Ui};
use tortuga_core::{Auth, vars};

use super::{FieldWidth, Lines, VarHelp, var_edit};
use crate::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Inherit,
    None,
    Basic,
    Bearer,
    Header,
}

impl Kind {
    fn of(auth: &Auth) -> Self {
        match auth {
            Auth::None => Self::None,
            Auth::Basic { .. } => Self::Basic,
            Auth::Bearer { .. } => Self::Bearer,
            Auth::Header { .. } => Self::Header,
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
            Self::Header => Some(Auth::Header {
                name: String::new(),
                value: String::new(),
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
        Auth::Header { name, .. } if name.trim().is_empty() => "a header".to_owned(),
        Auth::Header { name, value } if vars::names(value).is_empty() => {
            format!("the header {}", name.trim())
        }
        Auth::Header { name, value } => format!("the header {}: {value}", name.trim()),
    }
}

/// Whose defaults the user asked to edit from a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditDefaults {
    Project,
    /// R-SET-2.
    Workspace,
}

/// The request's Auth tab (R-HTTP-5). `inherited` is the auth it inherits, `None` for
/// the scratch request; `origin` says where from. Returns whose defaults the user asked
/// to edit.
pub fn request_auth(
    ui: &mut Ui,
    auth: &mut Option<Auth>,
    inherited: Option<&Auth>,
    origin: &str,
    help: &VarHelp,
) -> Option<EditDefaults> {
    let from_project = origin == "the project";
    let inherit = inherited.map(|_| {
        if from_project {
            "Inherit from project".to_owned()
        } else {
            format!("Inherit from {origin}")
        }
    });
    kind_picker(ui, auth, inherit.as_deref());
    ui.add_space(6.0);

    let mut edit_defaults = None;
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
                edit_defaults = ui
                    .button("Edit project defaults…")
                    .clicked()
                    .then_some(EditDefaults::Project);
            } else if origin == "the workspace" {
                edit_defaults = ui
                    .button("Edit workspace defaults…")
                    .clicked()
                    .then_some(EditDefaults::Workspace);
            } else {
                theme::hint(
                    ui,
                    "Change it with “Folder defaults…” in that folder's menu.",
                );
            }
        }
        (None, None) => theme::hint(ui, "This request sends no authentication."),
        (Some(a), _) => {
            auth_fields(ui, "request-auth", a, help, "request file");
        }
    }
    edit_defaults
}

/// Picks the kind of auth: `None` inherits, `Some(Auth::None)` sends none. `inherit` labels
/// the "inherit" choice; without one (nothing to inherit from), `None` shows as "No auth".
pub fn kind_picker(ui: &mut Ui, auth: &mut Option<Auth>, inherit: Option<&str>) {
    let current = match auth {
        None if inherit.is_some() => Kind::Inherit,
        None => Kind::None,
        Some(a) => Kind::of(a),
    };
    let mut chosen = current;
    ui.horizontal(|ui| {
        if let Some(label) = inherit {
            ui.radio_value(&mut chosen, Kind::Inherit, label);
        }
        ui.radio_value(&mut chosen, Kind::None, "No auth");
        ui.radio_value(&mut chosen, Kind::Basic, "Basic");
        ui.radio_value(&mut chosen, Kind::Bearer, "Bearer token");
        ui.radio_value(&mut chosen, Kind::Header, "Header");
    });
    if chosen != current {
        *auth = chosen.empty();
    }
}

/// Fields of a concrete auth (no kind picker). `saved_in` names the file the
/// values are written to, for the plain-text credential warning.
pub fn auth_fields(ui: &mut Ui, id: &str, auth: &mut Auth, help: &VarHelp, saved_in: &str) -> bool {
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
                changed |= field(ui, "Username", (id, "user"), username, help, width, false);
                changed |= field(
                    ui,
                    "Password",
                    (id, "password"),
                    password,
                    help,
                    width,
                    true,
                );
                credential = Some(password.clone());
            }
            Auth::Bearer { token } => {
                changed |= field(ui, "Token", (id, "token"), token, help, width, true);
                credential = Some(token.clone());
            }
            Auth::Header { name, value } => {
                changed |= field(ui, "Header", (id, "name"), name, help, width, false);
                changed |= field(ui, "Value", (id, "value"), value, help, width, true);
                credential = Some(value.clone());
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
    match auth.header() {
        Some((name, _)) => {
            theme::hint(ui, format!("A {name} header added by hand replaces this."));
        }
        None if matches!(auth, Auth::Header { .. }) => {
            theme::hint(ui, "No header is added until it has a name.");
        }
        None => {}
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
    help: &VarHelp,
    width: f32,
    sensitive: bool,
) -> bool {
    ui.label(RichText::new(label).color(theme::palette(ui).muted));
    // Literal credentials are masked; `{{variables}}` stay readable.
    let mask = sensitive && looks_like_plain_credential(value);
    let field_id = ui.make_persistent_id(id);
    let changed = var_edit(
        ui,
        field_id,
        value,
        help,
        Lines::One,
        FieldWidth::Fixed(width),
        |e| {
            e.font(egui::TextStyle::Monospace)
                .password(mask)
                .hint_text(if sensitive { "{{secret}}" } else { "" })
        },
    )
    .changed();
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
        assert_eq!(
            describe(&Auth::Header {
                name: "X-API-Key".into(),
                value: "s3cret".into()
            }),
            "the header X-API-Key",
            "R-HTTP-22: never shows a literal value"
        );
        assert_eq!(
            describe(&Auth::Header {
                name: "X-API-Key".into(),
                value: "{{key}}".into()
            }),
            "the header X-API-Key: {{key}}"
        );
    }
}
