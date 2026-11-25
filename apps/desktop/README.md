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
    icons/             app, tray and notification icons (from assets/)
  setup/               the installer's window (Svelte), sharing the app's look
    Setup.svelte       every screen and the way from one to the next
    mock.ts            a fake Rust side, to see each screen in a browser
  assets/              logo sources (SVG)
  scripts/bundle.mjs   builds the Windows installer
```

The installer's Rust side is in [`apps/setup`](../setup).

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

`npm run bundle` (`scripts/bundle.mjs`) builds the three programs in release
mode, then `uninstall.exe` (the installer program with nothing inside),
gathers the files to install in `target/setup-payload`, and builds the
installer with them inside. It lands in
`target/release/bundle/setup/spacetify_<version>_x64-setup.exe`. Set
`CARGO_TARGET_DIR` to build somewhere else (useful while a service built from
`target/release` is running, since Windows locks its file).

The installer is spacetify's own (`apps/setup`, Rust, with its window in
`setup/`), so it has the app's look and explains every problem in plain
words: spacetify already open (it offers to close it), a file blocked by an
antivirus, a full disk, a newer version already installed, a step that
failed after the files were copied (it can be run again). It also repairs,
updates and uninstalls. To see every screen in a browser:

```sh
npm run dev:setup    # then open http://localhost:1421/?s=update
```

with `?s=` one of `fresh`, `older`, `same`, `newer`, `uninstall`,
`running`, `diskfull`, `blocked`, `inuse`, `other`, `partial`,
`restorefail` (see `setup/mock.ts`).

The files are copied all or nothing: everything is unpacked next to the old
files first, then swapped in; if anything fails, the old files are put back.
Only the files listed in `apps/setup/src/files.rs` are ever replaced or
removed.

What it does, for the current user only and without admin rights:

- installs `spacetify-app.exe` (the window), `spacetify.exe` (the CLI),
  `spacetify-service.exe` and `uninstall.exe` in `%LOCALAPPDATA%\spacetify`,
  with shortcuts in the Start menu and on the desktop, an entry in
  *Settings > Apps*, and the example theme and extension;
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

These steps are in `crates/core/src/setup.rs`, so they are tested; the
installer runs them one by one and writes what it did to a log it can open.
`spacetify installer post-install` and `pre-uninstall` (hidden commands) run
the same steps.

Note: the install folder, `%LOCALAPPDATA%\spacetify`, is also where
spacetify keeps its backups. The uninstaller only deletes the files it
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
a signing step in `scripts/bundle.mjs` before the files are packed, to add
once the account exists.

## Automatic updates of the app

Not set up. It is possible with Tauri's updater plugin and GitHub Releases,
and needs: a signing key pair for updates (the private key as a CI secret, the
public key in `tauri.conf.json`), a release workflow that publishes the
installer with its `.sig` file and a `latest.json`, and a check in the app.
The app would then download its updates from GitHub, which has to be
weighed against the "never downloads anything" rule followed so far.
