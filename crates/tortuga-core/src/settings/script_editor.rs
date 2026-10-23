//! The script editor's keys and pairs (R-TEST-17, R-SET-16).
//!
//! Keys are kept as text such as `"Ctrl+Shift+K"`: modifiers (`Ctrl`, which is Cmd on
//! macOS, `Alt`, `Shift`) and a key name as the UI's toolkit writes it (`K`, `Slash`,
//! `Up`). The engine doesn't read them; the UI parses them and skips any it can't.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// A command of the script editor that has keys (R-TEST-17).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScriptCommand {
    ToggleComment,
    BlockComment,
    MoveLinesUp,
    MoveLinesDown,
    CopyLinesUp,
    CopyLinesDown,
    DeleteLines,
    IndentLines,
    OutdentLines,
    Beautify,
}

impl ScriptCommand {
    /// Every command, in the order Settings and the field's menu list them.
    pub const ALL: [Self; 10] = [
        Self::ToggleComment,
        Self::BlockComment,
        Self::MoveLinesUp,
        Self::MoveLinesDown,
        Self::CopyLinesUp,
        Self::CopyLinesDown,
        Self::DeleteLines,
        Self::IndentLines,
        Self::OutdentLines,
        Self::Beautify,
    ];

    /// The name the settings file keeps it under.
    pub fn id(self) -> &'static str {
        match self {
            Self::ToggleComment => "toggle-comment",
            Self::BlockComment => "block-comment",
            Self::MoveLinesUp => "move-lines-up",
            Self::MoveLinesDown => "move-lines-down",
            Self::CopyLinesUp => "copy-lines-up",
            Self::CopyLinesDown => "copy-lines-down",
            Self::DeleteLines => "delete-lines",
            Self::IndentLines => "indent-lines",
            Self::OutdentLines => "outdent-lines",
            Self::Beautify => "beautify",
        }
    }

    /// Its usual keys in `scheme`. VS Code's get Ctrl+' for comments too, for keyboards
    /// where `/` needs Shift. Indenting keeps Ctrl+] and Ctrl+[ in every scheme, as
    /// those keys have no other use in a script field (Tab and Shift+Tab always work).
    /// Block comments use `?`: with Shift held, that is the key the toolkit reports for
    /// `/` on most layouts.
    pub fn default_keys(self, scheme: KeyScheme) -> &'static [&'static str] {
        match scheme {
            KeyScheme::VsCode => match self {
                Self::ToggleComment => &["Ctrl+Slash", "Ctrl+Quote"],
                Self::BlockComment => &["Alt+Shift+A"],
                Self::MoveLinesUp => &["Alt+Up"],
                Self::MoveLinesDown => &["Alt+Down"],
                Self::CopyLinesUp => &["Alt+Shift+Up"],
                Self::CopyLinesDown => &["Alt+Shift+Down"],
                Self::DeleteLines => &["Ctrl+Shift+K"],
                Self::IndentLines => &["Ctrl+CloseBracket"],
                Self::OutdentLines => &["Ctrl+OpenBracket"],
                Self::Beautify => &["Alt+Shift+F"],
            },
            KeyScheme::JetBrains => match self {
                Self::ToggleComment => &["Ctrl+Slash"],
                Self::BlockComment => &["Ctrl+Shift+Questionmark"],
                Self::MoveLinesUp => &["Alt+Shift+Up"],
                Self::MoveLinesDown => &["Alt+Shift+Down"],
                Self::CopyLinesUp => &[],
                Self::CopyLinesDown => &["Ctrl+D"],
                Self::DeleteLines => &["Ctrl+Y"],
                Self::IndentLines => &["Ctrl+CloseBracket"],
                Self::OutdentLines => &["Ctrl+OpenBracket"],
                Self::Beautify => &["Ctrl+Alt+L"],
            },
            KeyScheme::Sublime => match self {
                Self::ToggleComment => &["Ctrl+Slash"],
                Self::BlockComment => &["Ctrl+Shift+Questionmark"],
                Self::MoveLinesUp => &["Ctrl+Shift+Up"],
                Self::MoveLinesDown => &["Ctrl+Shift+Down"],
                Self::CopyLinesUp => &[],
                Self::CopyLinesDown => &["Ctrl+Shift+D"],
                Self::DeleteLines => &["Ctrl+Shift+K"],
                Self::IndentLines => &["Ctrl+CloseBracket"],
                Self::OutdentLines => &["Ctrl+OpenBracket"],
                // Sublime Text has no formatter of its own; the usual plugin's key.
                Self::Beautify => &["Ctrl+Alt+F"],
            },
            KeyScheme::Eclipse => match self {
                Self::ToggleComment => &["Ctrl+Slash", "Ctrl+7"],
                Self::BlockComment => &["Ctrl+Shift+Questionmark"],
                Self::MoveLinesUp => &["Alt+Up"],
                Self::MoveLinesDown => &["Alt+Down"],
                Self::CopyLinesUp => &["Ctrl+Alt+Up"],
                Self::CopyLinesDown => &["Ctrl+Alt+Down"],
                Self::DeleteLines => &["Ctrl+D"],
                Self::IndentLines => &["Ctrl+CloseBracket"],
                Self::OutdentLines => &["Ctrl+OpenBracket"],
                Self::Beautify => &["Ctrl+Shift+F"],
            },
        }
    }
}

/// Which editor's keys the script editor starts from (R-SET-16).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KeyScheme {
    JetBrains,
    Sublime,
    Eclipse,
    /// Also what a settings file from a newer version with a scheme this one doesn't
    /// know gets (`other` has to be the last variant).
    #[default]
    #[serde(rename = "vscode", other)]
    VsCode,
}

impl KeyScheme {
    /// Every scheme, in the order Settings lists them.
    pub const ALL: [Self; 4] = [Self::VsCode, Self::JetBrains, Self::Sublime, Self::Eclipse];

    /// What Settings calls it.
    pub fn name(self) -> &'static str {
        match self {
            Self::VsCode => "VS Code",
            Self::JetBrains => "JetBrains (IntelliJ, WebStorm, …)",
            Self::Sublime => "Sublime Text",
            Self::Eclipse => "Eclipse",
        }
    }
}

/// The script editor's settings (R-SET-16).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ScriptEditorSettings {
    /// Typing an opening bracket or quote puts in its partner (R-TEST-17).
    pub close_pairs: bool,
    /// The editor whose keys the commands have unless changed.
    #[serde(skip_serializing_if = "is_vs_code")]
    pub scheme: KeyScheme,
    /// Keys of the commands whose keys were changed, by [`ScriptCommand::id`]; an empty
    /// list means no keys. Kept as text, so a file from a newer version with commands
    /// this one doesn't know still loads.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub keys: BTreeMap<String, Vec<String>>,
}

impl Default for ScriptEditorSettings {
    fn default() -> Self {
        Self {
            close_pairs: true,
            scheme: KeyScheme::VsCode,
            keys: BTreeMap::new(),
        }
    }
}

impl ScriptEditorSettings {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// The keys `command` has: changed ones, else its usual ones.
    pub fn keys(&self, command: ScriptCommand) -> Vec<String> {
        self.keys
            .get(command.id())
            .cloned()
            .unwrap_or_else(|| self.usual_keys(command))
    }

    /// The keys `command` has in the chosen scheme.
    pub fn usual_keys(&self, command: ScriptCommand) -> Vec<String> {
        command
            .default_keys(self.scheme)
            .iter()
            .map(|k| (*k).to_owned())
            .collect()
    }

    /// Starts from `scheme`'s keys, dropping every changed one.
    pub fn use_scheme(&mut self, scheme: KeyScheme) {
        self.scheme = scheme;
        self.keys.clear();
    }

    /// Gives `command` these keys; its usual ones are not written.
    pub fn set_keys(&mut self, command: ScriptCommand, keys: Vec<String>) {
        if keys
            .iter()
            .map(String::as_str)
            .eq(command.default_keys(self.scheme).iter().copied())
        {
            self.keys.remove(command.id());
        } else {
            self.keys.insert(command.id().to_owned(), keys);
        }
    }

    /// Whether `command` has keys other than its usual ones.
    pub fn changed(&self, command: ScriptCommand) -> bool {
        self.keys.contains_key(command.id())
    }
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's skip_serializing_if passes a reference"
)]
fn is_vs_code(scheme: &KeyScheme) -> bool {
    *scheme == KeyScheme::VsCode
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AppSettings;

    /// R-SET-16: only changed keys are written, and the usual ones come back.
    #[test]
    fn changed_keys_round_trip_and_usual_ones_are_not_written() {
        let mut settings = AppSettings::default();
        assert!(
            !toml::to_string(&settings)
                .expect("toml")
                .contains("script_editor")
        );
        let s = &mut settings.script_editor;
        assert_eq!(
            s.keys(ScriptCommand::ToggleComment),
            ["Ctrl+Slash", "Ctrl+Quote"]
        );
        s.set_keys(ScriptCommand::DeleteLines, vec!["Ctrl+D".into()]);
        s.set_keys(ScriptCommand::Beautify, Vec::new());
        s.close_pairs = false;
        let text = toml::to_string(&settings).expect("toml");
        let back: AppSettings = toml::from_str(&text).expect("parse");
        assert_eq!(back, settings);
        let s = &back.script_editor;
        assert_eq!(s.keys(ScriptCommand::DeleteLines), ["Ctrl+D"]);
        assert!(s.keys(ScriptCommand::Beautify).is_empty(), "no keys");
        assert!(s.changed(ScriptCommand::Beautify));

        let mut s = back.script_editor;
        s.set_keys(ScriptCommand::DeleteLines, vec!["Ctrl+Shift+K".into()]);
        assert!(
            !s.changed(ScriptCommand::DeleteLines),
            "back to its usual key"
        );
    }

    /// R-SET-16: keys of a command this version doesn't know are kept.
    #[test]
    fn keys_of_unknown_commands_load() {
        let s: ScriptEditorSettings =
            toml::from_str("[keys]\nfold-all = [\"Ctrl+ü😀\"]\n").expect("parse");
        assert!(s.close_pairs);
        assert_eq!(s.keys["fold-all"], ["Ctrl+ü😀"]);
    }

    /// R-SET-16: a scheme is written unless it is VS Code, choosing one drops changed
    /// keys, and one this version doesn't know loads as VS Code.
    #[test]
    fn schemes_round_trip_and_replace_changed_keys() {
        let mut s = ScriptEditorSettings::default();
        s.set_keys(ScriptCommand::DeleteLines, vec!["Ctrl+E".into()]);
        s.use_scheme(KeyScheme::JetBrains);
        assert!(!s.changed(ScriptCommand::DeleteLines));
        assert_eq!(s.keys(ScriptCommand::DeleteLines), ["Ctrl+Y"]);
        s.set_keys(ScriptCommand::DeleteLines, vec!["Ctrl+Y".into()]);
        assert!(
            !s.changed(ScriptCommand::DeleteLines),
            "the scheme's own key"
        );
        s.set_keys(ScriptCommand::Beautify, vec!["Alt+Shift+F".into()]);

        let text = toml::to_string(&s).expect("toml");
        assert!(text.contains("scheme = \"jetbrains\""), "{text}");
        let back: ScriptEditorSettings = toml::from_str(&text).expect("parse");
        assert_eq!(back, s);

        s.use_scheme(KeyScheme::VsCode);
        assert!(s.is_default());
        assert!(!toml::to_string(&s).expect("toml").contains("scheme"));

        let newer: ScriptEditorSettings =
            toml::from_str("scheme = \"emacs-ü😀\"\n").expect("parse");
        assert_eq!(newer.scheme, KeyScheme::VsCode);
    }

    /// R-SET-16: within a scheme no key belongs to two commands.
    #[test]
    fn no_scheme_gives_a_key_to_two_commands() {
        for scheme in KeyScheme::ALL {
            let mut seen = std::collections::HashSet::new();
            for command in ScriptCommand::ALL {
                for key in command.default_keys(scheme) {
                    assert!(seen.insert(*key), "{key} twice in {}", scheme.name());
                }
            }
        }
    }
}
