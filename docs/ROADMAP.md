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
features, and Phase 3: test and pre-request scripts, captures, workflows (also data-driven) and the `tortuga-cli`
runner. Request tabs grouped by project (R-COL-13, R-COL-14), a cookie viewer (R-CK-5), unsaved workflow edits kept across restarts (R-FLOW-5) and imports from OpenAPI, Postman and `.http` files (R-COL-15, R-COL-16, R-COL-17), settings on three levels (R-SET-1 to R-SET-7) and mock APIs from examples (R-MOCK-1 to R-MOCK-7) came after Phase 3. See [FEATURES.md](FEATURES.md) for what each one does and how it was verified.

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

Picked by demand, not in a fixed order:

- ~~Import OpenAPI~~ (done: R-COL-15), ~~then Postman collections~~ (done: R-COL-16) ~~and `.http` files~~ (done: R-COL-17), so teams can adopt Tortuga gradually (D7).
- GraphQL, WebSockets, Server-Sent Events, gRPC (IDEA-25).
- Client certificates / mTLS (IDEA-20).
- Code snippets for other languages (IDEA-22).
- Web version (IDEA-11): the code keeps compiling for wasm, but nothing is built for it until a team asks (D6).
- ~~Settings on three levels~~ (done: R-SET-1 to R-SET-7, D39).
- In-app commit/pull/push (IDEA-21) — only if working outside the app proves to be a real pain point.
- ~~Mock APIs from examples~~ (done: R-MOCK-1 to R-MOCK-7, D43 to D46): responses saved with each request, served by a
  local mock in the app and by `tortuga-cli`, so an API can be designed and tested before it exists (S7).

### Not on the roadmap

Accounts, cloud sync, real-time co-editing (IDEA-40); documentation generation, monitors (IDEA-41). They conflict
with P4/P5 or are outside the purpose of an API client. Mock servers were here until D43. See [§7](#7-ideas).

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
| R-HTTP-19 | **XML requests and responses**, on a par with JSON. An **XML body type** that sends `application/xml` unless the user sets a Content-Type, is checked as you type (what is wrong, line and column) and has a Beautify button; saved as `kind = "xml"`. Responses whose Content-Type says XML (`application/xml`, `text/xml`, `+xml` types such as SOAP), or that have none and parse as XML, are **pretty-printed** (two-space indents, text-only elements on one line, comments and CDATA as written), with "Raw" showing them as they came (R-HTTP-18). The response **filter takes an XPath subset** above XML bodies (D40): `/`, `//`, names (a name without a prefix matches any prefix), `*`, `@name`, `@*`, `text()`, `node()`, `.`, `..`, and predicates `[n]`, `[last()]`, `[last()-n]`, `[path]`, `[path <op> literal]` with `= != < <= > >=`; elements are shown pretty-printed, attributes and text as their values; it runs off the UI thread on the whole body. **Captures** take `xpath:<expression>`: the text of the first match (R-TEST-4). The curl, Postman and `.http` imports make XML bodies from an XML Content-Type or Postman's "xml" language. |
| R-COL-19 | **Postman scripts are translated** when a collection is imported (R-COL-16): `pm.test`, `pm.expect(…)` with Chai's common assertions (`equal`, `eql`, `deep.equal`, `true`/`false`/`null`/`undefined`/`ok`/`exist`, `include`, `property`, `lengthOf`, `match`, `above`/`below`/`least`/`most`, `a("string")` and the like, with `not`), `pm.response.to.have.status(…)`/`.header(…)`/`.be.ok`, the response's code, status, time, size, headers, `json()` and `text()`, `pm.request.method`/`url`, and getting and setting variables of every scope (`pm.globals` as workspace variables, R-WS-8; the legacy `postman.*EnvironmentVariable`). A translated script runs, under a note to check it, only when nothing of Postman's API is left and it parses; otherwise it is kept as comments, translated as far as it went (D36). The import warns how many scripts were translated and how many kept as comments. *(IDEA-48)* |
| R-SRV-7 | **Environments are edited one at a time.** The environments dialog lists "Defaults" and the project's environments on the left (drag ☰ to reorder, R-SRV-6); the right side shows only the chosen one: an environment's name, each server's base URL and each variable's value (an empty value uses the default, shown as a hint), or under Defaults the server and variable names, the 🔒 secret flag and default values. Servers and variables can be added and removed from either; removing one removes it from every environment. The dialog opens on the active environment and can be resized by dragging its bottom-right corner; it keeps that size for the session. Renaming the environment the workspace has chosen keeps it chosen (R-WS-6). Replaces the one-table layout of R-SRV-2 and R-VAR-7 (D41). |
| R-SET-8 | **"No auth" as a default.** A project's and a folder's Defaults… → Auth offer Inherit, No auth, Basic and Bearer token. "No auth" is saved (`[auth] kind = "none"`) and stops the auth of the levels above: a project with it sends none even when the workspace sets some, a folder with it sends none whatever the project sets. A request can still choose its own. The workspace, with nothing above it, offers No auth, Basic and Bearer token. Project files without `[auth]` inherit, as before. |
| R-TEST-6 | **Failed tests show in red.** After a response, the response panel's **Tests** tab and its count (e.g. `1/2`) are drawn in the failure colour when a test failed or a script or capture went wrong; otherwise it looks like the other tabs. |
| R-TEST-7 | **Beautify scripts.** A "Beautify" button beside each script (a request's pre-request and test scripts, a workflow step's tests) puts one statement per line, indents by nesting and adds the usual spaces (after commas, around comparison, logical and assignment operators, before `{`, after `if`/`for`/`while`/`catch`). Strings, template literals, regular expressions and comments are copied unchanged, so commented-out Postman code (R-COL-19) survives; the user's line breaks are kept, with at most one blank line. A beautified script still parses, and beautifying twice changes nothing. Implemented in `tortuga-core` without a new dependency. |
| R-TEST-8 | **Chai-style assertions in scripts**, next to the Jest style: `expect(x).to.equal(1)`, as in Postman's `pm.expect`. Chains (`to`, `be`, `been`, `is`, `that`, `which`, `and`, `has`, `have`, `with`, `at`, `of`, `same`, …), flags (`not`, `deep`, `nested`, `own`, `any`, `all`), property checks (`ok`, `true`, `false`, `null`, `undefined`, `exist`, `empty`, `NaN`) and methods (`equal`/`eq`, `eql`, `a`/`an`, `include`/`contain`, `property` (after which the chain is about the value), `ownProperty`, `lengthOf`/`length`, `match`, `above`/`gt`, `least`/`gte`, `below`/`lt`, `most`/`lte`, `within`, `oneOf`, `keys`, `instanceOf`, `closeTo`, `satisfy`, `string`). Failures read like the Jest ones: "Expected 7 to equal 8". |
| R-TEST-9 | **Script console.** After a send, the response panel has a **Console** tab when the request has scripts: in order, per script (pre-request, tests), every `console.log`/`info`/`warn`/`error`/`debug` with its line and level colour, each `test(...)` with ✓/✗, its line (for a failure, the line that failed) and reason, and the error that stopped a script with its line. Objects and arrays fold open; values JSON can't hold read as JavaScript (`undefined`, `function f()`, `NaN`). At most 1000 entries per script, then a note. The tab is red when a script failed or logged an error. Workflow steps show their console under their tests. Script errors elsewhere say their line, e.g. "TypeError: … (line 7)". The CLI prints the log lines as before. |
| R-TEST-10 | **Snapshot breakpoints** (D42). Clicking a line number in a request's pre-request or test script sets or clears a breakpoint (a red dot). When the script reaches that line, before running it, the Console records the script's variables in scope there with their values, then the script runs on; a breakpoint passed in a loop is recorded each time (up to 200 per script). A line where a breakpoint would change what the script does (it continues the line before, sits inside a string or comment, or is not a place for a statement) is not set, with a note in the Console saying why. Breakpoints are kept per request while the app runs, follow renames, and are never saved in project files. Workflow step scripts get line numbers but no breakpoints. |
| R-TEST-11 | **Scripts tab with sub-tabs, results on the lines.** The Scripts tab has sub-tabs **Pre-request**, **Tests** and **Captures**; it opens on Tests unless only another has content. Badges: a dot for a script with code, the last run's tests passed/total (or ✗ for a script error), breakpoint counts, the number of captures; a sub-tab is red when its part of the last run failed. A script fills the panel; what it can use is folded under "Reference". After a run, each line that ran a `test(...)` shows ✓, or ✗ with the reason, after its code; the line that threw shows the error; a breakpoint line shows its hits. Marks show only while the script is the one that ran. |
| R-TEST-12 | **Re-run tests on the last response.** "Re-run tests" on the Tests sub-tab runs the captures and the test script again on the request's last response, without sending: edit, re-run, edit. The pre-request script's console from that send stays; tests, marks, Console and variables update as after a send. Off while sending or before a response. |
| R-TEST-13 | **Tests and captures from the response.** Right-clicking a line of the formatted JSON body (not Raw, not a filter result, not a body cut short) shows the value's path and offers **Add test** (equals the value as it is now; is a string/number/boolean/array/object, or null; is there), **Capture into “name”** (the key, or a name made from it; `_2`, … when taken) and **Copy path**. A test goes after the test script with a blank line, a capture into the Captures table, and the Scripts tab opens on it. Written tests pass on the response they came from; captures use `$.path` with `['key']` for other names. *(IDEA-51)* |
| R-TEST-14 | **Script editor comfort.** In a script field: Tab indents (two spaces, or every selected line), Shift+Tab outdents, Enter keeps the line's indent (one more after `{`, `[`, `(`, with the closing bracket on its own line between a pair), Ctrl+/ (Cmd+/ on macOS) comments the selected lines out or back in, at one column, leaving blank lines alone. Keywords, strings, numbers, comments and regular expressions are coloured, from the scanner Beautify uses (no new dependency); scripts over 100 kB are shown plain. *(IDEA-52)* |
| R-TEST-15 | **Snippets.** A "Snippets" menu beside Beautify puts a ready piece of script on lines of its own after the cursor's line (or at the end): for pre-request scripts, set a variable, set a header, a timestamp or random number in a variable, log the request; for tests, status is 200, status is 2xx, response time under 500 ms, body is JSON, body has a property, save a field to a variable, log the body. Every snippet parses, and the test snippets pass on a typical JSON response. *(IDEA-53)* |

## 7. Ideas

Candidates that are not requirements yet. To add one, append a row with the next free `IDEA-n` (see [§9](#9-how-to-change-this-roadmap)).

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
| IDEA-45 | Import again: update a project from a changed OpenAPI description, keeping local changes. | An import always creates a new project today (D34). |
| IDEA-46 | Import Swagger 2.0 descriptions. | Today they are refused with a hint to convert them (D33). |
| IDEA-47 | Import Postman environment files into a project's environments (base URLs into servers, secrets into the secret store). | A Postman import makes one "Default" environment today (R-COL-16). |
| IDEA-54 | Dynamic mock answers: templates that read the incoming request (`{{request.path.id}}`, `{{request.body.$.name}}`, a timestamp, a random id), or a response script in the script engine. | Static examples first (D46, R-MOCK-3). |
| IDEA-55 | Stateful mocks: a `POST` that later `GET`s return, reset on restart. | Turns the mock into a fake backend; only if static examples prove too little. |
| IDEA-56 | **Contract check:** after a real send, compare the response with the request's examples (status, Content-Type, JSON keys and value types) and flag where the implementation differs from the design. Also in `tortuga-cli run --check-examples`. | Closes the loop of S7: design → mock → real API. |
| IDEA-57 | Fault injection in the mock: drop the connection, answer slowly, return 5xx for a share of calls. | `delay_ms` (R-MOCK-3) covers slow answers. |
| IDEA-49 | Import `.http` environment files (`http-client.env.json` and its private twin, REST Client's `rest-client.environmentVariables`) into environments and the secret store, and several `.http` files at once as a folder each. | A `.http` import reads one file and makes one "Default" environment today (R-COL-17). |

### Not planned

| ID | Idea | Reason |
|---|---|---|
| IDEA-40 | Accounts, cloud sync, real-time co-editing. | Conflicts with local-first and git-based collaboration (R-P-1, R-P-2). |
| IDEA-41 | API documentation generation, monitors. | Outside the purpose of an API client. Mock servers were part of this idea until D43 made them R-MOCK-1 to R-MOCK-6. |

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

New questions go here as `D24`, `D25`, … with options, until they are decided.

## 9. How to change this roadmap

- New idea: add a row to [§7](#7-ideas) with the next free `IDEA-n`.
- Open question: add it to [§8](#8-decisions) as the next `Dn` with options; record the choice and date when decided.
- When an idea gets a phase, give it a requirement ID in [§6](#6-requirements) (next free number in its area) and
  remove the idea row; the requirement says *(IDEA-n)*.
- When a requirement is fully met and verified, move its row to FEATURES.md ("Agreed requirements") and record
  how it was verified there.

Next free IDs (removed rows keep their numbers): `R-P-5` · `R-HTTP-20` · `R-CK-6` · `R-COL-20` · `R-WS-9` · `R-VAR-8` · `R-SRV-8` · `R-GIT-15` · `R-HIST-4` · `R-LOC-3` · `R-NF-5` · `IDEA-58` · `D47` · `R-TEST-16` · `R-FLOW-6` · `R-CLI-5` · `R-SET-9` · `R-MOCK-8`.
