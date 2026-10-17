---
name: verify
description: Run Tortuga's full local quality gate (fmt, clippy, tests, web-target check, cargo-deny). Use before declaring any code change done, before committing, or when the user asks to check/verify the build.
---

# Verify

1. `make fmt` to format, then `make verify` (fmt-check, lint, test, deny — the same as CI).
2. Fix the first failure and re-run until it passes. Fix warnings properly; silence a lint only with
   `#[expect(lint, reason = "...")]`.
3. Read the output for `NOTE: GTK 3 not found`. That means the desktop launcher (`tortuga-app`: window setup,
   `dialogs.rs`, `local_store.rs`) was **not** linted natively. Say so explicitly when reporting; don't claim the desktop
   build was checked.
4. If the change affects what the user sees, checks pass only part of the story: say whether the app was
   actually run (`make run`) and what was tried in it.

Report what ran and what passed. Never claim a step passed that was skipped.
