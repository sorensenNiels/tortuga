//! The Script editor page of Settings (R-SET-16): the editor whose keys to start from,
//! every line command's keys, changed by pressing new ones, and whether brackets and quotes are paired (R-TEST-17).

use egui::{Event, Key, KeyboardShortcut, Modifiers, RichText, Ui};
use tortuga_core::settings::{KeyScheme, ScriptCommand, ScriptEditorSettings};

use super::layout::{About, group};
use super::network::switch_row;
use crate::components::script_keys::{command_name, key_label, key_text, parse};
use crate::theme;

/// Where a recorded key goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Slot {
    /// In place of the command's key at this place.
    Replace(usize),
    /// As one more key.
    Add,
}

/// What Settings says under a command after a key was pressed for it.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Note {
    /// The key wasn't taken, and why.
    Refused(String),
    /// The key was taken; it does something else outside script fields.
    Elsewhere(String),
}

/// A key pressed while recording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pressed {
    Key(KeyboardShortcut),
    /// Escape: keep the keys as they are.
    Cancel,
}

/// The tab's own state: the key being recorded and the last note.
#[derive(Debug, Clone, Default)]
pub(super) struct ScriptEditorTab {
    recording: Option<(ScriptCommand, Slot)>,
    note: Option<(ScriptCommand, Note)>,
}

impl ScriptEditorTab {
    pub(super) fn show(&mut self, ui: &mut Ui, settings: &mut ScriptEditorSettings) {
        let mac = ui.ctx().os() == egui::os::OperatingSystem::Mac;
        if let Some((command, slot)) = self.recording
            && let Some(pressed) = take_key(ui)
        {
            self.recording = None;
            self.note = match pressed {
                Pressed::Key(key) => record(settings, command, slot, key, mac),
                Pressed::Cancel => None,
            }
            .map(|note| (command, note));
        }
        group(ui, &t!("settings-group-typing"), None, |rows| {
            switch_row(
                rows,
                &t!("settings-close-pairs"),
                About::both(
                    &t!("settings-close-pairs-about"),
                    &t!("settings-close-pairs-hint"),
                ),
                &mut settings.close_pairs,
            );
        });
        let note = (t!("settings-keys-about"), t!("settings-keys-hint"));
        group(ui, &t!("settings-keys"), Some((&note.0, &note.1)), |rows| {
            let changed = ScriptCommand::ALL.into_iter().any(|c| settings.changed(c));
            let replaces = t!("settings-scheme-replaces");
            let about = if changed {
                About::short(&replaces)
            } else {
                About::default()
            };
            rows.row(&t!("settings-keys-like"), about, |ui, name| {
                self.scheme_choice(ui, settings, changed, name);
            });
            for command in ScriptCommand::ALL {
                rows.row(&command_name(command), About::default(), |ui, _| {
                    ui.horizontal_wrapped(|ui| self.keys_row(ui, settings, command, mac));
                    if let Some((_, note)) = self.note.as_ref().filter(|(c, _)| *c == command) {
                        match note {
                            Note::Refused(text) => theme::error(ui, text),
                            Note::Elsewhere(text) => theme::warning(ui, text),
                        }
                    }
                });
            }
        });
    }

    /// The editor whose keys to start from; choosing one drops the changed keys.
    /// `changed`: some keys differ from the scheme's. `name` names the list.
    fn scheme_choice(
        &mut self,
        ui: &mut Ui,
        settings: &mut ScriptEditorSettings,
        changed: bool,
        name: egui::Id,
    ) {
        ui.horizontal(|ui| {
            let mut chosen = settings.scheme;
            let shown = if changed {
                t!("settings-scheme-changed", scheme = settings.scheme.name())
            } else {
                settings.scheme.name().to_owned()
            };
            egui::ComboBox::from_id_salt("script-key-scheme")
                .selected_text(shown)
                .show_ui(ui, |ui| {
                    for scheme in KeyScheme::ALL {
                        ui.selectable_value(&mut chosen, scheme, scheme.name());
                    }
                })
                .response
                .labelled_by(name);
            if chosen != settings.scheme {
                self.use_scheme(settings, chosen);
            }
            if changed
                && ui
                    .add(theme::ghost_button(t!("settings-reset-all")))
                    .on_hover_text(t!("settings-reset-all-hint"))
                    .clicked()
            {
                self.use_scheme(settings, settings.scheme);
            }
        });
    }

    fn use_scheme(&mut self, settings: &mut ScriptEditorSettings, scheme: KeyScheme) {
        settings.use_scheme(scheme);
        self.recording = None;
        self.note = None;
    }

    /// The keys of `command`, each to click and change or remove, "+" for one more, and
    /// "Reset" when they were changed.
    fn keys_row(
        &mut self,
        ui: &mut Ui,
        settings: &mut ScriptEditorSettings,
        command: ScriptCommand,
        mac: bool,
    ) {
        let keys = settings.keys(command);
        let current = self.recording;
        let recording = |slot| current == Some((command, slot));
        let mut remove = None;
        for (n, text) in keys.iter().enumerate() {
            let shown = if recording(Slot::Replace(n)) {
                t!("settings-press-key")
            } else {
                parse(text).map_or_else(
                    || t!("settings-key-unknown", key = text.as_str()),
                    |k| key_label(k, mac),
                )
            };
            let button = egui::Button::new(RichText::new(shown).monospace())
                .selected(recording(Slot::Replace(n)));
            if ui
                .add(button)
                .on_hover_text(t!("settings-key-change-hint"))
                .clicked()
            {
                self.start(command, Slot::Replace(n));
            }
            if ui
                .add(theme::icon_button(ui, "🗙"))
                .on_hover_text(t!("settings-key-remove"))
                .clicked()
            {
                remove = Some(n);
            }
        }
        let add = if recording(Slot::Add) {
            t!("settings-press-key")
        } else {
            "+".to_owned()
        };
        if ui
            .add(egui::Button::new(add).selected(recording(Slot::Add)))
            .on_hover_text(t!("settings-key-add"))
            .clicked()
        {
            self.start(command, Slot::Add);
        }
        if settings.changed(command)
            && ui
                .add(theme::ghost_button(t!("reset")))
                .on_hover_text(t!("settings-key-reset-hint"))
                .clicked()
        {
            settings.set_keys(command, settings.usual_keys(command));
            self.note = None;
        }
        if let Some(n) = remove {
            let mut keys = keys;
            keys.remove(n);
            settings.set_keys(command, keys);
            self.note = None;
        }
    }

    fn start(&mut self, command: ScriptCommand, slot: Slot) {
        self.recording = Some((command, slot));
        self.note = None;
    }
}

/// The key pressed this frame, taken so nothing else acts on it; `None` when no key was
/// pressed.
fn take_key(ui: &Ui) -> Option<Pressed> {
    ui.input_mut(|i| {
        let n = i
            .events
            .iter()
            .position(|e| matches!(e, Event::Key { pressed: true, .. }))?;
        let Event::Key { key, modifiers, .. } = i.events.remove(n) else {
            return None;
        };
        if key == Key::Escape && modifiers.is_none() {
            return Some(Pressed::Cancel);
        }
        let mut m = Modifiers::NONE;
        if modifiers.command || modifiers.ctrl || modifiers.mac_cmd {
            m = m.plus(Modifiers::COMMAND);
        }
        if modifiers.alt {
            m = m.plus(Modifiers::ALT);
        }
        if modifiers.shift {
            m = m.plus(Modifiers::SHIFT);
        }
        Some(Pressed::Key(KeyboardShortcut::new(m, key)))
    })
}

/// Gives `command` the key `key` in `slot`, unless it can't have it. Returns what to say
/// about it: why it was refused, or what it does elsewhere.
fn record(
    settings: &mut ScriptEditorSettings,
    command: ScriptCommand,
    slot: Slot,
    key: KeyboardShortcut,
    mac: bool,
) -> Option<Note> {
    let text = key_text(key);
    let label = key_label(key, mac);
    let function_key = key
        .logical_key
        .name()
        .strip_prefix('F')
        .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
    if !(key.modifiers.command || key.modifiers.alt || function_key) {
        return Some(Note::Refused(t!(
            "settings-key-needs-modifier",
            key = label.as_str()
        )));
    }
    let same = |k: &String| parse(k).is_some_and(|k| key_text(k) == text);
    let taken = ScriptCommand::ALL
        .into_iter()
        .find(|&c| settings.keys(c).iter().any(same));
    match taken {
        Some(other) if other != command => {
            return Some(Note::Refused(t!(
                "settings-key-taken",
                key = label.as_str(),
                command = command_name(other)
            )));
        }
        Some(_) => return None,
        None => {}
    }
    let mut keys = settings.keys(command);
    match slot {
        Slot::Replace(n) if n < keys.len() => keys[n].clone_from(&text),
        Slot::Replace(_) | Slot::Add => keys.push(text.clone()),
    }
    settings.set_keys(command, keys);
    crate::shell::app_keys()
        .into_iter()
        .find(|(k, _)| key_text(*k) == text)
        .map(|(_, does)| {
            Note::Elsewhere(t!(
                "settings-key-elsewhere",
                key = label.as_str(),
                does = does
            ))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(modifiers: Modifiers, key: Key) -> KeyboardShortcut {
        KeyboardShortcut::new(modifiers, key)
    }

    /// R-SET-16: a key goes in place of another or as one more; one another command has,
    /// or one without Ctrl or Alt, is refused; one the app uses elsewhere gets a note.
    #[test]
    fn keys_are_recorded_refused_or_noted() {
        let mut s = ScriptEditorSettings::default();
        let delete = ScriptCommand::DeleteLines;
        let d = key(Modifiers::COMMAND, Key::D);
        assert_eq!(record(&mut s, delete, Slot::Replace(0), d, false), None);
        assert_eq!(s.keys(delete), ["Ctrl+D"]);

        let up = key(Modifiers::ALT, Key::ArrowUp);
        assert_eq!(
            record(&mut s, delete, Slot::Add, up, false),
            Some(Note::Refused("Alt+Up is already “Move lines up”.".into()))
        );
        let plain = key(Modifiers::NONE, Key::A);
        assert!(matches!(
            record(&mut s, delete, Slot::Add, plain, false),
            Some(Note::Refused(_))
        ));
        assert_eq!(s.keys(delete), ["Ctrl+D"], "unchanged");

        let save = key(Modifiers::COMMAND, Key::S);
        assert_eq!(
            record(&mut s, delete, Slot::Add, save, false),
            Some(Note::Elsewhere(
                "Outside script fields, Ctrl+S still does “Save the request or workflow”.".into()
            ))
        );
        assert_eq!(s.keys(delete), ["Ctrl+D", "Ctrl+S"]);
        assert_eq!(record(&mut s, delete, Slot::Add, d, false), None, "has it");
        assert_eq!(s.keys(delete), ["Ctrl+D", "Ctrl+S"]);
        let f2 = key(Modifiers::NONE, Key::F2);
        assert_eq!(record(&mut s, delete, Slot::Replace(1), f2, false), None);
        assert_eq!(s.keys(delete), ["Ctrl+D", "F2"]);
    }
}
