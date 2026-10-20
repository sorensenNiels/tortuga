//! Snapshot breakpoints (R-TEST-10): before a script runs, a call that records the
//! variables in scope goes in at the start of each breakpoint line, on that same line, so
//! line numbers in errors and logs stay right. The script then runs on; the console
//! shows each hit. Lines where such a call would change what the script does are
//! skipped with a note.

use std::collections::BTreeSet;

use crate::js_format;

/// Names a snapshot never shows: the script API, JavaScript's own globals and keywords.
const NOT_SHOWN: &[&str] = &[
    "test",
    "expect",
    "console",
    "vars",
    "req",
    "res",
    "JSON",
    "Math",
    "Object",
    "Array",
    "String",
    "Number",
    "Boolean",
    "Date",
    "RegExp",
    "Error",
    "TypeError",
    "RangeError",
    "SyntaxError",
    "ReferenceError",
    "Promise",
    "Set",
    "Map",
    "WeakMap",
    "WeakSet",
    "Symbol",
    "BigInt",
    "Reflect",
    "Proxy",
    "Intl",
    "globalThis",
    "eval",
    "arguments",
    "undefined",
    "NaN",
    "Infinity",
    "parseInt",
    "parseFloat",
    "isNaN",
    "isFinite",
    "encodeURIComponent",
    "decodeURIComponent",
    "encodeURI",
    "decodeURI",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "debugger",
    "default",
    "delete",
    "do",
    "else",
    "export",
    "extends",
    "finally",
    "for",
    "function",
    "if",
    "import",
    "in",
    "instanceof",
    "new",
    "return",
    "super",
    "switch",
    "this",
    "throw",
    "try",
    "typeof",
    "var",
    "void",
    "while",
    "with",
    "yield",
    "let",
    "static",
    "await",
    "async",
    "of",
    "true",
    "false",
    "null",
    "get",
    "set",
];

/// A script ready to run with its breakpoints, and the breakpoints that could not be set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Instrumented {
    pub code: String,
    /// (line, why) for each breakpoint left out.
    pub skipped: Vec<(u32, &'static str)>,
}

/// `script` with a snapshot call at the start of each of `lines` (1-based).
pub(super) fn instrument(script: &str, lines: &BTreeSet<u32>) -> Instrumented {
    let mut out = Instrumented {
        code: script.to_owned(),
        skipped: Vec::new(),
    };
    if lines.is_empty() || !super::parses(script) {
        // A script that does not parse fails with its own error; breakpoints can't help.
        return out;
    }
    let scan = js_format::scan(script);
    let names: Vec<&str> = scan
        .names
        .iter()
        .map(String::as_str)
        .filter(|n| !n.starts_with("__") && !NOT_SHOWN.contains(n))
        .collect();
    let call = |line: u32| {
        let list = serde_json::to_string(&names).unwrap_or_else(|_| "[]".into());
        format!("__bp({line}, {list}, (__n) => eval(__n)); ")
    };

    let mut usable = Vec::new();
    for &line in lines {
        let start = usize::try_from(line)
            .ok()
            .and_then(|l| l.checked_sub(1))
            .and_then(|l| scan.lines.get(l));
        match start {
            None => out.skipped.push((line, "the script has no such line")),
            Some(s) if !s.in_code => {
                out.skipped
                    .push((line, "it starts inside a string or comment"));
            }
            // After anything else the line may continue the one before (`x =` ⏎ `5`), and
            // a call put in front of it would change what the script does.
            Some(s) if !matches!(s.after, None | Some(';' | '{' | '}')) => out.skipped.push((
                line,
                "it may continue the line before; end that line with “;”",
            )),
            Some(_) => usable.push(line),
        }
    }
    // A call can still land where no statement may go (inside an object literal or the
    // arguments of a call): those breakpoints are dropped one by one.
    usable.retain(|&line| {
        let ok = super::parses(&insert(script, &[line], &call));
        if !ok {
            out.skipped.push((line, "a statement can't go there"));
        }
        ok
    });
    out.skipped.sort_unstable();
    out.code = insert(script, &usable, &call);
    out
}

/// `script` with `call(line)` before the first character of code on each of `lines`.
fn insert(script: &str, lines: &[u32], call: &impl Fn(u32) -> String) -> String {
    let lines_out: Vec<String> = (1..)
        .zip(script.split('\n'))
        .map(|(n, text)| {
            if !lines.contains(&n) {
                return text.to_owned();
            }
            let indent = text.len() - text.trim_start_matches([' ', '\t']).len();
            // The indent is ASCII spaces and tabs, so `indent` is a char boundary.
            let (head, tail) = text.split_at(indent);
            format!("{head}{}{tail}", call(n))
        })
        .collect();
    lines_out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(lines: &[u32]) -> BTreeSet<u32> {
        lines.iter().copied().collect()
    }

    #[test]
    fn calls_go_on_the_line_itself_with_the_names_in_use() {
        let out = instrument("const a = 1;\n  const b = a + 1;\n", &set(&[2]));
        assert_eq!(
            out.code,
            "const a = 1;\n  __bp(2, [\"a\",\"b\"], (__n) => eval(__n)); const b = a + 1;\n"
        );
        assert_eq!(out.skipped, Vec::<(u32, &str)>::new());
        assert_eq!(out.code.lines().count(), 2, "line numbers stay the same");
    }

    #[test]
    fn lines_that_continue_or_sit_in_strings_are_skipped() {
        let script =
            "const x =\n  5;\nconst t = `a\nb`;\nconst o = {\n  k: 1,\n};\nif (x)\n  f();\n";
        let out = instrument(script, &set(&[2, 4, 6, 9, 42]));
        let lines: Vec<u32> = out.skipped.iter().map(|(l, _)| *l).collect();
        assert_eq!(lines, [2, 4, 6, 9, 42]);
        assert_eq!(out.code, script, "nothing inserted");
        assert!(super::super::parses(&out.code));
    }

    #[test]
    fn closing_lines_and_blank_lines_can_take_a_breakpoint() {
        let script = "test(\"t\", () => {\n  const id = 7;\n\n});";
        let out = instrument(script, &set(&[3, 4]));
        assert_eq!(out.skipped, Vec::<(u32, &str)>::new());
        assert!(super::super::parses(&out.code), "{}", out.code);
        assert_eq!(out.code.lines().count(), 4);
    }

    #[test]
    fn scripts_that_do_not_parse_are_left_alone() {
        let out = instrument("const = ;", &set(&[1]));
        assert_eq!(out.code, "const = ;");
        let empty = instrument("", &set(&[1]));
        assert_eq!(empty.skipped, Vec::<(u32, &str)>::new());
        assert!(super::super::parses(&empty.code), "{}", empty.code);
    }
}
