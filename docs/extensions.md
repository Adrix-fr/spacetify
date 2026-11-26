# Writing an extension

A spacetify extension adds behaviour to Spotify with JavaScript. It is a
folder with a manifest and a script. A complete example lives in
[`examples/extensions/hello-badge`](../examples/extensions/hello-badge).

```text
hello-badge/
  extension.toml   required: name, version, selectors, settings…
  main.js          required: the code
```

## Start one

```sh
spacetify new extension my-extension   # makes the folder my-extension, ready to edit
spacetify dev my-extension             # runs it in Spotify, reloads it at each save
```

`spacetify new` writes a manifest, a `main.js` that already does something
(a button that opens a popup), and two files for your editor:
`spacetify.d.ts` describes the whole API, and `jsconfig.json` tells editors
such as VS Code to use it, so they suggest `spacetify.player.…` as you type
and underline mistakes. spacetify never reads these two files, and the
gallery leaves them out.

`spacetify dev` installs the extension, starts Spotify in live mode and
watches the folder: each time you save a file, the extension is checked and
installed again, and Spotify's interface reloads in a second or two, without
restarting Spotify. A save that breaks the manifest is reported in the
terminal, and Spotify keeps the last good version. Close Spotify to stop.
Like `launch --live`, it gives Spotify a debugging port on `127.0.0.1` while
it runs.

Without `dev`:

```sh
spacetify install path/to/my-extension   # copies it and turns it on
spacetify doctor                         # checks its selectors against your Spotify
spacetify apply                          # runs it in Spotify
```

After editing your files, run `spacetify install` again: it replaces the
installed copy. `spacetify disable <id>` turns it off, and `spacetify restore`
removes everything from Spotify.

## extension.toml

```toml
id = "hello-badge"              # unique, lowercase letters, digits and dashes
name = "Hello badge"            # shown to the user
version = "1.0.0"               # MAJOR.MINOR.PATCH
author = "Your name"
description = "Shows a badge."  # optional
spacetify = ">=0.2.0"           # versions of spacetify this extension works with

# Every selector your code relies on. `spacetify doctor` checks them.
selectors = ['[data-testid="now-playing-bar"]']

# Settings, read and written from your code. Optional.
[settings.text]
type = "string"                 # "boolean", "number", "string" or "choice"
default = "spacetify ✓"
label = "Badge text"            # optional

[settings.side]
type = "choice"
choices = ["left", "right"]
default = "right"
```

Unknown fields are refused, to catch typos. The id `none` is reserved.

**Declare your selectors.** spacetify cannot read your code to find them, so
`doctor` only knows the ones listed here. List every selector you pass to
`waitFor`, `observe` or `querySelector`.

**Choose stable selectors.** Prefer `data-testid` attributes and Spotify's
colour variables. Avoid generated class names such as `.gyQ3lHdZrOLMpQhHd2rw`
and translated attributes such as `aria-label`. The [theme guide](themes.md)
explains why, with numbers.

## main.js

Your code runs once Spotify's page has loaded, inside a function that
receives one argument: `spacetify`, an API object of your own.

```js
spacetify.waitFor('[data-testid="now-playing-bar"]').then((bar) => {
  spacetify.log.info("player bar found", bar);
});
```

Plain modern JavaScript, no modules or build step: the file is injected as it
is. Limit: 1 MiB.

### Isolation

Each extension runs on its own. If your code throws, at start or later in a
callback, the error is logged under your extension's name and Spotify, the
theme and the other extensions keep working. An extension that fails 25
times is stopped: its callbacks stop being called and its styles are
removed.

The runtime catches errors thrown at start, in API callbacks, in promises
returned to the API, and uncaught errors coming from your file. Errors in
your own event listeners are caught too, as long as they are not swallowed
by your code.

## The `spacetify` API

### `spacetify.waitFor(selector, options?) → Promise<Element>`

Resolves with the first element matching `selector`, as soon as it exists.
Rejects after `options.timeout` milliseconds (default `30000`, `0` waits
forever). An invalid selector throws right away.

```js
const bar = await spacetify.waitFor('[data-testid="now-playing-bar"]');
```

### `spacetify.observe(selector, callback) → stop()`

Calls `callback(element)` once for every element matching `selector`: those
already on the page, then every new one, for as long as Spotify runs. Spotify
redraws parts of its interface when you change pages; new elements are found
again. Returns a function that stops observing.

```js
spacetify.observe('[data-testid="play-button"]', (button) => {
  button.style.outline = "2px solid var(--essential-bright-accent)";
});
```

Checks are grouped and run at most every 100 ms, so observing is cheap.

### `spacetify.onNavigate(callback) → stop()`

Calls `callback(path)` when Spotify shows another page, for example
`/playlist/37i9dQZF1DXcBWIGoYBM5M`.

### `spacetify.currentPath() → string` and `spacetify.navigate(target)`

`currentPath()` gives the page Spotify shows. In the desktop app the page's
own address always stays `/index.html`, so use this instead of `location`.
`navigate` shows another page, from a path (`"/search"`) or a Spotify URI
(`"spotify:album:1XslIirSxfAhhxRdn4Li9t"`).

### `spacetify.addStyle(css) → remove()`

Adds CSS to the page, after Spotify's own styles. Returns a function that
removes it. Styles are removed automatically if the extension is stopped.

### `spacetify.settings`

The settings declared in `extension.toml`. Values are saved in Spotify, so
they survive restarts and updates. A missing or invalid value gives the
default.

| Method                         | What it does                                          |
| ------------------------------ | ----------------------------------------------------- |
| `get(name)`                    | Current value                                         |
| `set(name, value)`             | Saves a value; throws if the type does not match       |
| `reset(name?)`                 | Back to the default, for one setting or all of them   |
| `all()`                        | Every value, as an object                             |
| `onChange((name, value) => …)` | Called after `set` or `reset`; returns `stop()`       |
| `definitions`                  | The settings as declared in the manifest              |

The desktop app shows every declared setting in its Extensions tab (using
`label`), and saves the chosen values in spacetify's `config.toml`, under
`[settings.<id>]`. At the next start of Spotify after an apply, values that
changed in the app replace the ones saved in Spotify; a setting reset in the
app goes back to its default. Values set with `set()` from your own controls
stay until the app changes them again, so both can live side by side.

### `spacetify.player`

What plays, and how to control it.

```js
await spacetify.player.ready();             // Spotify finds its player a moment after starting
const now = spacetify.player.state();       // { track, isPlaying, position, duration, … }
spacetify.player.onTrackChange((track) => spacetify.log.info("now playing", track && track.name));
await spacetify.player.addToQueue("spotify:track:6rqhFgbbKwnb9MLmUQDhG6");
```

| Member                              | What it does                                                 |
| ----------------------------------- | ------------------------------------------------------------ |
| `available`                         | `false` if this Spotify hides its player (see below)         |
| `ready(timeout?)`                   | Resolves once the player can be used                         |
| `state()`                           | What plays now, or `null` before the player is ready         |
| `onChange(callback)`                | Called on every change: track, pause, position, shuffle…     |
| `onTrackChange(callback)`           | Called with the new track when another one starts            |
| `onVolumeChange(callback)`          | Called with the new volume, from 0 to 1                      |
| `play()`, `pause()`, `togglePlay()` | Starts or stops playing                                      |
| `next()`, `previous()`              | Skips                                                        |
| `seek(milliseconds)`                | Goes to a position                                           |
| `setShuffle(on)`                    | `true` or `false`                                            |
| `setRepeat(mode)`                   | `"off"`, `"context"` (the album or playlist) or `"track"`    |
| `playUri(uri)`                      | Plays a track, an album, a playlist…                         |
| `getVolume()`, `setVolume(volume)`  | From 0 to 1                                                  |
| `queue()`                           | `{ current, queued, next }`: what plays, what was added with *Add to queue*, what comes next |
| `addToQueue(uris)`                  | One URI or a list                                            |
| `removeFromQueue(uris)`             | Takes back tracks added to the queue                         |
| `clearQueue()`                      | Empties what was added to the queue                          |

`state()` gives `track` (`uri`, `name`, `type`, `duration`, `artists`,
`album`, `image`, `isLocal`, `isExplicit`), `context` (the album or playlist
playing), `isPlaying`, `isPaused`, `position` and `duration` in
milliseconds, `shuffle`, `repeat` and `speed`. Every action returns a
promise. Callbacks and promises are isolated like the rest of the API.

**How it stays reliable.** Spotify hands its player to its own interface;
spacetify finds it by what it can do, never by the generated names in
Spotify's code, and changes nothing in Spotify. Your extension only sees
spacetify's API above: if a Spotify update moves the player, spacetify is
fixed in one place and your extension keeps working unchanged.

**Plan B.** Until then, `available` is `false` and the player works through
what is on screen: its buttons and bars, found by their `data-testid`.
`state()` (track, artists, album, cover, position, duration, playing,
shuffle, repeat), the change, track and volume events, `play`, `pause`,
`togglePlay`, `next`, `previous`, `seek`, `setShuffle`, `setRepeat`,
`getVolume` and `setVolume` keep working. Only `playUri` and the queue
reject, with a clear error. `ready()` resolves either way.

### `spacetify.ui`

Ready-made pieces of interface, drawn in Spotify's style and in the
theme's colours. Each one is removed if the extension is stopped.

```js
const page = spacetify.ui.page({
  render(container) {
    container.innerHTML = "<h1>My page</h1>";
  },
});
spacetify.ui.topbarButton({ label: "My page", icon: '<svg viewBox="0 0 16 16">…</svg>', onClick: () => page.open() });
spacetify.ui.contextMenuItem({
  label: "Log the link",
  when: (target) => target.type === "track",
  onClick: (target) => spacetify.log.info(target.uri),
});
```

| Function                   | What it adds                                                    |
| -------------------------- | --------------------------------------------------------------- |
| `topbarButton(options)`    | A button in the top bar, after the back and forward arrows      |
| `playbarButton(options)`   | A button in the player bar, before the lyrics button            |
| `contextMenuItem(options)` | An entry at the end of Spotify's right-click menu               |
| `popup(options)`           | A window over Spotify, closed with ×, Escape or a click outside |
| `page(options)`            | A page of its own, in Spotify's main view                       |

- **Buttons** take `label` (shown when pointed at), `onClick` and an
  optional `icon`: one `<svg>` element, drawn with `currentColor` (without
  it, the label's first letter shows). They return `{ element, setLabel,
  setActive, remove }`; `setActive(true)` shows the button in the accent
  colour.
- **Right-click entries** take `label`, `onClick(target)`, and optionally
  `icon` and `when(target)`, which hides the entry when it returns false.
  `target` is what was right-clicked: `{ uri, uris, type, contextUri }`,
  where `uris` holds every selected item and `type` is `"track"`,
  `"album"`, `"playlist"`, `"artist"`…
- **Popups** take `title`, `content` (text or an element of your own),
  `width` in pixels and `onClose`. They return `{ element, close }`.
- **Pages** take `render(container)`, called each time the page opens, and
  an optional `name` to have several. `render` may return a function,
  called when the page closes. The page lives at
  `/spacetify/<extension id>` (or `/spacetify/<extension id>/<name>`):
  Spotify's back and forward arrows work with it, and the sidebar and the
  player bar stay. It returns `{ path, isOpen, open, remove }`.

### `spacetify.keyboard.add(shortcut, callback, options?) → stop()`

A keyboard shortcut, such as `"ctrl+shift+k"`, `"alt+arrowleft"` or
`"f2"`. Modifiers are `ctrl`, `shift`, `alt` and `meta` (`cmd`). Letters
and digits also match by their place on the keyboard, so shortcuts work
with any layout. It does not fire while typing in a text field, unless
`options.inInputs` is `true`. When it fires, Spotify does not see the keys.

### `spacetify.log.info / warn / error(...values)`

Writes to Spotify's console, prefixed with `[spacetify:<id>]`. The last 300
lines, all extensions together, are kept and can be read with
`__spacetify.logs()`.

### Information

- `spacetify.version`: version of spacetify.
- `spacetify.mode`: `"file"` after `apply`, `"live"` with `launch --live`.
- `spacetify.extension`: `{ id, name, version }` from your manifest.

### Debugging

In Spotify's console (see the [theme guide](themes.md) for how to open it in
live mode):

```js
__spacetify.extensions()   // [{ id, name, version, status, errors }]
__spacetify.logs()         // recent log lines of spacetify and every extension
```

`status` is `loading`, `running`, `failed` (it threw while starting, or its
file has a syntax error) or `stopped` (too many errors).

## Showing your mod in the gallery

An optional `showcase/` folder gives your extension pictures, a longer
description and tags in the gallery. It works exactly as for themes: see
[Showing your mod in the gallery](themes.md#showing-your-mod-in-the-gallery).

## Coming from Spicetify

Spicetify extensions use `Spicetify.Player`, `Spicetify.Platform` and React,
which Spicetify creates by rewriting Spotify's code. spacetify never rewrites
Spotify's code, so these extensions cannot run as they are, but most of what
they do has an equivalent:

| Spicetify                                         | spacetify                                     |
| ------------------------------------------------- | --------------------------------------------- |
| `Spicetify.Player.play`, `pause`, `next`, `seek`… | `spacetify.player.play`, `pause`, `next`, `seek`… |
| `Spicetify.Player.data`, the `songchange` event   | `player.state()`, `player.onTrackChange`      |
| `Spicetify.addToQueue`, `Platform.PlayerAPI`      | `player.addToQueue`, `player.queue()`         |
| `Spicetify.Platform.History.push`, `listen`       | `spacetify.navigate`, `spacetify.onNavigate`  |
| `Spicetify.Topbar.Button`, `Playbar.Button`       | `ui.topbarButton`, `ui.playbarButton`         |
| `Spicetify.ContextMenu.Item`                      | `ui.contextMenuItem`                          |
| `Spicetify.PopupModal.display`                    | `ui.popup`                                    |
| `Spicetify.Mousetrap.bind`                        | `keyboard.add`                                |
| a custom app                                      | `ui.page`                                     |

Replace generated class names with `data-testid` selectors, and React
components with plain elements.
