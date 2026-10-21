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
| **S7 Design** an API before it is built. | Define each request with the responses it should give, serve them from a local mock so the client and its tests can be written now, then point the same tests at the real implementation (D43). |

## 4. Status

Milestones M1 to M9 are done: sending requests, projects on disk, environments and servers, auth and secrets,
cookies, live git collaboration, workspaces, the v1 extras, release builds, the Phase 2 "daily driver"
features, and Phase 3: scripts, captures, workflows and the `tortuga-cli` runner. Since then: request tabs,
imports from OpenAPI, Postman (with its scripts) and `.http` files, settings on three levels, XML, scripts in depth
(console, breakpoints, Beautify, snippets, tests from the response), and mock APIs with the contract check. See
[FEATURES.md](FEATURES.md) for what each one does and how it was verified.

Open before the first release (v1):

| Work | Requirement | What is left |
|---|---|---|
| Installers on every OS | R-NF-4 | Run the Release workflow; install and check the `.msi` and `.dmg` (and the `.deb` with `apt`). See [RELEASING.md](RELEASING.md). |
| macOS and Windows | R-P-3, R-HTTP-8 | Run the app on macOS at all, and use it beyond start-up on Windows; system proxy settings on both. |
| OS credential store | R-LOC-2 | Check Keychain, Credential Manager and Secret Service (only the file fallback is verified). |
| No telemetry | R-P-4 | Confirm with a network monitor. |
| Signing | D17 | Decide whether v1 installers are signed. |
| CLI archives | R-CLI-4 | Run the Release workflow and check the archives on each OS (only the Linux one was built locally). |

Next: **Phase 4 — Reach**, picked by demand.

## 5. Phases

The phases are an order, not a calendar. Each one ends with something a team can use every day. Phase 1 (v1: share
collections safely), Phase 2 (daily driver) and Phase 3 (workflows and tests, v2) are done.

### Phase 4 — Reach

Picked by demand, not in a fixed order. Done so far: imports from OpenAPI, Postman and `.http` files (R-COL-15 to
R-COL-17, D7), settings on three levels (R-SET-1 to R-SET-7, D39), and mock APIs with the contract check (R-MOCK-1 to
R-MOCK-8, D43 to D47). Still open:

- More from the importers, and new ones (see [Importers](#importers) below).
- GraphQL, WebSockets, Server-Sent Events, gRPC (IDEA-25).
- Client certificates / mTLS (IDEA-20).
- Code snippets for other languages (IDEA-22).
- Web version (IDEA-11): the code keeps compiling for wasm, but nothing is built for it until a team asks (D6).
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
(R-MOCK-6). **Built, being verified:** descriptions of the API, tags and operations (R-COL-22).

| ID | Idea | Notes |
|---|---|---|
| IDEA-45 | Import again: update a project from a changed OpenAPI description, keeping local changes. | An import always creates a new project today (D34). |
| IDEA-46 | Import Swagger 2.0 descriptions. | Today they are refused with a hint to convert them (D33). |

#### Postman — done

**Done:** collections v2.0 and v2.1 with folders, variables, servers, auth, bodies and scripts (R-COL-16, D36);
scripts translated to Tortuga's API (R-COL-19), with Chai-style assertions to translate to (R-TEST-8);
environment files into a new or an existing project (R-COL-20, D49).
**Built, being verified:** descriptions of the collection, folders and requests (R-COL-22); cookies, `pm.info`,
`replaceIn`, the data row and `setNextRequest` translated (R-COL-21),
with the script API they need (R-TEST-16, R-FLOW-6, D50).

| ID | Idea | Notes |
|---|---|---|
| IDEA-58 | Import a request's saved responses (`response[]`) as its examples (R-MOCK-1): name, status, headers and body, so an imported collection can be mocked and contract-checked (R-MOCK-8) like an OpenAPI import (R-MOCK-6). | Ignored today. Small: the examples already exist in the file format. |
| IDEA-59 | Import a request's settings (`protocolProfileBehavior`): `followRedirects` as its follow-redirects choice, `disableCookies` as its cookies switch (R-SET-1). | Ignored today. Small. `strictSSL: false` has no shared counterpart: hosts not to verify are an app setting only (R-SET-3). |
| IDEA-60 | Keep collection- and folder-level scripts: as project and folder scripts in Tortuga, or copied into each request below them. | Left out with a warning today (D36). They often get or refresh a token. Project and folder scripts would need a new decision. |
| IDEA-61 | Translate the rest of Postman's API that scripts use: `pm.sendRequest` (sending a request from a script, mostly to get a token) and `require('crypto-js')`/`CryptoJS` (hashes, HMAC and Base64 for signing requests). | Scripts using them are kept as comments today (R-COL-19). Each needs Tortuga's script API to offer it first: scripts can't send requests or hash. Cookies, `pm.info`, `replaceIn`, the data row and `setNextRequest` are R-COL-21. |
| IDEA-62 | Import Postman's data export (all collections, environments and globals in one archive) as a workspace: a project per collection, environments as in R-COL-20, globals as workspace variables (R-WS-8). | Builds on R-COL-20. For a team moving over all at once. |
| IDEA-63 | Import again: update a project from a changed Postman collection, keeping local changes. | As IDEA-45 for OpenAPI. For a team moving over gradually, with the collection still changed in Postman for a while. |
| IDEA-72 | Translate lodash (`_`, `require('lodash')`) in Postman scripts, or offer it in Tortuga's scripts. | **Not planned**: modern JavaScript covers what Postman scripts mostly use it for, and bundling it would slow down every script run. Scripts using it are kept as comments to port. |
| IDEA-71 | Fetch a collection from Postman's API with an API key instead of from an exported file. | **Not planned**: it contacts a third-party host and needs a Postman account (R-P-4, P5). Exporting a file does the same. |

#### `.http` files — done

**Done:** VS Code REST Client and JetBrains HTTP Client files (R-COL-17, D37).

| ID | Idea | Notes |
|---|---|---|
| IDEA-49 | Import `.http` environment files (`http-client.env.json` and its private twin, REST Client's `rest-client.environmentVariables`) into environments and the secret store, and several `.http` files at once as a folder each. | A `.http` import reads one file and makes one "Default" environment today (R-COL-17). |

#### Not done yet

| ID | Idea | Notes |
|---|---|---|
| IDEA-68 | Import an Insomnia export (v4 JSON, v5 YAML): workspaces, folders, requests, environments. | Insomnia is one of the clients the vision names. |
| IDEA-69 | Import a Bruno collection (a folder of `.bru` files with its environments). | Bruno is also file-based and git-friendly, so its users are close to Tortuga's. |
| IDEA-70 | Import a HAR file saved from a browser's dev tools: a request per entry, its response as the request's example. | Fits S1 (explore); `curl` paste takes one request at a time (R-HTTP-11). |

#### Engine features the importers need

Gaps the imports can only warn about today, because Tortuga has nothing to map them to. Each is a feature in its own
right; the imports would use it once it exists.

| ID | Idea | Notes |
|---|---|---|
| IDEA-66 | More auth types: Digest, AWS Signature v4, OAuth 1, NTLM, Hawk; and getting an OAuth 2 token in the app (client credentials, authorization code with PKCE). | Today auth is Basic, Bearer and API key (R-HTTP-5). The Postman import leaves the others out with a warning, and OAuth 2 becomes `Bearer {{access_token}}` (R-COL-16). |
| IDEA-67 | Send a file as the whole request body. | Postman's `file` bodies are left out today (R-COL-16). Multipart bodies can already upload files (R-HTTP-15). |

## 6. Requirements

Requirements still open: not built yet, or implemented but not yet verified everywhere ([FEATURES.md](FEATURES.md),
"In progress").

| ID | Requirement |
|---|---|
| R-P-3 | **Cross-platform.** Windows, Linux, macOS desktop in v1. Web later, so the engine must not depend on desktop-only APIs. |
| R-P-4 | **No telemetry.** Tortuga only contacts hosts the user sends requests to. |
| R-HTTP-8 | Settings: follow redirects (on/off), verify TLS certificates (on/off, with a visible warning when off), proxy from system settings. |
| R-LOC-2 | Secret values are stored in the OS credential store (Windows Credential Manager, macOS Keychain, Linux Secret Service). If none is available, they fall back to a file in the app-data directory readable only by the current user, and the app shows a warning. |
| R-NF-4 | Installable packages: `.msi` (Windows), `.dmg` (macOS), `.deb` and `.AppImage` (Linux). |
| R-CLI-4 | Each release has a `tortuga-cli` archive per OS next to the installers, for CI: the binary with the README and licences, `.zip` on Windows, `.tar.gz` elsewhere. *(IDEA-43)* |
| R-SRV-8 | **A colour per environment** (D48). In the environments dialog (R-SRV-7), an environment's view has a **Colour** row under its name: Default, green, teal, blue, purple, pink, red, orange, yellow and grey, each a dot with its name on hover. The colour is saved in the environment's file (`color = "red"`; nothing for Default), so the team shares it through git and production can be red for everyone. The **⏺ dot** in front of the environment's name in the environment picker, in its list, and in the dialog's list of environments has that colour, in a shade for the light or the dark theme; Default is today's green. The picker switches environments by name across the workspace (R-WS-6), so a name's colour is the selected project's for it, else the first project's that sets one. The Mock environment (R-MOCK-4) keeps the default. A colour a newer build added reads as Default, and files without one load as before. |
| R-VAR-8 | **Dynamic variables** (D52): `{{$guid}}`, `{{$uuid}}` and `{{$random.uuid}}` (a random version 4 UUID), `{{$timestamp}}` (seconds since 1970, UTC), `{{$isoTimestamp}}` (ISO 8601, UTC, with milliseconds), `{{$randomInt}}` (0 to 1000) and a range written as REST Client does (`{{$randomInt 1 10}}`) or the JetBrains client (`{{$random.integer(1, 10)}}`), from the first number up to but not including the second. Each place a dynamic variable appears gets a new value on every send; a variable of the same name wins. They work wherever `{{variables}}` do: URL, params, headers, body, auth, and the examples the mock answers with (R-MOCK-3), so a mock can answer with a new id each time. They count as defined (no highlight, R-VAR-5); other `{{$…}}` names stay undefined. The Preview (R-HTTP-14) shows them as written, since each send has new values. The Postman and `.http` imports keep them without a warning and still warn about the ones Tortuga doesn't know (e.g. `{{$randomFirstName}}`). *(IDEA-64)* |
| R-COL-22 | **Descriptions** of projects, folders and requests, in Markdown (D51). Saved in the files they describe, as a `description` under the name (`tortuga.toml`, `_folder.toml`, the request file), written as a multi-line string so diffs read well, and left out when empty; they move with a renamed, moved or duplicated folder or request. A project's request has a **Docs** tab (with a dot when it has a description); a project's and a folder's are on a **Description** tab of their Defaults dialog, opened directly by **Description…** in their menus. Each shows the description **rendered** (headings, emphasis, lists, quotes, tables, task lists, code, links that open when clicked; images are not loaded, so nothing is fetched, R-P-4), with **Edit**/**Done** to switch to the text and a Markdown hint; read-only projects show it without Edit. Hovering a project, folder or request in the sidebar shows the description's first line. The imports fill them: a Postman collection's, folders' and requests' descriptions (text or `{content}`, on the request or its item); an OpenAPI description's `info.description`, each tag's description for its folder, and an operation's `description`, after its summary when the summary was too long to be the request's name as it is. *(IDEA-65)* |
| R-COL-21 | **More of Postman's API is translated** on import (R-COL-19): `pm.cookies.get(name)` → `res.cookie(name)`, `.has(name)` and `.toObject()` (the cookies the response set); `pm.info.eventName`, `.requestName`, `.iteration` and `.iterationCount` → `info.…` (R-TEST-16); `pm.variables.replaceIn` → `vars.replaceIn`; `pm.iterationData.get`/`has` → `vars.get`/`has` (a data row's values are variables, R-FLOW-4); `postman.setNextRequest(name)` and `pm.execution.setNextRequest(name)` → `workflow.next(name)`, and with `null` → `workflow.stop()` (R-FLOW-6). The rest of R-COL-19 holds: a script runs only when nothing of Postman's API is left. *(IDEA-61, in part)* |
| R-TEST-16 | **More in the script API**: `res.cookies` (name → value of each cookie the response set with `Set-Cookie`, the last one for a name) and `res.cookie(name)` (`undefined` when not set); `vars.replaceIn(text)` fills in the `{{variables}}` it knows, also ones set earlier in the script, and leaves the others as written; `info` with `event` (`"prerequest"` or `"test"`, as Postman's), `requestName`, `iteration` (from 0) and `iterationCount` (0 and 1 outside a data-driven run). A script that declares its own `info` or `workflow` still runs. The script reference under the editor lists them. |
| R-FLOW-6 | **Scripts choose what a workflow runs next** (D50). `workflow.next("name")` in any of a step's scripts runs the step with that name after this one (its own name, else its request's; the first such step), also an earlier one, so a step can repeat until a job is done; `workflow.stop()` ends the iteration after this step without failing it. The last call in a step counts. A name the workflow doesn't have fails the step; a failed step ends the iteration as before (R-FLOW-3), and with "continue after failures" the jump still happens. An iteration stops after 1000 steps and fails ("a loop that never ends?"). The results (app and `tortuga-cli`) say when a script ended an iteration; JUnit lists the steps after the failed one as skipped, and the JSON report has `ended`. Outside a workflow the calls change nothing, and the Console says so. Snippets: "Workflow: repeat a step until done", "Workflow: stop the run". |

## 7. Ideas

Candidates that are not requirements yet. To add one, append a row with the next free `IDEA-n` (see [§9](#9-how-to-change-this-roadmap)).
Ideas for importing from other tools are listed per importer under [Importers](#importers) instead.

### Planned for later

Agreed as worth doing, not scheduled yet.

| ID | Idea | Notes |
|---|---|---|
| IDEA-8 | "Compare" option when a file changed on disk while it had unsaved edits. | Extends R-GIT-11. |
| IDEA-11 | Web version of the app. | The architecture already keeps this possible (R-P-3). |
| IDEA-14 | Signed installers: Apple Developer ID signing and notarization for the `.dmg`, Authenticode for the `.msi`. | Removes the "unidentified developer" and SmartScreen warnings. Needs paid certificates; see D17. |
| IDEA-15 | Automatic update check and in-app updates. | Would contact a server, so it must be opt-in (R-P-4). |
| IDEA-44 | A container image of `tortuga-cli` for CI. | The Release workflow attaches CLI archives (R-CLI-4); an image needs a registry to publish to. |

### Under consideration

Not decided. Needs a real user need before it moves up.

| ID | Idea | Notes |
|---|---|---|
| IDEA-20 | Client certificates (mTLS). | |
| IDEA-21 | In-app commit, pull, push, and branch switching. | Large scope: needs git credential handling on three OSes. |
| IDEA-22 | Code snippets for other languages (Python, JavaScript, …). | |
| IDEA-25 | Other protocols: GraphQL tooling, WebSockets, SSE, gRPC. | |
| IDEA-54 | Dynamic mock answers: templates that read the incoming request (`{{request.path.id}}`, `{{request.body.$.name}}`, a timestamp, a random id), or a response script in the script engine. | Static examples first (D46, R-MOCK-3). |
| IDEA-55 | Stateful mocks: a `POST` that later `GET`s return, reset on restart. | Turns the mock into a fake backend; only if static examples prove too little. |
| IDEA-57 | Fault injection in the mock: drop the connection, answer slowly, return 5xx for a share of calls. | `delay_ms` (R-MOCK-3) covers slow answers. |

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

New questions go here as `D24`, `D25`, … with options, until they are decided.

## 9. How to change this roadmap

- New idea: add a row to [§7](#7-ideas) with the next free `IDEA-n`, or under [Importers](#importers) for an import.
- Open question: add it to [§8](#8-decisions) as the next `Dn` with options; record the choice and date when decided.
- When an idea gets a phase, give it a requirement ID in [§6](#6-requirements) (next free number in its area) and
  remove the idea row; the requirement says *(IDEA-n)*.
- When a requirement is fully met and verified, move its row to FEATURES.md ("Agreed requirements") and record
  how it was verified there.

Next free IDs (removed rows keep their numbers): `R-P-5` · `R-HTTP-20` · `R-CK-6` · `R-COL-23` · `R-WS-9` · `R-VAR-9` · `R-SRV-9` · `R-GIT-15` · `R-HIST-4` · `R-LOC-3` · `R-NF-5` · `IDEA-73` · `D53` · `R-TEST-17` · `R-FLOW-7` · `R-CLI-5` · `R-SET-9` · `R-MOCK-9`.
