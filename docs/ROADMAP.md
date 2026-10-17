# Tortuga roadmap

> The single planning document: vision, principles, the plan, the requirements, ideas and decisions.
> [FEATURES.md](FEATURES.md) records what is implemented and verified.
>
> - **Requirements** (`R-AREA-n`, [§6](#6-requirements)) describe agreed behaviour precisely.
> - **Ideas** (`IDEA-n`, [§7](#7-ideas)) are candidates; one becomes a requirement when it gets a phase and an `R-` ID.
> - **Decisions** (`Dn`, [§8](#8-decisions)) record choices and the options considered.
> - IDs are never reused or renumbered. Code, tests, commits and FEATURES.md refer to them.

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

## 4. Status and milestones

Milestones are built in order. Each one ends with something usable that can be tested on all three operating systems.
A requirement moves to [FEATURES.md](FEATURES.md) once it is implemented and verified.

| Milestone | Goal | Requirements | Status |
|---|---|---|---|
| **M1: Send a request** | A complete single-request editor: build any request, send it, inspect the response. | R-HTTP-1, 2, 3, 4, 6, 7, 8 · R-COL-7 · R-P-4 | Done (see FEATURES.md) |
| **M2: Collections on disk** | Organise requests in collections that are saved as TOML files and can be committed to git. | R-COL-1, 2, 3, 5, 6 · R-GIT-1 to 8 · R-P-1 | Done (see FEATURES.md) |
| **M3: Environments, servers & variables** | Switch between dev/staging/prod: each server gets its own address per environment, and `{{variables}}` fill in the rest. | R-SRV-1 to 5 · R-VAR-1, 2, 4, 5, 7 · R-COL-4 (variables) | Done (see FEATURES.md) |
| **M4: Auth & secrets** | Share collections without leaking credentials. | R-VAR-3 · R-HTTP-5 · R-COL-4 (headers, auth) · R-LOC-1, 2 | Done, except R-LOC-2 on a real OS credential store (see FEATURES.md) |
| **M5: Cookies** | Session-based APIs work, with per-request control. | R-CK-1 to 4 | Done (see FEATURES.md) |
| **M6: Live git collaboration** | Teammates' changes appear automatically, and the git state is visible. | R-GIT-10 to 14 | Done (see FEATURES.md) |
| **M6.5: Workspaces & projects** | Group projects (today's collections) into workspaces, also when several share a repo. Built before M7 (D15). | R-WS-1 to 7 | Done (see FEATURES.md) |
| **M7: Release** | Installable, fast builds for all three platforms. | R-NF-1 to 4 · R-P-3 | Built; verified on Linux. Installers for Windows and macOS are built by the release workflow and still need a check on those OSes (see FEATURES.md, [RELEASING.md](RELEASING.md)) |
| **UI: egui** | Move the UI from Dioxus to egui (`tortuga-ui-egui`) with a modern look. | — | Done (2026-10-17) |
| **v1 extras** | Decided in D1, plus path variables (requested 2026-10-17). | R-COL-8 · R-HTTP-9 · R-HTTP-10 | Done (see FEATURES.md) |

Since M4, credentials belong in secret variables (R-VAR-3). The app warns where a credential is typed as plain text,
because everything else is written to the collection files and shared through git.

## 5. Phases

The phases are an order, not a calendar. Each one ends with something a team can use every day.

### Phase 1 — v1: a team can share collections safely (M4–M7)

Specified in [§6 Requirements](#6-requirements); order as in the milestone table in [§4](#4-status-and-milestones):

1. **M4 Auth & secrets.** Basic/Bearer auth, collection defaults for headers and auth, secret variables in the
   OS credential store. *This unblocks real use: today environment values are committed.*
2. **M5 Cookies.** Per-collection cookie jar, per-request opt-out.
3. **M6 Live git collaboration.** Reload on disk changes, "keep mine / take theirs", git status in the tree.
4. **M7 Release.** Installers, start-up budget measured on all three OSes.

Also in v1 (D1): **R-COL-8** keep unsaved edits across restarts / warn on close, and **R-HTTP-9** copy as
`curl` with secrets masked. They can be built alongside any of M4–M7.

### Phase 2 — Daily driver: remove friction (v1.x)

Small features with a big effect on S1–S4. Proposed order (cheapest high-value first):

| Item | Idea | Scenario |
|---|---|---|
| Paste a `curl` command into the URL bar to fill the request (first import, D7) | IDEA-28 | S1 |
| Quick open and command palette (Ctrl+P / Ctrl+K): the way to move between requests (D2) | IDEA-27 | S1, S2 |
| Search and filter in responses (text search; JSON path filter) | IDEA-30 | S3 |
| Request history per collection (local only) | IDEA-5 | S3 |
| Preview the resolved request, secrets masked | IDEA-7 | S3 |
| Folder-level defaults | IDEA-1 | S2 |
| `multipart/form-data` bodies and file upload | IDEA-2 | S1 |
| Custom CA certificates | IDEA-3 | S2 |
| Drag and drop in the tree; reorder environments | IDEA-9, IDEA-13 | — |

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

What the app must do, agreed in detail. Anything not listed here or in a phase is out of scope until it is
added.

### 6.1 Principles

| ID | Requirement |
|---|---|
| R-P-1 | **Local-first.** All data are plain files the user owns. No account, login, or cloud service is needed. |
| R-P-2 | **Git is the collaboration layer.** Teams share collections by committing them to a git repository, often next to the API source code. The on-disk format is effectively our public collaboration API. |
| R-P-3 | **Cross-platform.** Windows, Linux, macOS desktop in v1. Web later, so the engine must not depend on desktop-only APIs. |
| R-P-4 | **No telemetry.** Tortuga only contacts hosts the user sends requests to. |

### 6.2 HTTP requests

| ID | Requirement |
|---|---|
| R-HTTP-1 | Methods: `GET`, `POST`, `PUT`, `PATCH`, `DELETE`, `HEAD`, `OPTIONS`. |
| R-HTTP-2 | URL editor; query parameters also editable as a key/value table, kept in sync with the URL. Each param can be disabled without deleting it. |
| R-HTTP-3 | Headers as a key/value table; each header can be disabled. Duplicate header names are allowed. |
| R-HTTP-4 | Body types: none, raw text, JSON, `x-www-form-urlencoded`. Content-Type is set automatically unless overridden. |
| R-HTTP-5 | Auth: none, Basic, Bearer token. Set per request, or inherited from the collection. |
| R-HTTP-6 | Response view: status, time, size, headers, body. JSON bodies are pretty-printed; others shown as text; binary bodies can be saved to a file. |
| R-HTTP-7 | An in-flight request can be cancelled. Per-request timeout with a global default. |
| R-HTTP-8 | Settings: follow redirects (on/off), verify TLS certificates (on/off, with a visible warning when off), proxy from system settings. |
| R-HTTP-9 | Copy a request as a `curl` command with variables resolved. Secret values are masked unless the user confirms including them. *(D1; was IDEA-4)* |
| R-HTTP-10 | Path variables: a URL path segment starting with `:` (e.g. `/users/:id`) is a path variable. Its value is edited in a table that follows the URL, may contain `{{variables}}`, is URL-encoded when sent, and is saved in the request file. Sending stops with a message naming any path variable without a value. Only the path counts, so `host:8080` is not a variable. |

### 6.3 Cookies

| ID | Requirement |
|---|---|
| R-CK-1 | Each collection has a cookie jar. Cookies from responses are stored in it and sent with later matching requests, like a browser does. On by default. |
| R-CK-2 | **Cookies can be disabled per request.** A disabled request sends no cookies from the jar and does not store cookies from its response. The setting is saved in the request file. |
| R-CK-3 | The cookie jar is local data (§6.7), never written to the collection directory. The jar can be cleared. |
| R-CK-4 | A `Cookie` header set manually on a request is always sent, even when the jar is disabled for that request. |

### 6.4 Collections

| ID | Requirement |
|---|---|
| R-COL-1 | A collection is a named tree of folders and requests, with any nesting depth. |
| R-COL-2 | Create, rename, duplicate, move, and delete requests and folders. |
| R-COL-3 | The user controls the order of items; the order is saved. |
| R-COL-4 | The collection can define defaults that all its requests inherit: headers, auth, and variables. |
| R-COL-5 | Several collections can be open at once. The app remembers which were open. |
| R-COL-6 | Unsaved request edits are clearly marked. Saving writes to disk; sending a request does not save it. |
| R-COL-7 | Requests can be sent ad hoc without being saved to a collection. |
| R-COL-8 | Unsaved request edits survive closing and reopening the app (kept as local data, §6.7), or the app warns before closing. *(D1; was IDEA-12)* |

#### 6.4.1 Workspaces & projects

Decided in D12–D15. A **project** is what earlier versions called a collection; a **workspace** groups projects.

| ID | Requirement |
|---|---|
| R-WS-1 | A workspace is a named set of projects that are open together. A default workspace always exists. Workspaces are switched from the title bar, and each one remembers its open projects, active environments and selection. *(IDEA-32)* |
| R-WS-2 | A project is a directory with `tortuga.toml`: in its own git repo, in a repo shared with other projects, or outside git. The file format does not change. The UI and docs say "project", not "collection" (D12). *(IDEA-33)* |
| R-WS-3 | Add existing projects: the user picks a folder or repository, and the app lists every `tortuga.toml` below it as a checklist, skipping git-ignored folders. *(IDEA-34)* |
| R-WS-4 | Projects in the same repository are grouped in the sidebar, with the branch and git state shown once per repository, and one watcher per repository. *(IDEA-35)* |
| R-WS-5 | Workspaces are local data (§6.7). "Save as file…" writes a shareable `<name>.tortuga-workspace.toml` with each project's relative path and git remote URL. Opening one adds its projects; missing ones show greyed with "Locate…" and their remote URL (D13). *(IDEA-36)* |
| R-WS-6 | Choosing an environment switches every project in the workspace that has an environment with that name; projects without one keep theirs and are marked. Environments stay defined per project (D14). *(IDEA-37)* |
| R-WS-7 | New project: in a new folder (optionally initialised as a git repository by the app) or inside a folder of an existing repository. *(IDEA-39)* |

### 6.5 Variables & environments

| ID | Requirement |
|---|---|
| R-VAR-1 | `{{name}}` placeholders are substituted in URL, params, headers, body, and auth fields. |
| R-VAR-2 | A collection has any number of named environments (e.g. `dev`, `staging`), plus collection-level variables. One environment is active at a time. |
| R-VAR-3 | **Secrets are never written into the collection directory.** A variable can be marked secret; its value is stored only on the local machine (§6.7). Teammates see that the secret exists, not its value. |
| R-VAR-4 | Order of precedence, highest first: local secret value, then the active environment, then collection variables. |
| R-VAR-5 | Undefined variables are highlighted in the editor and reported before the request is sent. |
| R-VAR-7 | Collection variables and per-environment values are edited in one table: one row per variable, one column for the collection default and one per environment. An empty cell falls back to the collection default. |

#### 6.5.1 Servers

A collection usually talks to a few services (e.g. "Users API", "Auth"), and each environment has its own address for each one.
Servers make that a first-class concept instead of a naming convention for variables.

| ID | Requirement |
|---|---|
| R-SRV-1 | A collection defines a list of named servers. Renaming a server does not break the requests that use it. |
| R-SRV-2 | Each environment sets a base URL for each server, edited in one table: one row per server, one column per environment. Base URLs are shared data (committed to git) and may contain `{{variables}}`. |
| R-SRV-3 | A request picks a server from a dropdown in front of the URL field; the URL field then holds only the path and query (e.g. `/users?page=2`). "No server" means the URL field holds a full URL. |
| R-SRV-4 | When sending, the request goes to the selected server's base URL in the active environment plus the path. If the server has no URL in that environment, or no longer exists, sending stops with a message naming the server and environment. |
| R-SRV-5 | The active environment is chosen per collection from the title bar, and remembered locally (§6.7). |

### 6.6 Collaboration through git

#### 6.6.1 On-disk format

| ID | Requirement |
|---|---|
| R-GIT-1 | A collection is a directory and can live anywhere, inside or outside a git repo. Folders are directories. |
| R-GIT-2 | **One TOML file per request**, so concurrent edits to different requests never conflict. |
| R-GIT-3 | Output is deterministic, so saving an unchanged request never modifies its file. Key order is fixed, and only fields that differ from their defaults are written. |
| R-GIT-4 | Shared files hold only shared data. No timestamps, response history, window/UI state, cookies, or secret values. |
| R-GIT-5 | File names come from request names (e.g. `create-user.toml`). Renaming a request renames its file, so git sees a rename. |
| R-GIT-6 | Order is stored as a `seq` number in each request file, not in a central list, so two people adding requests don't both edit the same file. |
| R-GIT-7 | The collection manifest `tortuga.toml` holds a `format` version. Newer app versions migrate older formats. If a file uses a newer format than the app understands, it is shown read-only. |
| R-GIT-8 | Files are hand-editable. A file that fails to parse (e.g. it contains merge-conflict markers) is shown as an error on that item only; the rest of the collection still loads. |

Example layout:

```
my-api/                    # collection root, typically inside the API's git repo
  tortuga.toml             # manifest: name, format version, servers, collection variables
  environments/
    dev.toml
    staging.toml
  users/                   # a folder
    _folder.toml           # optional: folder display name and position (seq)
    list-users.toml
    create-user.toml
```

Example manifest and environment:

```toml
# tortuga.toml
format = 1
name = "Acme API"

[[servers]]
id = "users"               # stable key used by requests; the name can change freely
name = "Users API"

[variables]                # collection defaults
api_version = "v2"
```

```toml
# environments/dev.toml
name = "Development"
seq = 1

[servers]
users = "http://localhost:8080"

[variables]
user_id = "42"
```

Example request file:

```toml
name = "Create user"
seq = 2
method = "POST"
server = "users"
url = "/users"

[[headers]]
name = "Accept"
value = "application/json"

[body]
kind = "json"
content = '''
{
  "name": "{{user_name}}"
}
'''
```

#### 6.6.2 Working with git

| ID | Requirement |
|---|---|
| R-GIT-10 | The app watches open collections. Changes on disk (git pull, branch switch, edits in another editor) are reloaded automatically. |
| R-GIT-11 | If a file changes on disk while it has unsaved edits in the app, the user chooses: keep mine or take theirs. |
| R-GIT-12 | **Git status is shown in the app.** For a collection inside a git repo, show the current branch and mark each request/folder as modified, new, deleted, or conflicted. |
| R-GIT-13 | Commit, pull, push, and branch switching happen outside the app. The app can open the collection folder in the system file manager or terminal. |
| R-GIT-14 | Git status works without a `git` executable installed: it is read from the repository by the app itself. Desktop only. |

### 6.7 Local (non-shared) data

| ID | Requirement |
|---|---|
| R-LOC-1 | Secret values, cookies, the selected environment, and UI state are stored in the user's app-data directory, keyed by collection path. Nothing is written inside the repo, so there is no `.gitignore` to forget. |
| R-LOC-2 | Secret values are stored in the OS credential store (Windows Credential Manager, macOS Keychain, Linux Secret Service). If none is available, they fall back to a file in the app-data directory readable only by the current user, and the app shows a warning. |

### 6.8 Non-functional

| ID | Requirement |
|---|---|
| R-NF-1 | Cold start under 1 s on a typical developer laptop. A collection with 1,000 requests opens in under 1 s. |
| R-NF-2 | Response bodies up to 50 MB are handled without freezing the UI. Larger bodies are truncated in the viewer and can be saved to a file. |
| R-NF-3 | Fully usable offline. |
| R-NF-4 | Installable packages: `.msi` (Windows), `.dmg` (macOS), `.deb` and `.AppImage` (Linux). |

## 7. Ideas

Candidates that are not requirements yet. To add one, append a row with the next free `IDEA-n`
(planned 1–19, under consideration 20–39, not planned 40+).

### Planned for later

These were discussed and agreed as worth doing, but are not needed for v1.

| ID | Idea | Notes |
|---|---|---|
| IDEA-1 | Folder-level defaults (headers, auth, variables) inherited by the requests in that folder. | Extends R-COL-4. Would use an optional `_folder.toml` per directory. |
| IDEA-2 | `multipart/form-data` bodies, with text fields and file uploads. | Extends R-HTTP-4. |
| IDEA-3 | Custom CA certificates (e.g. internal APIs signed by a company CA). | |
| IDEA-4 | Copy a request as a `curl` command. Variables resolved; warn before copying if it contains a secret. | **Moved to v1 as R-HTTP-9** (D1). |
| IDEA-5 | Local request history per collection: last 200 requests and responses, with response bodies over 1 MB truncated. Browse, re-open, clear. | Local data only. |
| IDEA-6 | Cookie viewer: list cookies in the jar and delete individual ones. | Extends R-CK-3. |
| IDEA-7 | Preview the fully resolved request, with secret values masked. | |
| IDEA-8 | "Compare" option when a file changed on disk while it had unsaved edits. | Extends R-GIT-11. |
| IDEA-9 | Move requests and folders by drag and drop. | R-COL-2 covers moving through menus. |
| IDEA-10 | Import Postman collections (v2.1 JSON). | Once our own format is stable. |
| IDEA-11 | Web version of the app. | The architecture already keeps this possible (R-P-3). |
| IDEA-12 | Warn about unsaved edits when closing the app, or keep them across restarts. | **Moved to v1 as R-COL-8** (D1). |
| IDEA-13 | Drag environments and variables to reorder them in the environments table. | Today environments keep their creation order and variables are sorted by name. |
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
| IDEA-27 | Quick open and command palette (Ctrl+P / Ctrl+K): jump to any request, environment or action by typing. | Phase 2. |
| IDEA-28 | Paste a `curl` command into the URL bar to fill method, URL, headers and body. | Counterpart of IDEA-4. Phase 2. |
| IDEA-29 | Import an OpenAPI description as a collection (servers, folders per tag, example bodies). | Phase 4. |
| IDEA-30 | Search in responses, and filter JSON responses with a path expression. | Phase 2. |
| IDEA-31 | Several requests open at once in tabs. | Deferred by decision D2 (single editor + quick open + history first). |
| IDEA-32 | **Workspaces**: a named set of projects that are open together. Switch workspace from the title bar; each one remembers its open projects, active environments and selection. A default workspace always exists, so there is still no start screen (P1). Quick open (IDEA-27) searches every project in the workspace. | **Moved to M6.5 as R-WS-1** (D15). Replaces the flat list of open collections (R-COL-5). Naming D12, storage D13. Proposed 2026-10-17. |
| IDEA-33 | **Projects**: what is called a collection today. A directory with `tortuga.toml`, anywhere: in its own git repo, in a repo shared with other projects, or outside git. | **Moved to M6.5 as R-WS-2** (D15). No file format change. Naming D12. |
| IDEA-34 | Add existing projects: pick a folder or repository, and the app lists every `tortuga.toml` below it (skipping git-ignored folders such as `target/` and `node_modules/`) as a checklist. | **Moved to M6.5 as R-WS-3** (D15). Makes monorepos and "clone, then open" quick. |
| IDEA-35 | Several projects in one repository: the sidebar groups projects by repository and shows the branch and git state once per repo. One watcher per repository instead of one per project. | **Moved to M6.5 as R-WS-4** (D15). Already works today: each collection finds its own repository (R-GIT-12). This makes it visible and cheaper. |
| IDEA-36 | Shareable workspace file (`<name>.tortuga-workspace.toml`): the projects by relative path, plus each project's git remote URL. Commit it to a "platform" repo; a new teammate opens it and sees which repos to clone. Projects that are missing show greyed with "Locate…". | **Moved to M6.5 as R-WS-5** (D15). Like VS Code's `.code-workspace`. Only shared data (R-GIT-4); the open/active state stays local. D13. |
| IDEA-37 | Workspace-wide environment switch: choosing "staging" switches every project that has an environment with that name. Projects without one keep theirs and are marked. | **Moved to M6.5 as R-WS-6** (D15). Makes S2 one click across services. D14. |
| IDEA-38 | Workspace variables, local only: values used across projects, e.g. log in with the Auth project and use `{{token}}` in the others. | Requests that use them depend on the workspace, so teammates without it see them as undefined (R-VAR-5). Precedence to be decided. |
| IDEA-39 | New project: create it in a new folder (optionally `git init`, done by the app with gix) or inside a folder of an existing repository. | **Moved to M6.5 as R-WS-7** (D15). |

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

New questions go here as `D18`, `D19`, … with options, until they are decided.

## 9. How to change this roadmap

- New idea: add a row to [§7](#7-ideas).
- Open question: add it to [§8](#8-decisions) as the next `Dn` with options; record the choice and date when decided.
- When an idea gets a phase, give it a requirement ID in [§6](#6-requirements) and mark the idea row "Moved to … as R-…".
- When something is implemented and verified, add it to [FEATURES.md](FEATURES.md).
