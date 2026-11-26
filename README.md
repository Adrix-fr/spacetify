# spacetify

A mod manager for the Spotify desktop client, built to survive Spotify updates.

> **Status: early development (phase 5).** The core, the CLI, themes,
> extensions, automatic repair after updates and the desktop app with its
> Windows installer work on Windows. macOS and Linux are supported in the
> code but untested.

## Why

Existing tools patch Spotify's minified JavaScript with search-and-replace
rules. Every Spotify update renames things, the rules stop matching, and mods
break until someone fixes them by hand.

spacetify takes a different approach:

- **Minimal patch.** Only two tags are added to `index.html` inside
  `xpui.spa`. Spotify's own code is never rewritten.
- **Verified backups.** The original file is backed up and checked by SHA-256
  before anything is written. `restore` gives back the exact same bytes.
- **Update-aware.** The patched file records which original it came from, so
  spacetify can tell when an update has replaced it, and never "restores" an
  older interface over a newer one.
- **No file changes at all, if you prefer.** `launch --live` injects at
  runtime through Chromium's debugging protocol instead.
- **Mods built on stable hooks.** Themes and extensions rely on Spotify's
  colour variables and `data-testid` attributes, not on generated class
  names. `spacetify doctor` checks them against the installed Spotify and
  warns before something breaks.
- **One broken mod never breaks Spotify.** Each extension runs on its own,
  and mods are validated before anything is injected.
- **Repairs itself after updates.** An optional background service notices
  when a Spotify update removes the mods and puts them back.

## Install

### Windows installer

There are no releases yet. The CI builds the installer on every push: open
the latest run of the *CI* workflow on GitHub and download the
`spacetify-windows` artifact (it holds `SHA256SUMS.txt` too). Or build it
yourself, see [apps/desktop/README.md](apps/desktop/README.md).

It installs for your user only, without admin rights: the app, the
`spacetify` command (added to your `Path`, so it works in any new terminal)
and the background service. The app sits in the notification area; closing
its window does not stop the automatic repair. Uninstall it from *Settings >
Apps*: it offers to put Spotify back first, and keeps your mods and backups.

### From source

Build with Rust 1.95 or newer (get it with [rustup](https://rustup.rs)):

```sh
git clone https://github.com/Adrix-fr/spacetify
cd spacetify
cargo install --path crates/cli
cargo install --path crates/service   # for automatic repair
```

The two programs must sit in the same folder (`cargo install` puts both in
`~/.cargo/bin`). The desktop app is built with Node.js too, see
[apps/desktop/README.md](apps/desktop/README.md).

## Usage

The desktop app has a tab for each part: Home (state of Spotify, Apply and
Restore), Themes, Extensions (with their settings), Compatibility, Automatic
repair, a Terminal that runs the commands below, and Settings. Everything is
also available from a terminal.

Run `spacetify` on its own to open the interactive menu (arrow keys, Enter,
`q` to quit). Every action is also available as a command:

| Command                   | What it does                                       |
| ------------------------- | -------------------------------------------------- |
| `spacetify status`        | Where Spotify is, its version, and who modified it |
| `spacetify backup`        | Back up the original interface, change nothing     |
| `spacetify apply`         | Back up, then inject (safe to run again)           |
| `spacetify restore`       | Put back the original file, verified by hash       |
| `spacetify launch`        | Start Spotify                                      |
| `spacetify launch --live` | Start Spotify and inject without touching any file |
| `spacetify list`          | Installed themes and extensions                    |
| `spacetify install DIR`   | Install a theme or an extension, and turn it on    |
| `spacetify uninstall ID`  | Delete an installed theme or extension             |
| `spacetify enable ID`     | Turn an extension on, or make a theme active       |
| `spacetify disable ID`    | Turn an extension or the active theme off          |
| `spacetify theme ID`      | Choose the active theme (`none` for Spotify's own) |
| `spacetify doctor`        | Check the active mods against the installed Spotify |
| `spacetify service …`     | Automatic repair after updates, see below          |

Useful options: `--yes` closes Spotify without asking, `--lang en|fr` picks the
language, `--spotify-path DIR` points to a custom install, `-v` shows logs.

The interface is available in English and French. By default it follows the
system language.

### Themes and extensions

```sh
spacetify install examples/themes/midnight
spacetify install examples/extensions/hello-badge
spacetify doctor
spacetify apply
```

One theme is active at a time; any number of extensions can be on. Changes
show in Spotify after the next `apply` (or `launch --live`). Once mods have
been applied, the background service also does it for you: it applies any
change the next time Spotify is closed (see below). `restore` takes
everything out again.

### The gallery, inside Spotify

Once mods are applied, Spotify shows the spacetify logo next to its back and
forward arrows. It opens a page over the whole window where you browse the
themes and extensions of the official list, install them and turn them on or
off, without leaving Spotify. *Back to Spotify*, the logo again, Spotify's
back arrow or Escape close it.
A theme shows at once. Extensions need Spotify to restart: the *Restart
Spotify* button closes it, applies them and opens it again in a few seconds
(or it is done the next time you close Spotify).

The page needs the background service (see below), which does the work:

- The list is one file in this repository, [`gallery/index.json`](gallery/index.json).
  It is downloaded when you open the gallery, kept for 10 minutes, and kept
  on disk to show something when GitHub cannot be reached.
- A mod is downloaded only when you click *Install*. Every file must have the
  SHA-256 written in the list, then the mod is checked like any other mod
  before it is installed. If anything differs, nothing is installed.
- Files are read from `raw.githubusercontent.com`, not from GitHub's API,
  which allows few requests without an account. If GitHub refuses or cannot
  be reached, the same files come from the jsDelivr mirror
  (`cdn.jsdelivr.net`); the hashes make a mirror as safe as GitHub.

Set `SPACETIFY_GALLERY` to a folder (a copy of this repository) or to a base
URL to try another list, for example before publishing a change to it.
Accepting mods sent by other people is planned, see
[docs/roadmap.md](docs/roadmap.md).

Making your own: see [docs/themes.md](docs/themes.md) and
[docs/extensions.md](docs/extensions.md). Spicetify themes and extensions do
not work as they are, because they rely on changes Spicetify makes to
Spotify's code; the guides explain what can be carried over.

### Automatic repair after Spotify updates

A Spotify update replaces its interface file, which removes the mods. The
background service puts them back by itself:

| Command                                   | What it does                                          |
| ----------------------------------------- | ----------------------------------------------------- |
| `spacetify service enable`                | Start the service now and at every login              |
| `spacetify service disable`               | Stop it and remove it from startup (uninstall)        |
| `spacetify service start` / `stop`        | Same, for now only                                    |
| `spacetify service status`                | Whether it runs and what it is doing                  |
| `spacetify service events`                | What it did lately (`-n 50` for more)                 |
| `spacetify service simulate-update`       | Do what an update does, to test the repair            |

The same actions are in the menu, under *Automatic repair*.

How it works:

- It sleeps until the system reports a change in Spotify's `Apps` folder,
  with a safety check every 10 minutes. Idle, it uses about 2-3 MB of memory
  and no processor time.
- It never writes while Spotify or its updater runs, nor while the file is
  still changing. Spotify usually reopens by itself after an update, so a
  notification offers a *Restart Spotify* button: it closes Spotify, puts the
  mods back and opens it again in a few seconds. Without a click, nothing is
  cut: the mods come back the next time you close Spotify, even briefly.
- It first backs up the new interface and checks it by hash, so `restore`
  always gives back the version you have, never an older one.
- It runs `doctor` on the new version and turns off mods that no longer fit
  (`spacetify enable ID` turns one back on), and says so.
- It only uses the file method. It never starts Spotify with `--live`'s
  debugging port and never needs admin rights. It downloads nothing but what
  you install from the gallery (see above).
- It keeps Spotify in step with your mods. When a theme, an extension or a
  setting changed (in the app, the CLI or the gallery), or spacetify itself
  was updated, it applies them again the next time Spotify is closed, or
  right away with the gallery's *Restart Spotify*. It compares what Spotify
  carries with what it should carry, at startup and whenever the mods or
  `config.toml` change, and does nothing when they match.
- After `spacetify restore`, it leaves Spotify alone until the next `apply`.
- Notifications show as "spacetify" once the desktop app's installer has
  registered that name. Without it (built from source), they show under
  "Windows PowerShell", since a program needs a registered name to show
  its own.

Only one service runs at a time. It talks to the CLI and the desktop app
through files in its folder. The gallery page inside Spotify cannot write
files, so the service also listens on `127.0.0.1` (never on the network),
on a port chosen once. It only answers requests that come from Spotify's
interface and carry a secret that spacetify puts into Spotify with the mods.
Both are kept in `service/gallery.json`, which only your user can read.

To test it: apply your mods, run `spacetify service enable`, then
`spacetify service simulate-update`. It puts Spotify's original interface
back exactly as an update does (Spotify is restarted if it was open). Press
*Restart Spotify* in the notification, or close Spotify, and the mods are
back a few seconds later. `spacetify service events` shows each step.

### Safety rules

- Nothing is modified while Spotify is running. You are offered to close it.
- If Spicetify has modified Spotify, spacetify changes nothing and explains
  how to switch. Inactive Spicetify leftovers are reported and left alone.
- The Microsoft Store and Snap versions are read-only and are detected as such.

### Live mode

`launch --live` starts Spotify with a remote debugging port bound to
`127.0.0.1` and keeps the terminal attached. Nothing is written to disk, but
while Spotify runs, any program on your computer could use that port to
control it. Use it if you understand that trade-off.

## Where files go

| What    | Windows                    | macOS                                     | Linux                      |
| ------- | -------------------------- | ----------------------------------------- | -------------------------- |
| Config  | `%APPDATA%\spacetify`      | `~/Library/Application Support/spacetify` | `~/.config/spacetify`      |
| Mods    | `%APPDATA%\spacetify`      | `~/Library/Application Support/spacetify` | `~/.config/spacetify`      |
| Backups | `%LOCALAPPDATA%\spacetify` | `~/Library/Application Support/spacetify` | `~/.local/share/spacetify` |

Mods go in the `themes` and `extensions` folders, and `config.toml` remembers
which ones are on. The service keeps its status, events and log in
`service` next to the backups, and the last gallery list in `gallery`.

Set `SPACETIFY_HOME` to keep everything in a single folder instead.

What `spacetify service enable` adds to the system, and nothing else:

| System  | Entry                                                                       |
| ------- | --------------------------------------------------------------------------- |
| Windows | value `spacetify` in `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` |
| macOS   | `~/Library/LaunchAgents/io.github.adrix-fr.spacetify.plist`                 |
| Linux   | `~/.config/autostart/spacetify-service.desktop`                             |

On Windows it shows as *spacetify* in Task Manager > Startup apps, where it
can also be turned off. `spacetify service disable` removes the entry (and
the Task Manager's on/off flag for it).

The Windows installer also adds, for the current user only: its folder in
the user `Path` (`HKCU\Environment`), the name of its notifications
(`HKCU\Software\Classes\AppUserModelId\io.github.adrix-fr.spacetify`), and
the usual Start menu and desktop shortcuts and uninstall information
(`HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\spacetify`).
Uninstalling removes them all.

To remove spacetify completely: uninstall the app (or run `spacetify restore`,
then `spacetify service disable`), then delete the folders above.

## Project layout

```text
crates/
  core/      library with all the logic, shared by every front end
    src/spotify.rs    find the install, read its version, start/stop it
    src/detect.rs     tell original, patched and Spicetify files apart
    src/backup.rs     hashed backups and atomic writes
    src/patcher/      file-based patch (mod.rs), live injection (live.rs)
                      and the injected payload (payload.rs)
    src/mods/         themes and extensions: manifests, checks, install
    src/doctor.rs     compatibility checks against the installed Spotify
    src/service/      automatic repair: main loop, autostart, events, notifications
    src/gallery/      the gallery: official list, checked downloads, local server
    src/setup.rs      what the Windows installer sets up (Path, notification name)
    src/config.rs     config.toml and app folders
    assets/runtime.js the code injected into Spotify that runs extensions
    assets/gallery.js the gallery page drawn inside Spotify
  cli/       the `spacetify` binary: menu and commands
  service/   the `spacetify-service` binary, which runs in the background
build/       Windows version information shared by both programs
apps/
  desktop/   the desktop app (Tauri + Svelte), and the installer's window
  setup/     the Windows installer and uninstaller (Rust side)
docs/        guides for theme and extension authors, and the roadmap
examples/    an example theme and an example extension
gallery/     the gallery's official list (index.json)
```

## Development

The desktop app's and the installer's Rust code embed their web files, so
build them once first (Node.js 20 or newer):

```sh
cd apps/desktop && npm ci && npm run build && npm run build:setup && cd ../..
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Tests build fake `xpui.spa` archives and fake mods in temporary folders.
They never touch a real Spotify install.

Contributions are welcome, see [CONTRIBUTING.md](CONTRIBUTING.md).

## Roadmap

1. ~~Core, CLI, backup/restore, proof of injection~~
2. ~~Themes and extensions that rely on stable selectors and CSS variables~~
3. ~~Background service that re-applies mods after a Spotify update~~
4. ~~Desktop app (Tauri) with its own Windows installer~~
5. A gallery inside Spotify: browse, install and turn on themes and
   extensions without leaving Spotify (in progress)
6. Anyone can submit a theme or an extension to the gallery; each request
   is accepted or refused by the repository's owner (later)

Phases 5 and 6 are detailed in [docs/roadmap.md](docs/roadmap.md).

## Releases and antivirus software

There are no releases yet. New, unsigned programs that start with Windows and
change another app's files are sometimes flagged by mistake by antivirus
software. To keep that rare, spacetify:

- has clear names (`spacetify.exe`, `spacetify-service.exe`) and Windows
  version information (product, publisher, description, version);
- is never packed or compressed (no UPX);
- starts with Windows only through the standard per-user `Run` entry, visible
  in the Task Manager, and never copies itself anywhere;
- never asks for admin rights, never downloads a program, and never touches
  any program other than Spotify;
- downloads only the themes and extensions you install from the gallery:
  text files (CSS, JavaScript, TOML, images) that run inside Spotify like a
  web page, from this repository's official list only, when you click
  *Install*, each one checked against the SHA-256 written in the list;
- installs for the current user only, with its own small installer
  (`apps/setup`) that writes nothing but what is listed under *Where files
  go*. Like most uninstallers, `uninstall.exe` runs a copy of itself from the
  temporary folder, so it can remove its own file.

Planned for releases: programs built by GitHub Actions and signed through the
[SignPath Foundation](https://signpath.org) (free signing for open-source
projects), with the SHA-256 of every file published next to it. The CI is
ready for it and already publishes `SHA256SUMS.txt` with the installer.

If Microsoft Defender flags spacetify, you can report a false positive at
<https://www.microsoft.com/wdsi/filesubmission>.

## License

[GPL-3.0-or-later](LICENSE).

spacetify is not affiliated with or endorsed by Spotify AB. Modifying the
Spotify client may go against its terms of use: use it at your own risk.
