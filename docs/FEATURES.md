# Tortuga features

> Features that are implemented **and verified**, by requirement ID, and (at the end, under
> [Agreed requirements](#agreed-requirements)) the requirements they were built to. Open work and new
> requirements are in [ROADMAP.md](ROADMAP.md).
>
> Add an entry only when the requirement is fully met, has automated tests where practical, and has
> been checked in the running app on at least one OS. Note which OSes were checked. Then move the
> requirement's row from the roadmap to "Agreed requirements".

**Verified on:** Linux = Ubuntu 24.04 under WSL2/WSLg (desktop build). Windows: start-up only (M7). macOS has not been run yet.

**UI rewrite (2026-10-16):** the UI moved from Dioxus to egui (`tortuga-ui-egui`). The engine and file formats did
not change. Re-checked in the running egui app (Linux, X11 via WSLg): sending, URL/params sync, pretty JSON,
status colours, headers tab, "Save to file…" (byte-identical), cancel, settings + TLS banner (saved to disk),
collection tree, selection, new folder (starts in rename mode), inline rename, duplicate, move up, move to,
delete with confirmation, unsaved marker + Ctrl+S + Ctrl+Enter, broken-file error, environments dialog (files on
disk), environment picker, undefined-variable highlight and send block, 5 MB body truncation, collections reopened
after a restart. Not yet re-checked in the egui app: the body editor types and sending through a server base URL.
(Creating and adding projects was checked with M6.5.)

## Implemented

### M1: Send a request

| Requirement | Feature | Verified on | How it was verified |
|---|---|---|---|
| R-HTTP-1 | All seven methods (GET, POST, PUT, PATCH, DELETE, HEAD, OPTIONS), chosen from a dropdown. | Linux | Automated test sends each method to a mock server. GET and POST sent from the app. |
| R-HTTP-2 | URL bar and Params table stay in sync. Params can be disabled without deleting them; special characters are encoded in the URL and shown decoded in the table. | Linux | Unit tests for splitting/joining and encoding. In the app: typing a URL filled the table; disabling a param removed it from the URL and from the sent request. |
| R-HTTP-3 | Headers table with enable toggles; duplicate names are sent as separate headers. | Linux | Automated test (duplicates, disabled, blank names). In the app: added header arrived at the server. |
| R-HTTP-4 | Body types None, Text, JSON, Form URL-encoded, with automatic Content-Type unless the user sets one. JSON is validated as you type and has a Format button. Switching body type keeps what was typed. | Linux | Automated tests for each body type and the Content-Type override. In the app: JSON validation message, switching types, and a JSON POST received by the server. |
| R-HTTP-6 | Response status (coloured), time, size, headers tab, pretty-printed JSON (key order kept), plain text, and binary detection with "Save to file…". | Linux | Unit and render tests. In the app: JSON, 302 headers, binary response; saved file was byte-identical to the server's response. |
| R-HTTP-7 | Cancel button while a request is in flight. Per-request timeout (Options tab) with a global default (Settings). | Linux | Automated tests for both timeouts and for cancelling. In the app: cancelled a 10 s request; a 500 ms timeout reported "Request timed out after 500 ms". |
| R-HTTP-8 | Settings dialog: follow redirects, verify TLS certificates (with a warning banner while off), default timeout. Settings are saved and survive a restart. Proxy is taken from the system (`HTTP_PROXY`/`HTTPS_PROXY`/`NO_PROXY` on Linux); loopback addresses always bypass the proxy. | Linux | Automated redirect test. In the app: redirect on/off; self-signed HTTPS server accepted with verification off and rejected (with an explanation) with it on; request sent through `HTTP_PROXY`; settings reloaded after restart. |
| R-COL-7 | Requests are sent ad hoc, without saving. | Linux | The whole M1 workspace is ad hoc. |

### M2: Collections on disk

| Requirement | Feature | Verified on | How it was verified |
|---|---|---|---|
| R-COL-1 | Collections are trees of folders and requests, shown in the sidebar with expand/collapse. | Linux | Automated tests on a temp directory. In the app: folders and requests created, nested, shown. |
| R-COL-2 | Create, rename (inline), duplicate, move ("Move to…"), and delete (with confirmation) requests and folders, from each item's … menu (or right-click). A new folder starts in rename mode. | Linux | Automated tests for every operation. In the app: each one done and checked on disk. |
| R-COL-3 | "Move up"/"Move down" in the item menu; the order is saved. | Linux | Automated tests (including hand-made files without a position). In the app: moving up swapped the `seq` of exactly two files. |
| R-COL-5 | Several collections open at once; the open list is remembered locally and reopened at start. | Linux | In the app: two collections open; reopened after restart. |
| R-COL-6 | Unsaved edits are marked (● in the tree, "Unsaved" + Revert in the header) and kept while you switch between requests. Ctrl+S or Save writes to disk; sending never saves. | Linux | In the app: edited, switched away and back, edits and marker kept; saved with Ctrl+S. |
| R-GIT-1/2 | A collection is any folder with `tortuga.toml`; one TOML file per request, folders as directories. "Open…" opens a collection or turns a folder into one. | Linux | Automated tests. In the app: created a collection through the folder dialog; files checked on disk. |
| R-GIT-3 | Deterministic files; saving unchanged content does not touch the file. | Linux | Automated tests compare file content and modification time. |
| R-GIT-4 | Request files hold only shared data. Open collections and active environments are stored in the user's data folder, not in the repo. | Linux | Files inspected on disk. |
| R-GIT-5 | File names come from request/folder names; renaming renames the file; clashes get `-2`, `-3`. | Linux | Automated tests. In the app: "New request" saved as "Get user" became `get-user.toml`. |
| R-GIT-6 | Position stored as `seq` in each file, with gaps so most moves change one or two files. | Linux | Automated tests; checked on disk. |
| R-GIT-7 | `format` version in `tortuga.toml`; a collection from a newer version opens read-only with a badge. | Linux | Automated test. The badge has not been checked in the app. |
| R-GIT-8 | Hand-editable files; a file that can't be read (e.g. merge-conflict markers) shows as ⚠ with an explanation and can be deleted; the rest loads. | Linux | Automated tests. In the app: a file with conflict markers showed as ⚠ next to working requests. |
| R-P-1 | Local-first: collections are plain files on disk; no account or service involved. | Linux | By design; nothing else is contacted. |
| R-P-2 | Git is the collaboration layer: projects are plain files committed next to the code; teammates' changes arrive with `git pull` (M6) and workspaces can be shared as files (M6.5). | Linux | By design; see R-GIT-1 to 14 and R-WS-5. |

### M3: Environments, servers & variables

| Requirement | Feature | Verified on | How it was verified |
|---|---|---|---|
| R-SRV-1 | Named servers per collection; ids are generated once, so renaming a server keeps requests working. | Linux | Automated tests (rename keeps the id). In the app: server created and used. |
| R-SRV-2 | Environments dialog: one row per server, one column per environment, base URL in each cell. | Linux | In the app: two environments, one server, URLs filled in; files on disk match the documented format. |
| R-SRV-3 | Server dropdown in front of the URL field; the URL then holds only the path. The base URL of the active environment is shown under the URL bar. | Linux | In the app. |
| R-SRV-4 | Sending combines base URL and path. A missing server or URL stops the send with a message naming the server and environment. | Linux | Automated tests for both errors. In the app: same request went to `/dev/users/42` and `/staging/users/1` depending on the environment. |
| R-SRV-5 | Environment picker in the title bar for the selected request's collection; remembered locally per collection. | Linux | In the app: switched environment; still selected after restart. |
| R-VAR-1 | `{{name}}` substituted in URL, params, headers, and body (text, JSON, form). | Linux | Automated tests. In the app: `{{user_id}}` in the path. |
| R-VAR-2 | Collection-level variables and per-environment values. | Linux | Automated tests; in the app. |
| R-VAR-4 | Environment value wins over the collection default (secrets come in M4). | Linux | Automated tests. In the app: Development used 42, Staging fell back to the default 1. |
| R-VAR-5 | Undefined variables are highlighted (field turns amber), listed under the URL bar, and the send is stopped with "Undefined variable: …". | Linux | Automated tests. In the app: `Bearer {{token}}` header. |
| R-VAR-7 | Variables table: one row per variable, a Default column and one column per environment; empty cells show the default as a hint. | Linux | In the app. |
| R-COL-4 | Collection defaults for variables (headers and auth defaults: see M4). | Linux | As R-VAR-2. |

### M4: Auth & secrets

| Requirement | Feature | Verified on | How it was verified |
|---|---|---|---|
| R-HTTP-5 | Auth tab: inherit from collection, no auth, Basic, Bearer token. Fields take `{{variables}}`; literal credentials are masked and trigger a warning to use a secret variable. A hand-written `Authorization` header wins. | Linux | Automated tests (RFC 7617 Basic encoding, inherit/override/none, manual header wins, mock server receives the header). In the app: collection Bearer `{{token}}` inherited by a request; echo server received `Bearer s3cret`. |
| R-COL-4 | Collection defaults dialog (collection menu → "Defaults (headers, auth)…"): default headers and auth in `tortuga.toml`. A request header with the same name replaces a default; adding it switched off leaves it out. Inherited headers are listed read-only on the request's Headers tab. | Linux | Automated tests (inheritance, override, opt-out, exact file output). In the app: header and auth saved and inherited. |
| R-VAR-3 | Variables can be marked secret (🔒) in the environments table. `tortuga.toml` lists only the names (`secrets = ["token"]`); values (default and per environment) are stored on this machine and masked in the table. Marking a variable secret moves its values out of the files. | Linux | Automated tests (values go to the local store, never the files). In the app: secret saved; `grep` found the value in no collection file; still used after a restart. |
| R-VAR-4 | Local secret value, then environment, then collection default. | Linux | Automated tests. |
| R-LOC-1 | Secret values, cookie jars, open collections and active environments are stored in the user's app-data folder, keyed by collection path; nothing is written into the collection. | Linux | Files inspected on disk. |

### M5: Cookies

| Requirement | Feature | Verified on | How it was verified |
|---|---|---|---|
| R-CK-1 | Each collection has a cookie jar, on by default. Cookies from responses — also those set during redirects — are sent with later matching requests, with browser rules for domain, path, expiry and `Secure` (via the `cookie_store` crate). The jar is kept across restarts, session cookies included. The scratch request has its own jar for the session. | Linux | Automated tests (domain/path matching, redirect hop, JSON round trip). In the app: `/login` redirect set `session=abc123`; the next request sent it; still sent after a restart. |
| R-CK-2 | "Use cookies from this collection's cookie jar" on the request's Options tab; when off, nothing is sent from the jar and response cookies are not stored. Saved as `cookies = false` in the request file. | Linux | Automated tests. In the app: with it off the server got no cookie; request file checked. |
| R-CK-3 | Jars are saved in the app-data folder (`cookies/<hash>.json`, owner-only), never in the collection. "Clear cookies (N)" in the collection menu empties the jar (now **Clear all** in the cookie viewer, R-CK-5). | Linux | Automated tests. In the app: jar file `-rw-------`, emptied by Clear; no cookie files in the collection. |
| R-CK-4 | A `Cookie` header set on the request is always sent, also with the jar off, and then replaces the jar's cookies. | Linux | Automated test for both switch positions. In the app: `Cookie: mine=1` sent with the jar off. |

### M6: Live git collaboration

| Requirement | Feature | Verified on | How it was verified |
|---|---|---|---|
| R-GIT-10 | Open collections are watched; changes on disk (another editor, `git pull`, branch switch) reload automatically, debounced so a burst reloads once. Reading files is not a change. An open request without unsaved edits shows the new version. Windows drives under WSL are polled. | Linux | Automated tests (one report per burst; reads not reported; follow the disk). In the app: changed, deleted and new files appeared without any click. **Fixed with M7 (2026-10-17):** the app's own reads (reload, git status) counted as changes, so an open project reloaded about four times a second while idle; now 0 reloads in 6 s idle, 1 after an outside edit. |
| R-GIT-11 | A request with unsaved edits whose file changed shows a banner: "Keep mine" (save to overwrite) or "Take theirs"; if the file was deleted: "Restore with my edits" or "Discard my edits". The tree marks it ⚠. | Linux | Automated tests for all four answers. In the app: banner shown, "Take theirs" loaded the new version. |
| R-GIT-12 | Current branch next to the collection name; M / N / D / C markers on requests and folders (a folder shows the most important state inside); deleted requests stay visible as struck-through rows until the deletion is committed. Unreadable conflicted files show as ⚠ with C. | Linux | Automated tests against real repositories (modified, new, deleted, conflict). In the app: markers, a commit clearing them, branch switch, a merge conflict. |
| R-GIT-13 | "Open in file manager" and "Open in terminal" in the collection menu. Terminal: Windows Terminal/cmd, Terminal.app, `$TERMINAL` or common Linux terminals; under WSL, Windows Explorer and Windows Terminal running WSL in the folder. | Linux (WSL) | In the app: Windows Terminal started; no error for the file manager (the Windows window itself was not visible to the test). |
| R-GIT-14 | Git status is read with gitoxide (`gix`); no `git` executable is used. | Linux | In the app started with `PATH=/nonexistent`: branch and conflict marker still shown. |

### M6.5: Workspaces & projects

What was called a collection is now a **project** in the UI and the docs (D12); the file format did not change.

| Requirement | Feature | Verified on | How it was verified |
|---|---|---|---|
| R-WS-1 | Workspaces: a switcher in the title bar lists them, with New, Rename and Remove (never the last one). Each workspace opens its own projects and remembers its environment and selected request, also after a restart. Unsaved edits survive switching for the session. State saved before workspaces becomes "My workspace". | Linux | Automated tests (switching, restart, removing, migration, edits kept). In the app: old state migrated, renamed, new empty workspace, switched back and forth, selection restored after a restart. |
| R-WS-2 | A project is a folder with `tortuga.toml`, in its own repo, in a shared repo or outside git. "Collection" is replaced by "project" in the UI (sidebar "Projects", "Save to project…", "Project defaults", messages). | Linux | In the app: projects from two repositories and one without a remote. |
| R-WS-3 | "+ Add" → "Add existing projects…": pick a folder or repository; every `tortuga.toml` below it is listed as a checklist. Hidden, git-ignored, `node_modules` and `target` folders are skipped, and projects inside a project are not searched. Projects already in the workspace are shown but can't be added twice. If nothing is found, "Create a project there…". | Linux | Automated tests (search with `.gitignore`, checklist, nothing found). In the app: the platform repo listed `services/orders` and `services/users` (already open), `node_modules` skipped. |
| R-WS-4 | Projects that share a git repository are grouped under a repository row with its name and branch, shown once. One file watcher per repository, which tells each project only about its own changes and all of them about commits and checkouts. | Linux | Automated tests (grouping; one watcher, changes routed per project, `.git` changes to all). In the app: Users API and Orders API grouped under "platform · feature/ws". |
| R-WS-5 | Workspaces are kept in the app-data folder. "Save as file…" writes `<name>.tortuga-workspace.toml` with each project's path relative to the file and its git remote URL (passwords and tokens removed); after that the file is kept in sync and only rewritten when its content changes. "Open workspace file…" adds the workspace, or switches to it if it is already there. Missing projects show greyed with "missing", their path and clone URL, and "Locate…", "Copy clone URL" and "Remove from workspace". | Linux | Automated tests (file format, relative paths, credentials removed, missing and located projects, file follows). In the app: saved the file (checked on disk), moved a project away, opened the file: project shown as missing, located in its new folder, file updated. |
| R-WS-6 | One environment picker for the workspace: it lists every project's environment names and switches all projects that have the chosen one. A project without it keeps its own choice, is marked "⚠ <environment>" in the sidebar, and gets "Use environment" in its menu. | Linux | Automated tests. In the app: "Staging" switched both platform projects; Billing (no Staging) was marked and set to Dev from its menu. |
| R-WS-7 | "+ Add" → "New project…": pick or create a folder and name the project. Inside a git repository the dialog says so; otherwise "Make the folder a git repository" runs `git init` through gitoxide (using `init.defaultBranch`). A folder that already is a project is just added. | Linux | Automated test for `git init`. In the app: new project with a repository created (`.git`, branch from the git config), added to the workspace file. |
| R-WS-8 | Workspace variables (D38): "Variables…" in the workspace menu and the command palette; a name/value table with 🔒 secret toggles. Every project and the scratch request use them below every project value (runtime → secret → environment → folder → project → workspace); secret values go to this machine's secret store keyed by the workspace's local id and are masked in previews. Saved in local data and in the workspace file (`[variables]`, `secrets = [...]`, format 2 only when there are variables). A capture or `vars.set` named `workspace.<name>` sets `{{name}}` for the whole workspace until the app closes, below the project's own runtime values; the dialog lists these with "Clear captured". Workflows use the saved values; `tortuga-cli --workspace FILE`. | Linux | Automated tests (file format and version, local secrets, precedence and masking, prefix split in the runner and in scripts, capture key in request files, app state across two projects, rename keeps secrets, removed secrets deleted, the dialog via the workspace menu, CLI with `--workspace` and `--var` winning). In the app: a workspace file with `tenant` and secret `api_key` opened Auth and Orders; "List orders" showed only `{{token}}` undefined; "Log in" sent `X-Tenant: acme` and captured `workspace.token`; "List orders" then got 200 with `Bearer t-123`; the dialog listed both variables and `token = t-123`; typing the `api_key` value and saving put it in `secrets.toml` (`workspace:acme\|*\|api_key`), the workspace file unchanged; "Clear captured" made `{{token}}` undefined again. |

### v1 extras

| Requirement | Feature | Verified on | How it was verified |
|---|---|---|---|
| R-COL-8 | Unsaved edits (and the scratch request) are kept across restarts: written about once a second while they change, and when the app closes, to `drafts.json` in the app-data folder (owner-only, never in a project). On the next start they come back marked "Unsaved", with the selection. A request whose file changed or was deleted while the app was closed gets the "Keep mine / Take theirs" banner (R-GIT-11). | Linux | Automated tests (restart, scratch, saved edits not kept, change while closed, file round trip). In the app: an edit survived killing the app; after changing the file while closed, the banner appeared; "Take theirs" emptied the drafts file (`-rw-------`). |
| R-HTTP-9 | "Copy as curl" next to Save: method, URL with params, project headers, auth, body, Content-Type, timeout, and `-L` / `-k` from the settings, with all variables resolved (POSIX quoting). Basic auth is written as `-u user:password`. If the request uses secret values, a dialog offers "Copy masked" (shown as `<secret:name>`) or "Include secret values". Undefined variables stop the copy, like sending. Cookies from the jar are not included. | Linux | Automated tests (methods, headers, bodies, quoting, masking). In the app: masked and full copies read back from the clipboard; "Copied" feedback; undefined variable reported. |
| R-HTTP-10 | Path variables: `:name` segments in the URL path get a "Path variables" table on the Params tab that follows the URL. Values take `{{variables}}`, are URL-encoded when sent and saved as `[[path_vars]]`; an empty value stops the send ("Path variables without a value: id, name"). `host:8080` is never a variable. | Linux | Automated tests (parsing, encoding, server base URL, file round trip, mock server path). In the app: `/users/:id/files/:name` sent as `/users/42/files/my%20report.pdf`; saved request file checked. |

### M7: Release

Measured on this machine: Linux = Ubuntu 24.04 under WSL2/WSLg (OpenGL through `d3d12`); Windows = the Windows 11
host of that WSL (build 26100), running a release build cross-compiled with MinGW. Times are to the first frame
shown, from `tortuga --startup-time` (release build).

| Requirement | Feature | Verified on | How it was verified |
|---|---|---|---|
| R-NF-1 | Cold start under 1 s; a project with 1,000 requests opens in under 1 s. Only the sidebar rows in view are built, so a frame takes about 1 ms however big the projects are. `tortuga --startup-time` (or `make startup-time`) prints the start-up times and how long a frame takes, then quits. | Linux, Windows | Automated tests: 1,000 requests open in 40 ms (unoptimised build); only rows in view are built; a skipped row takes the space of a drawn one. Empty workspace: Linux 0.35–0.49 s, Windows 0.27–0.49 s (first launch slowest). With 1,000 requests (40 folders × 25, all expanded): Linux 0.38–0.40 s, Windows 0.42–0.52 s; frames 1.3 ms (were 6.5 ms before only visible rows were built). |
| R-NF-2 | Response bodies of 50 MB arrive and are shown without freezing the UI: bodies over 2 MB are shown as their first 2 MB, prepared off the UI thread without copying the rest; "Save to file…" saves the whole body. | Linux | Automated tests: a 50 MB body arrives unchanged through the client; a 50 MB JSON body is prepared in 44 ms (unoptimised, off the UI thread), keeps only 2 MB and draws in about 1 ms. In the app: a 50 MB JSON response (200 OK in 123 ms), scrolled through the shown part and clicked around without delay; "Save to file…" wrote a file byte-identical to the served one. |
| R-NF-3 | Fully usable offline: nothing is downloaded or contacted at start or while working; fonts and icons are built in. | Linux | In the app, started in a network namespace with only a loopback interface: started in 0.38 s, a request to a local server returned 200, a request to a public host failed with a DNS error message and the app carried on. |
| R-P-3 | The same code builds and runs on Windows, macOS and Linux (CI builds and tests all three). | Linux, Windows | Linux: everything in this file. Windows: the release build started, opened its window and drew frames (start-up measured above). |

### M8: Daily driver (Phase 2)

Checked in the running app on Linux (Ubuntu 24.04, WSL2/WSLg, release build) against a local echo server that returns
the request it received.

| Requirement | Feature | Verified on | How it was verified |
|---|---|---|---|
| R-HTTP-11 | Paste a `curl` command into the URL field: method, URL and params, headers, body (JSON, form or text), `-u` and `Authorization: Basic/Bearer` as auth, `-m` as timeout, `-G`, `-I`, `--json`, `--data-urlencode`, `-b` cookies. Bash quoting (`'…'`, `"…"`, `$'…'`) and line continuations. It fills the current request as unsaved edits (D19); a URL that starts with a server's base URL picks that server. Options that can't be used are listed in a notice. | Linux | Automated tests (parser with browser-style commands, combined short options, errors; server matching; pasting in the UI). In the app: pasted into the scratch request (POST, params, headers, Basic auth, JSON body; `--insecure` reported) and sent: the server received all of it. Pasted into a saved request: "Echo API" server chosen with only the path left, Unsaved, Revert restored it. |
| R-COL-9 | Quick open (Ctrl+P, or "Go to request…" in the title bar): every request in the workspace by name, folder, project, method or URL; words in any order, fuzzy; recent first. | Linux | Automated tests (matching, order, keys, opening from the app). In the app: "who" found "Who am I · Demo API › Admin", Enter opened it. |
| R-COL-10 | Command palette (Ctrl+K or Ctrl+Shift+P, or `>` in quick open): send, quick open, save, revert, copy as curl, new request, environments by name, workspaces, environments dialog and project defaults per project, settings, history, add/new project. | Linux | Automated test (Ctrl+K, filter, run). In the app: the full list, with shortcuts. |
| R-HTTP-12 | Search in the response body (Ctrl+F or the search field): matches highlighted, "n of m", Enter / Shift+Enter or ↑ ↓ move between them and scroll to them. Case-insensitive. | Linux | Automated tests (case folding, counting, Enter). In the app: "item 1" in a filtered body gave "2 of 3" after Enter, with the current match highlighted and scrolled into view. |
| R-HTTP-13 | JSON filter (`JSONPath` subset, D18) above JSON bodies: names, indexes (also negative), slices, wildcards, recursive `..`, unions and `?(@.field op value)` filters. Runs off the UI thread on the whole body (also over 2 MB); errors say where. Kept per request while the app runs. | Linux | Automated tests (every form, errors, a body over 2 MB filtered whole). In the app: `$.items[?(@.admin == true)].name` listed items 0, 3, 6…; the filter stayed for the next send. |
| R-HTTP-14 | Preview tab: the request as it will be sent (method, URL, all headers including inherited ones, Content-Type and Authorization, body), with secrets as `<secret:name>` and undefined variables left as `{{name}}`. | Linux | Automated tests (masking, undefined names, Content-Type; the tab with readable Basic credentials). In the app: `GET http://127.0.0.1:8770/echo/acme` with the folder's `X-Area` header. |
| R-HIST-1 | Every send is kept in local history per project (and for the scratch request): request as edited, environment, response with body up to 1 MB; the newest 200 per project. Written in the background, files readable only by the user (0600, folder 0700). | Linux | Automated tests (store, pruning, big and binary bodies, errors, file permissions, recording a send). In the app: entries appeared after each send and were still there after a restart; files checked on disk. |
| R-HIST-2 | History view in the sidebar (Projects \| History, D20): grouped by day in local time, filter field, method, name or URL, status, time. An entry opens its request with that response and a banner ("From history: sent Today 21:48 · Local") with "Restore request" and "Back to latest". | Linux | Automated tests (grouping, filter, opening, restoring). In the app: opened an older send, its response shown with the banner; "Restore request" put the old URL back. |
| R-HIST-3 | "Clear" in the History view: per project, the scratch request, or all (with confirmation). | Linux | Automated test. In the app: "Clear all history" emptied the list and removed the history folder. |
| R-HTTP-15 | Multipart bodies ("Multipart" body type): parts with a name and a text value or a file (with "Choose…"), switch, and optional content type; `{{variables}}` in names, values and paths. A file inside the project is saved relative to it (D21), a file elsewhere keeps its full path with a warning. Files are read when sending (by the app, off the UI thread); a missing file stops the send naming it. The Content-Type with its boundary is always set automatically. Copy as curl writes `--form-string` / `-F name=@path;type=…`; pasting curl reads `-F` and `--form-string`. Directories with only such files (no requests, no `_folder.toml`) are not shown as folders. | Linux | Automated tests (file format, path resolution, a mock server receiving text and file parts with the boundary, missing file, curl both ways, the parts table, relative paths, hidden asset folders). In the app: an "Upload avatar" request sent `title` (with a folder variable) and `avatar` from `fixtures/me.png` as `image/png` with its content; "Choose…" stored `fixtures/notes.txt` relative, and a file outside the project as a full path with the warning; the preview listed the parts; `fixtures/` was no longer shown in the tree. |
| R-HTTP-16 | Custom CA certificates in Settings ("Add…", remove): PEM files whose certificates are trusted besides the system's; kept in the local settings (D22). A file that can't be read or has no certificate is named (in the dialog, or in the notice at start) and the others still apply. The "not trusted" message points to it. | Linux | Automated tests: a real HTTPS server on localhost with a certificate from a test CA is refused without the CA and accepted with it; a bad file is reported while the good one applies; settings round-trip; the dialog. In the app: `https://localhost:8771/` failed with "UnknownIssuer", the CA was added through the file dialog and saved, and the same request returned 200. |
| R-COL-12 | Drag and drop in the tree: requests and folders can be dragged before or after another item, into a folder, or onto the project row (top level); a line or outline shows where. Only within one project, never a folder into itself, not in read-only projects. Usually only the moved file is written. | Linux | Automated tests (placing before, at the end, into a folder, renumbering when positions touch, into itself refused; dragging rows in the UI). In the app: "Upload avatar" dragged above "Who am I" (only its file rewritten, seq 900 before 1000), "Who am I" dropped on the project row (moved to the top level) and back into "Admin". |
| R-SRV-6 | Environments (columns) and servers (rows) are reordered in the environments dialog by dragging their ☰ handle; each environment keeps its URLs and values. Variables stay sorted by name (D23). | Linux | Automated test (moving columns and rows, saved order). In the app: "Staging" dragged before "Local", saved: `seq` 1 and 2 on disk with each URL kept, and the environment picker lists Staging first. |
| R-COL-11 | Folder defaults ("Folder defaults…" in a folder's menu): headers, auth and variables in `_folder.toml`. Requests inherit nearest folder first, then the project; the environment and secrets still win over folder variables. The editor shows where inherited headers and auth come from ("From folder “Admin”", "Inherit from folder “Admin”"). Renaming, moving and duplicating a folder keep its defaults. | Linux | Automated tests (file format, precedence and origins, folder operations, old files, the dialog, the editor). In the app: the server received the folder's `X-Area` header, `{{tenant}}` from the folder in the URL, and the folder's Bearer token (`acme-token`); `_folder.toml` checked on disk. |

Also fixed while checking M8: long names in the sidebar, quick open and history now shorten the name instead of the
method tag; a new response opens at the top instead of where the last one was scrolled to.

### M9: Workflows and tests (Phase 3)

Checked in the running app on Linux (Ubuntu 24.04, WSL2/WSLg, debug build) with a project of two chained requests
and a workflow over a two-row CSV file, against a local echo server; the CLI was run on the same project.

| Requirement | Feature | Verified on | How it was verified |
|---|---|---|---|
| R-TEST-1 | Test scripts in the request's **Scripts** tab: `test(name, fn)` and `expect(value)` with `toBe`, `toEqual`, `toBeTruthy`, `toBeFalsy`, `toBeDefined`, `toBeUndefined`, `toBeNull`, `toContain`, `toMatch`, `toHaveLength`, `toHaveProperty`, `toBeGreaterThan(OrEqual)`, `toBeLessThan(OrEqual)` and `.not` (D28). Results, variables set and console output in the response's **Tests** tab (count in the tab title). Saved as `[scripts] tests` in the request file. | Linux | Automated tests (every matcher, `.not`, failure messages, syntax and runtime errors, file format round trip, the Tests tab). In the app: "Log in" showed **Tests 2/2** with both names ticked and its `console.log` line. |
| R-TEST-2 | Pre-request scripts run before variables are filled in: they read and set variables and set headers; an error stops the send with its message. Saved as `[scripts] pre_request`. | Linux | Automated tests (headers, variables, an error stopping the send; against a mock server). In the app: the server received `X-Run: pre-none` set by the script. |
| R-TEST-3 | Script API: `res` (`status`, `statusText`, `headers`, `body`, `json()`, `header(name)`, `time`, `size`), `req` (`method`, `url`, `headers`, `body`, `setHeader`), `vars` (`get`, `set`, `unset`, `has`), `console.log`. Boa (D24), off the UI thread, no file or network access; endless loops and deep recursion are stopped with a clear message; response bodies over 10 MB are not passed to scripts. | Linux | Automated tests (each object, no `require`/`fetch`, loop and recursion limits). In the app: as above. |
| R-TEST-4 | Captures (table under the scripts, and per workflow step): JSONPath (first match), `header:<name>`, `status`. Saved as `[capture]` in the request or step. | Linux | Automated tests (each kind, missing values reported, file format). In the app: `token = $.items[1].name` captured "item 1"; a step capture added in the workflow editor was saved to the workflow file. |
| R-TEST-5 | Runtime variables: captured and script-set values per project until Tortuga closes, never written to disk (D26); they win over every other value and count as defined; **Runtime variables…** in the project menu lists and clears them. | Linux | Automated tests (precedence, defined, clearing). In the app: after "Log in", `{{token}}` in "Get user" was no longer marked undefined and its value was used; the dialog listed `token = item 1`. |
| R-FLOW-1 | Workflows are TOML files in `workflows/` (D29): name, `continue_on_failure`, optional `data` file, and steps naming request files with optional `name`, `capture` and `tests`. Steps follow requests that are renamed or moved. | Linux | Automated tests (file format round trip, broken files, moving and renaming requests and folders). In the app: files created, changed and deleted on disk as expected. |
| R-FLOW-2 | A **Workflows** group under each project in the sidebar; **New workflow…**, **Rename…**, **Delete…**, **Run** in the menus, and **Run workflow: …** / **New workflow…** in the command palette. The editor: name, "Continue after a failed step", data file, steps (add from a picker, reorder, remove, open the request, **Checks…** for a step's name, captures and tests), Unsaved / Save / Revert. | Linux | Automated tests (adding, moving, removing steps; creating, renaming with unsaved edits kept, saving, deleting). In the app: created "Smoke test", added both requests, renamed "Lifecycle" to "User lifecycle", deleted a workflow; checked on disk each time. |
| R-FLOW-3 | **Run** (or Ctrl+Enter) sends the steps in order with the active environment, the steps' scripts, captures and tests, and the unsaved draft. Captured values flow to later steps without touching the project's runtime variables. A failed step (failed test, error, 4xx/5xx) ends the iteration unless the workflow continues after failures. Results per step: status, time, tests; a step's detail shows the sent request, tests, variables set, console output and the response body. **Cancel** stops a run. | Linux | Automated tests (chained captures against a mock server, stop and continue on failure, cancel, the results view). In the app: two-row run with live progress; a failed test shown under its step and in the detail; Cancel during the second of two slow steps stopped the run ("Cancelled", step 2 never arrived). |
| R-FLOW-4 | Data-driven runs: a CSV (RFC 4180, first row names the variables) or JSON array of objects, chosen in the editor (saved relative to the project); one iteration per row with that row's values as variables and a fresh cookie jar; results grouped per row. | Linux | Automated tests (CSV quoting and errors, JSON values, one iteration per row). In the app: `users.csv` with `ann` and `bob` gave "Row 1" passed and "Row 2" failed on the `u=ann` test, as written. |
| R-CLI-1 | `tortuga-cli run <project> [workflow, folder or request]` with `--env`, `--data`, `--var`, `--insecure`, `--cacert`: prints each step with its tests and logs; exit code 0 / 1 / 2. No GUI libraries (D5). | Linux | Automated tests (argument parsing, target lookup, the binary end to end against a mock server, exit codes). By hand: the workflow above (exit 1 with the failing row), a single request (undefined variable reported, exit 1). |
| R-CLI-2 | Variables from `--var name=value` and `TORTUGA_VAR_<name>` (D30); they win over the project's values. | Linux | Automated tests. By hand: `TORTUGA_VAR_user=ann` was used by the request. |
| R-CLI-3 | `--report junit --out FILE` (a test suite per iteration, a test case per test or per step without tests, skipped steps as `<skipped/>`) and `--report json`. | Linux | Automated tests (both formats, escaping). By hand: the JUnit file for the two-row run (10 tests, 1 failure). |

Also fixed while checking M9: the results list now uses the whole page until a step is chosen; step names line up;
counts say "1 step" rather than "1 steps".

### Request tabs (D31, D32)

Checked in the running app on Linux (Ubuntu 24.04, WSL2/WSLg, debug build, dark theme) with two projects, "Users API"
and "Orders API", in one workspace.

| Requirement | Feature | Verified on | How it was verified |
|---|---|---|---|
| R-COL-13 | Requests open in tabs above the editor. A single click (tree, quick open, history, a workflow step) opens the *preview tab* (italic title; the next single click replaces it); double-clicking the tab or the tree row, or editing the request, keeps it; sending does not. New, duplicated and saved-from-scratch requests open as kept tabs. Closing (🗙, middle-click, Ctrl+W) never discards unsaved edits; the shown tab then goes back to the one used before. Right-click: Keep open, Close, Close others, Close to the right, Close saved tabs. "⏷" lists every tab (unsaved ones with a dot). Ctrl+Tab goes to the previously used tab, Ctrl+PageDown/PageUp to the next/previous one. Tabs follow renames and moves and close when their request is deleted (also in git) unless it has unsaved edits (then the conflict banner asks, R-GIT-11). Open tabs, the preview tab and folded groups are kept per workspace in local data and come back after a restart. | Linux | Automated tests (the tab model: preview, keep, close, used order, neighbours, renames, deletes, saved form; the shell: send does not keep, edit keeps, closing keeps edits, restart, deleted in git; the strip: clicking, closing, chips; the local-data round trip). In the app: single clicks replaced the preview, double-clicks kept tabs, editing "Delete user" kept it with the unsaved dot, Ctrl+Tab, Ctrl+PageDown, Ctrl+W, middle-click and "Close others" (edits of "Delete user" still there), the "⏷" list, and the tabs back after a restart with the unsaved edit. |
| R-COL-14 | With tabs of two or more projects open, each project's tabs sit together after a chip with its name in a colour worked out from the name; the project rows in the sidebar get a dot in the same colour when the workspace has several projects. Clicking a chip folds the group into "Name · count" (the shown tab stays visible) and unfolds it; opening one of its requests unfolds it; right-click: Collapse/Expand, Close the project's tabs. The preview tab comes after all groups with a stripe in its project's colour and moves into its group when kept. Folded state is forgotten once the chip disappears. | Linux | Automated tests (groups and chips with one and two projects, closing a group, folding, stable colours). In the app: Users API (pink) and Orders API (orange) chips and dots; the preview "List orders" moved into its group when double-clicked; Users API folded to "Users API · 2" and back; closing the last Orders tab removed both chips. Light theme not checked (the app follows the system theme, which is dark here). |

### After Phase 3

Checked in the running app on Linux (Ubuntu 24.04, WSL2/WSLg, debug build, dark theme) with one project, a workflow and
a local server that sets three cookies.

| Requirement | Feature | Verified on | How it was verified |
|---|---|---|---|
| R-CK-5 | **Cookies (N)…** in the project menu opens the cookie viewer: name, value (full value on hover), where it is sent (`host/path`, `*.domain` when subdomains get it too, explained on hover), expiry in local time or "Session", `Secure` / `HttpOnly`. 🗙 removes one cookie, **Clear all** empties the jar; both are saved at once. | Linux | Automated tests (listing with attributes, removing by domain/path/name, the dialog). In the app: after "Log in" the viewer listed `session` (Session, HttpOnly), `theme` (`*.127.0.0.1`, tomorrow) and `api_token` (`/api`, Secure); still there after a restart; 🗙 on `theme` removed it from the jar file (`-rw-------`). Clear all only by the automated test and the existing R-CK-3 path (the session ended before it was clicked in the app). |
| R-FLOW-5 | Unsaved workflow edits are kept in local data with the request drafts (R-COL-8) and come back after a restart; the workflow row in the sidebar has the unsaved dot. If the file changed on disk meanwhile the edits are kept and a notice says Revert takes the file; if it was deleted the edits are dropped with a notice. Edits of a project that is not open (another workspace) wait until it opens. | Linux | Automated tests (restart, changed and deleted files, saving clears the draft, the drafts file round trip). In the app: a saved draft with 2 steps and "Continue after a failed step" came back marked Unsaved with the dot on "Smoke", ran with both steps, and was still there after a second restart. |

Also fixed: the workflow row in the sidebar says "1 step" rather than "1 steps".

### OpenAPI import (Phase 4)

Checked in the running app on Linux (Ubuntu 24.04, WSL2/WSLg, debug build, dark theme) with a YAML Pet Store
description in a git repository, against a local echo server.

| Requirement | Feature | Verified on | How it was verified |
|---|---|---|---|
| R-COL-15 | **+ Add → Import OpenAPI…** (and the command palette) reads an OpenAPI 3.0/3.1 file in JSON or YAML (D33). A dialog shows the project name, "N requests in M folders", the environments and secret variables, the folder it will create (named after the API, next to the file; **Choose…** picks another; a folder that already is a project is refused, D34; the git repository it is in is named) and what could not be imported as written. The project gets one server named after the API with a base URL per `servers` entry as environments, server `{variables}` filled with defaults (D35); a folder per tag in the description's order, untagged requests at the top level; a request per operation named by its summary, `{param}` → `:param` path variables, query and header parameters (required on, optional off) with example or default values, JSON / form / multipart / text bodies from the example or built from the schema (`$ref`, `allOf`, `oneOf`, recursion stopped); security schemes as project or request auth with secret variables (bearer, basic, API key in header/query/cookie; OAuth 2 and OpenID Connect as `Bearer {{access_token}}` with a note), `security: []` as no auth. With secrets, the Environments dialog opens after the import. | Linux | Automated tests (mapping of servers, tags, parameters, bodies, security and warnings; versions and errors; files on disk; refusing an existing project; the dialog; the shell). In the app: the dialog for `openapi.yaml` showed "5 requests in 2 folders", Local, Staging and `token`, the repository hint; Import created `pet-store/` (Pets, Owners, Health check, marked N in git) and opened Environments with both URLs and 🔒 `token`; "Get a pet" had `:petid = 7` and was stopped for the undefined `{{token}}`; "Health check" (`security: []`) got 200 from `/v1/health` without an Authorization header; "Create a pet" had the JSON body from the schema. Sending with a typed token was not tried (WSLg kept dropping keyboard focus); bearer auth from secret variables is covered by the R-HTTP-5 and R-VAR-3 tests. |

### Postman import (Phase 4)

Checked in the running app on Linux (Ubuntu 24.04, WSL2/WSLg, debug build, dark theme) with a v2.1 collection in a
git repository, against a local echo server.

| Requirement | Feature | Verified on | How it was verified |
|---|---|---|---|
| R-COL-16 | **+ Add → Import Postman collection…** (and the command palette) reads a Postman collection exported as v2.0 or v2.1 JSON, also wrapped in `{"collection": …}` as Postman's API returns it; v1 collections are refused with a hint to export again. Either import entry recognises the other format. The same dialog as R-COL-15 shows the project, "N requests in M folders", the environments and secret variables, whether their values were found in the file, the folder it will create and what could not be imported as written. The project gets the collection's folders and requests in their order and nesting; collection variables as project variables (disabled ones left out); folder variables, auth and API key headers as folder defaults (R-COL-11); a `{{variable}}` that starts request URLs as a server named after it, with its value as the base URL of a "Default" environment (D36); query parameters with their on/off state, `:path` variables with their values, headers; raw (JSON, or text with a Content-Type for XML/HTML/JavaScript), urlencoded, form-data (files with a warning to check their paths) and GraphQL (as a JSON body) bodies. Bearer, basic and API key auth (header or query) at collection, folder and request level become auth with secret variables; values written in the collection go to this machine's secret store as the default for every environment, never into the files (D35); a folder with "no auth" turns auth off on its requests; OAuth 2 becomes `Bearer {{access_token}}` with a note; other auth types are left out with a warning. Request scripts are kept as comments under a note to port them (D28, D36); collection- and folder-level scripts, file bodies, unsupported methods and `{{$dynamic}}` variables are warnings. The Environments dialog opens after the import only when a secret still has no value. | Linux | Automated tests (mapping of folders, variables, base URLs, auth at every level, bodies, scripts and warnings; v2.0, wrapped and URL-only requests; versions and errors; format detection; files on disk without credentials; the dialog; secret values into the secret store in the shell). In the app: the menu showed both import entries; the dialog for `shop.postman_collection.json` showed "3 requests in 2 folders", Default, `token` with "Values for these were found in the file", the repository hint and the script warning; Import created `shop-api/` (Orders, Public, marked N in git) without opening Environments, and no file contains the token; "List orders" showed server `baseUrl`, `limit = 10`, the commented test script, and got 200 with `Authorization: Bearer demo-token-123` from the secret store; "Health" in the "no auth" folder was sent without an Authorization header. |

### `.http` import (Phase 4)

Checked in the running app on Linux (Ubuntu 24.04, WSL2/WSLg, debug build, dark theme) with the Pokémon read-API
file (13 requests, `@baseUrl`), against a local echo server.

| Requirement | Feature | Verified on | How it was verified |
|---|---|---|---|
| R-COL-17 | **+ Add → Import .http file…** (and the command palette) reads a `.http`/`.rest` file of the VS Code REST Client or the JetBrains HTTP Client; the other import entries recognise one too (by extension or by its first request line), and a file without requests is refused. Same dialog as R-COL-15. Requests: one per `###` block in file order, named by the `###` title, else `# @name`, else the comment above the request line; `METHOD URL [HTTP/1.1]` or a bare URL (GET); `?`/`&` query lines; headers (comments skipped); `GET /path` with `Host`; `# @no-cookie-jar`. `@name = value` variables (anywhere in the file) become project variables; one that starts URLs becomes a server with a "Default" environment (D36). `Authorization: Bearer/Basic` (also REST Client's `Basic user pass` and base64) becomes auth, other schemes stay a header; their values and key/token headers become secret variables, and found values go to the secret store (D35); the same auth on every request becomes the project's. REST Client request variables `{{name.response.body.$.x}}` / `.headers.X` become a capture in that request and `{{name_x}}` (R-TEST-4); others are a warning. Bodies: JSON (by Content-Type or shape), urlencoded as form fields (decoded), GraphQL (`X-Request-Type: GraphQL`) as JSON, others as text; `< file` bodies, script files, TRACE and `{{$dynamic}}` variables are warnings. JetBrains `> {% %}` / `< {% %}` scripts are kept as comments (D37). | Linux | Automated tests (the file from the request; REST Client and JetBrains features: names, query lines, auth, secrets, captures, bodies, scripts, directives, `Host`; shared auth; warnings; detection and errors; files on disk with captures; the dialog). In the app: the menu showed **Import .http file…**; the dialog for `pokemon.http` showed project "Pokemon", "13 requests", Default, no secrets and no warnings; Import added the project with the 13 requests in file order under their `###` titles; "Filter by type and sort by stat total" showed server `baseUrl` with `type`, `sort` and `limit`, and got 200 from `/pokemon?type=fire&sort=total&limit=5`. |
| R-COL-18 | Settings → **Importers** (the dialog now has General and Importers tabs): a checkbox per importer, all on by default. One that is off is left out of "+ Add" and the command palette, and a file another entry recognises as that kind is refused ("… importing Postman collections is turned off in Settings"). The app's settings file gets `[importers] disabled = [...]` only when one is off; ids are kept as text, so files from newer versions load. | Linux | Automated tests (defaults and round trip, older and newer settings files, the Importers tab, "+ Add" and palette filtering, refusing a recognised file). In the app: unticked "Postman collections" and saved; `settings.toml` got `disabled = ["postman"]`; "+ Add" listed only OpenAPI and `.http`. The refusal message was checked by test only (the file dialog can't be driven under WSL). |

### Settings on three levels (D39)

| Requirement | What | Verified on | How |
|---|---|---|---|
| R-SET-1 | Timeout, follow redirects and send and store cookies cascade: request (own timeout, cookies off) → project → workspace → app. The project's are set on the **Requests** tab of its **Defaults (headers, auth, requests)…** dialog and kept in `tortuga.toml` `[settings]` (format 2 only then, so older builds open the project read-only instead of dropping them); the workspace's on the Requests tab of **Defaults…** in the workspace menu (and the command palette), kept in local data and the workspace file. Each field shows the inherited value and its level ("9000 (from the workspace)", "Inherit: on (from the app's settings)"); empty or "Inherit" removes the override, and only overrides are written. Settings → General has "Send and store cookies" for the app level. The request's Options tab shows where its timeout comes from, and greys out "Use cookies" when the project or workspace turns cookies off. A request's redirect choice uses a second client internally (also with cookie jars); `curl` export follows it. Workflows and `tortuga-cli --workspace` apply the workspace's and project's. | Linux | Automated tests (the cascade and levels, file formats and versions, prepared requests, redirects against a mock server with and without a jar, workflows with workspace defaults, the Requests tab, the workspace menu, app state across two projects and a teammate opening the file). In the app: a workspace file with 9 s timeout and "don't follow redirects", Auth set to follow: Orders' "Moved" stopped at 302, Auth's followed to `/landed`; Options showed "9000 (from the workspace)"; the Auth project's Requests tab showed the inherited values, and choosing cookies Off wrote `cookies = false` (format 2) and greyed out the request's cookie switch. |
| R-SET-2 | The workspace's **Defaults…** also has Headers and Auth, inherited below the project's: request → folder → project → workspace (a project without auth inherits the workspace's; "No auth" on a request still wins). Inherited headers and auth say "from the workspace" in the request, with **Edit workspace defaults…**. Kept like R-SET-1; secrets in them use `{{secret variables}}`. The scratch request doesn't use them. | Linux | Automated tests (header and auth precedence and origins, workflows, the dialog). In the app: a workspace with `X-Tenant: {{tenant}}` and `Bearer {{token}}`; "List orders" in Orders (no auth of its own) showed both as from the workspace and got 200 with `Bearer t-123` and `x-tenant: acme`; the login request used "No auth". |
| R-SET-3 | Settings → **Requests**: under "Verify TLS certificates", a list of hosts not to verify, one per line (`localhost`, `*.dev.internal` for subdomains; a scheme, port or path typed with a host is ignored), with a warning. Only an app setting: shared files have no way to turn verification off. Requests to a listed host show "⚠ The certificate of … is not verified" under the URL bar. The client for a listed host doesn't verify certificates and only follows redirects to other listed hosts. The "not trusted" error points at the list. | Linux | Automated tests (host patterns, a real HTTPS server with an untrusted certificate: listed host accepted, unlisted refused, a redirect from the listed to the unlisted host not followed; the settings field). In the app: with `localhost` listed, "Secure localhost" (`https://localhost:4443`) got 200 with the warning, "Secure IP" (`https://127.0.0.1:4443`, same certificate) was refused. |
| R-SET-4 | Settings → **Appearance** → Theme: like the system (default), light or dark, applied as soon as it is saved. | Linux | Automated test (the setting reaches egui). In the app: Light was applied at once (the first check of the light theme). |
| R-SET-5 | Settings → **Appearance** → Text size (80–200 % in the list, 100 % by default) zooms the whole UI; Ctrl+= / Ctrl+- / Ctrl+0 change it too, and that is saved. | Linux | Automated test (applied, changed with the keys, saved). In the app: two Ctrl+= gave 120 % (saved), Ctrl+0 went back to 100 %. |
| R-SET-6 | Settings → **History**: "Keep a history of sent requests" (on) and entries per project (200). Off records nothing more and leaves what is there (the History view says history is off); a new size trims every project's history when saved. | Linux | Automated tests (the store's keep and trim, off and trimming in the shell, the settings tab). In the app: off, a send added nothing (9 entries before and after) and the History view showed the note; on with 3 trimmed a project from 6 entries to 3. |
| R-SET-7 | Settings → **Appearance** → "Wrap long lines in responses" (off), also a **Wrap** switch above the body. Wrapped rows are cut at the panel's width (the body is monospace), so only the rows in view are still laid out (R-NF-2); search and "next match" follow the wrapped rows. | Linux | Automated tests (cutting rows, multi-byte characters, the settings tab). In the app: a 120-word line wrapped to the panel and reflowed at 120 % text size; the switch was saved in the settings. |

## In progress

| Requirement | State |
|---|---|
| R-P-4 | No telemetry or analytics code exists, and the app only sends requests the user makes. Not yet confirmed with a network monitor. |
| R-HTTP-8 | Proxy settings from the Windows and macOS system settings are expected to work (handled by the HTTP library), but have not been tested on those OSes. |
| R-LOC-2 | The OS credential store (Keychain, Credential Manager, Secret Service via `keyring`) is used when available; otherwise values go to `secrets.toml` in the app-data folder with owner-only permissions (0600 on Unix) and the environments dialog shows a warning. **Verified:** the fallback on Linux/WSL (no Secret Service), including file permissions. **Not yet checked:** the OS credential store itself, on any OS. |
| R-NF-4 | Installers are built with cargo-packager (D16): `make package` locally, and the **Release** workflow on each OS for a `v*` tag (draft GitHub release) or by hand (artifacts); see [RELEASING.md](RELEASING.md). The app has an icon (window, `.exe`, packages), version information in the `.exe`, no console window on Windows, and `--version`. **Verified:** on Linux the `.deb` (dependencies, desktop entry, icons) and `.AppImage` were built and both started, within the start-up budget; the Windows `.exe` shows the icon and version information. **Not yet checked:** the `.msi` and `.dmg` (built only by the workflow, which has not run yet) and installing the `.deb` with `apt`. |
| R-P-3 | macOS has not been run yet (built and tested by CI only). |
| R-CLI-4 | The **Release** workflow builds `tortuga-cli` on each OS, checks `--version` and attaches `tortuga-cli-<version>-<os>.tar.gz` (`.zip` on Windows) with the README and licences. **Verified:** the archive step run locally on Linux (binary needs only libc). **Not yet checked:** the workflow itself, and the Windows and macOS archives. |

## Known limitations

- macOS builds are checked by CI but have not been run yet. On Windows only start-up was checked (M7), not the features themselves.
- The installers are not signed (D17): macOS and Windows warn before opening them the first time.
- Form URL-encoded bodies are covered by automated tests; in the app only switching to the form editor was checked, not sending a form.

## Agreed requirements

The requirements the features above were built to, as agreed in [ROADMAP.md](ROADMAP.md) (moved here on
2026-10-18, when Phase 2 was done, and the Phase 3 ones the same day when it was done). Open ones are still in the roadmap.

### Principles

| ID | Requirement |
|---|---|
| R-P-1 | **Local-first.** All data are plain files the user owns. No account, login, or cloud service is needed. |
| R-P-2 | **Git is the collaboration layer.** Teams share collections by committing them to a git repository, often next to the API source code. The on-disk format is effectively our public collaboration API. |

### HTTP requests

| ID | Requirement |
|---|---|
| R-HTTP-1 | Methods: `GET`, `POST`, `PUT`, `PATCH`, `DELETE`, `HEAD`, `OPTIONS`. |
| R-HTTP-2 | URL editor; query parameters also editable as a key/value table, kept in sync with the URL. Each param can be disabled without deleting it. |
| R-HTTP-3 | Headers as a key/value table; each header can be disabled. Duplicate header names are allowed. |
| R-HTTP-4 | Body types: none, raw text, JSON, `x-www-form-urlencoded`. Content-Type is set automatically unless overridden. |
| R-HTTP-5 | Auth: none, Basic, Bearer token. Set per request, or inherited from the collection. |
| R-HTTP-6 | Response view: status, time, size, headers, body. JSON bodies are pretty-printed; others shown as text; binary bodies can be saved to a file. |
| R-HTTP-7 | An in-flight request can be cancelled. Per-request timeout with a global default. |
| R-HTTP-9 | Copy a request as a `curl` command with variables resolved. Secret values are masked unless the user confirms including them. *(D1; was IDEA-4)* |
| R-HTTP-10 | Path variables: a URL path segment starting with `:` (e.g. `/users/:id`) is a path variable. Its value is edited in a table that follows the URL, may contain `{{variables}}`, is URL-encoded when sent, and is saved in the request file. Sending stops with a message naming any path variable without a value. Only the path counts, so `host:8080` is not a variable. |
| R-HTTP-11 | **Paste a `curl` command** into the URL field to fill the current request: method, URL and query params, headers, body (JSON, form or text), Basic auth (`-u`) and timeout (`-m`). It becomes unsaved edits (Revert undoes it, D19). If the URL starts with a server's base URL in the active environment, that server is chosen and the URL keeps only the path. Options Tortuga can't use (e.g. `-F`, `-k`) are listed in a notice. Bash/zsh quoting and line continuations are understood (what browsers' "Copy as cURL (bash)" produces). *(IDEA-28, D7)* |
| R-HTTP-12 | **Search in the response body** (Ctrl+F while the response is shown, or the search field above it): matches are highlighted, "n of m" is shown, Enter / Shift+Enter go to the next / previous match and scroll to it. Case-insensitive. Searches the shown text (the first 2 MB of big bodies). *(IDEA-30)* |
| R-HTTP-13 | **Filter JSON responses** with a JSONPath expression (D18): `$`, `.name`, `['name']`, `[n]` (negative counts from the end), `[a:b]`, `[*]`, `..name`, `[a,b]`, and filters `[?(@.field)]` / `[?(@.field <op> value)]` with `== != < <= > >=`. The matches are shown pretty-printed; an invalid expression is explained. The filter is kept per request while the app runs, so a new response is filtered the same way. Works on bodies of any size: it runs off the UI thread. *(IDEA-30)* |
| R-HTTP-14 | **Preview** tab in the request editor: the request exactly as it will be sent (method, full URL, every header including inherited ones, Content-Type and Authorization, body), with variables, server and path variables resolved and secret values masked as `<secret:name>`. Undefined variables are named instead. Updates while typing. *(IDEA-7)* |
| R-HTTP-15 | **`multipart/form-data` bodies**: a list of parts, each with a name and either a text value or a file to upload; parts can be switched off; names, values and file paths may use `{{variables}}`. A file inside the project folder is saved relative to it, so teammates can send the request too; a file elsewhere keeps its full path and is marked as working only on this computer (D21). The file is read when sending; a missing file stops the send with a message naming it. A part can set its own content type. A directory with files to upload but no requests (and no `_folder.toml`) is not shown as a folder. "Copy as curl" writes `-F`/`--form-string`; pasting curl reads them (R-HTTP-9, R-HTTP-11). *(IDEA-2)* |
| R-HTTP-16 | **Custom CA certificates**: in Settings, the user adds PEM files (each may hold several certificates) whose certificates are trusted in addition to the system's, e.g. for internal APIs signed by a company CA. They are local settings, not shared (D22). A file that can't be read or holds no certificate is reported naming it, and the others still apply. *(IDEA-3)* |

#### History

| ID | Requirement |
|---|---|
| R-HIST-1 | Every sent request is kept in a **local history** (§6.7) per project, and one for the scratch request: when it was sent, the environment, the request as it was in the editor, and the response (status, time, size, headers, body). The last 200 entries per project are kept; bodies over 1 MB are cut to 1 MB. Like unsaved edits, history may hold credentials typed into a request, so it is readable only by the user. *(IDEA-5)* |
| R-HIST-2 | The sidebar has a **History** view next to Projects (D20): the workspace's entries, newest first, grouped by day, each with method, name or URL, status and time; a filter field searches them. Choosing an entry opens it: its request is selected (or the scratch request, if it is gone) and the response panel shows that past response, marked with its time. "Restore request" puts that version of the request into the editor as unsaved edits. *(IDEA-5)* |
| R-HIST-3 | History can be cleared per project, or all at once. *(IDEA-5)* |

### Cookies

| ID | Requirement |
|---|---|
| R-CK-1 | Each collection has a cookie jar. Cookies from responses are stored in it and sent with later matching requests, like a browser does. On by default. |
| R-CK-2 | **Cookies can be disabled per request.** A disabled request sends no cookies from the jar and does not store cookies from its response. The setting is saved in the request file. |
| R-CK-3 | The cookie jar is local data (§6.7), never written to the collection directory. The jar can be cleared. |
| R-CK-4 | A `Cookie` header set manually on a request is always sent, even when the jar is disabled for that request. |
| R-CK-5 | **Cookie viewer**: the cookies in a project's jar, with value, domain, path, expiry and flags; single cookies can be removed. *(IDEA-6)* |

### Collections

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
| R-COL-9 | **Quick open** (Ctrl+P): a search box over every request in the workspace (name, folder path, project, method, URL). Words match in any order and fuzzily; recently opened requests come first. Arrow keys move, Enter opens, Escape closes. *(IDEA-27, D2)* |
| R-COL-10 | **Command palette** (Ctrl+K, or Ctrl+Shift+P): every frequent action by name, with its shortcut: send, save, revert, copy as curl, new request in the current folder, switch environment, switch workspace, open environments, project defaults, settings, history, add project. Typing filters like quick open. *(IDEA-27)* |
| R-COL-12 | **Drag and drop in the tree**: drag a request or folder onto a folder to move it into it, or between two items to put it there; the order is saved like "Move up/down" (R-COL-3, R-GIT-6). Only within one project, and not into itself; read-only projects can't be changed. A line or highlight shows where it will land. *(IDEA-9)* |
| R-COL-11 | **Folder defaults**: a folder can define headers, auth and variables (in its `_folder.toml`), edited from the folder's menu. A request inherits from its folders, nearest first, then from the project: its own header replaces an inherited one with the same name; its auth is its own, else the nearest folder's, else the project's. Variables: local secret, then the active environment, then folder variables (nearest first), then project variables. The editor shows where an inherited header or auth comes from. *(IDEA-1; extends R-COL-4, R-VAR-4)* |
| R-COL-13 | **Request tabs** (D31). Several requests are open at once in tabs above the editor; only requests open in tabs, not workflows. A single click (tree, quick open, history) opens the request in the *preview tab*: there is at most one, its title is in italics, and the next single click replaces it. Double-clicking the tab or the tree row keeps it open. Editing a request keeps its tab; sending does not. New, duplicated and saved-from-scratch requests open as kept tabs. Closing a tab never discards unsaved edits (they stay, as in R-COL-6). Tab menu: Keep open, Close, Close others, Close to the right, Close saved tabs; middle-click closes. A "⏷" list shows every open tab; Ctrl+Tab goes to the previously used tab, Ctrl+PageUp/PageDown to the neighbours, Ctrl+W closes the shown tab. Tabs follow renames and moves, and close when their request is deleted. Open tabs are local data per workspace (§6.7) and come back after a restart. *(IDEA-31)* |
| R-COL-14 | **Tabs grouped by project** (D32). Tabs of one project sit together after a chip with the project's name, in a colour worked out from the name; the project's row in the sidebar shows a dot in the same colour. Chips only show when tabs of two or more projects are open. Clicking a chip collapses the group to the chip (with a count) and expands it again; opening a request of a collapsed group expands it. The preview tab sits after all groups, with a stripe in its project's colour, and moves into its group when it is kept. The "⏷" list groups tabs by project. Collapsed groups are remembered with the tabs. |
| R-COL-15 | **Import an OpenAPI description** (D7, D33–D35) as a new project: servers become environments of one server, tags become folders, operations become requests with their parameters, example bodies and auth that uses secret variables. What can't be imported as written is listed before importing. *(IDEA-29)* |
| R-COL-16 | **Import a Postman collection** (v2.0 or v2.1 JSON; D7, D34, D36) as a new project: folders and requests in the same order and nesting; collection variables as project variables, folder variables and auth as folder defaults; a `{{variable}}` that starts request URLs as a server with its base URL in a "Default" environment; bearer, basic and API key auth with secret variables, whose values found in the file are kept in this machine's secret store only (D35); raw, form, multipart and GraphQL bodies; scripts kept as comments to port. What can't be imported as written is listed before importing. *(IDEA-10)* |
| R-COL-17 | **Import a `.http` file** (VS Code REST Client, JetBrains HTTP Client; D7, D34, D37) as a new project named after the file: a request per `###` block in file order, `@variables` as project variables, a base URL variable as a server with a "Default" environment (D36), `Authorization` and key headers as auth with secret variables whose values found in the file are kept in this machine's secret store only (D35), request variables (`{{login.response.body.$.token}}`) as captures, JSON, form, GraphQL and text bodies, handler scripts kept as comments. What can't be imported as written is listed before importing. |
| R-COL-18 | **Turn importers on and off** in the app's settings; all are on by default, also ones added later. One that is off is not offered, and a file recognised as that kind is refused. |

#### Workspaces & projects

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
| R-WS-8 | **Workspace variables** (D38): values every project of the workspace can use as `{{name}}`, below every value a project defines; shared in the workspace file, secret ones by name only with values on this machine; captures and scripts set them with a `workspace.<name>` name for the session. *(IDEA-38)* |

### Variables & environments

| ID | Requirement |
|---|---|
| R-VAR-1 | `{{name}}` placeholders are substituted in URL, params, headers, body, and auth fields. |
| R-VAR-2 | A collection has any number of named environments (e.g. `dev`, `staging`), plus collection-level variables. One environment is active at a time. |
| R-VAR-3 | **Secrets are never written into the collection directory.** A variable can be marked secret; its value is stored only on the local machine (§6.7). Teammates see that the secret exists, not its value. |
| R-VAR-4 | Order of precedence, highest first: local secret value, then the active environment, then collection variables. |
| R-VAR-5 | Undefined variables are highlighted in the editor and reported before the request is sent. |
| R-VAR-7 | Collection variables and per-environment values are edited in one table: one row per variable, one column for the collection default and one per environment. An empty cell falls back to the collection default. |

#### Servers

A collection usually talks to a few services (e.g. "Users API", "Auth"), and each environment has its own address for each one.
Servers make that a first-class concept instead of a naming convention for variables.

| ID | Requirement |
|---|---|
| R-SRV-1 | A collection defines a list of named servers. Renaming a server does not break the requests that use it. |
| R-SRV-2 | Each environment sets a base URL for each server, edited in one table: one row per server, one column per environment. Base URLs are shared data (committed to git) and may contain `{{variables}}`. |
| R-SRV-3 | A request picks a server from a dropdown in front of the URL field; the URL field then holds only the path and query (e.g. `/users?page=2`). "No server" means the URL field holds a full URL. |
| R-SRV-4 | When sending, the request goes to the selected server's base URL in the active environment plus the path. If the server has no URL in that environment, or no longer exists, sending stops with a message naming the server and environment. |
| R-SRV-5 | The active environment is chosen per collection from the title bar, and remembered locally (§6.7). |
| R-SRV-6 | In the environments dialog, environments (columns) and servers (rows) can be dragged into a new order, which is saved. Variables stay sorted by name (D23). *(IDEA-13)* |

### Collaboration through git

#### On-disk format

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

#### Working with git

| ID | Requirement |
|---|---|
| R-GIT-10 | The app watches open collections. Changes on disk (git pull, branch switch, edits in another editor) are reloaded automatically. |
| R-GIT-11 | If a file changes on disk while it has unsaved edits in the app, the user chooses: keep mine or take theirs. |
| R-GIT-12 | **Git status is shown in the app.** For a collection inside a git repo, show the current branch and mark each request/folder as modified, new, deleted, or conflicted. |
| R-GIT-13 | Commit, pull, push, and branch switching happen outside the app. The app can open the collection folder in the system file manager or terminal. |
| R-GIT-14 | Git status works without a `git` executable installed: it is read from the repository by the app itself. Desktop only. |

### Local (non-shared) data

| ID | Requirement |
|---|---|
| R-LOC-1 | Secret values, cookies, the selected environment, and UI state are stored in the user's app-data directory, keyed by collection path. Nothing is written inside the repo, so there is no `.gitignore` to forget. |

### Scripts and tests

| ID | Requirement |
|---|---|
| R-TEST-1 | **Test scripts**: a request can have a JavaScript test script that runs after the response arrives. `test("name", () => …)` records a named check; `expect(value)` offers `toBe`, `toEqual`, `toBeTruthy`, `toBeFalsy`, `toBeDefined`, `toBeUndefined`, `toBeNull`, `toContain`, `toMatch`, `toHaveLength`, `toHaveProperty`, `toBeGreaterThan(OrEqual)`, `toBeLessThan(OrEqual)`, each also with `.not` (D28). The results (passed, or failed with the reason) and `console.log` output are shown in a Tests tab next to the response. The script is saved in the request file. *(IDEA-23, D3)* |
| R-TEST-2 | **Pre-request scripts** run before variables are filled in and the request is sent. They can read and set variables and set request headers (e.g. a timestamp or a signature). An error stops the send with its message. *(IDEA-23, D25)* |
| R-TEST-3 | **Script API and sandbox**: `res` (`status`, `statusText`, `headers`, `body`, `json()`, `header(name)`, `time`, `size`), `req` (`method`, `url`, `headers`, `body`; `setHeader(name, value)` in pre-request scripts), `vars` (`get`, `set`, `unset`, `has`) and `console.log`. Scripts run in Boa (D24), off the UI thread, with no access to files or the network, and are stopped after too many loop iterations or too deep a recursion. |
| R-TEST-4 | **Captures**: a request, or a workflow step, can put values from its response into variables: a JSONPath expression on the JSON body (R-HTTP-13), `header:<name>`, or `status`. Edited as a table next to the test script; saved in the request file. |
| R-TEST-5 | **Runtime variables**: values captured or set by scripts are kept per project until the app closes, and never written to disk (D26). They win over every other value (secret, environment, folder, project), count as defined (R-VAR-5), and are listed with their values and can be cleared. |

### Workflows

| ID | Requirement |
|---|---|
| R-FLOW-1 | A **workflow** is a TOML file in the project's `workflows/` folder (D4, D29): a name and an ordered list of steps; each step names a request file, and may add captures and a test script of its own. Requests are referenced, not copied: changing a request changes every workflow that uses it. *(IDEA-26)* |
| R-FLOW-2 | Workflows are listed in the sidebar under their project; they are created, renamed, deleted and edited in the app: add a step by picking a request, reorder, remove, edit its captures and tests. |
| R-FLOW-3 | **Running a workflow** sends its steps in order with the active environment, each with its pre-request script, captures and tests. Captured values flow to later steps; a run starts with no runtime variables of its own and does not change the project's. It stops at the first failed step (a failed test, an error, or a 4xx/5xx response) unless the workflow says to continue. A result view shows each step: status, time, tests passed and failed, captured values, and its response. A run can be cancelled. |
| R-FLOW-4 | **Data-driven runs**: a workflow can run once per row of a CSV file (first row: variable names) or a JSON file (an array of objects); each row's values are variables for that iteration (D27). The data file is chosen when running, or named in the workflow file (relative to the project). Results are shown per iteration. |
| R-FLOW-5 | Unsaved workflow edits are kept when Tortuga closes, like request edits (R-COL-8), as local data. *(IDEA-42)* |

### Command-line runner

| ID | Requirement |
|---|---|
| R-CLI-1 | **`tortuga-cli`**, a separate binary without GUI libraries (D5), runs a workflow, a folder (its requests in order) or a single request of a project: `tortuga-cli run <project> <workflow, folder or request> [--env NAME] [--data FILE] [--var name=value]… [--insecure] [--cacert FILE]…`. It prints each step with its tests and exits with 0 when everything passed, 1 when a test or step failed, and 2 for usage or setup errors. *(IDEA-24)* |
| R-CLI-2 | Variables and secrets come from `--var name=value` and from environment variables `TORTUGA_VAR_<name>` (D30), so CI can pass secrets; they win over the project's values. |
| R-CLI-3 | Reports: `--report junit --out FILE` (JUnit XML: a suite per workflow iteration, a test case per test) and `--report json --out FILE`. |

### Settings

| ID | Requirement |
|---|---|
| R-SET-1 | **Settings that cascade** (D39): default timeout (30 s), follow redirects (on) and send and store cookies (on) can be set in the app's settings, for a workspace and for a project; the nearest level wins: request → project → workspace → app. Each level is edited where it lives and shows the inherited value until it is overridden. *(IDEA-50)* |
| R-SET-2 | **Workspace headers and auth** (D39): inherited by every project below its own: request → folder → project → workspace. *(IDEA-50)* |
| R-SET-3 | **TLS verification is only turned off on this machine** (D39): shared files (workspace file, `tortuga.toml`) cannot turn it off. The app's settings keep the global switch (R-HTTP-8) and a list of hosts to skip verification for (e.g. `localhost`, `*.dev.internal`), with the existing warning; requests to those hosts show that verification is off. *(IDEA-50)* |
| R-SET-4 | **Theme**: system (default), light or dark, in the app's settings. *(IDEA-50)* |
| R-SET-5 | **Text size**: zoom the whole UI, 100 % by default (also Ctrl+= / Ctrl+-, Ctrl+0 to reset). *(IDEA-50)* |
| R-SET-6 | **History size**: how many entries per project are kept, 200 by default, or off (nothing is recorded). Lowering it trims existing history. *(IDEA-50, R-HIST-1)* |
| R-SET-7 | **Wrap long lines** in the response body, off by default. *(IDEA-50)* |

### Non-functional

| ID | Requirement |
|---|---|
| R-NF-1 | Cold start under 1 s on a typical developer laptop. A collection with 1,000 requests opens in under 1 s. |
| R-NF-2 | Response bodies up to 50 MB are handled without freezing the UI. Larger bodies are truncated in the viewer and can be saved to a file. |
| R-NF-3 | Fully usable offline. |
