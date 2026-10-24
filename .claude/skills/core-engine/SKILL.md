---
name: core-engine
description: How to add or change engine functionality in crates/tortuga-core (data model, HTTP execution, collections, environments, variables, persistence, import/export). Use for any non-UI feature work in Tortuga.
---

# Working in tortuga-core

1. **Check scope** in `docs/ROADMAP.md` (§6 Requirements; completed ones are in `docs/REQUIREMENTS.md`: `grep -n` the ID, don't read the files whole). If the feature has no requirement yet, add one
   (next free ID) as part of the work; if it conflicts with a recorded decision (§8), ask first.
2. **Model first.** Add/extend types in `model.rs` (or a new module once it grows):
   `Debug, Clone, PartialEq, Serialize, Deserialize`; `#[serde(default)]` on new fields;
   identity in a collection is the file path; stable ids users can rename (servers, environments) come from `slug`. Re-export public types from `lib.rs`.
3. **Errors:** add variants to `error::Error` with useful context (what input failed). No panics,
   no `unwrap()` outside tests.
4. **Portability:** core must compile for `wasm32-unknown-unknown`. Use `web_time::Instant`.
   File access only through `storage::Storage` (tests use `FsStorage` on a `tempfile::tempdir()`).
   Keychain, OS dialogs etc. go behind a trait or live in the app.
5. **File format changes** (`format.rs`): keep output deterministic (fixed key order, defaults skipped,
   `BTreeMap`s), add `#[serde(default)]` for new fields, extend the exact-output tests, and bump
   `FORMAT_VERSION` only for changes older versions would misread.
6. **Tests are required:**
   - pure logic → `#[cfg(test)] mod tests` in the same file;
   - serde types → JSON round-trip test;
   - HTTP behaviour → `tests/http_client.rs` with `wiremock::MockServer` (`#[tokio::test]`). Never hit real hosts.
   - collection operations → `tests/collection.rs` on a temp directory, checking the files on disk.
7. Run `cargo test -p tortuga-core <filter>` while working, then `/verify` once at the end.
8. Update `docs/FEATURES.md`: "In progress" until the requirement is fully met and checked in the running app; then
   move the requirement's row from the roadmap to `docs/REQUIREMENTS.md`. Find the place with `grep -n` and edit it there.
