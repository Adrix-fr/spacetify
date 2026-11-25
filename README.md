# spacetify

A mod manager for the Spotify desktop client, built to survive Spotify updates.

> **Status: early development (phase 1).** The core, the CLI and a proof of
> injection work on Windows. Themes, extensions, the desktop app and automatic
> repair after updates are not there yet. macOS and Linux are supported in the
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

## Install

There are no releases yet. Build from source with Rust 1.95 or newer
(get it with [rustup](https://rustup.rs)):

```sh
git clone https://github.com/Adrix-fr/spacetify
cd spacetify
cargo install --path crates/cli
```

## Usage

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

Useful options: `--yes` closes Spotify without asking, `--lang en|fr` picks the
language, `--spotify-path DIR` points to a custom install, `-v` shows logs.

The interface is available in English and French. By default it follows the
system language.

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
| Backups | `%LOCALAPPDATA%\spacetify` | `~/Library/Application Support/spacetify` | `~/.local/share/spacetify` |

Set `SPACETIFY_HOME` to keep everything in a single folder instead.

## Project layout

```text
crates/
  core/      library with all the logic, shared by every front end
    src/spotify.rs    find the install, read its version, start/stop it
    src/detect.rs     tell original, patched and Spicetify files apart
    src/backup.rs     hashed backups and atomic writes
    src/patcher/      file-based patch (mod.rs) and live injection (live.rs)
    src/config.rs     config.toml and app folders
    assets/           what gets injected
  cli/       the `spacetify` binary: menu and commands
apps/
  desktop/   future Tauri app (not started)
```

## Development

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Tests build fake `xpui.spa` archives in temporary folders. They never touch a
real Spotify install.

Contributions are welcome, see [CONTRIBUTING.md](CONTRIBUTING.md).

## Roadmap

1. ~~Core, CLI, backup/restore, proof of injection~~
2. Themes and extensions that rely on stable selectors and CSS variables
3. Background service that re-applies mods after a Spotify update
4. Desktop app (Tauri) with an installer

## License

[GPL-3.0-or-later](LICENSE).

spacetify is not affiliated with or endorsed by Spotify AB.
