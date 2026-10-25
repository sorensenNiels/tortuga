// Tortuga in VS Code (IDEA-97 (a), R-VSC-1..4): the projects in the open folders as tests,
// run through `tortuga-cli`, with code lenses on their files and a way into the app.

import { spawn } from "node:child_process";
import { randomUUID } from "node:crypto";
import { readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import * as path from "node:path";
import * as vscode from "vscode";

import {
  Entry,
  Listing,
  Report,
  duration,
  execute,
  exitMessage,
  failures,
  listArgs,
  outcomesByRequest,
  runArgs,
} from "./cli";

/** What a test item stands for. `target` is what `tortuga-cli run` is given. */
type Node =
  | { kind: "project"; root: string }
  | { kind: "workflows"; root: string }
  | { kind: "workflow"; root: string; target: string }
  | { kind: "folder"; root: string; target: string }
  | { kind: "request"; root: string; target: string };

const MANIFEST = "tortuga.toml";
const FOLDER_FILE = "_folder.toml";

let controller: vscode.TestController;
const nodes = new WeakMap<vscode.TestItem, Node>();
/** The listing of each project, by its root. */
const listings = new Map<string, Listing>();
const lensesChanged = new vscode.EventEmitter<void>();
let status: vscode.StatusBarItem;

/** What `activate` gives other code: the integration tests. */
export interface Api {
  controller: vscode.TestController;
  results: ReadonlyMap<string, Result>;
  /** Lists the projects again; resolves when the tree is up to date. */
  discover(): Promise<void>;
  run(items: vscode.TestItem[], mock: boolean): Promise<void>;
}

export function activate(context: vscode.ExtensionContext): Api {
  controller = vscode.tests.createTestController("tortuga", "Tortuga");
  controller.refreshHandler = () => discover();
  controller.createRunProfile(
    "Run",
    vscode.TestRunProfileKind.Run,
    (request, token) => run(request, token, false),
    true,
  );
  controller.createRunProfile(
    "Run against the mock",
    vscode.TestRunProfileKind.Run,
    (request, token) => run(request, token, true),
    false,
  );

  status = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 50);
  status.command = "tortuga.selectEnvironment";
  showEnvironment();

  const watcher = vscode.workspace.createFileSystemWatcher("**/*.toml");
  const changed = debounce((uri: vscode.Uri) => onFileChanged(uri), 300);
  watcher.onDidChange(changed);
  watcher.onDidCreate(changed);
  watcher.onDidDelete(changed);

  context.subscriptions.push(
    controller,
    status,
    watcher,
    lensesChanged,
    vscode.languages.registerCodeLensProvider({ pattern: "**/*.toml" }, { provideCodeLenses, onDidChangeCodeLenses: lensesChanged.event }),
    vscode.commands.registerCommand("tortuga.run", (uri?: vscode.Uri) => runFile(uri, false)),
    vscode.commands.registerCommand("tortuga.runWithMock", (uri?: vscode.Uri) => runFile(uri, true)),
    vscode.commands.registerCommand("tortuga.openInApp", openInApp),
    vscode.commands.registerCommand("tortuga.selectEnvironment", selectEnvironment),
    vscode.commands.registerCommand("tortuga.refresh", () => discover()),
    vscode.workspace.onDidChangeConfiguration((e) => {
      if (e.affectsConfiguration("tortuga")) {
        showEnvironment();
        if (e.affectsConfiguration("tortuga.cliPath") || e.affectsConfiguration("tortuga.workspaceFile")) {
          void discover();
        }
      }
    }),
    vscode.workspace.onDidChangeWorkspaceFolders(() => discover()),
  );
  void discover();
  return {
    controller,
    results,
    discover,
    run: (items, mock) => run(new vscode.TestRunRequest(items), new vscode.CancellationTokenSource().token, mock),
  };
}

export function deactivate(): void {}

// ---------------------------------------------------------------- settings

function settings() {
  const config = vscode.workspace.getConfiguration("tortuga");
  return {
    cli: config.get<string>("cliPath") || "tortuga-cli",
    app: config.get<string>("appPath") || "tortuga",
    environment: config.get<string>("environment") ?? "",
    workspaceFile: resolve(config.get<string>("workspaceFile") ?? ""),
  };
}

/** A setting's path, relative to the first workspace folder. */
function resolve(file: string): string {
  const folder = vscode.workspace.workspaceFolders?.[0];
  return file && folder && !path.isAbsolute(file) ? path.join(folder.uri.fsPath, file) : file;
}

function showEnvironment(): void {
  const name = settings().environment;
  status.text = `$(globe) ${name || "No environment"}`;
  status.tooltip = "Tortuga: the environment requests run in. Click to change it.";
  status.show();
}

// ---------------------------------------------------------------- the tree

/** Finds every project in the open folders and lists it (R-VSC-1). */
async function discover(): Promise<void> {
  const manifests = await vscode.workspace.findFiles(`**/${MANIFEST}`, "{**/node_modules/**,**/target/**}");
  const roots = manifests.map((m) => path.dirname(m.fsPath)).sort();
  for (const root of [...listings.keys()]) {
    if (!roots.includes(root)) {
      listings.delete(root);
      controller.items.delete(root);
    }
  }
  await Promise.all(roots.map((root) => load(root)));
  treeChanged();
}

/** Lenses and menus follow the tree: they show on the files it has items for. */
function treeChanged(): void {
  const files: string[] = [];
  const collect = (item: vscode.TestItem) => {
    if (item.uri) {
      files.push(item.uri.fsPath);
      if (nodes.get(item)?.kind === "folder") {
        files.push(path.join(item.uri.fsPath, FOLDER_FILE));
      }
    }
    item.children.forEach(collect);
  };
  controller.items.forEach(collect);
  void vscode.commands.executeCommand("setContext", "tortuga.files", files);
  lensesChanged.fire();
}

/** (Re)reads one project into the tree, or shows why it can't be read. */
async function load(root: string): Promise<void> {
  const { cli, workspaceFile } = settings();
  const exit = await execute(cli, listArgs(root, workspaceFile));
  const item = controller.createTestItem(root, path.basename(root), vscode.Uri.file(path.join(root, MANIFEST)));
  nodes.set(item, { kind: "project", root });
  controller.items.add(item);
  if (exit.code !== 0) {
    listings.delete(root);
    item.error = exitMessage(cli, exit);
    return;
  }
  let listing: Listing;
  try {
    listing = JSON.parse(exit.stdout) as Listing;
  } catch (e) {
    item.error = `tortuga-cli list printed something that is not JSON: ${String(e)}`;
    return;
  }
  listings.set(root, listing);
  item.label = listing.name;
  item.description = path.relative(vscode.workspace.workspaceFolders?.[0]?.uri.fsPath ?? "", root) || undefined;
  const children: vscode.TestItem[] = [];
  if (listing.workflows.length > 0) {
    const group = controller.createTestItem(`${root}::workflows`, "Workflows", vscode.Uri.file(path.join(root, "workflows")));
    nodes.set(group, { kind: "workflows", root });
    group.sortText = "0";
    group.children.replace(
      listing.workflows.map((w) => {
        const wf = controller.createTestItem(`${root}::workflow::${w.id}`, w.name, vscode.Uri.file(path.join(root, "workflows", `${w.id}.toml`)));
        nodes.set(wf, { kind: "workflow", root, target: w.id });
        return wf;
      }),
    );
    children.push(group);
  }
  children.push(...entries(root, listing.items));
  item.children.replace(children);
}

function entries(root: string, items: Entry[]): vscode.TestItem[] {
  // Keep the engine's order (`seq`), after the workflows.
  return items.map((entry, n) => {
    const uri = vscode.Uri.file(path.join(root, entry.path));
    let item: vscode.TestItem;
    switch (entry.kind) {
      case "folder":
        item = controller.createTestItem(`${root}::${entry.path}`, entry.name, uri);
        nodes.set(item, { kind: "folder", root, target: entry.path });
        item.children.replace(entries(root, entry.items));
        break;
      case "request":
        item = controller.createTestItem(`${root}::${entry.path}`, entry.name, uri);
        item.description = entry.method;
        nodes.set(item, { kind: "request", root, target: entry.path });
        break;
      case "broken":
        item = controller.createTestItem(`${root}::${entry.path}`, path.basename(entry.path), uri);
        item.error = entry.error;
        break;
    }
    item.sortText = `1${String(n).padStart(6, "0")}`;
    return item;
  });
}

/** A changed `.toml` reloads the project it is in; a new or removed project re-scans. */
function onFileChanged(uri: vscode.Uri): void {
  const root = rootOf(uri.fsPath);
  if (path.basename(uri.fsPath) === MANIFEST || root === undefined) {
    void discover();
  } else {
    void load(root).then(treeChanged);
  }
}

/** The project a file belongs to: the nearest known root above it. */
function rootOf(file: string): string | undefined {
  return [...listings.keys()]
    .filter((root) => file === root || file.startsWith(root + path.sep))
    .sort((a, b) => b.length - a.length)[0];
}

/** The test item for a file of a project, if it is one. */
function itemFor(file: string): vscode.TestItem | undefined {
  const root = rootOf(file);
  const project = root === undefined ? undefined : controller.items.get(root);
  if (!root || !project) {
    return undefined;
  }
  const relative = path.relative(root, file).split(path.sep).join("/");
  if (relative === MANIFEST) {
    return project;
  }
  const workflow = /^workflows\/([^/]+)\.toml$/.exec(relative);
  if (workflow) {
    return project.children.get(`${root}::workflows`)?.children.get(`${root}::workflow::${workflow[1]}`);
  }
  const id = path.basename(relative) === FOLDER_FILE ? path.posix.dirname(relative) : relative;
  return find(project, `${root}::${id}`);
}

function find(parent: vscode.TestItem, id: string): vscode.TestItem | undefined {
  let found = parent.children.get(id);
  parent.children.forEach((child) => {
    found ??= find(child, id);
  });
  return found;
}

// ---------------------------------------------------------------- running

/** R-VSC-2: runs the chosen items with `tortuga-cli`, one call per item. */
async function run(request: vscode.TestRunRequest, token: vscode.CancellationToken, mock: boolean): Promise<void> {
  const testRun = controller.createTestRun(request);
  const queue: vscode.TestItem[] = [];
  if (request.include) {
    queue.push(...request.include);
  } else {
    controller.items.forEach((item) => queue.push(item));
  }
  const excluded = (item: vscode.TestItem) => request.exclude?.some((e) => e === item || isWithin(item, e));
  for (const item of queue) {
    if (token.isCancellationRequested) {
      break;
    }
    if (!excluded(item)) {
      await runItem(testRun, item, token, mock, excluded);
    }
  }
  testRun.end();
}

async function runItem(
  testRun: vscode.TestRun,
  item: vscode.TestItem,
  token: vscode.CancellationToken,
  mock: boolean,
  excluded: (item: vscode.TestItem) => boolean | undefined,
): Promise<void> {
  const node = nodes.get(item);
  switch (node?.kind) {
    case undefined:
      return;
    case "project": {
      // Every request, in the project's order; then each workflow.
      const workflows = item.children.get(`${node.root}::workflows`);
      await runTarget(testRun, item, node.root, undefined, token, mock, workflows);
      if (workflows && !excluded(workflows)) {
        await runItem(testRun, workflows, token, mock, excluded);
      }
      return;
    }
    case "workflows": {
      const all: vscode.TestItem[] = [];
      item.children.forEach((child) => all.push(child));
      testRun.started(item);
      for (const child of all) {
        if (!token.isCancellationRequested && !excluded(child)) {
          await runItem(testRun, child, token, mock, excluded);
        }
      }
      const failed = all.filter((child) => ["failed", "errored"].includes(results.get(child.id) ?? ""));
      if (failed.length > 0) {
        record(testRun, item, "failed", [`Failed: ${failed.map((c) => c.label).join(", ")}`]);
      } else {
        record(testRun, item, "passed");
      }
      return;
    }
    case "workflow":
    case "folder":
    case "request":
      await runTarget(testRun, item, node.root, node.target, token, mock);
      return;
  }
}

type Result = "passed" | "failed" | "errored" | "skipped";

/** Each item's last result, by its id: for the workflows group, and for tests. */
const results = new Map<string, Result>();

function record(testRun: vscode.TestRun, item: vscode.TestItem, result: Result, lines: string[] = [], ms?: number): void {
  results.set(item.id, result);
  switch (result) {
    case "passed":
      testRun.passed(item, ms);
      break;
    case "failed":
      testRun.failed(item, messages(lines, item), ms);
      break;
    case "errored":
      testRun.errored(item, messages(lines, item));
      break;
    case "skipped":
      testRun.skipped(item);
      break;
  }
}

async function runTarget(
  testRun: vscode.TestRun,
  item: vscode.TestItem,
  root: string,
  target: string | undefined,
  token: vscode.CancellationToken,
  mock: boolean,
  skip?: vscode.TestItem,
): Promise<void> {
  const requests = requestsUnder(item, skip);
  testRun.started(item);
  requests.forEach((r) => testRun.started(r));
  const { cli, environment, workspaceFile } = settings();
  const reportFile = path.join(tmpdir(), `tortuga-vscode-${randomUUID()}.json`);
  const abort = new AbortController();
  const stop = token.onCancellationRequested(() => abort.abort());
  const args = runArgs(root, target, { environment, workspaceFile, mock }, reportFile);
  testRun.appendOutput(`> ${cli} ${args.join(" ")}\r\n`, undefined, item);
  const exit = await execute(cli, args, (text) => testRun.appendOutput(text.replace(/\r?\n/g, "\r\n"), undefined, item), abort.signal);
  stop.dispose();

  let report: Report | undefined;
  try {
    report = JSON.parse(await readFile(reportFile, "utf8")) as Report;
  } catch {
    // No report: the command or the project had an error, which `exit` says.
  }
  // The report was read (or never written); a leftover file in the temp folder is harmless.
  await rm(reportFile, { force: true }).catch(() => undefined);

  if (token.isCancellationRequested || !report) {
    if (token.isCancellationRequested) {
      record(testRun, item, "skipped");
    } else {
      record(testRun, item, "errored", [exitMessage(cli, exit)]);
    }
    requests.forEach((r) => record(testRun, r, "skipped"));
    return;
  }

  const outcomes = outcomesByRequest(report);
  for (const request of requests) {
    const outcome = outcomes.get((nodes.get(request) as { target: string }).target);
    if (!outcome) {
      record(testRun, request, "skipped");
    } else {
      record(testRun, request, outcome.passed ? "passed" : "failed", outcome.messages, outcome.durationMs);
    }
  }
  record(testRun, item, report.passed ? "passed" : "failed", failures(report), duration(report));
}

function messages(lines: string[], item: vscode.TestItem): vscode.TestMessage[] {
  return (lines.length > 0 ? lines : ["Failed."]).map((line) => {
    const message = new vscode.TestMessage(line);
    if (item.uri && item.uri.fsPath.endsWith(".toml")) {
      message.location = new vscode.Location(item.uri, new vscode.Position(0, 0));
    }
    return message;
  });
}

/** The request items in `item`'s subtree (itself, if it is one), leaving out `skip`'s. */
function requestsUnder(item: vscode.TestItem, skip?: vscode.TestItem): vscode.TestItem[] {
  if (item === skip) {
    return [];
  }
  if (nodes.get(item)?.kind === "request") {
    return [item];
  }
  const found: vscode.TestItem[] = [];
  item.children.forEach((child) => found.push(...requestsUnder(child, skip)));
  return found;
}

function isWithin(item: vscode.TestItem, ancestor: vscode.TestItem): boolean {
  for (let p = item.parent; p; p = p.parent) {
    if (p === ancestor) {
      return true;
    }
  }
  return false;
}

// ---------------------------------------------------------------- commands and lenses

/** R-VSC-2: "Run" on a project, folder, request or workflow file (or the open one). */
async function runFile(uri: vscode.Uri | undefined, mock: boolean): Promise<void> {
  const file = (uri ?? vscode.window.activeTextEditor?.document.uri)?.fsPath;
  const item = file === undefined ? undefined : itemFor(file);
  if (!item) {
    void vscode.window.showWarningMessage("This file is not a request, folder, workflow or project of a Tortuga project.");
    return;
  }
  const request = new vscode.TestRunRequest([item]);
  const source = new vscode.CancellationTokenSource();
  // Opens the Test Results view, as running from the Testing view does.
  vscode.commands.executeCommand("testing.showMostRecentOutput").then(undefined, () => undefined);
  await run(request, source.token, mock);
  source.dispose();
}

/** R-VSC-3: opens the project (and a request file in a tab) in the desktop app. */
function openInApp(uri?: vscode.Uri): void {
  const target = (uri ?? vscode.window.activeTextEditor?.document.uri)?.fsPath
    ?? [...listings.keys()][0];
  if (target === undefined) {
    void vscode.window.showWarningMessage("There is no Tortuga project in the open folders.");
    return;
  }
  const { app } = settings();
  const child = spawn(app, [target], { detached: true, stdio: "ignore", windowsHide: false });
  child.on("error", (e: NodeJS.ErrnoException) => {
    void vscode.window.showErrorMessage(
      e.code === "ENOENT"
        ? `Could not find '${app}'. Install Tortuga or set "tortuga.appPath".`
        : `Could not start Tortuga: ${e.message}`,
    );
  });
  // The app lives on after VS Code closes.
  child.unref();
}

/** R-VSC-4: picks the environment runs use, from the projects' and the workspace's. */
async function selectEnvironment(): Promise<void> {
  const names = [...new Set([...listings.values()].flatMap((l) => l.environments.map((e) => e.name)))].sort();
  const none = "No environment";
  const picked = await vscode.window.showQuickPick([none, ...names], {
    placeHolder: names.length > 0 ? "The environment requests run in" : "The projects have no environments",
  });
  if (picked !== undefined) {
    await vscode.workspace
      .getConfiguration("tortuga")
      .update("environment", picked === none ? "" : picked, vscode.ConfigurationTarget.Workspace);
  }
}

function provideCodeLenses(document: vscode.TextDocument): vscode.CodeLens[] {
  const item = itemFor(document.uri.fsPath);
  if (!item) {
    return [];
  }
  const top = new vscode.Range(0, 0, 0, 0);
  const what = nodes.get(item)?.kind === "project" ? "Run project" : "Run";
  return [
    new vscode.CodeLens(top, { title: `$(play) ${what}`, command: "tortuga.run", arguments: [document.uri] }),
    new vscode.CodeLens(top, { title: "Run against the mock", command: "tortuga.runWithMock", arguments: [document.uri] }),
    new vscode.CodeLens(top, { title: "Open in Tortuga", command: "tortuga.openInApp", arguments: [document.uri] }),
  ];
}

function debounce<T>(f: (arg: T) => void, ms: number): (arg: T) => void {
  const timers = new Map<string, NodeJS.Timeout>();
  return (arg: T) => {
    const key = String(arg);
    clearTimeout(timers.get(key));
    timers.set(
      key,
      setTimeout(() => {
        timers.delete(key);
        f(arg);
      }, ms),
    );
  };
}
