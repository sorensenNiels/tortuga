# Releasing Tortuga

How installers are built (R-NF-4) and how a release is made. The tool is
[cargo-packager](https://github.com/crabnebula-dev/cargo-packager) (decision D16); its settings are in
`[package.metadata.packager]` of `crates/tortuga-app/Cargo.toml`.

| OS | Package | Built on | Notes |
|---|---|---|---|
| Windows | `.msi` | `windows-latest` | WiX 3, downloaded by cargo-packager. |
| macOS | `.dmg` (Apple silicon and Intel) | `macos-latest` | Minimum macOS 11. |
| Linux | `.deb`, `.AppImage` | `ubuntu-22.04` | Built on the oldest supported Ubuntu, so they run on its glibc (2.35) and newer. |

The installers are **not signed** yet (D17). macOS will say the app "cannot be opened because the developer
cannot be verified": right-click the app, choose **Open**, then **Open** again (needed once). Windows SmartScreen
may warn: **More info → Run anyway**.

## Build installers locally

```sh
make setup        # once: installs cargo-packager with the other cargo tools
make package      # installers for this OS in target/packages/
```

`make package` builds the optimised app and packages it: `.deb` and `.AppImage` on Linux, `.msi` on Windows,
`.dmg` on macOS. An installer can only be built on its own OS.

## Make a release

1. Make sure `docs/FEATURES.md` is up to date, and that `CHANGELOG.md` says under "## Unreleased" what changed for
   users (see "The changelog" below). CI runs only on release tags (or by hand from the Actions tab),
   so `make verify` locally is the check before a release; the release script runs it too.
2. Run one of these on an up-to-date, clean `main`:

   ```sh
   make release-patch   # 0.3.0 → 0.3.1: fixes only
   make release-minor   # 0.3.0 → 0.4.0: new features
   make release-major   # 0.3.0 → 1.0.0: breaking changes (e.g. the project file format)
   ```

   It shows what is under "## Unreleased" (and stops if nothing is) and asks first, then sets the version in the root
   `Cargo.toml` (`[workspace.package] version`) and `Cargo.lock`, puts a heading `## X.Y.Z — date` under
   "## Unreleased" in `CHANGELOG.md`, runs `make verify`, commits "Release vX.Y.Z", makes the tag `vX.Y.Z` and pushes the commit and the
   tag together. If verify fails, the version and the changelog are put back and nothing is committed. If only the push fails, the
   commit and tag are local and it prints the command to push them. (It runs in Git Bash on Windows.)

   By hand, the same is: set the version, commit and push, then `git tag vX.Y.Z` and `git push origin vX.Y.Z`.

4. The **Release** workflow builds every installer and a `tortuga-cli` archive per OS (R-CLI-4), checks that the tag
   matches the version, and **publishes** a GitHub release with them and `SHA256SUMS.txt` attached, marked latest. Its notes are the
   version's section of `CHANGELOG.md`, followed by GitHub's list of commits.
   The app's update check (R-SET-15) only sees published releases, so it offers the new version right away.
5. Download the installers and run the checks below on each OS. Edit the release notes on GitHub if needed. If an
   installer is broken, mark the release as a draft again (or delete it) so the update check stops offering it.

To try the installers without releasing, run the workflow by hand: **Actions → Release → Run workflow**. The
installers are then attached to the run as artifacts.

## The changelog

`CHANGELOG.md` tells users what changed. Every commit that changes what they see adds a line under "## Unreleased",
under **Added**, **Changed** or **Fixed** (add the heading if it is missing): one line per entry (GitHub shows line
breaks in release notes as they are), in plain words, about what the user can now do or no longer runs into, with no
requirement IDs. A commit users can't notice (refactoring, tests, docs, build) says `Changelog: none` on a line of its
message instead.

`scripts/check-changelog.sh` checks this: a commit that changes a crate's `src/`, `locales/` or `icons/`, or the VS Code
extension's `src/` or `package.json` (test files aside), without `CHANGELOG.md` and without `Changelog: none` is
refused. Claude Code runs it before each `git commit` (a hook in `.claude/settings.json`); for your own commits, turn
it on once with `git config core.hooksPath .githooks`.

## Check an installer

On each OS, after installing:

- The app starts from the Start menu / Launchpad / application menu, with the turtle icon.
- `tortuga --version` prints the release version (on macOS:
  `/Applications/Tortuga.app/Contents/MacOS/tortuga --version`).
- Start-up stays within budget (R-NF-1): run `tortuga --startup-time`. It opens the app with your workspace,
  prints how long start-up took and how long a frame takes, and quits. The first frame must be shown in under
  1 s; frames should take a few milliseconds. On Windows, pipe it so PowerShell shows the output (the installer
  normally puts the app in `C:\Program Files\Tortuga`):
  `& "$env:ProgramFiles\Tortuga\tortuga.exe" --startup-time | Out-String`.
- Send a request, save it to a project, close and reopen the app: the project is still there.
- Uninstall: the app is removed; your projects and app data are left alone.
- The CLI archive for that OS unpacks to `tortuga-cli-<version>-<os>/` with the binary, `README.md` and the licences;
  `tortuga-cli --version` prints the release version.

Record what was checked, and on which OS, in `docs/FEATURES.md`.

## App icon

The icon is drawn in `crates/tortuga-app/icons/tortuga.svg`. After changing it, run `make icons` (needs
`resvg`: `cargo binstall resvg`) and commit the regenerated PNG and ICO files. The PNGs are used by the
Linux packages, the macOS app and the window; the ICO is embedded in `tortuga.exe` and used by the Windows installer.
