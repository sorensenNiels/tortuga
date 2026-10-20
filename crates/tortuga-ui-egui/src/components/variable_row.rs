//! Cells shared by the variable tables of projects and workspaces (R-VAR-2, R-VAR-3, R-WS-8).

use egui::{AsIdSalt, RichText, TextEdit, Ui};

use crate::theme;

/// Room the 🔒 toggle and remove button take next to the name.
const BUTTONS_WIDTH: f32 = 82.0;

/// Hover texts of the 🔒 toggle, which say where the values go.
#[derive(Debug, Clone, Copy)]
pub(super) struct SecretHelp {
    pub secret: &'static str,
    pub shared: &'static str,
}

/// A variable's name, its 🔒 secret toggle and a remove button, `width` wide in all.
/// Returns whether remove was clicked.
pub(super) fn name_cell(
    ui: &mut Ui,
    id: impl AsIdSalt,
    width: f32,
    name: &mut String,
    secret: &mut bool,
    help: SecretHelp,
) -> bool {
    ui.horizontal(|ui| {
        theme::sized_edit(
            ui,
            width - BUTTONS_WIDTH,
            TextEdit::singleline(name)
                .id_salt(id)
                .hint_text("name")
                .font(egui::TextStyle::Monospace),
        );
        let p = theme::palette(ui);
        let lock = RichText::new("🔒").size(12.0).color(if *secret {
            p.accent
        } else {
            p.muted.gamma_multiply(0.6)
        });
        let toggle = ui
            .add(egui::Button::selectable(*secret, lock).frame_when_inactive(false))
            .on_hover_text(if *secret { help.secret } else { help.shared });
        if toggle.clicked() {
            *secret = !*secret;
        }
        ui.add(theme::icon_button(ui, "🗙"))
            .on_hover_text("Remove this variable")
            .clicked()
    })
    .inner
}

/// A blank input that adds a row when Enter is pressed or the field is left. Returns the
/// trimmed name to add.
pub(super) fn new_row_input(
    ui: &mut Ui,
    id: &str,
    width: f32,
    value: &mut String,
    hint: &str,
) -> Option<String> {
    let response = theme::sized_edit(
        ui,
        width,
        TextEdit::singleline(value)
            .id_salt(id)
            .hint_text(hint.to_owned()),
    );
    let name = value.trim().to_owned();
    if response.lost_focus() && !name.is_empty() {
        value.clear();
        return Some(name);
    }
    None
}
