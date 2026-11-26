# Roadmap

Phases 1 to 4 are done and on `main`: the core and the CLI, themes and
extensions, the background service that repairs mods after a Spotify update,
and the desktop app with its own Windows installer.

This page details what comes next.

## Phase 5: a gallery inside Spotify (in progress)

A page of its own inside Spotify's interface, where you browse themes and
extensions, install them, turn them on or off, and apply the changes, all
without leaving Spotify or opening the spacetify app.

- **Where it shows.** The spacetify logo, next to Spotify's back and forward
  arrows, opens the gallery over the whole window; only Spotify's top bar
  stays, so the window can still be moved. *Back to Spotify*, the logo,
  Spotify's back arrow or Escape close it. It is drawn by the injected
  runtime, on a stable hook; if Spotify removes it, the logo floats in a
  corner, so the gallery keeps working.
- **What it shows.** Themes and extensions with a preview, their author,
  version and description, and what is already installed or on.
- **Where the mods come from.** An official list kept in this repository,
  [`gallery/index.json`](../gallery/index.json). Nothing is downloaded until
  you open the gallery, and a mod is downloaded only when you click
  *Install*. Every file is checked against the SHA-256 written in the list,
  then validated like any other mod before it is installed.
- **GitHub's limits.** Files are read from `raw.githubusercontent.com`, not
  GitHub's API (60 requests an hour without an account). Opening the gallery
  is one request for the whole list, kept 10 minutes in memory and on disk.
  If GitHub refuses, the jsDelivr mirror serves the same files; the hashes
  make it as safe.
- **What runs it.** The page inside Spotify cannot write files, so it asks
  spacetify's background service, which does the downloading, checking and
  installing with the same code as the CLI and the app.
- **Applying.** A theme change shows at once. Extensions need Spotify to
  restart: a *Restart Spotify* button closes it, applies the mods and opens
  it again in a few seconds.
- **Rules.** Downloading mods changed the "never downloads code" rule. The
  README (*Releases and antivirus software*) and `CONTRIBUTING.md` now say
  exactly what is downloaded, from where, when, and how it is checked.
  spacetify still never downloads programs.

## Phase 6: anyone can submit a mod (later)

Not started: this only describes the plan.

Like Spicetify's Marketplace, people will be able to send their own themes
and extensions to the gallery. Each submission is reviewed by the owner of
this repository (Adrix-fr), who accepts or refuses it. Only accepted mods
appear in the gallery.

- **Submitting.** An author sends a request on GitHub that points to their
  mod and its version. spacetify's checks (manifest, validation, `doctor`
  rules) run on it automatically, so the reviewer sees right away whether
  it is valid.
- **Reviewing.** Accepting a request adds the mod, pinned to that exact
  version and its SHA-256, to the official list. Refusing it leaves the list
  unchanged, with a reason for the author.
- **Updates.** A new version of an accepted mod goes through the same
  review before the gallery offers it.
- **Removal.** A mod can be taken off the list at any time, for example if
  it stops working or breaks the rules.
