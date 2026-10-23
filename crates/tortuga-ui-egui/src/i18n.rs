//! The UI in the user's language (R-I18N-1 to R-I18N-3, D64): Fluent messages from
//! `locales/<language>.ftl`, built into the binary. English is the source and the
//! fallback for any message a translation lacks.
//!
//! Text is looked up with the [`t!`] macro by message id, with named arguments:
//! `t!("send")`, `t!("workflow-captures", count = n)`.

use std::sync::LazyLock;

use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::{FluentArgs, FluentResource};
use tortuga_core::settings::Language;

/// The text of message `id` in the current language, with `name = value` arguments.
macro_rules! t {
    ($id:literal) => {
        $crate::i18n::text($id, None)
    };
    ($id:literal, $($name:ident = $value:expr),+ $(,)?) => {{
        let mut args = $crate::i18n::Args::new();
        $(args.set(stringify!($name), $value);)+
        $crate::i18n::text($id, Some(&args))
    }};
}

pub use fluent_bundle::FluentArgs as Args;

const ENGLISH: &str = include_str!("../locales/en.ftl");
const DANISH: &str = include_str!("../locales/da.ftl");

struct Bundles {
    english: FluentBundle<FluentResource>,
    danish: FluentBundle<FluentResource>,
}

static BUNDLES: LazyLock<Bundles> = LazyLock::new(|| Bundles {
    english: bundle(unic_langid::langid!("en"), ENGLISH),
    danish: bundle(unic_langid::langid!("da"), DANISH),
});

/// A bundle of `source`. Mistakes in it are found by the tests below, so a broken
/// message is left out rather than stopping the app; English then stands in for it.
fn bundle(language: unic_langid::LanguageIdentifier, source: &str) -> FluentBundle<FluentResource> {
    let resource =
        FluentResource::try_new(source.to_owned()).unwrap_or_else(|(partial, _)| partial);
    let mut bundle = FluentBundle::new_concurrent(vec![language]);
    // Unicode isolation marks around arguments are for right-to-left scripts, which egui
    // doesn't lay out, and its fonts draw them as boxes.
    bundle.set_use_isolating(false);
    if let Err(errors) = bundle.add_resource(resource) {
        // Only duplicate ids, which the tests rule out: the first one is kept.
        debug_assert!(errors.is_empty(), "{errors:?}");
    }
    bundle
}

fn messages(language: Language) -> &'static FluentBundle<FluentResource> {
    match language {
        Language::Danish => &BUNDLES.danish,
        Language::English | Language::System => &BUNDLES.english,
    }
}

// The language every text is shown in. Tests run in parallel threads and some switch
// language, so each test thread has its own; the app has one for all threads.
#[cfg(not(test))]
static CURRENT: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
#[cfg(test)]
thread_local! {
    static CURRENT: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
}

/// Shows every text in `language` from now on.
pub fn set(language: Language) {
    let code = match language {
        Language::English | Language::System => 0,
        Language::Danish => 1,
    };
    #[cfg(not(test))]
    CURRENT.store(code, std::sync::atomic::Ordering::Relaxed);
    #[cfg(test)]
    CURRENT.with(|c| c.set(code));
}

/// The language texts are shown in: [`Language::English`] or a translation.
pub fn current() -> Language {
    #[cfg(not(test))]
    let code = CURRENT.load(std::sync::atomic::Ordering::Relaxed);
    #[cfg(test)]
    let code = CURRENT.with(std::cell::Cell::get);
    if code == 1 {
        Language::Danish
    } else {
        Language::English
    }
}

/// The language to show for the user's `choice`, `os_locale` being the operating
/// system's language (like `da-DK`) when it is known (R-I18N-1).
pub fn resolve(choice: Language, os_locale: Option<&str>) -> Language {
    match choice {
        Language::English | Language::Danish => choice,
        Language::System => {
            let primary = os_locale
                .and_then(|l| l.split(['-', '_', '.']).next())
                .unwrap_or_default();
            if primary.eq_ignore_ascii_case("da") {
                Language::Danish
            } else {
                Language::English
            }
        }
    }
}

/// The text of message `id` in the current language; use [`t!`].
pub fn text(id: &str, args: Option<&FluentArgs<'_>>) -> String {
    text_in(current(), id, args)
}

/// The text of message `id` in `language`, else in English, else `id` itself.
pub fn text_in(language: Language, id: &str, args: Option<&FluentArgs<'_>>) -> String {
    for bundle in [messages(language), messages(Language::English)] {
        if let Some(pattern) = bundle.get_message(id).and_then(|m| m.value()) {
            // A missing argument shows as its name in braces, which the tests rule out.
            let mut errors = Vec::new();
            return bundle
                .format_pattern(pattern, args, &mut errors)
                .into_owned();
        }
    }
    id.to_owned()
}

/// `value` with `decimals` decimals and the current language's decimal mark (R-I18N-3).
pub fn decimal(value: f64, decimals: usize) -> String {
    let text = format!("{value:.decimals$}");
    match current() {
        Language::Danish => text.replace('.', ","),
        Language::English | Language::System => text,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use fluent_syntax::ast;

    use super::*;

    fn sources() -> [(&'static str, &'static str); 2] {
        [("en.ftl", ENGLISH), ("da.ftl", DANISH)]
    }

    /// Each message id with the variables its text uses.
    fn variables(source: &str) -> BTreeMap<String, BTreeSet<String>> {
        let resource = fluent_syntax::parser::parse(source).expect("parses");
        let mut found = BTreeMap::new();
        for entry in resource.body {
            if let ast::Entry::Message(message) = entry {
                let mut names = BTreeSet::new();
                if let Some(pattern) = &message.value {
                    pattern_variables(pattern, &mut names);
                }
                let id = message.id.name.to_owned();
                assert!(found.insert(id.clone(), names).is_none(), "{id} twice");
            }
        }
        found
    }

    fn pattern_variables(pattern: &ast::Pattern<&str>, names: &mut BTreeSet<String>) {
        for element in &pattern.elements {
            if let ast::PatternElement::Placeable { expression } = element {
                expression_variables(expression, names);
            }
        }
    }

    fn expression_variables(expression: &ast::Expression<&str>, names: &mut BTreeSet<String>) {
        match expression {
            ast::Expression::Select { selector, variants } => {
                inline_variables(selector, names);
                for variant in variants {
                    pattern_variables(&variant.value, names);
                }
            }
            ast::Expression::Inline(inline) => inline_variables(inline, names),
        }
    }

    fn inline_variables(inline: &ast::InlineExpression<&str>, names: &mut BTreeSet<String>) {
        match inline {
            ast::InlineExpression::VariableReference { id } => {
                names.insert(id.name.to_owned());
            }
            ast::InlineExpression::FunctionReference { arguments, .. } => {
                for argument in &arguments.positional {
                    inline_variables(argument, names);
                }
            }
            ast::InlineExpression::Placeable { expression } => {
                expression_variables(expression, names);
            }
            ast::InlineExpression::StringLiteral { .. }
            | ast::InlineExpression::NumberLiteral { .. }
            | ast::InlineExpression::MessageReference { .. }
            | ast::InlineExpression::TermReference { .. } => {}
        }
    }

    /// The message ids `t!` is called with in the crate's code.
    fn ids_in_code() -> BTreeSet<String> {
        let mut ids = BTreeSet::new();
        let mut dirs = vec![std::path::PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src"
        ))];
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(dir).expect("src") {
                let path = entry.expect("entry").path();
                if path.is_dir() {
                    dirs.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    let code = std::fs::read_to_string(&path).expect("read");
                    // Split, so this line isn't found itself.
                    for (at, _) in code.match_indices(concat!("t", "!(")) {
                        let before = code[..at].chars().next_back();
                        if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                            continue; // `assert!(`, `print!(`
                        }
                        let rest = code[at + 3..].trim_start();
                        if let Some(rest) = rest.strip_prefix('"') {
                            let end = rest.find('"').expect("closing quote");
                            ids.insert(rest[..end].to_owned());
                        }
                    }
                }
            }
        }
        ids
    }

    /// D64: both files parse, have the same messages, and each translation uses the
    /// variables the English text has, so no argument goes missing.
    #[test]
    fn translations_match_the_english_messages() {
        for (name, source) in sources() {
            if let Err((_, errors)) = FluentResource::try_new(source.to_owned()) {
                panic!("{name}: {errors:?}");
            }
        }
        let english = variables(ENGLISH);
        let danish = variables(DANISH);
        let missing: Vec<_> = english
            .keys()
            .filter(|id| !danish.contains_key(*id))
            .collect();
        assert!(missing.is_empty(), "not in da.ftl: {missing:?}");
        let extra: Vec<_> = danish
            .keys()
            .filter(|id| !english.contains_key(*id))
            .collect();
        assert!(extra.is_empty(), "only in da.ftl: {extra:?}");
        for (id, names) in &english {
            assert_eq!(Some(names), danish.get(id), "variables of {id}");
        }
    }

    /// D64: every message the code shows exists in English, and every English message
    /// is shown somewhere.
    #[test]
    fn every_message_the_code_uses_exists() {
        let used = ids_in_code();
        let defined: BTreeSet<String> = variables(ENGLISH).into_keys().collect();
        let undefined: Vec<_> = used.difference(&defined).collect();
        assert!(undefined.is_empty(), "not in en.ftl: {undefined:?}");
        let unused: Vec<_> = defined.difference(&used).collect();
        assert!(unused.is_empty(), "never used: {unused:?}");
    }

    /// egui panics on characters its bundled fonts lack: every one in the translations
    /// must be drawable.
    #[test]
    fn bundled_fonts_have_every_character_of_the_translations() {
        let mut harness = crate::test_host::harness(|ui| {
            for (name, source) in sources() {
                let chars: BTreeSet<char> = source.chars().filter(|c| !c.is_control()).collect();
                for c in chars {
                    let found = ui.ctx().fonts_mut(|f| {
                        f.has_glyphs(&egui::FontId::proportional(13.0), &c.to_string())
                    });
                    assert!(found, "{name}: the bundled fonts have no glyph for {c:?}");
                }
            }
        });
        harness.run();
    }

    /// R-I18N-1: an explicit choice wins; otherwise the OS's language if the UI has it.
    #[test]
    fn the_language_follows_the_os_unless_chosen() {
        assert_eq!(resolve(Language::System, Some("da-DK")), Language::Danish);
        assert_eq!(
            resolve(Language::System, Some("da_DK.UTF-8")),
            Language::Danish
        );
        assert_eq!(resolve(Language::System, Some("DA")), Language::Danish);
        assert_eq!(resolve(Language::System, Some("en-US")), Language::English);
        assert_eq!(resolve(Language::System, Some("de-DE")), Language::English);
        assert_eq!(resolve(Language::System, Some("ü😀")), Language::English);
        assert_eq!(resolve(Language::System, Some("")), Language::English);
        assert_eq!(resolve(Language::System, None), Language::English);
        assert_eq!(resolve(Language::English, Some("da-DK")), Language::English);
        assert_eq!(resolve(Language::Danish, Some("en-US")), Language::Danish);
    }

    /// R-I18N-2: texts come in the chosen language, with arguments and no isolation
    /// marks; an unknown id shows as itself.
    #[test]
    fn texts_come_in_the_current_language() {
        set(Language::English);
        assert_eq!(t!("send"), "Send");
        set(Language::Danish);
        assert_eq!(current(), Language::Danish);
        assert_eq!(t!("cancel"), "Annuller");
        let mut args = Args::new();
        args.set("count", 2);
        let text = text_in(Language::Danish, "workflow-captures", Some(&args));
        assert_eq!(text, "2 opsamlinger", "no isolation marks");
        args.set("count", 1);
        let text = text_in(Language::Danish, "workflow-captures", Some(&args));
        assert_eq!(text, "1 opsamling");
        assert_eq!(
            text_in(Language::Danish, "no-such-message", None),
            "no-such-message"
        );
        set(Language::English);
    }

    /// R-I18N-3: the decimal mark follows the language.
    #[test]
    fn decimals_follow_the_language() {
        set(Language::Danish);
        assert_eq!(decimal(1.5, 1), "1,5");
        set(Language::English);
        assert_eq!(decimal(1.5, 1), "1.5");
        assert_eq!(decimal(1.234, 2), "1.23");
    }
}
