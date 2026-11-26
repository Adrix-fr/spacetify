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

## Phase 6: anyone can submit a mod (in progress)

People send their own themes and extensions to the gallery. Each request is
checked by a robot, then accepted or refused by the spacetify team. Only
accepted mods appear in the gallery. Everything happens on GitHub, so there
is no server of our own to run or secure.

- **Submitting.** In the app, the *Submit* page (the gallery's *Submit a
  mod* button leads there) runs the robot's checks on the author's computer,
  puts the mod in a `.zip` in their Downloads folder and opens GitHub's
  request form, already filled in, in their browser. The author drops the
  `.zip` into it and sends it. spacetify itself never uploads anything. A
  GitHub account is needed: it is also how the author hears back.
- **The robot.** A GitHub Action (`gallery-submission.yml`) downloads the
  mod (from GitHub only), opens it safely, scans it with ClamAV and writes a
  security report on the request: hidden or disguised files, invisible
  characters, scrambled code, every web address, access to the Spotify
  account or the keyboard, scripts in pictures, and what changed since the
  accepted version. Anything blocking stops there, with what to fix. If
  nothing is blocking, it opens a pull request that adds the mod, pinned to
  that version and the SHA-256 of each file, to `gallery/index.json`.
- **Deciding.** The maintainers get a notification (GitHub's, on the phone
  too). Merging accepts; closing with a comment refuses, and the comment is
  sent to the author as the reason (`gallery-decision.yml`). An accepted mod
  shows in every gallery within a few minutes.
- **Who decides.** Only the spacetify team can merge: its members are
  collaborators, `CODEOWNERS` asks them for each request, and branch
  protection keeps everyone else out of `main`.
- **Updates.** A new version goes through the same robot and review. Only
  the account that sent the mod first can update it (`gallery/owners.json`).
- **Removal.** A mod can be taken off the list at any time, for example if
  it stops working or breaks the rules.
