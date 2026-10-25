# Tortuga for VS Code

Run the requests, folders and workflows of your [Tortuga](https://github.com/sorensenNiels/tortuga#readme) projects without leaving the editor.
The extension shows them in VS Code's **Testing** view and runs them with `tortuga-cli`, the same runner CI uses.
It reads and writes nothing itself: the project files stay the ones the app and git see.

## What it does

- **Testing view.** Every folder in the window that has a `tortuga.toml` is listed as a project: its
  workflows, folders and requests, in the app's order. The tree follows the files as they change, including
  after `git pull`.
- **Run.** Run a project, folder, request or workflow from the Testing view, with the ▶ code lens at the top of its
  `.toml` file, or with **Tortuga: Run**. Each request passes or fails with its tests. Failed tests,
  errors and data rows show as messages, and the runner's output is in Test Results.
  - Running a project runs every request, then every workflow.
  - **Run against the mock** (a second run profile and code lens) starts the project's mock and sends every request
    there, as `tortuga-cli run --mock` does.
- **Environment.** The status bar shows the environment runs use. Click it, or use **Tortuga: Select Environment**, to
  pick one of the projects' or the workspace file's environments.
- **Open in Tortuga.** The code lens, the explorer's context menu and the command open the project in the
  desktop app, and a request file in a tab.

## Setup

1. Install `tortuga-cli` (from a release's `tortuga-cli-<version>-<os>` archive, or `make cli`) and the Tortuga app.
   Put both on `PATH`, or set their paths in the settings below.
2. Install the extension: `make vscode` builds `editors/vscode/tortuga-<version>.vsix`. Then run
   `code --install-extension editors/vscode/tortuga-0.1.0.vsix`, or use **Extensions → … → Install from VSIX…**.

| Setting | Default | What it is |
|---|---|---|
| `tortuga.cliPath` | `tortuga-cli` | The runner. A path such as `target/debug/tortuga-cli` works while developing Tortuga. |
| `tortuga.appPath` | `tortuga` | The desktop app that **Open in Tortuga** starts. |
| `tortuga.environment` | (none) | The environment runs use (`--env`). |
| `tortuga.workspaceFile` | (none) | A `.tortuga-workspace.toml` whose variables, environments, settings, headers and auth runs use (`--workspace`), relative to the first folder. |

Secrets come from where `tortuga-cli` finds them: `TORTUGA_VAR_<name>` environment variables in VS Code's environment.
The app's credential store isn't read.

## Limits

- **Open in Tortuga** starts a new app window each time. A window that is already open isn't reused.
- Runs show results per request and per workflow. Steps of a workflow show as messages, not as items of their own.

## Developing

```sh
npm ci
npm test              # unit tests (node --test)
npm run test:vscode   # starts VS Code on selftest/ with the extension loaded (needs a display and
                      # target/debug/tortuga-cli; `make vscode-test` builds it and runs both)
```

`src/cli.ts` is what is passed to and read from `tortuga-cli` (no `vscode` import, so it is unit tested).
`src/extension.ts` is the VS Code side. The tree comes from `tortuga-cli list <project>`, so the extension never
re-implements how a project is read.
