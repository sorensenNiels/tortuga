# Tortuga 🐢

A fast, local-first API client written in Rust. Projects are plain files that teams share through git.
Runs on Windows, Linux and macOS (web support planned).

**Status:** milestones M1–M8 and the v1 extras are implemented: sending requests, projects on disk, environments with servers and
variables, auth and secrets, cookies, live git status, workspaces, kept unsaved edits, copy as curl, installers for
Windows, macOS and Linux, and the first daily-driver features: paste curl, quick open and command palette, search and
filter responses, history, request preview, folder defaults, multipart uploads, custom CA certificates and drag and drop. See [docs/FEATURES.md](docs/FEATURES.md) for what
works today and [docs/ROADMAP.md](docs/ROADMAP.md) for the plan.

## Install

Download the installer for your OS from the [releases page](https://github.com/sorensenNiels/tortuga/releases):

| OS | File | Install |
|---|---|---|
| **Windows** | `.msi` | Run it. If SmartScreen warns, choose **More info → Run anyway**. |
| **macOS** | `.dmg`, one for Apple silicon (`aarch64`) and one for Intel | Drag Tortuga to Applications. The first time, right-click it and choose **Open**: the app is not signed yet. |
| **Linux** (Debian, Ubuntu) | `.deb` | `sudo apt install ./tortuga_*.deb` |
| **Linux** (other) | `.AppImage` | `chmod +x tortuga_*.AppImage`, then run it. |

Tortuga needs no account and works offline. To build installers yourself, see [docs/RELEASING.md](docs/RELEASING.md).

## Getting started (development)

### 1. Install the prerequisites (once per machine)

You need `git` and `make`. Then run the setup for your OS. It installs the system libraries the desktop
app needs, the Rust toolchain (version pinned in `rust-toolchain.toml`), and the cargo tools
`cargo-deny` and `cargo-nextest`.

| OS | Command | What it installs |
|---|---|---|
| **Linux** (Ubuntu/Debian, Fedora, Arch) | `make setup` | GTK 3 dev package, `libxkbcommon-x11` and OpenGL libraries (asks for your sudo password), Rust, cargo tools |
| **WSL2** | `make setup` | Same as Linux. The app window opens through WSLg (Windows 11, or Windows 10 with WSLg). |
| **macOS** | `make setup` | Xcode Command Line Tools (if missing), Rust, cargo tools |
| **Windows** | `powershell -ExecutionPolicy Bypass -File scripts\setup-windows.ps1` | MSVC Build Tools, GNU make, Rust, cargo tools |

On Windows the first run installs `make`, so call the script directly; afterwards `make setup` works too.
Open a new terminal after setup so the updated `PATH` is picked up.

### 2. Run the app

```sh
make run          # desktop app
```

The first build takes a few minutes; later builds are incremental.
In VS Code, press F5 to debug ("Debug Tortuga (desktop)").

### 3. Check your changes

```sh
make verify       # formatting, lints, all tests, dependency checks: the same as CI
```

## Using Tortuga

**Workspaces and projects.** A *project* is a folder with a `tortuga.toml`, typically inside your API's git
repository. Requests are saved as one `.toml` file each and folders are directories, so you share them with
`git add`, `commit` and `push` like any other file. A *workspace* (switcher in the title bar) is the set of projects
you work on together; several projects may live in one repository, and the sidebar groups them under it.

- **+ Add → New project…** creates a project in a folder (and can make it a git repository).
- **+ Add → Add existing projects…** finds every project in a folder or repository.
- **Save as file…** in the workspace menu writes a `.tortuga-workspace.toml` you can commit; teammates open it with
  **Open workspace file…** and see which repositories to clone.

Use an item's **…** menu (or right-click it) to add, rename, duplicate, move, reorder or delete. **Ctrl+S** saves,
**Ctrl+Enter** sends.

**Environments and servers.** In a project's **…** menu, choose **Environments…**:

- Add environments (e.g. *Development*, *Staging*).
- Add servers (e.g. *Users API*) and give each one a base URL per environment.
- Add variables with a default value and, where needed, a different value per environment. Mark credentials as
  secret: their values stay on your machine.

In a request, pick the server in front of the URL field and type only the path (`/users/{{user_id}}`). The environment
picker in the title bar switches every project in the workspace that has an environment with that name. Variables
that aren't defined are highlighted, and the request isn't sent until they are.

The scratch request (top of the sidebar) is for quick one-off calls; **Save to project…** keeps it.

**Moving around.** **Ctrl+P** (or **Go to request…**) finds any request in the workspace by typing part of its name,
folder or URL. **Ctrl+K** lists every command (send, save, switch environment, settings, …).

**From curl to request.** Paste a `curl` command (e.g. from API docs or the browser's "Copy as cURL") into the URL
field: method, URL, headers, body and auth are filled in. In a saved request this is an unsaved change; **Revert**
undoes it.

**Responses.** **Ctrl+F** searches the body. Above a JSON body, **Filter** takes a JSONPath expression such as
`$.items[*].id` or `$..users[?(@.active == true)].email`. The **Preview** tab of the request shows exactly what will be
sent, with secret values masked.

**History.** The **History** tab in the sidebar lists what you sent, per day. Click an entry to see that response;
**Restore request** brings back the request as it was then. History stays on your computer; **Clear** removes it.

**Uploads.** Choose the **Multipart** body type to send form fields and files. Keep files to upload inside the project
(e.g. a `fixtures/` folder) and they are saved with a path relative to it, so the request works for teammates too.

**Company certificates.** If your internal APIs use certificates from your company's own CA, add its PEM file under
**Settings → CA certificates**.

**Reordering.** Drag requests and folders in the sidebar to reorder them or move them into a folder. In the
environments dialog, drag the ☰ handle of an environment or server to change their order.

**Folder defaults.** A folder's **…** menu has **Folder defaults…**: headers, auth and variables for every request in
that folder. They are saved in the folder's `_folder.toml`, so they are shared through git like the requests.
Unsaved edits are kept when you close Tortuga. **Copy as curl** (next to Save) copies the request with variables
filled in; if it uses secret values, you choose whether to mask them.

## Make targets

| Target | What it does |
|---|---|
| `make setup` | One-time machine setup (see above). |
| `make run` | Run the desktop app. |
| `make run-web` | Preview the web build in a browser with [trunk](https://trunkrs.dev) (`cargo binstall trunk`). Experimental: browsers block most cross-site API calls (CORS). |
| `make verify` | Everything CI checks: `fmt-check`, `lint`, `test`, `deny`. |
| `make test` | All tests. `make test-core` runs only the engine tests, `make test-ui` only the UI tests. |
| `make lint` | Clippy with warnings as errors, for the desktop and web targets. |
| `make fmt` | Format all code. |
| `make deny` | Check dependency licences and security advisories. |
| `make build` / `make release` | Debug / optimised build of the desktop app. |
| `make package` | Installers for this OS in `target/packages/` (see [docs/RELEASING.md](docs/RELEASING.md)). |
| `make startup-time` | Open the optimised app with your workspace, print how long start-up took, and quit. |
| `make doc` | Build and open the API documentation. |
| `make clean` | Remove build output. |

On Linux without the GTK 3 headers installed, `make test` and `make lint` still check everything that can be built
(engine, UI and its tests, web target) and tell you what was skipped; `make run` asks you to run `make setup`.

## Project layout

```
crates/
  tortuga-core/    The engine: request model, HTTP execution, projects, variables, workspaces. No UI code.
  tortuga-ui-egui/ The user interface (egui): components and UI state. No platform code.
  tortuga-app/     Launches the UI on desktop (web later) and provides files, dialogs and settings storage.
                   Builds the `tortuga` binary.
docs/
  ROADMAP.md       The plan: vision, milestones and phases, requirements, ideas and decisions.
  FEATURES.md      What is implemented and verified, per requirement.
scripts/           One-time setup per OS (used by `make setup`).
```

[CLAUDE.md](CLAUDE.md) describes the coding conventions.

## Troubleshooting

| Problem | Fix |
|---|---|
| `make run` says GTK 3 is missing | Run `make setup` (Linux needs the `libgtk-3-dev` package). |
| `Library libxkbcommon-x11.so could not be loaded` | Install `libxkbcommon-x11-0` (Debian/Ubuntu; `make setup` does this). Needed when the app runs on X11. |
| `libEGL warning: egl: failed to create dri2 screen` in WSL | Harmless: Mesa found no `/dev/dri` and fell back to CPU rendering. `make run` sets `GALLIUM_DRIVER=d3d12` under WSL so the GPU is used; set it yourself when starting `tortuga` another way. |
| No window appears in WSL | Check that WSLg works: `echo $WAYLAND_DISPLAY` should print something. Update WSL with `wsl --update` in Windows. |
| `cargo-deny: command not found` | Open a new terminal after `make setup`, or add `~/.cargo/bin` to your `PATH`. |
| Settings file | Saved to `~/.config/tortuga/settings.toml` (Linux), `~/Library/Application Support/dk.SoftDesign.Tortuga/` (macOS), `%APPDATA%\SoftDesign\Tortuga\config\` (Windows). Delete it to reset. |

## License

MIT OR Apache-2.0
