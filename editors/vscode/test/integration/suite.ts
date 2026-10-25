// Runs inside VS Code (see main.ts): the selftest/ projects in the Testing view, and runs.

import * as assert from "node:assert/strict";
import * as vscode from "vscode";

import type { Api } from "../../src/extension";

function child(item: vscode.TestItem | undefined, label: string): vscode.TestItem {
  let found: vscode.TestItem | undefined;
  item?.children.forEach((c) => {
    if (c.label === label) {
      found = c;
    }
  });
  assert.ok(found, `${item?.label ?? "?"} has no item "${label}"`);
  return found;
}

export async function run(): Promise<void> {
  const extension = vscode.extensions.getExtension<Api>("tortuga.tortuga");
  assert.ok(extension, "the extension is installed");
  const api = await extension.activate();
  const config = vscode.workspace.getConfiguration("tortuga");
  await config.update("cliPath", process.env.TORTUGA_CLI, vscode.ConfigurationTarget.Global);
  // Its variables log in to the mock; relative to the first folder, selftest/.
  await config.update("workspaceFile", "tortuga-selftest.tortuga-workspace.toml", vscode.ConfigurationTarget.Global);
  await api.discover();

  // R-VSC-1: each project, with its workflows, folders and requests as the engine reads them.
  const projects: vscode.TestItem[] = [];
  api.controller.items.forEach((p) => projects.push(p));
  const sending = projects.find((p) => p.label === "Sending");
  assert.ok(sending, `Sending is listed (found: ${projects.map((p) => p.label).join(", ")})`);
  assert.equal(sending.error, undefined);
  const lifecycle = child(child(sending, "Workflows"), "User lifecycle");
  const getUser = child(child(sending, "Users"), "Get user");
  assert.equal(getUser.description, "GET");

  // R-VSC-2: a workflow passes against the project's mock ...
  await api.run([lifecycle], true);
  assert.equal(api.results.get(lifecycle.id), "passed");
  // ... and a request without an environment fails, saying why, rather than erroring.
  await api.run([getUser], false);
  assert.equal(api.results.get(getUser.id), "failed");

  // A missing tortuga-cli shows on the project, not as a crash.
  await config.update("cliPath", "tortuga-cli-that-does-not-exist", vscode.ConfigurationTarget.Global);
  await api.discover();
  assert.match(String(api.controller.items.get(sending.id)?.error ?? ""), /tortuga\.cliPath/);
}
