//! Descriptions of projects, folders and requests (R-COL-22, D51): Markdown shown
//! rendered, with Edit to change the text. Images are not loaded, so a description
//! never makes the app contact a host (R-P-4); links open only when clicked.

use std::sync::{Arc, Mutex, PoisonError};

use egui::{Id, Ui};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};

use crate::theme;

const MARKDOWN_HINT: &str =
    "Markdown: # heading, **bold**, *italic*, - list, `code`, ```code block```, [link](https://…)";

/// Shows `text` rendered, or a text editor for it while `editing`, at most
/// `max_height` high. Read-only projects get no Edit button. Returns whether the text
/// changed.
pub fn description_editor(
    ui: &mut Ui,
    id_salt: &str,
    text: &mut String,
    editing: &mut bool,
    read_only: bool,
    max_height: f32,
) -> bool {
    let mut changed = false;
    // The hint names the text field for screen readers.
    let mut hint = None;
    if !read_only {
        ui.horizontal(|ui| {
            let label = if *editing { "Done" } else { "Edit" };
            if ui.add(theme::ghost_button(label)).clicked() {
                *editing = !*editing;
            }
            if *editing {
                let muted = theme::palette(ui).muted;
                hint = Some(
                    ui.label(egui::RichText::new(MARKDOWN_HINT).small().color(muted))
                        .id,
                );
            }
        });
    }
    let editing = *editing && !read_only;
    egui::ScrollArea::vertical()
        .id_salt(("description", id_salt))
        .max_height(max_height)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            if editing {
                let edit = egui::TextEdit::multiline(text)
                    .id_salt(("description-text", id_salt))
                    .font(egui::TextStyle::Monospace)
                    .desired_rows(12)
                    .desired_width(f32::INFINITY);
                let mut response = ui.add(edit);
                if let Some(hint) = hint {
                    response = response.labelled_by(hint);
                }
                changed = response.changed();
            } else if text.trim().is_empty() {
                theme::hint(
                    ui,
                    if read_only {
                        "No description."
                    } else {
                        "No description yet. Click Edit to write one in Markdown: it is saved in \
                         the file and shared through git."
                    },
                );
            } else {
                rendered(ui, id_salt, text);
            }
        });
    changed
}

/// `text` as rendered Markdown. The viewer's cache lives in egui's memory, one for the
/// app.
fn rendered(ui: &mut Ui, id_salt: &str, text: &str) {
    let cache = ui.data_mut(|d| {
        Arc::clone(
            d.get_temp_mut_or_default::<Arc<Mutex<CommonMarkCache>>>(Id::unique("markdown-cache")),
        )
    });
    // A panic while rendering leaves the cache usable: it only holds scroll positions.
    let mut cache = cache.lock().unwrap_or_else(PoisonError::into_inner);
    ui.push_id(("markdown", id_salt), |ui| {
        CommonMarkViewer::new().show(ui, &mut cache, text);
    });
}

#[cfg(test)]
mod tests {
    use egui_kittest::kittest::Queryable;

    use super::*;

    const SAMPLE: &str = "# Orders ü😀\n\nLists **all** orders, *newest* first.\n\n\
        - one\n- two\n  1. nested\n\n> a quote\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n\
        - [x] done\n- [ ] open\n\n`inline` and\n\n```json\n{\"id\": 1}\n```\n\n\
        [docs](https://example.test) ![logo](https://example.test/logo.png)\n\n---\n";

    /// R-COL-22: a description is rendered; Edit shows the text, and typing changes it.
    #[test]
    fn shows_rendered_markdown_and_edits_it() {
        let mut text = SAMPLE.to_owned();
        let mut editing = false;
        let mut changed = false;
        {
            let mut h = crate::test_host::harness(|ui| {
                changed |= description_editor(ui, "t", &mut text, &mut editing, false, 600.0);
            });
            h.run();
            h.get_by_label_contains("Orders ü😀");
            assert!(h.query_by_label_contains("# Orders").is_none(), "rendered");
            h.get_by_label("Edit").click();
            h.run();
            h.get_by_label("Done");
            h.get_by_label_contains("Markdown:");
            let field = h.get_by_role(egui::accesskit::Role::MultilineTextInput);
            field.focus();
            field.type_text("More.");
            h.run();
        }
        assert!(editing);
        assert!(changed);
        assert!(text.contains("More."));
    }

    /// R-COL-22: no Edit button in a read-only project, and an empty description says so.
    #[test]
    fn read_only_and_empty() {
        let mut text = String::new();
        let mut editing = true;
        let mut h = crate::test_host::harness(|ui| {
            description_editor(ui, "t", &mut text, &mut editing, true, 600.0);
        });
        h.run();
        assert!(h.query_by_label("Edit").is_none());
        assert!(h.query_by_label("Done").is_none());
        h.get_by_label("No description.");
    }
}
