---
name: verify
description: Run Tortuga's full local quality gate (fmt, clippy, tests, web-target check, cargo-deny). Use before declaring any code change done, before committing, or when the user asks to check/verify the build.
---

# Verify

1. Format, then run the gate (fmt-check, lint, test, deny — the same as CI) with its output in a log, not in context:

   ```sh
   make fmt && make verify > target/verify.log 2>&1; echo "exit $?"
   grep -E '^test result:|NOTE: GTK|^make: \*\*\*' target/verify.log | sort | uniq -c
   ```

2. On a non-zero exit, show only the failures, and read more of the log by line range if that is not enough:

   ```sh
   grep -nE -A8 '^(error|warning)(\[|:)|^---- .* ----$|panicked at' target/verify.log | head -120
   ```

   `make` stops at the first failing step. Fix it and re-run step 1 until it passes. Fix warnings properly; silence a
   lint only with `#[expect(lint, reason = "...")]`.
3. If the log has `NOTE: GTK 3 not found`, the desktop launcher (`tortuga-app`: window setup, `dialogs.rs`,
   `local_store.rs`) was **not** linted natively. Say so explicitly when reporting; don't claim the desktop build was
   checked.
4. If the change affects what the user sees, checks pass only part of the story: say whether the app was
   actually run (`make run`) and what was tried in it.

Report what ran and what passed. Never claim a step passed that was skipped.
