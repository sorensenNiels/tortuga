---
name: egui-ui
description: Conventions for writing UI in crates/tortuga-ui-egui with egui 0.37 (immediate-mode components, state, async sends, Host trait, styling, desktop+web compatibility, kittest UI tests). Use whenever adding or changing UI code, components, dialogs, or colours in Tortuga.
---

# egui UI in Tortuga

egui/eframe version: **0.37** (see workspace `Cargo.toml`). APIs changed a lot recently — check the source in
`~/.cargo/registry/src/*/egui-0.37*/` rather than relying on memory (`grep -rn "pub fn name"` there, then read
that range; the files are large). Notably: one `Panel` type
(`Panel::left/top(..).show(ui, ..)`), `eframe::App::ui(&mut self, ui, frame)`, buttons/labels take "atoms"
(`Atom::grow()` pushes the rest right), menus close themselves on click, `Id::new` is deprecated (`Id::unique`).

## Structure

- `tortuga-ui-egui` (library) — all UI. `TortugaUi::new(Box<dyn Host>)` once, then `ui.ui(egui_ui)` every frame.
  - `shell.rs` — `TortugaUi`: layout, selection, unsaved drafts, send/cancel, save, dialogs.
  - `state.rs` — `AppState` (settings, client, open collections, local state, notice). `run()` writes to
    disk then reloads the collection: never patch the in-memory tree.
  - `components/` — one component (family) per file, re-exported from `components/mod.rs`.
    `key_value_table` is the reusable name/value table (params, headers, form fields).
  - `host.rs` — the `Host` trait: storage, `spawn`, settings/workspace persistence, folder picker, save file.
  - `theme.rs` — the design system: `install` (fonts + light/dark style, called on the first frame),
    `palette`, `semibold`, `section_label`, `badge`, `tabs`, `input_group`, `card`, `callout`,
    `primary_button`/`danger_button`/`ghost_button`/`icon_button`, `sized_edit`.
  - `assets/fonts/` — Inter (UI) and JetBrains Mono (code), OFL-licensed, embedded with `include_bytes!`.
  - `format.rs` — pure display helpers with unit tests.
- `tortuga-app` (binary) — eframe launch + `Host` impls only (`desktop.rs`, `web.rs`, `dialogs.rs`, `local_store.rs`).

## Rules

- **UI only.** Parsing, HTTP, variable substitution, persistence formats → `tortuga-core`.
  **No platform code in `tortuga-ui-egui`** (no `std::fs`, `rfd`, tokio): add a `Host` method instead.
- Components are plain functions `fn thing(ui: &mut Ui, data: &mut T, …) -> Option<Action>` (or a dialog struct
  with `show(&mut self, ctx, …) -> DialogResult<T>`). They edit what they are given and *return* what the user
  asked for; the shell applies it. This avoids borrowing `TortugaUi` twice.
- State that is not part of the model (raw URL text, tab, body drafts) lives in a small state struct
  (`EditorState`, `ResponseState`, `SidebarState`). Recreate `EditorState::new` when the request is replaced
  from outside (`TortugaUi::load`). Never re-derive a text field's buffer from parsed data while typing.
- Give text fields stable ids (`.id_salt(..)`) when the widgets around them can change (tables, rows that
  appear), otherwise focus jumps.
- Inside a `Grid`, `desired_width` is capped by last frame's column width — use `theme::sized_edit`.
- Clickable things must be real widgets (`Button`, `TextEdit`, …). An invisible `scope(..).response.interact()`
  laid over labels loses its clicks inside dialogs (see `theme::tabs`).
- A `Panel` remembers its size from what its content used last frame; content that should fill a resizable
  panel calls `ui.set_min_height(ui.available_height())` (see the request panel in `shell.rs`).
- Content in a `Modal` must not size itself from `available_width()` without the dialog fixing its width
  (`dialog()` does `set_width`), or the grid and the dialog chase each other and never settle.
- Rows with trailing buttons: lay out with `Sides::new().shrink_left()` (see `sidebar::tree_row`); sizing a
  widget from `available_width()` in a resizable panel otherwise makes the panel creep wider every frame.
- **Glyphs:** egui *panics* on characters its bundled fonts lack (e.g. ▸ ● ✕ ⋯). Use only symbols listed in
  `lib.rs` `bundled_fonts_have_every_symbol_the_ui_uses` (⏵ ⏷ ⏺ ⚠ 🗙 ⚙ ✏ …) and add any new one to that test.
- Async: build the future, wrap it with `futures_util::future::abortable`, hand it to `host.spawn`; send the
  result back over the `mpsc` channel and call `ctx.request_repaint()`. Cancel = `AbortHandle::abort()` (drops
  the reqwest future, closing the connection). Do slow work (pretty-printing, splitting big bodies) inside the
  task, not in `ui()`. Never block the UI thread — except native modal dialogs, which must run on it.
- Big text: never put megabytes into one `Label`/`TextEdit`; draw visible rows only (`ScrollArea::show_rows`,
  see `response_panel`).
- **Look:** use the `theme` building blocks instead of raw egui widgets where one exists (underlined `tabs`,
  not `selectable_value`; `badge` for status/counts; `section_label` for small upper-case headings;
  `ghost_button`/`icon_button` for secondary actions; `callout` for inline warnings). Emphasis is
  `theme::semibold(..)` (egui's `.strong()` only changes colour). Colours come from `theme::palette(ui)`
  (light/dark variants) — never hard-code colours in components. Check new UI in both light and dark mode.
- Everything must compile for wasm32 (`make lint-web`); `BoxTask` is `Send` on native only.

## Tests

`egui_kittest` drives components headlessly (no window, no GPU, no system libraries):

```rust
let mut harness = crate::test_host::harness(|ui| { key_value_table(ui, "t", &mut rows, "Header", &[]); });
harness.run();
harness.get_by_label("Send");                 // panics if missing (or if several match)
harness.query_all_by_label("🗙").next().expect("button").click();
harness.run();
```

Placeholders are not accessible labels in kittest: find text fields by role
(`get_all_by_role(Role::TextInput)`) or by value. Build harnesses with `crate::test_host::harness(|ui| ..)`: it installs the theme (the `semibold` font family
must exist or egui panics). Use `crate::test_host::TestHost` for the whole app; it counts spawned sends. Keep logic in small pure functions and unit-test those.

## Check your work

`make test-ui` and `make lint`, then run the app (`make run`) and try the change. Finally `/verify`.
