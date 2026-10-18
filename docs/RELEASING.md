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

1. Make sure `main` is green in CI and `docs/FEATURES.md` is up to date.
2. Set the version in the root `Cargo.toml` (`[workspace.package] version`), commit, and push.
3. Tag the commit and push the tag:

   ```sh
   git tag v0.1.0
   git push origin v0.1.0
   ```

4. The **Release** workflow builds every installer and a `tortuga-cli` archive per OS (R-CLI-4), checks that the tag
   matches the version, and creates a **draft** GitHub release with them and `SHA256SUMS.txt` attached.
5. Download the installers and run the checks below on each OS.
6. Edit the release notes and publish the draft.

To try the installers without releasing, run the workflow by hand: **Actions → Release → Run workflow**. The
installers are then attached to the run as artifacts.

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
