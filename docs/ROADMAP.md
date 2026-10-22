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

Picked by demand, not in a fixed order. Done so far: imports from OpenAPI, Postman, Insomnia, `.http` files and Bruno (R-COL-15
to R-COL-17, R-COL-31, R-COL-32, D7), settings on three levels (R-SET-1 to R-SET-7, D39), and mock APIs with the contract check (R-MOCK-1 to
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
(R-MOCK-6); descriptions of the API, tags and operations (R-COL-22).

| ID | Idea | Notes |
|---|---|---|
| IDEA-45 | Import again: update a project from a changed OpenAPI description, keeping local changes. | An import always creates a new project today (D34). |
| IDEA-46 | Import Swagger 2.0 descriptions. | Today they are refused with a hint to convert them (D33). |

#### Postman — done

**Done:** collections v2.0 and v2.1 with folders, variables, servers, auth, bodies and scripts (R-COL-16, D36);
scripts translated to Tortuga's API (R-COL-19), with Chai-style assertions to translate to (R-TEST-8);
environment files into a new or an existing project (R-COL-20, D49); descriptions of the collection, folders and
requests (R-COL-22); cookies, `pm.info`, `replaceIn`, the data row and `setNextRequest` translated (R-COL-21), with
the script API they need (R-TEST-16, R-FLOW-6, D50); saved responses as examples and `disableCookies` (R-COL-30).

| ID | Idea | Notes |
|---|---|---|
| IDEA-59 | Import a request's `followRedirects: false` as its own follow-redirects choice. | Needs a per-request setting first: following redirects is a project setting (R-SET-1), so the import warns today (R-COL-30). `strictSSL: false` has no shared counterpart: hosts not to verify are an app setting only (R-SET-3). |
| IDEA-60 | Keep collection- and folder-level scripts: as project and folder scripts in Tortuga, or copied into each request below them. | Left out with a warning today (D36). They often get or refresh a token. Project and folder scripts would need a new decision. |
| IDEA-61 | Translate the rest of Postman's API that scripts use: `pm.sendRequest` (sending a request from a script, mostly to get a token) and `require('crypto-js')`/`CryptoJS` (hashes, HMAC and Base64 for signing requests). | Scripts using them are kept as comments today (R-COL-19). Each needs Tortuga's script API to offer it first: scripts can't send requests or hash. Cookies, `pm.info`, `replaceIn`, the data row and `setNextRequest` are R-COL-21. |
| IDEA-62 | Import Postman's data export (all collections, environments and globals in one archive) as a workspace: a project per collection, environments as in R-COL-20, globals as workspace variables (R-WS-8). | Builds on R-COL-20. For a team moving over all at once. |
| IDEA-63 | Import again: update a project from a changed Postman collection, keeping local changes. | As IDEA-45 for OpenAPI. For a team moving over gradually, with the collection still changed in Postman for a while. |
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
| R-SET-9 | **Preview tab on or off** (Settings → Editing → "Open requests in a preview tab", on by default). Off: a single click opens a request in a tab that stays, and a preview tab already open (or restored from last time) is kept. Local setting. *(IDEA-86, D31)* |
| R-SET-10 | **Density, code font and code size** (Settings → Appearance). Density: comfortable (default, today's spacing) or compact (less room around controls). Code font: the bundled JetBrains Mono, or a `.ttf`/`.otf`/`.ttc` file chosen with "Choose…", with "Use the bundled one" to go back; a file that is not a font is refused when saving, and one that can't be read later (moved, deleted) falls back to the bundled font with a notice. Code size: 80–150 % of its usual size, relative to the text size (R-SET-5). Code means everything in the monospace font: bodies, scripts, URLs. Local settings. *(IDEA-87)* |
| R-SET-11 | **Save automatically** (Settings → Editing, off by default; D53). On: the shown request's and the open workflow's edits are saved one second after the last key press or click, and at once before another request or workflow opens and when the app closes. Not while the conflict banner waits for an answer (R-GIT-11), not for a project that is read-only, and never the scratch request. Revert and a pasted `curl` command (D19) then only stay unsaved for that second; git is the undo. *(IDEA-89)* |
| R-HTTP-20 | **Proxy setting** (Settings → Requests → Proxy; D54): from the system (default, as R-HTTP-8), none (straight to the server, whatever the system says), or manual: a proxy URL (`http://` or `https://`; `127.0.0.1:8080` means `http://127.0.0.1:8080`) and hosts that bypass it, one per line (`example.com` and `*.example.com` both cover its subdomains, `10.0.0.0/8` a network). A URL with a user name or password is refused, and Save stays off while the manual URL can't be used, saying why on every tab. Errors never repeat the URL. Local setting; `tortuga-cli` keeps using the system proxy and `HTTP_PROXY`-style variables. *(IDEA-88)* |
| R-HTTP-21 | **Move query params into the body.** When the method changes from GET to one that sends a body (POST, PUT, PATCH, DELETE) while the request has query parameters and no body, a line under the URL offers to move them into the body as **JSON** or **XML**, or **Keep in URL**. JSON is an object with a key per parameter: a value written as a JSON number or `true`/`false` keeps that type (`01` and `1e3` stay text), a name used more than once becomes an array. XML is a `<request>` element with an element per parameter in order, characters a tag name can't hold becoming `_`. Only switched-on parameters move; the URL and the Body tab show the result at once, and Revert undoes it like any edit. The offer goes away once a body exists, the parameters are gone or the method is GET again. |
| R-COL-23 | **One header line per request**: the breadcrumb (project and folders), the editable name, "Unsaved", and Save, Revert and Copy as curl on one line above the URL; the scratch request shows "Scratch request" and "Untitled request" there. The tab already names the request, so there is no separate heading. *(IDEA-73)* |
| R-SET-12 | **The response gets the room.** Stacked (the default), the request takes the height its tab needs, at most the height the divider below it was dragged to (300 points until then); the response gets the rest. Double-clicking the divider shows only the URL line of the request, and again brings the tabs back. Settings → Appearance → **Response**: below the request (default), beside it, or beside it when the window is wide (from 1100 points of main area). Side by side, the divider sets the widths, and double-clicking it makes them equal. Where the divider was left is kept on this machine. *(IDEA-74)* |
| R-SET-13 | **Turn off what you don't use** (Settings → **Features**, all on by default): Scripts, tests and captures (the Scripts tab); Examples and the mock server (the Examples tab, "Save as example", starting a mock); Workflows (the sidebar's workflows, their palette commands and "New workflow…"); Descriptions (the Docs tab and the "Description…" menu items). Off only hides: files stay as they are, a request that already has scripts, examples or a description still shows that tab, a written project or folder description still shows in its Defaults dialog, and a mock server that runs can still be stopped. Kept as ids of what is off, so features added later are on. *(IDEA-75)* |
| R-COL-24 | **Preview from the URL line** (D55): the line under the URL (the server's base URL, if any, and "Preview") has a ⏵ that opens the request as it will be sent (R-HTTP-14) right there, above the tabs, at most a third of the window tall and scrolling; ⏷ or Escape (when no field has the cursor) closes it. The Preview tab is gone. *(IDEA-76)* |
| R-COL-25 | **A palette for the project you're in**: the command palette lists the commands of the current project (the open workflow's, else the selected request's, else the first) as before; another project's commands only when a word of the typed text matches that project's name. *(IDEA-77)* |
| R-COL-26 | **Hide the sidebar** with Ctrl+B, the ☰ button at the left of the title bar, or the palette ("Hide the sidebar" / "Show the sidebar"); kept per workspace. The "Projects" heading is left out where the sidebar's Projects and History tabs already name the view. *(IDEA-78)* |
| R-COL-27 | **Shortcuts you can find**: F1 (or "Keyboard shortcuts" in the palette) shows every shortcut by where it works, written as the OS does ("Ctrl+S", "Cmd+S"), built from the table the app's shortcuts are defined in. New: Ctrl+L puts the cursor in the URL, Ctrl+E opens the palette on the environments, Ctrl+1 … Ctrl+8 go to the tab at that place and Ctrl+9 to the last one. *(IDEA-79)* |
| R-COL-28 | **Quiet notices**: messages that need no action show for four seconds in the bottom-right corner and fade; a click dismisses one, and a newer one replaces it. They are: the mock server started (with its port) or stopped, history cleared, a project imported, the workspace saved as a file, and what a pasted curl command left out. Problems keep the banner until it is closed. *(IDEA-80)* |
| R-VAR-9 | **Variables explain themselves.** Hovering a `{{name}}` in a request's fields (URL, params, headers, path variables, auth, body, form and multipart parts, examples' headers) shows its value and where it comes from: a capture or script (this session), the workspace's captures, this computer's secret value, the environment, a folder, the project, the workspace, or a dynamic variable (what it makes); secret values are shown as "secret, hidden" (P6), never their value. An undefined one says so. Ctrl+click opens where it is defined (the environments dialog, the folder's Variables, the workspace's variables, the runtime variables), or where an undefined one can be. Typing `{{` and part of a name suggests the names the request can use and the dynamic variables, those that start with the text first; ↑ ↓ choose, Enter or Tab put in `{{name}}`, Escape closes the list. *(IDEA-81)* |
| R-HIST-5 | **Compare responses.** After a send, a chip next to the status says how much changed since the request's previous response ("3 changes", "Same as before"); it opens **What changed**: the status, time and size, the headers (Date and Age left out), and the body: JSON by structure and value with a `JSONPath` per change (added, removed, changed), other text by lines; at most 500 changes are listed. In the History view, an entry's menu has **Compare with the send before** (the same request's previous send that got a response), **Select for comparing** and **Compare with the selected one**; the older one is always "before". The comparison after a send is worked out off the UI thread. *(IDEA-82)* |
| R-SRV-10 | **Send to every environment** (right-click on Send, the palette, Ctrl+Shift+Enter; D57): the request goes at once to each environment of its project with that environment's variables, secrets, servers, auth and the project's cookies, but without its scripts, captures or history, so nothing changes in the app. **Every environment** shows a row per environment as answers arrive: status, time and size, or why it failed (an undefined variable fails at once), and how the response's shape differs from the first environment's answer: keys added or missing and values of another type, not values; array items by their first. Closing it stops what is still on its way. A method other than GET, HEAD or OPTIONS asks first when one of the environments is protected (R-SRV-9). *(IDEA-83)* |
| R-HIST-4 | **A request's trend**: in the response line, small bars for the request's last 20 sends from history, oldest first, as tall as they took, failures (no response, 4xx, 5xx) in red; hovering says "Last 12 sends: 80 ms to 1.2 s, 2 failed" and when the bar under the pointer was sent, its status and time. Only with two sends or more, and only where history is kept. *(IDEA-84)* |
| R-SRV-9 | **Protected environments** (D56): **Protected** in an environment's view of the environments dialog, saved as `protected = true` in its file (only when on), so the team shares it. Sending anything but GET, HEAD or OPTIONS to a protected environment asks every time ("Send DELETE to “Production”?"); running a workflow against one asks once before it starts when a step may change data, naming those steps. The mock's environment is never protected. `tortuga-cli` refuses such a run with exit code 2, naming the steps, unless `--allow-protected` is given; a run that only reads goes as usual. *(IDEA-85)* |
| R-COL-29 | **Import `.http` environment files, and several `.http` files at once** (D58). **+ Add → Import .http file…** takes several files: they become one project named after their folder with a folder per file in the order picked (names made unique), one server per base URL variable; a variable the files give different values becomes a folder variable of each, and a base URL they disagree on keeps the first with a warning; auth all requests share becomes the project's, else auth all requests of a file share becomes that folder's. A file without requests is refused by name. **Environment files**: the JetBrains HTTP Client's `http-client.env.json` and `http-client.private.env.json` (every value of the private file secret), and REST Client's `rest-client.environmentVariables` in a VS Code `settings.json` or `.code-workspace` file (comments and trailing commas allowed). They come in three ways: picked with the `.http` files, found next to them (the two JetBrains names), and "Add environment files…" in the import dialog, which lists them; each name in them (not `$shared`) becomes an environment instead of "Default", with `$shared` values below its own and REST Client's `{{$shared name}}` filled in. On a project, **Import HTTP environments…** (menu and palette, not for read-only projects) does the same as R-COL-20 for Postman: what it adds and updates and the secret values found, before writing, updating an environment of the same name (D49). Values are mapped as R-COL-20 maps them (base URLs to servers, secrets to this machine's secret store per environment, D35). Settings objects (SSL, auth) and values read from the machine (`{{$processEnv …}}`, `{{$dotenv …}}`) are listed as not imported. Environment files given to an import entry on their own are refused with a hint. Follows the `.http` importer's switch (R-COL-18). *(IDEA-49)* |
| R-COL-30 | **Postman's saved responses and request settings** (D36). Each saved response of a request (`response[]`) becomes one of its examples (R-MOCK-1), in the same order: its name (else "Example"; made unique with " 2", " 3", … since the mock picks one by name), its status code (200 when it has none or one that is no HTTP status), its headers without those a response saved in Tortuga leaves out (credentials, cookies, length, transfer and content encoding, connection, date; the body is kept decoded) and its body; no match conditions, so the first answers unless one is picked by name (R-MOCK-3). A request's `protocolProfileBehavior` `disableCookies: true` turns its cookies switch off (R-CK-2); `followRedirects: false` and `strictSSL: false` are listed as not imported, saying where Tortuga sets them. *(IDEA-58, part of IDEA-59)* |
| R-COL-31 | **Import a Bruno collection** (D59). **+ Add → Import Bruno collection…** (and the palette) picks the collection's folder, the one with `bruno.json` (also found when `bruno.json` is picked in another import's file dialog); it becomes a new project (D34) beside the folder, never in it, named as in `bruno.json`. Folders and requests in the order of their `seq` (those without after, by name), skipping hidden folders and those `bruno.json` ignores. A request's method, URL, query parameters (also turned-off ones), path variables, headers, docs, timeout and the body of its chosen mode (JSON, text, XML, SPARQL, form, multipart with `@file(…)` and `@contentType(…)`, GraphQL as JSON). The headers, variables, docs and auth of `collection.bru` and `folder.bru` become the project's and folders'. Auth is mapped as the Postman import maps it (R-COL-16): Basic, Bearer, API key (header or query), OAuth 2 as `Bearer {{access_token}}`, others left out with a warning; credentials become secret variables (D35). A variable that starts request URLs becomes a server (D36). Each file in `environments/` becomes an environment as in R-COL-20, its `vars:secret` names secret variables without a value (Bruno keeps those on its user's computer, so the environments dialog asks for them). `{{process.env.NAME}}` becomes the secret variable `NAME`, with its value from the collection's `.env` file for this machine's secret store. A request's `assert` lines become tests (`test` with `expect`, every Bruno operator), its `vars:post-response` values read from the response (status, a header, a path into the body) captures (R-TEST-4), and its own variables `vars.set(…)` in its pre-request script. Scripts are kept as comments to port (D28). Listed as not imported: collection and folder scripts, WebSocket and gRPC requests, file bodies, `followRedirects: false`, client certificates and the proxy of `bruno.json`, assertions and response values that can't be translated, files that can't be read. **Import Bruno environments…** on a project (menu and palette) does the same as R-COL-20 with `.bru` environment files. Follows its own importer switch (R-COL-18). *(IDEA-69)* |
| R-COL-32 | **Import an Insomnia export** (D60). **+ Add → Import Insomnia export…** (and the palette) takes an export of a collection or design document: v4 JSON (Insomnia ≤ 10) or v5 YAML (Insomnia 11+), also recognised when picked with another import. Folders and requests in Insomnia's order; a request's method, URL, query parameters (also turned-off ones), path parameters, headers, description and body by its MIME type (JSON, XML, form, multipart with files, GraphQL as JSON, any other as text with its `Content-Type`); folder headers, auth, environment and description as folder defaults. Auth as the Postman import maps it (R-COL-16): Basic, Bearer (a prefix other than “Bearer” is listed), API key (header or query), OAuth 2 as `Bearer {{access_token}}`, empty as inherit, turned off as no auth, others left out with a warning; credentials become secret variables (D35). The base environment becomes project variables, each sub-environment an environment as in R-COL-20, a private one's values secret; nested values become `parent.child` variables. A variable that starts request URLs becomes a server (D36). `{{ _.name }}` becomes `{{name}}`, `{% uuid %}` and `{% now %}` dynamic variables (R-VAR-8); other template tags are kept and listed, `{% response %}` with a hint to use a capture. Scripts are translated as Postman's are (R-COL-19, `insomnia` is `pm`), else kept as comments. A request that neither sends nor stores cookies gets its cookies switch off. Listed as not imported: other collections in the export (named), cookies, the API description, unit test suites, folder scripts, gRPC and WebSocket requests, file bodies, `followRedirects` off. An export of environments only, or of another kind, is refused with a hint. Follows its own importer switch (R-COL-18). *(IDEA-68)* |
| R-COL-33 | **Bruno scripts translated** (D59, D61). The Bruno import (R-COL-31) translates a request's pre-request, post-response and test scripts to Tortuga's script API, as R-COL-19 does for Postman: Bruno's variables (`bru.setVar`, `getVar`, `hasVar`, `deleteVar`, their `EnvVar` forms, `getProcessEnv`, `getCollectionVar`, `getFolderVar`, `getRequestVar`, `getGlobalEnvVar`) as `vars`, `bru.setGlobalEnvVar` as a workspace variable (R-WS-8), `bru.interpolate` as `vars.replaceIn`, the response's and request's getters (`res.getStatus()`, `getStatusText()`, `getHeader(…)`, `getHeaders()`, `getBody()`, `getResponseTime()`, `req.getUrl()`, `getMethod()`, `getHeader(…)`, `getHeaders()`, `getBody()`, `getName()`), `res.body` and `res.responseTime`, `bru.setNextRequest`, `bru.runner.setNextRequest` and `bru.runner.stopExecution()` as `workflow.next`/`stop` (R-FLOW-6), and the request's name as `info.requestName`; `test` and `expect` stay as they are. In Bruno `res.body` is the parsed JSON, so it becomes `res.json()`. A script still using `bru`, `require`, `res(…)` or a getter Tortuga lacks is kept as comments under the note, translated as far as it went. The import's warnings count translated and kept scripts. *(IDEA-91)* |
| R-COL-34 | **Bruno's YAML collections** (D61). **Import Bruno collection…** also takes a collection in Bruno's YAML format (`OpenCollection`, Bruno 3 and later): a folder with `opencollection.yml` (also found when that file is picked in another import's file dialog), `folder.yml` per folder, a `.yml` file per request and `environments/*.yml`. Everything R-COL-31 and R-COL-33 import from `.bru` files is imported the same way from YAML: the collection's and folders' defaults (headers, auth, variables, scripts, docs), a request's `http` or `graphql` section (method, URL, query and path params, headers, body, auth), its `runtime` (variables, `set-variable` actions as captures, assertions as tests, scripts), settings and docs, and environments with their secret variables. A request's saved examples become its examples (R-MOCK-1), named as in Bruno (made unique), with their status, headers (filtered as R-MOCK-1 does) and body. **Import Bruno environments…** takes `.yml` files too. *(IDEA-92)* |
| R-COL-35 | **Draft a request's docs.** A request's Docs tab with no description has **Draft from request** next to Edit. It fills the description with Markdown built from the request and opens the editor: the request's name as heading, a line to replace with what it is for, the method and URL (and server), tables (Name, Example, Description) of its path variables, query parameters and switched-on headers, a switched-off parameter marked “Optional.”, the auth by kind, the body with its content type (JSON and XML pretty-printed, form and multipart fields as a table), and a section per example (status, reason, name, body) or an empty status table. Credentials never go in: auth is named by its kind only, and `Authorization` and `Cookie` header values show as “…”. Values are kept as inline code so Markdown in them stays text. The draft is an unsaved edit like any other. *(R-COL-22)* |
| R-SET-14 | **About** tab in Settings: the version, the OS and architecture it was built for, the project and workspace file formats it reads (newer project files open read-only), the licence and a link to the source; **Copy version information** copies one line for a bug report ("Tortuga 0.5.0 (linux x86_64, project format 2, workspace format 2)"); and that the app contacts nothing on its own (R-P-4). |

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
| IDEA-90 | **Find shared workspaces.** When "Add existing projects" (R-WS-3) finds a `*.tortuga-workspace.toml` in the chosen folder or repository, offer to open that workspace (R-WS-5) instead of adding its projects one by one; a project opened on its own whose repository has a workspace file that lists it gets a quiet hint with "Open workspace". | Today each teammate opens a committed workspace file once by hand ("Open workspace file…"), so a workspace in the repo is easy to miss. |

### UI and settings

From a review of the UI on 2026-10-21, now that most features are in. Not decided. Two rules guided the list:
**simpler first**: fewer rows, tabs and repeated labels before anything new; and **a setting earns its place only
when developers really disagree on it** and the wrong default costs them every day. Where a better default would do,
the idea is the default, not a setting (P1). The settings ideas IDEA-86 to IDEA-89 became R-SET-9, R-SET-10,
R-HTTP-20 and R-SET-11; the "simpler" ideas IDEA-73 to IDEA-80 became R-COL-23 to R-COL-28, R-SET-12 and
R-SET-13; and IDEA-81 to IDEA-85 became R-VAR-9, R-HIST-5, R-SRV-10, R-HIST-4 and R-SRV-9.

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
| D53 | How does "Save automatically" work (R-SET-11)? | **One second after the last key press or click, and before another request or workflow opens or the app closes** (2026-10-21). Never over a file that changed on disk while the conflict banner waits (R-GIT-11), never the scratch request; off by default, since an explicit Save keeps half-finished edits out of `git diff`. | Only when leaving the request (fewer writes and file renames while typing a name, but `git diff` lags); on send. |
| D54 | Can a manual proxy have credentials (R-HTTP-20)? | **No, not yet** (2026-10-21): the URL is refused with a user name or password in it, so no password lands in the plain settings file. Covers mitmproxy, Charles, Fiddler and most company proxies. | A user name with the password in the OS credential store (R-LOC-2). |
| D55 | How does the preview open from the URL line (R-COL-24)? | **In place**: under the line, above the tabs, at most a third of the window tall with its own scrolling; open per request until closed, Escape closes it (2026-10-21). | A popup over the editor, closed by clicking elsewhere; keep the Preview tab and let the line switch to it. |
| D56 | How do protected environments ask (R-SRV-9)? | **Before every send that may change data, and once before a workflow run with such steps; `tortuga-cli` refuses those runs unless `--allow-protected`** (2026-10-21). A CI job that should touch production says so in its command. | Ask once per session per environment; ignore protection in the CLI. |
| D57 | What does "Send to every environment" send (R-SRV-10)? | **Plain sends**: each environment's values, secrets, servers, auth and cookies, but no scripts, captures or history, so comparing never changes state; shapes are compared, not values (2026-10-21). | Full sends with scripts, captures and history, closer to reality but leaking runtime values from one environment into the next. |
| D60 | How is an Insomnia export imported (R-COL-32)? | **v4 and v5 read into one tree and mapped once; one collection per import** (the first, the others named; IDEA-93), **the base environment as project variables and sub-environments as environments** (private ones secret, D35); **scripts through the Postman translation** with `insomnia` read as `pm`, since Insomnia's script API copies Postman's (2026-10-21). | A project per collection in the export; the base environment as a "Base" environment; scripts always as comments. |
| D61 | How are Bruno scripts and YAML collections imported (R-COL-33, R-COL-34)? | **Scripts are translated with the scanner the Postman translation uses (now shared), `res.body` as `res.json()`** since Bruno parses it; **YAML files are read into the blocks of `.bru` files** and mapped once, so both formats always import alike, with saved examples as the only addition (2026-10-21). | Keep Bruno scripts as comments; a separate mapping for YAML; translate `res.body` to the text. |
| D59 | How is a Bruno collection imported (R-COL-31)? | **From its folder, `.bru` files only, beside it as a new project** (D34); the declarative parts (`assert`, `vars:post-response`, request variables) are translated, scripts kept as comments as the first Postman import did (D36), and translating Bruno's script API is IDEA-91 (done since R-COL-33, D61); `{{process.env.X}}` becomes a secret variable with its `.env` value (D35) (2026-10-21). | Pick `bruno.json` only; import into the Bruno folder itself; translate scripts in the first version; keep `process.env` as plain variables. |
| D58 | How are several `.http` files and their environment files imported (R-COL-29)? | **Several files are one new project with a folder per file** (D34 still holds: never into an existing project), named after their folder; **environment files are found next to the `.http` files** as the JetBrains client finds them, besides being picked, and go in like Postman's (D49): into the new project, or into an existing one from its menu; `$shared` values are copied into each environment (2026-10-21). | A project per file; `$shared` as project variables; only environment files picked by hand. |

New questions go here as `D24`, `D25`, … with options, until they are decided.

## 9. How to change this roadmap

- New idea: add a row to [§7](#7-ideas) with the next free `IDEA-n`, or under [Importers](#importers) for an import.
- Open question: add it to [§8](#8-decisions) as the next `Dn` with options; record the choice and date when decided.
- When an idea gets a phase, give it a requirement ID in [§6](#6-requirements) (next free number in its area) and
  remove the idea row; the requirement says *(IDEA-n)*.
- When a requirement is fully met and verified, move its row to FEATURES.md ("Agreed requirements") and record
  how it was verified there.

Next free IDs (removed rows keep their numbers): `R-P-5` · `R-HTTP-22` · `R-CK-6` · `R-COL-36` · `R-WS-9` · `R-VAR-10` · `R-SRV-11` · `R-GIT-15` · `R-HIST-6` · `R-LOC-3` · `R-NF-5` · `IDEA-94` · `D62` · `R-TEST-17` · `R-FLOW-7` · `R-CLI-5` · `R-SET-15` · `R-MOCK-9`.
