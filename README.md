# Tortuga 🐢

A fast, local-first API client written in Rust. Collections are plain files that teams share through git.
Runs on Windows, Linux and macOS (web support planned).

**Status:** milestones M1–M3 are implemented: sending requests, collections on disk, and environments with servers and variables. See [docs/FEATURES.md](docs/FEATURES.md) for what
works today and [docs/ROADMAP.md](docs/ROADMAP.md) for the plan.

## Getting started

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

**Collections.** Click **Open…** in the sidebar and pick a folder, typically inside your API's git repository.
If it isn't a collection yet, Tortuga offers to create one (it adds a `tortuga.toml`). Requests are saved as one
`.toml` file each and folders are directories, so you share them with `git add`, `commit` and `push` like any other file.
Use an item's **…** menu (or right-click it) to add, rename, duplicate, move, reorder or delete. **Ctrl+S** saves, **Ctrl+Enter** sends.

**Environments and servers.** In a collection's **…** menu, choose **Environments…**:

- Add environments (e.g. *Development*, *Staging*).
- Add servers (e.g. *Users API*) and give each one a base URL per environment.
- Add variables with a default value and, where needed, a different value per environment.

In a request, pick the server in front of the URL field and type only the path (`/users/{{user_id}}`). The environment
picker in the title bar switches every request in that collection at once. Variables that aren't defined are
highlighted, and the request isn't sent until they are.

Environment values are committed with the collection. **Don't put real passwords or tokens in them yet**; secret
variables that stay on your machine are the next milestone (M4).

The scratch request (top of the sidebar) is for quick one-off calls; **Save to collection…** keeps it.

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
| `make doc` | Build and open the API documentation. |
| `make clean` | Remove build output. |

On Linux without the GTK 3 headers installed, `make test` and `make lint` still check everything that can be built
(engine, UI and its tests, web target) and tell you what was skipped; `make run` asks you to run `make setup`.

## Project layout

```
crates/
  tortuga-core/    The engine: request model, HTTP execution, collections, variables. No UI code.
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
