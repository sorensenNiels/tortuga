# Changelog

What changes in each release of Tortuga, for the people who use it. The newest release is first.

## Unreleased

## 0.22.0 — 2026-11-13

### Added

- **WFS features on a map.** In the Features tab, features are shown on an OpenStreetMap background, or on a tile server you add by name and URL in the Background list, zoomed to their extent with the bounding box drawn. Features in longitude/latitude, Web Mercator and UTM are placed. Scroll to zoom, drag to pan, and "Fit" goes back.

## 0.21.0 — 2026-11-12

### Added

- **GeoServer add-on**, off by default. "New WFS service…" in a project's or folder's menu lists a WFS service's layers and creates requests for the ones you pick, with the auth you choose for the service. GeoJSON responses get a Features tab with a shape preview and a table.
- **Map server errors show as errors.** An OGC exception report sent with status 200 is shown above the response and fails the request in workflow and `tortuga-cli` runs.

### Changed

- **Variables set by scripts and captures no longer show where they are set.** The Environments list now shows only each name, its value this session and "Add a default".

### Fixed

- **A new folder's name field is always in view.** In a long project the sidebar now scrolls to it; a folded parent folder opens and the filter is cleared so you can name it.

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
