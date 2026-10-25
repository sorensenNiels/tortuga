// Starts a real VS Code on the repository's selftest/ workspace with the extension loaded,
// and runs suite.ts inside it (R-VSC-1..2). Needs a display and the debug tortuga-cli.

import * as path from "node:path";
import { runTests } from "@vscode/test-electron";

const extension = path.resolve(__dirname, "../../..");
const repository = path.resolve(extension, "../..");
const cli = path.join(repository, "target", "debug", process.platform === "win32" ? "tortuga-cli.exe" : "tortuga-cli");

// Set when started from a VS Code terminal; it would start the test VS Code as plain Node.
delete process.env.ELECTRON_RUN_AS_NODE;

runTests({
  extensionDevelopmentPath: extension,
  extensionTestsPath: path.join(__dirname, "suite.js"),
  launchArgs: [path.join(repository, "selftest"), "--disable-extensions", "--disable-workspace-trust"],
  extensionTestsEnv: { TORTUGA_CLI: cli },
}).catch((e: unknown) => {
  console.error(`tortuga: the VS Code integration tests failed: ${String(e)}`);
  process.exit(1);
});
