//! The script editor's keys (R-TEST-17, R-SET-16): parsed from the settings' text, kept
//! in the egui context for every script field, written for the settings file and for
//! people.

use std::sync::Arc;

use egui::{Context, Id, InputState, Key, KeyboardShortcut, ModifierNames, Modifiers};
use tortuga_core::settings::{ScriptCommand, ScriptEditorSettings};

/// What Settings and the menu call `command`.
pub fn command_name(command: ScriptCommand) -> String {
    match command {
        ScriptCommand::ToggleComment => t!("command-toggle-comment"),
        ScriptCommand::BlockComment => t!("command-block-comment"),
        ScriptCommand::MoveLinesUp => t!("command-move-lines-up"),
        ScriptCommand::MoveLinesDown => t!("command-move-lines-down"),
        ScriptCommand::CopyLinesUp => t!("command-copy-lines-up"),
        ScriptCommand::CopyLinesDown => t!("command-copy-lines-down"),
        ScriptCommand::DeleteLines => t!("command-delete-lines"),
        ScriptCommand::IndentLines => t!("command-indent-lines"),
        ScriptCommand::OutdentLines => t!("command-outdent-lines"),
        ScriptCommand::Beautify => t!("script-beautify"),
    }
}

/// The keys in use and whether brackets are paired.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptKeys {
    settings: ScriptEditorSettings,
    /// Every command's keys, those with more modifiers first: `consume_key` ignores
    /// Shift and Alt it isn't asked for, so Alt+↑ would also take Shift+Alt+↑.
    bindings: Vec<(KeyboardShortcut, ScriptCommand)>,
}

impl Default for ScriptKeys {
    fn default() -> Self {
        Self::new(&ScriptEditorSettings::default())
    }
}

impl ScriptKeys {
    pub fn new(settings: &ScriptEditorSettings) -> Self {
        let mut bindings: Vec<_> = ScriptCommand::ALL
            .iter()
            .flat_map(|&command| {
                settings
                    .keys(command)
                    .iter()
                    .filter_map(|k| parse(k))
                    .map(|k| (k, command))
                    .collect::<Vec<_>>()
            })
            .collect();
        bindings.sort_by_key(|(k, _)| std::cmp::Reverse(modifier_count(k.modifiers)));
        Self {
            settings: settings.clone(),
            bindings,
        }
    }

    pub fn close_pairs(&self) -> bool {
        self.settings.close_pairs
    }

    /// The keys of `command` that could be read.
    pub fn of(&self, command: ScriptCommand) -> Vec<KeyboardShortcut> {
        self.settings
            .keys(command)
            .iter()
            .filter_map(|k| parse(k))
            .collect()
    }

    /// The command whose key was pressed, taking the key press.
    pub fn take_pressed(&self, input: &mut InputState) -> Option<ScriptCommand> {
        self.bindings
            .iter()
            .find(|(k, _)| input.consume_shortcut(k))
            .map(|(_, command)| *command)
    }

    /// Whether a key of a command is pressed this frame, without taking it.
    fn any_pressed(&self, input: &InputState) -> bool {
        input.events.iter().any(|e| match e {
            egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } => self
                .bindings
                .iter()
                .any(|(k, _)| k.logical_key == *key && modifiers.matches_logically(k.modifiers)),
            _ => false,
        })
    }
}

fn modifier_count(m: Modifiers) -> usize {
    [m.command || m.ctrl, m.alt, m.shift]
        .iter()
        .filter(|on| **on)
        .count()
}

fn keys_id() -> Id {
    Id::unique("tortuga-script-keys")
}

fn focus_id() -> Id {
    Id::unique("tortuga-script-focus")
}

/// Puts the keys of `settings` in the context for the script fields; once per frame,
/// parsing only when they changed.
pub fn use_settings(ctx: &Context, settings: &ScriptEditorSettings) {
    let same = ctx.data(|d| {
        d.get_temp::<Arc<ScriptKeys>>(keys_id())
            .is_some_and(|k| k.settings == *settings)
    });
    if !same {
        let keys = Arc::new(ScriptKeys::new(settings));
        ctx.data_mut(|d| d.insert_temp(keys_id(), keys));
    }
}

/// The keys in use: the settings' or, before any were given, the usual ones.
pub fn current(ctx: &Context) -> Arc<ScriptKeys> {
    ctx.data(|d| d.get_temp::<Arc<ScriptKeys>>(keys_id()))
        .unwrap_or_default()
}

/// Notes that the script field `field` has the keyboard focus.
pub(super) fn note_focus(ctx: &Context, field: Id) {
    ctx.data_mut(|d| d.insert_temp(focus_id(), field));
}

/// Whether a script field has the focus and one of its keys is pressed: then the field
/// gets the key, not the app's shortcuts (R-TEST-17).
pub fn field_takes_keys(ctx: &Context) -> bool {
    let field = ctx.data(|d| d.get_temp::<Id>(focus_id()));
    if !field.is_some_and(|f| ctx.memory(|m| m.has_focus(f))) {
        return false;
    }
    // Not inside `ctx.input`: reading the keys locks the context too.
    let keys = current(ctx);
    ctx.input(|i| keys.any_pressed(i))
}

/// Reads `"Ctrl+Shift+K"`: modifiers, then a key by name or symbol. `Ctrl` (or `Cmd`)
/// is Cmd on macOS.
pub fn parse(text: &str) -> Option<KeyboardShortcut> {
    let (modifiers, key) = match text.strip_suffix("++") {
        Some(modifiers) => (modifiers, "+"),
        None => text.rsplit_once('+').unwrap_or(("", text)),
    };
    let mut m = Modifiers::NONE;
    for part in modifiers.split('+').filter(|p| !p.is_empty()) {
        m = m.plus(match part.trim() {
            "Ctrl" | "Cmd" | "Command" => Modifiers::COMMAND,
            "Alt" | "Option" => Modifiers::ALT,
            "Shift" => Modifiers::SHIFT,
            _ => return None,
        });
    }
    Some(KeyboardShortcut::new(m, Key::from_name(key.trim())?))
}

/// `shortcut` as the settings file keeps it, e.g. `"Ctrl+Shift+K"`.
pub fn key_text(shortcut: KeyboardShortcut) -> String {
    let m = shortcut.modifiers;
    let mut text = String::new();
    for (on, name) in [
        (m.command || m.ctrl || m.mac_cmd, "Ctrl+"),
        (m.alt, "Alt+"),
        (m.shift, "Shift+"),
    ] {
        if on {
            text.push_str(name);
        }
    }
    text.push_str(shortcut.logical_key.name());
    text
}

/// `shortcut` as `mac` or other systems write it, e.g. "Cmd+S" or "Ctrl+S". Words, not
/// symbols like ⌘, which the bundled fonts may lack; arrows as words for the same reason.
pub fn key_label(shortcut: KeyboardShortcut, mac: bool) -> String {
    let mut text = ModifierNames::NAMES.format(&shortcut.modifiers, mac);
    if !text.is_empty() {
        text.push('+');
    }
    text.push_str(&match shortcut.logical_key {
        Key::ArrowUp => t!("key-up"),
        Key::ArrowDown => t!("key-down"),
        Key::ArrowLeft => t!("key-left"),
        Key::ArrowRight => t!("key-right"),
        Key::Quote => "'".to_owned(),
        key => key.symbol_or_name().to_owned(),
    });
    text
}

/// The keys of `command` for people, "Ctrl+/ or Ctrl+'"; empty without keys.
pub fn keys_label(keys: &ScriptKeys, command: ScriptCommand, mac: bool) -> String {
    keys.of(command)
        .into_iter()
        .map(|k| key_label(k, mac))
        .collect::<Vec<_>>()
        .join(&t!("or-separator"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// R-SET-16: keys are read and written back the same, and odd text is skipped.
    #[test]
    fn keys_are_read_and_written() {
        for text in ["Ctrl+Shift+K", "Alt+Up", "Ctrl+Quote", "F1", "Ctrl+Plus"] {
            let key = parse(text).expect(text);
            assert_eq!(key_text(key), text);
        }
        assert_eq!(parse("Ctrl+/"), parse("Ctrl+Slash"));
        assert_eq!(parse("Ctrl++"), parse("Ctrl+Plus"));
        assert_eq!(parse("Shift+Alt+A"), parse("Alt+Shift+A"));
        for bad in ["", "Ctrl+", "Hyper+K", "Ctrl+ü😀", "Ctrl+NoSuchKey"] {
            assert_eq!(parse(bad), None, "{bad}");
        }
    }

    /// R-SET-16: every key of every scheme can be read.
    #[test]
    fn every_scheme_key_is_read() {
        use tortuga_core::settings::KeyScheme;
        for scheme in KeyScheme::ALL {
            for command in ScriptCommand::ALL {
                for key in command.default_keys(scheme) {
                    let shortcut = parse(key).unwrap_or_else(|| panic!("{key}"));
                    assert_eq!(key_text(shortcut), *key);
                }
            }
        }
    }

    /// R-TEST-17: keys are written as each OS does, in characters the fonts have.
    #[test]
    fn keys_are_labelled_for_people() {
        let keys = ScriptKeys::default();
        assert_eq!(
            keys_label(&keys, ScriptCommand::ToggleComment, false),
            "Ctrl+/ or Ctrl+'"
        );
        assert_eq!(
            keys_label(&keys, ScriptCommand::MoveLinesUp, false),
            "Alt+Up"
        );
        assert_eq!(
            keys_label(&keys, ScriptCommand::DeleteLines, true),
            "Shift+Cmd+K"
        );
    }

    /// R-TEST-17: Shift+Alt+↑ copies rather than moves, though Alt+↑ matches it too.
    #[test]
    fn keys_with_more_modifiers_win() {
        let keys = ScriptKeys::default();
        let mut input = InputState::default();
        let shift_alt = Modifiers::SHIFT.plus(Modifiers::ALT);
        input.modifiers = shift_alt;
        input.events.push(egui::Event::Key {
            key: Key::ArrowUp,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: shift_alt,
        });
        assert!(keys.any_pressed(&input));
        assert_eq!(
            keys.take_pressed(&mut input),
            Some(ScriptCommand::CopyLinesUp)
        );
        assert_eq!(keys.take_pressed(&mut input), None, "taken");
    }
}
