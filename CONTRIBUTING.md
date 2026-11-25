# Contributing

Thanks for your interest in spacetify. Bug reports, ideas and pull requests
are all welcome.

## Getting started

You need Rust 1.95 or newer, and Node.js 20 or newer for the desktop app.

```sh
git clone https://github.com/Adrix-fr/spacetify
cd spacetify
cd apps/desktop && npm ci && npm run build && cd ../..
cargo run -p spacetify-cli
```

The `npm` line builds the desktop app's web files, which its Rust code
embeds: `cargo` commands on the whole workspace need them. Working on the app
itself is described in [apps/desktop/README.md](apps/desktop/README.md).

## Before opening a pull request

Make sure these pass. CI runs the same checks on Windows, Linux and macOS.

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd apps/desktop && npm run check
```

## Guidelines

- **Logic belongs in `crates/core`.** The CLI, the service and the desktop
  app only display things and ask questions.
- **Nothing that looks like malware.** No hidden or disguised processes, no
  copies of the program in system folders, no admin rights, no code
  downloaded from the internet, no executable packers (UPX or others), and
  no registry or system change beyond the documented startup entry and what
  the installer documents (user `Path`, notification name). The installer is
  Tauri's standard NSIS one, per user; its extra steps live in
  `crates/core/src/setup.rs`, not in installer scripts. The
  service only uses the file method, never `--live`.
- **Tests never touch a real Spotify install.** Use the fixtures in
  `crates/core/src/testing.rs`, which build fake `xpui.spa` archives in a
  temporary folder. The service's logic is tested with a fake clock and a
  fake process list (`crates/core/src/service/daemon/tests.rs`). The only
  test that writes to the registry uses a throwaway key under
  `HKCU\Software\spacetify-tests` and deletes it.
- **Trying the service by hand without touching your Spotify.** Set
  `SPACETIFY_HOME` to an empty folder, copy an `xpui.spa` into a fake
  `Spotify/Apps` folder, set `spotify_path` in that folder's `config.toml`,
  then use `spacetify apply`, `spacetify service start` and
  `spacetify service simulate-update`. Stop the service
  (`spacetify service stop`) before rebuilding: Windows locks a running
  program's file.
- **Keep the patch minimal.** Do not rewrite Spotify's own code; it is what
  breaks other tools after every update.
- **Mods are untrusted input.** Everything a theme or extension provides is
  validated in `crates/core/src/mods` before it is injected, with a message
  that says what to fix (`ModIssue`, in English and French).
- **The injected runtime must never break Spotify.** Every call into an
  extension in `crates/core/assets/runtime.js` goes through the error guard.
  Check its syntax with `node --check crates/core/assets/runtime.js`.
- **Examples stay valid.** `crates/core/tests/examples.rs` loads the mods in
  `examples/`. If you change the mod format, update the examples and the
  guides in `docs/`.
- **User-facing text comes in English and French.** Use the `tr!` macro for
  messages and add both languages to `Error::localized` for new errors. In
  the desktop app's interface, use `tr(en, fr)` from `lib/state.svelte.ts`.
- **The version** is the workspace's (`Cargo.toml`); keep
  `apps/desktop/package.json` in step when bumping it.
- **Commit messages** are short, in English, in the imperative mood
  (`Add …`, `Fix …`), with details in the body when useful.

## Reporting a bug

Please include your operating system and the output of `spacetify status`.
Run the failing command with `-vv` to get detailed logs.
