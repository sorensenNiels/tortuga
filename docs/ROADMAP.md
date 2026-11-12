# Tortuga roadmap

> What comes next: vision, principles, the open work, ideas and decisions.
> Everything that is built and verified is in [FEATURES.md](FEATURES.md); the requirements it was built to are in
> [REQUIREMENTS.md](REQUIREMENTS.md).
>
> - **Requirements** (`R-AREA-n`, [§6](#6-requirements)) describe agreed behaviour precisely. Once one is fully
>   met and verified it moves to REQUIREMENTS.md.
> - **Ideas** (`IDEA-n`, [§7](#7-ideas)) are candidates; one becomes a requirement when it gets a phase and an `R-` ID.
> - **Decisions** (`Dn`, [§8](#8-decisions)) record choices and the options considered. They stay here, done or not:
>   code and docs refer to them.
> - IDs are never reused or renumbered, also after they move to REQUIREMENTS.md. Code, tests, commits and FEATURES.md
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
| **S7 Design** an API before it is built. | Define each request with the responses it should give, serve them from a local mock so the client and its tests can be written now, then point the same tests at the real implementation (D43). |

## 4. Status

Milestones M1 to M9 are done: sending requests, projects on disk, environments and servers, auth and secrets,
cookies, live git collaboration, workspaces, the v1 extras, release builds, the Phase 2 "daily driver"
features, and Phase 3: scripts, captures, workflows and the `tortuga-cli` runner. Since then: request tabs,
imports from OpenAPI, Postman (with its scripts) and `.http` files, settings on three levels, XML, scripts in depth
(console, breakpoints, Beautify, snippets, tests from the response), mock APIs with the contract check, imports
from Bruno and Insomnia, one list of environments per workspace, the editor and layout settings, Postman test suites
run as Postman runs them, the UI in Danish, the VS Code extension, and where a send's time went. See
[FEATURES.md](FEATURES.md) for what each one does and how it was verified.

Pre-releases v0.1.0 to v0.11.0 are tagged, each built by the Release workflow on every OS. **v1** is the `v1.0.0` tag,
once this is checked:

| Work | Requirement | What is left |
|---|---|---|
| Installers on every OS | R-NF-4 | Install and check the `.msi` and `.dmg` from a Release workflow run (and the `.deb` with `apt`). See [RELEASING.md](RELEASING.md). |
| macOS and Windows | R-P-3, R-HTTP-8 | Run the app on macOS at all (Windows was used beyond start-up on 2026-10-26); system proxy settings on both. |
| OS credential store | R-LOC-2 | Check Keychain, Credential Manager and Secret Service (only the file fallback is verified). |
| No telemetry | R-P-4 | Confirm with a network monitor. |
| CLI archives | R-CLI-4 | Check the archives from a Release workflow run on each OS (only the Linux one was checked, built locally). |
| R-MCP-1 | **`tortuga-cli mcp`** (D72, D74). `tortuga-cli mcp [<project>]` serves a project (default: the current folder), with a workspace's shared environments, variables and settings when `--workspace FILE` is given as for `run`, over the Model Context Protocol on stdio, with `rmcp`, for an AI agent to start itself (Claude Code, Cursor, VS Code). It offers the tools of R-MCP-2 to R-MCP-4 and nothing else, reads the same files as the app, and uses this machine's secret values like `tortuga-cli run`. `--env NAME` sets the environment used when a tool names none. Nothing is printed to stdout but the protocol; errors go to stderr. *(IDEA-107)* |
| R-MCP-2 | **Browse tools** (D73). An agent can list the projects, folders, requests and workflows (the tree of `tortuga-cli list`), read a request's method, URL, params, headers, body, auth type, scripts, docs and examples as they are saved, and list the environments with each project's servers and variables. Secret variables are listed by name only; their values never leave Tortuga. Requests are read as saved on disk, never with the app's unsaved edits. *(IDEA-107)* |
| R-MCP-3 | **Send a request** (D73). An agent can send a saved request by its path, in a named environment (else the default of R-MCP-1 or R-MCP-5), with optional variable values that win over the environment's for that send only. Variables, auth, defaults and cookies apply and the request's scripts and tests run, as when it is sent in `tortuga-cli run`. The result has the status, time, headers, test results and the body, cut at 100 KB with its full size stated (a binary body is described, not returned). The request is shown with secret variables as `{{name}}`, not their values. In a **Protected** environment (R-SRV-9) only GET, HEAD and OPTIONS are sent; anything else is refused with that reason, since there is no one to ask, so an agent never changes production data. Nothing is saved to History. *(IDEA-107)* |
| R-MCP-4 | **Edit tools** (D73). An agent can create, change, rename, move and delete requests and folders, and add, change and delete a request's examples, through the same collection operations as the UI: the files change on disk, the app re-opens them, and git shows the change. Names are checked as in the UI. Edits to a read-only project are refused. An agent cannot set secret values. A request the app shows with unsaved edits is handled as any change on disk (R-GIT-11). *(IDEA-107)* |
| R-MCP-5 | **MCP server in the app** (D72, D75). **Start MCP server** in the workspace menu and the command palette serves the open workspace with the tools of R-MCP-2 to R-MCP-4 over Streamable HTTP at `http://127.0.0.1:4040/mcp` (another free port when 4040 is taken), with a random bearer token made at each start; a request without it is refused. It uses the environment chosen in the app when a tool names none, and runs while the app does. While it runs the top bar shows "MCP" and clicking it opens the **MCP server dialog**: the address, **Copy agent config** (the `claude mcp add` command, and JSON for Cursor and VS Code, each with the URL and token), a **log** of what agents did (requests sent with status, files changed) and **Stop**. Desktop only; not in the web version. *(IDEA-107)* |

Next: **Phase 4 — Reach**, picked by demand.

## 5. Phases

The phases are an order, not a calendar. Each one ends with something a team can use every day. Phase 1 (v1: share
collections safely), Phase 2 (daily driver) and Phase 3 (workflows and tests, v2) are done.

### Phase 4 — Reach

Picked by demand, not in a fixed order. Done so far: imports from OpenAPI, Postman, Insomnia, `.http` files and Bruno (R-COL-15
to R-COL-17, R-COL-31, R-COL-32, D7), settings on three levels (R-SET-1 to R-SET-7, D39), mock APIs with the contract check (R-MOCK-1 to
R-MOCK-8, D43 to D47), the UI in English and Danish (R-I18N-1 to R-I18N-3, D64), and a VS Code extension over
`tortuga-cli` (R-VSC-1 to R-VSC-4, D67). Still open:

- More from the importers, and new ones (see [Importers](#importers) below).
- GraphQL, WebSockets, Server-Sent Events, gRPC (IDEA-25).
- Client certificates / mTLS (IDEA-20).
- Code snippets for other languages (IDEA-22).
- More languages, when someone offers to keep one up to date (D64).
- Web version (IDEA-11): the code keeps compiling for wasm, but nothing is built for it until a team asks (D6).
- An MCP server for AI coding agents, in `tortuga-cli` and the app (R-MCP-1 to R-MCP-5, D72 to D75): built and tested
  on Linux; waiting for a check with a real agent (Claude Code, Cursor, VS Code) and on Windows and macOS (see
  [FEATURES.md](FEATURES.md#in-progress)).
- The full UI in VS Code (IDEA-97 (b)).
- Catching mistakes and committed credentials before a push (D76): `tortuga-cli check` and the secret guard in the app
  are done (R-CLI-5, R-CLI-6, R-VAR-10); the findings in VS Code's Problems panel (R-VSC-5) wait for a check by hand.
  The contract check in CI (IDEA-99) is already `tortuga-cli run --check-examples` (R-MOCK-8).
- Reviewing an API change in a pull request (S5, D77): `tortuga-cli diff` (R-CLI-7), then the same summary in the app
  (IDEA-108).
- **Add-ons** (D78): parts of the app not everyone needs are off until turned on in Settings → Add-ons, and are
  not loaded while off; each lives apart in the code (R-ADD-1 to R-ADD-3). The MCP server is the first; GraphQL
  (IDEA-25) and GeoServer (IDEA-109) would come as add-ons.
- In-app commit/pull/push (IDEA-21) — only if working outside the app proves to be a real pain point.

### Not on the roadmap

Accounts, cloud sync, real-time co-editing (IDEA-40); documentation generation, monitors (IDEA-41). They conflict
with P4/P5 or are outside the purpose of an API client. Mock servers were here until D43. See [§7](#7-ideas).

### Importers

What each import already does and the ideas for it, in one place. Every import creates a new project (D34), lists
what it can't import as written before importing, keeps credential values in this machine's secret store only (D35),
and can be turned off in Settings (R-COL-18). The idea rows here are ideas like those in [§7](#7-ideas) and share
its numbering; one that gets a phase becomes an `R-COL-` requirement in [§6](#6-requirements).

#### `curl` — done

**Done:** paste a `curl` command into the URL field (R-HTTP-11).

#### OpenAPI — done

**Done:** OpenAPI 3.0 and 3.1, JSON or YAML (R-COL-15, D33); responses become the requests' examples for the mock
(R-MOCK-6); descriptions of the API, tags and operations (R-COL-22).

| ID | Idea | Notes |
|---|---|---|
| IDEA-45 | Import again: update a project from a changed OpenAPI description, keeping local changes. | An import always creates a new project today (D34). |
| IDEA-46 | Import Swagger 2.0 descriptions. | Today they are refused with a hint to convert them (D33). |

#### Postman

**Done:** collections v2.0 and v2.1 with folders, variables, servers, auth, bodies and scripts (R-COL-16, D36);
scripts translated to Tortuga's API (R-COL-19), with Chai-style assertions to translate to (R-TEST-8);
environment files into a new or an existing project (R-COL-20, D49); descriptions of the collection, folders and
requests (R-COL-22); a real test-suite collection run as Postman's "Run collection" runs it: `xml2js`, more
assertions and the legacy API translated, with Run… on a project or folder and Postman's runner options (R-COL-39 to
R-COL-41, R-TEST-19, R-FLOW-7 to R-FLOW-9, D65, D66); cookies, `pm.info`, `replaceIn`, the data row and `setNextRequest` translated (R-COL-21), with
the script API they need (R-TEST-16, R-FLOW-6, D50); saved responses as examples and `disableCookies` (R-COL-30).

| ID | Idea | Notes |
|---|---|---|
| IDEA-59 | Import a request's `followRedirects: false` as its own follow-redirects choice. | Needs a per-request setting first: following redirects is a project setting (R-SET-1), so the import warns today (R-COL-30). `strictSSL: false` has no shared counterpart: hosts not to verify are an app setting only (R-SET-3). |
| IDEA-60 | Keep collection- and folder-level scripts: as project and folder scripts in Tortuga, or copied into each request below them. | Left out with a warning today (D36). They often get or refresh a token. Project and folder scripts would need a new decision. |
| IDEA-61 | Translate the rest of Postman's API that scripts use: `pm.sendRequest` (sending a request from a script, mostly to get a token) and `require('crypto-js')`/`CryptoJS` (hashes, HMAC and Base64 for signing requests). | Scripts using them are kept as comments today (R-COL-19). Each needs Tortuga's script API to offer it first: scripts can't send requests or hash. Cookies, `pm.info`, `replaceIn`, the data row and `setNextRequest` are R-COL-21. |
| IDEA-62 | Import Postman's data export (all collections, environments and globals in one archive) as a workspace: a project per collection, environments as in R-COL-20, globals as workspace variables (R-WS-8). | Builds on R-COL-20. For a team moving over all at once. |
| IDEA-63 | Import again: update a project from a changed Postman collection, keeping local changes. | As IDEA-45 for OpenAPI. For a team moving over gradually, with the collection still changed in Postman for a while. |
| IDEA-96 | Name the secret variables of request-level credentials after where they come from (`username_registrationcreate`, or the folder's name when every request in it has the same ones) instead of `username_2`. | Values that match the collection's are already shared; different ones get a number today (R-COL-16). |
| IDEA-72 | Translate lodash (`_`, `require('lodash')`) in Postman scripts, or offer it in Tortuga's scripts. | **Not planned**: modern JavaScript covers what Postman scripts mostly use it for, and bundling it would slow down every script run. Scripts using it are kept as comments to port. |
| IDEA-71 | Fetch a collection from Postman's API with an API key instead of from an exported file. | **Not planned**: it contacts a third-party host and needs a Postman account (R-P-4, P5). Exporting a file does the same. |

#### `.http` files — done

**Done:** VS Code REST Client and JetBrains HTTP Client files (R-COL-17, D37); several at once as a folder each,
with their environment files into a new or an existing project (R-COL-29, D58).

#### Insomnia — done

**Done:** v4 JSON and v5 YAML exports with folders, environments, auth, template syntax and scripts translated as
Postman's (R-COL-32, D60).

| ID | Idea | Notes |
|---|---|---|
| IDEA-93 | Import an Insomnia export with several collections as a workspace: a project per collection, as IDEA-62 for Postman's data export. | Only the first collection is imported today, naming the others (D60). |

#### Bruno — done

**Done:** a collection's folder with its folders, requests, auth, variables, environments and `.env` secrets;
assertions as tests and response values as captures (R-COL-31, D59); scripts translated to Tortuga's API
(R-COL-33); collections in Bruno's YAML format, with their saved examples (R-COL-34, D61).

#### Not done yet

| ID | Idea | Notes |
|---|---|---|
| IDEA-70 | Import a HAR file saved from a browser's dev tools: a request per entry, its response as the request's example. | Fits S1 (explore); `curl` paste takes one request at a time (R-HTTP-11). |

#### Engine features the importers need

Gaps the imports can only warn about today, because Tortuga has nothing to map them to. Each is a feature in its own
right; the imports would use it once it exists.

| ID | Idea | Notes |
|---|---|---|
| IDEA-66 | More auth types: Digest, AWS Signature v4, OAuth 1, NTLM, Hawk; and getting an OAuth 2 token in the app (client credentials, authorization code with PKCE). | Today auth is Basic, Bearer (R-HTTP-5) and any header, e.g. an API key (R-HTTP-22). The Postman import leaves the others out with a warning, and OAuth 2 becomes `Bearer {{access_token}}` (R-COL-16). |
| IDEA-67 | Send a file as the whole request body. | Postman's `file` bodies are left out today (R-COL-16). Multipart bodies can already upload files (R-HTTP-15). |

## 6. Requirements

Requirements still open: not built yet, or implemented but not yet verified everywhere ([FEATURES.md](FEATURES.md),
"In progress").

| ID | Requirement |
|---|---|
| R-P-3 | **Cross-platform.** Windows, Linux, macOS desktop in v1. Web later, so the engine must not depend on desktop-only APIs. |
| R-P-4 | **No telemetry.** Tortuga only contacts hosts the user sends requests to, and GitHub to look for a new version (R-SET-15, D63), which sends nothing about the user or their projects. |
| R-HTTP-8 | Settings: follow redirects (on/off), verify TLS certificates (on/off, with a visible warning when off), proxy from system settings. |
| R-HTTP-26 | **Fold JSON and XML in the response.** In the formatted view of a JSON or XML body (and of a filter's result), each object or array that spans lines, and each XML element with child lines, has a ⏷ before its first line that folds it to that line followed by `… }` (or `… ]`, `… </name>`), which says how many lines are hidden and unfolds it when clicked (as does the ⏵). **Fold all** / **Unfold all** in the body's toolbar fold or unfold every section. A search match inside a folded section unfolds it when stepped to. Folds are forgotten for a new response or filter result. The raw view and plain text don't fold. |
| R-HTTP-27 | **Scroll the response body with the keyboard.** While the pointer is over the body, or it was the last thing clicked, and no text field has the keyboard: ↑/↓ scroll a line, ←/→ scroll sideways (when not wrapping), Page Up/Page Down a page, Home/End (or Ctrl/⌘+Home/End) to the top or end. Ctrl+Page Up/Down still switch tabs. Listed in the shortcut sheet. |
| R-LOC-2 | Secret values are stored in the OS credential store (Windows Credential Manager, macOS Keychain, Linux Secret Service). If none is available, they fall back to a file in the app-data directory readable only by the current user, and the app shows a warning. |
| R-NF-4 | Installable packages: `.msi` (Windows), `.dmg` (macOS), `.deb` and `.AppImage` (Linux). |
| R-WS-11 | **Projects in other repositories are found by remote** (D70). In a workspace file (R-WS-5), a project in the file's own repository, or without a git remote, keeps its path relative to the file; a project in another repository is listed by that repository's remote URL and its `folder` in it (format 4, written only when there is one), with no path. Opening the file finds such a project where it was located before, else in a clone of the same repository that any workspace on this machine has a project of (remotes match however written: SSH or HTTPS, user, port, `.git`, case), else shows it missing as today, where a clone next to the file's folder would be. **Locating a missing project is kept on this machine** and never changes the file: an entry keeps the path the file lists; a teammate's other way of writing the remote doesn't replace it. *(Changes R-WS-5)* |
| R-WS-12 | **The workspace's servers are in every environment and every project** (D71). The servers in the values all projects share (R-WS-9) are one list for the workspace: adding, renaming or removing one in an environment's view does so in every environment, and each environment gives its own base URL (empty: none there). Saving the environments dialog gives every project that is not read-only a server of each of those names in its `tortuga.toml`, if it has none, so the project still works on its own in git and `tortuga-cli`. Until then, and for a project that joins later, the request's server picker lists them under "From the workspace" (picking one adds it to the project), and the dialog shows them among the project's servers, greyed with the workspace's URL (typing a URL adds it). The scratch request lists none. *(Extends R-WS-9)* |
| R-COL-43 | **Collapse or expand all folders of a project or folder.** The "…" menu (its Arrange submenu, R-COL-45) of a project with folders (or a Workflows group) and of a folder with folders has **Collapse all**, which folds every folder below it at any depth, and the project's Workflows group, and leaves the row itself open so its top level shows; and **Expand all**, which unfolds the row and everything below it. Other rows keep their folded state. Kept per workspace like other folded rows (R-COL-36). *(Extends R-WS-13 to one project)* |
| R-COL-44 | **Sort a project or folder.** "Sort A–Z" in the Arrange submenu of a project's or folder's "…" menu puts folders first, then requests, each by name (ignoring case; numbers by value, so "v2" before "v10"), in that folder and every folder below it. The new order is saved in the files like "Move up/down" (R-COL-3), so teammates and `tortuga-cli` see it; a level already in that order is not rewritten. Not offered in read-only projects or where there is nothing to sort. |
| R-COL-45 | **A grouped project menu.** A project's "…" menu lists the frequent actions first (New request, New folder, New workflow, Run…; then Rename, Environments, Defaults, Description), then submenus by theme: **Arrange** (Collapse all, Expand all, Sort A–Z), **Mock server** (start, or show and stop), **Runtime data** (runtime variables, cookies), **Import** (environment imports that are on) and **Files** (open in file manager or terminal, reload from disk), and last Remove from workspace. A submenu with nothing to offer is not shown. A folder's menu has the same Arrange submenu. *(Changes R-COL-42: the project menu's "More" becomes "Import")* |
| R-COL-46 | **Filter the sidebar.** A filter box under the scratch request narrows the sidebar to the current workspace's projects, folders, requests and workflows whose name matches, with the same fuzzy matching as quick open (R-COL-9: every word, in any order, "gtusr" finds "Get user"). A match shows with the projects and folders above it, unfolded, so you see where it is; a matching project or folder shows everything in it. Projects with no match, missing projects, unreadable files and requests deleted in git are hidden while filtering. Folded rows come back as they were when the box is cleared (🗙 or Esc). "Nothing matches the filter" when nothing does. The text is not kept after a restart. |
| R-COL-47 | **Search request bodies.** A `{}` switch at the end of the sidebar's filter box (R-COL-46) also finds requests by their saved body: a request matches when its body holds every word of the query, ignoring case (JSON, XML and text bodies; the names and values of form fields and multipart parts). Names still match as before. Off by default, not kept after a restart; unsaved edits are not searched. The search is done again only when the query, the switch or a project on disk changes, so big projects stay fast (R-NF-1). *(Extends R-COL-46)* |
| R-COL-48 | **Clear the scratch request.** While the scratch request is not empty, its sidebar row shows a 🗙 at its right end that empties it: method, URL, headers, body and auth back to a new request, and its last response is gone. Works whether or not the scratch request is the one shown; the shown request stays. No confirmation, since every sent version stays in History (R-HIST-2) and History's Remove (R-HIST-3) clears those. *(Extends R-COL-7)* |
| R-COL-49 | **Requests open on the tab they were left on.** Switching to another request and back opens the request editor on the tab it was left on (Params, Headers, Auth, Body, Options, Scripts with its sub-tab, Examples, Docs); a request not shown yet opens on Params. Kept per request, including the scratch request, while the app runs; it follows a rename and is gone with the request. Reloading the shown request (a change on disk, Revert, a pasted curl, restoring from History) keeps its tab. A tab the request no longer shows falls back to Params. *(Extends R-COL-13)* |
| R-WS-13 | **Collapse or expand all projects.** With two or more projects in the workspace, the sidebar's Projects heading has **Collapse all** while any project shows its requests, which folds every project's row, else **Expand all**, which unfolds every project and the shared-repository rows above them (R-WS-4). Folders inside keep their own folded state. Kept per workspace like other folded rows (R-COL-36). |
| R-SET-18 | **Download command on Linux.** When a newer version is available (R-SET-15), on Linux a **Copy download command** menu sits beside the link on Settings → About (not in the top bar, which only has the link) and lists the release's `.deb` and `.AppImage` by file name; picking one copies `curl -fLO '<url>'` for it, with a toast saying so. The URLs come from GitHub's answer for the release (only `https` ones that can't break out of the quotes). Nothing is installed by the app. *(Extends R-SET-15)* |
| R-CLI-4 | Each release has a `tortuga-cli` archive per OS next to the installers, for CI: the binary with the README and licences, `.zip` on Windows, `.tar.gz` elsewhere. *(IDEA-43)* |
| R-VSC-5 | **`tortuga-cli check` in the Problems panel.** The VS Code extension runs `tortuga-cli check --report json` for each project when it opens and after a project file is saved, and shows each finding on its file in the Problems panel (as a warning), cleared when it is gone. *(IDEA-101)* |
| R-CLI-7 | **API changes between two revisions** (D77, S5). `tortuga-cli diff <project> [<rev>|<from>..<to>|<from>...<to>] [--report markdown|json --out FILE] [--fail-on-breaking]` compares a project at two git revisions (default: `HEAD` and the working tree; `...` compares with the merge base, as a pull request does) and prints a Markdown summary to post on the pull request: a count line, then a table with a row per request added (➕), removed (➖), moved (➡) or changed (✏), as `METHOD /path` and its name, with its first change, and a folded `<details>` with every change of a request that has more than one. Compared: method, URL path and server, params, headers and form fields by name, auth type, body type and JSON body shape (fields and their types, not values), and each example by name: status, Content-Type and JSON body shape. A removed file with the same method and path as an added one is a move. Values are never shown, so no credential reaches a comment (P6). Changes that may break clients are marked ⚠: anything removed, renamed or of another type, and a changed method, path, auth type, body type or example status; adding never is. Exits with 1 under `--fail-on-breaking` when one is marked, 2 for errors. Descriptions, scripts and captures are not compared. *(IDEA-98)* |
| R-ADD-1 | **Settings → Add-ons** (D78). A page of the settings dialog lists every add-on this build has, each with its name, one line on what it does and a switch. **All are off by default**; the app settings keep the ids of the add-ons turned on, so an add-on a later version brings starts off and an id this version doesn't know still loads. A switch takes effect when the dialog is saved, without a restart: turning an add-on on adds its menu items, commands and settings; turning it off stops what it runs (with a toast saying so) and removes them. An add-on that can't work in this build (desktop only, in the web version) is not listed. |
| R-ADD-2 | **An add-on that is off is not loaded** (D78). While off, nothing of it exists in the app: no menu items, command palette commands, top-bar items, dialogs, settings rows, shortcuts, listeners or background tasks, and none of its code runs at start-up. In the code each add-on's UI lives in its own module under `tortuga-ui-egui/src/addons/`, its engine code in its own `tortuga-core` module, and the rest of the UI reaches it only through the one list of add-ons, so adding or removing one touches that list and nothing else that is shared. A UI test opens the app with every add-on off and finds none of them. |
| R-ADD-3 | **The MCP server is an add-on** (D78). "MCP server" in Settings → Add-ons ("Lets AI coding agents browse, send and edit your requests"). **Start MCP server** in the workspace menu and the command palette, the top bar's "MCP" and the MCP server dialog (R-MCP-5) exist only while it is on; turning it off stops a running server. `tortuga-cli mcp` (R-MCP-1) is not affected: the switch is the app's. *(Changes R-MCP-5)* |

## 7. Ideas

Candidates that are not requirements yet. To add one, append a row with the next free `IDEA-n` (see [§9](#9-how-to-change-this-roadmap)).
Ideas for importing from other tools are listed per importer under [Importers](#importers) instead.

### Planned for later

Agreed as worth doing, not scheduled yet.

| ID | Idea | Notes |
|---|---|---|
| IDEA-8 | "Compare" option when a file changed on disk while it had unsaved edits. | Extends R-GIT-11. |
| IDEA-11 | Web version of the app. | The architecture already keeps this possible (R-P-3). |
| IDEA-14 | Signed installers: Apple Developer ID signing and notarization for the `.dmg`, Authenticode for the `.msi`. | Removes the "unidentified developer" and SmartScreen warnings. Needs paid certificates; decided after v1 (D17). |
| IDEA-15 | In-app updates: download and install a new version from the app. | The check is R-SET-15. Needs signed update files (IDEA-14), and must not fight the `.deb` (apt updates it). |
| IDEA-44 | A container image of `tortuga-cli` for CI. | The Release workflow attaches CLI archives (R-CLI-4); an image needs a registry to publish to. |

### Under consideration

Not decided. Needs a real user need before it moves up.

| ID | Idea | Notes |
|---|---|---|
| IDEA-20 | Client certificates (mTLS). | |
| IDEA-21 | In-app commit, pull, push, and branch switching. | Parked until a user needs it (2026-10-24). Design if it comes back: a setting under Settings → Features, off by default, desktop only; the actions run the system `git` (its credential helpers, SSH agent, hooks and signing do the work, so Tortuga handles no credentials; gix and `git2` would mean writing that for three OSes); the branch in the top bar becomes a menu; commit only the project's files, pull `--ff-only`, push to the upstream, switch between local branches. Merging, conflicts, new branches, stash and history stay outside: Tortuga is not a git client. Changes R-GIT-13. |
| IDEA-22 | Code snippets for other languages (Python, JavaScript, …). | |
| IDEA-25 | Other protocols: GraphQL tooling, WebSockets, SSE, gRPC. | GraphQL would be an add-on (D78): schema from introspection, query editor with completion, variables. |
| IDEA-54 | Dynamic mock answers: templates that read the incoming request (`{{request.path.id}}`, `{{request.body.$.name}}`, a timestamp, a random id), or a response script in the script engine. | Static examples first (D46, R-MOCK-3). |
| IDEA-55 | Stateful mocks: a `POST` that later `GET`s return, reset on restart. | Turns the mock into a fake backend; only if static examples prove too little. |
| IDEA-57 | Fault injection in the mock: drop the connection, answer slowly, return 5xx for a share of calls. | `delay_ms` (R-MOCK-3) covers slow answers. |
| IDEA-95 | Translate the engine's messages too: errors, import warnings, test and contract-check results, which stay English with R-I18N-2. | They are built in `tortuga-core` as English text; translating them means returning structured messages the UI words. |
| IDEA-97 | **Tortuga in VS Code.** (a) runs requests, folders and workflows through `tortuga-cli` in VS Code's Testing view, with code lenses and "Open in Tortuga": **became R-VSC-1 to R-VSC-4 (D67)**. Still an idea: (b) the full UI in a webview, the wasm build (IDEA-11) with a `Host` that reaches files, secrets and HTTP through the extension host, which also sidesteps the webview's CORS limits. | (b) is the web version's work plus a bridge, and changes D6 (nothing web-specific until a team asks). |
| IDEA-108 | **API changes in the app** (S5): a "Changes since…" dialog on a project that shows R-CLI-7's summary against a branch or commit picked from a list, with a button that copies its Markdown. | Uses the same comparison in `tortuga-core`; needs the read-only git `Storage` moved from `tortuga-cli` to where the app can use it too (D77). |
| IDEA-90 | **Find shared workspaces.** When "Add existing projects" (R-WS-3) finds a `*.tortuga-workspace.toml` in the chosen folder or repository, offer to open that workspace (R-WS-5) instead of adding its projects one by one; a project opened on its own whose repository has a workspace file that lists it gets a quiet hint with "Open workspace". | Today each teammate opens a committed workspace file once by hand ("Open workspace file…"), so a workspace in the repo is easy to miss. |
| IDEA-106 | **Say which repository an edit goes to.** In the environments dialog, under the values all projects share and under each project's, a quiet "Saved in …" names the workspace file's repository or the project's (from git, R-WS-4), so a change to a shared URL and one to a project's own value are not mistaken for the same commit. | The footer says where things are kept in general (R-WS-9); with projects in several repositories (R-WS-11) which repo gets dirty is still a guess. |
| IDEA-102 | **Repeat a workflow step until** a test passes, at most N times, with a wait between tries: polling an asynchronous API (`202 Accepted`, then a status URL until it is done) (S6). | Possible today with `workflow.next` back to the same step (R-FLOW-6), but each workflow writes the loop by hand. |
| IDEA-103 | **A `.env` file next to the code as a source of values**: a project names a git-ignored `.env` (or similar) whose values fill in its secret variables, so the app and the service being built share one local configuration. | Secret values are entered once per machine today (R-VAR-3). The Bruno import already reads a `.env` once (R-COL-31). |
| IDEA-105 | **Export a project as OpenAPI 3.1**: requests, descriptions and examples written as an API description, for the end of a design-first project (S7) or for code generators and other teams. | The reverse of R-COL-15. Borders on documentation generation (IDEA-41): would need a decision on where an export ends and documentation starts. |
| IDEA-109 | **A GeoServer add-on** (D78): GeoServer's REST API (workspaces, stores, layers, styles) and its OGC services (WMS, WFS, WMTS): read a service's capabilities, build GetMap/GetFeature requests from them, and show a map image or features in the response. | What it offers beyond requests with a project of its own needs a user who works with GeoServer. |

### UI and settings

All ideas from the UI review of 2026-10-21 became requirements (IDEA-73 to IDEA-89; see REQUIREMENTS.md). Its two rules
still apply to new UI ideas: **simpler first**, and **a setting earns its place only when developers really disagree
on it**; where a better default would do, the idea is the default, not a setting (P1).

### Not planned

| ID | Idea | Reason |
|---|---|---|
| IDEA-40 | Accounts, cloud sync, real-time co-editing. | Conflicts with local-first and git-based collaboration (R-P-1, R-P-2). |
| IDEA-41 | API documentation generation, monitors. | Outside the purpose of an API client. Mock servers were part of this idea until D43 made them R-MOCK-1 to R-MOCK-8. |

## 8. Decisions

| ID | Question | Decision (2026-10-17) | Other options considered |
|---|---|---|---|
| D1 | Does v1 include any Phase 2 items? | **Yes**: keep unsaved edits across restarts (IDEA-12) and copy as `curl` (IDEA-4). | Only one of them; nothing extra. |
| D2 | One request at a time, or tabs? | **Single editor + quick open (Ctrl+P) + history.** Revisit tabs (IDEA-31) only if people keep switching back and forth. *Revisited by D31: tabs (2026-10-18).* | Browser-style tabs; both. |
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
| D14 | How do environments work across projects (IDEA-37)? | **Switch by name across the workspace**; environments stay defined per project (2026-10-17). *Replaced by D62: workspace environments (2026-10-22).* | Per project only (as today); workspace-level environments that projects inherit (a file format change). |
| D15 | When are workspaces built? | **Before M7**, so the first release ships the final names (2026-10-17). | After the release, in Phase 2. |

| D16 | How are the installers built (R-NF-4)? | **cargo-packager**: one tool and one config for `.msi`, `.dmg`, `.deb` and `.AppImage`, run by a release workflow on each OS (2026-10-17). | cargo-dist (no `.dmg`, `.deb` or `.AppImage`); one tool per format (cargo-wix, cargo-bundle, cargo-deb, appimagetool). |
| D17 | Are the v1 installers signed? | **No: v1 ships unsigned**, with instructions to open it anyway; signing (IDEA-14) is decided after v1 is out (2026-10-23). | Sign from the first release (Apple Developer Program, about USD 99 a year, plus a Windows code-signing certificate); sign only the macOS build. |

| D18 | Which syntax does the JSON response filter use (R-HTTP-13)? | **A JSONPath subset**, implemented in `tortuga-core` (2026-10-17). | jq-style (`.items[].name`); text search only. |
| D19 | What does pasting a `curl` command into a saved request do (R-HTTP-11)? | **It fills the current request** as unsaved edits; Revert undoes it (2026-10-17). | Always fill the scratch request. |
| D20 | Where is history shown (R-HIST-2)? | **A History view in the sidebar**, next to Projects; an entry opens its request with that past response (2026-10-17). | Past responses per request in the response panel only; a separate history dialog. |

| D21 | How does a request file refer to a file to upload (R-HTTP-15)? | **Relative to the project folder when the file is inside it**, otherwise the full path, marked as working only on this computer (2026-10-17). | Always the full path; copy the file into the project. |

| D22 | Where are custom CA certificates configured (R-HTTP-16)? | **In the app's Settings**, as local data: file paths differ per machine (2026-10-18). | Per project, shared through git; both. |
| D23 | Can variables be reordered (IDEA-13)? | **No: they stay sorted by name**, which keeps `tortuga.toml` and environment files stable in diffs (R-GIT-3); environments and servers can be reordered (2026-10-18). | Store variables as an ordered list (a file format change). |
| D24 | Which JavaScript engine runs scripts (D3)? | **Boa**: pure Rust, works in the web build (D6), sandboxed (2026-10-18). | QuickJS via `rquickjs` (faster, C code, no web build). |
| D25 | Do pre-request scripts come with test scripts? | **Yes, in Phase 3**, same engine and API (2026-10-18). | Tests only, pre-request later. |
| D26 | How long do captured and script-set values live? | **Until the app closes**, per project, never on disk; workflow runs start clean (2026-10-18). | Kept locally across restarts; written to the environment file. |
| D27 | Are data-driven runs part of Phase 3? | **Yes** (2026-10-18). | Later. |
| D28 | What does the test API look like? | **Jest-like**: `test(name, fn)` and `expect(x).toBe(y)` with `.not` (2026-10-18). | Chai-style `pm.expect(x).to.equal(y)` (Postman); declarative checks only. |
| D29 | Where are workflow files? | **`workflows/` in the project**, like `environments/`; not shown as a folder of requests (2026-10-18). | Anywhere in the tree with a `.workflow.toml` suffix. |
| D30 | How does CI pass secrets to `tortuga-cli`? | **Environment variables `TORTUGA_VAR_<name>`**, plus `--var name=value` (2026-10-18). | A secrets file; only `--var`. |
| D31 | Tabs after all (revisits D2)? How do they not pile up? | **Tabs with a preview tab** (R-COL-13): single click previews, double click or an edit keeps the tab; sending does not keep it, since history has the response. Only requests open in tabs. No automatic closing of tabs; bulk close, a list of all tabs and Ctrl+Tab instead (2026-10-18). | Tab limit that closes the least recently used saved tab; tabs per project; an "open requests" list in the sidebar. |
| D32 | How are tabs of several projects told apart? | **Automatic groups per project** (R-COL-14): a coloured chip per project, collapsible, shown only with two or more projects; colours worked out from the project name (2026-10-18). | Colour stripe only; a project switcher above the tabs; groups only in the tab list; colours chosen by the user. |
| D33 | Which descriptions does the OpenAPI import read (R-COL-15)? | **OpenAPI 3.0 and 3.1, JSON or YAML**, local `$ref`s only; Swagger 2.0 is refused with a hint to convert it (2026-10-18). | Also Swagger 2.0; also references to other files and URLs. |
| D34 | Where does an import go? | **Always a new project**, in a folder next to the description by default; re-importing is IDEA-45 (2026-10-18). | Into an existing project, merging by operation. |
| D35 | How are an API's servers and credentials mapped? | **One server per API with a base URL per environment** (each `servers` entry is an environment); **security schemes become auth with secret variables** (`{{token}}`, `{{username}}`/`{{password}}`, API keys by name), so nothing secret is written (2026-10-18). | A server per `servers` entry; plain variables for credentials. |
| D36 | How are Postman's base URLs, credentials and scripts mapped (R-COL-16)? | **A `{{variable}}` that starts request URLs becomes a server**, its value the base URL of a "Default" environment; **credentials become secret variables**, and values written in the collection go to this machine's secret store, never into the files (as D35); **scripts are translated** to Tortuga's API when every `pm` call in them is one the import knows, and otherwise kept as comments with a note, since the `pm` API is not Tortuga's (D28; translation added 2026-10-19, R-COL-19); collection- and folder-level scripts are left out with a warning (2026-10-18). | Keep `{{baseUrl}}` as a plain variable; drop credential values; always keep scripts as comments; run half-translated scripts. |
| D37 | How is a `.http` file mapped (R-COL-17)? | **One file is one new project** named after the file, a request per `###` block named by its `###` title, else `# @name`, else the comment above it; `@variables` as project variables and base URLs as servers (D36); `Authorization` and key/token headers as auth or headers with secret variables (D35), moved to the project when every request has the same auth; **REST Client request variables become captures** (R-TEST-4); JetBrains handler scripts are kept as comments (2026-10-18). | A folder per file in an existing project; keep request variables and warn. |

| D38 | How do workspace variables work (IDEA-38)? | **Shared through the workspace file, secrets local; below every project value; captures opt in with a `workspace.` name** (2026-10-18). Shared, because the point is collaboration; lowest, so existing projects behave as before and a project can always override; opt-in, so runtime values stay per project by default (D26). | Local only; ranked above project variables; every captured value shared; static values only. |

| D39 | How do settings work across app, workspace and project (IDEA-50)? | **Three levels; nearest wins** (request → folder → project → workspace → app). Timeout, follow redirects, cookies (R-SET-1) and default headers and auth (R-SET-2) cascade. **TLS verification is only turned off on this machine**, per host (R-SET-3). No per-machine project layer: app settings cover "just me". App-only next: theme, text size, history size, wrapping (R-SET-4..7). Every setting has a default that matches today's behaviour (2026-10-18). | Turning TLS off in shared files with a warning, or for local hosts only; a fourth, per-machine project layer; one settings dialog with a level picker. |

| D40 | How are XML responses filtered and captured (R-HTTP-19)? | **An XPath subset**, implemented in `tortuga-core` on `quick-xml` (already in the build), like the JSONPath subset (D18). **Names without a prefix match any prefix**, so `//Body` finds `soap:Body` without declaring namespaces; a prefixed name matches as written. Captures say `xpath:` in front, since `JSONPath` needs no prefix. Request files with an XML body say `kind = "xml"` without a new format version: older builds report such a file as unreadable rather than losing data (2026-10-20). | Full XPath 1.0 through a library (`sxd-xpath`: unmaintained, needs namespace setup for SOAP); a CSS-selector-like syntax; telling captures apart by a leading `/`. |
| D41 | How are environments edited when a project has several (R-SRV-7)? | **One at a time**: a list of environments beside the chosen one's values, with "Defaults" for what they share (names, secret flags, defaults). A table with a column per environment got too wide to read with more than two or three (2026-10-20). | Keep the table and make the dialog wider; a tab per environment. |
| D42 | How are scripts debugged (R-TEST-10)? | **Snapshot breakpoints**: a breakpoint records the variables in scope and the script runs on; the Console shows each hit afterwards. Works with Boa as it is, on every platform, and never holds up a send (2026-10-20). | A pausing debugger with Continue/Step (Boa has no debugger API: needs scripts on a thread, line-by-line rewriting, and the send's timeout suspended); console only. |
| D43 | Are mock servers in scope (revisits IDEA-41)? | **Yes: mock APIs served from example responses saved with the requests** (R-MOCK-1 to R-MOCK-6), for designing an API before it exists and testing against it (S7, S6). They fit P4 and P5: plain files in git, served locally, no account. Documentation generation and monitors stay out (2026-10-20). | Keep them out and leave mocks to other tools (Prism, WireMock) fed by an OpenAPI description; a hosted mock (conflicts with P5). |
| D44 | Where do mock responses live (R-MOCK-1)? | **In the request file as `[[examples]]`** (2026-10-20). The request is the definition, so one file's diff shows the request and its responses (S5), and a rename or move takes them along. | A `mocks/` folder per project, like `workflows/`, with a file per endpoint; only in an OpenAPI description the project points at. |
| D45 | How do requests reach the mock (R-MOCK-4)? | **A "Mock" environment that exists while the mock runs** (2026-10-20), built from the servers: switching to it is the same one click as switching to staging (S2). | A per-request "send to mock" switch; the user adds an ordinary environment pointing at `localhost` by hand. |
| D46 | Static or dynamic answers, and where does the listener run (R-MOCK-2, R-MOCK-3)? | **Static examples with match conditions first** (2026-10-20) (dynamic answers are IDEA-54), and **a listener in `tortuga-core` for native targets only** on `hyper`, which is already in the build through `reqwest`, so the app and `tortuga-cli` share it (P7). | Response scripts in the script engine from the start; an `axum` dependency; a listener per binary. |
| D47 | How does the contract check judge a response (R-MOCK-8)? | **By shape, not values**, against the one example that fits the response's status (and the request, through its `match` conditions): status, media type, and JSON keys and value types, with keys missing from either side both counting, since an undocumented field is drift too. **A test result**, so the Tests tab, the CLI's exit code and the reports show it with nothing new to learn. **On for a send in the app, opt-in for workflows** (`--check-examples`), so a workflow passes or fails the same way in the app and in CI unless CI asks for more (2026-10-20). | Compare values too (fails on every real id and date); ignore keys the example lacks (misses fields added without updating the design); a separate "Contract" tab; a per-project switch; on for workflows by default. |
| D48 | Where is an environment's colour kept, and which colours (R-SRV-8)? | **In the environment file, from a fixed set of named colours** (2026-10-20). Shared, because the point is that everyone sees production in red (P5); named rather than RGB, so each theme draws a shade that reads on it and the file stays readable. | Local only, per machine (like the active environment); any RGB colour from a colour picker; colours worked out from the name, like projects' (D32). |
| D49 | How are Postman environment files imported (R-COL-20)? | **Into the new project while importing its collection, and into an existing project from its menu**; an environment with the file's name is **updated** (what the file sets replaces, the rest is kept), so importing a newer export again just works (2026-10-20). | Only while importing a collection; only into an existing project; always add a copy ("Staging 2"). |
| D50 | How do scripts change the order of a workflow run (R-FLOW-6)? | **`workflow.next(stepName)` and `workflow.stop()` in scripts**, by the step's name as the results show it; the last call in a step counts and takes effect after the step; at most 1000 steps per iteration (2026-10-20). Like Postman's `setNextRequest`, so imported scripts translate one to one, but by step rather than request, since a workflow may run a request in several steps. | Conditions in the workflow file (a step's `next`/`until`); jumping by request path; no step limit. |
| D51 | Where do descriptions live, and how are they shown (R-COL-22)? | **In the file each describes, as Markdown, shown rendered with an Edit toggle** (egui_commonmark, without images); a project's and a folder's on a tab of their Defaults dialog (2026-10-21). No format bump: v1 isn't released, so no older build can drop them on save. | Plain text without rendering; a `README.md` per project and folder; an overview page when a project or folder row is clicked. |
| D52 | Which dynamic variables, and how often do they change (R-VAR-8)? | **The ones Postman and the `.http` clients share, under each tool's names, with a new value at every place they appear** (2026-10-21), so imported requests behave as before; one value used twice is a `vars.set(…)` in a pre-request script. Random values come from `rand`, already in the build through the script engine. | One value per send; also REST Client's `$datetime` formats and offsets and JetBrains' `$random.alphabetic(…)` and the like; Postman's fake-data variables (`$randomFirstName`, …). |
| D53 | How does "Save automatically" work (R-SET-11)? | **One second after the last key press or click, and before another request or workflow opens or the app closes** (2026-10-21). Never over a file that changed on disk while the conflict banner waits (R-GIT-11), never the scratch request; off by default, since an explicit Save keeps half-finished edits out of `git diff`. | Only when leaving the request (fewer writes and file renames while typing a name, but `git diff` lags); on send. |
| D54 | Can a manual proxy have credentials (R-HTTP-20)? | **No, not yet** (2026-10-21): the URL is refused with a user name or password in it, so no password lands in the plain settings file. Covers mitmproxy, Charles, Fiddler and most company proxies. | A user name with the password in the OS credential store (R-LOC-2). |
| D55 | How does the preview open from the URL line (R-COL-24)? | **In place**: under the line, above the tabs, at most a third of the window tall with its own scrolling; open per request until closed, Escape closes it (2026-10-21). | A popup over the editor, closed by clicking elsewhere; keep the Preview tab and let the line switch to it. |
| D56 | How do protected environments ask (R-SRV-9)? | **Before every send that may change data, and once before a workflow run with such steps; `tortuga-cli` refuses those runs unless `--allow-protected`** (2026-10-21). A CI job that should touch production says so in its command. | Ask once per session per environment; ignore protection in the CLI. |
| D57 | What does "Send to every environment" send (R-SRV-10)? | **Plain sends**: each environment's values, secrets, servers, auth and cookies, but no scripts, captures or history, so comparing never changes state; shapes are compared, not values (2026-10-21). | Full sends with scripts, captures and history, closer to reality but leaking runtime values from one environment into the next. |
| D58 | How are several `.http` files and their environment files imported (R-COL-29)? | **Several files are one new project with a folder per file** (D34 still holds: never into an existing project), named after their folder; **environment files are found next to the `.http` files** as the JetBrains client finds them, besides being picked, and go in like Postman's (D49): into the new project, or into an existing one from its menu; `$shared` values are copied into each environment (2026-10-21). | A project per file; `$shared` as project variables; only environment files picked by hand. |
| D59 | How is a Bruno collection imported (R-COL-31)? | **From its folder, `.bru` files only, beside it as a new project** (D34); the declarative parts (`assert`, `vars:post-response`, request variables) are translated, scripts kept as comments as the first Postman import did (D36), and translating Bruno's script API is IDEA-91 (done since R-COL-33, D61); `{{process.env.X}}` becomes a secret variable with its `.env` value (D35) (2026-10-21). | Pick `bruno.json` only; import into the Bruno folder itself; translate scripts in the first version; keep `process.env` as plain variables. |
| D60 | How is an Insomnia export imported (R-COL-32)? | **v4 and v5 read into one tree and mapped once; one collection per import** (the first, the others named; IDEA-93), **the base environment as project variables and sub-environments as environments** (private ones secret, D35); **scripts through the Postman translation** with `insomnia` read as `pm`, since Insomnia's script API copies Postman's (2026-10-21). | A project per collection in the export; the base environment as a "Base" environment; scripts always as comments. |
| D61 | How are Bruno scripts and YAML collections imported (R-COL-33, R-COL-34)? | **Scripts are translated with the scanner the Postman translation uses (now shared), `res.body` as `res.json()`** since Bruno parses it; **YAML files are read into the blocks of `.bru` files** and mapped once, so both formats always import alike, with saved examples as the only addition (2026-10-21). | Keep Bruno scripts as comments; a separate mapping for YAML; translate `res.body` to the text. |
| D62 | How do environments work across a workspace's projects (R-WS-9)? Replaces D14. | **One list, kept by the workspace; projects only fill in values** (2026-10-22). Name, colour and protection are set once; values every project shares live with the workspace (server URLs by server name), each project's own values in its files and win. One dialog edits it all. Keeping environments per project (D14) meant creating and renaming each in every project, which teams found hard to follow. Two kinds of environment (the workspace's, plus a project's own of the same name or only in it, with "Move to workspace") was built and found too hard to explain. | Per project, kept in step from a project × environment table with a fallback per environment; workspace and project environments side by side with "Move to workspace"; environments only in the workspace file (a project no longer runs on its own in CI, and every service's URL lives in one file). |
| D63 | How does the app tell that a new version is available (R-SET-15)? Changes R-P-4. | **A check at start-up, on by default, that can be turned off, plus "Check now" in Settings → About**; the source is GitHub's latest published release; the result is a quiet link in the top bar, never a pop-up (P1) (2026-10-23). Contacting GitHub on its own is the one exception to "only the hosts you send requests to"; it sends nothing but the request. | Only a button the user clicks (R-P-4 unchanged); opt-in (off by default); package managers only (winget, Homebrew, apt); in-app installing (IDEA-15, needs signing). |
| D64 | How are UI strings translated (IDEA-94)? | **Project Fluent** (`fluent-bundle`), one `.ftl` file per language embedded in the binary, looked up by message ID through one function in `tortuga-ui-egui`; plurals and grammar are handled by translators in the file, and it is pure Rust, so the web build keeps working (D6). Strings move over area by area, with a test that every ID in the code exists in the English file (2026-10-23). | `rust-i18n` (YAML/TOML keys, a macro; simpler, weaker plurals); gettext `.po` files (familiar to translators, C tooling or less maintained Rust crates); English text as the key (no IDs, but every wording change breaks the translations). |
| D65 | How does an imported Postman collection get run like Postman's "Run collection" (R-COL-41)? | **Run… on a project or folder** opens a run of its requests that is not saved, with **Save as workflow…** to keep it (R-FLOW-7); **the import writes no workflow file** (2026-10-24, replacing the same day's "the import writes a workflow file"): a file nobody asked for in every imported project, while Run… works for every project, imported or not. | The import writes a workflow file (built first, then dropped); both. |
| D66 | What does "Keep variable values" keep (R-FLOW-9)? | **The project's runtime variables**: the run starts from them, carries its changes across iterations, and writes each step's changes back as it finishes; off by default, so R-FLOW-3's clean runs stay the default (2026-10-24). Values carried over sit below the data row, so a data file still decides its columns. | Write to the project's variables in `tortuga.toml` (Postman's collection variables), which would change files in git on every run; write back only at the end of the run (a cancelled run would lose them); on by default, as in Postman. |
| D67 | How does Tortuga get into VS Code (IDEA-97)? | **A thin TypeScript extension over `tortuga-cli`** in `editors/vscode`: the Testing view, code lenses, an environment picker, and "Open in Tortuga" starting the desktop app with a path. It never reads project files itself: `tortuga-cli list` gives the tree, `run --report json` the results (P7). Not in `make verify` (it needs Node.js, and its integration test VS Code and a display): `make vscode-test`. Not published to the Marketplace until it is verified on every OS (2026-10-24). | The full UI in a webview (IDEA-97 (b), changes D6); parsing the TOML files in TypeScript; a language server. |
| D68 | Where does the breakdown of a send's time show (R-HTTP-25)? | **A popover from the time in the response line** (2026-10-26): it is where the eye is when a response is slow, and it takes no room the rest of the time ("simpler first", §7). A **Connection** tab earns its place only once the response has more to show about its connection (server address, TLS certificate, redirect hops). | A Timing tab next to Body and Headers; a Connection tab with the timing and connection details at once. |
| D69 | How is the settings dialog laid out (R-SET-17)? | **Pages listed on the left, groups of rows on the right, switches and segmented buttons** (2026-10-26), as in VS Code's and macOS's settings: eight tabs in a row, a height that changed per tab and paragraphs of hints made the dialog feel crowded. Long explanations moved to hover, one short line stays. | Fewer tabs on top (five) with the same rows; keeping checkboxes and radio buttons. |

| D70 | How does a workspace file find projects in other repositories (R-WS-11)? Changes D13's file. | **By git remote and folder; where each clone is stays on this machine** (2026-10-27). A path such as `../users-api` assumed every teammate cloned the repositories side by side, and "Locate…" wrote one machine's layout into the shared file, so teammates kept undoing each other's fix in git. Projects in the file's own repository, and repositories without a remote (a developer on their own), keep relative paths. As in VS Code's `.code-workspace`, with folders found by remote. | Keep relative paths and only stop "Locate…" writing the file; only allow projects in the file's own repository; a per-machine map of every project, nothing shared. |
| D71 | How do the workspace's servers reach environments and projects (R-WS-12)? | **One list of server names for the workspace, in every environment, and copied into every project when the environments are saved** (2026-10-28). Before, a shared URL was kept per environment and only reached projects that already had a server of that name, so a server added in one environment was missing from the others and from every project. A first take only listed them in projects and added one when picked; that was found too hidden. Writing the names into each project's manifest keeps D62's rule that a project works on its own. | Workspace servers as virtual servers of every project (a project using one needs `--workspace` in CI); only listing them and adding one when picked. |
| D72 | How do AI agents reach Tortuga over MCP (IDEA-107)? | **Both a stdio command and a server the app starts, over one engine module** (2026-11-06). `tortuga-cli mcp` (stdio) is how agents usually start an MCP server and works without the app (CI, headless); Start/Stop in the app (HTTP on `localhost`, like the mock server, R-MOCK-1) gives one-click setup with **Copy agent config**, and serves the workspace that is open. | Only the app's server (agents work only while the app is open); only the CLI command, with the UI just copying or writing the agent's config. |
| D73 | What may an agent do through the MCP server (IDEA-107)? | **Browse, send a request, and edit requests, folders and examples** (2026-11-07). Edits go to disk through the same collection operations as the UI, so they show in the app and in git like a teammate's change, and secret values never reach the agent. Running workflows and folders and reading History wait for a need. | Read and run only (no file changes); read only (never sends); also running workflows and folders, and reading History. |
| D74 | Which MCP library (IDEA-107)? | **`rmcp`, the official Rust SDK** (2026-11-07): it follows the protocol's changes and has the stdio and HTTP transports. Its dependencies must pass `cargo deny`; it lives behind the native-only server code, so the engine still builds for wasm (R-P-3). | Hand-written JSON-RPC on `serde_json`; a spike before deciding. |
| D75 | How is the app's MCP server kept from other programs (IDEA-107)? | **Listen on `127.0.0.1` only, with a random bearer token made at each start** (2026-11-07), included in "Copy agent config". Stops other local programs and web pages (DNS rebinding) from sending requests with the user's secrets. | `127.0.0.1` without a token; a token plus asking the user to allow each new agent. |
| D76 | How are mistakes and committed credentials caught before they are pushed (IDEA-100, IDEA-101)? | **One check in `tortuga-core`, run by `tortuga-cli check` for CI and hooks, shown in the app and VS Code** (2026-11-10). In the app it **warns under the field with "Make it a secret", never blocks** (P1). Patterns are hand-written prefix and shape checks, no regex crate. A flagged value is **accepted by its place in `tortuga.toml`** (`[check] allow`), committed and reviewable; a stale entry is reported. **Examples are not scanned**: they are documentation and hold fake tokens on purpose. | Ask before saving a flagged value; CLI only; an allow-list per request file; no allow-list; scan examples too. |
| D77 | How does the summary of API changes between two revisions look (IDEA-98)? | **A Markdown table, a row per request with its first change, and the rest folded in `<details>`** (2026-11-11). Short in a pull request thread, and a reviewer opens what concerns them. A change is marked as one that may break clients by one rule: removing or changing something may, adding never does. The comparison is a pure function in `tortuga-core` over two loaded projects; the CLI reads the old revision through a read-only `Storage` over the git tree (`gix`), so core stays free of git and builds for WASM. The CLI comes first (R-CLI-7); a view in the app is IDEA-108. | A section per request with before and after in full (long beyond 2 or 3 changes); grouped by Added / Removed / Changed as OpenAPI diff tools do (a request's changes spread over groups). |
| D78 | How are features that not everyone needs added (add-ons)? Changes R-MCP-5. | **Add-ons: built in, off by default, turned on in Settings → Add-ons, and not loaded while off** (2026-11-12). Each is kept apart in the code: its UI in its own module under `tortuga-ui-egui/src/addons/`, its engine code in its own `tortuga-core` module, and the shell reaches it only through one list of add-ons (an `AddOn` enum: its menu items, commands, top-bar item, dialog and settings). Unlike Settings → Features (R-SET-13), which hides core parts that are on by default, an add-on off is not there at all (P1). One build for everyone; a Cargo feature per add-on only when one brings heavy dependencies. The switch is the app's: `tortuga-cli` commands run only when called. The MCP server is the first add-on (R-ADD-3); GraphQL (IDEA-25) and GeoServer (IDEA-109) would be next. | Plugins loaded at run time (dynamic libraries have no stable Rust ABI; a WASM plugin runtime is heavy and needs an API to keep stable); a Cargo feature per add-on now (separate builds to test and release); add-ons on by default (the UI grows with every one); folding them into Settings → Features. |

New questions go here as the next free `Dn` (see the end of this file) with options, until they are decided.

## 9. How to change this roadmap

- New idea: add a row to [§7](#7-ideas) with the next free `IDEA-n`, or under [Importers](#importers) for an import.
- Open question: add it to [§8](#8-decisions) as the next `Dn` with options; record the choice and date when decided.
- When an idea gets a phase, give it a requirement ID in [§6](#6-requirements) (next free number in its area) and
  remove the idea row; the requirement says *(IDEA-n)*.
- When a requirement is fully met and verified, move its row to REQUIREMENTS.md and record
  how it was verified in FEATURES.md.

Next free IDs (removed rows keep their numbers): `R-P-5` · `R-HTTP-28` · `R-CK-7` · `R-COL-50` · `R-WS-14` · `R-VAR-11` · `R-SRV-11` · `R-GIT-15` · `R-HIST-6` · `R-LOC-3` · `R-NF-5` · `IDEA-110` · `D79` · `R-TEST-20` · `R-FLOW-10` · `R-CLI-8` · `R-SET-19` · `R-MOCK-9` · `R-I18N-4` · `R-VSC-6` · `R-MCP-6` · `R-ADD-4`.
