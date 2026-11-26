# Writing an extension

A spacetify extension adds behaviour to Spotify with JavaScript. It is a
folder with a manifest and a script. A complete example lives in
[`examples/extensions/hello-badge`](../examples/extensions/hello-badge).

```text
hello-badge/
  extension.toml   required: name, version, selectors, settings…
  main.js          required: the code
```

## Try it

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

## What the API does not do

There is no API to read or control playback (current track, play, pause…).
The only way to get that is to reach into Spotify's internal code, which is
exactly what breaks with every update. If a reliable way appears, it will be
added; until then, extensions work with what is on screen.

## Showing your mod in the gallery

An optional `showcase/` folder gives your extension pictures, a longer
description and tags in the gallery. It works exactly as for themes: see
[Showing your mod in the gallery](themes.md#showing-your-mod-in-the-gallery).

## Coming from Spicetify

Spicetify extensions use `Spicetify.Player`, `Spicetify.Platform` and React,
which Spicetify creates by rewriting Spotify's code. spacetify never rewrites
Spotify's code, so these extensions cannot run as they are. Small ones that
only change the page can be ported by replacing their selectors with
`data-testid` ones and using `waitFor` and `observe`.
