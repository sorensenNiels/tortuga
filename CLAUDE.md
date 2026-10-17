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
| `docs/ROADMAP.md` | Vision, principles, milestones and phases, requirements (`R-COL-3`), ideas (`IDEA-n`), decisions (`D1`). | The plan and the spec. Implement requirements as written; suggest new ideas by adding a row to its Ideas section; record new decisions there. |
| `docs/FEATURES.md` | Implemented **and verified** features, by requirement ID. | Add an entry only when the requirement is fully met, tested, and checked in the running app; record which OS. Otherwise it goes under "In progress". |

IDs are never reused or renumbered. Reference requirement IDs in commit messages and tests.
When building something that has no requirement yet, add one to the roadmap (next free ID in its area).

## Layout

| Path | Purpose |
|---|---|
| `crates/tortuga-core` | UI-agnostic engine. No UI deps, ever. `model` (request/response), `http` (client), `format` (TOML file format), `collection` (tree + operations on disk), `vars` (environments, servers, `{{variables}}`, auth, defaults), `secrets` (secret values + `SecretStore` trait), `cookies`, `git` (status data), `workspace` (shareable workspace file), `storage` (file access trait). In code a project is still a `Collection`; the UI and docs say "project" (D12). |
| `crates/tortuga-ui-egui` | The UI, built with egui 0.37: components, UI state (`AppState`, `TortugaUi`), display formatting. No platform code: files, dialogs, settings storage and task spawning come in through the `Host` trait. |
| `crates/tortuga-app` | Thin eframe launcher (`tortuga` binary): window, tokio runtime, `local_store`, native dialogs (`rfd`), and the `Host` impls. Features: `desktop` (default, native), `web` (wasm32). |
| `scripts/` | One-time dev setup per OS. |
| `deny.toml` | Dependency licence/advisory policy (`cargo deny check`). |

## Commands

Use the Makefile (`make help` lists everything). The main ones:

```sh
make verify      # fmt-check + lint + test + deny: run before calling work done (= CI)
make test        # all tests; make test-core for the engine only
make lint        # clippy -D warnings for desktop and web targets
make run         # desktop app (cargo run)
```

Without the GTK 3 headers (Linux), `make test`/`make lint` fall back to what can be built and say so:
- UI tests drive components headlessly with `egui_kittest`: `make test-ui` (`cargo test -p tortuga-ui-egui`),
  no window or system libraries needed.
- `make lint-web` type-checks the UI and launcher for wasm32, which also needs no GTK.

## Conventions

- Edition 2024, stable toolchain pinned by `rust-toolchain.toml`. Lints are workspace-wide
  (`clippy::pedantic` on, `unsafe_code` denied) — fix warnings; if a lint must be silenced use
  `#[expect(lint, reason = "...")]`. There is no `unsafe` code.
- Errors: `thiserror` enum in `tortuga-core::error`; UI converts to display strings at the edge.
  No `unwrap()` outside tests.
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

## Project skills

- `/verify` — full local quality gate; run before saying work is done.
- `egui-ui` — how to write UI code in this repo.
- `core-engine` — how to add engine features to `tortuga-core`.
