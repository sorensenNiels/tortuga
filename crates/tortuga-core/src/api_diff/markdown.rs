//! The summary to post on a pull request (R-CLI-7, D77): a count line, a table with a
//! row per request and its first change, and every change of a request with more than
//! one folded in `<details>`.

use std::fmt::Write as _;

use super::{ApiDiff, RequestDiff, Status, code};

/// `diff` as Markdown, headed with the project's name and the revisions compared
/// (`main..HEAD`).
pub fn markdown(diff: &ApiDiff, project: &str, range: &str) -> String {
    let mut out = format!("### API changes: {} ({})\n\n", html(project), html(range));
    if diff.requests.is_empty() {
        out.push_str("No API changes.\n");
        return out;
    }
    out.push_str(&counts(diff));
    out.push_str("\n\n| | Request | Change |\n|---|---|---|\n");
    for r in &diff.requests {
        let lines = lines(r);
        let first = lines.first().map_or("", String::as_str);
        let more = match lines.len() {
            0 | 1 => String::new(),
            n => format!(" (+{} more)", n - 1),
        };
        let mark = if r.breaking() { " ⚠" } else { "" };
        // Written to a String, which cannot fail.
        let _ = writeln!(
            out,
            "| {}{mark} | {} | {}{more} |",
            icon(&r.status),
            cell(&request(r)),
            cell(first),
        );
    }
    for r in diff.requests.iter().filter(|r| lines(r).len() > 1) {
        let _ = write!(
            out,
            "\n<details><summary><code>{} {}</code> {}</summary>\n\n",
            html(&r.method),
            html(&r.path),
            html(&r.name),
        );
        for line in lines(r) {
            let _ = writeln!(out, "- {line}");
        }
        out.push_str("\n</details>\n");
    }
    out
}

/// `**1 added, 3 changed** · ⚠ 2 may break clients`
fn counts(diff: &ApiDiff) -> String {
    let count =
        |want: fn(&Status) -> bool| diff.requests.iter().filter(|r| want(&r.status)).count();
    let parts: Vec<String> = [
        (count(|s| *s == Status::Added), "added"),
        (count(|s| *s == Status::Removed), "removed"),
        (count(|s| matches!(s, Status::Moved { .. })), "moved"),
        (count(|s| *s == Status::Changed), "changed"),
    ]
    .into_iter()
    .filter(|(n, _)| *n > 0)
    .map(|(n, what)| format!("{n} {what}"))
    .collect();
    let mut line = format!("**{}**", parts.join(", "));
    match diff.breaking() {
        0 => {}
        1 => line.push_str(" · ⚠ 1 may break clients"),
        n => {
            let _ = write!(line, " · ⚠ {n} may break clients");
        }
    }
    line
}

fn icon(status: &Status) -> &'static str {
    match status {
        Status::Added => "➕",
        Status::Removed => "➖",
        Status::Moved { .. } => "➡",
        Status::Changed => "✏",
    }
}

/// `` `GET /orders` List orders ``
fn request(r: &RequestDiff) -> String {
    let what = code(&format!("{} {}", r.method, r.path));
    if r.name.trim().is_empty() {
        what
    } else {
        format!("{what} {}", html(r.name.trim()))
    }
}

/// What the row and the details list: the status where it says something itself, then
/// each change, marked when it may break clients.
fn lines(r: &RequestDiff) -> Vec<String> {
    let head = match &r.status {
        Status::Added => Some("added".to_owned()),
        Status::Removed => Some("⚠ removed".to_owned()),
        Status::Moved { from } => Some(format!("moved from {}", code(from))),
        Status::Changed => None,
    };
    head.into_iter()
        .chain(r.changes.iter().map(|c| {
            if c.breaking {
                format!("⚠ {}", c.text)
            } else {
                c.text.clone()
            }
        }))
        .collect()
}

/// Text that GitHub must not read as HTML.
fn html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Text for a table cell: one line, its `|` escaped.
fn cell(text: &str) -> String {
    text.replace(['\n', '\r'], " ").replace('|', "\\|")
}
