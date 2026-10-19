# Tortuga

A fast, git-friendly API client for developers, written in Rust.
Targets: Windows, Linux, macOS desktop now; web (WASM) later.

**Scope rule:** `docs/ROADMAP.md` is the plan. Build what the user asks for; when choosing what comes next,
follow the roadmap's order. Aim for something unique that still feels familiar: reuse concepts developers know
from other API clients, and give them Tortuga's own take (see the roadmap's vision). If a request is unclear, or would
change a decision recorded in the roadmap, ask before building.

## Planning documents

| File | Holds | Rule |
|---|---|---|
| `docs/ROADMAP.md` | Vision, principles, open work and phases, open requirements (`R-COL-3`), ideas (`IDEA-n`), decisions (`D1`), next free IDs. | The plan and the spec for what is not done. Implement requirements as written; suggest new ideas by adding a row to its Ideas section; record new decisions there. |
| `docs/FEATURES.md` | Implemented **and verified** features by requirement ID, and the text of every completed requirement ("Agreed requirements"). | Add an entry only when the requirement is fully met, tested, and checked in the running app; record which OS, and move the requirement's row there from the roadmap. Otherwise it goes under "In progress". |

IDs are never reused or renumbered. Reference requirement IDs in commit messages and tests.
When building something that has no requirement yet, add one to the roadmap (next free ID in its area, listed at the end of the roadmap).

## Layout

| Path | Purpose |
|---|---|
| `crates/tortuga-core` | UI-agnostic engine. No UI deps, ever. `model` (request/response), `http` (client), `format` (TOML file format), `collection` (tree + operations on disk), `vars` (environments, servers, `{{variables}}`, auth, defaults), `secrets` (secret values + `SecretStore` trait), `settings` (the app's own settings, e.g. importers on/off), `cookies`, `git` (status data), `workspace` (shareable workspace file), `storage` (file access trait). In code a project is still a `Collection`; the UI and docs say "project" (D12). |
| `crates/tortuga-ui-egui` | The UI, built with egui 0.37: components, UI state (`AppState`, `TortugaUi`), display formatting. No platform code: files, dialogs, settings storage and task spawning come in through the `Host` trait. |
| `crates/tortuga-app` | Thin eframe launcher (`tortuga` binary): window, tokio runtime, `local_store`, native dialogs (`rfd`), and the `Host` impls. Features: `desktop` (default, native), `web` (wasm32). Also the app icon (`icons/`) and the installer settings (`[package.metadata.packager]`, see `docs/RELEASING.md`). |
| `crates/tortuga-cli` | `tortuga-cli` binary: runs a workflow, folder or request from a terminal or CI with the same project files (R-CLI-1..4). Only a thin layer over `tortuga-core`. |
| `scripts/` | One-time dev setup per OS. |
| `deny.toml` | Dependency licence/advisory policy (`cargo deny check`). |

## Commands

Use the Makefile (`make help` lists everything). The main ones:

```sh
make verify      # fmt-check + lint + test + deny: run before calling work done (= CI)
make test        # all tests; make test-core for the engine only
make lint        # clippy -D warnings for desktop and web targets
make run         # desktop app (cargo run)
make package     # installers for this OS (docs/RELEASING.md)
```

Without the GTK 3 headers (Linux), `make test`/`make lint` fall back to what can be built and say so:
- UI tests drive components headlessly with `egui_kittest`: `make test-ui` (`cargo test -p tortuga-ui-egui`),
  no window or system libraries needed.
- `make lint-web` type-checks the UI and launcher for wasm32, which also needs no GTK.

## Conventions

- Edition 2024, stable toolchain pinned by `rust-toolchain.toml`. Lints are workspace-wide
  (`clippy::pedantic` on, `unsafe_code` denied) — fix warnings; if a lint must be silenced use
  `#[expect(lint, reason = "...")]`. There is no `unsafe` code.
- Errors: `thiserror` enum in `tortuga-core::error`; UI converts to display strings at the edge
  (`error::message`). No `unwrap()` outside tests. See "Code quality" below for the rest.
- Everything in `tortuga-core` must compile for `wasm32-unknown-unknown`:
  use `web_time::Instant` (not `std::time::Instant`), no threads/tokio runtime in core logic.
  File access goes through the `storage::Storage` trait (`FsStorage` on native, `UnsupportedStorage` on wasm);
  never call `std::fs` directly from core logic.
- Collection operations write to disk, then the app re-opens the collection (`AppState::run`), so the UI
  always shows what is on disk. Keep it that way; don't patch the in-memory tree.
- Data that must not be shared (workspaces and their open projects, active environments, settings, cookies) is stored by
  `tortuga-app/src/local_store.rs` (behind `Host`), never in a collection directory.
- Persisted model types derive `Serialize`/`Deserialize`; add a round-trip test when adding fields,
  and use `#[serde(default)]` for new fields so older saved files still load.
- Tests: unit tests next to the code; HTTP behaviour tested in `crates/tortuga-core/tests/` against
  `wiremock` — never against the real internet.
- egui panics on characters its bundled fonts lack (e.g. ▸ ● ✕ ⋯). Use symbols from the list in
  `tortuga-ui-egui/src/lib.rs` (`bundled_fonts_have_every_symbol_the_ui_uses`) and add new ones there.

## Code quality

Clippy enforces most mechanical rules. These cover what it can't check. Follow them in new code, and fix
violations in code you touch.

**KISS**
- Write the simplest code that meets the requirement. Add a trait, generic, macro or builder only when a
  second real use exists today (test fakes count, e.g. `Host`/`Storage`). Don't add one for a phase that is
  still on the roadmap.
- Use plain functions and structs. Reach for `Rc<RefCell<…>>`, lifetimes on structs or `async` only when
  the problem needs them.
- Keep non-test code in a module under ~1000 lines. When a file grows past that, split it into submodules
  along the existing pattern (`shell.rs` + `shell/*.rs`, each an `impl TortugaUi` block for one area).
  If a block inside a function needs a heading comment, it is a function: give it a name.

**DRY: search before you write**
- Before writing a helper, grep for an existing one. Known shared helpers: `error::sentence`/`error::message`,
  `slug`, `import.rs` (`unique_name`, `variable_name`, `variables_in`, `short_name`), `query`,
  `model::is_json_content_type`, `KeyValue::is_named`, `Request::header`/`has_header`,
  `Response::header`/`content_type`; in `tortuga-ui-egui`: `format::truncate`, `theme::*` (`sized_edit`,
  `icon_button`, `section_label`, `hint`, `palette`), `components::key_value_table`,
  `components/variable_row.rs` (variable name + 🔒 cell, "Add … (press Enter)" input), and
  `shell::Inbox` for results that background tasks send to the UI thread.
- A rule that has to agree everywhere (e.g. "is this content type JSON?", "is this header Content-Type?")
  lives in one function in `tortuga-core`, never as inline `contains("json")` or
  `eq_ignore_ascii_case` copies.
- The second time you copy a block of more than ~10 lines, extract it into a function.

**State that belongs together stays together**
- Don't add another `HashMap<ItemKey | Selection | PathBuf, _>` next to existing ones holding state for the
  same request or project. Parallel maps drift apart on rename and delete. Put the field into a struct
  that is already keyed that way (e.g. `shell::Sent` for what a request's last send left to show). If a
  new map really can't be avoided, update `TortugaUi::remap` and `TortugaUi::forget` (and
  `AppState::load_project`/`unload_project` for per-project maps) too, and test rename and delete.
- Use enums, not `bool` pairs or magic strings, for states, and an enum or newtype where `Option<T>`
  carries a meaning such as "the scratch request". In `match` on this workspace's own enums, list the
  variants rather than `_ =>` when a new variant would need a decision.

**Errors and panics**
- New fallible code in `tortuga-core` returns `crate::Result` with an `Error` variant. `Result<_, String>`
  is fine only at the `Host`/UI/CLI edge and in small parsers whose message is wrapped into an `Error`.
- `expect("…")` only for real invariants, with a message that states the invariant. Never use it on
  user input, files, network data or anything a `Host` returns.
- Text from responses, files and users is untrusted. Never slice a `&str` at an offset you computed
  yourself. Use offsets from `find`/`char_indices`/`match_indices`, check `is_char_boundary`, or use
  `format::truncate`. Prefer `.get(i)` to `[i]` when the index comes from data.
- Don't discard a `Result` (`let _ =`, `.ok();`) unless failure is harmless, and say why in a comment,
  as the existing code does ("The receiver only goes away with the app.").
- `tortuga-core` and `tortuga-ui-egui` never print (`println!`/`eprintln!`): return the error, or put it
  in `AppState::notice`. Only the binaries print, with a `tortuga:` prefix (the `print_stdout`/
  `print_stderr` lints enforce this; the binaries opt out at the crate root).

**Secrets**
- A type that holds secret values (`SecretValues`, secret stores, resolved auth) gets a hand-written
  `Debug` that hides the values, not `#[derive(Debug)]`. Secret values never go into error messages,
  history, logs or `notice`.

**Ownership and cloning**
- Parameters borrow (`&str`, `&[T]`, `&Path`, `&Request`). Take ownership only when the function stores
  the value. Return owned values.
- Don't `.clone()` just to quiet the borrow checker in code that runs every frame or per response body:
  restructure, or wrap big shared data in `Arc` (as `ResponseView` is). Write `Arc::clone(&x)` so a cheap
  clone reads differently from a deep one.

**Tests**
- Every bug fix comes with a test that fails without the fix. Name tests as sentences about behaviour
  (`unsaved_edits_are_not_overwritten`) and put the requirement ID in a comment or the test name.
- Test edge cases: empty input, non-ASCII text (`"ü😀"`), very long text, rename/move/delete of the item
  under test.

## Project skills

- `/verify` — full local quality gate; run before saying work is done.
- `egui-ui` — how to write UI code in this repo.
- `core-engine` — how to add engine features to `tortuga-core`.
