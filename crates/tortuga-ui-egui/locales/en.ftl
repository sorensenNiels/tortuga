# Tortuga's UI in English: the source text (R-I18N-2, D64).
# Every message here must also be in da.ftl, with the same variables.
# Literal braces are written {"{{"} and {"}}"}.

## common
close = Close
cancel = Cancel
send = Send
save = Save
remove = Remove
value = Value
name = Name
and-separator = {" and "}
edit = Edit
done = Done
choose = Choose…
clear = Clear
revert = Revert
open = Open
save-shortcut = Save (Ctrl+S)
unsaved = Unsaved
move-up = Move up
move-down = Move down
variable = Variable
tests = Tests
console = Console
no-body = No body.
passed = Passed
failed = Failed
cancelled = Cancelled
running = Running…
rename-ellipsis = Rename…
delete-ellipsis = Delete…
unsaved-changes = Unsaved changes
git-branch = Git branch: {$branch}
none = None
project = Project
import = Import
delete = Delete
sending = Sending…
headers = Headers
rename = Rename
create = Create
history = History
settings = Settings
error = Error
cut = Cut
copy = Copy
paste = Paste
folder = Folder
reset = Reset
status = Status
add-variable = Add variable (press Enter)…
not-set = not set
value-hint = value
none-yet = None yet.
header = Header
edit-project-defaults = Edit project defaults…
edit-workspace-defaults = Edit workspace defaults…
parameter = Parameter
environment = Environment
not-available-here = Not available here.
add-server = Add server (press Enter)…
servers = Servers
variables = Variables
base-url = Base URL
default = Default
on = On
off = Off

## dates

date-today = Today
date-yesterday = Yesterday
date-this-year = {$weekday} {$day} {$month}
date-other-year = {$day} {$month} {$year}
weekday-mon = Mon
weekday-tue = Tue
weekday-wed = Wed
weekday-thu = Thu
weekday-fri = Fri
weekday-sat = Sat
weekday-sun = Sun
month-jan = Jan
month-feb = Feb
month-mar = Mar
month-apr = Apr
month-may = May
month-jun = Jun
month-jul = Jul
month-aug = Aug
month-sep = Sep
month-oct = Oct
month-nov = Nov
month-dec = Dec

## components/variable_row

variable-name-hint = name
variable-remove = Remove this variable

## shell/drafts

drafts-not-kept = Unsaved edits could not be kept for next time: {$error}
drafts-not-read = Could not read the unsaved edits of last time: {$error}

## shell/toast

toast-dismiss = Click to dismiss

## variables

variable-secret-hidden = secret, hidden
var-from = From {$from}
var-ctrl-click-open = Ctrl+click to open where it is defined
var-not-defined = Not defined
var-ctrl-click-define = Ctrl+click to define it in the project's environments
var-source-runtime = a capture or script, kept while the app runs
var-source-workspace-runtime = a capture or script for the workspace, kept while the app runs
var-source-secret = this computer's secret value
var-source-environment = the environment “{$name}”
var-source-workspace-environment = the workspace's environment “{$name}”
var-source-folder = the folder “{$name}”
var-source-project = the project
var-source-workspace = the workspace
var-source-dynamic = a dynamic variable: a new value every time
dynamic-uuid = a random UUID
dynamic-timestamp = the time in seconds since 1970
dynamic-iso-timestamp = the time in ISO 8601, UTC
dynamic-random-int = a random whole number from {$min} up to {$max}
workspace-vars-title = Workspace variables — {$name}
workspace-vars-captured = Captured this session
workspace-vars-clear-captured = Clear captured
workspace-vars-clear-captured-hint = Forget the captured values
workspace-vars-prefix = Leave out “{$prefix}”: “{$name}” is used as {"{{"}{$rest}{"}}"}.

## mocks

mock-no-port = no port to try
mock-project-not-open = The project is not open.

## shell/appearance

code-font-fallback = The code font can't be used, so the bundled one is: {$error}
code-font-no-files = fonts can't be read from files here

## shell/response

saved-to = Saved to {$path}
could-not-save = Could not save: {$error}
changes-previous-response = The previous response → this one

## shell/sending

protected-send-title = Send {$method} to “{$env}”?
protected-send-message = “{$env}” is a protected environment: this request may change its data.

## shell/updates

update-available = Tortuga {$version} is available
update-available-hint = Opens its release page, with what's new and the installers

## components/shortcuts_dialog

shortcuts-title = Keyboard shortcuts

## shell/mock

mock-started = The mock server of “{$name}” runs on port {$port}.
mock-start-failed = Could not start the mock server: {$error}
mock-stopped = The mock server of “{$name}” stopped.

## shell/dialogs

choose-ca-certificate = Choose a CA certificate (PEM)
choose-code-font = Choose a font for code (.ttf, .otf)
dynamic-variable-info = {"{{"}{$name}{"}}"} gets a new value every time it is sent.
scratch-no-variables = The scratch request has no project: save it in one to define its variables.
code-font-unusable = The code font can't be used: {$error}

## shell/layout

divider-show-request = Drag to change the request's height. Double-click to show the request again.
divider-response-full = Drag to change the request's height. Double-click to give the response all of it.
divider-widths = Drag to change the widths. Double-click for equal widths.
divider-name = Request and response divider

## components/response_panel/filter

filter-help-json = JSONPath: $.name, [0], [-1], [1:3], [*], ..name, [?(@.price < 10)]
filter-help-xml = XPath: /a/b, //name, *, @id, text(), .., [1], [last()], [@id='7'], [price < 10]. A name without a prefix matches any prefix.
filter-not-json = The body is not valid JSON.
filter-not-xml = The body is not valid XML.
filter-only-json-xml = Only JSON and XML bodies can be filtered.

## components/response_panel/json_menu

json-menu-reading = Reading the JSON…
json-menu-right-click = Right-click a line with a value.
json-menu-add-test = Add test
json-menu-capture-into = Capture into “{$name}”
json-menu-capture-hint = Adds the capture {$name} = {$path}
json-menu-copy-path = Copy path

## components/response_panel/trend

trend-last-sends = Last {$count} sends
trend-range = {": "}{$fastest} to {$slowest}
trend-failed = {", "}{$count} failed
trend-no-response = no response

## shell/disk

conflict-changed = This request changed on disk (for example after git pull) while you had unsaved edits.
conflict-keep-mine = Keep mine
conflict-take-theirs = Take theirs
conflict-deleted = This request was deleted on disk while you had unsaved edits.
conflict-restore = Restore with my edits
conflict-discard = Discard my edits

## shell/fanout

fanout-save-first = Save the request in a project to send it to its environments.
fanout-no-environments = This project has no environments to send to.
fanout-confirm-title = Send {$method} to every environment?
fanout-confirm-message = That includes the protected {$environments}: this request may change their data.

## components/key_value_table

kv-include = Include
kv-add = Add {$name}

## shell/tabs

scratch-request = Scratch request
close-project-tabs-title = Close the tabs of “{$name}”?
close-project-tabs-message =
    { $count ->
        [one] 1 of its requests has unsaved edits.
       *[other] {$count} of its requests have unsaved edits.
    } They are not lost: opening the request again shows them, until you save or revert.
close-project-tabs = Close tabs

## components/description

description-draft = Draft from request
description-draft-hint = Start from what the request sends and its examples, then fill in the rest
description-none = No description.
description-none-yet-draft = No description yet. Click Edit to write one in Markdown, or Draft from request to start from what it sends: it is saved in the file and shared through git.
description-none-yet = No description yet. Click Edit to write one in Markdown: it is saved in the file and shared through git.
description-markdown-hint = Markdown: # heading, **bold**, *italic*, - list, `code`, ```code block```, [link](https://…)

## components/environments/draft

env-needs-name = Every environment needs a name.
env-duplicate = There are two environments called “{$name}”.
env-variable-braces = Variable names cannot contain braces: “{$name}”.
env-variable-duplicate = There are two variables called “{$name}”.
env-server-twice = “{$env}” names the server “{$server}” twice.
env-variable-duplicate-in = “{$env}” has two variables called “{$name}”.

## components/palette

palette-command-hint = Type a command
palette-search-hint = Search requests by name, folder, project or URL. Type > for commands
palette-no-command = No command matches.
palette-no-request = No request matches.
palette-more = {$count} more: type to narrow the list.

## components/sidebar/problems

project-missing = missing
project-locate = Locate…
project-locate-hint = Pick the folder where the project is now
project-copy-clone-url = Copy clone URL
project-remove-from-workspace = Remove from workspace
project-clone-hint = Clone it from {$remote}, then use Locate…
broken-file-delete = Delete this file
broken-file-error = Could not read {$path}: {$error}. Fix the file in an editor, then use “Reload from disk”.

## workflows

workflow = Workflow
workflow-continue-on-failure = Continue after a failed step
workflow-data-file = Data file
workflow-data-none = none: one run
workflow-data-hint = A CSV or JSON file, relative to the project: one run per row, its values as variables
workflow-steps = Steps
workflow-steps-empty = Add the requests to run, in order. Values captured by a step can be used by the next ones as {"{{"}variables{"}}"}.
workflow-name = Workflow name
workflow-run = Run
workflow-run-hint = Run the steps in order with the active environment (unsaved changes included)
workflow-step-missing = ⚠ {$request} is not in the project
workflow-captures =
    { $count ->
        [one] 1 capture
       *[other] {$count} captures
    }
workflow-step-has-tests = tests
workflow-step-remove = Remove this step
workflow-step-open = Go to the request
workflow-step-checks = Checks…
workflow-step-checks-hint = Captures and tests for this step, on top of the request's own
workflow-step-name = Step name
captures = Captures
workflow-captures-hint = $.json.path, xpath://xml/path, header:Name or status; available as {"{{"}variable{"}}"} in later steps.
workflow-step-tests-hint = Runs after the request's own tests.
workflow-add-step = + Add step…
workflow-no-requests = This project has no requests yet.
workflow-add-step-hint = Add a request of this project as the next step
workflow-not-run = Not run yet
workflow-not-run-hint = Run the workflow to see each step's result here.
workflow-row = Row {$row}
workflow-skipped =
    { $count ->
        [one] 1 later step skipped after the failure.
       *[other] {$count} later steps skipped after the failure.
    }
workflow-running-row-step = Row {$row}, step {$step} of {$of} …
workflow-running-step = Step {$step} of {$of} …
workflow-tests-passed =
    { $count ->
        [one] 1 test passed
       *[other] {$count} tests passed
    }
workflow-tests-failed = {$count} failed
workflow-steps-failed =
    { $count ->
        [one] 1 step failed
       *[other] {$count} steps failed
    }
workflow-rows-done = {$done} of {$planned} rows
workflow-step-tests = tests {$passed}/{$total}
response-body = Response body
workflow-new-title = New workflow
workflow-rename-title = Rename workflow
workflow-delete-title = Delete workflow?
workflow-delete-message = “{$name}” will be deleted from disk. Its requests stay.
workflow-deleted-drafts-dropped = The workflow “{$name}” was deleted, so its unsaved edits were dropped.
workflow-choose-data = Choose a CSV or JSON data file
workflow-protected-title = Run “{$name}” against “{$env}”?
workflow-protected-message = “{$env}” is a protected environment, and these steps may change its data: {$steps}.
workflow-data-unreadable = Could not read the data file {$file}: {$error}

## components/sidebar

sidebar-projects = Projects
sidebar-add = + Add
sidebar-add-hint = Add a project to this workspace
sidebar-empty = Add a project to start. Projects are plain files you can commit to git, next to your code.
scratch-request-hint = A request that is not saved anywhere, for quick one-off calls
sidebar-mock-port = mock :{$port}
read-only = read-only
sidebar-workflows = Workflows
sidebar-workflows-hint = Requests run one after the other, with checks (R-FLOW-1)
sidebar-workflow-steps =
    { $count ->
        [one] 1 step
       *[other] {$count} steps
    }
sidebar-repository-hint =
    Git repository {$path}
    { $count ->
        [one] 1 project in this workspace
       *[other] {$count} projects in this workspace
    }
sidebar-deleted-in-git = Deleted since the last commit. Commit the deletion, or restore the file with git.
sidebar-conflict-hint = Changed on disk while you had unsaved edits: open it to choose
git-modified = Git: modified since the last commit
git-new = Git: new, not committed yet
git-deleted = Git: deleted
git-conflicted = Git: merge conflict — fix it, then commit

## import

import-postman-environments = Import Postman environments…
import-http-environments = Import HTTP environments…
import-bruno-environments = Import Bruno environments…
import-environments = Import environments…
import-kind-openapi = OpenAPI description
import-kind-postman = Postman collection
import-kind-http = HTTP file
import-kind-bruno = Bruno collection
import-kind-insomnia = Insomnia export
import-environments-postman-hint = Postman environments exported as JSON become this project's environments
import-environments-http-hint = http-client.env.json, http-client.private.env.json or a VS Code settings file with REST Client environments: they become this project's environments
import-environments-bruno-hint = Bruno environments (.bru files) from another collection become this project's environments too
import-from-bruno-folder = From the folder {$files} (Bruno collection). Tortuga creates a new project from it; the collection itself is not changed.
import-from-file = From {$files} ({$kind}). Tortuga creates a new project from it; the file itself is not changed.
import-from-files = From {$files}. Tortuga creates a new project from them, with a folder per file; the files themselves are not changed.
import-title-openapi = Import OpenAPI
import-title-postman = Import Postman collection
import-title-http = Import HTTP file
import-title-bruno = Import Bruno collection
import-title-insomnia = Import Insomnia export
import-add-environment-files = Add environment files…
import-project-folder = Project folder
import-folder-taken = That folder already is a Tortuga project: choose another one.
import-inside-repository = Inside the git repository at {$path}: commit the project with your code.
import-not-as-written = Not imported as written ({$count})
import-requests =
    { $count ->
        [one] 1 request
       *[other] {$count} requests
    }
import-requests-in-folders =
    { $count ->
        [one] 1 request
       *[other] {$count} requests
    } in { $folders ->
        [one] 1 folder
       *[other] {$folders} folders
    }
import-row-requests = Requests
import-row-environments = Environments
import-row-environment-files = Environment files
import-row-secret-variables = Secret variables
import-secrets-fill-in = Secret values stay on this computer: fill them in under Environments… after the import.
import-secrets-found = Values for these were found in the file: they are kept on this computer, not in the project files.
import-secrets-some-found = Values for {$found} of these were found in the file: they are kept on this computer, not in the project files. Fill in the other {$missing} under Environments… after the import.
import-environments-from = From {$files}. Base URLs go to the project's servers; secret values stay on this computer.
import-row-adds = Adds
import-row-updates = Updates
import-row-secret-values = Secret values
import-environments-updated-keep = Environments that are updated keep what the files don't set.
import-environments-secrets-found = These were found in the files: they are kept on this computer, not in the project files.
import-pick-http-environments = Choose HTTP Client environment files, or VS Code settings with REST Client environments
import-pick-postman-environments = Choose Postman environments (exported as JSON)
import-pick-bruno-environments = Choose Bruno environments (the .bru or .yml files in a collection's environments folder)
import-failed-file = Could not import {$file}: {$error}
import-turned-off = it is a {$kind}, and importing {$importer} is turned off in Settings
import-pick-openapi = Choose an OpenAPI description (JSON or YAML)
import-pick-postman = Choose a Postman collection (exported as JSON)
import-pick-http = Choose .http files (REST Client or JetBrains HTTP Client), and their environment files
import-pick-insomnia = Choose an Insomnia export (v4 JSON or v5 YAML)
import-pick-bruno = Choose a Bruno collection (the folder with bruno.json or opencollection.yml)
import-pick-project-folder = Choose or create a folder for the project
import-failed = Could not import: {$error}
import-done = Imported “{$name}”.
import-secrets-not-kept = Could not keep the secret values: {$error}
importer-openapi = OpenAPI descriptions
importer-postman = Postman collections
importer-http = .http files
importer-bruno = Bruno collections
importer-insomnia = Insomnia exports

## commands

cmd-add-projects = Add existing projects…
cmd-new-project = New project…
cmd-import-openapi = Import OpenAPI…
cmd-import-postman = Import Postman collection…
cmd-import-insomnia = Import Insomnia export…
cmd-import-http = Import .http file…
cmd-import-bruno = Import Bruno collection…
cmd-workspace-environments = Workspace environments…
cmd-workspace-variables = Workspace variables…
cmd-workspace-defaults = Workspace defaults…
cmd-scratch-detail = Not saved anywhere
cmd-environment = Environment: {$name}
cmd-environment-mock = Environment: Mock
cmd-current = current
cmd-mock-server = Mock server…
cmd-stop-mock = Stop mock server
cmd-start-mock = Start mock server
cmd-run-workflow = Run workflow: {$name}
cmd-new-workflow = New workflow…
cmd-environments = Environments, servers and variables…
cmd-project-defaults = Project defaults…
cmd-send = Send request
cmd-send-everywhere = Send to every environment
cmd-quick-open = Quick open: go to a request
cmd-revert = Revert changes
copy-as-curl = Copy as curl
cmd-new-request = New request
cmd-workspace = Workspace: {$name}
cmd-runtime-variables = Runtime variables…
cmd-settings = Settings…
cmd-check-updates = Check for updates
cmd-show-sidebar = Show the sidebar
cmd-hide-sidebar = Hide the sidebar
cmd-show-history = Show history

## components/fanout_dialog

fanout-title = Every environment
fanout-baseline = compared with this one
fanout-same-shape = same shape

## shell/projects

new-request-name = New request
new-folder-name = New folder
delete-folder-message = “{$name}” and everything in it will be deleted from disk.
delete-message = “{$name}” will be deleted from disk.
delete-title = Delete?
new-project-pick-folder = Choose or create a folder for the new project
add-projects-pick-folder = Add the projects in a folder or repository
locate-project = Locate “{$name}”
not-a-project = {$path} is not a Tortuga project (it has no tortuga.toml).

## keys

key-up = Up
key-down = Down
key-left = Left
key-right = Right
or-separator = {" or "}
command-toggle-comment = Comment lines out or in
command-block-comment = Block comment
command-move-lines-up = Move lines up
command-move-lines-down = Move lines down
command-copy-lines-up = Copy lines up
command-copy-lines-down = Copy lines down
command-delete-lines = Delete lines
command-indent-lines = Indent lines
command-outdent-lines = Outdent lines

## components/changes_dialog

changes-title = What changed
changes-nothing = Nothing changed: the same status, headers and body.
changes-status = Status {$before} → {$after}
changes-time-size = Time {$time_before} → {$time_after} · Size {$size_before} → {$size_after}
changes-headers = Headers ({$count})
changes-body-same = Body: the same
changes-body = Body ({$count})
changes-more = … and {$count} more.
body = Body
changes-lines-differ =
    { $count ->
        [one] 1 line differs
       *[other] {$count} lines differ
    }, the first at line {$first}.

## components/request_preview

preview = Preview
preview-hide = Hide the request as it will be sent (Esc)
preview-show = Show the request as it will be sent: variables filled in, secrets masked
preview-no-headers = No headers.
preview-form-file = {$name}: file {$file}{$content_type}
preview-body-cut = The body is longer; only the start is shown.

## shell/request_head

untitled-request = Untitled request
save-to-project = Save to project…
request-name = Request name
request-unsaved-hint = This request has changes that are not saved
curl-copy-failed = Could not copy as curl: {$error}
curl-read-failed = Could not read the curl command: {$error}
curl-filled-in = Filled in from the curl command. Not used: {$ignored}.
copied = Copied
copy-as-curl-hint = Copy this request as a curl command, with variables filled in

## response

image = Image
image-too-large = Too large to show here. Use “Save to file” to keep it.
image-scale = {$width} × {$height} px, shown at {$percent}%
cookies-none-set = This response set no cookies.
cookies-unreadable = Could not read Set-Cookie: {$value}
cookies-stored-in-jar = Stored in the project's cookie jar unless their domain or path does not match the request.
cookies-open-jar = Open the jar…
cookies-domain-path = Domain and path
cookies-expires = Expires
cookies-sent-to-domain = Sent to {$domain} and its subdomains{$under}
cookies-this-host = This host
cookies-sent-to-host = Sent only to the host that answered{$under}
cookies-under-path = {", "}for paths under {$path}
cookies-under-request-folder = {", "}for paths under the request's folder
cookies-session = Session
cookies-removes-it = Removes it
response-not-sent = Not sent.
response-same-as-before = Same as before
response-changes =
    { $count ->
        [one] 1 change
       *[other] {$count} changes
    }
response-changes-hint = What changed since the previous response
response-cookies = Cookies
response-sending = Sending request…
response-none-yet = No response yet
response-none-yet-hint = Send a request to see the response here.
response-keys-hint = Ctrl+Enter sends, Ctrl+S saves
response-cancelled = Request cancelled.
response-failed = Could not get a response
response = Response
response-size = Size
response-save-to-file = Save to file…
response-save-as-example = Save as example
response-save-as-example-hint = Keep this response as an example the mock server answers with
response-pretty = Pretty
response-binary = Binary content. Use “Save to file” to keep it, or “Raw” to see its bytes.
response-filtering = Filtering…
response-truncated = Showing the first {$limit}. Use “Save to file” for the full body.
response-pretty-hint = Show the body formatted
response-raw = Raw
response-raw-hint = Show the body as it came: unformatted text, or the bytes as hex
response-no-tests = No tests ran. Add some with test("name", () => expect(res.status).toBe(200)) in the Scripts tab.
response-variables-set = Variables set
response-variable-removed = (removed)
response-variables-kept = Kept for this project until Tortuga closes; they win over environment values.
response-filter = Filter
response-wrap = Wrap
response-wrap-hint = Wrap long lines (also in Settings → Appearance)
response-next-match = Next match (Enter)
response-previous-match = Previous match (Shift+Enter)
response-no-matches = No matches
response-match-of = {$current} of {$total}
response-search = Search (Ctrl+F)
response-search-hint = Search in the body (Ctrl+F). Enter: next match, Shift+Enter: previous.

## history

history-no-response-to-compare = No response to compare
history-compare-before = Compare with the send before
history-compare-selected = Compare with the selected one
history-select-for-compare = Select for comparing
history-filter = Filter history
history-filter-hint = Search by name, URL, status, project or environment
history-clear-one = Clear {$name}
history-clear-all = Clear all history
history-clear-hint = Remove history kept on this computer
history-empty = Requests you send appear here. History stays on this computer.
history-no-match = Nothing in the history matches.
history-older = Older entries: use the filter to find them.
history-read-failed = Could not read the history: {$error}
history-off = History is off: new sends are not recorded (Settings → History).
history-first-send = This is the request's first send with a response in history.
history-selected-for-compare = Selected: now right-click another send and compare it.
history-clear-title = Clear history?
history-clear-message = The history of {$name} will be removed from this computer.
history-clear-all-title = Clear all history?
history-clear-all-message = All request history will be removed from this computer.
history-open-failed = Could not open the history entry: {$error}
history-compare-needs-response = Only sends that got a response can be compared.
history-body-cut = {" · "}body cut to 1 MB
history-past-label = From history: sent {$sent}
history-back-to-latest = Back to latest
history-back-to-latest-hint = Show the latest response of this request
history-restore = Restore request
history-restore-hint = Put this version of the request in the editor, as unsaved changes
history-cleared = History cleared.
history-clear-failed = Could not clear the history: {$error}

## shell

tls-off-warning = TLS certificate verification is off. Responses could come from an impostor server.
tls-off-change = Change in settings
dismiss = Dismiss
sidebar-show-hint = Show the sidebar (Ctrl+B)
sidebar-hide-hint = Hide the sidebar (Ctrl+B)
go-to-request = Go to request…
go-to-request-hint = Quick open (Ctrl+P). Ctrl+K lists every command.
choose-upload-file = Choose a file to upload

## scripts

script-snippets = Snippets
script-rerun = Re-run tests
script-rerun-hint = Run the captures and tests again on the last response, without sending
script-rerun-disabled = Send the request first: tests run on its response
script-beautify = Beautify
script-beautify-hint = One statement per line, indented; comments and strings are kept
script-test-failed = failed
script-and-more = {" "}(and {$count} more)
script-passed = {$count} passed
script-hits =
    { $count ->
        [one] 1 hit
       *[other] {$count} hits
    }
script-gutter-hint = Click a line number to set or clear a breakpoint
console-pre-request = Pre-request script
console-step-tests = Step tests
console-line = line {$line}
console-breakpoint =
    ⏺ Breakpoint, line {$line} — { $count ->
        [one] 1 variable
       *[other] {$count} variables
    }
console-no-variables = No variables of the script are in scope here.
console-array =
    {"[…]"} { $count ->
        [one] 1 item
       *[other] {$count} items
    }
console-object =
    {"{"}…{"}"} { $count ->
        [one] 1 key
       *[other] {$count} keys
    }
snippet-set-variable = Set a variable
snippet-set-header = Set a header
snippet-timestamp = Timestamp in a variable
snippet-random = Random number in a variable
snippet-log-request = Log the request
snippet-status-200 = Status is 200
snippet-status-2xx = Status is 2xx
snippet-response-time = Response time under 500 ms
snippet-body-json = Body is JSON
snippet-body-property = Body has a property
snippet-save-field = Save a field to a variable
snippet-log-body = Log the body
snippet-save-cookie = Save a cookie to a variable
snippet-workflow-repeat = Workflow: repeat a step until done
snippet-workflow-stop = Workflow: stop the run
snippet-read-xml = Read an XML body
scripts-pre-request = Pre-request
scripts-pre-request-about = Runs before variables are filled in and the request is sent.
scripts-tests-about = Runs after the response arrives.
scripts-pre-request-reference = vars.get/set/unset/has/replaceIn, req.method/url/headers/body, req.setHeader(name, value), xml2Json(text), info.requestName/iteration, workflow.next(step)/stop() in workflow runs, console.log/info/warn/error. Click a line number to set a breakpoint: the Console then shows the variables there.
scripts-tests-reference = test(name, fn); expect(x).toBe/toEqual/toContain/toMatch/toHaveProperty/… (and .not) or Chai's expect(x).to.equal/.to.be.an("object")/.to.have.property/…; res.status, res.json(), res.header(name), res.cookie(name), res.time, res.body; xml2Json(text); vars; info; workflow.next(step)/stop() in workflow runs; console.log. Click a line number to set a breakpoint: the Console then shows the variables there.
scripts-reference = Reference
scripts-captures-hint = Puts a value from the response into a variable: $.json.path, xpath://xml/path, header:Name or status. Captured values are kept until Tortuga closes and win over environment values.

## tabs

expand = Expand
collapse = Collapse
tabs-show-project = Show this project's tabs
tabs-fold-project = Fold this project's tabs into the chip
tabs-close-project = Close the project's tabs
tabs-close-hint = Close (unsaved edits are kept)
tabs-preview-hint = Preview: the next request you open replaces it. Double-click to keep it open.
tabs-keep-open = Keep open
tabs-close-others = Close others
tabs-close-right = Close to the right
tabs-close-saved = Close saved tabs
tabs-close-saved-hint = Close every tab without unsaved edits
tabs-all = All open tabs

## state

files-unreadable-here = files can't be read here
ca-certificate-problem = CA certificate {$path}: {$error}
history-trim-failed = Could not trim the history: {$error}
project-reload-failed = Could not reload the project: {$error}
secrets-read-failed = Could not read secret values: {$error}
workspace-file-read-failed = Could not read the workspace file: {$error}
workspace-file-save-failed = Could not save the workspace file {$file}: {$error}
watch-failed = Changes on disk won't show automatically (use “Reload from disk”): {$error}
git-status-failed = Could not read the git status: {$error}
cookies-save-failed = Could not save cookies: {$error}
cookies-read-failed = Could not read saved cookies: {$error}

## mock

mock-title = Mock server — {$project}
mock-received = Received
mock-running = Running
mock-stopped-badge = Stopped
mock-port = Port
mock-port-range = A port from 1 to 65535
mock-restart-on-port = Restart on this port
mock-use-port = Use this port
mock-stop = Stop
mock-send-here = Send requests here
mock-send-here-hint = Choose the “Mock” environment
mock-start = Start
mock-nothing-yet = Nothing yet.
mock-time = Time
mock-request = Request
mock-answered-by = Answered by

## settings

settings-close-pairs = Close brackets and quotes
settings-close-pairs-hint = Typing ( [ {"{"} or a quote puts in its partner, or wraps the selection in the pair; a closing one next to the cursor is stepped over.
settings-keys = Keys
settings-keys-hint = Click a key and press another to change it; Escape cancels. In a script field these keys win over the app's own. Ctrl is Cmd on macOS.
settings-keys-like = Keys like
settings-scheme-changed = {$scheme} (changed)
settings-reset-all = Reset all
settings-reset-all-hint = Back to this scheme's keys for every command
settings-scheme-replaces = Choosing a scheme replaces the keys you changed.
settings-press-key = Press a key…
settings-key-unknown = {$key} (unknown)
settings-key-change-hint = Click, then press the new key
settings-key-remove = Remove this key
settings-key-add = Add a key
settings-key-reset-hint = Back to the scheme's keys
settings-key-needs-modifier = {$key} needs Ctrl or Alt with it, or the key would no longer type.
settings-key-taken = {$key} is already “{$command}”.
settings-key-elsewhere = Outside script fields, {$key} still does “{$does}”.
settings-appearance = Appearance
settings-editing = Editing
settings-script-editor = Script editor
settings-features = Features
settings-importers = Importers
settings-about = About
settings-requests-tab-error = Requests tab: {$error}.
settings-verify-tls = Verify TLS certificates
settings-verify-tls-off = Turning this off lets anyone between you and the server read and change your traffic. Only use it for local test servers.
settings-default-timeout = Default timeout (ms)
settings-requests-hint = Workspaces and projects can set their own timeout, redirects and cookies (their “Defaults…”).
settings-insecure-hosts = Don't verify certificates of these hosts:
settings-insecure-hosts-hint = One per line; *.example.test covers its subdomains. Only for test servers with self-signed certificates. Kept on this computer: shared files can't turn verification off.
settings-insecure-hosts-warning = Traffic to these hosts could be read and changed by anyone in between.
settings-proxy = Proxy
settings-proxy-system = From the system
settings-proxy-manual = Manual
settings-proxy-system-hint = The operating system's proxy settings, or HTTP_PROXY, HTTPS_PROXY and NO_PROXY.
settings-proxy-none-hint = Requests go straight to the server, whatever the system's settings say.
settings-proxy-url = Proxy URL
settings-no-proxy = Don't use it for these hosts:
settings-no-proxy-hint = One per line; example.com and *.example.com both cover its subdomains, and 10.0.0.0/8 a network. For mitmproxy, Charles or Fiddler, or a company proxy. A user name or password in the URL is not supported. Kept on this computer.
settings-ca-certificates = CA certificates
settings-ca-remove = Stop trusting it
settings-add = Add…
settings-ca-hint = PEM files whose certificates are trusted besides the system's, e.g. your company's CA for internal APIs. Kept on this computer only.
settings-theme = Theme
settings-like-system = Like the system
settings-theme-light = Light
settings-theme-dark = Dark
settings-language = Language
settings-text-size = Text size
settings-layout-below = Below the request
settings-layout-beside = Beside it
settings-layout-automatic = Beside it when wide
settings-density = Density
settings-density-comfortable = Comfortable
settings-density-compact = Compact
settings-code-font = Code font
settings-code-size = Code size
settings-appearance-hint = Ctrl+= and Ctrl+- change the text size too; Ctrl+0 resets it. The code font and size are for bodies, scripts and URLs, relative to the text size.
settings-wrap-lines = Wrap long lines in responses
settings-code-font-bundled = JetBrains Mono (bundled)
settings-code-font-reset = Use the bundled one
settings-preview-tab = Open requests in a preview tab
settings-preview-tab-hint = A single click shows a request in the preview tab (its title in italics), which the next single click replaces; editing it or double-clicking keeps it. Off: every request you open gets a tab that stays.
settings-auto-save = Save automatically
settings-auto-save-hint = Edits to requests and workflows are saved a second after you stop typing, and when you open something else or close the app. Not while a file changed on disk under your edits (the banner asks first), and never the scratch request. Git is your undo.
settings-features-hint = Turn off what you don't use to keep the app simple. Nothing is deleted: a request that already uses a feature still shows it, and a mock server that runs can still be stopped.
feature-scripts = Scripts, tests and captures
feature-examples = Examples and the mock server
feature-workflows = Workflows
feature-docs = Descriptions
feature-scripts-hides = Off hides the Scripts tab.
feature-examples-hides = Off hides the Examples tab, “Save as example” and the mock server.
feature-workflows-hides = Off hides workflows in the sidebar and the palette.
feature-docs-hides = Off hides the Docs tab and the “Description…” items.
settings-history-on = Keep a history of sent requests
settings-history-size = Entries per project
settings-history-lower = Lowering it removes the oldest entries when you save.
settings-history-off = Nothing more is recorded. History that is already there stays until you clear it in the History view.
about-version = Version {$version}
about-built-for = Built for
about-project-files = Project files
about-project-format = format {$format}; newer ones open read-only
about-workspace-files = Workspace files
about-format = format {$format}
about-licence = Licence
about-source = Source
about-copy-version = Copy version information
updates-unsupported = Tortuga sends nothing on its own: no telemetry, no update checks. It only contacts the servers your requests go to.
updates = Updates
updates-check-at-start = Look for a new version when the app starts
updates-check-now = Check now
updates-checking = Checking…
updates-up-to-date = You have the latest version.
updates-hint = Asks GitHub for the latest published release; nothing about you or your projects is sent. Tortuga has no telemetry, and otherwise only contacts the servers your requests go to.
settings-importers-hint = Turn off the importers you don't use; they disappear from “+ Add” and the command palette. Projects you imported earlier are not affected.

## dialogs

destination-top-level = {$project} (top level)
move-title = Move “{$name}”
move-to-folder = To folder
move = Move
save-request-title = Save request
save-request-no-project = Add or create a project first (“+ Add” in the sidebar).
save-request-name-hint = e.g. List users
copy-to-project-title = Copy “{$name}” to project
copy-to-project-none = There is no project that can be changed to copy into.
drag-to-resize = Drag to resize
new-project-title = New project
new-project-hint = Tortuga will add a tortuga.toml file to {$path}; requests are saved next to it.
new-project-name = Project name
new-project-git-init = Make the folder a git repository
new-project-git-init-hint = So the project can be shared with git (git init)
add-projects-title = Add existing projects
add-projects-looking = Looking for projects in {$path}…
add-projects-none = No projects (folders with a tortuga.toml file) in {$path}.
add-projects-create-there = Create a project there…
add-projects-found = Found in {$path}. Git-ignored folders were skipped.
add-projects-already = {$path}  (already in the workspace)
add-projects-add =
    { $count ->
        [one] Add 1 project
       *[other] Add {$count} projects
    }
curl-secrets-question = This request uses secret values ({$names}). Copy it with them shown as <secret:name>, or include the real values?
curl-secrets-hint = Commands you paste into chats, tickets or shell history may be kept for a long time.
curl-copy-masked = Copy masked
curl-include-secrets = Include secret values
runtime-vars-title = Runtime variables — {$place}
clear-all = Clear all
cookies-title = Cookies — {$place}
cookies-jar-empty = The jar is empty.
cookies-remove = Remove this cookie
cookies-host-only = Sent to {$domain} only, for paths under {$path}
cookies-with-subdomains = Sent to {$domain} and its subdomains, for paths under {$path}

## environments

secret-help-project-secret = Secret: values stay on this machine. Click to share them through the project files.
secret-help-project-shared = Shared through the project files. Click to make it secret (values stay on this machine).
secret-help-workspace-secret = Secret: the value stays on this machine. Click to share it.
secret-help-workspace-shared = Shared with the workspace. Click to make it secret (the value stays on this machine).
env-title = Environments — {$name}
env-none-yet = None yet. Add one (for example “Development”): every project gets it.
env-add = + Add environment
env-servers-and-variables = Servers and variables
env-project-hint = The project's servers and variables, secret flags and defaults
env-default-name = Environment {$number}
env-remove = Remove environment
env-remove-hint = Removes it from the workspace and from every project
env-colour = Colour
env-protected = Protected
env-protected-hint = Asks before anything but GET, HEAD or OPTIONS is sent here, in every project
env-all-projects = All projects
env-all-projects-hint = Values every project uses unless it sets its own.
env-show-all = Show every project and the shared values
env-project-servers = {$name}: servers and variables
env-read-only = {$name} (read-only)
env-project-values-hint = Its own values win; an empty one uses the workspace's (greyed), then the default.
env-kept-in-file = the workspace file {$file}
env-kept-on-machine = this machine (“Save as file…” in the workspace menu shares it)
env-kept = The list and the values all projects share are kept in {$kept}; each project's values in its own files, shared through git. Secret (🔒) values stay on this machine.
drag-to-reorder = Drag to change the order
env-remove-server-everywhere = Remove this server from every environment
env-workspace-secret = workspace secret
env-default-hint = default
env-remove-server = Remove this server from the project, in every environment
env-remove-variable = Remove this variable from the project, in every environment
color-green = Green
color-teal = Teal
color-blue = Blue
color-purple = Purple
color-pink = Pink
color-red = Red
color-orange = Orange
color-yellow = Yellow
color-grey = Grey
env-server-name-hint = server name
env-remove-server-url = Remove this server's URL
env-shared-servers-hint = Each URL goes to every project's server of that name.
env-shared-servers-hint-missing = Each URL goes to every project's server of that name. Servers in the projects: {$servers}.

## examples

example-no-name = (no name)
example-add = + Add example
example-default-name = Example
example-earlier = Earlier: asked before the ones below
example-later = Later
example-remove = Remove this example
example-status-range = A status from 100 to 999
example-delay = Delay (ms)
example-delay-none = none
example-content-type-hint = Without a Content-Type, JSON, XML or text is worked out from the body.
example-variables-hint = {"{{"}variables{"}}"} are filled from the project's active environment.
example-answers-when = Answers when
example-conditions-hint = Every condition must hold. Without conditions this example answers any request that reaches it.
example-query-parameter = Query parameter
example-body-condition = Body ($.path or xpath:…)

## levels

level-project = the project
level-workspace = the workspace
level-folder = folder “{$name}”
level-app = the app's settings

## auth

auth-describe-none = no auth
auth-describe-basic = Basic auth
auth-describe-basic-as = Basic auth as {$username}
auth-describe-bearer = Bearer token
auth-describe-bearer-token = Bearer token {$token}
auth-describe-a-header = a header
auth-describe-header = the header {$name}
auth-describe-header-value = the header {$name}: {$value}
auth-inherit-project = Inherit from project
auth-inherit-from = Inherit from {$origin}
auth-uses-inherited = This request uses the auth of {$origin}: {$auth}.
auth-change-in-folder = Change it with “Folder defaults…” in that folder's menu.
auth-none-sent = This request sends no authentication.
auth-saved-in-request = request file
auth-kind-none = No auth
auth-kind-basic = Basic
auth-kind-bearer = Bearer token
auth-no-header-added = No Authorization header is added.
auth-username = Username
auth-password = Password
auth-token = Token
auth-plain-credential = This value is saved in the {$saved_in} and shared through git. Use a secret variable instead, e.g. {"{{"}token{"}}"} — mark it secret (🔒) in Environments to keep it on this machine.
auth-header-replaces = A {$name} header added by hand replaces this.
auth-header-needs-name = No header is added until it has a name.

## request

request-server-no-url = No URL for this server in the active environment
request-undefined-variables = Undefined: {$names} — define them in the project's environments or the workspace's variables.
request-unverified-host = ⚠ The certificate of {$host} is not verified (Settings → Requests). Use this for test servers only.
tab-params = Params
tab-auth = Auth
tab-options = Options
tab-scripts = Scripts
tab-examples = Examples
tab-docs = Docs
request-content-type-hint = Content-Type is set automatically from the body type unless you add it here.
request-send-hint = Send (Ctrl+Enter). Right-click to send to every environment.
request-path-variable = Path variable
request-path-variable-rename = Change the name in the URL
request-path-variables-hint = Path variables come from :name segments in the URL. An empty value stops the send.
request-inherited-headers-hint = Add a header with the same name above to replace one, or add it switched off to leave it out.
request-method = HTTP method
request-params-offer =
    {$method} usually sends data in its body. Move { $count ->
        [one] the query parameter
       *[other] the {$count} query parameters
    } into it as
request-keep-in-url = Keep in URL
request-no-server = No server
request-server-missing = {$id} (missing)
request-url-hint = Enter request URL
request-timeout = Timeout (ms)
request-from-level = {$value} (from {$level})
request-timeout-hint = Leave empty to use the project's, the workspace's or the app's setting.
request-jar-project = this project's cookie jar
request-jar-scratch = the scratch cookie jar (kept until the app closes)
request-use-cookies = Use cookies from {$jar}
request-cookies-off = Cookies are turned off in {$level}, so this request sends and stores none.
request-cookies-hint =
    { $count ->
        [one] 1 cookie stored
       *[other] {$count} cookies stored
    }. When off, no cookies from the jar are sent and cookies in the response are not stored. A Cookie header you add yourself is always sent.

## body

body-text = Text
body-form = Form URL-encoded
body-beautify-hint = Indent the body; it must be valid first
body-none = This request has no body.
body-hint = Request body
body-field = Field
body-full-path-warning = A file outside the project keeps its full path: the request only works on this computer.
body-multipart-hint = Files are read when sending. A file inside the project is saved relative to the project folder, so teammates can send the request too. The Content-Type with the boundary is set automatically.
body-part = Part
body-value-or-file = Value or file
body-content-type = Content type
body-add-part = Add part
body-file-path-hint = path/to/file
body-pick-file = Pick the file to upload
body-auto = auto
body-file = File
body-kind-hint = A text value, or a file to upload

## host

history-not-kept-here = History is not kept here.
mock-needs-desktop = The mock server needs the desktop app.
default-workspace = My workspace
secrets-in-file = No OS credential store is available ({$reason}). Secret values are kept in {$path}, readable only by your user account.
secrets-file-unreadable = That file could not be read ({$error}), so secret values are only kept until the app closes.
secrets-no-folder = No OS credential store is available ({$reason}) and no app-data folder was found. Secret values are only kept until the app closes.
secrets-page-only = The web version keeps secret values only until the page is closed.
save-response-body = Save response body

## workspaces

workspaces = Workspaces
workspace-saved-in = Saved in {$file}
workspace-new = New workspace…
workspace-environments = Environments…
workspace-environments-hint = Environments every project in this workspace has, such as Staging
workspace-variables = Variables…
workspace-variables-hint = Variables every project in this workspace can use
workspace-defaults = Defaults…
workspace-defaults-hint = Headers, auth, timeout, redirects and cookies every project in this workspace inherits
workspace-file-synced = Kept in sync with {$file}. Choose another file to move it.
workspace-save-as-file-hint = Write the workspace to a file you can commit, so teammates can open it
workspace-save-as-file = Save as file…
workspace-open-file = Open workspace file…
workspace-open-file-hint = Open a workspace a teammate saved as a file
workspace-remove = Remove workspace…
workspace-remove-last = There is always at least one workspace
workspace-switch = Switch workspace
workspace-new-title = New workspace
workspace-rename-title = Rename workspace
workspace-save-as-title = Save workspace as
workspace-saved = Saved the workspace in {$file}.
workspace-open-file-title = Open a workspace file
workspace-remove-title = Remove workspace?
workspace-remove-message = “{$name}” is removed from this machine. Its projects and files stay where they are.
workspace-add-environments = Add environments…
workspace-mock-environment-hint = Sends the requests of projects with a running mock server to it
workspace-environment-picker-hint = Switches every project in the workspace to this environment
workspace-edit-environments-hint = Edit the environments and each project's values
mock-label-port = Mock (port {$port})
mock-label-servers = Mock ({$count} servers)

## shortcuts

shortcut-group-anywhere = Anywhere
shortcut-group-tabs = Tabs
shortcut-group-request = Request
shortcut-group-scripts = Scripts
shortcut-send = Send the request, or run the open workflow
shortcut-send-everywhere = Send the request to every environment
shortcut-save = Save the request or workflow
shortcut-quick-open = Go to a request
shortcut-commands = Every command
shortcut-focus-url = Go to the URL
shortcut-environment = Switch the environment
shortcut-find = Search in the response
shortcut-sidebar = Hide or show the sidebar
shortcut-zoom = Larger or smaller text
shortcut-zoom-reset = Text at its usual size
shortcut-this-list = This list
shortcut-previous-tab = Back to the tab shown before
shortcut-next-tab = The next or the previous tab
shortcut-tab-at = The tab at that place
shortcut-last-tab = The last tab
shortcut-close-tab = Close the tab
shortcut-close-preview = Close the preview under the URL
shortcut-indent = Indent or outdent the lines

## sidebar

add-new-project-hint = Create a project in a folder of your choice
add-existing-hint = Find the projects in a folder or git repository
add-openapi-hint = Create a project from an OpenAPI 3 description (JSON or YAML)
add-postman-hint = Create a project from a Postman collection (v2.0 or v2.1 JSON)
add-insomnia-hint = Create a project from an Insomnia export (v4 JSON or v5 YAML)
add-http-hint = Create a project from a .http file (VS Code REST Client, JetBrains HTTP Client)
add-bruno-hint = Create a project from a Bruno collection (the folder with bruno.json or opencollection.yml)
menu-new-folder = New folder
menu-project-defaults = Defaults (headers, auth, requests)…
menu-description = Description…
menu-mock-server = Mock server (port {$port})…
menu-cookies = Cookies ({$count})…
menu-reload = Reload from disk
menu-file-manager = Open in file manager
menu-terminal = Open in terminal
menu-folder-defaults = Folder defaults…
menu-folder-defaults-hint = Headers, auth and variables every request in this folder inherits
menu-folder-description-hint = What this folder holds, in Markdown
menu-duplicate = Duplicate
menu-move-to = Move to…
menu-copy-to-project = Copy to project…
menu-copy-to-project-hint = Copy into a folder of another project; this one stays as it is
menu-actions = Actions

## defaults

defaults-file-folder = folder file (_folder.toml)
defaults-file-project = project file (tortuga.toml)
defaults-file-workspace = workspace file
defaults-file-workspace-local = workspace (on this machine)
defaults-variables-hint = Defaults for {"{{"}variables{"}}"} in this folder's requests. A value from the active environment, or a secret value, wins over these.
defaults-title-workspace = Workspace defaults — {$name}
defaults-title-project = Project defaults — {$name}
defaults-title-folder = Folder defaults — {$name}
defaults-description = Description
defaults-explain-folder = Every request in this folder and its subfolders inherits these, before the project's defaults. A request (or a subfolder) can replace a header by using the same name, and can choose its own auth.
defaults-explain-project = Every request in this project inherits these, over the workspace's. A request can replace a header by adding one with the same name (or switch it off), and can choose its own auth.
defaults-explain-workspace = Every project in this workspace inherits these, below its own: a project, folder or request header with the same name replaces one of these, and their own auth wins. The scratch request doesn't use them.
defaults-follow-redirects = Follow redirects
defaults-cookies = Send and store cookies
defaults-requests-hint-workspace = Empty or “Inherit” uses the app's settings. Projects can set their own, and a request can still set its own timeout and turn cookies off.
defaults-requests-hint-project = Empty or “Inherit” uses the workspace's value, else the app's settings. A request can still set its own timeout and turn cookies off.
defaults-inherit-on = Inherit: on (from {$level})
defaults-inherit-off = Inherit: off (from {$level})
defaults-inherit = Inherit
defaults-auth-folder-inherits = Requests in this folder use the auth of a parent folder, or of the project.
defaults-auth-project-inherits = Requests in this project use the workspace's auth, if it has one.
defaults-auth-workspace-none = Requests send no authentication unless their project sets some.
defaults-auth-folder-none = Requests in this folder send no authentication, whatever a parent folder or the project sets. A request can still choose its own.
defaults-auth-project-none = Requests in this project send no authentication, even when the workspace sets some. A folder or request can still choose its own.
