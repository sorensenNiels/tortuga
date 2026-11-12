# Changelog

What changes in each release of Tortuga, for the people who use it. The newest release is first.

## Unreleased

## 0.20.0 — 2026-11-12

### Added

- **Settings → Add-ons.** Features not everyone needs can be turned on and off here. While one is off, it isn't loaded at all: no menu items, commands or background work.
- **Copy the response body** with the 📋 icon in the body's toolbar: the formatted text in Pretty, the body as it came in Raw.
- **See the variables your scripts set.** In Environments, a project's page lists every variable its captures and scripts set, with where it is set and its value in this session. Click a request to open it, or give a variable a default so requests have a value before the script runs.

### Changed

- **Workflows and the MCP server are now add-ons, off by default.** Turn them on in Settings → Add-ons. The mock server is an add-on too, still on by default.

## 0.19.0 — 2026-11-11

### Added

- **Secret guard.** A value that looks like a credential, such as a GitHub token in a header or a variable, gets a quiet warning, with **Make it a secret** to keep its value on your computer instead of in the shared files.
- **`tortuga-cli check`** finds mistakes and credentials in a project before you push, for CI and git hooks.
- **Findings in VS Code.** The VS Code extension shows `tortuga-cli check` findings in the Problems panel.
- **`tortuga-cli diff`** summarizes the API changes between two git revisions as Markdown, for a pull request.
- **Fold JSON and XML** in the response body, and scroll it with the keyboard.

## Earlier releases

See the [GitHub releases](https://github.com/sorensenNiels/tortuga/releases).
