# Desktop app

The spacetify window, its tray icon and the Windows installer. Built with
[Tauri 2](https://tauri.app) (Rust) and [Svelte 5](https://svelte.dev)
(interface). Like the CLI, it holds no mod logic of its own: every action
calls `spacetify-core`.

```text
apps/desktop/
  src/                 the interface (Svelte + TypeScript)
    App.svelte         header, tabs, pages
    pages/             Home, Themes, Extensions, Compatibility,
                       Automatic repair, Terminal, Settings
    lib/api.ts         calls to the Rust side and the shapes it returns
    lib/state.svelte.ts  language, messages, dialogs, apply/restore steps
    dev/mock.ts        sample data, to work on the look in a browser
  src-tauri/
    src/commands.rs    every command the window can call
    src/model.rs       what the window shows, built from the core's types
    src/terminal.rs    the Terminal tab (runs the bundled `spacetify`)
    src/tray.rs        the notification-area icon and its menu
    windows/hooks.nsh  installer steps (Path, notifications, startup entry)
    icons/             app, tray and notification icons (from assets/)
  assets/              logo sources (SVG)
  scripts/sidecars.mjs builds the CLI and the service for the installer
```

## Working on it

You need Rust, Node.js 20 or newer, and on Linux the WebKitGTK packages
listed in [the Tauri prerequisites](https://tauri.app/start/prerequisites/).

```sh
cd apps/desktop
npm ci
npm run build        # needed once: the Rust side embeds these files
npx tauri dev        # opens the app, reloads on change
```

`npm run dev` alone serves the interface at <http://localhost:1420> in a
normal browser, with sample data from `src/dev/mock.ts` instead of the real
Spotify. That is the easy way to work on the look.

`npm run check` type-checks the interface. The Rust side is part of the
workspace: `cargo clippy --workspace` and `cargo test --workspace` cover it,
once `npm run build` has produced `dist/`.

The app uses the same folders and `config.toml` as the CLI, and starts the
service it finds next to itself. In development that is `target/debug`, so
build the whole workspace first (`cargo build --workspace`).

## The installer

```sh
cd apps/desktop
npm ci
npm run bundle
```

`npm run bundle` builds `spacetify` and `spacetify-service` in release mode,
copies them to `src-tauri/binaries/` under the names Tauri expects, then runs
`tauri build`. The installer lands in
`target/release/bundle/nsis/spacetify_<version>_x64-setup.exe`. Set
`CARGO_TARGET_DIR` to build somewhere else (useful while a service built from
`target/release` is running, since Windows locks its file).

What it does, for the current user only and without admin rights:

- installs `spacetify-app.exe` (the window), `spacetify.exe` (the CLI) and
  `spacetify-service.exe` in `%LOCALAPPDATA%\spacetify`, with a Start menu
  entry, and the example theme and extension;
- adds that folder to the user `Path`, so `spacetify` works in any new
  terminal;
- registers the name and icon of notifications
  (`HKCU\Software\Classes\AppUserModelId\io.github.adrix-fr.spacetify`), so
  the service's notifications show as "spacetify";
- if automatic repair was on from another copy (for example a build in
  `target\release`), moves its startup entry to the installed copy and
  restarts the service from there.

Uninstalling stops the service, removes the startup entry, the `Path` entry
and the notification name, and offers to put Spotify back to its original
state first ("No" by default). Mods, settings and backups are never deleted:
they stay in `%APPDATA%\spacetify` and `%LOCALAPPDATA%\spacetify`.

These steps run `spacetify.exe installer post-install` and
`spacetify.exe installer pre-uninstall` (hidden commands, code in
`crates/core/src/setup.rs`), so they are tested and print what they do in the
installer's details.

Note: Tauri installs per-user apps in `%LOCALAPPDATA%\<name>`, which is also
where spacetify keeps its backups. The uninstaller only deletes the files it
installed and never a folder that is not empty, so backups are safe.

## Signing

The CI builds the installer on every push (artifact `spacetify-windows`, with
`SHA256SUMS.txt`). Signing is prepared for the
[SignPath Foundation](https://signpath.org), free for open-source projects,
and turns on by itself once these repository secrets exist:
`SIGNPATH_API_TOKEN` and `SIGNPATH_ORGANIZATION_ID`. The SignPath project is
expected to be named `spacetify`, with a `release-signing` policy and the
artifact configuration in `.signpath/artifact-configuration.xml` (slug
`windows`). Signing runs only for `v*` tags and manual runs.

That configuration signs the installer and the three programs uploaded next
to it. The copies packed inside the installer are not signed yet: that needs
Tauri's `bundle.windows.signCommand` to call SignPath for each file, to add
once the account exists.

## Automatic updates of the app

Not set up. It is possible with Tauri's updater plugin and GitHub Releases,
and needs: a signing key pair for updates (the private key as a CI secret, the
public key in `tauri.conf.json`), a release workflow that publishes the
installer with its `.sig` file and a `latest.json`, and a check in the app.
The app would then download its updates from GitHub, which has to be
weighed against the "never downloads anything" rule followed so far.
