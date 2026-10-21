//! Every keyboard shortcut in one table (R-COL-27): the shell handles the keys defined
//! here, and the shortcut sheet (F1) lists this table, so it can't go out of date.

use egui::gui_zoom::kb_shortcuts::{ZOOM_IN_SECONDARY, ZOOM_OUT, ZOOM_RESET};
use egui::{Key, KeyboardShortcut, ModifierNames, Modifiers};

use crate::components::ShortcutRow;
use crate::components::request_preview::CLOSE_PREVIEW;
use crate::components::script_editor::{INDENT, OUTDENT, TOGGLE_COMMENT};

pub(super) const SAVE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::S);
pub(super) const SEND: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Enter);
pub(super) const SEND_EVERYWHERE: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::Enter);
pub(super) const FIND: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::F);
pub(super) const QUICK_OPEN: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::P);
pub(super) const COMMANDS: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::K);
pub(super) const COMMANDS_TOO: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::P);
pub(super) const FOCUS_URL: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::L);
pub(super) const ENVIRONMENT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::E);
pub(super) const TOGGLE_SIDEBAR: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::COMMAND, Key::B);
pub(super) const SHORTCUTS: KeyboardShortcut = KeyboardShortcut::new(Modifiers::NONE, Key::F1);
/// Ctrl+Tab even on macOS, as browsers have it.
pub(super) const PREVIOUS_TAB: KeyboardShortcut = KeyboardShortcut::new(Modifiers::CTRL, Key::Tab);
pub(super) const NEXT_TAB: KeyboardShortcut = KeyboardShortcut::new(Modifiers::CTRL, Key::PageDown);
pub(super) const TAB_BEFORE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::CTRL, Key::PageUp);
pub(super) const CLOSE_TAB: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::W);
/// Ctrl+1 … Ctrl+9: the tab at that place; the last one goes to the last tab.
pub(super) const GO_TO_TAB: [KeyboardShortcut; 9] = [
    KeyboardShortcut::new(Modifiers::COMMAND, Key::Num1),
    KeyboardShortcut::new(Modifiers::COMMAND, Key::Num2),
    KeyboardShortcut::new(Modifiers::COMMAND, Key::Num3),
    KeyboardShortcut::new(Modifiers::COMMAND, Key::Num4),
    KeyboardShortcut::new(Modifiers::COMMAND, Key::Num5),
    KeyboardShortcut::new(Modifiers::COMMAND, Key::Num6),
    KeyboardShortcut::new(Modifiers::COMMAND, Key::Num7),
    KeyboardShortcut::new(Modifiers::COMMAND, Key::Num8),
    KeyboardShortcut::new(Modifiers::COMMAND, Key::Num9),
];

/// The shortcut sheet's lines, keys written as on `mac` or elsewhere.
pub(super) fn sheet(mac: bool) -> Vec<ShortcutRow> {
    let keys = |k: KeyboardShortcut| keys(k, mac);
    let either = |a: KeyboardShortcut, b: KeyboardShortcut| format!("{} or {}", keys(a), keys(b));
    let row = |group, keys: String, does| ShortcutRow { group, keys, does };
    let [first, .., eighth, last] = GO_TO_TAB;
    vec![
        row(
            "Anywhere",
            keys(SEND),
            "Send the request, or run the open workflow",
        ),
        row(
            "Anywhere",
            keys(SEND_EVERYWHERE),
            "Send the request to every environment",
        ),
        row("Anywhere", keys(SAVE), "Save the request or workflow"),
        row("Anywhere", keys(QUICK_OPEN), "Go to a request"),
        row("Anywhere", either(COMMANDS, COMMANDS_TOO), "Every command"),
        row("Anywhere", keys(FOCUS_URL), "Go to the URL"),
        row("Anywhere", keys(ENVIRONMENT), "Switch the environment"),
        row("Anywhere", keys(FIND), "Search in the response"),
        row("Anywhere", keys(TOGGLE_SIDEBAR), "Hide or show the sidebar"),
        row(
            "Anywhere",
            either(ZOOM_IN_SECONDARY, ZOOM_OUT),
            "Larger or smaller text",
        ),
        row("Anywhere", keys(ZOOM_RESET), "Text at its usual size"),
        row("Anywhere", keys(SHORTCUTS), "This list"),
        row("Tabs", keys(PREVIOUS_TAB), "Back to the tab shown before"),
        row(
            "Tabs",
            either(NEXT_TAB, TAB_BEFORE),
            "The next or the previous tab",
        ),
        row(
            "Tabs",
            format!("{} … {}", keys(first), keys(eighth)),
            "The tab at that place",
        ),
        row("Tabs", keys(last), "The last tab"),
        row("Tabs", keys(CLOSE_TAB), "Close the tab"),
        row(
            "Request",
            keys(CLOSE_PREVIEW),
            "Close the preview under the URL",
        ),
        row(
            "Scripts",
            either(INDENT, OUTDENT),
            "Indent or outdent the lines",
        ),
        row(
            "Scripts",
            keys(TOGGLE_COMMENT),
            "Comment the lines out or back in",
        ),
    ]
}

/// `shortcut` as `mac` or other systems write it, e.g. "Cmd+S" or "Ctrl+S". Words, not
/// symbols like ⌘, which the bundled fonts may lack.
fn keys(shortcut: KeyboardShortcut, mac: bool) -> String {
    let mut text = ModifierNames::NAMES.format(&shortcut.modifiers, mac);
    if !text.is_empty() {
        text.push('+');
    }
    text.push_str(shortcut.logical_key.symbol_or_name());
    text
}

#[cfg(test)]
mod tests {
    use super::super::TortugaUi;
    use super::*;

    /// R-COL-27: the sheet writes keys as each OS does, and only with characters the
    /// bundled fonts have.
    #[test]
    fn keys_are_written_for_the_os() {
        assert_eq!(keys(SAVE, false), "Ctrl+S");
        assert_eq!(keys(SAVE, true), "Cmd+S");
        assert_eq!(keys(COMMANDS_TOO, false), "Ctrl+Shift+P");
        assert_eq!(keys(TOGGLE_COMMENT, false), "Ctrl+/");
        assert_eq!(keys(SHORTCUTS, false), "F1");
        let rows = sheet(false);
        assert!(rows.iter().any(|r| r.keys == "Ctrl+1 … Ctrl+8"));
        assert!(
            rows.iter()
                .any(|r| r.keys == "Ctrl+L" && r.does == "Go to the URL")
        );
        for row in sheet(true).iter().chain(&rows) {
            assert!(
                row.keys.chars().all(|c| c.is_ascii() || "…−".contains(c)),
                "{}",
                row.keys
            );
        }
    }

    /// R-COL-27: F1 shows the sheet, Ctrl+L goes to the URL, Ctrl+E lists the
    /// environments, Ctrl+2 goes to the second tab and Ctrl+9 to the last.
    #[test]
    fn the_new_shortcuts_work() {
        use egui_kittest::kittest::Queryable;

        let (mut ui, root, first) = crate::shell::tests::setup("shortcuts");
        let second = crate::shell::tests::add_request(&mut ui, &root, "Second");
        let third = crate::shell::tests::add_request(&mut ui, &root, "Third");
        for key in [&first, &second, &third] {
            ui.open_tab(Some(key.clone()), true);
        }
        ui.select(Some(first.clone()));
        let url = ui.current.url.clone();
        {
            let mut h = egui_kittest::Harness::builder()
                .with_size(egui::vec2(1200.0, 900.0))
                .build_ui(|egui_ui| ui.ui(egui_ui));
            h.run();
            h.key_press(Key::Escape);
            h.run();
            h.event(egui::Event::Text("z".into()));
            h.run();
            h.key_press_modifiers(Modifiers::COMMAND, Key::L);
            h.run();
            h.event(egui::Event::Text("z".into()));
            h.run();

            h.key_press(Key::F1);
            h.run();
            h.get_by_label("Keyboard shortcuts");
            h.get_by_label("Go to the URL");
            h.get_by_label("Close").click();
            h.run();
            assert!(h.query_by_label("Go to the URL").is_none());

            h.key_press_modifiers(Modifiers::COMMAND, Key::E);
            h.run();
            assert!(
                h.get_all_by_role(egui::accesskit::Role::TextInput)
                    .any(|n| n.value().as_deref() == Some(">Environment: ")),
                "the palette, on the environments"
            );
            h.key_press(Key::Escape);
            h.run();
        }
        assert_ne!(ui.current.url, url, "Ctrl+L put the cursor in the URL");
        assert!(!ui.current.url.ends_with("zz"), "not before Ctrl+L");
        ui.tabs.keep_preview();
        let place = |n: usize| {
            let key = GO_TO_TAB[n];
            move |ui: &mut TortugaUi| {
                // A new egui context, which needs the fonts again.
                ui.styled = false;
                ui.applied_appearance = None;
                let mut h = egui_kittest::Harness::builder().build_ui(|egui_ui| ui.ui(egui_ui));
                h.run();
                h.key_press_modifiers(key.modifiers, key.logical_key);
                h.run();
            }
        };
        place(1)(&mut ui);
        assert_eq!(ui.selection, Some(second));
        place(8)(&mut ui);
        assert_eq!(ui.selection, Some(third), "Ctrl+9 is the last tab");
        std::fs::remove_dir_all(root).expect("clean up");
    }
}
