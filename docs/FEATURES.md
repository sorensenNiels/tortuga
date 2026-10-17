# Tortuga features

> Features that are implemented **and verified**. Each entry points to the requirement it satisfies
> in [ROADMAP.md §6](ROADMAP.md#6-requirements).
>
> Add an entry only when the requirement is fully met, has automated tests where practical, and has
> been checked in the running app on at least one OS. Note which OSes were checked.

**Verified on:** Linux = Ubuntu 24.04 under WSL2/WSLg (desktop build). Windows and macOS have not been run yet.

**UI rewrite (2026-10-16):** the UI moved from Dioxus to egui (`tortuga-ui-egui`). The engine and file formats did
not change. Re-checked in the running egui app (Linux, X11 via WSLg): sending, URL/params sync, pretty JSON,
status colours, headers tab, "Save to file…" (byte-identical), cancel, settings + TLS banner (saved to disk),
collection tree, selection, new folder (starts in rename mode), inline rename, duplicate, move up, move to,
delete with confirmation, unsaved marker + Ctrl+S + Ctrl+Enter, broken-file error, environments dialog (files on
disk), environment picker, undefined-variable highlight and send block, 5 MB body truncation, collections reopened
after a restart. Not yet re-checked in the egui app: "Open…"/create collection, the body editor types, and sending
through a server base URL.

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
| R-COL-4 | Collection defaults for variables (headers and auth defaults come in M4). | Linux | As R-VAR-2. |

## In progress

| Requirement | State |
|---|---|
| R-P-4 | No telemetry or analytics code exists, and the app only sends requests the user makes. Not yet confirmed with a network monitor. |
| R-HTTP-8 | Proxy settings from the Windows and macOS system settings are expected to work (handled by the HTTP library), but have not been tested on those OSes. |
| R-NF-2 | Partly done: bodies over 2 MB are not pretty-printed and are truncated in the viewer, with the full body available through "Save to file…". Not yet measured with 50 MB responses. |

## Known limitations

- Unsaved edits are kept only while the app is open; closing the app does not warn about them yet.
- Files changed outside the app (e.g. `git pull`) appear after "Reload from disk" in the collection menu. Automatic reloading is planned (M6).
- Windows and macOS builds are checked by CI but have not been run by a person yet.
- Form URL-encoded bodies are covered by automated tests; in the app only switching to the form editor was checked, not sending a form.
