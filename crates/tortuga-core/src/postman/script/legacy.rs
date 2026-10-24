//! Postman's script API from before `pm` (R-COL-19, R-COL-40), still common in older
//! collections: `responseBody`, `responseCode.code`, `responseTime`,
//! `tests["name"] = condition`, `postman.getResponseHeader(name)` and
//! `postman.get`/`set`/`clearGlobalVariable` (workspace variables, R-WS-8).

use crate::import::js::{group_end, rewrite, skip_space, statement_end, word};

/// The translation of the legacy name or statement that `s` starts with.
pub(super) fn translate_at(s: &str) -> Option<(usize, String)> {
    let renames = [
        ("responseBody", "res.body"),
        ("responseCode.code", "res.status"),
        ("responseTime", "res.time"),
        ("postman.getResponseHeader", "res.header"),
    ];
    if let Some(found) = renames
        .into_iter()
        .find_map(|(from, to)| Some((word(s, from)?, to.to_owned())))
    {
        return Some(found);
    }
    if let Some(at) = word(s, "postman.setGlobalVariable") {
        return super::workspace_call(s, at, "set");
    }
    if let Some(at) = word(s, "postman.clearGlobalVariable") {
        return super::workspace_call(s, at, "unset");
    }
    test_assignment(s)
}

/// `tests["name"] = condition` → `test("name", () => expect(Boolean(condition)).toBe(true))`.
fn test_assignment(s: &str) -> Option<(usize, String)> {
    let open = skip_space(s, word(s, "tests")?);
    if s.as_bytes().get(open) != Some(&b'[') {
        return None;
    }
    let close = group_end(s, open)?;
    let name = s.get(open + 1..close - 1)?.trim();
    let equals = skip_space(s, close);
    let rest = s.get(equals..)?;
    if !rest.starts_with('=') || rest.starts_with("==") || name.is_empty() {
        return None;
    }
    let end = statement_end(s, equals + 1);
    let condition = s.get(equals + 1..end)?.trim();
    if condition.is_empty() {
        return None;
    }
    let condition = rewrite(condition, &super::translate_at);
    Some((
        end,
        format!("test({name}, () => expect(Boolean({condition})).toBe(true))"),
    ))
}
