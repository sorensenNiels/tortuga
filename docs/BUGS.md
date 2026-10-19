# Bugs

Known bugs, open until fixed. IDs (`BUG-n`) are never reused. When a bug is fixed, add a test that fails without
the fix, reference the ID in the commit message and test, and remove its entry here.

Next free ID: BUG-2

## BUG-1: Environments dialog does not use the full window width

**Status:** Fixed in code and covered by tests (`the_tables_fill_the_dialog_so_names_are_not_truncated`);
not yet checked in the running app. Remove this entry once it has been.

**Where:** Environments dialog (`crates/tortuga-ui-egui/src/components/environments.rs`, the `env-matrix`
`ScrollArea` holding the Servers and Variables tables).

**What happens:** The inner scroll container with the Servers and Variables tables is narrower than the
dialog around it. Server names and variable names are truncated even though the dialog has room to show them.

**Expected:**
- The Servers and Variables container fills the full width of the dialog, so names are not truncated
  while there is space.
- The vertical scrollbar sits on the right-hand edge of the dialog, not partway in after the narrower
  container.
