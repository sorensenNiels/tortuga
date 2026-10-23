//! Every keyboard shortcut in one table (R-COL-27): the shell handles the keys defined
//! here, and the shortcut sheet (F1) lists this table, so it can't go out of date.

use egui::gui_zoom::kb_shortcuts::{ZOOM_IN_SECONDARY, ZOOM_OUT, ZOOM_RESET};
use egui::{Key, KeyboardShortcut, Modifiers};
use tortuga_core::settings::ScriptCommand;

use crate::components::ShortcutRow;
use crate::components::request_preview::CLOSE_PREVIEW;
use crate::components::script_editor::{INDENT, OUTDENT};
use crate::components::script_keys::{self, ScriptKeys};

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

/// The shortcut sheet's lines, keys written as on `mac` or elsewhere, with the script
/// editor's `script` keys (R-SET-16).
pub(super) fn sheet(mac: bool, script: &ScriptKeys) -> Vec<ShortcutRow> {
    let keys = |k: KeyboardShortcut| keys(k, mac);
    let either = |a: KeyboardShortcut, b: KeyboardShortcut| {
        format!("{}{}{}", keys(a), t!("or-separator"), keys(b))
    };
    let (anywhere, tabs, request, scripts) = (
        t!("shortcut-group-anywhere"),
        t!("shortcut-group-tabs"),
        t!("shortcut-group-request"),
        t!("shortcut-group-scripts"),
    );
    let row = |group: &String, keys: String, does: String| ShortcutRow {
        group: group.clone(),
        keys,
        does,
    };
    let [first, .., eighth, last] = GO_TO_TAB;
    let mut rows = vec![
        row(&anywhere, keys(SEND), t!("shortcut-send")),
        row(
            &anywhere,
            keys(SEND_EVERYWHERE),
            t!("shortcut-send-everywhere"),
        ),
        row(&anywhere, keys(SAVE), t!("shortcut-save")),
        row(&anywhere, keys(QUICK_OPEN), t!("shortcut-quick-open")),
        row(
            &anywhere,
            either(COMMANDS, COMMANDS_TOO),
            t!("shortcut-commands"),
        ),
        row(&anywhere, keys(FOCUS_URL), t!("shortcut-focus-url")),
        row(&anywhere, keys(ENVIRONMENT), t!("shortcut-environment")),
        row(&anywhere, keys(FIND), t!("shortcut-find")),
        row(&anywhere, keys(TOGGLE_SIDEBAR), t!("shortcut-sidebar")),
        row(
            &anywhere,
            either(ZOOM_IN_SECONDARY, ZOOM_OUT),
            t!("shortcut-zoom"),
        ),
        row(&anywhere, keys(ZOOM_RESET), t!("shortcut-zoom-reset")),
        row(&anywhere, keys(SHORTCUTS), t!("shortcut-this-list")),
        row(&tabs, keys(PREVIOUS_TAB), t!("shortcut-previous-tab")),
        row(&tabs, either(NEXT_TAB, TAB_BEFORE), t!("shortcut-next-tab")),
        row(
            &tabs,
            format!("{} … {}", keys(first), keys(eighth)),
            t!("shortcut-tab-at"),
        ),
        row(&tabs, keys(last), t!("shortcut-last-tab")),
        row(&tabs, keys(CLOSE_TAB), t!("shortcut-close-tab")),
        row(&request, keys(CLOSE_PREVIEW), t!("shortcut-close-preview")),
        row(&scripts, either(INDENT, OUTDENT), t!("shortcut-indent")),
    ];
    for command in ScriptCommand::ALL {
        let shown = script_keys::keys_label(script, command, mac);
        if !shown.is_empty() {
            rows.push(row(&scripts, shown, script_keys::command_name(command)));
        }
    }
    rows
}

/// The keys the app uses outside script fields and what they do there, as the sheet
/// says it, so Settings can tell when a script key is one of them (R-SET-16).
pub(crate) fn app_keys() -> Vec<(KeyboardShortcut, String)> {
    let mut keys = vec![
        (SEND, t!("shortcut-send")),
        (SEND_EVERYWHERE, t!("shortcut-send-everywhere")),
        (SAVE, t!("shortcut-save")),
        (QUICK_OPEN, t!("shortcut-quick-open")),
        (COMMANDS, t!("shortcut-commands")),
        (COMMANDS_TOO, t!("shortcut-commands")),
        (FOCUS_URL, t!("shortcut-focus-url")),
        (ENVIRONMENT, t!("shortcut-environment")),
        (FIND, t!("shortcut-find")),
        (TOGGLE_SIDEBAR, t!("shortcut-sidebar")),
        (SHORTCUTS, t!("shortcut-this-list")),
        (PREVIOUS_TAB, t!("shortcut-previous-tab")),
        (NEXT_TAB, t!("shortcut-next-tab")),
        (TAB_BEFORE, t!("shortcut-next-tab")),
        (CLOSE_TAB, t!("shortcut-close-tab")),
    ];
    keys.extend(GO_TO_TAB.iter().map(|k| (*k, t!("shortcut-tab-at"))));
    keys
}

fn keys(shortcut: KeyboardShortcut, mac: bool) -> String {
    script_keys::key_label(shortcut, mac)
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
        assert_eq!(keys(SHORTCUTS, false), "F1");
        let rows = sheet(false, &ScriptKeys::default());
        assert!(rows.iter().any(|r| r.keys == "Ctrl+1 … Ctrl+8"));
        assert!(
            rows.iter()
                .any(|r| r.keys == "Ctrl+L" && r.does == "Go to the URL")
        );
        assert!(
            rows.iter()
                .any(|r| r.keys == "Ctrl+/ or Ctrl+'" && r.does == "Comment lines out or in")
        );
        for (_, does) in app_keys() {
            assert!(
                rows.iter().any(|r| r.does == does),
                "{does} is on the sheet"
            );
        }
        for row in sheet(true, &ScriptKeys::default()).iter().chain(&rows) {
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
