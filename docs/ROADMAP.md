# Tortuga roadmap

> What comes next: vision, principles, the open work, ideas and decisions.
> Everything that is built and verified, and the requirements it was built to, is in [FEATURES.md](FEATURES.md).
>
> - **Requirements** (`R-AREA-n`, [§6](#6-requirements)) describe agreed behaviour precisely. Once one is fully
>   met and verified it moves to FEATURES.md.
> - **Ideas** (`IDEA-n`, [§7](#7-ideas)) are candidates; one becomes a requirement when it gets a phase and an `R-` ID.
> - **Decisions** (`Dn`, [§8](#8-decisions)) record choices and the options considered. They stay here, done or not:
>   code and docs refer to them.
> - IDs are never reused or renumbered, also after they move to FEATURES.md. Code, tests, commits and FEATURES.md
>   refer to them.

## 1. Vision

Tortuga is the API client a developer keeps open all day next to the editor. It makes calling and
debugging APIs fast, and it lets a team share requests, environments and (later) test workflows through
the same git repository as the code.

Tortuga should feel **familiar and still be its own thing**. Developers coming from Postman, Insomnia,
Bruno, HTTPie or editor-based REST clients should recognise the concepts (collections, environments,
variables, `curl`, tests) and be productive at once. Tortuga then gives those concepts its own take:
plain files that live in git next to the code, speed, keyboard-first use, and nothing that gets in the way.
It is not a drop-in replacement for any of these tools; every feature should earn its place in a
developer's working day.

## 2. Product principles

These decide trade-offs when requirements conflict or are unclear.

| # | Principle | What it means in practice |
|---|---|---|
| P1 | **Stay out of the way.** | No sign-in, no start screen, no pop-ups that interrupt. Open the app, type a URL, press Ctrl+Enter. |
| P2 | **Fast, always.** | Cold start under 1 s, a 1,000-request collection opens under 1 s, typing never lags, big responses never freeze the UI (R-NF-1, R-NF-2). Performance regressions are bugs. |
| P3 | **Keyboard first, mouse friendly.** | Every frequent action has a shortcut. Nothing *needs* the keyboard. |
| P4 | **Plain files are the product.** | Collections, environments and workflows are readable TOML that diff and merge well in git (R-GIT-1 to 8). If a feature can't be stored that way, rethink the feature. |
| P5 | **Git is the collaboration layer.** | Sharing happens by committing next to the code. No accounts, no cloud, no telemetry (R-P-1, R-P-2, R-P-4). |
| P6 | **Secrets never leak by accident.** | Secret values stay on the machine (R-VAR-3). Copying, exporting and logging mask them. |
| P7 | **One engine, many front-ends.** | Everything that isn't UI lives in `tortuga-core`, so the desktop app, a CLI runner and a web build all behave the same. |

## 3. A developer's day: the scenarios we design for

Every roadmap item should make at least one of these faster or safer.

| Scenario | What the developer needs |
|---|---|
| **S1 Explore** an endpoint while writing code. | A scratch request in seconds. Paste a `curl` command from docs, logs or browser dev tools. Readable responses. |
| **S2 Switch context**: local, dev, staging. | One click (or shortcut) to change environment; servers and variables follow. |
| **S3 Debug** a bug report. | Real credentials without committing them, the exact request that was sent, search inside large responses, history of what was tried. |
| **S4 Share** with a co-developer. | Commit the request next to the code, or hand over a `curl` command. Teammates' changes appear after `git pull`. |
| **S5 Review** an API change in a pull request. | Request files whose diff explains the change. |
| **S6 Verify** that flows still work (login → create → read → delete). | Chained requests with checks, run in the app and in CI with the same files. |

## 4. Status

Milestones M1 to M8 are done: sending requests, projects on disk, environments and servers, auth and secrets,
cookies, live git collaboration, workspaces, the v1 extras, release builds, and the Phase 2 "daily driver"
features. See [FEATURES.md](FEATURES.md) for what each one does and how it was verified.

Open before the first release (v1):

| Work | Requirement | What is left |
|---|---|---|
| Installers on every OS | R-NF-4 | Run the Release workflow; install and check the `.msi` and `.dmg` (and the `.deb` with `apt`). See [RELEASING.md](RELEASING.md). |
| macOS and Windows | R-P-3, R-HTTP-8 | Run the app on macOS at all, and use it beyond start-up on Windows; system proxy settings on both. |
| OS credential store | R-LOC-2 | Check Keychain, Credential Manager and Secret Service (only the file fallback is verified). |
| No telemetry | R-P-4 | Confirm with a network monitor. |
| Signing | D17 | Decide whether v1 installers are signed. |

## 5. Phases

The phases are an order, not a calendar. Each one ends with something a team can use every day. Phase 1 (v1: share
collections safely) and Phase 2 (daily driver) are done.

### Phase 3 — Workflows and tests: verify APIs (v2)

The next big theme, and the one that makes the git-shared collection worth more than a set of bookmarks (S6).
It builds in layers, so each layer is useful on its own:

1. **Test scripts on a single request** (IDEA-23, D3): JavaScript that runs after the response arrives,
   with a small `expect`-style API for status, headers, JSON values and response time. Results show next to
   the response. The script is stored in the request file, so it is reviewed in git like code.
2. **Capture values** from a response into variables (e.g. a token or a created id) so requests can be chained.
3. **Workflows** (IDEA-26): an ordered list of requests with captures and checks, stored as a file in the
   collection, run with one click, with a clear step-by-step result view.
4. **Command-line runner** (IDEA-24, D5): a separate `tortuga-cli` binary without GUI libraries. Runs a
   workflow or a folder from a terminal or CI with the same files, chooses the environment, exits non-zero on
   failure, writes a JUnit/JSON report. Secrets come from environment variables in CI.
5. **Data-driven runs** (later): run a workflow once per row of a data file.

Workflows are TOML files that reference existing request files by path, with captures and checks per step (D4):

```toml
# workflows/user-lifecycle.toml
name = "User lifecycle"

[[steps]]
request = "auth/login.toml"
capture.token = "$.access_token"

[[steps]]
request = "users/create-user.toml"
capture.user_id = "$.id"
```

What D3 (JavaScript) brings with it, to settle before Phase 3 starts:

- **Engine choice.** It must run in `tortuga-core`, be sandboxed (no file or network access unless granted),
  have time and memory limits, and keep the wasm build working (D6). Candidates: Boa (pure Rust, compiles to
  wasm, slower) and QuickJS via `rquickjs` (fast and small, C code, needs checking for wasm).
- **API surface.** A small, documented API (`res`, `expect`, `vars`) that feels familiar to anyone who has
  written API tests elsewhere, but is designed for short, readable scripts.
- **Pre-request scripts** (also IDEA-23) use the same engine; decide whether they ship with test scripts or later.

### Phase 4 — Reach

Picked by demand, not in a fixed order:

- Import OpenAPI (IDEA-29), then Postman collections (IDEA-10), so teams can adopt Tortuga gradually (D7).
- GraphQL, WebSockets, Server-Sent Events, gRPC (IDEA-25).
- Client certificates / mTLS (IDEA-20).
- Code snippets for other languages (IDEA-22).
- Web version (IDEA-11): the code keeps compiling for wasm, but nothing is built for it until a team asks (D6).
- In-app commit/pull/push (IDEA-21) — only if working outside the app proves to be a real pain point.

### Not on the roadmap

Accounts, cloud sync, real-time co-editing (IDEA-40); mock servers, documentation generation, monitors
(IDEA-41). They conflict with P4/P5 or are outside the purpose of an API client. See [§7](#7-ideas).

## 6. Requirements

Requirements still open. They are implemented, but not yet verified everywhere ([FEATURES.md](FEATURES.md),
"In progress").

| ID | Requirement |
|---|---|
| R-P-3 | **Cross-platform.** Windows, Linux, macOS desktop in v1. Web later, so the engine must not depend on desktop-only APIs. |
| R-P-4 | **No telemetry.** Tortuga only contacts hosts the user sends requests to. |
| R-HTTP-8 | Settings: follow redirects (on/off), verify TLS certificates (on/off, with a visible warning when off), proxy from system settings. |
| R-LOC-2 | Secret values are stored in the OS credential store (Windows Credential Manager, macOS Keychain, Linux Secret Service). If none is available, they fall back to a file in the app-data directory readable only by the current user, and the app shows a warning. |
| R-NF-4 | Installable packages: `.msi` (Windows), `.dmg` (macOS), `.deb` and `.AppImage` (Linux). |

## 7. Ideas

Candidates that are not requirements yet. To add one, append a row with the next free `IDEA-n` (see [§9](#9-how-to-change-this-roadmap)).

### Planned for later

Agreed as worth doing, not scheduled yet.

| ID | Idea | Notes |
|---|---|---|
| IDEA-6 | Cookie viewer: list cookies in the jar and delete individual ones. | Extends R-CK-3. |
| IDEA-8 | "Compare" option when a file changed on disk while it had unsaved edits. | Extends R-GIT-11. |
| IDEA-10 | Import Postman collections (v2.1 JSON). | Once our own format is stable. |
| IDEA-11 | Web version of the app. | The architecture already keeps this possible (R-P-3). |
| IDEA-14 | Signed installers: Apple Developer ID signing and notarization for the `.dmg`, Authenticode for the `.msi`. | Removes the "unidentified developer" and SmartScreen warnings. Needs paid certificates; see D17. |
| IDEA-15 | Automatic update check and in-app updates. | Would contact a server, so it must be opt-in (R-P-4). |

### Under consideration

Not decided. Needs a real user need before it moves up.

| ID | Idea | Notes |
|---|---|---|
| IDEA-20 | Client certificates (mTLS). | |
| IDEA-21 | In-app commit, pull, push, and branch switching. | Large scope: needs git credential handling on three OSes. |
| IDEA-22 | Code snippets for other languages (Python, JavaScript, …). | |
| IDEA-23 | Pre-request scripts and response assertions/tests. | Written in JavaScript (decision D3). |
| IDEA-24 | Command-line runner that executes a collection, e.g. in CI. | The engine crate is already UI-independent. |
| IDEA-25 | Other protocols: GraphQL tooling, WebSockets, SSE, gRPC. | |
| IDEA-26 | Test workflows: an ordered list of requests with captured values (e.g. token, created id) and checks, stored as a file in the collection, runnable in the app and from the CLI runner. | Builds on IDEA-23 and IDEA-24. See Phase 3, decisions D3/D4. |
| IDEA-29 | Import an OpenAPI description as a collection (servers, folders per tag, example bodies). | Phase 4. |
| IDEA-31 | Several requests open at once in tabs. | Deferred by decision D2 (single editor + quick open + history first). |
| IDEA-38 | Workspace variables, local only: values used across projects, e.g. log in with the Auth project and use `{{token}}` in the others. | Requests that use them depend on the workspace, so teammates without it see them as undefined (R-VAR-5). Precedence to be decided. |

### Not planned

| ID | Idea | Reason |
|---|---|---|
| IDEA-40 | Accounts, cloud sync, real-time co-editing. | Conflicts with local-first and git-based collaboration (R-P-1, R-P-2). |
| IDEA-41 | Mock servers, API documentation generation, monitors. | Outside the purpose of an API client. |

## 8. Decisions

| ID | Question | Decision (2026-10-17) | Other options considered |
|---|---|---|---|
| D1 | Does v1 include any Phase 2 items? | **Yes**: keep unsaved edits across restarts (IDEA-12) and copy as `curl` (IDEA-4). | Only one of them; nothing extra. |
| D2 | One request at a time, or tabs? | **Single editor + quick open (Ctrl+P) + history.** Revisit tabs (IDEA-31) only if people keep switching back and forth. | Browser-style tabs; both. |
| D3 | How are checks written? | **JavaScript test scripts** run by an embedded, sandboxed engine. | Declarative TOML checks (first, or only); Rhai/Lua. |
| D4 | How are workflows stored? | **One TOML file per workflow that references request files**, with captures and checks per step. | Steps embedded in request files; self-contained workflow files. |
| D5 | What does the CLI ship as? | **A separate `tortuga-cli` binary** without GUI libraries. | Sub-command of the app; both. |
| D6 | Keep the web version as a goal? | **Keep the code compiling for wasm; build nothing web-specific** until a team asks. | Pursue it actively; drop it. |
| D7 | Which import first? | **`curl` paste, then OpenAPI, then Postman.** | Postman before OpenAPI; OpenAPI first; `curl` only. |

| D8 | Which library reads git status (R-GIT-14)? | **gitoxide (`gix`)**, pure Rust (2026-10-17). | `git2` (libgit2, C). |
| D9 | How is "keep mine / take theirs" offered (R-GIT-11)? | **A banner on the request**; nothing pops up (2026-10-17). | A dialog right away. |
| D10 | How do requests deleted in git show (R-GIT-12)? | **Greyed, struck-through rows** where they were (2026-10-17). | Only a count on the folder. |
| D11 | What does "open the collection folder" offer (R-GIT-13)? | **File manager and terminal** (2026-10-17). | File manager only. |
| D12 | What are the levels called (IDEA-32, 33)? | **Workspace → Project**: "collection" is no longer used in the UI or docs; `tortuga.toml` stays as it is (2026-10-17). | Workspace → Collection (Postman's terms); Workspace → Project → Collection (three levels). |
| D13 | Where is a workspace stored (IDEA-32, 36)? | **Locally by default, with "Save as file…" to share it** (2026-10-17). | Local only; always a file the user chooses. |
| D14 | How do environments work across projects (IDEA-37)? | **Switch by name across the workspace**; environments stay defined per project (2026-10-17). | Per project only (as today); workspace-level environments that projects inherit (a file format change). |
| D15 | When are workspaces built? | **Before M7**, so the first release ships the final names (2026-10-17). | After the release, in Phase 2. |

| D16 | How are the installers built (R-NF-4)? | **cargo-packager**: one tool and one config for `.msi`, `.dmg`, `.deb` and `.AppImage`, run by a release workflow on each OS (2026-10-17). | cargo-dist (no `.dmg`, `.deb` or `.AppImage`); one tool per format (cargo-wix, cargo-bundle, cargo-deb, appimagetool). |
| D17 | Are the v1 installers signed? | **Open.** Recommended: release v1 unsigned with instructions to open it anyway, and sign later (IDEA-14). | Sign from the first release (Apple Developer Program, about USD 99 a year, plus a Windows code-signing certificate); sign only the macOS build. |

| D18 | Which syntax does the JSON response filter use (R-HTTP-13)? | **A JSONPath subset**, implemented in `tortuga-core` (2026-10-17). | jq-style (`.items[].name`); text search only. |
| D19 | What does pasting a `curl` command into a saved request do (R-HTTP-11)? | **It fills the current request** as unsaved edits; Revert undoes it (2026-10-17). | Always fill the scratch request. |
| D20 | Where is history shown (R-HIST-2)? | **A History view in the sidebar**, next to Projects; an entry opens its request with that past response (2026-10-17). | Past responses per request in the response panel only; a separate history dialog. |

| D21 | How does a request file refer to a file to upload (R-HTTP-15)? | **Relative to the project folder when the file is inside it**, otherwise the full path, marked as working only on this computer (2026-10-17). | Always the full path; copy the file into the project. |

| D22 | Where are custom CA certificates configured (R-HTTP-16)? | **In the app's Settings**, as local data: file paths differ per machine (2026-10-18). | Per project, shared through git; both. |
| D23 | Can variables be reordered (IDEA-13)? | **No: they stay sorted by name**, which keeps `tortuga.toml` and environment files stable in diffs (R-GIT-3); environments and servers can be reordered (2026-10-18). | Store variables as an ordered list (a file format change). |

New questions go here as `D24`, `D25`, … with options, until they are decided.

## 9. How to change this roadmap

- New idea: add a row to [§7](#7-ideas) with the next free `IDEA-n`.
- Open question: add it to [§8](#8-decisions) as the next `Dn` with options; record the choice and date when decided.
- When an idea gets a phase, give it a requirement ID in [§6](#6-requirements) (next free number in its area) and
  remove the idea row; the requirement says *(IDEA-n)*.
- When a requirement is fully met and verified, move its row to FEATURES.md ("Agreed requirements") and record
  how it was verified there.

Next free IDs (removed rows keep their numbers): `R-P-5` · `R-HTTP-17` · `R-CK-5` · `R-COL-13` · `R-WS-8` · `R-VAR-8` · `R-SRV-7` · `R-GIT-15` · `R-HIST-4` · `R-LOC-3` · `R-NF-5` · `IDEA-42` · `D24`.
